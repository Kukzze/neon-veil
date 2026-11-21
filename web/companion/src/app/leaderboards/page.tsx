import type { Metadata } from "next";
import { DataSourceNote } from "@/components/data-source-note";
import { getLeaderboard } from "@/lib/leaderboard";
import { MAX_TOTAL_LEVEL } from "@/lib/xp";
import { LeaderboardTable } from "./leaderboard-table";

export const metadata: Metadata = {
  title: "Leaderboards",
  description: "The top 100 operatives in Neo-Veridia by total level and experience.",
};

export const revalidate = 60;

export default async function LeaderboardsPage() {
  const board = await getLeaderboard(100);
  return (
    <div className="mx-auto max-w-6xl px-5 pt-14">
      <p className="hud-label text-cyan-neon">Hiscores</p>
      <h1 className="mt-3 text-4xl font-bold text-white sm:text-5xl">Top 100 operatives</h1>
      <p className="mt-3 max-w-2xl text-slate-300">
        Ranked by total level out of {MAX_TOTAL_LEVEL}, then by total experience. Each skill caps at 200,000,000 XP.
      </p>
      <div className="mt-6">
        <DataSourceNote source={board.source} what="rankings" />
      </div>
      <LeaderboardTable entries={board.entries} />
    </div>
  );
}
