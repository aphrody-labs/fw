// meta.json and the per-style symbol tables, read from R2 and kept in isolate memory (read-only, refreshed after TTL).
export interface Axis {
  tag: string;
  min: number;
  default: number;
  max: number;
}
export interface Family {
  family: string;
  kind: "symbols" | "text";
  style: "normal" | "italic" | "outlined" | "rounded" | "sharp";
  src: string;
  srcBytes: number;
  axes: Axis[];
  glyphs: number;
  /** Merged codepoint ranges of the font's cmap (text families). */
  cmap?: number[][];
  icons?: number;
  symbols?: string;
  woff2?: string;
}
export interface Meta {
  rev: string;
  version: string;
  viewBox: string;
  styles: string[];
  weights: number[];
  sources: unknown;
  families: Record<string, Family>;
  ranges: Record<string, string>;
}
export interface Symbols {
  names: string[];
  cp: number[];
  fill: number[];
  index: Map<string, number>;
}

const TTL_MS = 5 * 60_000;
let metaCache: { at: number; value: Meta } | undefined;
const symbolsCache = new Map<string, { at: number; value: Symbols }>();

async function readJson<T>(env: Env, key: string): Promise<T> {
  const object = await env.MATERIAL.get(key);
  if (!object) throw new Error(`R2 object ${key} is missing: the CDN tree has not been published`);
  return object.json<T>();
}

export async function loadMeta(env: Env): Promise<Meta> {
  if (metaCache && Date.now() - metaCache.at < TTL_MS) return metaCache.value;
  const value = await readJson<Meta>(env, "v1/meta.json");
  metaCache = { at: Date.now(), value };
  return value;
}

export async function loadSymbols(env: Env, meta: Meta, style: string): Promise<Symbols> {
  const family = meta.families[`material-symbols-${style}`];
  if (!family?.symbols) throw new Error(`no Material Symbols data for style ${style}`);
  const hit = symbolsCache.get(family.symbols);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.value;
  const raw = await readJson<{ names: string[]; cp: number[]; fill: number[] }>(env, `v1/${family.symbols}`);
  const value: Symbols = { ...raw, index: new Map(raw.names.map((name, i) => [name, i])) };
  symbolsCache.set(family.symbols, { at: Date.now(), value });
  return value;
}
