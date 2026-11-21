export const SITE = {
  title: "Neon Veil: Zero Protocol",
  studio: "Cutting Onion Studios",
  creator: "@kukzze",
  instagram: "https://www.instagram.com/kukzze/",
  github: "https://github.com/Kukzze",
  downloadUrl: process.env.NEXT_PUBLIC_DOWNLOAD_URL || "https://cutting-onion-studios.itch.io/neon-veil",
} as const;

export const NAV = [
  { href: "/", label: "Overview", short: "Home" },
  { href: "/leaderboards", label: "Leaderboards", short: "Ranks" },
  { href: "/grand-exchange", label: "Grand Exchange", short: "Market" },
] as const;

export function formatNumber(value: number): string {
  return value.toLocaleString("en-US");
}

export function formatCompact(value: number): string {
  return Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(value);
}
