#!/usr/bin/env bun
// material-cdn: measure sizes and latency of the live CDN from this machine (curl timings, bytes on the wire).
//   bun tools/material-cdn/bench.ts [--base URL] [--icons N] [--runs N]
// Cold = a request the CDN has never seen (a fresh random icon list, so the Worker builds it); warm = the same URL again (edge cache);
// R2 = the same output through /v1/g/<hash> (read from R2, no edge cache). Sizes: the bytes curl downloads with Accept-Encoding: br.
export {};
const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1]! : fallback;
};
const base = opt("base", "https://material.aphrody.com/v1").replace(/\/$/, "");
const count = Number(opt("icons", "20"));
const runs = Number(opt("runs", "5"));

interface Timing {
  status: number;
  wire: number;
  ttfb: number;
  total: number;
  source: string;
  location: string;
  encoding: string;
}
async function curl(url: string): Promise<Timing> {
  const fmt = "%{http_code} %{size_download} %{time_starttransfer} %{time_total}";
  const p = Bun.spawn(["curl", "-sS", "-o", "NUL", "-D", "-", "-H", "Accept-Encoding: br", "--raw", "-w", `\n${fmt}`, url], { stdout: "pipe", stderr: "pipe" });
  const [out, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  if ((await p.exited) !== 0) throw new Error(`curl ${url}: ${err}`);
  const lines = out.trim().split(/\r?\n/);
  const [status, wire, ttfb, total] = lines.pop()!.split(" ");
  const header = (n: string) => lines.find((l) => l.toLowerCase().startsWith(`${n}:`))?.slice(n.length + 1).trim() ?? "";
  return { status: Number(status), wire: Number(wire), ttfb: Number(ttfb) * 1000, total: Number(total) * 1000, source: header("x-material-source"), location: header("content-location"), encoding: header("content-encoding") };
}
const median = (a: number[]) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)]!;

const index = (await (await fetch(`${base}/index.json`)).json()) as { icons: { name: string; styles: string[] }[]; fonts: { slug: string; subset: string; bytes: number }[] };
// Upstream's SVG set and the variable fonts disagree on a few dozen names: draw from the names every output can serve.
const fontNames = new Set(((await (await fetch(`${base}/data/symbols/outlined.json`)).json()) as { names: string[] }).names);
const names = index.icons.filter((i) => i.styles.length > 0 && fontNames.has(i.name)).map((i) => i.name);
const pick = (n: number) => {
  const chosen = new Set<string>();
  while (chosen.size < n) chosen.add(names[Math.floor(Math.random() * names.length)]!);
  return [...chosen].sort();
};
const full = (slug: string) => index.fonts.find((f) => f.slug === slug && (f.subset === "variable" || f.subset === "full"))!.bytes;

console.log(`## Sizes (${count} random icons, bytes on the wire with Accept-Encoding: br)\n`);
console.log("| Output | Bytes | vs full |\n| --- | ---: | ---: |");
const icons = pick(count).join(",");
const sizeRows: [string, string, number][] = [
  ["Outlined full variable font (static file)", "", full("material-symbols-outlined")],
  ["Rounded full variable font (static file)", "", full("material-symbols-rounded")],
  ["Sharp full variable font (static file)", "", full("material-symbols-sharp")],
];
for (const [label, path, ref] of [
  ["Outlined, defaults (FILL 0, wght 400, GRAD 0, opsz 24)", `icons.woff2?style=outlined&icons=${icons}`, full("material-symbols-outlined")],
  ["Outlined, FILL 0..1", `icons.woff2?style=outlined&icons=${icons}&fill=0..1`, full("material-symbols-outlined")],
  ["Outlined, FILL 0..1 + wght 100..700", `icons.woff2?style=outlined&icons=${icons}&fill=0..1&wght=100..700`, full("material-symbols-outlined")],
  ["Outlined, all four axes whole", `icons.woff2?style=outlined&icons=${icons}&axes=all`, full("material-symbols-outlined")],
  ["Rounded, FILL 0..1 + wght 100..700", `icons.woff2?style=rounded&icons=${icons}&fill=0..1&wght=100..700`, full("material-symbols-rounded")],
  ["Sharp, FILL 0..1", `icons.woff2?style=sharp&icons=${icons}&fill=0..1`, full("material-symbols-sharp")],
  ["Sprite, both fills (SVG)", `icons.svg?style=outlined&icons=${icons}&fill=0..1&ids=of`, 0],
  ["Sprite, both fills, rounded", `icons.svg?style=rounded&icons=${icons}&fill=0..1&ids=of`, 0],
  ["JSON path list, both fills", `icons.json?style=outlined&icons=${icons}&fill=0..1`, 0],
  ["icons.css (font-face + classes)", `icons.css?style=outlined&icons=${icons}&fill=0..1`, 0],
  ["Google Sans Flex, text=Aphrody, wght 400..700 wdth 100", "fonts/google-sans-flex.woff2?text=Aphrody&wght=400..700&wdth=100", full("google-sans-flex")],
  ["Google Sans Flex, latin, wght 400 only", "fonts/google-sans-flex.woff2?subsets=latin&wght=400&wdth=100", full("google-sans-flex")],
  ["Google Sans Flex, latin, all axes whole", "fonts/google-sans-flex.woff2?subsets=latin&axes=all", full("google-sans-flex")],
  ["Roboto Flex, U+20-7E, wght 400 opsz 14 wdth 100", "fonts/roboto-flex.woff2?unicodes=20-7E&wght=400&opsz=14&wdth=100", full("roboto-flex")],
  ["Google Sans Code, U+20-7E, wght 400..700", "fonts/google-sans-code.woff2?unicodes=20-7E&wght=400..700", full("google-sans-code")],
] as [string, string, number][]) {
  const t = await curl(`${base}/${path}`);
  if (t.status !== 200) throw new Error(`${path}: ${t.status}`);
  sizeRows.push([label, path, t.wire]);
  if (ref) sizeRows[sizeRows.length - 1]![2] = t.wire;
  console.log(`| ${label} | ${t.wire} | ${ref ? `${((t.wire / ref) * 100).toFixed(3)}% of ${ref}` : ""} |`);
}
for (const [label, , bytes] of sizeRows.slice(0, 3)) console.log(`| ${label} | ${bytes} | |`);

console.log(`\n## Latency from this machine (ms, median of ${runs}; each cold run uses a new random list of ${count} icons)\n`);
console.log("| Request | Cold (built) | Warm (edge cache) | R2 (g/ path) |\n| --- | ---: | ---: | ---: |");
const cases: [string, (list: string) => string][] = [
  ["icons.woff2 outlined, FILL 0..1", (l) => `icons.woff2?style=outlined&icons=${l}&fill=0..1`],
  ["icons.woff2 rounded, FILL 0..1 + wght 100..700 (15 MB source)", (l) => `icons.woff2?style=rounded&icons=${l}&fill=0..1&wght=100..700`],
  ["icons.svg sprite, both fills", (l) => `icons.svg?style=outlined&icons=${l}&fill=0..1&ids=of`],
  ["icons.css", (l) => `icons.css?style=outlined&icons=${l}&fill=0..1`],
];
for (const [label, path] of cases) {
  const cold: number[] = [];
  const warm: number[] = [];
  const r2: number[] = [];
  for (let i = 0; i < runs; i++) {
    const url = `${base}/${path(pick(count).join(","))}`;
    const first = await curl(url);
    if (first.status !== 200) throw new Error(`${url}: ${first.status}`);
    cold.push(first.total);
    await Bun.sleep(1500); // the cache write is in waitUntil
    const again = await curl(url);
    warm.push(again.total);
    r2.push((await curl(`${new URL(base).origin}${first.location}`)).total);
  }
  console.log(`| ${label} | ${median(cold).toFixed(0)} | ${median(warm).toFixed(0)} | ${median(r2).toFixed(0)} |`);
}
const stat = await curl(`${base}/sprites/outlined.svg`);
const statWarm = await curl(`${base}/sprites/outlined.svg`);
console.log(`\nStatic reference: sprites/outlined.svg (${stat.wire} B on the wire, content-encoding ${stat.encoding || "none"}): ${stat.total.toFixed(0)} ms first, ${statWarm.total.toFixed(0)} ms second.`);
