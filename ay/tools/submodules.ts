#!/usr/bin/env bun
// Pop!_OS / COSMIC-style declarative submodule manager for `ay`.
// Reads `submodules.toml`, generates `.gitmodules` with relative `../<fork>.git` URLs (same org as ay.toml product.owner),
// and verifies or updates `160000` gitlinks in the git index without requiring gigabytes of local checkouts.
//   bun tools/submodules.ts              write .gitmodules and stage 160000 gitlinks
//   bun tools/submodules.ts --check      verify .gitmodules, SHA pins, uniqueness, and git index gitlinks
//   bun tools/submodules.ts --init <p>   shallow-checkout one or more submodules on a build host
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { repoRoot } from "./local.ts";

export interface SubmoduleSpec {
  path: string;
  repo: string;
  branch: string;
  sha: string;
  domain: string;
  role: string;
  /** `owner/name` of the upstream the fork tracks, when it is one. */
  upstream?: string;
}

export interface SubmodulesManifest {
  version: number;
  submodule: SubmoduleSpec[];
}

const SHA_RE = /^[0-9a-f]{40}$/;

export function loadManifest(root = repoRoot): SubmodulesManifest {
  const raw = readFileSync(join(root, "submodules.toml"), "utf8");
  return Bun.TOML.parse(raw) as unknown as SubmodulesManifest;
}

/** Organization that owns this repository (ay.toml product.owner); relative submodule URLs resolve against it. */
export function homeOrg(root = repoRoot): string {
  const ay = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as { product?: { owner?: string } };
  const owner = ay.product?.owner;
  if (!owner) throw new Error("ay.toml: product.owner is missing");
  return owner;
}

/** `../<name>.git` inside the home org, `../../<org>/<name>.git` across orgs. */
export function relativeForkUrl(repo: string, home = homeOrg()): string {
  const [org, name] = repo.split("/");
  if (!org || !name) throw new Error(`invalid repo slug: ${repo}`);
  return org === home ? `../${name}.git` : `../../${org}/${name}.git`;
}

export function validateManifest(manifest: SubmodulesManifest, home = homeOrg()): string[] {
  const errors: string[] = [];
  if (manifest.version !== 1) errors.push(`unsupported version ${manifest.version}`);
  const seenPaths = new Set<string>();
  const seenRepos = new Set<string>();
  for (const s of manifest.submodule ?? []) {
    if (!s.path.startsWith("third_party/")) errors.push(`${s.path}: submodule must live under third_party/`);
    if (seenPaths.has(s.path)) errors.push(`duplicate submodule path: ${s.path}`);
    seenPaths.add(s.path);
    if (seenRepos.has(s.repo)) errors.push(`duplicate fork submodule (deduplication rule): ${s.repo}`);
    seenRepos.add(s.repo);
    if (!s.repo.startsWith(`${home}/`)) errors.push(`${s.path}: fork must live in ${home}, got ${s.repo}`);
    if (!SHA_RE.test(s.sha)) errors.push(`${s.path}: invalid 40-hex sha ${s.sha}`);
    if (!s.branch) errors.push(`${s.path}: missing branch`);
  }
  return errors;
}

export function renderGitmodules(manifest: SubmodulesManifest, home = homeOrg()): string {
  return (
    manifest.submodule
      .map(
        (s) =>
          `[submodule "${s.path}"]\n\tpath = ${s.path}\n\turl = ${relativeForkUrl(s.repo, home)}\n\tbranch = ${s.branch}`,
      )
      .join("\n") + "\n"
  );
}

export function readGitlinks(root = repoRoot): Map<string, string> {
  const p = Bun.spawnSync(["git", "-C", root, "ls-files", "--stage", "--", "third_party"], {
    stdout: "pipe",
    stderr: "pipe",
    windowsHide: true,
  });
  const map = new Map<string, string>();
  if (p.exitCode !== 0) return map;
  for (const line of p.stdout.toString().split(/\r?\n/)) {
    const m = /^160000\s+([0-9a-f]{40})\s+\d+\s+(.+)$/.exec(line.trim());
    if (m) map.set(m[2]!, m[1]!);
  }
  return map;
}

export async function applyGitmodulesAndLinks(root = repoRoot): Promise<void> {
  const manifest = loadManifest(root);
  const errs = validateManifest(manifest);
  if (errs.length) throw new Error(errs.join("\n"));
  await Bun.write(join(root, ".gitmodules"), renderGitmodules(manifest));
  for (const s of manifest.submodule) {
    mkdirSync(join(root, s.path), { recursive: true });
    const res = Bun.spawnSync(
      ["git", "-C", root, "update-index", "--add", "--cacheinfo", `160000,${s.sha},${s.path}`],
      { stdout: "pipe", stderr: "pipe", windowsHide: true },
    );
    if (res.exitCode !== 0) {
      throw new Error(`git update-index failed for ${s.path}: ${res.stderr.toString().trim()}`);
    }
  }
}

export function checkAll(root = repoRoot): string[] {
  const manifest = loadManifest(root);
  const errs = validateManifest(manifest);
  const gmPath = join(root, ".gitmodules");
  if (!existsSync(gmPath)) {
    errs.push(".gitmodules is missing");
  } else {
    const expected = renderGitmodules(manifest);
    const actual = readFileSync(gmPath, "utf8").replaceAll("\r\n", "\n");
    if (actual !== expected) errs.push(".gitmodules is out of date with submodules.toml (run `bun tools/submodules.ts`)");
  }
  const links = readGitlinks(root);
  for (const s of manifest.submodule) {
    const got = links.get(s.path);
    if (got !== s.sha) errs.push(`${s.path}: gitlink is ${got ?? "missing"}, expected ${s.sha}`);
  }
  const declared = new Set(manifest.submodule.map((s) => s.path));
  for (const path of links.keys()) if (!declared.has(path)) errs.push(`${path}: gitlink is not declared in submodules.toml`);
  return errs;
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.includes("--check")) {
    const errs = checkAll(repoRoot);
    for (const e of errs) console.error(e);
    const count = loadManifest(repoRoot).submodule.length;
    console.log(errs.length ? `${errs.length} submodule error(s)` : `${count} fork submodules verified (0 duplicates)`);
    process.exit(errs.length ? 1 : 0);
  } else if (args[0] === "--init") {
    const targets = args.slice(1);
    const p = Bun.spawnSync(
      ["git", "-C", repoRoot, "submodule", "update", "--init", "--depth", "1", "--jobs", "8", ...(targets.length ? ["--", ...targets] : [])],
      { stdout: "inherit", stderr: "inherit", windowsHide: true },
    );
    process.exit(p.exitCode);
  } else {
    await applyGitmodulesAndLinks(repoRoot);
    console.log(`wrote .gitmodules and staged ${loadManifest(repoRoot).submodule.length} gitlinks`);
  }
}
