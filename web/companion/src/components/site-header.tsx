import Link from "next/link";
import { Download } from "lucide-react";
import { NAV, SITE } from "@/lib/site";

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-20 border-b border-line bg-void/90 backdrop-blur">
      <div className="mx-auto flex max-w-6xl items-center justify-between gap-3 px-5 py-3">
        <Link href="/" className="flex shrink-0 items-baseline gap-2" aria-label={`${SITE.title} home`}>
          <span className="text-lg font-bold tracking-hud text-white">NEON VEIL</span>
          <span className="hidden font-mono text-[11px] uppercase tracking-hud text-cyan-neon md:inline">
            Zero Protocol
          </span>
        </Link>
        <nav aria-label="Main" className="flex items-center gap-1 sm:gap-4">
          {NAV.slice(1).map((item) => (
            <Link
              key={item.href}
              href={item.href}
              className="px-1.5 py-1 text-sm font-semibold uppercase tracking-hud text-mist transition-colors hover:text-white sm:px-2"
            >
              <span className="sm:hidden">{item.short}</span>
              <span className="hidden sm:inline">{item.label}</span>
            </Link>
          ))}
          <a href={SITE.downloadUrl} className="button-primary !px-3 !py-2" rel="noopener">
            <Download aria-hidden className="h-4 w-4" />
            <span className="hidden sm:inline">Download</span>
          </a>
        </nav>
      </div>
    </header>
  );
}
