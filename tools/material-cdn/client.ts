// Typed client for material.aphrody.com: turns a list of needs into the URL that serves exactly that.
//
//   import { material } from "../fw/tools/material-cdn/client.ts";
//   const cdn = material();                                     // https://material.aphrody.com/v1
//   cdn.sprite({ style: "rounded", icons: ["home", "search"], fill: [0, 1], ids: "of" });   // site/build.ts style <symbol>s
//   cdn.iconFont({ style: "outlined", icons: ["home", "search"], axes: { fill: [0, 1], wght: 400 } });
//   cdn.fontCss({ family: "google-sans-flex", text: "Aphrody", axes: { wght: [400, 700] } });
//
// Every method returns a canonical URL (sorted parameters, merged lists), the same bytes the Worker hashes, so equal needs
// share one cache entry. `fetch*` helpers return the response body. Needs that do not fit a URL are POSTed with `post`.
import { EXTENSION, formatUnicodes, GrammarError, type IconSet, type Output, pathOf, parse, type Range, type Style } from "./grammar.ts";

export { GrammarError, STYLES, WEIGHTS, parse, canonicalQuery, formatUnicodes, parseUnicodes, outputOf, LIMITS } from "./grammar.ts";
export type { IconSet, Need, Output, Parsed, Range, Style } from "./grammar.ts";

export const DEFAULT_BASE = "https://material.aphrody.com/v1";

/** Axis values: a number pins, `[min, max]` limits to a range. Keys are axis tags, any case (`FILL`, `wght`, `GRAD`, `opsz`, `wdth`, ...). */
export type Axes = Record<string, number | Range>;

interface Common {
  /** Axes not listed are pinned to their default unless this is true. */
  allAxes?: boolean;
  axes?: Axes;
  /** Pin to a data revision (`meta.json` rev): the response is then immutable. */
  rev?: string;
}

export interface IconNeeds extends Common {
  style?: Style;
  set?: IconSet;
  icons: readonly string[];
  /** Static outputs (sprite, json, svg): which fills to include, 0 and/or 1. Same as `axes.fill`. */
  fill?: 0 | 1 | readonly (0 | 1)[];
  /** Static outputs: weight 100..700 in steps of 100. Same as `axes.wght`. */
  wght?: number;
  ids?: "suffix" | "of";
}
export interface IconFontNeeds extends Common {
  style?: Style;
  icons: readonly string[];
  cssFamily?: string;
  cssClass?: string;
  display?: "auto" | "block" | "swap" | "fallback" | "optional";
}
export interface FontNeeds extends Common {
  /** `google-sans-flex`, `google-sans-code`, `google-sans-code-italic`, `roboto-flex` or `material-symbols-<style>`. */
  family: string;
  /** Characters to keep. */
  text?: string;
  /** `20-7E,A0` (U+ prefixes allowed, URL-encode the +), or codepoints. */
  unicodes?: string | readonly number[];
  /** Named ranges from meta.json: latin, latin-ext, cyrillic, cyrillic-ext, greek, greek-ext, vietnamese, symbols. */
  subsets?: readonly string[];
  features?: readonly string[];
  cssFamily?: string;
  display?: "auto" | "block" | "swap" | "fallback" | "optional";
}
export interface SingleIconNeeds extends Common {
  style?: Style;
  set?: IconSet;
  icon: string;
  fill?: 0 | 1;
  wght?: number;
  /** Pixel width and height; omitted keeps the viewBox only. */
  size?: number;
  /** CSS color, default currentColor. */
  color?: string;
}

function toParams(input: Record<string, unknown>): URLSearchParams {
  const p = new URLSearchParams();
  const axes: Axes = { ...((input.axes as Axes | undefined) ?? {}) };
  if (input.fill !== undefined) {
    const fills = [input.fill].flat() as number[];
    axes.fill = Math.min(...fills) === Math.max(...fills) ? fills[0]! : [Math.min(...fills), Math.max(...fills)];
  }
  if (input.wght !== undefined) axes.wght = input.wght as number;
  for (const [k, v] of Object.entries(axes)) p.set(k.toLowerCase(), Array.isArray(v) ? `${v[0]}..${v[1]}` : String(v));
  const set = (k: string, v: unknown) => {
    if (v !== undefined && v !== null && v !== "") p.set(k, Array.isArray(v) ? v.join(",") : String(v));
  };
  set("style", input.style);
  set("set", input.set);
  set("icons", input.icons ?? input.icon);
  set("text", input.text);
  set("unicodes", Array.isArray(input.unicodes) ? formatUnicodes([...new Set(input.unicodes as number[])].sort((a, b) => a - b), "") : input.unicodes);
  set("subsets", input.subsets);
  set("features", input.features);
  set("ids", input.ids);
  set("family", input.cssFamily);
  set("class", input.cssClass);
  set("display", input.display);
  set("size", input.size);
  set("color", input.color);
  if (input.allAxes) p.set("axes", "all");
  set("v", input.rev);
  return p;
}

export function material(base: string = DEFAULT_BASE) {
  const root = base.replace(/\/$/, "");
  const url = (output: Output, input: Record<string, unknown>, family?: string): string => {
    const params = toParams(input);
    const rev = params.get("v") ?? undefined;
    params.delete("v");
    const { need, canonical } = parse(output, params, family);
    const q = rev ? `${canonical}${canonical ? "&" : ""}v=${rev}` : canonical;
    return `${root}/${pathOf(need)}${q ? `?${q}` : ""}`;
  };
  const get = async (u: string) => {
    const r = await fetch(u);
    if (!r.ok) throw new Error(`${u}: ${r.status} ${await r.text()}`);
    return r;
  };
  return {
    base: root,
    /** SVG sprite of `<symbol>`s: ids `home`, `home-fill` (or `o-home`, `f-home` with ids: "of"). */
    sprite: (n: IconNeeds) => url("sprite", { ...n }),
    /** JSON path list: `{ icons: { home: { "0": "<d>", "1": "<d>" } } }`. */
    json: (n: IconNeeds) => url("json", { ...n }),
    /** One SVG. */
    icon: (n: SingleIconNeeds) => url("icon", { ...n }),
    /** Ligature font (woff2) with exactly these icons and axes. */
    iconFont: (n: IconFontNeeds) => url("icons-font", { ...n }),
    /** `@font-face` plus classes for the icon font. */
    iconCss: (n: IconFontNeeds) => url("icons-css", { ...n }),
    /** woff2 of a text family subset by text, unicodes or subsets, with axes pinned or limited. */
    font: (n: FontNeeds) => url("font", { ...n }, n.family),
    /** `@font-face` rules (one per subset, with `unicode-range`) whose src URLs are pinned and immutable. */
    fontCss: (n: FontNeeds) => url("font-css", { ...n }, n.family),
    async fetchSprite(n: IconNeeds): Promise<string> {
      return (await get(this.sprite(n))).text();
    },
    async fetchFont(n: FontNeeds): Promise<Uint8Array> {
      return new Uint8Array(await (await get(this.font(n))).arrayBuffer());
    },
    /** POST for lists too long for a URL; the response carries `Content-Location: /v1/g/<hash>.<ext>`, immutable and GET-able. */
    async post(output: Exclude<Output, "font" | "font-css">, n: IconNeeds | IconFontNeeds): Promise<Response> {
      const { icons, ...rest } = n;
      const params = toParams({ ...rest });
      const { need, canonical } = parse(output, params, undefined);
      const res = await fetch(`${root}/${pathOf(need)}${canonical ? `?${canonical}` : ""}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ icons }) });
      if (!res.ok) throw new Error(`POST ${pathOf(need)}: ${res.status} ${await res.text()}`);
      return res;
    },
    extension: (output: Output) => EXTENSION[output],
  };
}

export type Material = ReturnType<typeof material>;
export const isGrammarError = (e: unknown): e is GrammarError => e instanceof GrammarError;
