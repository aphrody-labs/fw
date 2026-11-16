#!/usr/bin/env bun
// Recon of a company's public surface: GitHub org, websites (sitemaps, recursively), OpenAPI specs; optional mirror.
//   bun tools/recon.ts <name> --org <github-org> --site <https://host/> [--site ...] [--openapi <url>]... [--out <dir>]
//   bun tools/recon.ts <name> ... --mirror [--concurrency 2]   also download every sitemap page (HTML) under <out>/mirror
//   --sitemap <url> (repeatable) takes one sitemap file instead of a site root, to mirror only part of a site
// Writes <out>/<name>.json (raw facts) and prints a summary; docs/research/<name>.toml is written from it by hand
// review or by --toml <path>.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { parseArgs } from "node:util";

const UA = "Mozilla/5.0 (X11; Linux x86_64) AphrodyRecon/1.0";

export function locs(xml: string): { sitemaps: string[]; pages: string[] } {
  const all = [...xml.matchAll(/<loc>\s*([^<\s]+)\s*<\/loc>/g)].map(m => m[1]!.replaceAll("&amp;", "&"));
  const isIndex = /<sitemapindex[\s>]/.test(xml);
  return isIndex ? { sitemaps: all, pages: [] } : { sitemaps: [], pages: all };
}

/** First path segment, or "/" for the home page: the section a page belongs to. */
export function section(url: string): string {
  const seg = new URL(url).pathname.split("/").filter(Boolean)[0];
  return seg ? `/${seg}` : "/";
}

async function get(url: string): Promise<{ status: number; body: string }> {
  const r = await fetch(url, { headers: { "user-agent": UA }, redirect: "follow" });
  return { status: r.status, body: await r.text() };
}

/** Sitemaps a site declares: `Sitemap:` lines of robots.txt and `<link rel="sitemap">` of the home page. */
export function declared(base: string, robots: string, home: string): string[] {
  const out = [...robots.matchAll(/^\s*sitemap:\s*(\S+)/gim)].map(m => m[1]!);
  for (const m of home.matchAll(/<link\b[^>]*>/gi)) {
    const tag = m[0];
    if (/rel=["']?sitemap/i.test(tag)) {
      const href = tag.match(/href=["']?([^"'\s>]+)/i)?.[1];
      if (href) out.push(new URL(href, base).href);
    }
  }
  return [...new Set(out)];
}

/** True when a body is a sitemap document, not an HTML page served with 200 (a soft 404). */
export const isSitemap = (body: string) => /<(urlset|sitemapindex)[\s>]/.test(body);

/** Every sitemap of a site: the declared ones, else /sitemap.xml, /sitemap-index.xml and /sitemap_index.xml. */
async function siteSitemaps(site: string) {
  const [robots, home] = await Promise.all([get(new URL("/robots.txt", site).href), get(site)].map(p => p.catch(() => ({ status: 0, body: "" }))));
  const roots = declared(site, robots!.status === 200 ? robots!.body : "", home!.body);
  const candidates = roots.length ? roots : ["/sitemap.xml", "/sitemap-index.xml", "/sitemap_index.xml"].map(p => new URL(p, site).href);
  const seen = new Set<string>();
  const files: string[] = [], pages: string[] = [];
  for (const c of candidates) {
    const r = await sitemap(c, seen);
    files.push(...r.files);
    pages.push(...r.pages);
    if (!roots.length && r.pages.length) break;
  }
  return { files, pages: [...new Set(pages)] };
}

async function sitemap(root: string, seen = new Set<string>()): Promise<{ files: string[]; pages: string[] }> {
  if (seen.has(root)) return { files: [], pages: [] };
  seen.add(root);
  const { status, body } = await get(root).catch(() => ({ status: 0, body: "" }));
  if (status !== 200) return { files: [`${root} (${status})`], pages: [] };
  if (!isSitemap(body)) return { files: [`${root} (not a sitemap)`], pages: [] };
  const { sitemaps, pages } = locs(body);
  const nested = await Promise.all(sitemaps.map(s => sitemap(s, seen)));
  return { files: [root, ...nested.flatMap(n => n.files)], pages: [...pages, ...nested.flatMap(n => n.pages)] };
}

async function org(name: string) {
  const out: { name: string; stars: number; language: string | null; pushed: string; archived: boolean; fork: boolean; description: string }[] = [];
  for (let page = 1; ; page++) {
    const p = Bun.spawn(["gh", "api", `orgs/${name}/repos?per_page=100&page=${page}`], { stdout: "pipe", stderr: "pipe" });
    const rows = JSON.parse(await new Response(p.stdout).text()) as any[];
    if (!rows.length) break;
    for (const r of rows)
      out.push({ name: r.name, stars: r.stargazers_count, language: r.language, pushed: r.pushed_at, archived: r.archived, fork: r.fork, description: r.description ?? "" });
  }
  return out.sort((a, b) => b.stars - a.stars);
}

async function openapi(url: string) {
  const { status, body } = await get(url);
  if (status !== 200) return { url, status };
  const spec = JSON.parse(body);
  const ops: { method: string; path: string; tag: string; summary: string }[] = [];
  for (const [path, item] of Object.entries<any>(spec.paths ?? {}))
    for (const [method, op] of Object.entries<any>(item))
      if (["get", "post", "put", "patch", "delete"].includes(method))
        ops.push({ method: method.toUpperCase(), path, tag: op.tags?.[0] ?? "untagged", summary: op.summary ?? "" });
  return { url, status, title: spec.info?.title, version: spec.info?.version, operations: ops };
}

async function mirror(pages: string[], dir: string, concurrency: number) {
  let i = 0, ok = 0, failed = 0;
  const worker = async () => {
    while (i < pages.length) {
      const url = pages[i++]!;
      const u = new URL(url);
      const file = join(dir, u.host, u.pathname.endsWith("/") ? `${u.pathname}index.html` : `${u.pathname}.html`);
      try {
        const { status, body } = await get(url);
        if (status === 200) {
          mkdirSync(dirname(file), { recursive: true });
          writeFileSync(file, body);
          ok++;
        } else failed++;
      } catch {
        failed++;
      }
      await Bun.sleep(300);
    }
  };
  await Promise.all(Array.from({ length: concurrency }, worker));
  return { ok, failed };
}

if (import.meta.main) {
  const { values, positionals } = parseArgs({
    allowPositionals: true,
    options: {
      org: { type: "string" },
      site: { type: "string", multiple: true, default: [] },
      sitemap: { type: "string", multiple: true, default: [] }, // explicit sitemap files (e.g. only the content part of a site)
      openapi: { type: "string", multiple: true, default: [] },
      out: { type: "string", default: "C:/data/recon" },
      mirror: { type: "boolean", default: false },
      concurrency: { type: "string", default: "2" },
    },
  });
  const name = positionals[0] ?? "recon";
  const out = join(values.out!, name);
  mkdirSync(out, { recursive: true });
  const [repos, sites, apis] = await Promise.all([
    values.org ? org(values.org) : Promise.resolve([]),
    Promise.all([
      ...values.site!.map(async s => ({ site: s, ...(await siteSitemaps(s)) })),
      ...values.sitemap!.map(async s => ({ site: s, ...(await sitemap(s)) })),
    ]),
    Promise.all(values.openapi!.map(openapi)),
  ]);
  const summary = {
    name,
    retrieved: new Date().toISOString().slice(0, 10),
    github: { org: values.org, repos: repos.length, active: repos.filter(r => !r.archived && !r.fork).length, top: repos.slice(0, 40) },
    sites: sites.map(s => {
      const bySection: Record<string, number> = {};
      for (const p of s.pages) bySection[section(p)] = (bySection[section(p)] ?? 0) + 1;
      return { site: s.site, sitemap_files: s.files, pages: s.pages.length, sections: bySection };
    }),
    apis: apis.map(a => ("operations" in a ? { ...a, operations_count: a.operations!.length } : a)),
  };
  writeFileSync(join(out, `${name}.json`), JSON.stringify({ ...summary, pages: sites.flatMap(s => s.pages) }, null, 2));
  console.log(JSON.stringify({ ...summary, github: { ...summary.github, top: summary.github.top.slice(0, 10).map(r => `${r.name} ★${r.stars}`) }, apis: summary.apis.map(a => ({ url: a.url, title: (a as any).title, version: (a as any).version, operations: (a as any).operations_count })) }, null, 2));
  if (values.mirror) {
    const res = await mirror(sites.flatMap(s => s.pages), join(out, "mirror"), Number(values.concurrency));
    console.log(`mirror: ${res.ok} saved, ${res.failed} failed → ${join(out, "mirror")}`);
  }
}
