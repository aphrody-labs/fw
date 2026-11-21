#!/usr/bin/env bun
/**
 * Recursive duplicate vendor / third_party detector across all workspace checkouts (`SRC_ROOT` / parent of repo root).
 *
 * Combines:
 *   - `eza` (`-D -T`) recursive directory discovery (catches uninitialized submodule mountpoints and nested vendor trees)
 *   - `rg` (`ripgrep`) recursive file, manifest (`.gitmodules`, `submodules.toml`, `forks.toml`), and package/crate metadata scanning
 *   - `fzf` + trigram/Levenshtein/stem fuzzy matching to detect exact and near-duplicate vendored dependencies across repos
 *
 * Usage:
 *   bun tools/vendor-dedup.ts [options] [<dir>...]
 *   ay vendor-dedup [options] [<dir>...]
 */
import { readdirSync, statSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { privatePath, repoRoot } from "./local.ts";

export type EntryKind = "vendor-dir" | "submodule" | "fork" | "root-checkout";

export interface VendorEntry {
  id: string;
  repo: string;
  relPath: string;
  absPath: string;
  vendorRoot: string;
  kind: EntryKind;
  name: string;
  gitSlug?: string;
  upstreamRepo?: string;
  repoLeaf?: string;
  pkgName?: string;
  branch?: string;
  sha?: string;
  fileCount: number;
  isTestFixture: boolean;
  discoveredBy: ("eza" | "rg" | "manifest" | "root")[];
}

export interface DuplicateGroup {
  key: string;
  tier: "exact" | "fuzzy";
  score: number;
  reasons: string[];
  repos: string[];
  entries: VendorEntry[];
}

export interface VendorDedupReport {
  scanRoot: string;
  scannedRepos: string[];
  generatedAt: string;
  tools: { eza: boolean; rg: boolean; fzf: boolean };
  stats: {
    totalEntries: number;
    vendorRoots: number;
    exactGroups: number;
    exactDuplicateEntries: number;
    fuzzyGroups: number;
    fuzzyDuplicateEntries: number;
    durationMs: number;
  };
  exactGroups: DuplicateGroup[];
  fuzzyGroups: DuplicateGroup[];
  entries: VendorEntry[];
}

export interface ScanOptions {
  scanRoot?: string;
  dirs?: string[];
  depth?: number;
  threshold?: number;
  includeTests?: boolean;
  includeRootRepos?: boolean;
  fuzzyQuery?: string;
  useFzf?: boolean;
}

const VENDOR_DIR_RE = /^(?:vendor|third[_-]?party|externals?|extern)$/i;
const TEST_FIXTURE_RE = /(?:^|\/)(?:tests?|fixtures?|testdata|__fixtures__|e2e|examples?)(?:\/|$)/i;
const EZA_IGNORE = ".git|node_modules|target|.cache|.private|dist|build|.venv|venv|__pycache__|.next|.turbo|out|.vercel";
const MANIFEST_FILES = new Set(["Cargo.toml", "package.json", "go.mod", "pyproject.toml", "CMakeLists.txt"]);
const GENERIC_NAMES = new Set([
  "app", "apps", "c", "cli", "common", "core", "crates", "dev", "docs", "example", "examples",
  "extern", "external", "externals", "fixture", "fixtures", "go", "include", "js", "kit", "lib",
  "libs", "main", "monorepo", "os", "package", "packages", "pkg", "pkgs", "py", "python", "repo",
  "root", "rs", "runtime", "rust", "sdk", "shared", "src", "test", "testdata", "tests",
  "third-party", "third_party", "thirdparty", "tools", "ts", "types", "ui", "utils", "vendor",
  "web", "workspace",
]);
const STOP_TOKENS = new Set([
  "app", "cli", "core", "dev", "git", "js", "kit", "lib", "libs", "org", "os", "pkg", "rs", "sdk", "sys", "ts", "ui", "web",
]);

export const posix = (p: string): string => p.replaceAll("\\", "/");

export function isDirectory(p: string): boolean {
  try {
    return statSync(p).isDirectory();
  } catch {
    return false;
  }
}

export function isFile(p: string): boolean {
  try {
    return statSync(p).isFile();
  } catch {
    return false;
  }
}

export function defaultScanRoot(root = repoRoot): string {
  const envRoot = Bun.env.SRC_ROOT?.trim();
  return envRoot ? resolve(envRoot) : dirname(root);
}

export function isVendorSegment(name: string): boolean {
  return VENDOR_DIR_RE.test(name);
}

export function isTestFixturePath(relPath: string): boolean {
  return TEST_FIXTURE_RE.test(posix(relPath));
}

// --- Git slug & name normalization -----------------------------------------------------------------------------------

export function parseGitSlug(raw: string): { slug?: string; leaf: string } | undefined {
  const trimmed = raw.trim().replace(/^git\+/, "").replace(/\/+$/, "").replace(/\.git$/i, "");
  if (!trimmed) return undefined;

  const rel = /^(?:\.\.\/)+(.+)$/.exec(trimmed);
  if (rel) {
    const parts = rel[1]!.split("/").filter(Boolean);
    if (parts.length >= 2) {
      return { slug: `${parts.at(-2)!}/${parts.at(-1)!}`.toLowerCase(), leaf: parts.at(-1)! };
    }
    if (parts.length === 1) return { leaf: parts[0]! };
  }

  const remote = /(?:github\.com|gitlab\.com|codeberg\.org|bitbucket\.org)[:/]([^/:]+)\/([^/]+)$/i.exec(trimmed);
  if (remote) {
    return { slug: `${remote[1]!}/${remote[2]!}`.toLowerCase(), leaf: remote[2]! };
  }

  const plain = /^([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)$/.exec(trimmed);
  if (plain) {
    return { slug: `${plain[1]!}/${plain[2]!}`.toLowerCase(), leaf: plain[2]! };
  }
  return undefined;
}

export function canonicalName(raw: string): string {
  return raw
    .trim()
    .toLowerCase()
    .replace(/^@[^/]+\//, "")
    .replace(/\.git$/i, "")
    .replace(/[-_.]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function normalizeStem(raw: string): string {
  let s = raw
    .trim()
    .toLowerCase()
    .replace(/^@[^/]+\//, "")
    .replace(/\.git$/i, "");
  const unprefixed = s.replace(/^(?:aphrody|ay|longbridge|cloudflare|vercel|storytold|rust-lang|uutils)[-_]/, "");
  // Keep prefix if stripping it would leave a generic/short stub like "cf" or "ai".
  if (unprefixed.replace(/[^a-z0-9]+/g, "").length >= 4) s = unprefixed;
  const unsuffixed = s.replace(/(?:\.js|[-_](?:rs|js|ts|app|sdk|core|cli|libs?|hal|sys|pure|community))$/i, "");
  if (unsuffixed.replace(/[^a-z0-9]+/g, "").length >= 3) s = unsuffixed;
  return s.replace(/[^a-z0-9]+/g, "");
}

function tokenize(raw: string): string[] {
  return raw
    .trim()
    .toLowerCase()
    .replace(/^@[^/]+\//, "")
    .replace(/\.git$/i, "")
    .split(/[^a-z0-9]+/)
    .filter((t) => t.length >= 3 && !STOP_TOKENS.has(t));
}

// --- Stage 1: eza tree parsing & discovery ---------------------------------------------------------------------------

/** Parses `eza -D -T --no-quotes --icons=never --color=never -a` output lines into absolute directory paths. */
export function parseEzaTree(output: string, baseDir: string): string[] {
  const out: string[] = [];
  const stack: string[] = [baseDir];
  const branchRe = /^((?:│   |    )*)(?:├── |└── )(.+)$/;

  for (const rawLine of output.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    if (!line) continue;
    const m = branchRe.exec(line);
    if (!m) continue;
    const depth = m[1]!.length / 4 + 1;
    const rawName = m[2]!.replace(/\s+->\s+.*$/, "").trim();
    if (!rawName || rawName === "." || rawName === "..") continue;
    stack[depth] = rawName;
    stack.length = depth + 1;
    out.push(join(...stack));
  }
  return out;
}

function walkDirsFallback(root: string, depth: number): string[] {
  const out: string[] = [];
  const ignore = new Set(EZA_IGNORE.split("|"));
  const visit = (dir: string, level: number) => {
    if (level > depth) return;
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (!e.isDirectory() || ignore.has(e.name)) continue;
      const full = join(dir, e.name);
      out.push(full);
      visit(full, level + 1);
    }
  };
  visit(root, 1);
  return out;
}

async function spawnText(cmd: string[]): Promise<{ code: number; out: string }> {
  const proc = Bun.spawn(cmd, { stdout: "pipe", stderr: "ignore", windowsHide: true });
  const out = await new Response(proc.stdout).text();
  const code = await proc.exited;
  return { code, out };
}

export async function discoverDirsWithEza(targetDir: string, depth: number, ezaBin?: string): Promise<string[]> {
  if (!ezaBin) return walkDirsFallback(targetDir, depth);
  const { code, out } = await spawnText([
    ezaBin, "-D", "-T", "-L", String(depth), "--no-quotes", "--icons=never", "--color=never", "-a", "-I", EZA_IGNORE, targetDir,
  ]);
  if (code !== 0) return walkDirsFallback(targetDir, depth);
  return parseEzaTree(out, targetDir);
}

// --- Stage 2: rg manifest & vendor file discovery --------------------------------------------------------------------

export interface ManifestRecord {
  repo: string;
  manifestPath: string;
  relPath: string;
  absPath: string;
  vendorRoot: string;
  kind: "submodule" | "fork";
  name: string;
  gitSlug?: string;
  repoLeaf?: string;
  branch?: string;
  sha?: string;
}

export function parseGitmodules(text: string): { path: string; url?: string; branch?: string }[] {
  const out: { path: string; url?: string; branch?: string }[] = [];
  let cur: { path?: string; url?: string; branch?: string } | null = null;
  const flush = () => {
    if (cur?.path) out.push({ path: posix(cur.path), url: cur.url, branch: cur.branch });
  };
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith(";")) continue;
    if (line.startsWith("[submodule ")) {
      flush();
      cur = {};
      continue;
    }
    if (!cur) continue;
    const kv = /^([A-Za-z0-9_.-]+)\s*=\s*(.+)$/.exec(line);
    if (!kv) continue;
    const key = kv[1]!.toLowerCase();
    const val = kv[2]!.trim().replace(/^"(.*)"$/, "$1");
    if (key === "path") cur.path = val;
    else if (key === "url") cur.url = val;
    else if (key === "branch") cur.branch = val;
  }
  flush();
  return out;
}

export function parseSubmodulesToml(text: string): { path: string; repo?: string; branch?: string; sha?: string }[] {
  try {
    const parsed = Bun.TOML.parse(text) as { submodule?: Array<{ path?: string; repo?: string; branch?: string; sha?: string }> };
    return (parsed.submodule ?? [])
      .filter((s) => typeof s.path === "string" && s.path.length > 0)
      .map((s) => ({ path: posix(s.path!), repo: s.repo, branch: s.branch, sha: s.sha }));
  } catch {
    return [];
  }
}

export function parseForksToml(text: string): { org?: string; forks: Array<{ path: string; name?: string; upstream?: string; branch?: string }> } {
  try {
    const parsed = Bun.TOML.parse(text) as {
      meta?: { org?: string };
      fork?: Array<{ path?: string; name?: string; upstream?: string; branch?: string }>;
    };
    return {
      org: parsed.meta?.org,
      forks: (parsed.fork ?? [])
        .filter((f) => typeof f.path === "string" && f.path.length > 0)
        .map((f) => ({ path: posix(f.path!), name: f.name, upstream: f.upstream, branch: f.branch })),
    };
  } catch {
    return { forks: [] };
  }
}

async function runRgFiles(args: string[], rgBin?: string): Promise<string[]> {
  if (!rgBin) return [];
  const { code, out } = await spawnText([rgBin, "--files", "--hidden", "--no-ignore", ...args]);
  if (code !== 0 && code !== 1) return [];
  return out.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);
}

function walkManifestsFallback(scanDir: string): string[] {
  const targets = new Set([".gitmodules", "submodules.toml", "forks.toml"]);
  const ignore = new Set(EZA_IGNORE.split("|"));
  const out: string[] = [];
  const visit = (dir: string, depth: number) => {
    if (depth > 8) return;
    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (ignore.has(e.name)) continue;
      const full = join(dir, e.name);
      if (e.isDirectory()) visit(full, depth + 1);
      else if (targets.has(e.name)) out.push(full);
    }
  };
  visit(scanDir, 0);
  return out;
}

function findVendorRootAncestors(relPath: string): string[] {
  const parts = posix(relPath).split("/").filter(Boolean);
  const roots: string[] = [];
  for (let i = 0; i < parts.length; i++) {
    if (isVendorSegment(parts[i]!)) roots.push(parts.slice(0, i + 1).join("/"));
  }
  return roots;
}

async function readUnitMetadata(unitDir: string): Promise<{
  pkgName?: string;
  upstreamRepo?: string;
  gitSlug?: string;
  repoLeaf?: string;
}> {
  let pkgName: string | undefined;
  let upstreamRepo: string | undefined;
  let gitSlug: string | undefined;
  let repoLeaf: string | undefined;

  const cargo = join(unitDir, "Cargo.toml");
  if (isFile(cargo)) {
    try {
      const text = await Bun.file(cargo).text();
      const parsed = Bun.TOML.parse(text) as { package?: { name?: string; repository?: string } };
      if (parsed.package?.name) pkgName = parsed.package.name;
      if (parsed.package?.repository) {
        const g = parseGitSlug(parsed.package.repository);
        if (g?.slug) upstreamRepo = g.slug;
      }
    } catch {}
  }

  const pkg = join(unitDir, "package.json");
  if (!pkgName && isFile(pkg)) {
    try {
      const parsed = JSON.parse(await Bun.file(pkg).text()) as { name?: string; repository?: string | { url?: string } };
      if (parsed.name) pkgName = parsed.name;
      const repoUrl = typeof parsed.repository === "string" ? parsed.repository : parsed.repository?.url;
      if (!upstreamRepo && repoUrl) {
        const g = parseGitSlug(repoUrl);
        if (g?.slug) upstreamRepo = g.slug;
      }
    } catch {}
  }

  const gomod = join(unitDir, "go.mod");
  if (!pkgName && isFile(gomod)) {
    try {
      const text = await Bun.file(gomod).text();
      const m = /^module\s+(\S+)/m.exec(text);
      if (m) {
        pkgName = m[1]!;
        const g = parseGitSlug(m[1]!);
        if (g?.slug) upstreamRepo = upstreamRepo ?? g.slug;
      }
    } catch {}
  }

  const gitConfig = join(unitDir, ".git", "config");
  if (isFile(gitConfig)) {
    try {
      const text = await Bun.file(gitConfig).text();
      const m = /\[remote\s+"origin"\][\s\S]*?url\s*=\s*(.+)/i.exec(text);
      if (m) {
        const g = parseGitSlug(m[1]!);
        if (g) {
          gitSlug = g.slug;
          repoLeaf = g.leaf;
        }
      }
    } catch {}
  }

  return { pkgName, upstreamRepo, gitSlug, repoLeaf };
}

function hasProjectMarker(dir: string): boolean {
  for (const name of MANIFEST_FILES) {
    if (isFile(join(dir, name))) return true;
  }
  if (isFile(join(dir, ".git")) || isDirectory(join(dir, ".git"))) return true;
  return false;
}

function hasDirectSubdirs(dir: string, allDirs: Set<string>): boolean {
  const prefix = `${posix(dir)}/`;
  for (const d of allDirs) {
    if (d.startsWith(prefix) && !d.slice(prefix.length).includes("/")) return true;
  }
  return false;
}

function hasDirectFiles(dir: string): boolean {
  try {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.isFile() && !/^readme/i.test(e.name)) return true;
    }
  } catch {
    return false;
  }
  return false;
}

// --- Discovery coordinator -------------------------------------------------------------------------------------------

async function mapConcurrent<T, R>(items: readonly T[], limit: number, fn: (item: T) => Promise<R>): Promise<R[]> {
  const results = new Array<R>(items.length);
  let idx = 0;
  const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (idx < items.length) {
      const i = idx++;
      results[i] = await fn(items[i]!);
    }
  });
  await Promise.all(workers);
  return results;
}

export async function collectVendorEntries(opts: ScanOptions = {}): Promise<{
  scanRoot: string;
  scannedRepos: string[];
  entries: VendorEntry[];
  tools: { eza: boolean; rg: boolean; fzf: boolean };
}> {
  const scanRoot = resolve(opts.scanRoot ?? defaultScanRoot());
  const depth = opts.depth ?? 6;
  const includeTests = opts.includeTests ?? false;
  const includeRootRepos = opts.includeRootRepos ?? true;
  const ezaBin = Bun.which("eza") ?? undefined;
  const rgBin = Bun.which("rg") ?? undefined;
  const fzfBin = opts.useFzf === false ? undefined : (Bun.which("fzf") ?? undefined);

  const repoDirs = opts.dirs?.length
    ? opts.dirs.map((d) => resolve(d)).filter(isDirectory)
    : readdirSync(scanRoot, { withFileTypes: true })
        .filter((d) => d.isDirectory() && !d.name.startsWith("."))
        .map((d) => join(scanRoot, d.name))
        .sort();

  const scannedRepos = repoDirs.map((d) => basename(d));

  // Run eza tree discovery concurrently across top-level repositories + rg file/manifest scans across all targets.
  const [ezaPerRepo, rgVendorFiles, rgManifestFiles] = await Promise.all([
    mapConcurrent(repoDirs, 6, (dir) => discoverDirsWithEza(dir, depth, ezaBin)),
    runRgFiles(
      [
        "-g", "**/{vendor,third_party,third-party,external,externals,External,Externals,extern}/**",
        "-g", "!**/.git/**", "-g", "!**/node_modules/**", "-g", "!**/target/**",
        "-g", "!**/.cache/**", "-g", "!**/.vercel/**", "-g", "!**/.next/**", "-g", "!**/.turbo/**",
        ...repoDirs,
      ],
      rgBin,
    ),
    rgBin
      ? runRgFiles(
          [
            "-g", "{.gitmodules,submodules.toml,forks.toml}",
            "-g", "!**/.git/**", "-g", "!**/node_modules/**", "-g", "!**/target/**", "-g", "!**/.cache/**",
            ...repoDirs,
          ],
          rgBin,
        )
      : Promise.resolve(repoDirs.flatMap(walkManifestsFallback)),
  ]);

  const allDirsSet = new Set<string>();
  for (const list of ezaPerRepo) for (const d of list) allDirsSet.add(posix(d));

  const dirFileCounts = new Map<string, number>();
  const deepVendorRoots = new Set<string>();

  for (const absFile of rgVendorFiles) {
    const p = posix(absFile);
    const repoDir = repoDirs.find((rd) => p.startsWith(`${posix(rd)}/`));
    if (!repoDir) continue;
    const relFile = p.slice(posix(repoDir).length + 1);
    if (!includeTests && isTestFixturePath(relFile)) continue;
    const parts = relFile.split("/");
    for (let i = 0; i < parts.length - 1; i++) {
      const cur = `${posix(repoDir)}/${parts.slice(0, i + 1).join("/")}`;
      dirFileCounts.set(cur, (dirFileCounts.get(cur) ?? 0) + 1);
      if (isVendorSegment(parts[i]!)) {
        deepVendorRoots.add(cur);
      }
    }
  }

  for (const d of allDirsSet) {
    if (!isVendorSegment(basename(d))) continue;
    const repoDir = repoDirs.find((rd) => d.startsWith(`${posix(rd)}/`));
    if (!repoDir) continue;
    const rel = d.slice(posix(repoDir).length + 1);
    if (!includeTests && isTestFixturePath(rel)) continue;
    deepVendorRoots.add(d);
  }

  const extraTrees = await mapConcurrent([...deepVendorRoots], 6, (vr) => discoverDirsWithEza(vr, 2, ezaBin));
  for (const list of extraTrees) for (const d of list) allDirsSet.add(posix(d));

  // Parse all discovered `.gitmodules`, `submodules.toml`, and `forks.toml`.
  const manifestMap = new Map<string, ManifestRecord>();
  const manifestDeclaredSet = new Set<string>();
  const parsedManifestContents = await mapConcurrent(rgManifestFiles, 8, async (mf) => ({
    mf: posix(mf),
    text: await Bun.file(mf).text().catch(() => ""),
  }));

  for (const { mf, text } of parsedManifestContents) {
    const repoDir = repoDirs.find((rd) => mf.startsWith(`${posix(rd)}/`));
    if (!repoDir) continue;
    const repo = basename(repoDir);
    const base = basename(mf);
    const manifestDir = posix(dirname(mf));
    const manifestRelPrefix = manifestDir === posix(repoDir) ? "" : `${manifestDir.slice(posix(repoDir).length + 1)}/`;
    if (!includeTests && isTestFixturePath(manifestRelPrefix)) continue;

    if (base === ".gitmodules") {
      for (const s of parseGitmodules(text)) {
        const relPath = posix(`${manifestRelPrefix}${s.path}`);
        const absPath = posix(`${posix(repoDir)}/${relPath}`);
        const parsedSlug = s.url ? parseGitSlug(s.url) : undefined;
        const roots = findVendorRootAncestors(relPath);
        const vendorRoot = roots.at(-1) ?? (manifestRelPrefix ? manifestRelPrefix.replace(/\/$/, "") : ".gitmodules");
        manifestDeclaredSet.add(absPath);
        manifestMap.set(absPath, {
          repo,
          manifestPath: mf,
          relPath,
          absPath,
          vendorRoot,
          kind: "submodule",
          name: basename(relPath),
          gitSlug: parsedSlug?.slug,
          repoLeaf: parsedSlug?.leaf,
          branch: s.branch,
        });
      }
    } else if (base === "submodules.toml") {
      for (const s of parseSubmodulesToml(text)) {
        const relPath = posix(`${manifestRelPrefix}${s.path}`);
        const absPath = posix(`${posix(repoDir)}/${relPath}`);
        const parsedSlug = s.repo ? parseGitSlug(s.repo) : undefined;
        const roots = findVendorRootAncestors(relPath);
        const vendorRoot = roots.at(-1) ?? "submodules.toml";
        manifestDeclaredSet.add(absPath);
        manifestMap.set(absPath, {
          repo,
          manifestPath: mf,
          relPath,
          absPath,
          vendorRoot,
          kind: "submodule",
          name: basename(relPath),
          gitSlug: parsedSlug?.slug,
          repoLeaf: parsedSlug?.leaf,
          branch: s.branch,
          sha: s.sha,
        });
      }
    } else if (base === "forks.toml") {
      const f = parseForksToml(text);
      for (const item of f.forks) {
        const relPath = posix(`${manifestRelPrefix}${item.path}`);
        const absPath = posix(`${posix(repoDir)}/${relPath}`);
        const parsedSlug = item.upstream ? parseGitSlug(item.upstream) : undefined;
        const roots = findVendorRootAncestors(relPath);
        const vendorRoot = roots.at(-1) ?? "forks.toml";
        manifestDeclaredSet.add(absPath);
        manifestMap.set(absPath, {
          repo,
          manifestPath: mf,
          relPath,
          absPath,
          vendorRoot,
          kind: "fork",
          name: item.name || basename(relPath),
          gitSlug: parsedSlug?.slug,
          repoLeaf: parsedSlug?.leaf,
          branch: item.branch,
        });
      }
    }
  }

  // Extract vendor unit directories under every vendor/third_party root, unwrapping pure category folders (e.g. cloudflare/, vercel/).
  const unitMap = new Map<string, VendorEntry>();
  const sortedRoots = [...deepVendorRoots].sort();

  for (const vr of sortedRoots) {
    const repoDir = repoDirs.find((rd) => vr.startsWith(`${posix(rd)}/`));
    if (!repoDir) continue;
    const repo = basename(repoDir);
    const vrRel = vr.slice(posix(repoDir).length + 1);
    if (!includeTests && isTestFixturePath(vrRel)) continue;

    const children = [...allDirsSet].filter((d) => posix(dirname(d)) === vr).sort();
    const resolveChildren = (dir: string, depthLeft: number): string[] => {
      if (depthLeft <= 0) return [dir];
      const isDeclared = manifestDeclaredSet.has(dir);
      const hasProj = hasProjectMarker(dir);
      const hasKids = hasDirectSubdirs(dir, allDirsSet);
      const directFiles = hasDirectFiles(dir);
      if (!isDeclared && !hasProj && !directFiles && hasKids) {
        const sub = [...allDirsSet].filter((d) => posix(dirname(d)) === dir).sort();
        return sub.flatMap((c) => resolveChildren(c, depthLeft - 1));
      }
      return [dir];
    };

    const resolvedUnits = children.flatMap((c) => resolveChildren(c, 2));
    for (const absPath of resolvedUnits) {
      const relPath = absPath.slice(posix(repoDir).length + 1);
      const isTest = isTestFixturePath(relPath);
      if (!includeTests && isTest) continue;
      const man = manifestMap.get(absPath);
      const discoveredBy: VendorEntry["discoveredBy"] = ["eza"];
      if ((dirFileCounts.get(absPath) ?? 0) > 0) discoveredBy.push("rg");
      if (man) discoveredBy.push("manifest");

      unitMap.set(absPath, {
        id: `${repo}:${relPath}`,
        repo,
        relPath,
        absPath,
        vendorRoot: `${repo}:${vrRel}`,
        kind: man?.kind ?? "vendor-dir",
        name: man?.name ?? basename(absPath),
        gitSlug: man?.gitSlug,
        repoLeaf: man?.repoLeaf,
        branch: man?.branch,
        sha: man?.sha,
        fileCount: dirFileCounts.get(absPath) ?? 0,
        isTestFixture: isTest,
        discoveredBy,
      });
    }
  }

  for (const [absPath, man] of manifestMap) {
    if (unitMap.has(absPath)) continue;
    const isTest = isTestFixturePath(man.relPath);
    if (!includeTests && isTest) continue;
    const diskExists = isDirectory(absPath);
    unitMap.set(absPath, {
      id: `${man.repo}:${man.relPath}`,
      repo: man.repo,
      relPath: man.relPath,
      absPath,
      vendorRoot: `${man.repo}:${man.vendorRoot}`,
      kind: man.kind,
      name: man.name,
      gitSlug: man.gitSlug,
      repoLeaf: man.repoLeaf,
      branch: man.branch,
      sha: man.sha,
      fileCount: dirFileCounts.get(absPath) ?? 0,
      isTestFixture: isTest,
      discoveredBy: diskExists ? ["eza", "manifest"] : ["manifest"],
    });
  }

  if (includeRootRepos && !opts.dirs?.length) {
    for (const dir of repoDirs) {
      const p = posix(dir);
      const name = basename(p);
      unitMap.set(p, {
        id: `${name}:.`,
        repo: name,
        relPath: ".",
        absPath: p,
        vendorRoot: `${name}:.`,
        kind: "root-checkout",
        name,
        fileCount: 1,
        isTestFixture: false,
        discoveredBy: ["root"],
      });
    }
  }

  const allEntries = [...unitMap.values()];
  await mapConcurrent(allEntries, 16, async (entry) => {
    if (!isDirectory(entry.absPath)) return;
    const meta = await readUnitMetadata(entry.absPath);
    if (meta.pkgName) entry.pkgName = meta.pkgName;
    if (meta.upstreamRepo) entry.upstreamRepo = meta.upstreamRepo;
    if (!entry.gitSlug && meta.gitSlug) entry.gitSlug = meta.gitSlug;
    if (!entry.repoLeaf && meta.repoLeaf) entry.repoLeaf = meta.repoLeaf;
  });

  allEntries.sort((a, b) => a.id.localeCompare(b.id));
  return {
    scanRoot,
    scannedRepos,
    entries: allEntries,
    tools: { eza: Boolean(ezaBin), rg: Boolean(rgBin), fzf: Boolean(fzfBin) },
  };
}

// --- Stage 3: Exact & fuzzy deduplication ----------------------------------------------------------------------------

function trigrams(s: string): Set<string> {
  const padded = `  ${s} `;
  const set = new Set<string>();
  for (let i = 0; i <= padded.length - 3; i++) set.add(padded.slice(i, i + 3));
  return set;
}

export function trigramDice(a: string, b: string): number {
  if (!a || !b) return 0;
  if (a === b) return 1;
  const ta = trigrams(a);
  const tb = trigrams(b);
  let overlap = 0;
  for (const t of ta) if (tb.has(t)) overlap++;
  return (2 * overlap) / (ta.size + tb.size);
}

export function levenshteinSimilarity(a: string, b: string): number {
  if (a === b) return 1;
  const maxLen = Math.max(a.length, b.length);
  if (maxLen === 0) return 1;
  const prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  const curr = new Array<number>(b.length + 1);
  for (let i = 1; i <= a.length; i++) {
    curr[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(curr[j - 1]! + 1, prev[j]! + 1, prev[j - 1]! + cost);
    }
    for (let j = 0; j <= b.length; j++) prev[j] = curr[j]!;
  }
  return 1 - prev[b.length]! / maxLen;
}

export function fuzzyPairSimilarity(aRaw: string, bRaw: string): { score: number; reason: string } {
  const aCan = canonicalName(aRaw);
  const bCan = canonicalName(bRaw);
  if (!aCan || !bCan || GENERIC_NAMES.has(aCan) || GENERIC_NAMES.has(bCan)) return { score: 0, reason: "" };
  if (aCan === bCan) return { score: 1, reason: "canonical-name" };

  const aStem = normalizeStem(aRaw);
  const bStem = normalizeStem(bRaw);
  if (aStem.length >= 4 && aStem === bStem) return { score: 0.95, reason: "normalized-stem" };

  const [shortStr, longStr] = aCan.length <= bCan.length ? [aCan, bCan] : [bCan, aCan];
  const ratio = shortStr.length / longStr.length;
  if (shortStr.length >= 4 && ratio >= 0.45) {
    const tokens = longStr.split("-");
    if (tokens.includes(shortStr) || longStr.startsWith(`${shortStr}-`) || longStr.endsWith(`-${shortStr}`)) {
      return { score: Number((0.80 + 0.14 * ratio).toFixed(3)), reason: "token-prefix" };
    }
  }
  if (shortStr.length >= 5 && ratio >= 0.45 && longStr.replaceAll("-", "").includes(shortStr.replaceAll("-", ""))) {
    return { score: Number((0.78 + 0.14 * ratio).toFixed(3)), reason: "substring-stem" };
  }

  const ta = tokenize(aRaw);
  const tb = tokenize(bRaw);
  if (ta.length && tb.length) {
    const setB = new Set(tb);
    const inter = ta.filter((t) => setB.has(t) && t.length >= 4).length;
    const jaccard = inter / new Set([...ta, ...tb]).size;
    if (jaccard >= 0.6 && inter >= 2) {
      return { score: Number((0.76 + 0.2 * jaccard).toFixed(3)), reason: "token-overlap" };
    }
  }

  // Fast length/trigram pre-filter before running Levenshtein matrix.
  if (ratio < 0.55) return { score: 0, reason: "" };
  const dice = trigramDice(aCan, bCan);
  if (dice < 0.45) return { score: 0, reason: "" };
  const lev = levenshteinSimilarity(aCan, bCan);
  const blended = Number((0.55 * dice + 0.45 * lev).toFixed(3));
  return { score: blended, reason: "trigram-levenshtein" };
}

class UnionFind {
  private parent: number[];
  constructor(n: number) {
    this.parent = Array.from({ length: n }, (_, i) => i);
  }
  find(x: number): number {
    while (this.parent[x] !== x) {
      this.parent[x] = this.parent[this.parent[x]!]!;
      x = this.parent[x]!;
    }
    return x;
  }
  union(a: number, b: number): void {
    const ra = this.find(a);
    const rb = this.find(b);
    if (ra !== rb) this.parent[rb] = ra;
  }
}

function entryLabels(e: VendorEntry): string[] {
  const labels = new Set<string>();
  const add = (s?: string) => {
    if (!s) return;
    const can = canonicalName(s);
    if (can.length >= 3 && !GENERIC_NAMES.has(can)) labels.add(s);
  };
  add(e.name);
  add(e.repoLeaf);
  add(e.pkgName);
  return [...labels];
}

function isDistinctPkgKey(pkg: string): string | undefined {
  const norm = pkg.trim().toLowerCase();
  const can = canonicalName(norm);
  if (can.length < 3 || GENERIC_NAMES.has(can)) return undefined;
  return norm;
}

export function findDuplicates(entries: VendorEntry[], threshold = 0.78): { exactGroups: DuplicateGroup[]; fuzzyGroups: DuplicateGroup[] } {
  const n = entries.length;
  const ufExact = new UnionFind(n);
  const reasonsByRoot = new Map<number, Set<string>>();

  const exactIndex = new Map<string, { idx: number; reason: string }[]>();
  const indexKey = (key: string, idx: number, reason: string) => {
    (exactIndex.get(key) ?? exactIndex.set(key, []).get(key)!).push({ idx, reason });
  };

  for (let i = 0; i < n; i++) {
    const e = entries[i]!;
    if (e.gitSlug) indexKey(`slug:${e.gitSlug}`, i, "git-slug");
    const dirCan = canonicalName(e.name);
    if (dirCan.length >= 2 && !GENERIC_NAMES.has(dirCan)) indexKey(`name:${dirCan}`, i, "dir-name");
    if (e.repoLeaf) {
      const leafCan = canonicalName(e.repoLeaf);
      if (leafCan.length >= 2 && !GENERIC_NAMES.has(leafCan)) indexKey(`name:${leafCan}`, i, "repo-leaf");
    }
    if (e.pkgName) {
      const pkgKey = isDistinctPkgKey(e.pkgName);
      if (pkgKey) indexKey(`pkg:${pkgKey}`, i, "pkg-name");
    }
  }

  for (const list of exactIndex.values()) {
    if (list.length < 2) continue;
    const first = list[0]!;
    for (let k = 1; k < list.length; k++) {
      ufExact.union(first.idx, list[k]!.idx);
    }
  }

  // Also link entries across different vendorRoots that share `upstreamRepo` (e.g. craft-anything/vendor/wgpu-hal -> gfx-rs/wgpu).
  const upstreamIndex = new Map<string, number[]>();
  for (let i = 0; i < n; i++) {
    const up = entries[i]!.upstreamRepo;
    if (up) (upstreamIndex.get(up) ?? upstreamIndex.set(up, []).get(up)!).push(i);
  }
  const extraReasons = new Map<number, string>();
  for (const [, idxs] of upstreamIndex) {
    if (idxs.length < 2) continue;
    const roots = new Set(idxs.map((i) => entries[i]!.vendorRoot));
    if (roots.size < 2) continue;
    for (let k = 1; k < idxs.length; k++) {
      if (entries[idxs[k]!]!.vendorRoot !== entries[idxs[0]!]!.vendorRoot) {
        ufExact.union(idxs[0]!, idxs[k]!);
        extraReasons.set(ufExact.find(idxs[0]!), "upstream-repo");
      }
    }
  }

  const buckets = new Map<number, VendorEntry[]>();
  for (let i = 0; i < n; i++) {
    const r = ufExact.find(i);
    (buckets.get(r) ?? buckets.set(r, []).get(r)!).push(entries[i]!);
  }

  for (const list of exactIndex.values()) {
    if (list.length < 2) continue;
    const r = ufExact.find(list[0]!.idx);
    const set = reasonsByRoot.get(r) ?? reasonsByRoot.set(r, new Set()).get(r)!;
    for (const item of list) set.add(item.reason);
  }
  for (const [idx, reason] of extraReasons) {
    const r = ufExact.find(idx);
    (reasonsByRoot.get(r) ?? reasonsByRoot.set(r, new Set()).get(r)!).add(reason);
  }

  const exactGroups: DuplicateGroup[] = [];
  const exactBucketOfEntry = new Map<string, number>();

  for (const [rootIdx, members] of buckets) {
    if (members.length < 2) continue;
    if (!members.some((m) => m.kind !== "root-checkout")) continue;
    const sorted = [...members].sort((a, b) => a.id.localeCompare(b.id));
    const names = sorted.map((m) => canonicalName(m.repoLeaf ?? m.name)).filter((x) => !GENERIC_NAMES.has(x));
    const key = names.sort((a, b) => a.length - b.length)[0] ?? canonicalName(sorted[0]!.name);
    for (const m of sorted) exactBucketOfEntry.set(m.id, rootIdx);
    exactGroups.push({
      key,
      tier: "exact",
      score: 1,
      reasons: [...(reasonsByRoot.get(rootIdx) ?? [])].sort(),
      repos: [...new Set(sorted.map((m) => m.repo))].sort(),
      entries: sorted,
    });
  }
  exactGroups.sort((a, b) => b.entries.length - a.entries.length || a.key.localeCompare(b.key));

  // Complete-linkage fuzzy clustering (prevents transitive A~B~C snowball chains).
  const labelsPerEntry = entries.map(entryLabels);
  const simMatrix = new Map<string, { score: number; reason: string }>();
  const pairs: { i: number; j: number; score: number; reason: string }[] = [];

  for (let i = 0; i < n; i++) {
    const ei = entries[i]!;
    if (labelsPerEntry[i]!.length === 0) continue;
    for (let j = i + 1; j < n; j++) {
      const ej = entries[j]!;
      if (labelsPerEntry[j]!.length === 0) continue;
      const bi = exactBucketOfEntry.get(ei.id);
      const bj = exactBucketOfEntry.get(ej.id);
      if (bi !== undefined && bi === bj) continue;
      if (ei.vendorRoot === ej.vendorRoot && ei.kind !== "root-checkout" && ej.kind !== "root-checkout") continue;

      let best = { score: 0, reason: "" };
      for (const la of labelsPerEntry[i]!) {
        for (const lb of labelsPerEntry[j]!) {
          const sim = fuzzyPairSimilarity(la, lb);
          if (sim.score > best.score) best = sim;
        }
      }
      if (best.score >= threshold) {
        simMatrix.set(`${i}:${j}`, best);
        pairs.push({ i, j, ...best });
      }
    }
  }

  pairs.sort((a, b) => b.score - a.score);
  const assigned = new Set<number>();
  const fuzzyGroups: DuplicateGroup[] = [];

  for (const p of pairs) {
    if (assigned.has(p.i) || assigned.has(p.j)) continue;
    const cluster = [p.i, p.j];
    const reasons = new Set<string>([p.reason]);
    let minScore = p.score;

    // Add candidates that match EVERY existing member in the cluster (complete linkage).
    for (let k = 0; k < n; k++) {
      if (assigned.has(k) || k === p.i || k === p.j) continue;
      let fitsAll = true;
      let localMin = 1;
      const localReasons: string[] = [];
      for (const m of cluster) {
        const bm = exactBucketOfEntry.get(entries[m]!.id);
        const bk = exactBucketOfEntry.get(entries[k]!.id);
        if (bm !== undefined && bm === bk) continue;
        const pairKey = m < k ? `${m}:${k}` : `${k}:${m}`;
        const hit = simMatrix.get(pairKey);
        if (!hit || hit.score < threshold) {
          fitsAll = false;
          break;
        }
        if (hit.score < localMin) localMin = hit.score;
        localReasons.push(hit.reason);
      }
      if (fitsAll) {
        cluster.push(k);
        if (localMin < minScore) minScore = localMin;
        for (const r of localReasons) reasons.add(r);
      }
    }

    const members = cluster.map((idx) => entries[idx]!).sort((a, b) => a.id.localeCompare(b.id));
    if (!members.some((m) => m.kind !== "root-checkout")) continue;
    for (const idx of cluster) assigned.add(idx);
    const key = members.map((m) => canonicalName(m.name)).sort((a, b) => a.length - b.length)[0] ?? members[0]!.name;
    fuzzyGroups.push({
      key,
      tier: "fuzzy",
      score: minScore,
      reasons: [...reasons].sort(),
      repos: [...new Set(members.map((m) => m.repo))].sort(),
      entries: members,
    });
  }
  fuzzyGroups.sort((a, b) => b.score - a.score || b.entries.length - a.entries.length || a.key.localeCompare(b.key));

  return { exactGroups, fuzzyGroups };
}

// --- Stage 4: fzf filter & pure-Bun fuzzy query scorer ----------------------------------------------------------------

/** Pure-Bun fuzzy subsequence scorer (fallback when `fzf` binary is absent or disabled). */
export function fuzzySubsequenceScore(query: string, candidate: string): number {
  const q = query.trim().toLowerCase();
  if (!q) return 1;
  const text = candidate.toLowerCase();
  const terms = q.split(/\s+/).filter(Boolean);
  let total = 0;
  for (const term of terms) {
    let qi = 0;
    let consecutive = 0;
    let score = 0;
    for (let ti = 0; ti < text.length && qi < term.length; ti++) {
      if (text[ti] === term[qi]) {
        const prev = ti === 0 ? "/" : text[ti - 1]!;
        const boundary = /[/_\-.: ]/.test(prev) ? 6 : 0;
        consecutive++;
        score += 10 + boundary + consecutive * 3;
        qi++;
      } else {
        consecutive = 0;
      }
    }
    if (qi < term.length) return 0;
    total += score / Math.max(1, text.length * 0.15);
  }
  return Number(total.toFixed(3));
}

export function filterItemsFuzzy<T>(items: readonly T[], toText: (item: T) => string, query: string, useFzfBin = true): T[] {
  const q = query.trim();
  if (!q) return [...items];

  const fzfBin = useFzfBin ? (Bun.which("fzf") ?? undefined) : undefined;
  if (fzfBin) {
    const lines = items.map((item, idx) => `${idx}\t${toText(item).replaceAll(/\r?\n/g, " ")}`);
    const res = Bun.spawnSync([fzfBin, "--filter", q, "--delimiter", "\t", "--nth", "2.."], {
      stdin: new TextEncoder().encode(lines.join("\n")),
      stdout: "pipe",
      stderr: "ignore",
      windowsHide: true,
    });
    if (res.exitCode === 0 || res.exitCode === 1) {
      const matched: T[] = [];
      for (const line of res.stdout.toString().split(/\r?\n/)) {
        if (!line) continue;
        const tab = line.indexOf("\t");
        if (tab <= 0) continue;
        const idx = Number(line.slice(0, tab));
        if (Number.isInteger(idx) && items[idx]) matched.push(items[idx]!);
      }
      return matched;
    }
  }

  return items
    .map((item) => ({ item, score: fuzzySubsequenceScore(q, toText(item)) }))
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .map((x) => x.item);
}

export async function runAudit(opts: ScanOptions = {}): Promise<VendorDedupReport> {
  const t0 = performance.now();
  const { scanRoot, scannedRepos, entries, tools } = await collectVendorEntries(opts);
  const threshold = opts.threshold ?? 0.78;
  let { exactGroups, fuzzyGroups } = findDuplicates(entries, threshold);

  let filteredEntries = entries;
  if (opts.fuzzyQuery?.trim()) {
    const q = opts.fuzzyQuery.trim();
    const useFzfBin = opts.useFzf !== false;
    const groupText = (g: DuplicateGroup) =>
      `${g.key} ${g.reasons.join(" ")} ${g.entries.map((e) => `${e.id} ${e.gitSlug ?? ""} ${e.pkgName ?? ""}`).join(" ")}`;
    exactGroups = filterItemsFuzzy(exactGroups, groupText, q, useFzfBin);
    fuzzyGroups = filterItemsFuzzy(fuzzyGroups, groupText, q, useFzfBin);
    filteredEntries = filterItemsFuzzy(
      entries,
      (e) => `${e.id} ${e.name} ${e.gitSlug ?? ""} ${e.pkgName ?? ""} ${e.absPath}`,
      q,
      useFzfBin,
    );
  }

  const vendorRoots = new Set(entries.filter((e) => e.kind !== "root-checkout").map((e) => e.vendorRoot)).size;
  return {
    scanRoot,
    scannedRepos,
    generatedAt: new Date().toISOString(),
    tools,
    stats: {
      totalEntries: filteredEntries.length,
      vendorRoots,
      exactGroups: exactGroups.length,
      exactDuplicateEntries: exactGroups.reduce((acc, g) => acc + g.entries.length, 0),
      fuzzyGroups: fuzzyGroups.length,
      fuzzyDuplicateEntries: fuzzyGroups.reduce((acc, g) => acc + g.entries.length, 0),
      durationMs: Math.round(performance.now() - t0),
    },
    exactGroups,
    fuzzyGroups,
    entries: filteredEntries,
  };
}

// --- Formatting & Interactive fzf UI ---------------------------------------------------------------------------------

export function renderMarkdown(r: VendorDedupReport): string {
  const lines = [
    "# Vendor & Third-Party Deduplication Audit",
    "",
    `- Scan root: \`${r.scanRoot}\` (${r.scannedRepos.length} checkouts, ${r.stats.durationMs} ms)`,
    `- Tools detected: eza=${r.tools.eza}, rg=${r.tools.rg}, fzf=${r.tools.fzf}`,
    `- Vendor/third-party entries: ${r.stats.totalEntries} across ${r.stats.vendorRoots} container roots`,
    `- Exact duplicate groups: ${r.stats.exactGroups} (${r.stats.exactDuplicateEntries} entries)`,
    `- Fuzzy near-duplicate groups: ${r.stats.fuzzyGroups} (${r.stats.fuzzyDuplicateEntries} entries)`,
    "",
    "## Exact Duplicates",
    "",
  ];
  for (const g of r.exactGroups) {
    lines.push(`### \`${g.key}\` (${g.entries.length} occurrences across ${g.repos.length} repo(s) — ${g.reasons.join(", ")})`);
    for (const e of g.entries) {
      const meta = [e.kind, e.gitSlug, e.pkgName ? `pkg:${e.pkgName}` : undefined, `${e.fileCount} files`].filter(Boolean).join(" · ");
      lines.push(`- \`${e.id}\` (${meta})`);
    }
    lines.push("");
  }
  lines.push("## Fuzzy Near-Duplicates", "");
  for (const g of r.fuzzyGroups) {
    lines.push(`### \`${g.key}\` (score=${g.score.toFixed(2)}, ${g.entries.length} occurrences — ${g.reasons.join(", ")})`);
    for (const e of g.entries) {
      const meta = [e.kind, e.gitSlug, e.pkgName ? `pkg:${e.pkgName}` : undefined, `${e.fileCount} files`].filter(Boolean).join(" · ");
      lines.push(`- \`${e.id}\` (${meta})`);
    }
    lines.push("");
  }
  return lines.join("\n");
}

export function renderTerminalSummary(r: VendorDedupReport, opts: { ezaTree?: boolean; showAll?: boolean } = {}): string {
  const lines = [
    `vendor-dedup: ${r.scannedRepos.length} repos under ${r.scanRoot} (${r.stats.durationMs} ms) [eza=${r.tools.eza ? "ok" : "fallback"} rg=${r.tools.rg ? "ok" : "off"} fzf=${r.tools.fzf ? "ok" : "fallback"}]`,
    `entries=${r.stats.totalEntries} roots=${r.stats.vendorRoots} exactGroups=${r.stats.exactGroups} (${r.stats.exactDuplicateEntries} hits) fuzzyGroups=${r.stats.fuzzyGroups} (${r.stats.fuzzyDuplicateEntries} hits)`,
    "",
    "=== Exact Duplicate Vendor / Third-Party Groups ===",
  ];
  for (const g of r.exactGroups) {
    lines.push(`  [${g.entries.length}x | ${g.reasons.join("+")}] ${g.key}`);
    for (const e of g.entries) {
      const slug = e.gitSlug ? ` -> ${e.gitSlug}` : "";
      lines.push(`    ${opts.ezaTree ? "├── " : "- "}${e.id} (${e.kind}, ${e.fileCount} files)${slug}`);
    }
  }
  if (r.fuzzyGroups.length) {
    lines.push("", "=== Fuzzy Near-Duplicate Families ===");
    for (const g of r.fuzzyGroups) {
      lines.push(`  [score=${g.score.toFixed(2)} | ${g.entries.length}x | ${g.reasons.join("+")}] ${g.key}`);
      for (const e of g.entries) {
        const slug = e.gitSlug ? ` -> ${e.gitSlug}` : "";
        lines.push(`    ${opts.ezaTree ? "├── " : "- "}${e.id} (${e.kind}, ${e.fileCount} files)${slug}`);
      }
    }
  }
  if (opts.showAll) {
    lines.push("", "=== All Discovered Entries ===");
    for (const e of r.entries) lines.push(`  ${e.id} [${e.kind}] ${e.gitSlug ?? ""}`);
  }
  return lines.join("\n");
}

function launchFzfInteractive(r: VendorDedupReport): number {
  const fzfBin = Bun.which("fzf");
  if (!fzfBin) {
    console.error("fzf binary not found on PATH; pass --fuzzy <query> for built-in fuzzy filtering.");
    return 1;
  }
  const rows: string[] = [];
  for (const g of [...r.exactGroups, ...r.fuzzyGroups]) {
    const tag = g.tier === "exact" ? `EXACT(${g.entries.length}x)` : `FUZZY(${g.score.toFixed(2)})`;
    for (const e of g.entries) {
      rows.push(`${tag}\t${g.key}\t${e.id}\t${e.gitSlug ?? e.pkgName ?? "-"}\t${e.absPath}`);
    }
  }
  if (!rows.length) {
    console.log("No duplicate vendor/third_party entries found.");
    return 0;
  }
  const previewCmd = Bun.which("eza")
    ? "eza -T -L 2 --icons=auto --color=always {5} 2>nul || dir {5}"
    : "rg --files --max-depth 2 {5}";
  const proc = Bun.spawnSync(
    [
      fzfBin,
      "--delimiter", "\t",
      "--with-nth", "1,2,3,4",
      "--header", `vendor-dedup (${r.stats.exactGroups} exact, ${r.stats.fuzzyGroups} fuzzy) — Enter to print path`,
      "--preview", previewCmd,
      "--preview-window", "right:45%:wrap",
    ],
    {
      stdin: new TextEncoder().encode(rows.join("\n")),
      stdout: "inherit",
      stderr: "inherit",
    },
  );
  return proc.exitCode === 130 ? 0 : proc.exitCode;
}

// --- CLI -------------------------------------------------------------------------------------------------------------

function printHelp(): void {
  console.log([
    "Usage: bun tools/vendor-dedup.ts [options] [<dir>...]",
    "",
    "Recursively scan all checkouts under SRC_ROOT (workspace parent) with eza + rg + fuzzy matching",
    "to locate duplicated vendor / third_party / external trees and submodule/fork declarations.",
    "",
    "Options:",
    "  --root <dir>        Workspace parent directory (default: SRC_ROOT or parent of repo root)",
    "  --fuzzy, -q <query> Fuzzy filter duplicate groups & entries (uses fzf --filter when installed)",
    "  --fzf, -i           Interactive fzf browser with live eza tree preview",
    "  --threshold <0..1>  Fuzzy near-duplicate threshold (default: 0.78)",
    "  --depth <n>         eza tree recursion depth per repo (default: 6)",
    "  --include-tests     Include test/fixture vendor directories",
    "  --no-root-repos     Do not cross-match top-level checkouts in scan root",
    "  --no-fzf            Force pure-Bun fuzzy scorer instead of spawning fzf",
    "  --eza-tree          Format terminal groups as tree branches",
    "  --all               Include non-duplicate entries in summary output",
    "  --json              Print full JSON report to stdout",
    "  --no-write          Skip writing .private/vendor-dedup.{json,md}",
    "  --help, -h          Show this help",
  ].join("\n"));
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.includes("--help") || args.includes("-h")) {
    printHelp();
    process.exit(0);
  }
  const flagValue = (names: string[]): string | undefined => {
    for (let i = 0; i < args.length; i++) {
      if (names.includes(args[i]!) && args[i + 1]) return args[i + 1];
    }
    return undefined;
  };
  const valueFlags = new Set(["--root", "--fuzzy", "-q", "--threshold", "--depth"]);
  const positional: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i]!;
    if (valueFlags.has(a)) {
      i++;
      continue;
    }
    if (!a.startsWith("-")) positional.push(a);
  }

  const thresholdRaw = flagValue(["--threshold"]);
  const depthRaw = flagValue(["--depth"]);
  const report = await runAudit({
    scanRoot: flagValue(["--root"]),
    dirs: positional.length ? positional : undefined,
    fuzzyQuery: flagValue(["--fuzzy", "-q"]),
    threshold: thresholdRaw ? Number(thresholdRaw) : undefined,
    depth: depthRaw ? Number(depthRaw) : undefined,
    includeTests: args.includes("--include-tests"),
    includeRootRepos: !args.includes("--no-root-repos"),
    useFzf: !args.includes("--no-fzf"),
  });

  if (!args.includes("--no-write")) {
    await Bun.write(privatePath("vendor-dedup.json", repoRoot, true), `${JSON.stringify(report, null, 2)}\n`);
    await Bun.write(privatePath("vendor-dedup.md", repoRoot, true), renderMarkdown(report));
  }

  if (args.includes("--fzf") || args.includes("-i")) {
    process.exit(launchFzfInteractive(report));
  } else if (args.includes("--json")) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(renderTerminalSummary(report, { ezaTree: args.includes("--eza-tree"), showAll: args.includes("--all") }));
  }
}
