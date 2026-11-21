#!/usr/bin/env bun
// Regenerate the inline icon sprite of the fw web UI from the Material Symbols CDN.
//   bun tools/material-cdn/ui-sprite.ts [--source URL|DIR] [--html FILE]
// tools/material-cdn/ui-icons.json maps each sprite id (`i-add`) to a Material Symbols name and fixes the style and fill.
// Each <symbol> in crates/fw-proto/ui/index.html is rewritten from icons/<style>/<name>[-fill].svg (viewBox 0 -960 960 960).
import { readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const source = opt("source", process.env.MATERIAL_CDN_BASE ?? "https://material.aphrody.com/v1");
const htmlPath = resolve(opt("html", join(import.meta.dir, "..", "..", "crates", "fw-proto", "ui", "index.html")));
const cfg = JSON.parse(await readFile(join(import.meta.dir, "ui-icons.json"), "utf8")) as { style: string; fill: boolean; icons: Record<string, string> };

async function fetchSvg(name: string): Promise<string> {
  const rel = `icons/${cfg.style}/${name}${cfg.fill ? "-fill" : ""}.svg`;
  if (/^https?:/.test(source)) {
    const r = await fetch(`${source.replace(/\/$/, "")}/${rel}`);
    if (!r.ok) throw new Error(`${rel}: ${r.status}`);
    return r.text();
  }
  return readFile(join(source, rel), "utf8");
}

let html = await readFile(htmlPath, "utf8");
for (const [id, name] of Object.entries(cfg.icons)) {
  const d = /<path d="([^"]+)"/.exec(await fetchSvg(name))?.[1];
  if (!d) throw new Error(`${name}: no path`);
  const re = new RegExp(`<symbol id="${id}"[^>]*>.*?</symbol>`);
  if (!re.test(html)) throw new Error(`${id}: no <symbol> in ${htmlPath}`);
  html = html.replace(re, `<symbol id="${id}" viewBox="0 -960 960 960"><path d="${d}"/></symbol>`);
}
await writeFile(htmlPath, html);
console.log(`${Object.keys(cfg.icons).length} symbols rewritten from ${source} (${cfg.style}${cfg.fill ? " fill" : ""})`);
