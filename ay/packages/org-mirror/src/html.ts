/** An escaping HTML template, the page layout and the small formatters every view shares. */
import { APP_CSS, FONT_CSS } from "./style.ts";

/** A string already safe to embed: output of `html` or `raw`. */
export class Safe {
  constructor(readonly value: string) {}
  toString() {
    return this.value;
  }
}

export const raw = (s: string) => new Safe(s);

export const escape = (s: unknown): string =>
  String(s ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

type Value = Safe | string | number | boolean | null | undefined | Value[];

function render(v: Value): string {
  if (v instanceof Safe) return v.value;
  if (Array.isArray(v)) return v.map(render).join("");
  if (v === false || v === null || v === undefined) return "";
  return escape(v);
}

export function html(strings: TemplateStringsArray, ...values: Value[]): Safe {
  let out = strings[0]!;
  values.forEach((v, i) => (out += render(v) + strings[i + 1]!));
  return new Safe(out);
}

export interface Page {
  title: string;
  subtitle?: string;
  /** Request path, used to mark the active drawer entry. */
  path: string;
  body: Safe;
}

const NAV = [
  { href: "/", label: "Overview", group: "Mirror" },
  { href: "/repos", label: "Repositories", group: "Mirror" },
  { href: "/packages", label: "Packages", group: "Mirror" },
  { href: "/api", label: "API", group: "Reference" },
];

export function layout(p: Page, org: string): string {
  const groups = [...new Set(NAV.map(n => n.group))];
  const active = NAV.filter(n => (n.href === "/" ? p.path === "/" : p.path === n.href || p.path.startsWith(`${n.href}/`))).sort((a, b) => b.href.length - a.href.length)[0]?.href;
  return html`<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${p.title} - ${org} mirror</title>
<link rel="icon" href="/assets/icon.svg">
<link rel="stylesheet" href="/assets/m3-tokens.css">
<link rel="stylesheet" href="${FONT_CSS}">
<style>${raw(APP_CSS)}</style>
</head>
<body>
<header class="bar">
  <a class="brand" href="/"><img src="/assets/icon.svg" alt=""><span>${org}</span></a>
  <span class="grow"></span>
  <form action="/repos" method="get"><input type="search" name="q" placeholder="Find a repository" aria-label="Find a repository"></form>
</header>
<div class="shell">
<nav class="drawer" aria-label="Sections">
${groups.map(g => html`<h2>${g}</h2>${NAV.filter(n => n.group === g).map(n => html`<a href="${n.href}"${n.href === active ? raw(' aria-current="page"') : ""}>${n.label}</a>`)}`)}
</nav>
<main>
<h1>${p.title}</h1>
${p.subtitle ? html`<p class="lead">${p.subtitle}</p>` : ""}
${p.body}
</main>
</div>
<footer>Replica of the GitHub organization ${org}, read from sqlite.</footer>
</body>
</html>`.toString();
}

const DATE = new Intl.DateTimeFormat("en-US", { month: "short", day: "numeric", year: "numeric", timeZone: "UTC" });

/** `2026-11-16T10:00:00Z` as `Nov 16, 2026` (UTC, so every reader sees the same page). */
export function fmtDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const t = Date.parse(iso);
  return Number.isNaN(t) ? iso : DATE.format(t);
}

export const fmtNum = (n: number | null | undefined): string => (n == null ? "" : n.toLocaleString("en-US"));

export function fmtBytes(n: number | null | undefined): string {
  if (n == null) return "";
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) (v /= 1024), i++;
  return `${i === 0 ? v : v.toFixed(1)} ${units[i]}`;
}

/** A link to an external URL; anything but http(s) is shown as plain text. */
export function ext(url: string | null | undefined, text: string): Safe | string {
  return url && /^https?:\/\//i.test(url) ? html`<a href="${url}" rel="noopener noreferrer">${text}</a>` : text;
}

/** The current URL with `set` applied to its query (null removes a key; page 1 is the default and omitted). */
export function withParams(url: URL, set: Record<string, string | number | null>): string {
  const sp = new URLSearchParams(url.search);
  for (const [k, v] of Object.entries(set)) {
    if (v === null || v === "" || (k === "page" && Number(v) === 1)) sp.delete(k);
    else sp.set(k, String(v));
  }
  const q = sp.toString();
  return q ? `${url.pathname}?${q}` : url.pathname;
}

export function tabs(items: { href: string; label: string; current: boolean }[]): Safe {
  return html`<div class="tabs">${items.map(i => html`<a href="${i.href}"${i.current ? raw(' aria-current="page"') : ""}>${i.label}</a>`)}</div>`;
}

export function pager(url: URL, p: { total: number; page: number; per: number }): Safe {
  const pages = Math.max(1, Math.ceil(p.total / p.per));
  return html`<nav class="pager" aria-label="Pages">
${p.page > 1 ? html`<a href="${withParams(url, { page: p.page - 1 })}" rel="prev">Previous</a>` : html`<span class="muted">Previous</span>`}
<span>Page ${p.page} of ${pages} (${fmtNum(p.total)} total)</span>
${p.page < pages ? html`<a href="${withParams(url, { page: p.page + 1 })}" rel="next">Next</a>` : html`<span class="muted">Next</span>`}
</nav>`;
}
