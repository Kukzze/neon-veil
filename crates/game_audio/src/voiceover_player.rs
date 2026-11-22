//! Sequential dialogue: one line at a time, each played to its full
//! length, with its subtitle raised as it starts. Also loads the tutorial
//! dialogue manifest shared with the voice generation tool.

use crate::audio_bus::SubtitleDisplayEvent;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Speaking rate used to estimate line length from its text.
pub const ESTIMATED_WORDS_PER_SECOND: f32 = 2.6;
/// Breath added after each estimated line, s.
pub const LINE_PADDING_SECONDS: f32 = 0.6;

/// The tutorial script, embedded so cue lookup never depends on the
/// working directory.
pub const TUTORIAL_DIALOGUE_MANIFEST: &str =
    include_str!("../../../assets/audio/voiceovers/dialogue_manifest.json");
pub const MISSION_ONE_DIALOGUE_MANIFEST: &str =
    include_str!("../../../assets/audio/voiceovers/mission_one_manifest.json");
pub const ONBOARDING_DIALOGUE_MANIFEST: &str =
    include_str!("../../../assets/audio/voiceovers/onboarding_manifest.json");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceCueData {
    pub cue_id: String,
    pub speaker: String,
    pub text: String,
    pub audio_path: String,
    pub duration_seconds: f32,
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct VoiceoverQueue {
    pub current_cue: Option<VoiceCueData>,
    pub time_remaining: f32,
    pub pending_cues: VecDeque<VoiceCueData>,
}

impl VoiceoverQueue {
    pub fn enqueue(&mut self, cue: VoiceCueData) {
        self.pending_cues.push_back(cue);
    }

    /// Advances playback by `dt`. When the current line has finished (or
    /// nothing is playing) the next queued line starts with its full
    /// duration and its subtitle is returned. At most one line starts per
    /// call, so no line is ever skipped.
    pub fn tick(&mut self, dt: f32) -> Option<SubtitleDisplayEvent> {
        if self.current_cue.is_some() {
            self.time_remaining -= dt.max(0.0);
            if self.time_remaining > 0.0 {
                return None;
            }
            self.current_cue = None;
            self.time_remaining = 0.0;
        }
        let next = self.pending_cues.pop_front()?;
        let duration = if next.duration_seconds.is_finite() {
            next.duration_seconds.max(0.0)
        } else {
            0.0
        };
        let subtitle = SubtitleDisplayEvent {
            speaker: next.speaker.clone(),
            text: next.text.clone(),
            duration_seconds: duration,
        };
        self.time_remaining = duration;
        self.current_cue = Some(next);
        Some(subtitle)
    }

    pub fn is_playing(&self) -> bool {
        self.current_cue.is_some() || !self.pending_cues.is_empty()
    }

    pub fn current_cue_id(&self) -> Option<&str> {
        self.current_cue.as_ref().map(|c| c.cue_id.as_str())
    }

    pub fn clear(&mut self) {
        self.current_cue = None;
        self.time_remaining = 0.0;
        self.pending_cues.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueEntry {
    pub cue_id: String,
    pub speaker: String,
    pub voice_id: String,
    pub text: String,
    pub output_path: String,
    /// Measured length of the recorded line, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueManifest {
    pub project: String,
    pub version: String,
    pub default_model_id: String,
    pub dialogues: Vec<DialogueEntry>,
}

/// Line length estimated from word count, for lines without measured audio.
pub fn estimated_duration_seconds(text: &str) -> f32 {
    text.split_whitespace().count() as f32 / ESTIMATED_WORDS_PER_SECOND + LINE_PADDING_SECONDS
}

impl DialogueEntry {
    pub fn to_voice_cue(&self) -> VoiceCueData {
        VoiceCueData {
            cue_id: self.cue_id.clone(),
            speaker: self.speaker.clone(),
            text: self.text.clone(),
            audio_path: self.output_path.clone(),
            duration_seconds: self.duration_ms.map_or_else(
                || estimated_duration_seconds(&self.text),
                |ms| ms as f32 / 1000.0 + LINE_PADDING_SECONDS,
            ),
        }
    }
}

impl DialogueManifest {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn tutorial() -> Self {
        Self::parse(TUTORIAL_DIALOGUE_MANIFEST).expect("embedded dialogue manifest is valid")
    }

    /// The first-run tutorial.
    pub fn onboarding() -> Self {
        Self::parse(ONBOARDING_DIALOGUE_MANIFEST).expect("embedded onboarding manifest is valid")
    }

    /// Voiced dialogue for the first mission, Night One.
    pub fn mission_one() -> Self {
        Self::parse(MISSION_ONE_DIALOGUE_MANIFEST).expect("embedded mission manifest is valid")
    }

    pub fn find(&self, cue_id: &str) -> Option<&DialogueEntry> {
        self.dialogues.iter().find(|d| d.cue_id == cue_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn cue(id: &str, duration: f32) -> VoiceCueData {
        VoiceCueData {
            cue_id: id.into(),
            speaker: "AURA".into(),
            text: format!("line {id}"),
            audio_path: format!("assets/audio/voiceovers/{id}.ogg"),
            duration_seconds: duration,
        }
    }

    #[test]
    fn three_cues_play_in_order_without_overlap() {
        let mut queue = VoiceoverQueue::default();
        queue.enqueue(cue("a", 1.0));
        queue.enqueue(cue("b", 0.5));
        queue.enqueue(cue("c", 0.75));
        assert!(queue.is_playing());

        let dt = 1.0 / 64.0;
        let mut started: Vec<(String, f32)> = Vec::new();
        let mut t = 0.0;
        for _ in 0..200 {
            if let Some(sub) = queue.tick(dt) {
                started.push((sub.text, t));
            }
            t += dt;
        }
        let order: Vec<&str> = started.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(order, vec!["line a", "line b", "line c"]);
        // Each line starts only after the previous one ran its full length.
        assert!(started[1].1 - started[0].1 >= 1.0 - 1e-4);
        assert!(started[2].1 - started[1].1 >= 0.5 - 1e-4);
        assert!(!queue.is_playing(), "finished after {t} s");
    }

    #[test]
    fn a_line_is_never_cut_by_a_new_cue() {
        let mut queue = VoiceoverQueue::default();
        queue.enqueue(cue("a", 2.0));
        assert!(queue.tick(0.0).is_some());
        assert!(queue.tick(1.0).is_none());
        queue.enqueue(cue("b", 1.0));
        assert!(queue.tick(0.5).is_none(), "b waits for a");
        assert_eq!(queue.current_cue_id(), Some("a"));
        let sub = queue.tick(0.6).expect("a finished, b starts");
        assert_eq!(sub.text, "line b");
        assert!(
            (queue.time_remaining - 1.0).abs() < 1e-6,
            "b gets its full length"
        );
    }

    #[test]
    fn clear_and_degenerate_durations() {
        let mut queue = VoiceoverQueue::default();
        queue.enqueue(cue("a", f32::NAN));
        queue.enqueue(cue("b", -3.0));
        assert!(queue.tick(0.1).is_some());
        assert_eq!(queue.time_remaining, 0.0);
        assert_eq!(
            queue.tick(0.1).unwrap().text,
            "line b",
            "zero-length lines still advance"
        );
        queue.enqueue(cue("c", 5.0));
        queue.clear();
        assert!(!queue.is_playing());
        assert!(queue.tick(1.0).is_none());
    }

    #[test]
    fn tutorial_manifest_has_all_thirteen_cues() {
        let manifest = DialogueManifest::tutorial();
        assert_eq!(manifest.project, "NEON_VEIL_ZERO_PROTOCOL");
        assert_eq!(manifest.default_model_id, "eleven_multilingual_v2");
        assert_eq!(manifest.dialogues.len(), 13);
        let ids: Vec<&str> = manifest
            .dialogues
            .iter()
            .map(|d| d.cue_id.as_str())
            .collect();
        assert_eq!(ids.first(), Some(&"TUT_00_INTRO_MONOLOGUE"));
        assert_eq!(ids.last(), Some(&"TUT_12_EXTRACTION"));
        for (i, id) in ids.iter().enumerate() {
            assert!(id.starts_with(&format!("TUT_{i:02}_")), "{id} out of order");
        }
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 13);
        for d in &manifest.dialogues {
            let voice = match d.speaker.as_str() {
                "Viktor Vance" => "pNInz6obpgDQGcFmaJgB",
                "AURA" => "21m00Tcm4TlvDq8ikWAM",
                other => panic!("unexpected speaker {other}"),
            };
            assert_eq!(d.voice_id, voice, "{}", d.cue_id);
            assert!(
                d.output_path.starts_with("assets/audio/voiceovers/")
                    && d.output_path.ends_with(".ogg")
            );
            assert!(!d.text.trim().is_empty());
        }
        let paths: HashSet<&str> = manifest
            .dialogues
            .iter()
            .map(|d| d.output_path.as_str())
            .collect();
        assert_eq!(paths.len(), 13, "every line has its own file");
    }

    #[test]
    fn manifest_entries_become_timed_cues() {
        let manifest = DialogueManifest::tutorial();
        let intro = manifest
            .find("TUT_00_INTRO_MONOLOGUE")
            .unwrap()
            .to_voice_cue();
        assert_eq!(intro.speaker, "Viktor Vance");
        assert_eq!(intro.audio_path, "assets/audio/voiceovers/tut_00_intro.ogg");
        // 31 words at 2.6 words/s plus padding.
        assert!(
            (intro.duration_seconds - (31.0 / 2.6 + 0.6)).abs() < 1e-3,
            "{}",
            intro.duration_seconds
        );
        assert!(manifest.find("TUT_99").is_none());
        assert!(DialogueManifest::parse("{\"project\": 1}").is_err());
    }
}
