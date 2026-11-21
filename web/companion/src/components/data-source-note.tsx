import type { DataSource } from "@/lib/leaderboard";

/** Says plainly whether the numbers are live or preview data. */
export function DataSourceNote({ source, what }: { source: DataSource; what: string }) {
  if (source === "live") {
    return (
      <p className="hud-label flex items-center gap-2">
        <span className="h-1.5 w-1.5 bg-emerald-neon" aria-hidden /> Live {what} feed
      </p>
    );
  }
  return (
    <p className="border-l-2 border-gold-neon/70 bg-gold-neon/5 px-3 py-2 text-sm text-slate-300">
      <span className="font-semibold text-gold-neon">Preview data.</span> These {what} are generated with the
      game&apos;s own formulas until the live server feed is connected.
    </p>
  );
}
