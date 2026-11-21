import { Github, Instagram } from "lucide-react";
import { SITE } from "@/lib/site";

export function SiteFooter() {
  return (
    <footer className="mt-24 border-t border-line">
      <div className="mx-auto flex max-w-6xl flex-col gap-4 px-5 py-8 text-sm text-mist sm:flex-row sm:items-center sm:justify-between">
        <p>
          <span className="text-slate-200">{SITE.title}</span> &middot; &copy; {new Date().getFullYear()} {SITE.studio}
        </p>
        <p className="flex items-center gap-4">
          <span>Created by {SITE.creator}</span>
          <a href={SITE.instagram} className="inline-flex items-center gap-1 hover:text-white" rel="noopener noreferrer" target="_blank">
            <Instagram aria-hidden className="h-4 w-4" /> Instagram
          </a>
          <a href={SITE.github} className="inline-flex items-center gap-1 hover:text-white" rel="noopener noreferrer" target="_blank">
            <Github aria-hidden className="h-4 w-4" /> GitHub
          </a>
        </p>
      </div>
    </footer>
  );
}
