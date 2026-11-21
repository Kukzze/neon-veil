"use client";

import { useMemo, useState } from "react";
import { Search } from "lucide-react";
import type { LeaderboardEntry } from "@/lib/leaderboard";
import { formatNumber } from "@/lib/site";
import { MAX_LEVEL, MAX_TOTAL_LEVEL } from "@/lib/xp";

/** Skill columns shown, as indices into the skill order. */
const COLUMNS = [
  { label: "Agility", index: 0 },
  { label: "Melee", index: 1 },
  { label: "Ballistics", index: 2 },
  { label: "Netrunning", index: 3 },
  { label: "Engineering", index: 4 },
];

function Level({ value }: { value: number | undefined }) {
  const level = value ?? 1;
  return <span className={level >= MAX_LEVEL ? "text-gold-neon" : "text-slate-200"}>{level}</span>;
}

export function LeaderboardTable({ entries }: { entries: LeaderboardEntry[] }) {
  const [query, setQuery] = useState("");
  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? entries.filter((e) => e.username.toLowerCase().includes(q)) : entries;
  }, [entries, query]);

  return (
    <div className="mt-8">
      <label className="relative block max-w-sm">
        <span className="sr-only">Search operatives</span>
        <Search aria-hidden className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-mist" />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search operative"
          className="text-input pl-9"
        />
      </label>
      <div className="mt-4 overflow-x-auto border border-line">
        <table className="data-table w-full">
          <thead className="bg-panel">
            <tr>
              <th scope="col">Rank</th>
              <th scope="col">Operative</th>
              <th scope="col">Total level</th>
              <th scope="col">Total XP</th>
              {COLUMNS.map((c) => (
                <th key={c.label} scope="col" className="!text-right">
                  {c.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {visible.map((entry) => (
              <tr key={entry.username} className="hover:bg-white/[0.02]">
                <td className={entry.rank <= 3 ? "text-gold-neon" : "text-mist"}>{entry.rank}</td>
                <td className="!font-display !text-base font-semibold text-white">{entry.username}</td>
                <td>
                  <span className="text-cyan-neon">{entry.totalLevel}</span>
                  <span className="text-mist"> / {MAX_TOTAL_LEVEL}</span>
                </td>
                <td className="text-slate-300">{formatNumber(entry.totalXp)}</td>
                {COLUMNS.map((c) => (
                  <td key={c.label} className="text-right">
                    <Level value={entry.levels[c.index]} />
                  </td>
                ))}
              </tr>
            ))}
            {visible.length === 0 && (
              <tr>
                <td colSpan={4 + COLUMNS.length} className="py-10 text-center !font-display text-mist">
                  No operative matches &ldquo;{query}&rdquo;.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <p className="mt-3 hud-label">
        Showing {visible.length} of {entries.length}
      </p>
    </div>
  );
}
