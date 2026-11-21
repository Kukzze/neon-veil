import type { Rarity } from "@/lib/market";

const STYLES: Record<Rarity, string> = {
  Common: "border-rarity-common/40 text-rarity-common",
  Uncommon: "border-rarity-uncommon/40 text-rarity-uncommon",
  Rare: "border-rarity-rare/50 text-rarity-rare",
  Epic: "border-rarity-epic/50 text-rarity-epic",
  Legendary: "border-rarity-legendary/50 text-rarity-legendary",
  Exotic: "border-rarity-exotic/60 text-rarity-exotic",
};

export function RarityBadge({ rarity }: { rarity: Rarity }) {
  return (
    <span className={`inline-block border px-1.5 py-0.5 font-mono text-[11px] uppercase tracking-hud ${STYLES[rarity]}`}>
      {rarity}
    </span>
  );
}
