import type { Metadata, Viewport } from "next";
import { JetBrains_Mono, Rajdhani } from "next/font/google";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { SITE } from "@/lib/site";
import "./globals.css";

const display = Rajdhani({
  subsets: ["latin"],
  weight: ["500", "600", "700"],
  variable: "--font-display",
  display: "swap",
});

const mono = JetBrains_Mono({
  subsets: ["latin"],
  weight: ["400", "500"],
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  title: { default: SITE.title, template: `%s // ${SITE.title}` },
  description:
    "A parkour action RPG in the vertical megacity of Neo-Veridia, by Cutting Onion Studios. Leaderboards, market prices and downloads.",
  applicationName: SITE.title,
  authors: [{ name: "Daksh Rawat (Kukzze)", url: SITE.github }],
  creator: SITE.studio,
  publisher: SITE.studio,
  openGraph: {
    title: SITE.title,
    description: "Run the walls of Neo-Veridia. A parkour action RPG by Cutting Onion Studios.",
    siteName: SITE.title,
    type: "website",
  },
};

export const viewport: Viewport = {
  themeColor: "#030712",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${display.variable} ${mono.variable}`}>
      <body className="min-h-screen font-display">
        <SiteHeader />
        <main>{children}</main>
        <SiteFooter />
      </body>
    </html>
  );
}
