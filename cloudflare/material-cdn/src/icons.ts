// Static icon outputs: SVG sprite, JSON path list and single SVG, assembled from the per-icon SVGs in R2 (`icons/<style>/[w<wght>/]<name>[-fill].svg`
// for Material Symbols, `aphrody/icons/<name>.svg` for our own set). Weights come in steps of 100; the static SVGs exist for opsz 24, GRAD 0 only.
import { GrammarError, type Need, STYLES, WEIGHTS } from "../../../tools/material-cdn/grammar.ts";
import { loadSymbols, type Meta } from "./meta.ts";

const PATH = /<path d="([^"]+)"\/>/;
const CONCURRENCY = 6;

export interface StaticRequest {
  style: string;
  wght: number;
  fills: (0 | 1)[];
}

/** Resolves the static-output view of the axes: wght in steps of 100, fill 0, 1 or both, opsz 24 and grad 0 only. */
export function staticRequest(need: Need): StaticRequest {
  const axes = need.axes ?? {};
  for (const [axis, [lo, hi]] of Object.entries(axes)) {
    if (axis === "fill" || axis === "wght") continue;
    if (!((axis === "opsz" && lo === 24 && hi === 24) || (axis === "grad" && lo === 0 && hi === 0))) {
      throw new GrammarError(`axis ${axis}: static SVGs exist for opsz 24 and GRAD 0 only; use icons.woff2 for other axes`);
    }
  }
  const wght = axes.wght ? axes.wght : ([400, 400] as const);
  if (wght[0] !== wght[1] || !(WEIGHTS as readonly number[]).includes(wght[0])) throw new GrammarError(`axis wght: static SVGs come in one weight of ${WEIGHTS.join(", ")} per request, got ${wght[0]}..${wght[1]}`);
  const fill = axes.fill ?? [0, 0];
  if (fill[0] < 0 || fill[1] > 1) throw new GrammarError("axis fill: must be within 0..1");
  const fills: (0 | 1)[] = fill[0] === fill[1] ? [fill[0] >= 0.5 ? 1 : 0] : [0, 1];
  return { style: need.style ?? "outlined", wght: wght[0], fills };
}

async function pool<T, R>(items: readonly T[], fn: (item: T) => Promise<R>): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;
  await Promise.all(
    Array.from({ length: Math.min(CONCURRENCY, items.length) }, async () => {
      while (next < items.length) {
        const i = next++;
        results[i] = await fn(items[i]!);
      }
    }),
  );
  return results;
}

async function readText(env: Env, key: string): Promise<string | undefined> {
  const object = await env.MATERIAL.get(key);
  return object ? object.text() : undefined;
}

export interface IconPaths {
  name: string;
  /** fill -> path data (Material) */
  paths: Partial<Record<0 | 1, string>>;
}

async function materialPaths(env: Env, meta: Meta, need: Need): Promise<{ request: StaticRequest; icons: IconPaths[] }> {
  const request = staticRequest(need);
  if (!(STYLES as readonly string[]).includes(request.style)) throw new GrammarError(`style ${request.style}`);
  const symbols = await loadSymbols(env, meta, request.style);
  const names = need.icons ?? [];
  const unknown = names.filter((n) => !symbols.index.has(n));
  if (unknown.length) throw new GrammarError(`unknown Material Symbols in ${request.style}`, { unknown });
  const dir = request.wght === 400 ? "" : `w${request.wght}/`;
  const icons = await pool(names, async (name) => {
    const paths: IconPaths["paths"] = {};
    for (const fill of request.fills) {
      const svg = await readText(env, `v1/icons/${request.style}/${dir}${name}${fill ? "-fill" : ""}.svg`);
      const d = svg && PATH.exec(svg)?.[1];
      if (d) paths[fill] = d;
    }
    return { name, paths };
  });
  const missing = icons.flatMap((i) => request.fills.filter((f) => !i.paths[f]).map((f) => `${i.name}${f ? "-fill" : ""}`));
  if (missing.length) throw new GrammarError(`no static SVG for these icons at wght ${request.wght} in ${request.style}`, { missing });
  return { request, icons };
}

const symbolId = (name: string, fill: 0 | 1, ids: "suffix" | "of") => (ids === "of" ? `${fill ? "f" : "o"}-${name}` : fill ? `${name}-fill` : name);

/** Inner markup and viewBox of one of our own SVGs. */
function aphrodySymbol(svg: string): { viewBox: string; body: string } {
  const open = /<svg\b([^>]*)>/.exec(svg);
  if (!open) throw new GrammarError("not an SVG");
  const viewBox = /viewBox="([^"]+)"/.exec(open[1]!)?.[1] ?? "0 0 24 24";
  const body = svg.slice(open.index + open[0].length, svg.lastIndexOf("</svg>")).trim();
  return { viewBox, body };
}

async function aphrodySymbolsOf(env: Env, need: Need): Promise<{ name: string; viewBox: string; body: string }[]> {
  const names = need.icons ?? [];
  const found = await pool(names, async (name) => {
    const svg = await readText(env, `v1/aphrody/icons/${name}.svg`);
    return svg ? { name, ...aphrodySymbol(svg) } : undefined;
  });
  const missing = names.filter((_, i) => !found[i]);
  if (missing.length) throw new GrammarError("unknown Aphrody icons", { unknown: missing });
  return found as { name: string; viewBox: string; body: string }[];
}

export async function sprite(env: Env, meta: Meta, need: Need): Promise<string> {
  if (!need.icons?.length) throw new GrammarError("icons: at least one icon name is required");
  let body = '<svg xmlns="http://www.w3.org/2000/svg">';
  if (need.set === "aphrody") {
    for (const s of await aphrodySymbolsOf(env, need)) body += `<symbol id="${s.name.replaceAll("/", "-")}" viewBox="${s.viewBox}">${s.body}</symbol>`;
  } else {
    const { request, icons } = await materialPaths(env, meta, need);
    for (const icon of icons) for (const fill of request.fills) body += `<symbol id="${symbolId(icon.name, fill, need.ids ?? "suffix")}" viewBox="${meta.viewBox}"><path d="${icon.paths[fill]}"/></symbol>`;
  }
  return body + "</svg>";
}

export async function pathList(env: Env, meta: Meta, need: Need): Promise<string> {
  if (!need.icons?.length) throw new GrammarError("icons: at least one icon name is required");
  if (need.set === "aphrody") {
    const symbols = await aphrodySymbolsOf(env, need);
    return JSON.stringify({ rev: meta.rev, set: "aphrody", icons: Object.fromEntries(symbols.map((s) => [s.name, { viewBox: s.viewBox, body: s.body }])) });
  }
  const { request, icons } = await materialPaths(env, meta, need);
  const list = Object.fromEntries(icons.map((i) => [i.name, Object.fromEntries(request.fills.map((f) => [String(f), i.paths[f]]))]));
  return JSON.stringify({ rev: meta.rev, style: request.style, wght: request.wght, viewBox: meta.viewBox, icons: list });
}

export async function single(env: Env, meta: Meta, need: Need): Promise<string> {
  if (need.icons?.length !== 1) throw new GrammarError("icon.svg needs exactly one icon (name= or icons=)");
  const size = need.size === undefined ? "" : ` width="${need.size}" height="${need.size}"`;
  const color = need.color ?? "currentColor";
  if (need.set === "aphrody") {
    const [s] = await aphrodySymbolsOf(env, need);
    return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${s!.viewBox}"${size}>${s!.body}</svg>`;
  }
  const request = staticRequest(need);
  if (request.fills.length !== 1) throw new GrammarError("icon.svg: fill must be 0 or 1");
  const { icons } = await materialPaths(env, meta, need);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${meta.viewBox}"${size} fill="${color}"><path d="${icons[0]!.paths[request.fills[0]!]}"/></svg>`;
}
