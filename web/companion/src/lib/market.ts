import catalog from "@/data/item-catalog.json";
import type { DataSource } from "./leaderboard";
import { seededRandom } from "./random";

/** Grand Exchange tax burned on every fill (crates/game_economy/src/grand_exchange.rs). */
export const MARKET_TAX_RATE = 0.02;

export const RARITIES = ["Common", "Uncommon", "Rare", "Epic", "Legendary", "Exotic"] as const;
export type Rarity = (typeof RARITIES)[number];

export interface CatalogItem {
  id: number;
  name: string;
  category: string;
  rarity: Rarity;
  is_tradable: boolean;
  high_alch_value: number;
  store_price: number;
}

export interface MarketItem extends CatalogItem {
  volume24h: number | null;
  avgPrice: number;
  /** Credits removed from circulation by the 2% tax over 24 h. */
  taxSink24h: number | null;
}

export interface MarketResponse {
  source: DataSource;
  taxRate: number;
  generatedAt: string;
  items: MarketItem[];
}

export const ITEM_CATALOG = catalog as CatalogItem[];

/** Exchange tax for a fill: 2% rounded half up, as the server computes it. */
export function marketTax(tradeValue: number): number {
  return Math.floor((tradeValue * 2 + 50) / 100);
}

interface LiveStat {
  item_id: number;
  volume_24h: number;
  avg_price: number;
}

const RARITY_VOLUME: Record<Rarity, number> = {
  Common: 9000,
  Uncommon: 3200,
  Rare: 900,
  Epic: 220,
  Legendary: 40,
  Exotic: 6,
};

function previewStats(): Map<number, LiveStat> {
  const random = seededRandom(0x6765);
  return new Map(
    ITEM_CATALOG.filter((item) => item.is_tradable).map((item) => [
      item.id,
      {
        item_id: item.id,
        volume_24h: Math.round(RARITY_VOLUME[item.rarity] * (0.4 + random() * 1.2)),
        avg_price: Math.round(item.store_price * (0.82 + random() * 0.3)),
      },
    ]),
  );
}

function isLiveStat(value: unknown): value is LiveStat {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return [v.item_id, v.volume_24h, v.avg_price].every((n) => typeof n === "number" && Number.isFinite(n) && n >= 0);
}

async function liveStats(url: string): Promise<Map<number, LiveStat> | null> {
  try {
    const response = await fetch(url, { next: { revalidate: 60 } });
    if (!response.ok) return null;
    const body: unknown = await response.json();
    if (!Array.isArray(body)) return null;
    return new Map(body.filter(isLiveStat).map((s) => [s.item_id, s]));
  } catch {
    return null;
  }
}

export async function getMarket(): Promise<MarketResponse> {
  const url = process.env.NEON_VEIL_MARKET_URL;
  const live = url ? await liveStats(url) : null;
  const stats = live ?? previewStats();
  return {
    source: live ? "live" : "preview",
    taxRate: MARKET_TAX_RATE,
    generatedAt: new Date().toISOString(),
    items: ITEM_CATALOG.map((item) => {
      const stat = item.is_tradable ? stats.get(item.id) : undefined;
      const avgPrice = stat?.avg_price ?? item.store_price;
      return {
        ...item,
        avgPrice,
        volume24h: stat ? stat.volume_24h : null,
        taxSink24h: stat ? marketTax(stat.volume_24h * avgPrice) : null,
      };
    }),
  };
}
