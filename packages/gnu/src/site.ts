// Static site builder: bundles site/app.ts, writes index.html, copies the feeds and the design tokens.
import { join } from "node:path";
import { buildAtom } from "./feed.ts";
import { repoRoot, writeAtomic } from "./store.ts";
import type { Feed, GnuEvent } from "./types.ts";
import catalog from "../locales.toml";

// Messages: locales.toml has one [locales.<tag>.messages] table per language (the shape of aphrody/assets/os/locales.toml that
// fw-theme Catalog reads). FW_LOCALE, then LANG, picks the language; a missing message falls back to the default locale, then to its id.
type Messages = Record<string, string>;
const locales = catalog.locales as unknown as { default_locale: string } & Record<string, { messages: Messages }>;
const requested = (process.env.FW_LOCALE ?? process.env.LANG ?? "").split(".")[0].replace("_", "-");
const fallback = locales[locales.default_locale]?.messages ?? {};
const messages = locales[requested]?.messages ?? locales[requested.split("-")[0]]?.messages ?? fallback;
export const l10n = (id: string): string => messages[id] ?? fallback[id] ?? id;

const CSS = `
:root{--bg:var(--md-sys-color-surface);--fg:var(--md-sys-color-on-surface);--dim:var(--md-sys-color-on-surface-variant);--pri:var(--md-sys-color-primary);--cont:var(--md-sys-color-surface-container);--line:var(--md-sys-color-outline-variant)}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.4 var(--md-ref-typeface-plain,system-ui,sans-serif)}
header,main{max-width:1280px;margin:0 auto;padding:12px 16px}
h1{font-size:22px;margin:8px 0 2px}h2{font-size:17px}h3{font-size:14px;margin:12px 0 4px}
a{color:var(--pri);text-decoration:none}a:hover{text-decoration:underline}
.dim{color:var(--dim);font-size:12px}.mono{font-family:var(--md-ref-typeface-code,ui-monospace,monospace);font-size:12px}
nav{display:flex;gap:4px;border-bottom:1px solid var(--line);margin:8px 0}
nav button{background:none;border:0;color:var(--fg);padding:8px 14px;cursor:pointer;font:inherit;border-bottom:2px solid transparent}
nav button[aria-selected=true]{border-bottom-color:var(--pri);color:var(--pri);font-weight:600}
#stats{display:flex;flex-wrap:wrap;gap:8px;margin:8px 0}.stat{background:var(--cont);border-radius:10px;padding:6px 12px;min-width:110px}.stat b{display:block;font-size:20px}.stat span{font-size:11px;color:var(--dim)}
.bar{display:flex;gap:8px;align-items:center;margin:8px 0}.bar input,.bar select{background:var(--cont);color:var(--fg);border:1px solid var(--line);border-radius:8px;padding:7px 10px;font:inherit}.bar input{flex:1;max-width:420px}
table{width:100%;border-collapse:collapse}th{text-align:left;font-size:12px;color:var(--dim);padding:6px 8px;border-bottom:1px solid var(--line);position:sticky;top:0;background:var(--bg)}th[data-sort]{cursor:pointer}
td{padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}tr:hover td{background:var(--cont)}
.b{display:inline-block;border-radius:6px;padding:0 6px;font-size:11px;background:var(--cont);border:1px solid var(--line)}
.s-verified,.k-drop-in{background:var(--md-sys-color-tertiary);color:var(--md-sys-color-on-tertiary);border-color:var(--md-sys-color-tertiary)}.s-invalid{background:var(--md-sys-color-error);color:var(--md-sys-color-on-error)}.s-unverified,.k-partial{background:var(--md-sys-color-secondary);color:var(--md-sys-color-on-secondary);border-color:var(--md-sys-color-secondary)}.s-missing{opacity:.7}.warn{background:var(--md-sys-color-primary-container);color:var(--md-sys-color-on-primary-container)}.k-alt,.k-lib{background:var(--pri);color:var(--bg)}
.links a{margin-right:6px}.cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:16px}ul{padding-left:18px;margin:4px 0}
#events{list-style:none;padding:0}#events li{padding:3px 0;border-bottom:1px solid var(--line)}
`;

const HTML = (feed: Feed) => `<!doctype html>
<html lang="en" data-theme="aphrody">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${l10n("gnu_upstream_and_rust_equivalents_aphrody")}</title>
<meta name="description" content="Real-time tracker of every GNU package upstream release, its Rust equivalent, and the Linux kernel.">
<link rel="alternate" type="application/atom+xml" title="${l10n("gnu_releases")}" href="feed.atom">
<link rel="alternate" type="application/json" title="${l10n("gnu_feed")}" href="feed.json">
<link rel="stylesheet" href="tokens.css"><link rel="stylesheet" href="themes.css">
<style>${CSS}</style>
</head>
<body>
<header>
<h1>${l10n("gnu_upstream_in_real_time")}</h1>
<div class="dim"><span id="generated">${feed.generated.slice(0, 19).replace("T", " ")} UTC</span> | <a href="feed.json">${l10n("feed_json")}</a> | <a href="feed.atom">${l10n("feed_atom")}</a> | <a href="events.json">${l10n("events_json")}</a></div>
<div id="stats"></div>
<nav aria-label="${l10n("sections")}" role="tablist"><button role="tab" id="t-packages" aria-controls="packages" data-tab="packages" aria-selected="true">${l10n("packages")}</button><button role="tab" id="t-kernel" aria-controls="kernel-tab" data-tab="kernel-tab" aria-selected="false" tabindex="-1">${l10n("linux_kernel")}</button><button role="tab" id="t-changes" aria-controls="changes" data-tab="changes" aria-selected="false" tabindex="-1">${l10n("changes")}</button><button role="tab" id="t-about" aria-controls="about" data-tab="about" aria-selected="false" tabindex="-1">${l10n("feeds")}</button></nav>
</header>
<main>
<section id="packages" role="tabpanel" aria-labelledby="t-packages">
<div class="bar"><input id="q" type="search" placeholder="${l10n("search_packages_descriptions_rust_equivalents")}" aria-label="${l10n("search")}"><select id="f" aria-label="${l10n("filter")}"><option value="all">${l10n("all")}</option><option value="rust">${l10n("with_rust_equivalent")}</option><option value="drop-in">${l10n("rust_drop_in")}</option><option value="norust">${l10n("no_rust_equivalent")}</option><option value="drift">${l10n("gsrc_behind_upstream")}</option><option value="verified">${l10n("signature_verified")}</option><option value="proven">${l10n("proof_present")}</option><option value="mirrored">${l10n("mirrored_to_r2")}</option></select><span id="count" class="dim" role="status"></span></div>
<table><thead><tr><th data-sort="id">${l10n("package")}</th><th data-sort="date">${l10n("upstream")}</th><th data-sort="sig">${l10n("signature")}</th><th>${l10n("gsrc")}</th><th data-sort="rust">${l10n("rust_equivalent")}</th><th>${l10n("proof")}</th><th>${l10n("links")}</th></tr></thead><tbody id="rows"></tbody></table>
</section>
<section id="kernel-tab" role="tabpanel" aria-labelledby="t-kernel" hidden><div id="kernel"></div></section>
<section id="changes" role="tabpanel" aria-labelledby="t-changes" hidden><h2>${l10n("recent_changes")}</h2><ul id="events"></ul></section>
<section id="about" role="tabpanel" aria-labelledby="t-about" hidden>
<h2>${l10n("feeds")}</h2>
<ul><li><a href="feed.json">${l10n("feed_json")}</a>${l10n("one_entry_per_package_plus_the_kernel_section")}</li><li><a href="feed.atom">${l10n("feed_atom")}</a>${l10n("atom_1_0_newest_upstream_release_first")}</li><li><a href="events.json">${l10n("events_json")}</a>: detected changes (release, Rust, announce, kernel).</li></ul>
<p class="dim">Sources: ftp.gnu.org ls-lrRt, info-gnu archive, Savannah git, GSRC, GitHub releases, crates.io, kernel.org, Android GKI and ChromeOS gitiles. Polled every 10 minutes on the test host; tarballs and signatures mirrored to R2.</p>
</section>
</main>
<script type="module" src="app.js"></script>
</body>
</html>
`;

/** Build the static site into `out`: index.html, app.js, feed files, tokens. */
export async function buildSite(
  feed: Feed,
  out: string,
  events: GnuEvent[] = [],
) {
  const entry = join(repoRoot(), "system", "gnu-ts", "site", "app.ts");
  const r = await Bun.build({
    entrypoints: [entry],
    outdir: out,
    minify: true,
    target: "browser",
    naming: "app.js",
  });
  if (!r.success) throw new Error(`site bundle failed: ${r.logs.join("\n")}`);
  await writeAtomic(join(out, "index.html"), HTML(feed));
  await writeAtomic(join(out, "feed.json"), JSON.stringify(feed));
  await writeAtomic(join(out, "feed.atom"), buildAtom(feed));
  await writeAtomic(
    join(out, "events.json"),
    JSON.stringify(events.slice(-500)),
  );
  for (const f of ["tokens.css", "themes.css"]) {
    const src = Bun.file(join(repoRoot(), "frameworks", "design", "tokens", f));
    if (await src.exists()) await Bun.write(join(out, f), src);
  }
  await writeAtomic(join(out, "robots.txt"), "User-agent: *\nAllow: /\n");
}
