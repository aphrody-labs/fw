#!/usr/bin/env bun
// Live inventory of the Aphrody estate with its evidence: GitHub organizations and repositories, the Cloudflare
// account (zones, DNS record types, Workers, queues, R2 buckets, tunnels), the npm scope and crates.io owner of
// ay.toml [registries], agent skills and MCP servers. Every number comes from a command run now; the output records
// the command next to each section so a reader can rerun it. Identifiers, emails and account names are not written.
//   bun tools/org-inventory.ts [--out <file> (default .private/org-inventory.toml)] [--summary]
// Needs `gh` (authenticated) and `cf` (authenticated); a missing source is reported, never invented.
import { privatePath } from "./local.ts";
import { readAy } from "./local.ts";
import { existsSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");

type Json = any;
const run = async (cmd: string[]): Promise<string | undefined> => {
  try {
    const p = Bun.spawn(cmd, { stdout: "pipe", stderr: "ignore" });
    const [out, code] = await Promise.all([new Response(p.stdout).text(), p.exited]);
    return code === 0 ? out : undefined;
  } catch {
    return undefined;
  }
};
const json = async (cmd: string[]): Promise<Json | undefined> => {
  const out = await run(cmd);
  if (!out) return undefined;
  // `cf` prints a `result_info:` line before the JSON body.
  const start = out.search(/^[[{]/m);
  try {
    return JSON.parse(start < 0 ? out : out.slice(start));
  } catch {
    return undefined;
  }
};

const tally = <T>(items: T[], key: (item: T) => string): Record<string, number> => {
  const out: Record<string, number> = {};
  for (const item of items) out[key(item)] = (out[key(item)] ?? 0) + 1;
  return Object.fromEntries(Object.entries(out).sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])));
};

export async function github(owners: string[]) {
  const orgs: Record<string, Json> = {};
  for (const owner of owners) {
    const repos = (await json(["gh", "repo", "list", owner, "--limit", "5000", "--json", "name,isFork,isArchived,isPrivate,primaryLanguage,pushedAt,description"])) as Json[] | undefined;
    if (!repos) {
      orgs[owner] = { error: "gh repo list failed" };
      continue;
    }
    const originals = repos.filter((r) => !r.isFork);
    orgs[owner] = {
      repos: repos.length,
      forks: repos.filter((r) => r.isFork).length,
      archived: repos.filter((r) => r.isArchived).length,
      private: repos.filter((r) => r.isPrivate).length,
      languages: tally(repos, (r) => r.primaryLanguage?.name ?? "none"),
      originals: originals.map((r) => ({ name: r.name, archived: r.isArchived, language: r.primaryLanguage?.name ?? "none", pushed: String(r.pushedAt).slice(0, 10) })),
    };
  }
  return orgs;
}

export async function cloudflare() {
  const zones = (await json(["cf", "zones", "list"])) as Json[] | undefined;
  if (!zones) return { error: "cf zones list failed" };
  const out: Json = { zones: [] as Json[] };
  for (const z of zones) {
    const records = ((await json(["cf", "dns", "records", "list", "-z", z.name])) as Json[] | undefined) ?? [];
    out.zones.push({ name: z.name, status: z.status, plan: z.plan?.name, records: records.length, types: tally(records, (r) => r.type), proxied: records.filter((r) => r.proxied).length, tunnelled: records.filter((r) => String(r.content).endsWith(".cfargotunnel.com")).length });
  }
  const workers = ((await json(["cf", "workers", "list", "--per-page", "100"])) as Json[] | undefined) ?? [];
  out.workers = workers.map((w) => ({ name: w.name, deployed: String(w.deployed_on).slice(0, 10), observability: !!w.observability?.enabled, workers_dev: !!w.subdomain?.enabled })).sort((a, b) => a.name.localeCompare(b.name));
  const queues = ((await json(["cf", "queues", "list"])) as Json[] | undefined) ?? [];
  out.queues = queues.map((q) => ({ name: q.queue_name, producers: q.producers_total_count, consumers: q.consumers_total_count }));
  const r2 = (await json(["cf", "r2", "buckets", "list"])) as Json | undefined;
  out.r2 = (r2?.buckets ?? []).map((b: Json) => b.name);
  const tunnels = ((await json(["cf", "tunnels", "list"])) as Json[] | undefined) ?? [];
  out.tunnels = tunnels.map((t) => ({ name: t.name, status: t.status }));
  out.d1 = (((await json(["cf", "d1", "list"])) as Json[] | undefined) ?? []).length;
  out.kv = (((await json(["cf", "kv", "namespaces", "list"])) as Json[] | undefined) ?? []).length;
  out.pages = (((await json(["cf", "pages", "list"])) as Json[] | undefined) ?? []).length;
  return out;
}

export async function registries(npmScope: string, cratesPrefix: string, cratesOwner: string) {
  const names = new Set<string>();
  for (let from = 0; from < 1000; from += 100) {
    const page = await json(["curl", "-s", `https://registry.npmjs.org/-/v1/search?text=${encodeURIComponent(npmScope)}&size=100&from=${from}`]);
    const found = (page?.objects ?? []).map((o: Json) => o.package.name as string);
    if (!found.length) break;
    for (const n of found) if (n.startsWith(`${npmScope}/`)) names.add(n);
  }
  const ua = ["-s", "-A", "aphrody-inventory"];
  const user = await json(["curl", ...ua, `https://crates.io/api/v1/users/${encodeURIComponent(cratesOwner)}`]);
  const listed = user?.user?.id ? await json(["curl", ...ua, `https://crates.io/api/v1/crates?user_id=${user.user.id}&per_page=1`]) : undefined;
  return { npm: { scope: npmScope, packages: names.size }, crates: { prefix: cratesPrefix, owner: user?.user?.login ?? null, published: listed?.meta?.total ?? null } };
}

export function agent() {
  const skills = join(homedir(), ".claude", "packages", "aphrody", "skills");
  return { skills: existsSync(skills) ? readdirSync(skills).length : null };
}

export const toml = (value: Json, path: string[] = []): string => {
  const scalar = (v: Json) => (typeof v === "string" ? JSON.stringify(v) : String(v));
  const lines: string[] = [];
  const tables: [string[], Json][] = [];
  const arrays: [string[], Json[]][] = [];
  for (const [k, v] of Object.entries(value)) {
    const key = /^[A-Za-z0-9_-]+$/.test(k) ? k : JSON.stringify(k);
    if (v === null || v === undefined) continue;
    if (Array.isArray(v)) {
      if (v.every((x) => typeof x !== "object")) lines.push(`${key} = [${v.map(scalar).join(", ")}]`);
      else arrays.push([[...path, key], v]);
    } else if (typeof v === "object") tables.push([[...path, key], v]);
    else lines.push(`${key} = ${scalar(v)}`);
  }
  let out = lines.join("\n") + (lines.length ? "\n" : "");
  for (const [p, v] of tables) out += `\n[${p.join(".")}]\n${toml(v, p)}`;
  for (const [p, items] of arrays) for (const item of items) out += `\n[[${p.join(".")}]]\n${toml(item, p)}`;
  return out;
};

if (import.meta.main) {
  const cfg = readAy(root) as Json;
  const owners = [...new Set<string>([...Object.values<string>(cfg.orgs ?? {}), ...Object.values<Json>(cfg.repos ?? {}).map((r) => String(r.slug).split("/")[0]!)])].sort();
  const report = {
    version: 1,
    retrieved: new Date().toISOString().slice(0, 10),
    commands: { github: "gh repo list <owner> --limit 5000", cloudflare: "cf zones|dns records|workers|queues|r2 buckets|tunnels|d1|kv|pages list", npm: "registry.npmjs.org/-/v1/search", crates: "crates.io/api/v1" },
    github: await github(owners),
    cloudflare: await cloudflare(),
    registries: await registries(cfg.registries?.npm ?? "@aphrody", cfg.registries?.crates ?? "aphrody-", cfg.orgs?.dev ?? ""),
    agent: agent(),
  };
  const outIdx = process.argv.indexOf("--out");
  const text = `# Generated by tools/org-inventory.ts; rerun it, do not edit. Identifiers and account names are not recorded.\n${toml(report)}`;
  await Bun.write(outIdx > 0 ? resolve(root, process.argv[outIdx + 1]!) : privatePath("org-inventory.toml", root, true), text);
  if (process.argv.includes("--summary") || outIdx < 0) {
    for (const [o, v] of Object.entries<Json>(report.github)) console.log(`github ${o}: ${v.repos ?? v.error} repos, ${v.forks} forks, ${v.archived} archived`);
    const c = report.cloudflare as Json;
    console.log(`cloudflare: ${c.zones?.length} zones, ${c.workers?.length} workers, ${c.queues?.length} queues, ${c.r2?.length} r2 buckets, ${c.tunnels?.length} tunnels, ${c.d1} d1, ${c.kv} kv, ${c.pages} pages`);
    console.log(`npm ${report.registries.npm.scope}: ${report.registries.npm.packages} packages; crates.io ${report.registries.crates.owner}: ${report.registries.crates.published} crates; skills: ${report.agent.skills}`);
  }
}
