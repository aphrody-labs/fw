#!/usr/bin/env bun
// material-cdn step 2: turn the synced sources into the publishable /v1 tree.
//   bun tools/material-cdn/build.ts [--work DIR] [--out DIR]
// Output tree (out/v1):
//   index.json                         icons (name, codepoint, tags, categories, styles, sha256, size) + fonts + sources
//   manifest/<style>.json              sha256 and size of every per-icon SVG of a style
//   icons/<style>/[w<weight>/]<name>[-fill].svg   minified, currentColor, viewBox 0 -960 960 960; style outlined|rounded|sharp
//   sprites/<style>[-fill].svg         one <symbol id=name> per icon (weight 400)
//   fonts/<slug>.css, fonts/<slug>/<subset>.<sha8>.woff2   content-addressed WOFF2 plus unicode-range subsets
//   rust/material_symbols.rs           self-contained constant table (codepoints + outlined and outlined-fill path data)
//   rust/<version>.tar.gz              Cargo crate source (material-symbols-data), reproducible, with .sha256
//   licenses/                          Apache-2.0 (icons) and OFL-1.1 (fonts) texts
import { mkdir, readdir, readFile, writeFile, copyFile, rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import { existsSync } from "node:fs";

const args = process.argv.slice(2);
const opt = (name: string, fallback: string) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const work = resolve(opt("work", ".cache/material-cdn"));
const out = resolve(opt("out", ".cache/material-cdn-out"));
const v1 = join(out, "v1");
const tool = import.meta.dir;

const STYLES = ["outlined", "rounded", "sharp"] as const;
const WEIGHTS = [100, 200, 300, 400, 500, 600, 700] as const;
const VIEWBOX = "0 -960 960 960";
const sources = JSON.parse(await readFile(join(work, "sources.json"), "utf8"));

const sha256 = (data: string | Uint8Array) => new Bun.CryptoHasher("sha256").update(data).digest("hex");
const sorted = <T>(a: T[], key: (t: T) => string) => a.sort((x, y) => (key(x) < key(y) ? -1 : key(x) > key(y) ? 1 : 0));

async function run(cmd: string[], cwd?: string): Promise<string> {
  const p = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  const [o, e, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  if (code !== 0) throw new Error(`${cmd.join(" ")} exited ${code}: ${e.trim() || o.trim()}`);
  return o;
}

async function pool<T>(items: T[], n: number, fn: (t: T) => Promise<void>) {
  let i = 0;
  await Promise.all(
    Array.from({ length: n }, async () => {
      while (i < items.length) await fn(items[i++]!);
    }),
  );
}

await rm(out, { recursive: true, force: true });
for (const d of ["icons", "manifest", "sprites", "fonts", "rust", "licenses", "data"]) await mkdir(join(v1, d), { recursive: true });

// ---------------------------------------------------------------- version
// Cargo semver: upstream commit date plus a content hash of the generated tables (set below), so two different tarballs never share a version.
// 2026-10-23 -> 2026.1023.<decimal of the first 6 hex of the hash>. Month and day carry no leading zero (semver).
const [yy, mm, dd] = (sources.icons.date as string).split("-");
let version = "";

// ---------------------------------------------------------------- icons
const iconsRoot = join(work, "material-design-icons");
const webDir = join(iconsRoot, "symbols", "web");

interface Variant {
  weight: number;
  fill: boolean;
  svg: string;
  d: string;
  /** Drawn from the variable font, not shipped by upstream. */
  drawn?: boolean;
}
// name -> style -> variants
const icons = new Map<string, Map<string, Variant[]>>();
const manifests: Record<string, Record<string, string>> = { outlined: {}, rounded: {}, sharp: {} };
let fileCount = 0;
let totalBytes = 0;

const ARITY: Record<string, number> = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, A: 7, Z: 0 };
const fmt = (n: number) => String(Math.round(n * 1e4) / 1e4);

// Some upstream files use another coordinate space: viewBox "0 96 960 960" (k 1, dy -1056) or none, i.e. the 24 grid
// (k 40, dy -960). Rescale so every icon shares viewBox "0 -960 960 960". Relative commands are only scaled; an initial "m" is absolute.
function xform(d: string, k: number, dy: number, file: string): string {
  let i = 0;
  let out = "";
  let cmd = "";
  let first = true;
  const num = (): number => {
    const m = /^[\s,]*(-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)/.exec(d.slice(i));
    if (!m) throw new Error(`${file}: bad path data at ${i}: ${d.slice(i, i + 20)}`);
    i += m[0].length;
    return Number(m[1]);
  };
  const flag = (): number => {
    const m = /^[\s,]*([01])/.exec(d.slice(i));
    if (!m) throw new Error(`${file}: bad arc flag at ${i}`);
    i += m[0].length;
    return Number(m[1]);
  };
  while (true) {
    const ws = /^[\s,]*/.exec(d.slice(i))![0].length;
    i += ws;
    if (i >= d.length) break;
    const c = d[i]!;
    if (/[A-Za-z]/.test(c)) {
      cmd = c;
      i++;
      if (!(cmd.toUpperCase() in ARITY)) throw new Error(`${file}: unknown path command ${cmd}`);
      if (cmd.toUpperCase() === "Z") {
        out += cmd;
        continue;
      }
      out += cmd;
    } else if (!cmd) throw new Error(`${file}: path data does not start with a command`);
    const U = cmd.toUpperCase();
    const abs = cmd === U || (first && cmd === "m");
    first = false;
    const args: string[] = [];
    if (U === "A") {
      args.push(fmt(num() * k), fmt(num() * k), fmt(num()), String(flag()), String(flag()), fmt(num() * k), fmt(num() * k + (abs ? dy : 0)));
    } else if (U === "V") {
      args.push(fmt(num() * k + (abs ? dy : 0)));
    } else if (U === "H") {
      args.push(fmt(num() * k));
    } else {
      for (let j = 0; j < ARITY[U]!; j += 2) args.push(fmt(num() * k), fmt(num() * k + (abs ? dy : 0)));
    }
    out += args.join(" ") + " ";
    if (U === "M") cmd = cmd === "M" ? "L" : "l"; // implicit lineto after moveto pairs
  }
  return out.replace(/ ([a-zA-Z])/g, "$1").trim().replace(/ -/g, "-");
}

function minify(raw: string, file: string): { svg: string; d: string } {
  const paths = [...raw.matchAll(/<path\b[^>]*?\sd="([^"]+)"[^>]*\/?>/g)];
  const others = raw.replace(/<svg\b[^>]*>|<\/svg>|<path\b[^>]*\/?>/g, "").trim();
  const vb = /viewBox="([^"]+)"/.exec(raw)?.[1];
  if (paths.length !== 1 || others || (vb !== undefined && vb !== VIEWBOX && vb !== "0 96 960 960")) throw new Error(`${file}: expected a single <path> in viewBox ${VIEWBOX}, got ${paths.length} paths, viewBox ${vb}, extra "${others.slice(0, 40)}"`);
  let d = paths[0]![1]!.trim();
  if (vb === "0 96 960 960") d = xform(d, 1, -1056, file);
  else if (vb === undefined) d = xform(d, 40, -960, file);
  if (/["\\]/.test(d)) throw new Error(`${file}: path data has a character that needs escaping`);
  return { d, svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${VIEWBOX}" fill="currentColor"><path d="${d}"/></svg>` };
}

const names = (await readdir(webDir)).sort();
await pool(names, 48, async (name) => {
  for (const style of STYLES) {
    const dir = join(webDir, name, `materialsymbols${style}`);
    if (!existsSync(dir)) continue;
    const variants: Variant[] = [];
    for (const f of await readdir(dir)) {
      if (!f.endsWith("_24px.svg") || !f.startsWith(`${name}_`)) continue;
      const tag = f.slice(name.length + 1, -"_24px.svg".length); // "", "fill1", "wght300", "wght300fill1"
      const m = /^(?:wght(\d+))?(fill1)?$/.exec(tag);
      if (!m) continue;
      const weight = m[1] ? Number(m[1]) : 400;
      const { svg, d } = minify(await readFile(join(dir, f), "utf8"), `${name}/${f}`);
      variants.push({ weight, fill: !!m[2], svg, d });
    }
    if (!variants.length) continue;
    if (!icons.has(name)) icons.set(name, new Map());
    icons.get(name)!.set(style, variants);
  }
});

// Icons with no static SVG upstream under any of their names (the codepoint has no file): drawn from the variable font at opsz 24, GRAD 0,
// every weight and both fills (glyphs.py), under every name of the codepoint, so each sprite and single-icon request finds them.
const codepoints = new Map<string, number>();
for (const style of STYLES) {
  const file = join(iconsRoot, "variablefont", `MaterialSymbols${style[0]!.toUpperCase()}${style.slice(1)}[FILL,GRAD,opsz,wght].codepoints`);
  for (const line of (await readFile(file, "utf8")).split("\n")) {
    const [n, hex] = line.trim().split(/\s+/);
    if (n && hex) codepoints.set(n, parseInt(hex, 16));
  }
}
let drawnIcons = 0;
for (const style of STYLES) {
  const namesOf = new Map<number, string[]>();
  for (const [name, cp] of codepoints) namesOf.set(cp, [...(namesOf.get(cp) ?? []), name]);
  const missing = [...namesOf].filter(([, names]) => !names.some((n) => icons.get(n)?.has(style)));
  if (!missing.length) continue;
  const tmp = join(work, `glyphs-${style}.json`);
  const ttf = join(iconsRoot, "variablefont", `MaterialSymbols${style[0]!.toUpperCase()}${style.slice(1)}[FILL,GRAD,opsz,wght].ttf`);
  await run(["uv", "run", "--quiet", "--with", "fonttools==4.68.0", join(tool, "glyphs.py"), ttf, tmp, missing.map(([cp]) => cp.toString(16)).join(",")]);
  const drawn = JSON.parse(await readFile(tmp, "utf8")) as Record<string, Record<string, Record<string, string>>>;
  for (const [cp, names] of missing) {
    for (const name of names) {
      const variants: Variant[] = [];
      for (const weight of WEIGHTS) {
        for (const fill of [0, 1]) {
          const d = drawn[String(cp)]![String(weight)]![String(fill)]!;
          variants.push({ weight, fill: fill === 1, d, drawn: true, svg: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${VIEWBOX}" fill="currentColor"><path d="${d}"/></svg>` });
        }
      }
      if (!icons.has(name)) icons.set(name, new Map());
      icons.get(name)!.set(style, variants);
      drawnIcons++;
    }
  }
}
console.log(`icons drawn from the variable fonts (no upstream SVG): ${drawnIcons} names x ${STYLES.length} styles`);

const rel = (v: Variant, name: string) => `${v.weight === 400 ? "" : `w${v.weight}/`}${name}${v.fill ? "-fill" : ""}.svg`;
const writes: [string, string][] = [];
for (const [name, byStyle] of icons) {
  for (const [style, variants] of byStyle) {
    for (const v of variants) {
      const r = rel(v, name);
      writes.push([join(v1, "icons", style, r), v.svg]);
      manifests[style]![r] = `${sha256(v.svg)}:${Buffer.byteLength(v.svg)}`;
      fileCount++;
      totalBytes += Buffer.byteLength(v.svg);
    }
  }
}
const madeDirs = new Set<string>();
await pool(writes, 64, async ([p, body]) => {
  const dir = p.slice(0, Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\")));
  if (!madeDirs.has(dir)) {
    madeDirs.add(dir);
    await mkdir(dir, { recursive: true });
  }
  await Bun.write(p, body);
});
console.log(`icons: ${icons.size} names, ${fileCount} svg files, ${totalBytes} bytes`);

const get = (name: string, style: string, fill: boolean) => icons.get(name)?.get(style)?.find((v) => v.weight === 400 && v.fill === fill);

// sprites (weight 400)
const spriteStats: Record<string, { bytes: number; symbols: number }> = {};
for (const style of STYLES) {
  for (const fill of [false, true]) {
    let body = '<svg xmlns="http://www.w3.org/2000/svg">';
    let n = 0;
    for (const name of [...icons.keys()].sort()) {
      const v = get(name, style, fill);
      if (!v) continue;
      body += `<symbol id="${name}" viewBox="${VIEWBOX}"><path d="${v.d}"/></symbol>`;
      n++;
    }
    body += "</svg>";
    const file = `${style}${fill ? "-fill" : ""}.svg`;
    await writeFile(join(v1, "sprites", file), body);
    spriteStats[file] = { bytes: Buffer.byteLength(body), symbols: n };
  }
}

// ---------------------------------------------------------------- codepoints and metadata
interface Meta {
  name: string;
  popularity?: number;
  codepoint?: number;
  tags?: string[];
  categories?: string[];
}
let meta = new Map<string, Meta>();
try {
  const r = await fetch("https://fonts.google.com/metadata/icons?key=material_symbols&incomplete=true", { headers: { "User-Agent": "fw-material-cdn" } });
  const text = (await r.text()).replace(/^\)\]\}'\s*/, "");
  meta = new Map((JSON.parse(text).icons as Meta[]).map((i) => [i.name, i]));
} catch (e) {
  console.warn(`tags/categories unavailable (fonts.google.com metadata): ${e}`);
}

// ---------------------------------------------------------------- fonts
const FONT_RANGES: Record<string, string> = {
  "cyrillic-ext": "U+0460-052F,U+1C80-1C8A,U+20B4,U+2DE0-2DFF,U+A640-A69F,U+FE2E-FE2F",
  cyrillic: "U+0301,U+0400-045F,U+0490-0491,U+04B0-04B1,U+2116",
  "greek-ext": "U+1F00-1FFF",
  greek: "U+0370-0377,U+037A-037F,U+0384-038A,U+038C,U+038E-03A1,U+03A3-03FF",
  vietnamese: "U+0102-0103,U+0110-0111,U+0128-0129,U+0168-0169,U+01A0-01A1,U+01AF-01B0,U+0300-0301,U+0303-0304,U+0308-0309,U+0323,U+0329,U+1EA0-1EF9,U+20AB",
  "latin-ext": "U+0100-02BA,U+02BD-02C5,U+02C7-02CC,U+02CE-02D7,U+02DD-02FF,U+0304,U+0308,U+0329,U+1D00-1DBF,U+1E00-1E9F,U+1EF2-1EFF,U+2020,U+20A0-20AB,U+20AD-20C0,U+2113,U+2C60-2C7F,U+A720-A7FF",
  latin: "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD",
  symbols: "U+2190-21FF,U+2300-23FF,U+2500-27BF,U+2900-297F",
};

const fontSources = sources.fonts.files as Record<string, { family: string; file: string; sha256: string; size: number }>;
const fontTtf = (id: string) => join(work, "fonts", id, fontSources[id]!.file);
const familyOf = (slug: string) => fontSources[slug.replace(/-(upright|ui)$/, "")]?.family ?? slug;
const specs: { slug: string; ttf: string; pin?: Record<string, number | [number, number]>; ranges: Record<string, string>; symbols?: { codepoints: string; out: string } }[] = [
  // Profiles of the same family: all axes; upright (slnt 0, the axes of the Google Fonts css2 API); ui (also wdth 100: opsz, wght, ROND, about a fifth of the size).
  { slug: "google-sans-flex", ttf: fontTtf("google-sans-flex"), ranges: FONT_RANGES },
  { slug: "google-sans-flex-upright", ttf: fontTtf("google-sans-flex"), pin: { slnt: 0 }, ranges: FONT_RANGES },
  { slug: "google-sans-flex-ui", ttf: fontTtf("google-sans-flex"), pin: { slnt: 0, wdth: 100 }, ranges: FONT_RANGES },
  { slug: "google-sans-code", ttf: fontTtf("google-sans-code"), ranges: FONT_RANGES },
  { slug: "google-sans-code-italic", ttf: fontTtf("google-sans-code-italic"), ranges: FONT_RANGES },
  { slug: "roboto-flex", ttf: fontTtf("roboto-flex"), ranges: FONT_RANGES },
  { slug: "roboto-flex-upright", ttf: fontTtf("roboto-flex"), pin: { slnt: 0 }, ranges: FONT_RANGES },
];
const symbolsDir = join(v1, "data", "symbols");
await mkdir(symbolsDir, { recursive: true });
const symbolSpecs = STYLES.map((style) => {
  const base = join(iconsRoot, "variablefont", `MaterialSymbols${style[0]!.toUpperCase()}${style.slice(1)}[FILL,GRAD,opsz,wght]`);
  return { slug: `material-symbols-${style}`, ttf: `${base}.ttf`, ranges: {}, symbols: { codepoints: `${base}.codepoints`, out: join(symbolsDir, `${style}.json`) } };
});
const fontTmp = join(out, "_fonts");
await mkdir(fontTmp, { recursive: true });
await writeFile(join(fontTmp, "spec.json"), JSON.stringify([...specs, ...symbolSpecs]));
const report = JSON.parse(
  await run(["uv", "run", "--quiet", "--with", "fonttools==4.68.0", "--with", "brotli==1.2.1", join(tool, "fonts.py"), fontTmp, join(fontTmp, "spec.json")]),
) as { slug: string; ttf: string; axes: { tag: string; min: number; default: number; max: number }[]; glyphs: number; cmap?: number[][]; icons?: number; files: { subset: string; file: string; bytes: number; range: string | null }[] }[];

interface FontFile {
  family: string;
  slug: string;
  subset: string;
  path: string;
  bytes: number;
  sha256: string;
  range: string | null;
}
const fontFiles: FontFile[] = [];
async function emitFont(slug: string, subset: string, src: string, range: string | null) {
  const body = new Uint8Array(await Bun.file(src).arrayBuffer());
  const h = sha256(body);
  const dir = join(v1, "fonts", slug);
  await mkdir(dir, { recursive: true });
  const name = `${subset}.${h.slice(0, 8)}.woff2`;
  await Bun.write(join(dir, name), body);
  fontFiles.push({ family: familyOf(slug), slug, subset, path: `fonts/${slug}/${name}`, bytes: body.length, sha256: h, range });
}
for (const r of report) for (const f of r.files) await emitFont(r.slug, f.subset, f.file, f.range);

const axisOf = (slug: string, tag: string) => report.find((r) => r.slug === slug)?.axes.find((a) => a.tag === tag);
const axesText = (slug: string) => report.find((r) => r.slug === slug)!.axes.map((a) => `${a.tag} ${a.min}..${a.max}`).join(", ");
function fontCss(slug: string, family: string, style: "normal" | "italic"): string {
  const w = axisOf(slug, "wght");
  const wd = axisOf(slug, "wdth");
  let css = `/* ${family}, SIL OFL 1.1, google/fonts. Variable axes: ${axesText(slug)}. Generated by tools/material-cdn. */\n`;
  for (const f of fontFiles.filter((x) => x.slug === slug && x.subset !== "full")) {
    css +=
      `/* ${f.subset} */\n@font-face{font-family:'${family}';font-style:${style};` +
      `font-weight:${w ? `${w.min} ${w.max}` : "400"};` +
      (wd ? `font-stretch:${wd.min}% ${wd.max}%;` : "") +
      `font-display:swap;src:url(${f.path.replace(/^fonts\//, "")}) format('woff2');unicode-range:${f.range}}\n`;
  }
  return css;
}
for (const slug of ["google-sans-flex", "google-sans-flex-upright", "google-sans-flex-ui"]) await writeFile(join(v1, "fonts", `${slug}.css`), fontCss(slug, "Google Sans Flex", "normal"));
for (const slug of ["roboto-flex", "roboto-flex-upright"]) await writeFile(join(v1, "fonts", `${slug}.css`), fontCss(slug, "Roboto Flex", "normal"));
await writeFile(join(v1, "fonts", "google-sans-code.css"), fontCss("google-sans-code", "Google Sans Code", "normal") + fontCss("google-sans-code-italic", "Google Sans Code", "italic"));

// Material Symbols variable fonts: the upstream WOFF2 is published as is (all four axes), the TTF is the source the Worker subsets.
const srcDir = join(v1, "src");
await mkdir(srcDir, { recursive: true });
const sourceFiles: Record<string, { path: string; bytes: number; sha256: string }> = {};
async function emitSource(slug: string, ttf: string) {
  const body = new Uint8Array(await Bun.file(ttf).arrayBuffer());
  const h = sha256(body);
  const name = `${slug}.${h.slice(0, 8)}.ttf`;
  await Bun.write(join(srcDir, name), body);
  sourceFiles[slug] = { path: `src/${name}`, bytes: body.length, sha256: h };
}
const symbolAxes = (style: string) => report.find((r) => r.slug === `material-symbols-${style}`)!.axes;
let symbolsCss = `/* Material Symbols (Apache-2.0, google/material-design-icons). Axes ${symbolAxes("outlined").map((a) => `${a.tag} ${a.min}..${a.max}`).join(", ")}. Generated by tools/material-cdn. */\n`;
for (const style of STYLES) {
  const cap = style[0]!.toUpperCase() + style.slice(1);
  const src = join(iconsRoot, "variablefont", `MaterialSymbols${cap}[FILL,GRAD,opsz,wght].woff2`);
  await emitFont(`material-symbols-${style}`, "variable", src, null);
  const f = fontFiles.find((x) => x.slug === `material-symbols-${style}`)!;
  symbolsCss +=
    `@font-face{font-family:'Material Symbols ${cap}';font-style:normal;font-weight:100 700;font-display:block;src:url(${f.path.replace(/^fonts\//, "")}) format('woff2')}\n` +
    `.material-symbols-${style}{font-family:'Material Symbols ${cap}';font-weight:400;font-style:normal;font-size:24px;line-height:1;letter-spacing:normal;text-transform:none;display:inline-block;white-space:nowrap;word-wrap:normal;direction:ltr;font-feature-settings:'liga';-webkit-font-smoothing:antialiased;font-variation-settings:'FILL' 0,'wght' 400,'GRAD' 0,'opsz' 24}\n`;
  await emitSource(`material-symbols-${style}`, join(iconsRoot, "variablefont", `MaterialSymbols${cap}[FILL,GRAD,opsz,wght].ttf`));
}
await writeFile(join(v1, "fonts", "material-symbols.css"), symbolsCss);
for (const id of Object.keys(fontSources)) await emitSource(id, fontTtf(id));
await rm(fontTmp, { recursive: true, force: true });

// ---------------------------------------------------------------- licenses
await copyFile(join(iconsRoot, "LICENSE"), join(v1, "licenses", "material-design-icons-APACHE-2.0.txt"));
await copyFile(join(work, "fonts", "google-sans-flex", "OFL.txt"), join(v1, "licenses", "google-sans-flex-OFL-1.1.txt"));
await copyFile(join(work, "fonts", "google-sans-code", "OFL.txt"), join(v1, "licenses", "google-sans-code-OFL-1.1.txt"));
await copyFile(join(work, "fonts", "roboto-flex", "OFL.txt"), join(v1, "licenses", "roboto-flex-OFL-1.1.txt"));
const notice =
  `Material Symbols and Material Icons: Copyright Google LLC, Apache License 2.0 (licenses/material-design-icons-APACHE-2.0.txt), https://github.com/google/material-design-icons.\n` +
  `Google Sans Flex: Copyright 2025 The Google Sans Flex Project Authors, SIL Open Font License 1.1 (licenses/google-sans-flex-OFL-1.1.txt), https://github.com/googlefonts/googlesans-flex.\n` +
  `Google Sans Code: Copyright 2025 The Google Sans Code Project Authors, SIL Open Font License 1.1 (licenses/google-sans-code-OFL-1.1.txt), https://github.com/googlefonts/googlesans-code.\n` +
  `Roboto Flex: Copyright 2016 The Roboto Flex Project Authors, SIL Open Font License 1.1 (licenses/roboto-flex-OFL-1.1.txt), https://github.com/googlefonts/roboto-flex.\n` +
  `Font files are read from https://github.com/google/fonts at the commit pinned in tools/material-cdn/lock.json.\n` +
  `Optimized, subset and repackaged by Aphrody (tools/material-cdn in aphrody-labs/fw). SVGs are minified; WOFF2 subsets are derived under the OFL without renaming.\n`;
await writeFile(join(v1, "licenses", "NOTICE.txt"), notice);

// ---------------------------------------------------------------- rust
const rs = (s: string) => `"${s}"`;
const tableFor = (style: string, fill: boolean) => {
  const rows: [string, string][] = [];
  for (const name of icons.keys()) {
    const v = get(name, style, fill);
    // The crate stays the upstream data set: drawn icons are served by the CDN only.
    if (v && !v.drawn) rows.push([name, v.d]);
  }
  sorted(rows, (r) => r[0]);
  return `&[\n${rows.map(([n, d]) => `(${rs(n)},${rs(d)}),`).join("\n")}\n]`;
};
const cpRows = sorted([...codepoints.entries()], (r) => r[0]);
const cpTable = `&[\n${cpRows.map(([n, c]) => `(${rs(n)},0x${c.toString(16)}),`).join("\n")}\n]`;
const header = `// @generated by tools/material-cdn from google/material-design-icons @ ${sources.icons.sha.slice(0, 12)} (${sources.icons.date}), Apache-2.0. Do not edit.\n`;
const tables: Record<string, string> = { codepoints: cpTable };
for (const style of STYLES) for (const fill of [false, true]) tables[`${style}${fill ? "_fill" : ""}`] = tableFor(style, fill);
version = `${yy}.${Number(mm! + dd!)}.${parseInt(sha256(sources.icons.sha + Object.keys(tables).sort().map((k) => tables[k]).join("\0")).slice(0, 6), 16)}`;

const single =
  header +
  `pub const VERSION: &str = ${rs(version)};\npub const VIEWBOX: &str = ${rs(VIEWBOX)};\n` +
  `/// (name, Unicode codepoint) sorted by name; the font ligature for a name is the name itself.\npub static CODEPOINTS: &[(&str, u32)] = ${cpTable};\n` +
  `/// (name, SVG path data in viewBox 0 -960 960 960) sorted by name, weight 400, outlined.\npub static OUTLINED: &[(&str, &str)] = ${tables.outlined};\n` +
  `/// Same icons, filled.\npub static OUTLINED_FILL: &[(&str, &str)] = ${tables.outlined_fill};\n`;
await writeFile(join(v1, "rust", "material_symbols.rs"), single);

// crate tarball: material-symbols-data-<version>/ (Cargo.toml, src/*.rs, licenses)
const crateName = "material-symbols-data";
const stage = join(out, "_crate");
const top = join(stage, `${crateName}-${version}`);
await mkdir(join(top, "src"), { recursive: true });
await writeFile(
  join(top, "Cargo.toml"),
  `[package]
name = "${crateName}"
version = "${version}"
edition = "2021"
license = "Apache-2.0"
description = "Material Symbols weight-400 SVG path data and codepoints, generated from google/material-design-icons @ ${sources.icons.sha.slice(0, 12)}."
repository = "https://github.com/aphrody-labs/material-design-icons"
publish = false

[lib]
path = "src/lib.rs"

[features]
default = ["outlined"]
outlined = []
rounded = []
sharp = []
`,
);
const libRs =
  header +
  `#![no_std]\n\npub const VERSION: &str = ${rs(version)};\npub const VIEWBOX: &str = ${rs(VIEWBOX)};\n` +
  `/// (name, Unicode codepoint) sorted by name.\npub static CODEPOINTS: &[(&str, u32)] = include!("codepoints.rs");\n` +
  STYLES.map((s) => {
    const up = s.toUpperCase();
    return (
      `#[cfg(feature = "${s}")]\npub static ${up}: &[(&str, &str)] = include!("${s}.rs");\n` +
      `#[cfg(feature = "${s}")]\npub static ${up}_FILL: &[(&str, &str)] = include!("${s}_fill.rs");\n`
    );
  }).join("");
await writeFile(join(top, "src", "lib.rs"), libRs);
for (const [k, body] of Object.entries(tables)) await writeFile(join(top, "src", `${k}.rs`), body + "\n");
await copyFile(join(v1, "licenses", "material-design-icons-APACHE-2.0.txt"), join(top, "LICENSE"));
// The published crate version is immutable and hashed over the tables only, so its NOTICE must not change when the CDN serves a new family.
await writeFile(join(top, "NOTICE"), notice.split("\n").filter((line) => !/^(Roboto Flex|Font files are read)/.test(line)).join("\n"));
const tgz = join(v1, "rust", `${version}.tar.gz`);
// Reproducible: sorted names, fixed mtime/owner, gzip without a timestamp.
await run([
  "bash",
  "-c",
  `tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner --mode='u=rwX,go=rX' -cf - -C '${stage}' '${crateName}-${version}' | gzip -n -9 > '${tgz}'`,
]);
const tgzBytes = new Uint8Array(await Bun.file(tgz).arrayBuffer());
const tgzSha = sha256(tgzBytes);
await writeFile(`${tgz}.sha256`, `${tgzSha}  ${version}.tar.gz\n`);
await rm(stage, { recursive: true, force: true });

// ---------------------------------------------------------------- manifests and index
const manifestInfo: Record<string, { path: string; files: number; sha256: string; bytes: number }> = {};
for (const style of STYLES) {
  const body = JSON.stringify({ version, style, files: Object.fromEntries(Object.entries(manifests[style]!).sort(([x], [y]) => (x < y ? -1 : x > y ? 1 : 0))) });
  await writeFile(join(v1, "manifest", `${style}.json`), body);
  manifestInfo[style] = { path: `manifest/${style}.json`, files: Object.keys(manifests[style]!).length, sha256: sha256(body), bytes: Buffer.byteLength(body) };
}
const allNames = sorted([...new Set([...codepoints.keys(), ...icons.keys()])], (n) => n); // codepoints come from the fonts, so names that only exist as SVG have a null codepoint
const iconRows = allNames.map((name) => {
  const m = meta.get(name);
  const base = get(name, "outlined", false);
  return {
    name,
    codepoint: codepoints.get(name) ?? m?.codepoint ?? null,
    tags: m?.tags ?? [],
    categories: m?.categories ?? [],
    popularity: m?.popularity ?? null,
    styles: STYLES.filter((s) => icons.get(name)?.has(s)),
    sha256: base ? sha256(base.svg) : null,
    size: base ? Buffer.byteLength(base.svg) : null,
  };
});
const index = {
  version,
  license: { icons: "Apache-2.0", fonts: "OFL-1.1", notice: "licenses/NOTICE.txt" },
  sources,
  assets: {
    count: iconRows.length,
    withSvg: icons.size,
    styles: STYLES,
    weights: WEIGHTS,
    fills: [0, 1],
    size: 24,
    viewBox: VIEWBOX,
    path: "icons/<style>/[w<weight>/]<name>[-fill].svg",
    note: "sha256 and size are those of icons/outlined/<name>.svg (weight 400, no fill); manifest/<style>.json lists every file",
    files: fileCount,
    bytes: totalBytes,
  },
  manifests: manifestInfo,
  sprites: spriteStats,
  fonts: fontFiles.map((f) => ({ family: f.family, slug: f.slug, subset: f.subset, path: f.path, bytes: f.bytes, sha256: f.sha256, unicodeRange: f.range })),
  css: ["fonts/google-sans-flex.css", "fonts/google-sans-flex-upright.css", "fonts/google-sans-flex-ui.css", "fonts/google-sans-code.css", "fonts/roboto-flex.css", "fonts/roboto-flex-upright.css", "fonts/material-symbols.css"],
  meta: "meta.json",
  rust: { single: "rust/material_symbols.rs", crate: `rust/${version}.tar.gz`, crateName, crateVersion: version, crateSha256: tgzSha },
  icons: iconRows,
};
await writeFile(join(v1, "index.json"), JSON.stringify(index));

// meta.json: what the Worker needs for à-la-carte builds (kept small: it is read once per isolate).
const rev = sha256(JSON.stringify(sources)).slice(0, 12);
const families: Record<string, unknown> = {};
for (const style of STYLES) {
  const slug = `material-symbols-${style}`;
  const r = report.find((x) => x.slug === slug)!;
  families[slug] = { family: `Material Symbols ${style[0]!.toUpperCase()}${style.slice(1)}`, kind: "symbols", style, src: sourceFiles[slug]!.path, srcBytes: sourceFiles[slug]!.bytes, axes: r.axes, glyphs: r.glyphs, icons: r.icons, symbols: `data/symbols/${style}.json`, woff2: fontFiles.find((f) => f.slug === slug)!.path };
}
for (const id of Object.keys(fontSources)) {
  const r = report.find((x) => x.slug === id)!;
  families[id] = { family: fontSources[id]!.family, kind: "text", style: id.endsWith("-italic") ? "italic" : "normal", src: sourceFiles[id]!.path, srcBytes: sourceFiles[id]!.bytes, axes: r.axes, glyphs: r.glyphs, cmap: r.cmap };
}
const workerMeta = { rev, version, viewBox: VIEWBOX, styles: STYLES, weights: WEIGHTS, sources, families, ranges: FONT_RANGES };
await writeFile(join(v1, "meta.json"), JSON.stringify(workerMeta));

// summary
let total = 0;
let count = 0;
async function walk(d: string) {
  for (const e of await readdir(d, { withFileTypes: true })) {
    if (e.isDirectory()) await walk(join(d, e.name));
    else {
      count++;
      total += Bun.file(join(d, e.name)).size;
    }
  }
}
await walk(v1);
console.log(JSON.stringify({ version, files: count, bytes: total, icons: icons.size, svg: fileCount, fonts: fontFiles.length, tarball: { bytes: tgzBytes.length, sha256: tgzSha } }));
