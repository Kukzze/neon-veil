import type { Metadata } from "next";
import { DataSourceNote } from "@/components/data-source-note";
import { getMarket } from "@/lib/market";
import { formatCompact, formatNumber } from "@/lib/site";
import { MarketTable } from "./market-table";

export const metadata: Metadata = {
  title: "Grand Exchange",
  description: "Prices, alch values and 24 hour volume for every item in the Neo-Veridia Cyber-Market.",
};

export const revalidate = 60;

export default async function GrandExchangePage() {
  const market = await getMarket();
  const tradable = market.items.filter((i) => i.is_tradable);
  const volume = tradable.reduce((sum, i) => sum + (i.volume24h ?? 0), 0);
  const traded = tradable.reduce((sum, i) => sum + (i.volume24h ?? 0) * i.avgPrice, 0);
  const burned = tradable.reduce((sum, i) => sum + (i.taxSink24h ?? 0), 0);
  const stats = [
    { label: "Items listed", value: `${tradable.length} / ${market.items.length}` },
    { label: "Units traded (24h)", value: formatNumber(volume) },
    { label: "Credits traded (24h)", value: formatCompact(traded) },
    { label: `Burned by ${market.taxRate * 100}% tax (24h)`, value: formatCompact(burned) },
  ];

  return (
    <div className="mx-auto max-w-6xl px-5 pt-14">
      <p className="hud-label text-cyan-neon">Cyber-Market</p>
      <h1 className="mt-3 text-4xl font-bold text-white sm:text-5xl">Grand Exchange</h1>
      <p className="mt-3 max-w-2xl text-slate-300">
        Every buy and sell order is escrowed and matched by price, then time. Fills execute at the seller&apos;s ask, and
        a {market.taxRate * 100}% tax on each fill is removed from circulation.
      </p>
      <dl className="mt-8 grid grid-cols-2 border-y border-line sm:grid-cols-4">
        {stats.map((s) => (
          <div key={s.label} className="py-5 pr-4">
            <dt className="hud-label">{s.label}</dt>
            <dd className="mt-1 font-mono text-2xl text-white">{s.value}</dd>
          </div>
        ))}
      </dl>
      <div className="mt-6">
        <DataSourceNote source={market.source} what="volumes and prices" />
      </div>
      <MarketTable items={market.items} />
    </div>
  );
}
