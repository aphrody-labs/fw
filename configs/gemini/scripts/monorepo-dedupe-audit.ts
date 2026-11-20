#!/usr/bin/env bun
/**
 * Deterministic 4-layer multi-monorepo deduplication auditor.
 * Scans tracked files (`git ls-files`) across 1..N Git repositories for:
 *   1. Exact SHA-256 content duplicates (cross-repo & intra-repo)
 *   2. Duplicate Cargo crate names & package.json names
 *   3. Duplicate SKILL.md entries
 *   4. Top heavyweight tracked directories (bloat candidates)
 *
 * Usage:
 *   bun ~/.gemini/scripts/monorepo-dedupe-audit.ts C:\src\fairwind C:\src\aphrody C:\src\ay
 */

import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { basename, join, resolve } from "node:path";

const repoPaths = Bun.argv.slice(2).map((p) => resolve(p));
if (repoPaths.length === 0) {
  console.error("Usage: bun monorepo-dedupe-audit.ts <repo1> [repo2 ...]");
  process.exit(1);
}

const byHash = new Map<string, Array<{ repo: string; rel: string; bytes: number }>>();
const cratesByName = new Map<string, Array<{ repo: string; rel: string }>>();
const pkgsByName = new Map<string, Array<{ repo: string; rel: string }>>();
const skillsByName = new Map<string, Array<{ repo: string; rel: string }>>();
const summaries: Array<{
  repo: string;
  head: string;
  branch: string;
  trackedFiles: number;
  trackedMB: number;
  dirtyLines: number;
  topDirs: Array<{ dir: string; files: number; mb: number }>;
}> = [];

for (const root of repoPaths) {
  const name = basename(root);
  if (!existsSync(join(root, ".git"))) {
    console.error(`Skipping non-git directory: ${root}`);
    continue;
  }
  const tracked = Bun.spawnSync(["git", "-C", root, "ls-files"], { stdout: "pipe" })
    .stdout.toString()
    .split(/\r?\n/)
    .filter(Boolean);
  const head = Bun.spawnSync(["git", "-C", root, "rev-parse", "--short", "HEAD"], { stdout: "pipe" })
    .stdout.toString()
    .trim();
  const branch = Bun.spawnSync(["git", "-C", root, "branch", "--show-current"], { stdout: "pipe" })
    .stdout.toString()
    .trim();
  const dirty = Bun.spawnSync(["git", "-C", root, "status", "--short"], { stdout: "pipe" })
    .stdout.toString()
    .trim();

  let totalBytes = 0;
  const dirStats: Record<string, { files: number; bytes: number }> = {};

  for (const rel of tracked) {
    const abs = join(root, rel);
    if (!existsSync(abs)) continue;
    let st;
    try {
      st = statSync(abs);
    } catch {
      continue;
    }
    if (!st.isFile()) continue;
    totalBytes += st.size;

    const top = rel.includes("/") ? rel.split("/")[0] : "(root)";
    const bucket = dirStats[top] ?? (dirStats[top] = { files: 0, bytes: 0 });
    bucket.files++;
    bucket.bytes += st.size;

    if (st.size >= 256 && st.size <= 20 * 1024 * 1024) {
      const buf = readFileSync(abs);
      const h = createHash("sha256").update(buf).digest("hex");
      const arr = byHash.get(h) ?? [];
      arr.push({ repo: name, rel, bytes: st.size });
      byHash.set(h, arr);

      if (rel.endsWith("Cargo.toml")) {
        const m = buf.toString("utf8").match(/\[package\][\s\S]*?name\s*=\s*"([^"]+)"/);
        if (m) {
          const carr = cratesByName.get(m[1]) ?? [];
          carr.push({ repo: name, rel });
          cratesByName.set(m[1], carr);
        }
      } else if (rel.endsWith("package.json")) {
        try {
          const pkg = JSON.parse(buf.toString("utf8")) as { name?: string };
          if (pkg.name) {
            const parr = pkgsByName.get(pkg.name) ?? [];
            parr.push({ repo: name, rel });
            pkgsByName.set(pkg.name, parr);
          }
        } catch {
          // Ignore malformed JSON.
        }
      } else if (rel.endsWith("SKILL.md")) {
        const m = buf.toString("utf8").match(/^name:\s*([^\r\n]+)/m);
        const sname = (m ? m[1] : rel).trim().replace(/^["']|["']$/g, "");
        const sarr = skillsByName.get(sname) ?? [];
        sarr.push({ repo: name, rel });
        skillsByName.set(sname, sarr);
      }
    }
  }

  summaries.push({
    repo: name,
    head,
    branch,
    trackedFiles: tracked.length,
    trackedMB: Number((totalBytes / 1048576).toFixed(2)),
    dirtyLines: dirty ? dirty.split(/\r?\n/).length : 0,
    topDirs: Object.entries(dirStats)
      .sort((a, b) => b[1].bytes - a[1].bytes)
      .slice(0, 6)
      .map(([dir, v]) => ({ dir, files: v.files, mb: Number((v.bytes / 1048576).toFixed(2)) })),
  });
}

let crossRepoDupGroups = 0;
let crossRepoWastedBytes = 0;
const crossRepoSample: string[] = [];
const intraRepoDupCount: Record<string, { files: number; wastedMB: number }> = {};

for (const arr of byHash.values()) {
  if (arr.length < 2) continue;
  const isLicense = arr.every((e) => /(^|\/)(LICENSE|COPYING)/i.test(e.rel));
  if (isLicense) continue;

  const repoSet = new Set(arr.map((e) => e.repo));
  if (repoSet.size > 1) {
    crossRepoDupGroups++;
    crossRepoWastedBytes += arr[0].bytes * (arr.length - 1);
    if (crossRepoSample.length < 15) {
      crossRepoSample.push(arr.map((e) => `${e.repo}:${e.rel}`).join(" == "));
    }
  } else {
    const r = arr[0].repo;
    const st = intraRepoDupCount[r] ?? (intraRepoDupCount[r] = { files: 0, wastedMB: 0 });
    st.files += arr.length - 1;
    st.wastedMB = Number((st.wastedMB + (arr[0].bytes * (arr.length - 1)) / 1048576).toFixed(2));
  }
}

const mapDups = (m: Map<string, Array<{ repo: string; rel: string }>>) =>
  [...m.entries()]
    .filter(([, arr]) => arr.length > 1)
    .map(([name, arr]) => ({ name, locations: arr.map((x) => `${x.repo}:${x.rel}`) }));

console.log(
  JSON.stringify(
    {
      auditedAt: new Date().toISOString(),
      repos: summaries,
      crossRepoDuplicates: {
        groups: crossRepoDupGroups,
        wastedMB: Number((crossRepoWastedBytes / 1048576).toFixed(2)),
        sample: crossRepoSample,
      },
      intraRepoDuplicates: intraRepoDupCount,
      duplicateCrates: mapDups(cratesByName),
      duplicatePackages: mapDups(pkgsByName).slice(0, 20),
      duplicateSkills: mapDups(skillsByName),
    },
    null,
    2,
  ),
);
