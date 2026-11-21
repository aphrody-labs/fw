#!/usr/bin/env bun
/**
 * Cross-workspace code duplication & copied-upstream detector across all checkouts under `SRC_ROOT` (workspace parent).
 *
 * Combines:
 *   - `git ls-files -s` (recursive across submodules): exact Git blob SHA-1 matching between first-party code and upstream/vendor trees
 *   - `ast-grep outline` + `web-tree-sitter` (`tools/ts.ts`) + `difft` (Difftastic): Type-1 exact & Type-2 alpha-renamed AST clones
 *   - Code graph (`packages/core` schema: symbols, imports, call refs): duplicate exported symbols & parallel module subgraphs
 *   - `rg` + `eza` + rolling k-gram text block winnowing + `fzf` fuzzy filtering & interactive TUI
 *
 * Usage:
 *   bun tools/code-dedup.ts [options] [<dir>...]
 *   ay code-dedup [options] [<dir>...]
 */
import { mkdirSync, readdirSync } from "node:fs";
import { basename, dirname, extname, join, resolve } from "node:path";
import { openStore, writeFiles } from "../packages/core/src/store.ts";
import type { IndexedFile, Span } from "../packages/core/src/ffi.ts";
import { normalizeSql } from "./dedup.ts";
import { privatePath, readAy, repoRoot } from "./local.ts";
import { parse as parseTreeSitter, type Lang as TsLang } from "./ts.ts";
import {
  defaultScanRoot,
  filterItemsFuzzy,
  isDirectory,
  isFile,
  isTestFixturePath,
  parseEzaTree,
  parseForksToml,
  parseGitmodules,
  parseGitSlug,
  parseSubmodulesToml,
  posix,
} from "./vendor-dedup.ts";

export interface GitIndexedFile {
  repo: string;
  relPath: string;
  absPath: string;
  blobSha: string;
  isUpstream: boolean;
  isTest: boolean;
  unitBucket: string;
}

export interface OutlineItem {
  role: string;
  symbolType: string;
  name: string;
  range: {
    byteOffset: { start: number; end: number };
    start: { line: number; column: number };
    end: { line: number; column: number };
  };
  signature: string;
  astKind: string;
  isImport: boolean;
  isExported: boolean;
}

export interface OutlineFile {
  path: string;
  language: string;
  items: OutlineItem[];
}

export interface UpstreamCopyHit {
  kind: "git-blob" | "ast-overlap" | "rg-provenance";
  firstPartyBucket: string;
  upstreamSource: string;
  score: number;
  fileCount: number;
  samples: { firstParty: string; upstream: string; detail?: string }[];
}

export interface DuplicateFileGroup {
  key: string;
  kind: "git-blob" | "ast-identical";
  difftVerified?: boolean;
  repos: string[];
  files: { repo: string; relPath: string; absPath: string; language?: string }[];
}

export interface AstCloneGroup {
  key: string;
  tier: "type1-exact" | "type2-renamed";
  symbolType: string;
  lines: number;
  repos: string[];
  members: {
    repo: string;
    relPath: string;
    absPath: string;
    name: string;
    language: string;
    startLine: number;
    endLine: number;
    signature: string;
  }[];
}

export interface GraphClonePair {
  a: string;
  b: string;
  score: number;
  symbolJaccard: number;
  importJaccard: number;
  callJaccard: number;
  sharedSymbols: string[];
  sharedImports: string[];
}

export interface DuplicateSymbolGroup {
  name: string;
  symbolType: string;
  repos: string[];
  locations: { repo: string; relPath: string; absPath: string; line: number; signature: string }[];
}

export interface TextBlockClone {
  hash: string;
  lines: number;
  preview: string;
  repos: string[];
  occurrences: { repo: string; relPath: string; absPath: string; startLine: number; endLine: number }[];
}

export interface SqlDuplicateGroup {
  sample: string;
  repos: string[];
  files: string[];
}

export interface CodeDedupReport {
  scanRoot: string;
  scannedRepos: string[];
  generatedAt: string;
  tools: {
    git: boolean;
    astGrep: boolean;
    webTreeSitter: boolean;
    difft: boolean;
    rg: boolean;
    eza: boolean;
    fzf: boolean;
  };
  stats: {
    trackedFiles: number;
    firstPartyFiles: number;
    upstreamFiles: number;
    astParsedFiles: number;
    astItems: number;
    upstreamCopyGroups: number;
    duplicateFileGroups: number;
    astCloneGroups: number;
    graphClonePairs: number;
    duplicateSymbolGroups: number;
    textBlockClones: number;
    sqlDuplicates: number;
    durationMs: number;
  };
  upstreamCopies: UpstreamCopyHit[];
  duplicateFiles: DuplicateFileGroup[];
  astClones: AstCloneGroup[];
  graphClones: GraphClonePair[];
  duplicateSymbols: DuplicateSymbolGroup[];
  textBlocks: TextBlockClone[];
  sqlDuplicates: SqlDuplicateGroup[];
}

export interface CodeScanOptions {
  scanRoot?: string;
  dirs?: string[];
  repos?: string[];
  fast?: boolean;
  minLines?: number;
  threshold?: number;
  includeTests?: boolean;
  verifyDifft?: boolean;
  fuzzyQuery?: string;
  useFzf?: boolean;
  dbPath?: string;
}

const EMPTY_BLOB_SHA = "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";
const VENDOR_PATH_RE = /(?:^|\/)(?:vendor|third[_-]?party|externals?|extern|node_modules)(?:\/|$)/i;
const SOURCE_EXT_RE = /\.(?:rs|ts|tsx|js|mjs|cjs|jsx|py|go|c|h|cc|cpp|cxx|hpp|zig)$/i;
const IGNORE_FILE_NAME_RE = /^(?:LICENSE.*|COPYING.*|\.editorconfig|\.gitattributes|\.gitignore|\.dockerignore|\.gitkeep|CHANGELOG.*|README.*|CODEOWNERS|\.markdownlint.*|DEPS\.rules|rust-toolchain\.toml|\.python-version|clippy\.toml|rustfmt\.toml|\.bazelrc|\.bazelversion|\.clang-format|\.clang-tidy|injections\.scm)$/i;
const IGNORE_BLOB_EXT_RE = /\.(?:lock|png|jpe?g|gif|webp|avif|ico|svg|woff2?|ttf|otf|wasm|bin|exe|dll|so|dylib|json|md|mdx|txt)$/i;
const FIRST_PARTY_ROOTS = new Set([
  "apps", "box", "corelib", "crates", "docs", "examples", "extensions", "lib", "libs",
  "os", "packages", "script", "scripts", "src", "tooling", "tools",
]);
const BOILERPLATE_SYMBOLS = new Set([
  "new", "default", "from", "into", "fmt", "clone", "drop", "main", "run", "render", "init",
  "setup", "teardown", "build", "check", "test", "tests", "config", "options", "error", "errors",
  "result", "state", "props", "index", "help", "version", "cli", "app", "id", "name", "type",
]);
const EZA_IGNORE = ".git|node_modules|target|.cache|.private|dist|build|.venv|venv|__pycache__|.next|.turbo|out|.vercel";

const AST_GREP_EXCLUDES = [
  "--globs", "!**/vendor/**",
  "--globs", "!**/third_party/**",
  "--globs", "!**/third-party/**",
  "--globs", "!**/external/**",
  "--globs", "!**/externals/**",
  "--globs", "!**/External/**",
  "--globs", "!**/node_modules/**",
  "--globs", "!**/target/**",
  "--globs", "!**/dist/**",
  "--globs", "!**/build/**",
  "--globs", "!**/.git/**",
  "--globs", "!**/.cache/**",
  "--globs", "!**/.next/**",
  "--globs", "!**/.turbo/**",
];

async function spawnText(cmd: string[], cwd?: string): Promise<{ code: number; out: string }> {
  const proc = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "ignore", windowsHide: true });
  const out = await new Response(proc.stdout).text();
  const code = await proc.exited;
  return { code, out };
}

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

function unitBucketOf(repo: string, relPath: string): string {
  const parts = posix(relPath).split("/").filter(Boolean);
  if (parts.length >= 2) return `${repo}:${parts[0]}/${parts[1]}`;
  return `${repo}:${parts[0] ?? "."}`;
}

function repoOwnerFromAy(root = repoRoot): Map<string, string> {
  const out = new Map<string, string>();
  try {
    const cfg = readAy(root) as { repos?: Record<string, { slug?: string; kind?: string }> };
    for (const [key, r] of Object.entries(cfg.repos ?? {})) {
      if (r.kind) out.set(key, r.kind);
    }
  } catch {}
  return out;
}

async function detectRepoForkInfo(repoDir: string, repoName: string, ayKinds: Map<string, string>): Promise<{
  isWholeRepoUpstream: boolean;
  upstreamSubPaths: Set<string>;
}> {
  const upstreamSubPaths = new Set<string>();
  const forksFile = join(repoDir, "forks.toml");
  const hasForksToml = isFile(forksFile);
  if (hasForksToml) {
    const parsed = parseForksToml(await Bun.file(forksFile).text().catch(() => ""));
    for (const f of parsed.forks) upstreamSubPaths.add(posix(f.path));
  }
  const submodulesFile = join(repoDir, "submodules.toml");
  if (isFile(submodulesFile)) {
    const subs = parseSubmodulesToml(await Bun.file(submodulesFile).text().catch(() => ""));
    for (const s of subs) upstreamSubPaths.add(posix(s.path));
  }
  const gitmodulesFile = join(repoDir, ".gitmodules");
  if (isFile(gitmodulesFile)) {
    for (const s of parseGitmodules(await Bun.file(gitmodulesFile).text().catch(() => ""))) {
      upstreamSubPaths.add(posix(s.path));
    }
  }

  // Whole-repo upstream checkouts: runtime forks (bun), browser/git mirror forks, or non-aphrody remotes.
  const kind = ayKinds.get(repoName);
  let isWholeRepoUpstream = kind === "runtime" || kind === "browser" || repoName === "aphrody-git" || repoName === "vercel";
  if (!isWholeRepoUpstream && !hasForksToml && !kind) {
    const { code, out } = await spawnText(["git", "-C", repoDir, "remote", "get-url", "origin"]);
    if (code === 0) {
      const slug = parseGitSlug(out)?.slug ?? "";
      if (slug && !slug.startsWith("aphrody-dev/") && !slug.startsWith("aphrody-labs/") && !slug.startsWith("rose-griffon/")) {
        isWholeRepoUpstream = true;
      }
    }
  }
  return { isWholeRepoUpstream, upstreamSubPaths };
}

function isUnderSetPrefix(relPath: string, prefixes: Set<string>): boolean {
  for (const p of prefixes) {
    if (relPath === p || relPath.startsWith(`${p}/`)) return true;
  }
  return false;
}

// --- Stage 1: Git index + submodule blob indexing --------------------------------------------------------------------

async function indexGitTree(
  dir: string,
  repoName: string,
  prefix: string,
  forceUpstream: boolean,
  upstreamPrefixes: Set<string>,
  includeTests: boolean,
  out: GitIndexedFile[],
): Promise<void> {
  const { code, out: raw } = await spawnText(["git", "-C", dir, "ls-files", "-s", "-z"]);
  if (code !== 0) return;

  const subDirs: { subDir: string; nextPrefix: string }[] = [];
  for (const row of raw.split("\0")) {
    if (!row) continue;
    const m = /^(\d+)\s+([0-9a-f]{40})\s+\d+\t(.+)$/.exec(row);
    if (!m) continue;
    const mode = m[1]!;
    const sha = m[2]!;
    const rel = posix(m[3]!);
    const fullRel = prefix ? `${prefix}/${rel}` : rel;
    if (mode === "160000") {
      const subDir = join(dir, rel);
      if (isDirectory(subDir) && (isFile(join(subDir, ".git")) || isDirectory(join(subDir, ".git")))) {
        subDirs.push({ subDir, nextPrefix: fullRel });
      }
      continue;
    }
    if (sha === EMPTY_BLOB_SHA) continue;
    const isTest = isTestFixturePath(fullRel);
    if (!includeTests && isTest) continue;
    const isUpstream = forceUpstream || VENDOR_PATH_RE.test(fullRel) || isUnderSetPrefix(fullRel, upstreamPrefixes);
    out.push({
      repo: repoName,
      relPath: fullRel,
      absPath: posix(join(dir, rel)),
      blobSha: sha,
      isUpstream,
      isTest,
      unitBucket: unitBucketOf(repoName, fullRel),
    });
  }

  for (const s of subDirs) {
    await indexGitTree(s.subDir, repoName, s.nextPrefix, true, upstreamPrefixes, includeTests, out);
  }
}

export async function collectGitFiles(
  repoDirs: string[],
  includeTests: boolean,
): Promise<{ files: GitIndexedFile[]; upstreamPathsByRepo: Map<string, Set<string>> }> {
  const ayKinds = repoOwnerFromAy();
  const files: GitIndexedFile[] = [];
  const upstreamPathsByRepo = new Map<string, Set<string>>();

  await mapConcurrent(repoDirs, 6, async (repoDir) => {
    const repoName = basename(repoDir);
    const info = await detectRepoForkInfo(repoDir, repoName, ayKinds);
    upstreamPathsByRepo.set(repoName, info.upstreamSubPaths);
    const repoFiles: GitIndexedFile[] = [];
    await indexGitTree(repoDir, repoName, "", info.isWholeRepoUpstream, info.upstreamSubPaths, includeTests, repoFiles);
    files.push(...repoFiles);
  });

  return { files, upstreamPathsByRepo };
}

// --- Stage 2: Tree-sitter AST parsing (`ast-grep outline` + `web-tree-sitter`) --------------------------------------

export function stripCommentsAndNormalize(code: string): string {
  const stripped = code
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/(^|\s)\/\/.*$/gm, "$1")
    .replace(/(^|\s)#(?!!\[|include|define|ifdef|ifndef|endif|pragma).*$/gm, "$1");
  return stripped.replace(/\s+/g, " ").trim();
}

const LANG_KEYWORDS = new Set([
  "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "crate",
  "def", "default", "do", "dyn", "elif", "else", "enum", "export", "extends", "extern",
  "false", "finally", "fn", "for", "from", "func", "function", "if", "impl", "import",
  "in", "interface", "is", "let", "loop", "match", "mod", "mut", "new", "nil", "none",
  "null", "or", "package", "pass", "pub", "raise", "ref", "return", "self", "Self",
  "static", "struct", "super", "switch", "this", "throw", "trait", "true", "try", "type",
  "typeof", "undefined", "unsafe", "use", "var", "void", "where", "while", "with", "yield",
]);

/** Alpha-normalized token signature (preserves structure & keywords, normalizes user identifiers and literals). */
export function alphaNormalizeCode(code: string): string {
  const noComments = code
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/(^|\s)\/\/.*$/gm, "$1");
  const noStrings = noComments
    .replace(/"(?:\\.|[^"\\])*"/g, " $STR ")
    .replace(/'(?:\\.|[^'\\])*'/g, " $STR ")
    .replace(/`(?:\\.|[^`\\])*`/g, " $STR ");
  const tokens: string[] = [];
  for (const m of noStrings.matchAll(/[A-Za-z_][A-Za-z0-9_]*|\d+(?:\.\d+)?|=>|->|::|==|!=|<=|>=|&&|\|\||[{}()[\];,.<>+\-*/%&|^!~?:=]/g)) {
    const t = m[0]!;
    if (t === "$STR") tokens.push("$LIT");
    else if (/^\d/.test(t)) tokens.push("$NUM");
    else if (/^[A-Za-z_]/.test(t)) tokens.push(LANG_KEYWORDS.has(t) ? t : "$ID");
    else tokens.push(t);
  }
  return tokens.join(" ");
}

/** Deep Tree-sitter AST node/token signature via `web-tree-sitter` (`tools/ts.ts`) for C, Rust, and Python. */
export async function treeSitterAlphaSignature(lang: TsLang, code: string): Promise<string> {
  try {
    const root = await parseTreeSitter(lang, code);
    const out: string[] = [];
    const visit = (node: { type: string; isNamed: boolean; text: string; children: unknown[] }) => {
      if (node.type === "comment" || node.type === "line_comment" || node.type === "block_comment") return;
      if (node.children.length === 0) {
        if (node.type === "identifier" || node.type === "field_identifier" || node.type === "type_identifier") {
          out.push("$ID");
        } else if (/string|number|integer|float|literal/.test(node.type)) {
          out.push("$LIT");
        } else {
          out.push(node.text);
        }
        return;
      }
      if (node.isNamed) out.push(`(${node.type}`);
      for (const c of node.children) {
        if (c) visit(c as typeof node);
      }
      if (node.isNamed) out.push(")");
    };
    visit(root as unknown as { type: string; isNamed: boolean; text: string; children: unknown[] });
    return out.join(" ");
  } catch {
    return alphaNormalizeCode(code);
  }
}

export function mapToTsLang(language: string): TsLang | undefined {
  const l = language.toLowerCase();
  if (l === "rust") return "rust";
  if (l === "python") return "python";
  if (l === "c") return "c";
  return undefined;
}

export function extractOutline(
  dir: string,
  astGrepBin: string,
  includeTests: boolean,
): Promise<OutlineFile[]> {
  const globs = [...AST_GREP_EXCLUDES];
  if (!includeTests) {
    globs.push("--globs", "!**/test/**", "--globs", "!**/tests/**", "--globs", "!**/fixtures/**", "--globs", "!**/testdata/**");
  }
  return spawnText([astGrepBin, "outline", "--json=stream", "--items=all", ...globs, dir]).then(({ out }) => {
    const files: OutlineFile[] = [];
    for (const line of out.split(/\r?\n/)) {
      if (!line.trim()) continue;
      try {
        files.push(JSON.parse(line) as OutlineFile);
      } catch {}
    }
    return files;
  });
}

function hashHex(input: string): string {
  return Bun.CryptoHasher.hash("blake2b256", input, "hex").slice(0, 24);
}

// --- Stage 3: Upstream copy detection --------------------------------------------------------------------------------

async function detectRgProvenance(firstPartyDirs: string[], rgBin?: string): Promise<UpstreamCopyHit[]> {
  if (!rgBin || !firstPartyDirs.length) return [];
  const pattern = "(?i)(?:copied|vendored|ported|adapted|forked)\\s+from\\s+https?://[^\\s)]+|Copyright\\s+(?:\\(c\\)\\s*)?(?:20\\d\\d|19\\d\\d)\\s+(?!Aphrody|aphrody)[^\\r\\n]{3,60}";
  const { out } = await spawnText([
    rgBin, "-n", "--no-heading", "--max-count", "2",
    "-g", "*.{rs,ts,tsx,js,py,go,c,h,cpp}",
    "-g", "!**/vendor/**", "-g", "!**/third_party/**", "-g", "!**/third-party/**",
    "-g", "!**/node_modules/**", "-g", "!**/target/**", "-g", "!**/tests/**", "-g", "!**/test/**",
    pattern, ...firstPartyDirs,
  ]);

  const byDir = new Map<string, { firstParty: string; detail: string }[]>();
  for (const line of out.split(/\r?\n/)) {
    if (!line.trim()) continue;
    const m = /^(.+?):(\d+):(.*)$/.exec(line);
    if (!m) continue;
    const filePath = posix(m[1]!);
    const detail = m[3]!.trim().slice(0, 120);
    const parent = posix(dirname(filePath));
    (byDir.get(parent) ?? byDir.set(parent, []).get(parent)!).push({ firstParty: `${filePath}:${m[2]}`, detail });
  }

  const hits: UpstreamCopyHit[] = [];
  for (const [dir, items] of byDir) {
    if (items.length < 2) continue;
    hits.push({
      kind: "rg-provenance",
      firstPartyBucket: dir,
      upstreamSource: items[0]!.detail,
      score: Math.min(1, 0.5 + items.length * 0.1),
      fileCount: items.length,
      samples: items.slice(0, 5).map((x) => ({ firstParty: x.firstParty, upstream: x.detail })),
    });
  }
  return hits;
}

function detectUpstreamCopies(
  gitFiles: GitIndexedFile[],
  outlineByAbsPath: Map<string, OutlineFile>,
  rgProvenanceHits: UpstreamCopyHit[],
): UpstreamCopyHit[] {
  const blobIndex = new Map<string, GitIndexedFile[]>();
  for (const f of gitFiles) {
    const base = basename(f.relPath);
    if (IGNORE_FILE_NAME_RE.test(base) || IGNORE_BLOB_EXT_RE.test(base)) continue;
    (blobIndex.get(f.blobSha) ?? blobIndex.set(f.blobSha, []).get(f.blobSha)!).push(f);
  }

  const blobBuckets = new Map<string, { upstreamDir: string; samples: { firstParty: string; upstream: string }[] }>();
  for (const locs of blobIndex.values()) {
    if (locs.length < 2) continue;
    const ups = locs.filter((l) => l.isUpstream);
    const fps = locs.filter((l) => !l.isUpstream && !l.isTest);
    if (!ups.length || !fps.length) continue;
    for (const fp of fps) {
      const up = ups[0]!;
      const key = `${fp.unitBucket}<<${up.unitBucket}`;
      const b = blobBuckets.get(key) ?? { upstreamDir: up.unitBucket, samples: [] };
      b.samples.push({ firstParty: `${fp.repo}:${fp.relPath}`, upstream: `${up.repo}:${up.relPath}` });
      blobBuckets.set(key, b);
    }
  }

  const hits: UpstreamCopyHit[] = [];
  for (const [key, b] of blobBuckets) {
    const [fpBucket] = key.split("<<");
    hits.push({
      kind: "git-blob",
      firstPartyBucket: fpBucket!,
      upstreamSource: b.upstreamDir,
      score: 1,
      fileCount: b.samples.length,
      samples: b.samples.slice(0, 5),
    });
  }

  // Per-file Tree-sitter AST symbol overlap (inverted index over real function/struct/class/trait definitions).
  const AST_DEF_KINDS = new Set(["function", "method", "struct", "class", "interface", "trait", "enum"]);
  const fileSyms: { f: GitIndexedFile; syms: Set<string> }[] = [];
  const symToUpIdxs = new Map<string, number[]>();

  for (const f of gitFiles) {
    if (!SOURCE_EXT_RE.test(f.relPath) || f.isTest) continue;
    const outline = outlineByAbsPath.get(f.absPath);
    if (!outline) continue;
    const syms = new Set<string>();
    for (const item of outline.items) {
      if (item.isImport || !AST_DEF_KINDS.has(item.symbolType)) continue;
      const nm = item.name.trim();
      if (nm.length >= 6 && !BOILERPLATE_SYMBOLS.has(nm.toLowerCase())) syms.add(nm);
    }
    if (syms.size < 4) continue;
    const idx = fileSyms.length;
    fileSyms.push({ f, syms });
    if (f.isUpstream) {
      for (const s of syms) (symToUpIdxs.get(s) ?? symToUpIdxs.set(s, []).get(s)!).push(idx);
    }
  }

  const astBuckets = new Map<string, { scoreSum: number; pairs: { firstParty: string; upstream: string; detail: string }[] }>();
  for (let i = 0; i < fileSyms.length; i++) {
    const fp = fileSyms[i]!;
    if (fp.f.isUpstream) continue;
    const candCounts = new Map<number, number>();
    for (const s of fp.syms) {
      const upList = symToUpIdxs.get(s);
      if (!upList || upList.length > 30) continue;
      for (const ui of upList) candCounts.set(ui, (candCounts.get(ui) ?? 0) + 1);
    }
    for (const [ui, sharedCount] of candCounts) {
      if (sharedCount < 4) continue;
      const up = fileSyms[ui]!;
      const jaccard = sharedCount / (fp.syms.size + up.syms.size - sharedCount);
      if (jaccard < 0.45) continue;
      const key = `${fp.f.unitBucket}<<${up.f.unitBucket}`;
      const b = astBuckets.get(key) ?? { scoreSum: 0, pairs: [] };
      b.scoreSum += jaccard;
      b.pairs.push({
        firstParty: `${fp.f.repo}:${fp.f.relPath}`,
        upstream: `${up.f.repo}:${up.f.relPath}`,
        detail: `${sharedCount} shared AST defs (J=${jaccard.toFixed(2)})`,
      });
      astBuckets.set(key, b);
    }
  }

  for (const [key, b] of astBuckets) {
    const [fpBucket, upBucket] = key.split("<<");
    hits.push({
      kind: "ast-overlap",
      firstPartyBucket: fpBucket!,
      upstreamSource: upBucket!,
      score: Number((b.scoreSum / b.pairs.length).toFixed(3)),
      fileCount: b.pairs.length,
      samples: b.pairs.slice(0, 5),
    });
  }

  hits.push(...rgProvenanceHits);
  return hits.sort((a, b) => b.fileCount - a.fileCount || b.score - a.score);
}

// --- Stage 4: Code graph, AST clones & text block clones -------------------------------------------------------------

function extractCallsFromText(text: string): Set<string> {
  const calls = new Set<string>();
  for (const m of text.matchAll(/\b([A-Za-z_][A-Za-z0-9_]{2,40})\s*\(/g)) {
    const c = m[1]!;
    if (!LANG_KEYWORDS.has(c) && !BOILERPLATE_SYMBOLS.has(c.toLowerCase())) calls.add(c);
  }
  return calls;
}

function jaccardSet(a: Set<string>, b: Set<string>): { score: number; shared: string[] } {
  if (!a.size || !b.size) return { score: 0, shared: [] };
  const shared: string[] = [];
  for (const x of a) if (b.has(x)) shared.push(x);
  if (!shared.length) return { score: 0, shared: [] };
  return { score: shared.length / (a.size + b.size - shared.length), shared };
}

function isNormalizedCodeLine(line: string): string {
  const trimmed = line
    .replace(/\/\/.*$/, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (trimmed.length < 16) return "";
  if (/^(?:import|use|pub use|#include|from\s+\S+\s+import|package\s+|module\s+|#\[derive)/.test(trimmed)) return "";
  return trimmed;
}

async function verifyWithDifft(aPath: string, bPath: string, difftBin?: string): Promise<boolean | undefined> {
  if (!difftBin) return undefined;
  const { code, out } = await spawnText([difftBin, "--color=never", aPath, bPath]);
  if (code !== 0) return undefined;
  return /\bNo changes\.\s*$/m.test(out);
}

export async function runCodeAudit(opts: CodeScanOptions = {}): Promise<CodeDedupReport> {
  const t0 = performance.now();
  const scanRoot = resolve(opts.scanRoot ?? defaultScanRoot());
  const includeTests = opts.includeTests ?? false;
  const minLines = opts.minLines ?? 6;
  const threshold = opts.threshold ?? 0.72;
  const repoFilter = opts.repos?.length ? new Set(opts.repos) : undefined;

  const astGrepBin = Bun.which("ast-grep") ?? undefined;
  const difftBin = Bun.which("difft") ?? undefined;
  const rgBin = Bun.which("rg") ?? undefined;
  const ezaBin = Bun.which("eza") ?? undefined;
  const fzfBin = opts.useFzf === false ? undefined : (Bun.which("fzf") ?? undefined);

  const repoDirs = opts.dirs?.length
    ? opts.dirs.map((d) => resolve(d)).filter(isDirectory)
    : readdirSync(scanRoot, { withFileTypes: true })
        .filter((d) => d.isDirectory() && !d.name.startsWith("."))
        .map((d) => join(scanRoot, d.name))
        .sort();

  const scannedRepos = repoDirs.map((d) => basename(d));

  // Optional eza directory tree check to locate first-party root folders cleanly.
  if (ezaBin) {
    await mapConcurrent(repoDirs.slice(0, 6), 6, async (dir) => {
      const { out } = await spawnText([ezaBin, "-D", "-T", "-L", "2", "--no-quotes", "--icons=never", "--color=never", "-I", EZA_IGNORE, dir]);
      return parseEzaTree(out, dir);
    });
  }

  const { files: gitFiles, upstreamPathsByRepo } = await collectGitFiles(repoDirs, includeTests);
  const firstPartyGitFiles = gitFiles.filter(
    (f) => !f.isUpstream && (!repoFilter || repoFilter.has(f.repo)),
  );

  // Build AST scan targets: in `--fast` mode scan only first-party roots; otherwise scan all repos for full AST upstream comparison.
  const astTargets: string[] = [];
  if (astGrepBin) {
    if (opts.fast) {
      for (const dir of repoDirs) {
        const repoName = basename(dir);
        if (repoFilter && !repoFilter.has(repoName)) continue;
        const ups = upstreamPathsByRepo.get(repoName) ?? new Set<string>();
        for (const sub of FIRST_PARTY_ROOTS) {
          const cand = join(dir, sub);
          if (isDirectory(cand) && !ups.has(sub)) astTargets.push(cand);
        }
      }
    } else {
      for (const dir of repoDirs) astTargets.push(dir);
    }
  }

  const firstPartyDirs = repoDirs
    .filter((d) => !repoFilter || repoFilter.has(basename(d)))
    .flatMap((d) => [...FIRST_PARTY_ROOTS].map((s) => join(d, s)).filter(isDirectory));

  const [outlineBatches, rgProvenanceHits] = await Promise.all([
    astGrepBin ? mapConcurrent(astTargets, 4, (t) => extractOutline(t, astGrepBin, includeTests)) : Promise.resolve([]),
    detectRgProvenance(firstPartyDirs, rgBin),
  ]);

  const outlineByAbsPath = new Map<string, OutlineFile>();
  let astItemsCount = 0;
  for (const batch of outlineBatches) {
    for (const f of batch) {
      const p = posix(f.path);
      outlineByAbsPath.set(p, { ...f, path: p });
      astItemsCount += f.items.length;
    }
  }

  const upstreamCopies = detectUpstreamCopies(gitFiles, outlineByAbsPath, rgProvenanceHits);

  // Read first-party source files once for AST cloning, code graph, text winnowing, and SQL normalization.
  const sourceFiles = firstPartyGitFiles.filter((f) => SOURCE_EXT_RE.test(f.relPath) && !f.isTest);
  const fileContents = new Map<string, string>();
  await mapConcurrent(sourceFiles, 16, async (f) => {
    try {
      const bf = Bun.file(f.absPath);
      if (bf.size > 350_000 || bf.size === 0) return;
      fileContents.set(f.absPath, await bf.text());
    } catch {}
  });

  // 1. Exact duplicate first-party files via Git blob SHA-1.
  const fpBlobs = new Map<string, GitIndexedFile[]>();
  for (const f of firstPartyGitFiles) {
    const base = basename(f.relPath);
    if (IGNORE_FILE_NAME_RE.test(base) || IGNORE_BLOB_EXT_RE.test(base)) continue;
    (fpBlobs.get(f.blobSha) ?? fpBlobs.set(f.blobSha, []).get(f.blobSha)!).push(f);
  }

  const duplicateFiles: DuplicateFileGroup[] = [];
  const verbatimFileIds = new Set<string>();
  for (const [sha, locs] of fpBlobs) {
    if (locs.length < 2) continue;
    const distinctReposOrTopDirs = new Set(locs.map((l) => l.unitBucket));
    if (distinctReposOrTopDirs.size < 2) continue;
    const sorted = [...locs].sort((a, b) => `${a.repo}:${a.relPath}`.localeCompare(`${b.repo}:${b.relPath}`));
    for (const l of sorted) verbatimFileIds.add(l.absPath);
    duplicateFiles.push({
      key: basename(sorted[0]!.relPath),
      kind: "git-blob",
      repos: [...new Set(sorted.map((l) => l.repo))].sort(),
      files: sorted.map((l) => ({
        repo: l.repo,
        relPath: l.relPath,
        absPath: l.absPath,
        language: outlineByAbsPath.get(l.absPath)?.language,
      })),
    });
  }

  // Whole-file AST-identical check (catches files identical structurally after formatting/comments).
  const fileAstAlphaBuckets = new Map<string, { file: GitIndexedFile; lang: string }[]>();
  for (const f of sourceFiles) {
    if (verbatimFileIds.has(f.absPath)) continue;
    const text = fileContents.get(f.absPath);
    if (!text || text.split(/\r?\n/).length < minLines) continue;
    const alpha = alphaNormalizeCode(text);
    if (alpha.length < 160) continue;
    const hash = hashHex(alpha);
    const lang = outlineByAbsPath.get(f.absPath)?.language ?? extname(f.relPath).slice(1);
    (fileAstAlphaBuckets.get(hash) ?? fileAstAlphaBuckets.set(hash, []).get(hash)!).push({ file: f, lang });
  }
  for (const [, locs] of fileAstAlphaBuckets) {
    if (locs.length < 2) continue;
    const buckets = new Set(locs.map((l) => l.file.unitBucket));
    if (buckets.size < 2) continue;
    const difftOk = opts.verifyDifft ? await verifyWithDifft(locs[0]!.file.absPath, locs[1]!.file.absPath, difftBin) : undefined;
    duplicateFiles.push({
      key: basename(locs[0]!.file.relPath),
      kind: "ast-identical",
      difftVerified: difftOk,
      repos: [...new Set(locs.map((l) => l.file.repo))].sort(),
      files: locs.map((l) => ({
        repo: l.file.repo,
        relPath: l.file.relPath,
        absPath: l.file.absPath,
        language: l.lang,
      })),
    });
  }
  duplicateFiles.sort((a, b) => b.files.length - a.files.length || a.key.localeCompare(b.key));

  // 2. Tree-sitter AST clones (Type-1 exact & Type-2 alpha-renamed functions/structs/classes/traits).
  type CloneBucket = { symbolType: string; lines: number; members: AstCloneGroup["members"] };
  const type1Buckets = new Map<string, CloneBucket>();
  const type2Buckets = new Map<string, CloneBucket>();
  const symbolDefs = new Map<string, DuplicateSymbolGroup["locations"]>();
  const graphNodes: {
    id: string;
    bucket: string;
    symbols: Set<string>;
    imports: Set<string>;
    calls: Set<string>;
  }[] = [];
  const sqlBuckets = new Map<string, { sample: string; files: Set<string>; repos: Set<string> }>();
  const textWindowBuckets = new Map<string, { preview: string; locs: { repo: string; relPath: string; absPath: string; startLine: number; endLine: number }[] }>();
  const coreDbFiles: IndexedFile[] = [];

  for (const f of sourceFiles) {
    const text = fileContents.get(f.absPath);
    if (!text) continue;
    const outline = outlineByAbsPath.get(f.absPath);
    const bytes = Buffer.from(text, "utf8");
    const symSet = new Set<string>();
    const impSet = new Set<string>();
    const callSet = extractCallsFromText(text);

    if (outline) {
      for (const item of outline.items) {
        if (item.isImport) {
          const mod = item.name.replace(/^["']|["']$/g, "").trim();
          if (mod) impSet.add(mod);
          continue;
        }
        const nm = item.name.trim();
        if (nm.length >= 4 && !BOILERPLATE_SYMBOLS.has(nm.toLowerCase())) {
          symSet.add(nm);
          if (item.isExported || item.symbolType === "function" || item.symbolType === "struct" || item.symbolType === "class" || item.symbolType === "trait" || item.symbolType === "interface") {
            const symKey = `${item.symbolType}:${nm}`;
            (symbolDefs.get(symKey) ?? symbolDefs.set(symKey, []).get(symKey)!).push({
              repo: f.repo,
              relPath: f.relPath,
              absPath: f.absPath,
              line: item.range.start.line + 1,
              signature: item.signature.slice(0, 120),
            });
          }
        }

        const itemLines = item.range.end.line - item.range.start.line + 1;
        if (itemLines < minLines) continue;
        if (!["function", "method", "struct", "class", "interface", "trait", "enum"].includes(item.symbolType)) continue;
        const body = bytes.subarray(item.range.byteOffset.start, item.range.byteOffset.end).toString("utf8");
        const normBody = stripCommentsAndNormalize(body);
        if (normBody.length < 120) continue;

        const member: AstCloneGroup["members"][number] = {
          repo: f.repo,
          relPath: f.relPath,
          absPath: f.absPath,
          name: item.name,
          language: outline.language,
          startLine: item.range.start.line + 1,
          endLine: item.range.end.line + 1,
          signature: item.signature.slice(0, 120),
        };
        const h1 = hashHex(`${item.symbolType}:${normBody}`);
        const entry1 = type1Buckets.get(h1) ?? { symbolType: item.symbolType, lines: itemLines, members: [] };
        entry1.members.push(member);
        type1Buckets.set(h1, entry1);

        const alpha = alphaNormalizeCode(body);
        if ((item.symbolType === "function" || item.symbolType === "method") && alpha.split(" ").length >= 26) {
          const h2 = hashHex(`${item.symbolType}:${alpha}`);
          const entry = type2Buckets.get(h2) ?? { symbolType: item.symbolType, lines: itemLines, members: [] };
          entry.members.push(member);
          type2Buckets.set(h2, entry);
        }
      }
    }

    if (symSet.size + impSet.size >= 4) {
      graphNodes.push({
        id: `${f.repo}:${f.relPath}`,
        bucket: f.unitBucket,
        symbols: symSet,
        imports: impSet,
        calls: callSet,
      });
    }

    if (opts.dbPath && outline) {
      const zeroSpan: Span = { start_line: 1, end_line: 1, start_col: 0, end_col: 0, start_byte: 0, end_byte: 0 };
      coreDbFiles.push({
        path: `${f.repo}:${f.relPath}`,
        language: outline.language,
        layer: f.repo,
        feature: f.unitBucket,
        hash: f.blobSha,
        summary: {
          path: `${f.repo}:${f.relPath}`,
          language: outline.language,
          symbols: outline.items
            .filter((i) => !i.isImport)
            .map((i) => ({
              name: i.name,
              kind: i.symbolType,
              signature: i.signature,
              span: {
                start_line: i.range.start.line + 1,
                end_line: i.range.end.line + 1,
                start_col: i.range.start.column + 1,
                end_col: i.range.end.column + 1,
                start_byte: i.range.byteOffset.start,
                end_byte: i.range.byteOffset.end,
              },
            })),
          calls: [...callSet].slice(0, 64).map((c) => ({ callee: c, span: zeroSpan })),
          imports: [...impSet].map((m) => ({ source_module: m, imported_symbols: [], span: zeroSpan })),
          parse_errors: 0,
          line_count: text.split(/\r?\n/).length,
          byte_count: bytes.byteLength,
        },
      });
    }

    // Text block winnowing (k = minLines normalized non-import code lines).
    const rawLines = text.split(/\r?\n/);
    const normLines: { lineNo: number; norm: string }[] = [];
    for (let i = 0; i < rawLines.length; i++) {
      const norm = isNormalizedCodeLine(rawLines[i]!);
      if (norm) normLines.push({ lineNo: i + 1, norm });
    }
    for (let i = 0; i <= normLines.length - minLines; i++) {
      const slice = normLines.slice(i, i + minLines);
      const block = slice.map((s) => s.norm).join("\n");
      if (block.length < 150) continue;
      const h = Bun.hash(block).toString(16);
      const bucket = textWindowBuckets.get(h) ?? { preview: slice[0]!.norm.slice(0, 100), locs: [] };
      bucket.locs.push({
        repo: f.repo,
        relPath: f.relPath,
        absPath: f.absPath,
        startLine: slice[0]!.lineNo,
        endLine: slice[slice.length - 1]!.lineNo,
      });
      textWindowBuckets.set(h, bucket);
    }

    // Normalized SQL clone detection.
    const SQL_RE = /\b(SELECT\b[^;"'`]{4,300}?\bFROM\s+[\w."`]+|INSERT\s+(?:OR\s+\w+\s+)?INTO\s+[\w."`]+|CREATE\s+TABLE(?:\s+IF\s+NOT\EXISTS)?\s+[\w."`]+|UPDATE\s+[\w."`]+\s+SET|DELETE\s+FROM\s+[\w."`]+)/gi;
    for (const m of text.matchAll(SQL_RE)) {
      const norm = normalizeSql(m[1]!);
      if (norm.length < 25) continue;
      const key = Bun.hash(norm).toString(16);
      const b = sqlBuckets.get(key) ?? { sample: norm.slice(0, 100), files: new Set<string>(), repos: new Set<string>() };
      b.files.add(`${f.repo}:${f.relPath}`);
      b.repos.add(f.repo);
      sqlBuckets.set(key, b);
    }
  }

  // Deep `web-tree-sitter` confirmation for candidate C/Rust/Python AST clones (`tools/ts.ts`).
  const astClones: AstCloneGroup[] = [];
  const type1MemberKeys = new Set<string>();
  let webTreeSitterUsed = false;

  for (const [, entry1] of type1Buckets) {
    const members = entry1.members;
    if (members.length < 2) continue;
    const distinctFiles = new Set(members.map((m) => m.absPath));
    if (distinctFiles.size < 2) continue;
    // Skip clones inside pairs of files already reported as verbatim git-blob identical.
    if ([...distinctFiles].every((p) => verbatimFileIds.has(p))) continue;
    for (const m of members) type1MemberKeys.add(`${m.absPath}:${m.startLine}:${m.name}`);
    astClones.push({
      key: members[0]!.name,
      tier: "type1-exact",
      symbolType: entry1.symbolType,
      lines: entry1.lines,
      repos: [...new Set(members.map((m) => m.repo))].sort(),
      members: [...members].sort((a, b) => a.repo.localeCompare(b.repo) || a.relPath.localeCompare(b.relPath)),
    });
  }

  for (const [, entry] of type2Buckets) {
    if (entry.members.length < 2) continue;
    const distinctFiles = new Set(entry.members.map((m) => m.absPath));
    if (distinctFiles.size < 2) continue;
    if ([...distinctFiles].every((p) => verbatimFileIds.has(p))) continue;
    if (entry.members.every((m) => type1MemberKeys.has(`${m.absPath}:${m.startLine}:${m.name}`))) continue;

    // For C/Rust/Python items, verify normalized AST subtree equality via web-tree-sitter (`tools/ts.ts`).
    const tsLang = mapToTsLang(entry.members[0]!.language);
    if (tsLang && entry.members.length <= 6) {
      const astSigs = await Promise.all(
        entry.members.slice(0, 4).map(async (m) => {
          const text = fileContents.get(m.absPath) ?? "";
          const snippet = text.split(/\r?\n/).slice(m.startLine - 1, m.endLine).join("\n");
          return await treeSitterAlphaSignature(tsLang, snippet);
        }),
      );
      webTreeSitterUsed = true;
      if (new Set(astSigs).size > 1) continue;
    }

    astClones.push({
      key: entry.members[0]!.name,
      tier: "type2-renamed",
      symbolType: entry.symbolType,
      lines: entry.lines,
      repos: [...new Set(entry.members.map((m) => m.repo))].sort(),
      members: [...entry.members].sort((a, b) => a.repo.localeCompare(b.repo) || a.relPath.localeCompare(b.relPath)),
    });
  }
  astClones.sort((a, b) => b.members.length - a.members.length || b.lines - a.lines);

  // 3. Code Graph duplicate symbols & module subgraph similarity.
  const duplicateSymbols: DuplicateSymbolGroup[] = [];
  for (const [symKey, locs] of symbolDefs) {
    if (locs.length < 2) continue;
    const distinctBuckets = new Set(locs.map((l) => unitBucketOf(l.repo, l.relPath)));
    if (distinctBuckets.size < 2) continue;
    if (locs.every((l) => verbatimFileIds.has(l.absPath))) continue;
    const colon = symKey.indexOf(":");
    duplicateSymbols.push({
      symbolType: symKey.slice(0, colon),
      name: symKey.slice(colon + 1),
      repos: [...new Set(locs.map((l) => l.repo))].sort(),
      locations: locs.sort((a, b) => a.repo.localeCompare(b.repo) || a.relPath.localeCompare(b.relPath)),
    });
  }
  duplicateSymbols.sort((a, b) => b.locations.length - a.locations.length || a.name.localeCompare(b.name));

  // Inverted index over symbols & imports so graph pair comparison stays sub-second even on 10k+ nodes.
  const nodeIndex = new Map<string, number[]>();
  for (let i = 0; i < graphNodes.length; i++) {
    for (const s of graphNodes[i]!.symbols) {
      (nodeIndex.get(`s:${s}`) ?? nodeIndex.set(`s:${s}`, []).get(`s:${s}`)!).push(i);
    }
    for (const imp of graphNodes[i]!.imports) {
      if (imp.length >= 4) (nodeIndex.get(`i:${imp}`) ?? nodeIndex.set(`i:${imp}`, []).get(`i:${imp}`)!).push(i);
    }
  }
  const pairCandidates = new Map<string, number>();
  for (const list of nodeIndex.values()) {
    if (list.length < 2 || list.length > 40) continue;
    for (let a = 0; a < list.length; a++) {
      for (let b = a + 1; b < list.length; b++) {
        const i = list[a]!;
        const j = list[b]!;
        if (graphNodes[i]!.bucket === graphNodes[j]!.bucket) continue;
        const key = i < j ? `${i}:${j}` : `${j}:${i}`;
        pairCandidates.set(key, (pairCandidates.get(key) ?? 0) + 1);
      }
    }
  }

  const graphClones: GraphClonePair[] = [];
  for (const [key, hits] of pairCandidates) {
    if (hits < 3) continue;
    const [ai, bi] = key.split(":").map(Number) as [number, number];
    const na = graphNodes[ai]!;
    const nb = graphNodes[bi]!;
    const symJ = jaccardSet(na.symbols, nb.symbols);
    const impJ = jaccardSet(na.imports, nb.imports);
    const callJ = jaccardSet(na.calls, nb.calls);
    const score = Number((0.45 * symJ.score + 0.3 * impJ.score + 0.25 * callJ.score).toFixed(3));
    if (score >= threshold && symJ.shared.length + impJ.shared.length >= 4) {
      graphClones.push({
        a: na.id,
        b: nb.id,
        score,
        symbolJaccard: Number(symJ.score.toFixed(3)),
        importJaccard: Number(impJ.score.toFixed(3)),
        callJaccard: Number(callJ.score.toFixed(3)),
        sharedSymbols: symJ.shared.slice(0, 8),
        sharedImports: impJ.shared.slice(0, 8),
      });
    }
  }
  graphClones.sort((a, b) => b.score - a.score);

  // 4. Text block clones (deduplicate overlapping rolling windows by keeping highest-span distinct pairs).
  const textBlocks: TextBlockClone[] = [];
  const seenBlockPair = new Set<string>();
  for (const [hash, bucket] of textWindowBuckets) {
    if (bucket.locs.length < 2) continue;
    const distinctFiles = new Set(bucket.locs.map((l) => l.absPath));
    if (distinctFiles.size < 2) continue;
    if ([...distinctFiles].every((p) => verbatimFileIds.has(p))) continue;
    const pairKey = [...distinctFiles].sort().slice(0, 3).join("|");
    if (seenBlockPair.has(pairKey)) continue;
    seenBlockPair.add(pairKey);
    textBlocks.push({
      hash,
      lines: minLines,
      preview: bucket.preview,
      repos: [...new Set(bucket.locs.map((l) => l.repo))].sort(),
      occurrences: bucket.locs.slice(0, 8),
    });
  }
  textBlocks.sort((a, b) => b.occurrences.length - a.occurrences.length);

  const sqlDuplicates: SqlDuplicateGroup[] = [...sqlBuckets.values()]
    .filter((b) => b.files.size > 1)
    .map((b) => ({ sample: b.sample, repos: [...b.repos].sort(), files: [...b.files].sort() }))
    .sort((a, b) => b.files.length - a.files.length);

  if (opts.dbPath && coreDbFiles.length) {
    mkdirSync(dirname(opts.dbPath), { recursive: true });
    const db = openStore(opts.dbPath);
    writeFiles(db, coreDbFiles);
    db.close();
  }

  let filtered: CodeDedupReport = {
    scanRoot,
    scannedRepos,
    generatedAt: new Date().toISOString(),
    tools: {
      git: true,
      astGrep: Boolean(astGrepBin),
      webTreeSitter: webTreeSitterUsed,
      difft: Boolean(difftBin),
      rg: Boolean(rgBin),
      eza: Boolean(ezaBin),
      fzf: Boolean(fzfBin),
    },
    stats: {
      trackedFiles: gitFiles.length,
      firstPartyFiles: firstPartyGitFiles.length,
      upstreamFiles: gitFiles.length - firstPartyGitFiles.length,
      astParsedFiles: outlineByAbsPath.size,
      astItems: astItemsCount,
      upstreamCopyGroups: upstreamCopies.length,
      duplicateFileGroups: duplicateFiles.length,
      astCloneGroups: astClones.length,
      graphClonePairs: graphClones.length,
      duplicateSymbolGroups: duplicateSymbols.length,
      textBlockClones: textBlocks.length,
      sqlDuplicates: sqlDuplicates.length,
      durationMs: Math.round(performance.now() - t0),
    },
    upstreamCopies,
    duplicateFiles,
    astClones,
    graphClones,
    duplicateSymbols: duplicateSymbols.slice(0, 200),
    textBlocks: textBlocks.slice(0, 200),
    sqlDuplicates,
  };

  if (opts.fuzzyQuery?.trim()) {
    const q = opts.fuzzyQuery.trim();
    const useFzfBin = opts.useFzf !== false;
    filtered = {
      ...filtered,
      upstreamCopies: filterItemsFuzzy(filtered.upstreamCopies, (x) => `${x.kind} ${x.firstPartyBucket} ${x.upstreamSource} ${x.samples.map((s) => s.firstParty).join(" ")}`, q, useFzfBin),
      duplicateFiles: filterItemsFuzzy(filtered.duplicateFiles, (x) => `${x.key} ${x.files.map((f) => `${f.repo}:${f.relPath}`).join(" ")}`, q, useFzfBin),
      astClones: filterItemsFuzzy(filtered.astClones, (x) => `${x.key} ${x.symbolType} ${x.members.map((m) => `${m.repo}:${m.relPath}:${m.name}`).join(" ")}`, q, useFzfBin),
      graphClones: filterItemsFuzzy(filtered.graphClones, (x) => `${x.a} ${x.b} ${x.sharedSymbols.join(" ")} ${x.sharedImports.join(" ")}`, q, useFzfBin),
      duplicateSymbols: filterItemsFuzzy(filtered.duplicateSymbols, (x) => `${x.name} ${x.symbolType} ${x.locations.map((l) => `${l.repo}:${l.relPath}`).join(" ")}`, q, useFzfBin),
      textBlocks: filterItemsFuzzy(filtered.textBlocks, (x) => `${x.preview} ${x.occurrences.map((o) => `${o.repo}:${o.relPath}`).join(" ")}`, q, useFzfBin),
      sqlDuplicates: filterItemsFuzzy(filtered.sqlDuplicates, (x) => `${x.sample} ${x.files.join(" ")}`, q, useFzfBin),
    };
  }

  return filtered;
}

// --- Reporting & fzf interactive browser -----------------------------------------------------------------------------

export function renderCodeMarkdown(r: CodeDedupReport): string {
  const lines = [
    "# Code Duplication & Copied Upstream Audit",
    "",
    `- Scan root: \`${r.scanRoot}\` (${r.scannedRepos.length} repos, ${r.stats.durationMs} ms)`,
    `- Tracked files: ${r.stats.trackedFiles} (${r.stats.firstPartyFiles} first-party, ${r.stats.upstreamFiles} upstream/vendor)`,
    `- Tree-sitter AST parsed: ${r.stats.astParsedFiles} files (${r.stats.astItems} items)`,
    `- Upstream copied into first-party hits: ${r.upstreamCopies.length}`,
    `- Duplicate first-party files: ${r.duplicateFiles.length}`,
    `- Tree-sitter AST clones (Type-1/Type-2): ${r.astClones.length}`,
    `- Code-graph module clones: ${r.graphClones.length} | duplicate symbols: ${r.duplicateSymbols.length}`,
    `- Text block clones: ${r.textBlocks.length} | SQL clones: ${r.sqlDuplicates.length}`,
    "",
    "## 1. Upstream / Vendor Code Copied into First-Party",
    "",
  ];
  for (const h of r.upstreamCopies.slice(0, 40)) {
    lines.push(`- **[${h.kind}]** \`${h.firstPartyBucket}\` <- \`${h.upstreamSource}\` (${h.fileCount} match(es), score=${h.score})`);
    for (const s of h.samples.slice(0, 3)) lines.push(`  - \`${s.firstParty}\` == \`${s.upstream}\``);
  }
  lines.push("", "## 2. Duplicate First-Party Files", "");
  for (const g of r.duplicateFiles.slice(0, 40)) {
    lines.push(`- \`${g.key}\` (${g.kind}, ${g.files.length}x): ${g.files.map((f) => `\`${f.repo}:${f.relPath}\``).join(", ")}`);
  }
  lines.push("", "## 3. Tree-sitter AST Clones", "");
  for (const c of r.astClones.slice(0, 40)) {
    lines.push(`- \`${c.key}\` (${c.symbolType}, ${c.tier}, ${c.lines}L): ${c.members.map((m) => `\`${m.repo}:${m.relPath}:${m.startLine}\``).join(", ")}`);
  }
  lines.push("", "## 4. Code-Graph Module Clones", "");
  for (const p of r.graphClones.slice(0, 40)) {
    lines.push(`- \`${p.a}\` <-> \`${p.b}\` (score=${p.score}, sym=${p.symbolJaccard}, imp=${p.importJaccard}, shared: ${p.sharedSymbols.join(", ")})`);
  }
  return lines.join("\n");
}

export function renderCodeSummary(r: CodeDedupReport, opts: { ezaTree?: boolean } = {}): string {
  const b = opts.ezaTree ? "├── " : "- ";
  const lines = [
    `code-dedup: ${r.scannedRepos.length} repos under ${r.scanRoot} (${r.stats.durationMs} ms) [git=ok ast-grep=${r.tools.astGrep ? "ok" : "off"} web-tree-sitter=${r.tools.webTreeSitter ? "ok" : "ready"} difft=${r.tools.difft ? "ok" : "off"} rg=${r.tools.rg ? "ok" : "off"} fzf=${r.tools.fzf ? "ok" : "fallback"}]`,
    `files=${r.stats.trackedFiles} (firstParty=${r.stats.firstPartyFiles}, upstream=${r.stats.upstreamFiles}) astFiles=${r.stats.astParsedFiles} astItems=${r.stats.astItems}`,
    `upstreamCopies=${r.upstreamCopies.length} dupFiles=${r.duplicateFiles.length} astClones=${r.astClones.length} graphClones=${r.graphClones.length} dupSymbols=${r.duplicateSymbols.length} textBlocks=${r.textBlocks.length} sql=${r.sqlDuplicates.length}`,
  ];
  if (r.upstreamCopies.length) {
    lines.push("", "=== Upstream / Vendor Code Copied Into First-Party (Top 20) ===");
    for (const h of r.upstreamCopies.slice(0, 20)) {
      lines.push(`  [${h.kind} | ${h.fileCount}x | score=${h.score}] ${h.firstPartyBucket} <- ${h.upstreamSource}`);
      for (const s of h.samples.slice(0, 2)) lines.push(`    ${b}${s.firstParty} == ${s.upstream}`);
    }
  }
  if (r.duplicateFiles.length) {
    lines.push("", "=== Duplicate First-Party Files (Top 20) ===");
    for (const g of r.duplicateFiles.slice(0, 20)) {
      lines.push(`  [${g.files.length}x | ${g.kind}] ${g.key}`);
      for (const f of g.files.slice(0, 4)) lines.push(`    ${b}${f.repo}:${f.relPath}`);
    }
  }
  if (r.astClones.length) {
    lines.push("", "=== Tree-sitter AST Clones (Top 20) ===");
    for (const c of r.astClones.slice(0, 20)) {
      lines.push(`  [${c.members.length}x | ${c.tier} | ${c.symbolType} | ${c.lines}L] ${c.key}`);
      for (const m of c.members.slice(0, 4)) lines.push(`    ${b}${m.repo}:${m.relPath}:${m.startLine}-${m.endLine}`);
    }
  }
  if (r.graphClones.length) {
    lines.push("", "=== Code-Graph Module Clones (Top 15) ===");
    for (const p of r.graphClones.slice(0, 15)) {
      lines.push(`  [score=${p.score} sym=${p.symbolJaccard} imp=${p.importJaccard}] ${p.a} <-> ${p.b} (${p.sharedSymbols.slice(0, 4).join(", ")})`);
    }
  }
  return lines.join("\n");
}

function launchCodeFzfInteractive(r: CodeDedupReport): number {
  const fzfBin = Bun.which("fzf");
  if (!fzfBin) {
    console.error("fzf binary not found on PATH; pass --fuzzy <query> for built-in fuzzy filtering.");
    return 1;
  }
  const rows: string[] = [];
  for (const h of r.upstreamCopies) {
    for (const s of h.samples) {
      rows.push(`UPSTREAM(${h.kind})\t${h.firstPartyBucket}\t${s.firstParty} == ${s.upstream}\t${s.firstParty.replace(/^[^:]+:/, "")}`);
    }
  }
  for (const g of r.duplicateFiles) {
    const base = g.files[0]?.absPath ?? "";
    for (const f of g.files) {
      rows.push(`DUP-FILE(${g.kind})\t${g.key}\t${f.repo}:${f.relPath}\t${f.absPath}\t${base}`);
    }
  }
  for (const c of r.astClones) {
    const base = c.members[0]?.absPath ?? "";
    for (const m of c.members) {
      rows.push(`AST(${c.tier})\t${c.key}\t${m.repo}:${m.relPath}:${m.startLine}\t${m.absPath}\t${base}`);
    }
  }
  if (!rows.length) {
    console.log("No code duplicates or copied upstream files found.");
    return 0;
  }
  const preview = Bun.which("difft")
    ? "difft --color=always {5} {4} 2>nul || rg -n --color=always . {4}"
    : "rg -n --color=always . {4}";
  const proc = Bun.spawnSync(
    [
      fzfBin,
      "--delimiter", "\t",
      "--with-nth", "1,2,3",
      "--header", `code-dedup (${r.upstreamCopies.length} upstream, ${r.duplicateFiles.length} dupFiles, ${r.astClones.length} astClones)`,
      "--preview", preview,
      "--preview-window", "right:50%:wrap",
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
    "Usage: bun tools/code-dedup.ts [options] [<dir>...]",
    "",
    "Detect code duplication, AST/code-graph clones, and upstream repos copied into first-party",
    "across all checkouts under SRC_ROOT (workspace parent) using git, ast-grep, web-tree-sitter, difft, rg, eza, and fzf.",
    "",
    "Options:",
    "  --root <dir>         Workspace parent directory (default: SRC_ROOT or parent of repo root)",
    "  --repo <name>        Filter first-party analysis to specific repo name(s) (repeatable)",
    "  --fuzzy, -q <query>  Fuzzy filter all findings (uses fzf --filter when installed)",
    "  --fzf, -i            Interactive fzf browser with live difft/rg preview",
    "  --verify-difft       Run difft (Difftastic) on AST-identical file groups",
    "  --fast               Skip full Tree-sitter AST parsing inside upstream/vendor trees",
    "  --min-lines <n>      Minimum lines for AST/text block clones (default: 6)",
    "  --threshold <0..1>   Code-graph similarity threshold (default: 0.72)",
    "  --include-tests      Include test/fixture paths",
    "  --db [path]          Save cross-repo SQLite code graph (default: .private/code-dedup.db)",
    "  --eza-tree           Render tree branches in terminal summary",
    "  --no-fzf             Force pure-Bun fuzzy scorer",
    "  --json               Print JSON report to stdout",
    "  --no-write           Skip writing .private/code-dedup.{json,md}",
    "  --help, -h           Show help",
  ].join("\n"));
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.includes("--help") || args.includes("-h")) {
    printHelp();
    process.exit(0);
  }
  const flagValues = (names: string[]): string[] => {
    const vals: string[] = [];
    for (let i = 0; i < args.length; i++) {
      if (names.includes(args[i]!) && args[i + 1] && !args[i + 1]!.startsWith("-")) vals.push(args[i + 1]!);
    }
    return vals;
  };
  const valueFlags = new Set(["--root", "--repo", "--fuzzy", "-q", "--min-lines", "--threshold"]);
  const positional: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i]!;
    if (valueFlags.has(a) || (a === "--db" && args[i + 1] && !args[i + 1]!.startsWith("-"))) {
      i++;
      continue;
    }
    if (!a.startsWith("-")) positional.push(a);
  }

  const dbIdx = args.indexOf("--db");
  const dbPath =
    dbIdx >= 0
      ? args[dbIdx + 1] && !args[dbIdx + 1]!.startsWith("-")
        ? resolve(args[dbIdx + 1]!)
        : privatePath("code-dedup.db", repoRoot, true)
      : undefined;

  const minLinesRaw = flagValues(["--min-lines"])[0];
  const thresholdRaw = flagValues(["--threshold"])[0];
  const report = await runCodeAudit({
    scanRoot: flagValues(["--root"])[0],
    dirs: positional.length ? positional : undefined,
    repos: flagValues(["--repo"]).flatMap((s) => s.split(",").filter(Boolean)),
    fuzzyQuery: flagValues(["--fuzzy", "-q"])[0],
    fast: args.includes("--fast"),
    minLines: minLinesRaw ? Number(minLinesRaw) : undefined,
    threshold: thresholdRaw ? Number(thresholdRaw) : undefined,
    includeTests: args.includes("--include-tests"),
    verifyDifft: args.includes("--verify-difft"),
    useFzf: !args.includes("--no-fzf"),
    dbPath,
  });

  if (!args.includes("--no-write")) {
    await Bun.write(privatePath("code-dedup.json", repoRoot, true), `${JSON.stringify(report, null, 2)}\n`);
    await Bun.write(privatePath("code-dedup.md", repoRoot, true), renderCodeMarkdown(report));
  }

  if (args.includes("--fzf") || args.includes("-i")) {
    process.exit(launchCodeFzfInteractive(report));
  } else if (args.includes("--json")) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(renderCodeSummary(report, { ezaTree: args.includes("--eza-tree") }));
  }
}
