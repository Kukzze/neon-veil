import { NextResponse, type NextRequest } from "next/server";
import { getMarket } from "@/lib/market";

export const dynamic = "force-dynamic";

/** GET /api/v1/ge/prices?category=Weapon&q=katana&tradable=true */
export async function GET(request: NextRequest) {
  const params = request.nextUrl.searchParams;
  const category = params.get("category");
  const query = (params.get("q") ?? "").trim().toLowerCase().slice(0, 64);
  const tradableOnly = params.get("tradable") === "true";

  const market = await getMarket();
  const items = market.items.filter(
    (item) =>
      (!category || item.category.toLowerCase() === category.toLowerCase()) &&
      (!query || item.name.toLowerCase().includes(query)) &&
      (!tradableOnly || item.is_tradable),
  );

  return NextResponse.json(
    {
      source: market.source,
      generatedAt: market.generatedAt,
      taxRate: market.taxRate,
      count: items.length,
      items: items.map((item) => ({
        id: item.id,
        name: item.name,
        category: item.category,
        rarity: item.rarity,
        tradable: item.is_tradable,
        storePrice: item.store_price,
        highAlchValue: item.high_alch_value,
        averagePrice: item.avgPrice,
        volume24h: item.volume24h,
        taxSink24h: item.taxSink24h,
      })),
    },
    { headers: { "Cache-Control": "public, s-maxage=60, stale-while-revalidate=300" } },
  );
}
