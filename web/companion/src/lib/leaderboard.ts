import { seededRandom } from "./random";
import { SKILLS, XP_CAP, XP_TABLE, levelForXp, totalLevel, totalXp } from "./xp";

export type DataSource = "live" | "preview";

export interface LeaderboardEntry {
  rank: number;
  username: string;
  totalLevel: number;
  totalXp: number;
  levels: number[];
}

export interface LeaderboardResponse {
  source: DataSource;
  generatedAt: string;
  entries: LeaderboardEntry[];
}

interface RawOperative {
  username: string;
  xp: number[];
}

const FIRST = ["glass", "null", "chrome", "static", "neon", "ghost", "rust", "volt", "sable", "hex", "drift", "onyx"];
const SECOND = ["ronin", "vector", "sector", "lotus", "cipher", "relay", "viper", "monk", "runner", "lattice", "kite", "fang"];

/** Deterministic sample operatives, distributed like a young server. */
function previewOperatives(count: number): RawOperative[] {
  const random = seededRandom(0x4e56);
  return Array.from({ length: count }, (_, i) => {
    const first = FIRST[i % FIRST.length] ?? "op";
    const second = SECOND[Math.floor(i / FIRST.length) % SECOND.length] ?? "unit";
    const dedication = Math.pow(random(), 1.6);
    const xp = SKILLS.map(() => {
      const level = Math.max(1, Math.min(99, Math.round(20 + dedication * 79 * (0.55 + random() * 0.45))));
      const base = XP_TABLE[level] ?? 0;
      const next = XP_TABLE[Math.min(level + 1, 99)] ?? base;
      return Math.min(XP_CAP, Math.round(base + (next - base) * random()));
    });
    return { username: `${first}_${second}${i % 7 === 0 ? "" : (i % 90) + 10}`, xp };
  });
}

function isRawOperative(value: unknown): value is RawOperative {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Record<string, unknown>;
  return (
    typeof candidate.username === "string" &&
    Array.isArray(candidate.xp) &&
    candidate.xp.length === SKILLS.length &&
    candidate.xp.every((x) => typeof x === "number" && Number.isFinite(x) && x >= 0)
  );
}

async function liveOperatives(url: string): Promise<RawOperative[] | null> {
  try {
    const response = await fetch(url, { next: { revalidate: 60 } });
    if (!response.ok) return null;
    const body: unknown = await response.json();
    return Array.isArray(body) ? body.filter(isRawOperative) : null;
  } catch {
    return null;
  }
}

export function rankOperatives(operatives: RawOperative[], limit: number): LeaderboardEntry[] {
  return operatives
    .map((op) => ({
      username: op.username,
      totalLevel: totalLevel(op.xp),
      totalXp: totalXp(op.xp),
      levels: op.xp.map(levelForXp),
    }))
    .sort((a, b) => b.totalLevel - a.totalLevel || b.totalXp - a.totalXp || a.username.localeCompare(b.username))
    .slice(0, limit)
    .map((entry, i) => ({ rank: i + 1, ...entry }));
}

export async function getLeaderboard(limit = 100): Promise<LeaderboardResponse> {
  const url = process.env.NEON_VEIL_LEADERBOARD_URL;
  const live = url ? await liveOperatives(url) : null;
  return {
    source: live ? "live" : "preview",
    generatedAt: new Date().toISOString(),
    entries: rankOperatives(live ?? previewOperatives(160), limit),
  };
}
