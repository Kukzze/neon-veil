import { NextResponse, type NextRequest } from "next/server";
import { getLeaderboard } from "@/lib/leaderboard";

export const dynamic = "force-dynamic";

/** GET /api/leaderboards?limit=100&q=ronin */
export async function GET(request: NextRequest) {
  const params = request.nextUrl.searchParams;
  const requested = Number.parseInt(params.get("limit") ?? "100", 10);
  const limit = Number.isFinite(requested) ? Math.min(Math.max(requested, 1), 100) : 100;
  const query = (params.get("q") ?? "").trim().toLowerCase().slice(0, 64);

  const board = await getLeaderboard(100);
  const entries = (query ? board.entries.filter((e) => e.username.toLowerCase().includes(query)) : board.entries).slice(0, limit);

  return NextResponse.json(
    { source: board.source, generatedAt: board.generatedAt, count: entries.length, entries },
    { headers: { "Cache-Control": "public, s-maxage=60, stale-while-revalidate=300" } },
  );
}
