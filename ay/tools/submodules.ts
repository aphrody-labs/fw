#!/usr/bin/env bun
// Pop!_OS / COSMIC-style declarative submodule manager for `ay`.
// Reads `submodules.toml`, generates `.gitmodules` with relative `../../aphrody-labs/<fork>.git` URLs,
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

export function relativeForkUrl(repo: string): string {
  const [org, name] = repo.split("/");
  if (!org || !name) throw new Error(`invalid repo slug: ${repo}`);
  return `../../${org}/${name}.git`;
}

export function validateManifest(manifest: SubmodulesManifest): string[] {
  const errors: string[] = [];
  if (manifest.version !== 1) errors.push(`unsupported version ${manifest.version}`);
  const seenPaths = new Set<string>();
  const seenRepos = new Set<string>();
  for (const s of manifest.submodule ?? []) {
    // Upstream forks live in aphrody-labs under third_party/; the kernels and OS layers of the product (own forks) live in aphrody-dev under os/.
    const own = s.path.startsWith("os/");
    if (!own && !s.path.startsWith("third_party/")) errors.push(`${s.path}: submodule must live under third_party/ or os/`);
    if (seenPaths.has(s.path)) errors.push(`duplicate submodule path: ${s.path}`);
    seenPaths.add(s.path);
    if (seenRepos.has(s.repo)) errors.push(`duplicate fork submodule (deduplication rule): ${s.repo}`);
    seenRepos.add(s.repo);
    const org = own ? "aphrody-dev/" : "aphrody-labs/";
    if (!s.repo.startsWith(org)) errors.push(`${s.path}: fork must live in ${org.slice(0, -1)}, got ${s.repo}`);
    if (!SHA_RE.test(s.sha)) errors.push(`${s.path}: invalid 40-hex sha ${s.sha}`);
    if (!s.branch) errors.push(`${s.path}: missing branch`);
  }
  return errors;
}

export function renderGitmodules(manifest: SubmodulesManifest): string {
  return (
    manifest.submodule
      .map(
        (s) =>
          `[submodule "${s.path}"]\n\tpath = ${s.path}\n\turl = ${relativeForkUrl(s.repo)}\n\tbranch = ${s.branch}`,
      )
      .join("\n") + "\n"
  );
}

export function readGitlinks(root = repoRoot): Map<string, string> {
  const p = Bun.spawnSync(["git", "-C", root, "ls-files", "--stage", "--", "third_party", "os"], {
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
