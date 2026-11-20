#!/usr/bin/env bun
/**
 * The workspace orchestrator: every sibling checkout of the shared root (the parent of this repository), as declared by
 * ay.toml [repos]. Nothing here names a repository; the list, the order and the roles come from the registry.
 *
 *   bun tools/workspace.ts status       # branch, dirty files, ahead/behind, last commit of every live checkout
 *   bun tools/workspace.ts audit        # registry vs disk: missing checkouts, unregistered or retired directories, remotes
 *   bun tools/workspace.ts manifests    # `cargo metadata --no-deps --offline` of every Cargo workspace (no build)
 *   bun tools/workspace.ts pull         # fetch, then fast-forward every clean checkout on its tracked branch
 *   bun tools/workspace.ts tools        # versions of the toolchain on PATH
 *   bun tools/workspace.ts cache        # cache-related environment and sccache statistics
 *   bun tools/workspace.ts run <repo> [recipe ...]   # a just recipe of one repository, run from its directory
 *
 * WORKSPACE_ROOT overrides the root. Exit code 1 when audit or manifests finds a problem.
 */

import { existsSync, readdirSync, statSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { checkoutOf, readAy, repoRoot } from "./local.ts";

type Json = Record<string, any>;

export interface Repo { key: string; slug: string; kind: string; dir: string }

export const workspaceRoot = (): string => resolve(Bun.env.WORKSPACE_ROOT ?? dirname(repoRoot));

/** Live repositories of the registry, in declaration (stack) order, with their resolved checkout. */
export function registry(cfg: Json = readAy()): Repo[] {
  return Object.entries<Json>(cfg.repos ?? {}).map(([key, r]) => ({ key, slug: String(r.slug ?? ""), kind: String(r.kind ?? ""), dir: checkoutOf(key, cfg) }));
}

const isGit = (dir: string) => existsSync(join(dir, ".git"));

async function sh(cmd: string[], cwd?: string): Promise<{ ok: boolean; out: string }> {
  const p = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "pipe", stdin: "ignore", env: { ...Bun.env, GIT_TERMINAL_PROMPT: "0" } });
  const [out, err] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text()]);
  const code = await p.exited;
  return { ok: code === 0, out: (code === 0 ? out : err || out).trim() };
}

const git = (dir: string, ...args: string[]) => sh(["git", "-C", dir, ...args]);

/** Directories of the root that hold a git checkout. */
export function checkoutsOnDisk(root: string): string[] {
  return readdirSync(root).filter((n) => !n.startsWith(".")).map((n) => join(root, n)).filter((d) => statSync(d, { throwIfNoEntry: false })?.isDirectory() && isGit(d));
}

/** Registry vs disk. Pure: the caller passes what it observed. */
export function compare(repos: Repo[], onDisk: string[], retired: Record<string, Json>): { missing: string[]; unregistered: string[]; retiredPresent: string[] } {
  const known = new Set(repos.map((r) => resolve(r.dir).toLowerCase()));
  const retiredNames = new Set(Object.keys(retired));
  const extra = onDisk.filter((d) => !known.has(resolve(d).toLowerCase()));
  return {
    missing: repos.filter((r) => !isGit(r.dir)).map((r) => r.key),
    unregistered: extra.filter((d) => !retiredNames.has(basename(d))).map((d) => basename(d)),
    retiredPresent: extra.filter((d) => retiredNames.has(basename(d))).map((d) => basename(d)),
  };
}

/** `owner/name` of a GitHub remote URL (https or ssh), lower case; undefined for other hosts. */
export function slugOf(url: string): string | undefined {
  const m = url.trim().match(/github\.com[:/]([^/\s]+)\/([^/\s]+?)(?:\.git)?\/?$/i);
  return m ? `${m[1]}/${m[2]}`.toLowerCase() : undefined;
}

const live = (cfg: Json) => registry(cfg).filter((r) => isGit(r.dir));

async function status(cfg: Json) {
  const rows = await Promise.all(live(cfg).map(async (r) => {
    const [branch, dirty, ab, last] = await Promise.all([
      git(r.dir, "branch", "--show-current"),
      git(r.dir, "status", "--porcelain"),
      git(r.dir, "rev-list", "--left-right", "--count", "@{upstream}...HEAD"),
      git(r.dir, "log", "-1", "--format=%h %cr %s"),
    ]);
    const [behind, ahead] = ab.ok ? ab.out.split(/\s+/) : ["?", "?"];
    const files = dirty.out ? dirty.out.split("\n").length : 0;
    return [r.key, r.kind, branch.out || "detached", `dirty=${files}`, `ahead=${ahead} behind=${behind}`, last.out.slice(0, 70)];
  }));
  for (const row of rows) console.log(row.map((c, i) => (i < 5 ? String(c).padEnd([10, 10, 8, 10, 18][i]!) : c)).join(" "));
}

async function audit(cfg: Json): Promise<number> {
  const root = workspaceRoot();
  const repos = registry(cfg);
  const diff = compare(repos, checkoutsOnDisk(root), cfg.retired ?? {});
  let problems = 0;
  for (const k of diff.missing) { console.log(`missing     ${k}: no checkout at ${checkoutOf(k, cfg)}`); }
  for (const d of diff.unregistered) { problems++; console.log(`unregistered ${d}: a checkout under ${root} that ay.toml [repos] does not list`); }
  for (const d of diff.retiredPresent) { problems++; console.log(`retired     ${d}: listed in ay.toml [retired] but still checked out under ${root}`); }
  await Promise.all(live(cfg).map(async (r) => {
    const url = await git(r.dir, "remote", "get-url", "origin");
    if (!url.ok) { problems++; console.log(`remote      ${r.key}: no origin remote`); return; }
    const slug = slugOf(url.out);
    if (slug !== r.slug.toLowerCase()) { problems++; console.log(`remote      ${r.key}: origin is ${url.out}, registry says ${r.slug}`); }
  }));
  console.log(problems ? `${problems} problem(s)` : `ok: ${repos.length} registered, ${repos.length - diff.missing.length} checked out under ${root}`);
  return problems ? 1 : 0;
}

async function manifests(cfg: Json): Promise<number> {
  const results = await Promise.all(live(cfg).filter((r) => existsSync(join(r.dir, "Cargo.toml"))).map(async (r) => {
    const res = await sh(["cargo", "metadata", "--no-deps", "--offline", "--format-version", "1", "--manifest-path", join(r.dir, "Cargo.toml")]);
    const n = res.ok ? (JSON.parse(res.out).packages?.length ?? 0) : 0;
    return { key: r.key, ok: res.ok, msg: res.ok ? `${n} packages` : res.out.split("\n").filter((l) => /error/i.test(l)).slice(0, 3).join(" | ") };
  }));
  for (const r of results) console.log(`${r.ok ? "ok  " : "FAIL"} ${r.key.padEnd(10)} ${r.msg}`);
  return results.every((r) => r.ok) ? 0 : 1;
}

async function pull(cfg: Json) {
  for (const r of live(cfg)) {
    const dirty = (await git(r.dir, "status", "--porcelain", "--untracked-files=no")).out;
    if (dirty) { console.log(`skip ${r.key}: tracked changes`); continue; }
    const res = await git(r.dir, "pull", "--ff-only", "--quiet");
    console.log(`${res.ok ? "ok  " : "FAIL"} ${r.key}${res.ok ? "" : `: ${res.out.split("\n")[0]}`}`);
  }
}

async function tools() {
  const list: [string, string][] = [["git", "--version"], ["just", "--version"], ["bun", "--version"], ["rustc", "--version"], ["cargo", "--version"], ["sccache", "--version"], ["gh", "--version"], ["dotnet", "--version"], ["go", "version"], ["python", "--version"], ["uv", "--version"], ["clang", "--version"], ["cmake", "--version"], ["ninja", "--version"]];
  const rows = await Promise.all(list.map(async ([bin, flag]) => [bin, Bun.which(bin) ? (await sh([bin, flag])).out.split("\n")[0] : "not found"] as const));
  for (const [bin, v] of rows) console.log(`${bin.padEnd(8)} ${v}`);
}

async function cache() {
  for (const k of ["CARGO_HOME", "RUSTUP_HOME", "CARGO_TARGET_DIR", "SCCACHE_DIR", "RUSTC_WRAPPER", "BUN_INSTALL", "VCPKG_DEFAULT_BINARY_CACHE"]) console.log(`${k.padEnd(28)} ${Bun.env[k] ?? "(unset)"}`);
  if (Bun.which("sccache")) console.log(`\n${(await sh(["sccache", "--show-stats"])).out}`);
}

async function run(cfg: Json, key: string | undefined, args: string[]): Promise<number> {
  const repo = registry(cfg).find((r) => r.key === key);
  if (!repo || !isGit(repo.dir)) { console.error(`unknown or absent repository: ${key ?? "(none)"}; one of ${live(cfg).map((r) => r.key).join(", ")}`); return 2; }
  const p = Bun.spawn(["just", "--justfile", join(repo.dir, "justfile"), "--working-directory", repo.dir, ...args], { stdio: ["inherit", "inherit", "inherit"] });
  return await p.exited;
}

if (import.meta.main) {
  const [cmd = "status", ...rest] = process.argv.slice(2);
  const cfg = readAy() as Json;
  const code = await ({
    status: async () => (await status(cfg), 0),
    audit: () => audit(cfg),
    manifests: () => manifests(cfg),
    pull: async () => (await pull(cfg), 0),
    tools: async () => (await tools(), 0),
    cache: async () => (await cache(), 0),
    run: () => run(cfg, rest[0], rest.slice(1)),
  } as Record<string, () => Promise<number>>)[cmd]?.() ?? (console.error(`usage: bun tools/workspace.ts <status|audit|manifests|pull|tools|cache|run <repo> [recipe...]>`), 2);
  process.exit(code);
}
