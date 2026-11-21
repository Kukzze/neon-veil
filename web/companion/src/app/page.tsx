import Link from "next/link";
import { ArrowRight, Download, Github, Instagram } from "lucide-react";
import { SITE } from "@/lib/site";

const SPECS = [
  { value: "64 Hz", label: "Server tick rate" },
  { value: "22", label: "Parkour states" },
  { value: "8 × 99", label: "Skills to master" },
  { value: "119", label: "Tradable items & gear" },
  { value: "40", label: "Polarity mod cards" },
  { value: "30", label: "Story quests" },
];

const FEATURES = [
  {
    title: "64 Hz netcode",
    body: "An authoritative server with client prediction and rollback. Your movement responds instantly at 150 ms of latency, and shots are checked against where targets were when you fired.",
  },
  {
    title: "22-state parkour engine",
    body: "Sprint, slide, wall-run, wall-kick, mantle, vault and zipline through four vertical districts, with a Flow meter that rewards unbroken movement up to Overdrive.",
  },
  {
    title: "1–99 skills",
    body: "Eight skills on the classic experience curve: 13,034,431 XP to reach level 99, 792 total level, and a mastery cape for each skill.",
  },
  {
    title: "Player-driven Cyber-Market",
    body: "Buy and sell 119 items on the Grand Exchange with escrowed orders, price-time matching and a 2% tax that removes credits from circulation.",
  },
  {
    title: "Polarity mod system",
    body: "Slot up to eight mod cards into your cyberware. Matching polarity halves the drain, so builds are a capacity puzzle, not a checklist.",
  },
];

const DISTRICTS = [
  { name: "Undercity Slums", elevation: "0 m", note: "Toxic alleys, heavy rain" },
  { name: "Industrial Foundry", elevation: "+50 m", note: "Molten slag, steam geysers" },
  { name: "Kuroshio Commercial Hub", elevation: "+250 m", note: "Neon avenues, bounce awnings" },
  { name: "Spire Megatowers", elevation: "+800 m", note: "Monoliths and skybridges" },
];

/** A static rendition of the in-game HUD, drawn with the same palette. */
function HudPreview() {
  return (
    <div className="border border-line bg-panel p-5 font-mono text-xs" aria-hidden>
      <div className="flex items-center justify-between border-b border-line pb-3">
        <span className="text-cyan-neon">NE 042&deg; // OBJECTIVE: SECTOR 4 EXTRACTION [185m]</span>
      </div>
      <div className="relative my-10 flex h-16 items-center justify-center">
        <span className="absolute h-px w-3 -translate-x-5 bg-white/80" />
        <span className="absolute h-px w-3 translate-x-5 bg-white/80" />
        <span className="absolute h-3 w-px -translate-y-5 bg-white/80" />
        <span className="absolute h-3 w-px translate-y-5 bg-white/80" />
      </div>
      <div className="flex items-end justify-between gap-6">
        <div className="min-w-0 flex-1 space-y-1.5">
          <p className="text-emerald-neon">SHIELDS 150 / 150 &nbsp;|&nbsp; HP 100 / 100</p>
          <div className="h-1.5 bg-white/10"><div className="h-full w-full bg-cyan-neon" /></div>
          <div className="h-1.5 bg-white/10"><div className="h-full w-[88%] bg-emerald-neon" /></div>
          <div className="h-1.5 w-2/3 bg-white/10"><div className="h-full w-[64%] bg-gold-neon" /></div>
          <p className="pt-1 text-gold-neon">FLOW &nbsp;82% // Overdrive 125%</p>
        </div>
        <p className="shrink-0 text-right text-lg text-cyan-neon">30 // 120 [AUTO]</p>
      </div>
    </div>
  );
}

export default function HomePage() {
  return (
    <>
      <section className="mx-auto grid max-w-6xl grid-cols-1 gap-12 px-5 pb-16 pt-16 lg:grid-cols-[1.1fr_1fr] lg:items-center lg:pt-24">
        <div>
          <p className="hud-label text-cyan-neon">{SITE.studio} presents</p>
          <h1 className="mt-4 text-5xl font-bold leading-[0.95] text-white sm:text-7xl">
            Neon Veil:
            <br />
            Zero Protocol
          </h1>
          <p className="mt-6 max-w-xl text-lg leading-relaxed text-slate-300">
            A parkour action RPG in the vertical megacity of Neo-Veridia. Run the walls from the flooded Undercity to the
            Spire, parry blades on the rooftops, and take eight skills from 1 to 99.
          </p>
          <div className="mt-8 flex flex-wrap gap-3">
            <a href={SITE.downloadUrl} className="button-primary" rel="noopener">
              <Download aria-hidden className="h-4 w-4" /> Download Client (Windows x64)
            </a>
            <Link href="/leaderboards" className="button-ghost">
              Live Leaderboards <ArrowRight aria-hidden className="h-4 w-4" />
            </Link>
            <Link href="/grand-exchange" className="button-ghost">
              Grand Exchange Market <ArrowRight aria-hidden className="h-4 w-4" />
            </Link>
          </div>
          <p className="mt-8 flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-mist">
            <span>
              Created by <span className="text-slate-200">{SITE.creator}</span>
            </span>
            <a href={SITE.instagram} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 hover:text-white">
              <Instagram aria-hidden className="h-4 w-4" /> Instagram
            </a>
            <a href={SITE.github} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 hover:text-white">
              <Github aria-hidden className="h-4 w-4" /> GitHub
            </a>
          </p>
        </div>
        <HudPreview />
      </section>

      <section aria-label="At a glance" className="border-y border-line bg-panel/60">
        <dl className="mx-auto grid max-w-6xl grid-cols-2 gap-px px-5 sm:grid-cols-3 lg:grid-cols-6">
          {SPECS.map((spec) => (
            <div key={spec.label} className="py-6">
              <dt className="hud-label">{spec.label}</dt>
              <dd className="mt-1 font-mono text-2xl text-white">{spec.value}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section className="mx-auto max-w-6xl px-5 py-20">
        <h2 className="hud-label">Systems</h2>
        <ol className="mt-6 divide-y divide-line border-y border-line">
          {FEATURES.map((feature, i) => (
            <li key={feature.title} className="grid grid-cols-1 gap-2 py-6 sm:grid-cols-[3rem_16rem_1fr] sm:gap-6">
              <span className="font-mono text-sm text-cyan-neon">{String(i + 1).padStart(2, "0")}</span>
              <h3 className="text-2xl font-bold leading-tight text-white">{feature.title}</h3>
              <p className="max-w-2xl leading-relaxed text-slate-300">{feature.body}</p>
            </li>
          ))}
        </ol>
      </section>

      <section className="mx-auto max-w-6xl px-5">
        <h2 className="hud-label">Neo-Veridia, bottom to top</h2>
        <div className="mt-6 overflow-x-auto">
          <table className="data-table w-full">
            <thead>
              <tr>
                <th>District</th>
                <th>Elevation</th>
                <th>Conditions</th>
              </tr>
            </thead>
            <tbody>
              {[...DISTRICTS].reverse().map((d) => (
                <tr key={d.name}>
                  <td className="!font-display !text-base font-semibold text-white">{d.name}</td>
                  <td className="text-cyan-neon">{d.elevation}</td>
                  <td className="!font-display !text-base text-slate-300">{d.note}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </>
  );
}
