#!/usr/bin/env bun
// Duplication audit across the first-party code of the Aphrody repositories (checkout hints of ay.local.toml [repos]). It measures, it
// never edits: duplicated crate and package families, dependency versions repeated inside a lockfile and spread across repositories,
// identical SQL statements in several files, the database libraries each repository uses, and crawler, mirror and index code. Output
// goes to the git-ignored private store (.private/dedup.json and dedup.md).
//   bun tools/dedup.ts            audit every repository that has a checkout hint
//   bun tools/dedup.ts <dir>...   audit these directories
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, join, relative, resolve } from "node:path";
import { privatePath, readAy, repoRoot, checkoutOf } from "./local.ts";

const EXCLUDE = /(^|\/)(node_modules|target|vendor|third_party|dist|build|generated|coverage|fixtures?|testdata|legacy|archive|archived|\.git|\.cache)(\/|$)/;
const SOURCE = /\.(rs|ts|tsx|js|mjs|py|go|sql|c|h|cpp)$/;
const MAX_BYTES = 400_000;

export type Repo = { name: string; dir: string };

export function listFiles(dir: string): string[] {
  const proc = Bun.spawnSync(["git", "-C", dir, "ls-files", "-z"], { stdout: "pipe", stderr: "ignore" });
  const names = proc.exitCode === 0 ? proc.stdout.toString().split("\0").filter(Boolean) : walk(dir, dir);
  return names.filter((n) => !EXCLUDE.test(n));
}

function walk(root: string, dir: string, out: string[] = []): string[] {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    const rel = relative(root, p).replaceAll("\\", "/");
    if (EXCLUDE.test(rel)) continue;
    if (e.isDirectory()) walk(root, p, out);
    else out.push(rel);
  }
  return out;
}

const read = (dir: string, rel: string): string => {
  const p = join(dir, rel);
  try {
    return statSync(p).size > MAX_BYTES ? "" : readFileSync(p, "utf8");
  } catch {
    return "";
  }
};

export type Dup = { kind: string; key: string; members: string[] };

/** Crate and package names with the organization prefixes removed, grouped; a family of two or more units is a candidate. */
export function families(units: { repo: string; name: string }[]): Dup[] {
  const stems = new Map<string, string[]>();
  for (const u of units) {
    const stem = u.name.replace(/^@[^/]+\//, "").replace(/^(aphrody|ay|yolo|bun|anu|nie|rg)[-_]/, "").replace(/[-_](sys|core|cli|server|client|types|macros?|proto|pb|wasm|pure)$/, "");
    if (stem.length < 3) continue;
    const list = stems.get(stem) ?? [];
    list.push(`${u.repo}:${u.name}`);
    stems.set(stem, list);
  }
  return [...stems].filter(([, m]) => new Set(m.map((x) => x.split(":")[0])).size > 1 || m.length > 2).map(([key, members]) => ({ kind: "family", key, members: members.sort() }));
}

export function lockVersions(text: string): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  for (const m of text.matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g)) (out.get(m[1]!) ?? out.set(m[1]!, new Set()).get(m[1]!)!).add(m[2]!);
  return out;
}

const SQL = /\b(SELECT\b[^;"'`]{4,300}?\bFROM\s+[\w."`]+|INSERT\s+(?:OR\s+\w+\s+)?INTO\s+[\w."`]+|CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?\s+[\w."`]+|UPDATE\s+[\w."`]+\s+SET|DELETE\s+FROM\s+[\w."`]+)/gi;

export function normalizeSql(sql: string): string {
  return sql.toLowerCase().replace(/\s+/g, " ").replace(/'[^']*'/g, "?").replace(/\$\d+|:\w+|\?\d+/g, "?").replace(/\b\d+\b/g, "?").replace(/["`]/g, "").trim();
}

const DB_LIBS: [string, RegExp][] = [["bun:sqlite", /bun:sqlite/], ["Bun.sql", /\bBun\.(sql|SQL)\b/], ["rusqlite", /\brusqlite\b/], ["sqlx", /\bsqlx\b/], ["sqlez", /\bsqlez\b/], ["diesel", /\bdiesel\b/], ["better-sqlite3", /better-sqlite3/], ["postgres client", /tokio-postgres|deadpool-postgres|from ["']pg["']/], ["redis", /\bredis\b/i]];

export type Audit = { repos: string[]; families: Dup[]; lockDuplicates: { repo: string; name: string; versions: string[] }[]; spread: { name: string; repos: string[]; versions: string[] }[]; sql: Dup[]; dbLibraries: Record<string, string[]>; crawlers: Record<string, string[]> };

export function audit(repos: Repo[]): Audit {
  const units: { repo: string; name: string }[] = [];
  const lockDuplicates: Audit["lockDuplicates"] = [];
  const crates = new Map<string, Map<string, Set<string>>>();
  const sql = new Map<string, Set<string>>();
  const sample = new Map<string, string>();
  const dbLibraries: Record<string, string[]> = {};
  const crawlers: Record<string, string[]> = {};
  for (const repo of repos) {
    const files = listFiles(repo.dir);
    for (const f of files) {
      const base = basename(f);
      if (base === "Cargo.toml") {
        const m = /^\[package\][\s\S]*?^name\s*=\s*"([^"]+)"/m.exec(read(repo.dir, f));
        if (m) units.push({ repo: repo.name, name: m[1]! });
      } else if (base === "package.json") {
        try {
          const name = (JSON.parse(read(repo.dir, f) || "{}") as { name?: string }).name;
          if (name) units.push({ repo: repo.name, name });
        } catch {}
      } else if (base === "Cargo.lock") {
        for (const [name, versions] of lockVersions(read(repo.dir, f))) {
          const perRepo = crates.get(name) ?? new Map<string, Set<string>>();
          perRepo.set(repo.name, versions);
          crates.set(name, perRepo);
          if (versions.size > 1) lockDuplicates.push({ repo: repo.name, name, versions: [...versions].sort() });
        }
      }
      if (/(crawl|scrap|mirror|indexer|sitemap)/i.test(f) && SOURCE.test(f)) (crawlers[repo.name] ??= []).push(f);
      if (!SOURCE.test(f)) continue;
      const text = read(repo.dir, f);
      if (!text) continue;
      for (const [lib, re] of DB_LIBS) if (re.test(text)) ((dbLibraries[lib] ??= []).includes(repo.name) || dbLibraries[lib]!.push(repo.name));
      if (/\.(sql|rs|ts|tsx|js|py|go)$/.test(f)) {
        for (const m of text.matchAll(SQL)) {
          const norm = normalizeSql(m[1]!);
          if (norm.length < 25) continue;
          const key = Bun.hash(norm).toString(16);
          (sql.get(key) ?? sql.set(key, new Set()).get(key)!).add(`${repo.name}:${f}`);
          sample.set(key, norm.slice(0, 100));
        }
      }
    }
  }
  const spread = [...crates].filter(([, per]) => per.size > 1 && new Set([...per.values()].flatMap((v) => [...v])).size > 1).map(([name, per]) => ({ name, repos: [...per.keys()].sort(), versions: [...new Set([...per.values()].flatMap((v) => [...v]))].sort() }));
  return {
    repos: repos.map((r) => r.name),
    families: families(units),
    lockDuplicates: lockDuplicates.sort((a, b) => b.versions.length - a.versions.length),
    spread: spread.sort((a, b) => b.repos.length - a.repos.length),
    sql: [...sql].filter(([, m]) => m.size > 1).map(([key, m]) => ({ kind: "sql", key: sample.get(key)!, members: [...m].sort() })),
    dbLibraries,
    crawlers,
  };
}

export function summarize(a: Audit): string {
  const top = <T>(xs: T[], n = 15) => xs.slice(0, n);
  return [
    "# Duplication audit", "", `Repositories: ${a.repos.join(", ")}`, "",
    `- crate/package families spanning repositories or three or more units: ${a.families.length}`,
    `- crates with several versions inside one lockfile: ${a.lockDuplicates.length}`,
    `- crates at different versions across repositories: ${a.spread.length}`,
    `- identical SQL statements in several files: ${a.sql.length}`,
    `- database libraries in use: ${Object.entries(a.dbLibraries).map(([k, v]) => `${k} (${v.length} repos)`).join(", ")}`,
    `- repositories with crawler, mirror or index code: ${Object.keys(a.crawlers).length}`, "",
    "## Families", ...top(a.families).map((f) => `- ${f.key}: ${f.members.join(", ")}`), "",
    "## Spread (same crate, several repositories and versions)", ...top(a.spread).map((s) => `- ${s.name}: ${s.repos.length} repos, ${s.versions.length} versions`), "",
    "## SQL in several files", ...top(a.sql).map((s) => `- \`${s.key}\`: ${s.members.length} files`), "",
    "## Crawler, mirror and index code", ...Object.entries(a.crawlers).map(([r, f]) => `- ${r}: ${f.length} files (${f.slice(0, 3).join(", ")})`), "",
  ].join("\n");
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const cfg = readAy(repoRoot) as { repos?: Record<string, { checkout?: string }> };
  const repos: Repo[] = args.length ? args.map((d) => ({ name: basename(resolve(d)), dir: resolve(d) })) : Object.keys(cfg.repos ?? {}).flatMap((name) => { const dir = checkoutOf(name, cfg as Record<string, unknown>); return existsSync(dir) ? [{ name, dir }] : []; });
  const a = audit(repos);
  await Bun.write(privatePath("dedup.json", repoRoot, true), JSON.stringify(a, null, 2) + "\n");
  await Bun.write(privatePath("dedup.md", repoRoot, true), summarize(a));
  console.log(summarize(a).split("\n").slice(0, 12).join("\n"));
}
