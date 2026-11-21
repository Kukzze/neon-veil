#!/usr/bin/env python3
"""Synthesize the tutorial voiceovers listed in the dialogue manifest.

Each manifest entry is sent to the ElevenLabs text-to-speech API with its
voice id and the manifest's default model, and the returned audio is written
to the entry's output path. Uses only the Python standard library.

Usage:
    python tools/voice_generator/generate_voices.py               # synthesize missing lines
    python tools/voice_generator/generate_voices.py --force       # regenerate everything
    python tools/voice_generator/generate_voices.py --mock        # offline placeholders (CI)
    python tools/voice_generator/generate_voices.py --only TUT_04_VAULT --only TUT_05_WALLRUN
    python tools/voice_generator/generate_voices.py --list

The API key is read from the ELEVENLABS_API_KEY environment variable and is
never written anywhere.

Audio is requested as raw 16-bit PCM and wrapped as WAV. Entries whose
output path ends in `.ogg` are encoded to Ogg Vorbis with ffmpeg when it is
on PATH; without ffmpeg the WAV is written next to it (same name, `.wav`).

Exit codes: 0 success, 1 some lines failed, 2 bad arguments, manifest or key.
"""

from __future__ import annotations

import argparse
import io
import json
import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import wave
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = REPO_ROOT / "assets" / "audio" / "voiceovers" / "dialogue_manifest.json"
API_URL = "https://api.elevenlabs.io/v1/text-to-speech/{voice_id}?output_format=pcm_{rate}"
API_KEY_ENV = "ELEVENLABS_API_KEY"
SUPPORTED_RATES = (16000, 22050, 24000, 44100)

# Must match game_audio::voiceover_player (line length estimate).
WORDS_PER_SECOND = 2.6
LINE_PADDING_SECONDS = 0.6

MAX_ATTEMPTS = 4
REQUEST_TIMEOUT_SECONDS = 60


@dataclass(frozen=True)
class DialogueLine:
    cue_id: str
    speaker: str
    voice_id: str
    text: str
    output_path: str


@dataclass(frozen=True)
class Manifest:
    project: str
    model_id: str
    lines: list[DialogueLine]


class ManifestError(ValueError):
    pass


def load_manifest(path: Path) -> Manifest:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise ManifestError(f"manifest not found: {path}") from exc
    except json.JSONDecodeError as exc:
        raise ManifestError(f"manifest is not valid JSON: {exc}") from exc

    for key in ("project", "default_model_id", "dialogues"):
        if key not in data:
            raise ManifestError(f"manifest is missing '{key}'")

    lines = []
    seen = set()
    for index, entry in enumerate(data["dialogues"]):
        missing = [k for k in ("cue_id", "speaker", "voice_id", "text", "output_path") if not entry.get(k)]
        if missing:
            raise ManifestError(f"dialogue #{index} is missing {', '.join(missing)}")
        if entry["cue_id"] in seen:
            raise ManifestError(f"duplicate cue_id {entry['cue_id']}")
        seen.add(entry["cue_id"])
        lines.append(DialogueLine(**{k: entry[k] for k in DialogueLine.__dataclass_fields__}))
    return Manifest(project=data["project"], model_id=data["default_model_id"], lines=lines)


def estimated_duration_seconds(text: str) -> float:
    return len(text.split()) / WORDS_PER_SECOND + LINE_PADDING_SECONDS


def pcm_to_wav(pcm: bytes, sample_rate: int) -> bytes:
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(sample_rate)
        wav.writeframes(pcm)
    return buffer.getvalue()


def placeholder_pcm(duration: float, sample_rate: int) -> bytes:
    """A quiet 220 Hz tone with short fades, so placeholders are audible
    but unmistakably not real dialogue."""
    frames = max(1, int(duration * sample_rate))
    fade = max(1, int(0.02 * sample_rate))
    amplitude = 0.05 * 32767
    samples = []
    for i in range(frames):
        envelope = min(1.0, i / fade, (frames - 1 - i) / fade)
        value = amplitude * envelope * math.sin(2.0 * math.pi * 220.0 * i / sample_rate)
        samples.append(int(value))
    return struct.pack(f"<{frames}h", *samples)


def synthesize_pcm(line: DialogueLine, model_id: str, api_key: str, sample_rate: int) -> bytes:
    body = json.dumps(
        {
            "text": line.text,
            "model_id": model_id,
            "voice_settings": {"stability": 0.5, "similarity_boost": 0.75},
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        API_URL.format(voice_id=line.voice_id, rate=sample_rate),
        data=body,
        method="POST",
        headers={
            "xi-api-key": api_key,
            "Content-Type": "application/json",
            "Accept": "audio/pcm",
        },
    )
    for attempt in range(1, MAX_ATTEMPTS + 1):
        try:
            with urllib.request.urlopen(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
                pcm = response.read()
            if not pcm or len(pcm) % 2:
                raise RuntimeError(f"unexpected audio payload ({len(pcm)} bytes)")
            return pcm
        except urllib.error.HTTPError as exc:
            retryable = exc.code == 429 or exc.code >= 500
            detail = exc.read().decode("utf-8", "replace")[:200]
            if not retryable or attempt == MAX_ATTEMPTS:
                raise RuntimeError(f"HTTP {exc.code}: {detail}") from exc
        except urllib.error.URLError as exc:
            if attempt == MAX_ATTEMPTS:
                raise RuntimeError(f"network error: {exc.reason}") from exc
        time.sleep(2 ** attempt)
    raise RuntimeError("unreachable")


def resolve_target(line: DialogueLine, output_dir: Path | None) -> Path:
    relative = Path(line.output_path)
    return output_dir / relative.name if output_dir else REPO_ROOT / relative


def existing_output(target: Path) -> Path | None:
    for candidate in (target, target.with_suffix(".wav")):
        if candidate.exists():
            return candidate
    return None


def write_audio(wav_bytes: bytes, target: Path) -> Path:
    """Writes WAV data to `target`, encoding to Ogg Vorbis when the target is
    `.ogg` and ffmpeg is available. Returns the path written."""
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.suffix.lower() != ".ogg":
        target.write_bytes(wav_bytes)
        return target

    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        fallback = target.with_suffix(".wav")
        fallback.write_bytes(wav_bytes)
        return fallback

    with tempfile.TemporaryDirectory() as scratch:
        source = Path(scratch) / "line.wav"
        source.write_bytes(wav_bytes)
        subprocess.run(
            [ffmpeg, "-y", "-loglevel", "error", "-i", str(source), "-c:a", "libvorbis", "-q:a", "5", str(target)],
            check=True,
        )
    return target


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Synthesize tutorial voiceovers with ElevenLabs.")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST, help="dialogue manifest JSON")
    parser.add_argument("--output-dir", type=Path, help="write all files here instead of their manifest paths")
    parser.add_argument(
        "--mock",
        "--dry-run",
        dest="mock",
        action="store_true",
        help="write placeholder audio without calling the API (offline / CI)",
    )
    parser.add_argument("--force", action="store_true", help="regenerate files that already exist")
    parser.add_argument("--only", action="append", metavar="CUE_ID", help="limit to these cues (repeatable)")
    parser.add_argument("--list", action="store_true", help="print the cues and exit")
    parser.add_argument("--model", help="override the manifest's default model id")
    parser.add_argument(
        "--sample-rate", type=int, default=24000, choices=SUPPORTED_RATES, help="PCM sample rate requested"
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        manifest = load_manifest(args.manifest)
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    lines = manifest.lines
    if args.only:
        wanted = set(args.only)
        unknown = wanted - {line.cue_id for line in lines}
        if unknown:
            print(f"error: unknown cue ids: {', '.join(sorted(unknown))}", file=sys.stderr)
            return 2
        lines = [line for line in lines if line.cue_id in wanted]

    if args.list:
        for line in lines:
            print(f"{line.cue_id:<24} {line.speaker:<14} ~{estimated_duration_seconds(line.text):4.1f}s  {line.output_path}")
        return 0

    api_key = os.environ.get(API_KEY_ENV, "").strip()
    if not args.mock and not api_key:
        print(f"error: set {API_KEY_ENV} or pass --mock for placeholder audio", file=sys.stderr)
        return 2

    model_id = args.model or manifest.model_id
    mode = "placeholder" if args.mock else f"ElevenLabs {model_id}"
    print(f"{manifest.project}: {len(lines)} lines, {mode}, {args.sample_rate} Hz")

    failures = 0
    for line in lines:
        target = resolve_target(line, args.output_dir)
        previous = existing_output(target)
        if previous and not args.force:
            print(f"  skip  {line.cue_id} ({previous.relative_to(REPO_ROOT) if previous.is_relative_to(REPO_ROOT) else previous})")
            continue
        try:
            if args.mock:
                pcm = placeholder_pcm(estimated_duration_seconds(line.text), args.sample_rate)
            else:
                pcm = synthesize_pcm(line, model_id, api_key, args.sample_rate)
            written = write_audio(pcm_to_wav(pcm, args.sample_rate), target)
        except (RuntimeError, OSError, subprocess.CalledProcessError) as exc:
            failures += 1
            print(f"  FAIL  {line.cue_id}: {exc}", file=sys.stderr)
            continue
        seconds = len(pcm) / 2 / args.sample_rate
        print(f"  wrote {line.cue_id} -> {written} ({seconds:.1f}s)")

    if failures:
        print(f"{failures} of {len(lines)} lines failed", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
