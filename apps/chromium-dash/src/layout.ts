import { l10n } from "./html.ts";
import { html, raw, type Safe } from "./html.ts";
import { DIR, LANG } from "./format.ts";
import { APP_CSS, FONT_CSS } from "./style.ts";

export interface NavItem {
  href: string;
  label: string;
  group: "Chromium Dash" | "Chrome Status" | "Reference";
}

export const NAV: NavItem[] = [
  { href: "/", label: l10n("home"), group: "Chromium Dash" },
  { href: "/releases", label: l10n("releases"), group: "Chromium Dash" },
  { href: "/schedule", label: l10n("schedule"), group: "Chromium Dash" },
  { href: "/branches", label: l10n("branches"), group: "Chromium Dash" },
  { href: "/commits", label: l10n("commits"), group: "Chromium Dash" },
  { href: "/serving-builds", label: l10n("serving_builds"), group: "Chromium Dash" },
  { href: "/roadmap", label: l10n("roadmap"), group: "Chrome Status" },
  { href: "/features", label: l10n("all_features"), group: "Chrome Status" },
  { href: "/channels", label: l10n("release_channels"), group: "Reference" },
  { href: "/api", label: l10n("api"), group: "Reference" },
];

export interface Page {
  title: string;
  /** Path used to mark the active drawer entry. */
  path: string;
  subtitle?: string;
  body: Safe;
  /** Set when any upstream document was served from a stale cache entry. */
  stale?: boolean;
  product: string;
}

export function layout(p: Page): string {
  const groups = [...new Set(NAV.map((n) => n.group))];
  const active = NAV.filter((n) =>
    n.href === "/" ? p.path === "/" : p.path === n.href || p.path.startsWith(`${n.href}/`),
  ).sort((a, b) => b.href.length - a.href.length)[0]?.href;
  return html`<!doctype html>
    <html lang="${LANG}" dir="${DIR}">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width,initial-scale=1" />
        <title>${p.title} - ${p.product}</title>
        <link rel="icon" href="/favicon.ico" sizes="32x32" />
        <link rel="icon" href="/assets/icon.svg" type="image/svg+xml" />
        <link rel="apple-touch-icon" href="/apple-touch-icon.png" />
        <link rel="manifest" href="/manifest.webmanifest" />
        <link rel="stylesheet" href="/assets/m3-tokens.css" />
        <link rel="stylesheet" href="${FONT_CSS}" />
        <style>
          ${raw(APP_CSS)}
        </style>
      </head>
      <body>
        <header class="bar">
          <a class="brand" href="/"><img src="/assets/icon.svg" alt="" />${p.product}</a>
          <h1>${p.title}</h1>
          ${p.subtitle ? html`<span class="sub">${p.subtitle}</span>` : ""}
          <span class="grow"></span>
          <form action="/commits" method="get" class="search" style="margin:0">
            <input type="search" name="q" placeholder="${l10n("commit_hash")}" />
          </form>
        </header>
        <div class="shell">
          <nav class="drawer" aria-label="${l10n("sections")}">
            ${groups.map(
              (g) =>
                html`<h2>${g}</h2>
                  ${NAV.filter((n) => n.group === g).map((n) => html`<a href="${n.href}" ${n.href === active ? raw(' aria-current="page"') : ""}>${n.label}</a>`)}`,
            )}
          </nav>
          <main>
            ${p.stale ? html`<p class="stale">An upstream service is unreachable; this page shows the last copy stored in the cache.</p>` : ""}
            ${p.body}
          </main>
        </div>
        <footer>
          ${l10n("data_read_live_from_chromiumdash_appspot_com_chr")}
        </footer>
      </body>
    </html>`.toString();
}
