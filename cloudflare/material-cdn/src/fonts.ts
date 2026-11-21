// À-la-carte fonts: icon ligature fonts and text subsets, instanced and subset by HarfBuzz, packed to WOFF2, plus the CSS that points at them.
import { canonicalQuery, formatUnicodes, GrammarError, identity, type Need, parseUnicodes } from "../../../tools/material-cdn/grammar.ts";
import { Flag, instantiate, loadInto, subsetAt, type SubsetSpec } from "./hb.ts";
import { type Axis, type Family, loadSymbols, type Meta } from "./meta.ts";
import { toWoff2 } from "./woff2.ts";

/** The axes of a font resolved against a request: what to pin, what to limit, and the CSS descriptors that follow. */
export interface Resolved {
  pin: Record<string, number>;
  pinDefault: string[];
  limit: Record<string, [number, number]>;
  /** Final range of every axis that stays or becomes a range, and the pinned value of the others. */
  final: Record<string, [number, number]>;
}

export function resolveAxes(family: Family, need: Need): Resolved {
  const out: Resolved = { pin: {}, pinDefault: [], limit: {}, final: {} };
  const requested = need.axes ?? {};
  const byLower = new Map(family.axes.map((a) => [a.tag.toLowerCase(), a]));
  for (const key of Object.keys(requested)) {
    if (!byLower.has(key)) throw new GrammarError(`axis ${key} is not in ${family.family}`, { axes: family.axes.map((a) => `${a.tag.toLowerCase()} ${a.min}..${a.max}`) });
  }
  for (const axis of family.axes) {
    const range = requested[axis.tag.toLowerCase()];
    if (!range) {
      if (need.allAxes) out.final[axis.tag] = [axis.min, axis.max];
      else {
        out.pinDefault.push(axis.tag);
        out.final[axis.tag] = [axis.default, axis.default];
      }
      continue;
    }
    const lo = Math.max(range[0], axis.min);
    const hi = Math.min(range[1], axis.max);
    if (lo > hi) throw new GrammarError(`axis ${axis.tag.toLowerCase()}: ${range[0]}..${range[1]} is outside ${axis.min}..${axis.max}`);
    out.final[axis.tag] = [lo, hi];
    if (lo === hi) out.pin[axis.tag] = lo;
    else if (lo !== axis.min || hi !== axis.max) out.limit[axis.tag] = [lo, hi];
  }
  return out;
}

async function sourceInto(env: Env, family: Family): Promise<{ x: ReturnType<typeof instantiate>; pointer: number; length: number }> {
  const object = await env.MATERIAL.get(`v1/${family.src}`);
  if (!object) throw new Error(`R2 object v1/${family.src} is missing`);
  const x = instantiate();
  const { pointer, length } = await loadInto(x, object.body, object.size);
  return { x, pointer, length };
}

const ICON_CHARS = (names: readonly string[]) => [...new Set(names.join("").split("").map((c) => c.charCodeAt(0)))];

export async function iconFont(env: Env, meta: Meta, need: Need): Promise<Uint8Array> {
  const style = need.style ?? "outlined";
  const family = meta.families[`material-symbols-${style}`];
  if (!family) throw new GrammarError(`style ${style}`);
  if (!need.icons?.length) throw new GrammarError("icons: at least one icon name is required");
  const symbols = await loadSymbols(env, meta, style);
  const unknown = need.icons.filter((n) => !symbols.index.has(n));
  if (unknown.length) throw new GrammarError(`unknown Material Symbols in ${style}`, { unknown });
  const axes = resolveAxes(family, need);
  const rows = need.icons.map((n) => symbols.index.get(n)!);
  const fillRange = axes.final.FILL ?? [0, 0];
  // The FILL axis swaps an icon for its `.fill` glyph through GSUB; without layout closure those glyphs must be named explicitly.
  const glyphs = fillRange[1] > 0 ? rows.map((i) => symbols.fill[i]!).filter(Boolean) : [];
  const spec: SubsetSpec = {
    unicodes: [...rows.map((i) => symbols.cp[i]!), ...ICON_CHARS(need.icons)],
    glyphs,
    // No layout closure keeps only the ligatures of the requested names; the default closure would pull in every icon spellable with their letters.
    flags: Flag.NoHinting | Flag.NoLayoutClosure | Flag.OptimizeIupDeltas,
    pin: axes.pin,
    pinDefault: axes.pinDefault,
    limit: axes.limit,
  };
  const { x, pointer, length } = await sourceInto(env, family);
  return toWoff2(subsetAt(x, pointer, length, spec));
}

/** Codepoints a text-font request keeps: text and unicodes, intersected with the named subsets when both are given. */
export function codepointsOf(need: Need, meta: Meta): number[] {
  let fromSubsets: Set<number> | undefined;
  for (const name of need.subsets ?? []) {
    const spec = meta.ranges[name];
    if (!spec) throw new GrammarError(`subsets: "${name}" is not one of ${Object.keys(meta.ranges).join(", ")}`);
    fromSubsets ??= new Set();
    for (const c of parseUnicodes(spec)) fromSubsets.add(c);
  }
  if (fromSubsets && need.unicodes) return need.unicodes.filter((c) => fromSubsets!.has(c));
  if (fromSubsets) return [...fromSubsets].sort((a, b) => a - b);
  return need.unicodes ?? [];
}

const inCmap = (cmap: number[][] | undefined, cp: number) => !cmap || cmap.some(([lo, hi]) => cp >= lo! && cp <= hi!);

export async function textFont(env: Env, meta: Meta, need: Need): Promise<Uint8Array> {
  const family = textFamily(meta, need.family);
  const codepoints = codepointsOf(need, meta).filter((c) => inCmap(family.cmap, c));
  if (!need.unicodes && !need.subsets) throw new GrammarError("a font needs text=, unicodes= or subsets=; the full fonts are the static files in index.json");
  if (!codepoints.length) throw new GrammarError(`${family.family} has none of the requested characters`);
  const axes = resolveAxes(family, need);
  const spec: SubsetSpec = {
    unicodes: codepoints,
    flags: Flag.NoHinting | Flag.OptimizeIupDeltas | Flag.NotdefOutline,
    pin: axes.pin,
    pinDefault: axes.pinDefault,
    limit: axes.limit,
    features: need.features,
  };
  const { x, pointer, length } = await sourceInto(env, family);
  return toWoff2(subsetAt(x, pointer, length, spec));
}

export function textFamily(meta: Meta, slug: string | undefined): Family {
  const family = slug ? meta.families[slug] : undefined;
  if (!family) throw new GrammarError(`unknown font family "${slug}"`, { families: Object.keys(meta.families) });
  return family;
}

const num = (n: number) => String(Math.round(n * 1e4) / 1e4);
const span = ([lo, hi]: [number, number], suffix = "") => (lo === hi ? `${num(lo)}${suffix}` : `${num(lo)}${suffix} ${num(hi)}${suffix}`);

/** `@font-face` descriptors implied by the resolved axes. */
function descriptors(family: Family, axes: Resolved): string {
  let css = `font-style:${family.style === "italic" ? "italic" : "normal"};`;
  const slnt = axes.final.slnt;
  if (slnt && (slnt[0] !== 0 || slnt[1] !== 0)) css = `font-style:oblique ${span([-slnt[1], -slnt[0]], "deg")};`;
  css += `font-weight:${axes.final.wght ? span(axes.final.wght) : "400"};`;
  if (axes.final.wdth) css += `font-stretch:${span(axes.final.wdth, "%")};`;
  return css;
}

/** Relative URL of a generated font, pinned to the revision (immutable). */
const fontUrl = (output: "font" | "icons-font", need: Need, rev: string, name: string) => {
  const q = canonicalQuery(identity({ ...need, output }));
  return `${name}?${q}&v=${rev}`;
};

export async function iconsCss(env: Env, meta: Meta, need: Need): Promise<string> {
  const style = need.style ?? "outlined";
  const family = meta.families[`material-symbols-${style}`];
  if (!family) throw new GrammarError(`style ${style}`);
  if (!need.icons?.length) throw new GrammarError("icons: at least one icon name is required");
  const symbols = await loadSymbols(env, meta, style);
  const unknown = need.icons.filter((n) => !symbols.index.has(n));
  if (unknown.length) throw new GrammarError(`unknown Material Symbols in ${style}`, { unknown });
  const axes = resolveAxes(family, need);
  const name = need.cssFamily ?? family.family;
  const prefix = need.cssClass ?? "ms";
  const variable = Object.entries(axes.final).filter(([, [lo, hi]]) => lo !== hi);
  const settings = Object.entries(axes.final).map(([tag, [lo, hi]]) => `'${tag}' ${num(tag === "wght" && lo !== hi ? Math.min(Math.max(400, lo), hi) : lo)}`);
  let css = `/* ${family.family}: ${need.icons.length} icons, axes ${Object.entries(axes.final).map(([t, r]) => `${t} ${span(r).replace(" ", "..")}`).join(", ")}. Apache-2.0, google/material-design-icons. rev ${meta.rev} */\n`;
  css += `@font-face{font-family:'${name}';${descriptors(family, axes).replace(/font-style:[^;]*;/, "font-style:normal;")}font-display:${need.display ?? "block"};src:url(${fontUrl("icons-font", need, meta.rev, "icons.woff2")}) format('woff2')}\n`;
  css += `.${prefix}{font-family:'${name}';font-weight:normal;font-style:normal;font-size:24px;line-height:1;letter-spacing:normal;text-transform:none;display:inline-block;white-space:nowrap;word-wrap:normal;direction:ltr;font-feature-settings:'liga';-webkit-font-smoothing:antialiased;font-variation-settings:${settings.join(",") || "normal"}}\n`;
  if (variable.some(([tag]) => tag === "FILL")) css += `.${prefix}-fill{font-variation-settings:'FILL' 1}\n`;
  for (const icon of need.icons) css += `.${prefix}-${icon.replaceAll("_", "-")}::before{content:"\\${symbols.cp[symbols.index.get(icon)!]!.toString(16)}"}\n`;
  return css;
}

export function fontCss(meta: Meta, need: Need): string {
  const family = textFamily(meta, need.family);
  if (!need.unicodes && !need.subsets) throw new GrammarError("a font needs text=, unicodes= or subsets=; the static css files in index.json cover the full fonts");
  const axes = resolveAxes(family, need);
  const name = need.cssFamily ?? family.family;
  const display = need.display ?? "swap";
  const faces: { label: string; range: string; need: Need }[] = [];
  if (need.subsets) {
    for (const subset of need.subsets) {
      const one: Need = { ...need, subsets: [subset] };
      const codepoints = codepointsOf(one, meta).filter((c) => inCmap(family.cmap, c));
      if (codepoints.length) faces.push({ label: subset, range: need.unicodes ? formatUnicodes(codepoints) : meta.ranges[subset]!, need: one });
    }
  } else {
    const codepoints = codepointsOf(need, meta).filter((c) => inCmap(family.cmap, c));
    if (codepoints.length) faces.push({ label: "text", range: formatUnicodes(codepoints), need });
  }
  if (!faces.length) throw new GrammarError(`${family.family} has none of the requested characters`);
  let css = `/* ${family.family}, SIL OFL 1.1, google/fonts. Axes ${Object.entries(axes.final).map(([t, r]) => `${t} ${span(r).replace(" ", "..")}`).join(", ")}. rev ${meta.rev} */\n`;
  for (const face of faces) {
    css += `/* ${face.label} */\n@font-face{font-family:'${name}';${descriptors(family, axes)}font-display:${display};src:url(${fontUrl("font", face.need, meta.rev, `${need.family}.woff2`)}) format('woff2');unicode-range:${face.range}}\n`;
  }
  return css;
}

export type { Axis };
