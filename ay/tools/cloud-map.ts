#!/usr/bin/env bun
// Validate docs/research/bun-cloudflare.toml (ADR-0010) against its two live sources: the docs of the Bun checkout
// (ay.toml [repos].bun) and the Cloudflare product catalog (docs/research/cloudflare.toml). Prints the Bun doc pages
// no feature covers, which is the plugin's backlog.
//   bun tools/cloud-map.ts [--summary]   exit 1 on a problem
import { readAy } from "./local.ts";
import { existsSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");

export const PARITY = ["native", "adapter", "partial", "container", "none"] as const;
export type Feature = {
  id: string;
  bun: string;
  /** Further doc pages the same feature covers. */
  also?: string[];
  names: string[];
  cloudflare: string[];
  binding: string;
  parity: (typeof PARITY)[number];
  plugin: string;
  proof?: string;
  source?: string;
};
export type Report = { features: number; sourced: number; parity: Record<string, number>; problems: string[]; uncovered: string[]; docs: number | null };

const toml = async (path: string) => Bun.TOML.parse(await Bun.file(path).text()) as Record<string, any>;

const pages = (docs: string): string[] => {
  const out: string[] = [];
  const walk = (dir: string, rel: string) => {
    for (const e of readdirSync(join(docs, dir), { withFileTypes: true })) {
      const next = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) walk(join(dir, e.name), next);
      else if (e.name.endsWith(".mdx")) out.push(next.slice(0, -4));
    }
  };
  for (const section of ["runtime", "bundler", "pm", "test"]) if (existsSync(join(docs, section))) walk(section, section);
  return out.sort();
};

export async function validate(rootDir = root, bunCheckout?: string): Promise<Report> {
  const map = await toml(join(rootDir, "docs/research/bun-cloudflare.toml"));
  const catalog = await toml(join(rootDir, map.catalog ?? "docs/research/cloudflare.toml"));
  const products = new Set<string>((catalog.product as { name: string }[]).map((p) => p.name));
  const features = (map.feature ?? []) as Feature[];
  const problems: string[] = [];

  const seen = new Set<string>();
  for (const f of features) {
    if (seen.has(f.id)) problems.push(`${f.id}: duplicate id`);
    seen.add(f.id);
    if (!PARITY.includes(f.parity)) problems.push(`${f.id}: parity "${f.parity}" is not one of ${PARITY.join(", ")}`);
    if (!f.cloudflare?.length) problems.push(`${f.id}: no Cloudflare product`);
    for (const name of f.cloudflare ?? []) if (!products.has(name)) problems.push(`${f.id}: "${name}" is not a product of the catalog`);
    if (!f.names?.length || !f.binding || !f.plugin) problems.push(`${f.id}: names, binding and plugin are required`);
  }
  for (const e of (map.extra ?? []) as { cloudflare: string }[]) if (!products.has(e.cloudflare)) problems.push(`extra: "${e.cloudflare}" is not a product of the catalog`);

  let uncovered: string[] = [];
  let docs: number | null = null;
  if (!bunCheckout) {
    const hint = (readAy(rootDir) as Record<string, any>).repos?.bun?.checkout as string | undefined;
    if (hint) bunCheckout = resolve(rootDir, hint);
  }
  const docsDir = bunCheckout ? join(bunCheckout, map.bun_docs ?? "docs") : undefined;
  if (docsDir && existsSync(docsDir)) {
    const all = pages(docsDir);
    docs = all.length;
    const known = new Set(all);
    for (const f of features) for (const page of [f.bun, ...(f.also ?? [])]) if (!known.has(page)) problems.push(`${f.id}: "${page}" is not a page of the Bun docs`);
    const covered = new Set(features.flatMap((f) => [f.bun, ...(f.also ?? [])]));
    uncovered = all.filter((p) => !covered.has(p));
  }

  const parity: Record<string, number> = {};
  for (const f of features) parity[f.parity] = (parity[f.parity] ?? 0) + 1;
  return { features: features.length, sourced: features.filter((f) => f.source).length, parity, problems, uncovered, docs };
}

if (import.meta.main) {
  const report = await validate();
  console.log(`${report.features} features (${report.sourced} with a docs source, ${report.features - report.sourced} unverified) ${JSON.stringify(report.parity)}; Bun docs read: ${report.docs ?? "checkout absent, skipped"}`);
  if (process.argv.includes("--summary")) console.log(`pages without a feature (${report.uncovered.length}): ${report.uncovered.join(", ")}`);
  for (const p of report.problems) console.error(p);
  process.exit(report.problems.length ? 1 : 0);
}
