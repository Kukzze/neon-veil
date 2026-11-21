"use client";

import { useMemo, useState } from "react";
import { ArrowDownUp, Search } from "lucide-react";
import { RarityBadge } from "@/components/rarity-badge";
import { RARITIES, type MarketItem } from "@/lib/market";
import { formatNumber } from "@/lib/site";

type SortKey = "name" | "store_price" | "high_alch_value" | "volume24h" | "taxSink24h";

const SORTS: { key: SortKey; label: string }[] = [
  { key: "volume24h", label: "24h volume" },
  { key: "store_price", label: "Store price" },
  { key: "high_alch_value", label: "High alch" },
  { key: "taxSink24h", label: "Tax sink" },
  { key: "name", label: "Name" },
];

function sortValue(item: MarketItem, key: SortKey): number | string {
  if (key === "name") return item.name;
  return item[key] ?? -1;
}

export function MarketTable({ items }: { items: MarketItem[] }) {
  const categories = useMemo(() => Array.from(new Set(items.map((i) => i.category))), [items]);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("All");
  const [sort, setSort] = useState<SortKey>("volume24h");

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return items
      .filter((i) => (category === "All" || i.category === category) && (!q || i.name.toLowerCase().includes(q)))
      .sort((a, b) => {
        const av = sortValue(a, sort);
        const bv = sortValue(b, sort);
        if (typeof av === "string" && typeof bv === "string") return av.localeCompare(bv);
        return (bv as number) - (av as number);
      });
  }, [items, query, category, sort]);

  return (
    <div className="mt-8">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <label className="relative block sm:w-72">
          <span className="sr-only">Search items</span>
          <Search aria-hidden className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-mist" />
          <input type="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search item" className="text-input pl-9" />
        </label>
        <label className="flex items-center gap-2">
          <span className="hud-label">Category</span>
          <select value={category} onChange={(e) => setCategory(e.target.value)} className="text-input !w-auto">
            <option>All</option>
            {categories.map((c) => (
              <option key={c}>{c}</option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2">
          <ArrowDownUp aria-hidden className="h-4 w-4 text-mist" />
          <span className="sr-only">Sort by</span>
          <select value={sort} onChange={(e) => setSort(e.target.value as SortKey)} className="text-input !w-auto">
            {SORTS.map((s) => (
              <option key={s.key} value={s.key}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="mt-4 overflow-x-auto border border-line">
        <table className="data-table w-full">
          <thead className="bg-panel">
            <tr>
              <th scope="col">Item</th>
              <th scope="col">Category</th>
              <th scope="col">Rarity</th>
              <th scope="col" className="!text-right">Store price</th>
              <th scope="col" className="!text-right">High alch</th>
              <th scope="col" className="!text-right">24h volume</th>
              <th scope="col" className="!text-right">Tax sink (24h)</th>
            </tr>
          </thead>
          <tbody>
            {visible.map((item) => (
              <tr key={item.id} className="hover:bg-white/[0.02]">
                <td className="!font-display !text-base font-semibold text-white">
                  {item.name}
                  <span className="ml-2 font-mono text-[11px] text-mist">#{item.id}</span>
                </td>
                <td className="!font-display text-slate-300">{item.category}</td>
                <td>
                  <RarityBadge rarity={item.rarity} />
                </td>
                <td className="text-right text-slate-200">{formatNumber(item.store_price)}</td>
                <td className="text-right text-slate-300">{formatNumber(item.high_alch_value)}</td>
                <td className="text-right text-cyan-neon">
                  {item.is_tradable ? formatNumber(item.volume24h ?? 0) : <span className="text-mist">Untradable</span>}
                </td>
                <td className="text-right text-gold-neon">{item.taxSink24h === null ? "—" : formatNumber(item.taxSink24h)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-3 hud-label">
        Showing {visible.length} of {items.length} items &middot; Rarity order: {RARITIES.join(" → ")}
      </p>
    </div>
  );
}
