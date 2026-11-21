// The material.aphrody.com à-la-carte grammar: one parser and one canonical form shared by the Worker, the typed client
// (client.ts) and the tests. Pure TypeScript, no runtime APIs, so it runs in Workers, Bun and browsers.
//
// A request names an output (icons.svg, icons.json, icons.woff2, icons.css, icon.svg, fonts/<slug>.woff2, fonts/<slug>.css)
// and needs, as query parameters:
//   style      outlined | rounded | sharp (Material Symbols)            default outlined
//   set        material | aphrody (icons.svg, icons.json, icon.svg)     default material
//   icons      comma-separated icon names                               (required for icons.*)
//   text       characters to keep (fonts)                               merged with unicodes and subsets
//   unicodes   20-7E,A0 or U+0020-007E,U+00A0 (hex codepoints and ranges; the canonical form has no U+, a literal + in a query means space) (fonts)
//   subsets    named ranges: latin, latin-ext, cyrillic, ... (fonts)
//   <axis>     fill, wght, grad, opsz, wdth, slnt, rond, ...: `400` pins, `300..700` limits to a range;
//              axes not named are pinned to their default unless axes=all keeps them whole
//   axes       default | all
//   features   OpenType layout features to keep besides the defaults (comma list, `*` keeps all)
//   ids        suffix | of: sprite ids `home`, `home-fill` or `o-home`, `f-home`
//   family     CSS font-family name (css outputs)
//   class      CSS class prefix (icons.css)                              default ms
//   display    CSS font-display                                          default block (icons), swap (text)
//   size, color  icon.svg only
//   v          data revision (meta.json rev): pins the response as immutable

export const STYLES = ["outlined", "rounded", "sharp"] as const;
export type Style = (typeof STYLES)[number];
export const WEIGHTS = [100, 200, 300, 400, 500, 600, 700] as const;
export type IconSet = "material" | "aphrody";
export type Output = "sprite" | "json" | "icon" | "icons-font" | "icons-css" | "font" | "font-css";
export const EXTENSION: Record<Output, string> = { sprite: "svg", json: "json", icon: "svg", "icons-font": "woff2", "icons-css": "css", font: "woff2", "font-css": "css" };

/** `[min, max]`; a pin has min === max. */
export type Range = [number, number];

export interface Need {
  output: Output;
  /** Font slug for `font` and `font-css` (e.g. `google-sans-flex`). */
  family?: string;
  style?: Style;
  set?: IconSet;
  icons?: string[];
  /** Unicode codepoints (sorted, unique) from text, unicodes and, for `font` outputs, the named subsets. */
  unicodes?: number[];
  subsets?: string[];
  axes?: Record<string, Range>;
  allAxes?: boolean;
  features?: string[];
  ids?: "suffix" | "of";
  cssFamily?: string;
  cssClass?: string;
  display?: string;
  size?: number;
  color?: string;
  /** Revision pin. */
  v?: string;
}

export const LIMITS = { icons: 1000, unicodes: 30000, text: 40000, nameLength: 64 };

export class GrammarError extends Error {
  constructor(
    message: string,
    readonly detail?: unknown,
  ) {
    super(message);
  }
}

const NAME = /^[a-z0-9_]+$/;
const APHRODY_NAME = /^[a-z0-9_]+(\/[a-z0-9_]+)*$/;
const FEATURE = /^[A-Za-z0-9 ]{4}$/;
const CSS_IDENT = /^[A-Za-z0-9 _-]{1,64}$/;
const COLOR = /^(#[0-9a-fA-F]{3,8}|[a-zA-Z]{3,20}|currentColor)$/;
const DISPLAY = new Set(["auto", "block", "swap", "fallback", "optional"]);
const RESERVED = new Set(["style", "set", "icons", "text", "unicodes", "subsets", "axes", "features", "ids", "family", "class", "display", "size", "color", "v", "name"]);

const num = (s: string, what: string): number => {
  const n = Number(s);
  if (!s.trim() || !Number.isFinite(n)) throw new GrammarError(`${what}: "${s}" is not a number`);
  return n;
};
const fmt = (n: number) => String(Math.round(n * 1e4) / 1e4);

/** `400` -> [400, 400]; `300..700` or `300-700` -> [300, 700] (a leading minus is part of the number). */
export function parseRange(value: string, axis: string): Range {
  const m = /^(-?\d+(?:\.\d+)?)(?:(?:\.\.|\u2013|(?<=\d)-)(-?\d+(?:\.\d+)?))?$/.exec(value.trim());
  if (!m) throw new GrammarError(`axis ${axis}: "${value}" is not a number or a min..max range`);
  const a = num(m[1]!, `axis ${axis}`);
  const b = m[2] === undefined ? a : num(m[2], `axis ${axis}`);
  if (b < a) throw new GrammarError(`axis ${axis}: range ${value} is reversed`);
  return [a, b];
}

/** Codepoints of `U+0020-007E,U+00A0` (the `U+` and case are optional, as in CSS unicode-range). */
export function parseUnicodes(spec: string): number[] {
  const out: number[] = [];
  for (const raw of spec.split(",")) {
    const part = raw.trim().replace(/^U[+ ]/i, "");
    if (!part) continue;
    const m = /^([0-9a-fA-F]{1,6})(?:-([0-9a-fA-F]{1,6}))?$/.exec(part);
    if (!m) throw new GrammarError(`unicodes: "${raw}" is not a codepoint or range`);
    const lo = parseInt(m[1]!, 16);
    const hi = m[2] ? parseInt(m[2], 16) : lo;
    if (hi < lo || hi > 0x10ffff || hi - lo > LIMITS.unicodes) throw new GrammarError(`unicodes: range "${raw}" is invalid or too large`);
    for (let c = lo; c <= hi; c++) out.push(c);
    if (out.length > LIMITS.unicodes) throw new GrammarError(`unicodes: more than ${LIMITS.unicodes} codepoints`);
  }
  return out;
}

/** Merged `U+` ranges of sorted unique codepoints, the compact CSS unicode-range / query form. */
export function formatUnicodes(codepoints: readonly number[], prefix = "U+"): string {
  const parts: string[] = [];
  const hex = (n: number) => n.toString(16).toUpperCase().padStart(2, "0");
  for (let i = 0; i < codepoints.length; ) {
    let j = i;
    while (j + 1 < codepoints.length && codepoints[j + 1] === codepoints[j]! + 1) j++;
    parts.push(i === j ? `${prefix}${hex(codepoints[i]!)}` : `${prefix}${hex(codepoints[i]!)}-${hex(codepoints[j]!)}`);
    i = j + 1;
  }
  return parts.join(",");
}

const uniqueSorted = <T extends string | number>(values: T[]): T[] => [...new Set(values)].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));

const list = (value: string | undefined): string[] => (value ?? "").split(/[,\s]+/).filter(Boolean);

export interface Parsed {
  need: Need;
  /** Canonical query string: sorted keys, merged lists; the hash input of the Worker. */
  canonical: string;
}

/**
 * Parses and canonicalises a request. `output` and `family` come from the path; `params` from the query string and, for POST,
 * from the JSON or text body (merged by the caller). Unknown parameters are treated as axes (case-insensitive tag).
 */
export function parse(output: Output, params: URLSearchParams, family?: string): Parsed {
  const need: Need = { output };
  if (family) need.family = family;
  const get = (k: string) => params.get(k) ?? undefined;
  const style = get("style");
  if (style !== undefined) {
    if (!(STYLES as readonly string[]).includes(style)) throw new GrammarError(`style: "${style}" is not one of ${STYLES.join(", ")}`);
    need.style = style as Style;
  }
  const set = get("set");
  if (set !== undefined) {
    if (set !== "material" && set !== "aphrody") throw new GrammarError(`set: "${set}" is not material or aphrody`);
    need.set = set;
  }
  const names = [...list(get("icons")), ...list(get("name"))];
  if (names.length) {
    const pattern = need.set === "aphrody" ? APHRODY_NAME : NAME;
    for (const n of names) if (n.length > LIMITS.nameLength || !pattern.test(n)) throw new GrammarError(`icons: "${n}" is not a valid icon name`);
    need.icons = uniqueSorted(names);
    if (need.icons.length > LIMITS.icons) throw new GrammarError(`icons: at most ${LIMITS.icons} per request, got ${need.icons.length}; use the full font or sprite instead`);
  }
  const text = get("text");
  const codepoints = new Set<number>();
  if (text) {
    if (text.length > LIMITS.text) throw new GrammarError(`text: at most ${LIMITS.text} characters`);
    for (const ch of text) codepoints.add(ch.codePointAt(0)!);
  }
  const unicodes = get("unicodes");
  if (unicodes) for (const c of parseUnicodes(unicodes)) codepoints.add(c);
  if (codepoints.size) need.unicodes = [...codepoints].sort((a, b) => a - b);
  const subsets = list(get("subsets"));
  if (subsets.length) need.subsets = uniqueSorted(subsets);
  const axes: Record<string, Range> = {};
  for (const [key, value] of params) {
    const k = key.toLowerCase();
    if (RESERVED.has(k)) continue;
    if (!/^[a-z]{4}$/.test(k)) throw new GrammarError(`unknown parameter "${key}"`);
    axes[k] = parseRange(value, k);
  }
  if (Object.keys(axes).length) need.axes = axes;
  const axesMode = get("axes");
  if (axesMode !== undefined) {
    if (axesMode !== "all" && axesMode !== "default") throw new GrammarError(`axes: "${axesMode}" is not default or all`);
    need.allAxes = axesMode === "all";
  }
  const features = list(get("features"));
  if (features.length) {
    for (const f of features) if (f !== "*" && !FEATURE.test(f)) throw new GrammarError(`features: "${f}" is not a four-character tag`);
    need.features = uniqueSorted(features);
  }
  const ids = get("ids");
  if (ids !== undefined) {
    if (ids !== "suffix" && ids !== "of") throw new GrammarError(`ids: "${ids}" is not suffix or of`);
    need.ids = ids;
  }
  const cssFamily = get("family");
  if (cssFamily !== undefined) {
    if (!CSS_IDENT.test(cssFamily)) throw new GrammarError(`family: "${cssFamily}" is not a valid font-family name`);
    need.cssFamily = cssFamily;
  }
  const cssClass = get("class");
  if (cssClass !== undefined) {
    if (!/^[A-Za-z][A-Za-z0-9_-]{0,31}$/.test(cssClass)) throw new GrammarError(`class: "${cssClass}" is not a valid CSS class prefix`);
    need.cssClass = cssClass;
  }
  const display = get("display");
  if (display !== undefined) {
    if (!DISPLAY.has(display)) throw new GrammarError(`display: "${display}" is not one of ${[...DISPLAY].join(", ")}`);
    need.display = display;
  }
  const size = get("size");
  if (size !== undefined) {
    need.size = num(size, "size");
    if (need.size <= 0 || need.size > 4096) throw new GrammarError("size: must be in 1..4096");
  }
  const color = get("color");
  if (color !== undefined) {
    if (!COLOR.test(color)) throw new GrammarError(`color: "${color}" is not a hex or named color`);
    need.color = color;
  }
  const v = get("v");
  if (v !== undefined) {
    if (!/^[0-9a-f]{6,64}$/.test(v)) throw new GrammarError(`v: "${v}" is not a revision`);
    need.v = v;
  }
  return { need, canonical: canonicalQuery(need) };
}

/** Canonical query string of a need (without `v`): fixed key order, merged lists, trimmed numbers. */
export function canonicalQuery(need: Need): string {
  const q: [string, string][] = [];
  if (need.allAxes) q.push(["axes", "all"]);
  if (need.cssClass) q.push(["class", need.cssClass]);
  if (need.color) q.push(["color", need.color]);
  if (need.display) q.push(["display", need.display]);
  if (need.cssFamily) q.push(["family", need.cssFamily]);
  if (need.features) q.push(["features", need.features.join(",")]);
  if (need.icons) q.push(["icons", need.icons.join(",")]);
  if (need.ids) q.push(["ids", need.ids]);
  if (need.set && need.set !== "material") q.push(["set", need.set]);
  if (need.size !== undefined) q.push(["size", fmt(need.size)]);
  if (need.style) q.push(["style", need.style]);
  if (need.subsets) q.push(["subsets", need.subsets.join(",")]);
  if (need.unicodes) q.push(["unicodes", formatUnicodes(need.unicodes, "")]);
  for (const k of Object.keys(need.axes ?? {}).sort()) {
    const [a, b] = need.axes![k]!;
    q.push([k, a === b ? fmt(a) : `${fmt(a)}..${fmt(b)}`]);
  }
  q.sort((x, y) => (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0));
  return q.map(([k, v]) => `${k}=${encodeURIComponent(v).replace(/%2C/g, ",").replace(/%2F/g, "/")}`).join("&");
}

/** Path of an output relative to the base (`/v1`). */
export function pathOf(need: Need): string {
  switch (need.output) {
    case "sprite":
      return "icons.svg";
    case "json":
      return "icons.json";
    case "icon":
      return "icon.svg";
    case "icons-font":
      return "icons.woff2";
    case "icons-css":
      return "icons.css";
    case "font":
      return `fonts/${need.family}.woff2`;
    case "font-css":
      return `fonts/${need.family}.css`;
  }
}

/** Reads the output (and family) back from a request path relative to `/v1/`, e.g. `icons.svg`, `fonts/roboto-flex.css`. */
export function outputOf(path: string): { output: Output; family?: string } | undefined {
  switch (path) {
    case "icons.svg":
      return { output: "sprite" };
    case "icons.json":
      return { output: "json" };
    case "icon.svg":
      return { output: "icon" };
    case "icons.woff2":
      return { output: "icons-font" };
    case "icons.css":
      return { output: "icons-css" };
  }
  const m = /^fonts\/([a-z0-9-]+)\.(woff2|css)$/.exec(path);
  return m ? { output: m[2] === "woff2" ? "font" : "font-css", family: m[1] } : undefined;
}

/** The need reduced to what changes the bytes of its output: CSS-only, id-only and size-only parameters are dropped for outputs they do not affect. */
export function identity(need: Need): Need {
  const { cssFamily, cssClass, display, ids, size, color, v: _v, ...rest } = need;
  const keep: Need = { ...rest };
  switch (need.output) {
    case "icons-css":
      Object.assign(keep, { cssFamily, cssClass, display });
      break;
    case "font-css":
      Object.assign(keep, { cssFamily, display });
      break;
    case "sprite":
      Object.assign(keep, { ids });
      break;
    case "icon":
      Object.assign(keep, { size, color });
      break;
  }
  for (const k of Object.keys(keep) as (keyof Need)[]) if (keep[k] === undefined) delete keep[k];
  return keep;
}
