import { CString, dlopen, FFIType, ptr as ffiPtr, suffix } from "bun:ffi";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join, relative, resolve } from "node:path";

export interface TargetModuleConfig {
  id: string;
  path: string;
  kind: string;
  grammar: string;
  gate: string | null;
  sampleFile: string | null;
  autoUpgrade: boolean;
}

export interface TargetArtifactConfig {
  crate?: string;
  entry?: string;
  buildOutput?: string;
  buildOutputUnix?: string;
  targetPath: string;
  targetPathUnix: string;
  distPath: string;
  distPathUnix: string;
  profile: string;
}

export interface FwTargetsConfig {
  $schema: string;
  product: string;
  agent: {
    id: string;
    upstreamStrategy: string;
    targetDir: string;
    manifestPath: string;
    historyPath: string;
    autoUpgradeCommand: string;
  };
  artifacts: {
    agyNativeLto: TargetArtifactConfig;
    fwAbiLto: TargetArtifactConfig;
    fwCliAot: TargetArtifactConfig;
  };
  modules: TargetModuleConfig[];
}

export interface ModuleSnapshot {
  id: string;
  path: string;
  kind: string;
  grammar: string;
  present: boolean;
  autoUpgrade: boolean;
  files: number;
  bytes: number;
  digest: string;
  topExtensions: Record<string, number>;
  astProbe: {
    ok: boolean;
    file?: string;
    rootKind?: string;
    nodeCount?: number;
    hasError?: boolean;
    error?: string;
  } | null;
}

export interface ArtifactSnapshot {
  name: string;
  profile: string;
  path: string;
  exists: boolean;
  bytes: number;
  sha256: string | null;
  pe: Record<string, unknown> | null;
}

export interface TargetManifest {
  schema: "fw.target-manifest.v1";
  generatedAt: string;
  root: string;
  gitHead: string | null;
  runtime: {
    bun: string;
    platform: string;
    arch: string;
  };
  upstreamAgy: {
    path: string | null;
    exists: boolean;
    version: string | null;
    readOnlySignedUpstream: boolean;
    pe: Record<string, unknown> | null;
  };
  repoDigest: string;
  totalTrackedFiles: number;
  totalTrackedBytes: number;
  modules: ModuleSnapshot[];
  artifacts: Record<string, ArtifactSnapshot>;
}

const SKIP_DIRS = new Set([
  ".git",
  ".venv",
  ".ruff_cache",
  "node_modules",
  "target",
  "dist",
  "__pycache__",
  ".next",
]);

export function loadTargetsConfig(root: string): FwTargetsConfig {
  const cfgPath = join(root, "fw.targets.json");
  if (!existsSync(cfgPath)) {
    throw new Error(`Missing fw.targets.json at ${cfgPath}`);
  }
  return JSON.parse(readFileSync(cfgPath, "utf8")) as FwTargetsConfig;
}

let winShadowDllCache: { src: string; mtimeMs: number; shadow: string } | null = null;

export function findAbiLibrary(root: string): string | null {
  const libName = process.platform === "win32" ? `fw_abi.${suffix}` : `libfw_abi.${suffix}`;
  const candidates = [
    join(root, "target", "agy", libName),
    join(root, "native", "fw-abi", "target", "release", libName),
    join(root, "native", "fw-abi", "target", "debug", libName),
    join(root, "dist", libName),
  ];
  let found: string | null = null;
  for (const p of candidates) {
    if (existsSync(p)) {
      found = p;
      break;
    }
  }
  if (!found) return null;
  if (process.platform === "win32") {
    try {
      const mtimeMs = statSync(found).mtimeMs;
      if (winShadowDllCache && winShadowDllCache.src === found && winShadowDllCache.mtimeMs === mtimeMs && existsSync(winShadowDllCache.shadow)) {
        return winShadowDllCache.shadow;
      }
      const shadowDir = join(tmpdir(), "fw-abi-shadow");
      mkdirSync(shadowDir, { recursive: true });
      const shadowPath = join(shadowDir, `fw_abi_${process.pid}_${Math.trunc(mtimeMs)}.dll`);
      copyFileSync(found, shadowPath);
      winShadowDllCache = { src: found, mtimeMs, shadow: shadowPath };
      return shadowPath;
    } catch {
      return found;
    }
  }
  return found;
}

export function probeRustAbi(root: string) {
  const libPath = findAbiLibrary(root);
  if (!libPath) return { loaded: false, path: null, version: null, health: null };
  try {
    const lib = dlopen(libPath, {
      fw_abi_version: { args: [], returns: FFIType.u32 },
      fw_health_json: { args: [], returns: FFIType.ptr },
    });
    const version = Number(lib.symbols.fw_abi_version());
    const ptr = lib.symbols.fw_health_json();
    const raw = ptr ? new CString(ptr).toString() : "{}";
    lib.close();
    return { loaded: true, path: libPath, version, health: JSON.parse(raw) };
  } catch (err) {
    return {
      loaded: false,
      path: libPath,
      version: null,
      health: null,
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

const TEXT_ENC = new TextEncoder();
const TEXT_DEC = new TextDecoder();

function invokeAbiJson(
  root: string,
  symbolName: string,
  argTypes: readonly FFIType[],
  outCap: number,
  invoke: (fn: (...args: unknown[]) => unknown, outBuf: Uint8Array) => number,
  failError: string,
  extraOnError?: Record<string, unknown>,
): Record<string, unknown> {
  const libPath = findAbiLibrary(root);
  if (!libPath) return { ok: false, ...extraOnError, error: "abi not found" };
  const ob = new Uint8Array(outCap);
  const lib = dlopen(libPath, {
    [symbolName]: { args: [...argTypes, FFIType.ptr, FFIType.usize], returns: FFIType.usize },
  });
  let n = 0;
  try {
    const sym = (lib.symbols as Record<string, (...args: unknown[]) => unknown>)[symbolName];
    n = Number(invoke(sym, ob));
  } finally {
    lib.close();
  }
  if (n <= 0) return { ok: false, ...extraOnError, error: failError };
  return JSON.parse(TEXT_DEC.decode(ob.subarray(0, n))) as Record<string, unknown>;
}

export function parseAstWithAbi(root: string, lang: string, filePath: string) {
  if (!existsSync(filePath)) return { ok: false, error: "file not found" };
  const lb = TEXT_ENC.encode(lang);
  const sb = new Uint8Array(readFileSync(filePath));
  const res = invokeAbiJson(
    root,
    "fw_parse_summary",
    [FFIType.ptr, FFIType.usize, FFIType.ptr, FFIType.usize],
    4096,
    (fn, ob) => Number(fn(ffiPtr(lb), lb.byteLength, ffiPtr(sb), sb.byteLength, ffiPtr(ob), ob.byteLength)),
    "parse failed",
  );
  return res.ok === false ? res : { ...res, file: filePath, bytes: sb.byteLength };
}

export function inspectPeWithAbi(root: string, filePath: string): Record<string, unknown> {
  if (!existsSync(filePath)) return { ok: false, path: filePath, error: "file not found" };
  const pb = TEXT_ENC.encode(filePath);
  return invokeAbiJson(
    root,
    "fw_inspect_pe",
    [FFIType.ptr, FFIType.usize],
    65536,
    (fn, ob) => Number(fn(ffiPtr(pb), pb.byteLength, ffiPtr(ob), ob.byteLength)),
    "pe inspect failed or buffer exceeded",
    { path: filePath },
  );
}

export function astSymbolsWithAbi(root: string, lang: string, filePath: string): Record<string, unknown> {
  if (!existsSync(filePath)) return { ok: false, error: "file not found" };
  const lb = TEXT_ENC.encode(lang);
  const sb = new Uint8Array(readFileSync(filePath));
  const res = invokeAbiJson(
    root,
    "fw_ast_symbols",
    [FFIType.ptr, FFIType.usize, FFIType.ptr, FFIType.usize],
    65536,
    (fn, ob) => Number(fn(ffiPtr(lb), lb.byteLength, ffiPtr(sb), sb.byteLength, ffiPtr(ob), ob.byteLength)),
    "ast symbols failed",
  );
  return res.ok === false ? res : { ...res, file: filePath };
}

export function astQueryWithAbi(root: string, lang: string, filePath: string, query: string): Record<string, unknown> {
  if (!existsSync(filePath)) return { ok: false, error: "file not found" };
  const lb = TEXT_ENC.encode(lang);
  const sb = new Uint8Array(readFileSync(filePath));
  const qb = TEXT_ENC.encode(query);
  const res = invokeAbiJson(
    root,
    "fw_ast_query",
    [FFIType.ptr, FFIType.usize, FFIType.ptr, FFIType.usize, FFIType.ptr, FFIType.usize],
    65536,
    (fn, ob) =>
      Number(fn(ffiPtr(lb), lb.byteLength, ffiPtr(sb), sb.byteLength, ffiPtr(qb), qb.byteLength, ffiPtr(ob), ob.byteLength)),
    "ast query failed",
  );
  return res.ok === false ? res : { ...res, file: filePath };
}

export function identifyFileWithAbi(root: string, filePath: string): Record<string, unknown> {
  if (!existsSync(filePath)) return { ok: false, error: "file not found" };
  const pb = TEXT_ENC.encode(filePath);
  return invokeAbiJson(
    root,
    "fw_identify_file",
    [FFIType.ptr, FFIType.usize],
    16384,
    (fn, ob) => Number(fn(ffiPtr(pb), pb.byteLength, ffiPtr(ob), ob.byteLength)),
    "identify failed",
  );
}

export function disasmX64WithAbi(root: string, filePath: string, maxInsns = 12): Record<string, unknown> {
  if (!existsSync(filePath)) return { ok: false, error: "file not found" };
  const pb = TEXT_ENC.encode(filePath);
  return invokeAbiJson(
    root,
    "fw_disasm_x64",
    [FFIType.ptr, FFIType.usize, FFIType.usize],
    65536,
    (fn, ob) => Number(fn(ffiPtr(pb), pb.byteLength, maxInsns, ffiPtr(ob), ob.byteLength)),
    "disasm failed",
  );
}

export function cryptoDigestWithAbi(root: string, input: string): Record<string, unknown> {
  const db = TEXT_ENC.encode(input);
  return invokeAbiJson(
    root,
    "fw_crypto_digest",
    [FFIType.ptr, FFIType.usize],
    8192,
    (fn, ob) => Number(fn(ffiPtr(db), db.byteLength, ffiPtr(ob), ob.byteLength)),
    "crypto digest failed",
  );
}

export function gitInspectWithAbi(root: string, repoDir = root): Record<string, unknown> {
  const pb = TEXT_ENC.encode(repoDir);
  return invokeAbiJson(
    root,
    "fw_git_inspect",
    [FFIType.ptr, FFIType.usize],
    8192,
    (fn, ob) => Number(fn(ffiPtr(pb), pb.byteLength, ffiPtr(ob), ob.byteLength)),
    "git inspect failed",
  );
}

export function resolveUpstreamAgyPath(): string | null {
  const explicit = process.env.AGY_UPSTREAM_BIN;
  if (explicit && existsSync(explicit)) return resolve(explicit);
  if (process.platform === "win32") {
    const localApp = process.env.LOCALAPPDATA;
    if (localApp) {
      const p = join(localApp, "agy", "bin", "agy.exe");
      if (existsSync(p)) return p;
    }
  } else {
    const home = process.env.HOME;
    if (home) {
      for (const rel of [".local/share/agy/bin/agy", ".agy/bin/agy"]) {
        const p = join(home, rel);
        if (existsSync(p)) return p;
      }
    }
  }
  return null;
}

export function probeUpstreamAgyVersion(agyPath: string | null): string | null {
  if (!agyPath || !existsSync(agyPath)) return null;
  try {
    const res = Bun.spawnSync([agyPath, "--version"], { stdout: "pipe", stderr: "pipe" });
    if (res.exitCode !== 0) return null;
    const text = res.stdout.toString().trim();
    return text.length > 0 ? text.split(/\r?\n/)[0] : null;
  } catch {
    return null;
  }
}

export function getGitHead(root: string): string | null {
  try {
    const res = Bun.spawnSync(["git", "rev-parse", "--short=12", "HEAD"], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    });
    if (res.exitCode !== 0) return null;
    const out = res.stdout.toString().trim();
    return out || null;
  } catch {
    return null;
  }
}

export function hashFileSha256(filePath: string): string | null {
  if (!existsSync(filePath)) return null;
  const hasher = new Bun.CryptoHasher("sha256");
  const data = readFileSync(filePath);
  hasher.update(data);
  return hasher.digest("hex");
}

function scanModuleTree(root: string, mod: TargetModuleConfig): ModuleSnapshot {
  const fullDir = resolve(root, mod.path);
  if (!existsSync(fullDir)) {
    return {
      id: mod.id,
      path: mod.path,
      kind: mod.kind,
      grammar: mod.grammar,
      present: false,
      autoUpgrade: mod.autoUpgrade,
      files: 0,
      bytes: 0,
      digest: "",
      topExtensions: {},
      astProbe: null,
    };
  }

  const paths: string[] = [];
  const stack = [fullDir];
  const extCounts: Record<string, number> = {};
  let totalBytes = 0;

  while (stack.length > 0) {
    const cur = stack.pop()!;
    const entries = readdirSync(cur, { withFileTypes: true });
    for (const entry of entries) {
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name)) {
          stack.push(join(cur, entry.name));
        }
      } else if (entry.isFile()) {
        const full = join(cur, entry.name);
        paths.push(full);
        const st = statSync(full);
        totalBytes += st.size;
        const ext = extname(entry.name) || "(none)";
        extCounts[ext] = (extCounts[ext] ?? 0) + 1;
      }
    }
  }

  paths.sort();
  const treeHasher = new Bun.CryptoHasher("sha256");
  // For large external trees (> 6000 files), hash relative path + size + mtimeMs for fast deterministic drift detection;
  // for all owned in-repo code, hash file contents directly.
  const hashContents = mod.autoUpgrade && paths.length <= 6000;
  for (const p of paths) {
    const rel = relative(root, p).replace(/\\/g, "/");
    const st = statSync(p);
    treeHasher.update(`${rel}:${st.size}:`);
    if (hashContents && st.size <= 2 * 1024 * 1024) {
      treeHasher.update(readFileSync(p));
    } else {
      treeHasher.update(`${Math.trunc(st.mtimeMs)}\n`);
    }
  }

  const topExtensions: Record<string, number> = {};
  for (const [k, v] of Object.entries(extCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 6)) {
    topExtensions[k] = v;
  }

  let astProbe: ModuleSnapshot["astProbe"] = null;
  if (mod.sampleFile) {
    const sampleAbs = resolve(root, mod.sampleFile);
    if (existsSync(sampleAbs)) {
      const grammar = sampleAbs.endsWith(".json") ? "json" : mod.grammar;
      const res = parseAstWithAbi(root, grammar, sampleAbs) as Record<string, unknown>;
      astProbe = {
        ok: Boolean(res.ok),
        file: mod.sampleFile,
        rootKind: typeof res.rootKind === "string" ? res.rootKind : undefined,
        nodeCount: typeof res.nodeCount === "number" ? res.nodeCount : undefined,
        hasError: typeof res.hasError === "boolean" ? res.hasError : undefined,
        error: typeof res.error === "string" ? res.error : undefined,
      };
    }
  }

  return {
    id: mod.id,
    path: mod.path,
    kind: mod.kind,
    grammar: mod.grammar,
    present: true,
    autoUpgrade: mod.autoUpgrade,
    files: paths.length,
    bytes: totalBytes,
    digest: treeHasher.digest("hex"),
    topExtensions,
    astProbe,
  };
}

function inspectArtifact(root: string, name: string, cfg: TargetArtifactConfig): ArtifactSnapshot {
  const rel = process.platform === "win32" ? cfg.targetPath : cfg.targetPathUnix;
  const fallbackRel = process.platform === "win32" ? cfg.distPath : cfg.distPathUnix;
  const primaryAbs = resolve(root, rel);
  const abs = existsSync(primaryAbs) ? primaryAbs : resolve(root, fallbackRel);
  const exists = existsSync(abs);
  const bytes = exists ? statSync(abs).size : 0;
  let pe: Record<string, unknown> | null = null;
  let sha256: string | null = null;
  if (exists) {
    if (process.platform === "win32" && (abs.endsWith(".exe") || abs.endsWith(".dll"))) {
      const inspected = inspectPeWithAbi(root, abs);
      if (inspected.ok) {
        pe = inspected;
        sha256 = typeof inspected.sha256 === "string" ? inspected.sha256 : null;
      }
    }
    if (!sha256) {
      sha256 = hashFileSha256(abs);
    }
  }
  return {
    name,
    profile: cfg.profile,
    path: abs,
    exists,
    bytes,
    sha256,
    pe,
  };
}

export function buildTargetManifest(root: string): TargetManifest {
  const cfg = loadTargetsConfig(root);
  const modules = cfg.modules.map((m) => scanModuleTree(root, m));

  const repoHasher = new Bun.CryptoHasher("sha256");
  let totalTrackedFiles = 0;
  let totalTrackedBytes = 0;
  for (const m of modules) {
    totalTrackedFiles += m.files;
    totalTrackedBytes += m.bytes;
    if (m.autoUpgrade) {
      repoHasher.update(`${m.id}:${m.digest}\n`);
    }
  }

  const upstreamPath = resolveUpstreamAgyPath();
  const upstreamExists = Boolean(upstreamPath && existsSync(upstreamPath));
  const upstreamPe =
    upstreamPath && upstreamExists && process.platform === "win32"
      ? inspectPeWithAbi(root, upstreamPath)
      : null;

  const artifacts: Record<string, ArtifactSnapshot> = {
    agyNativeLto: inspectArtifact(root, "agyNativeLto", cfg.artifacts.agyNativeLto),
    fwAbiLto: inspectArtifact(root, "fwAbiLto", cfg.artifacts.fwAbiLto),
    fwCliAot: inspectArtifact(root, "fwCliAot", cfg.artifacts.fwCliAot),
  };

  return {
    schema: "fw.target-manifest.v1",
    generatedAt: new Date().toISOString(),
    root,
    gitHead: getGitHead(root),
    runtime: {
      bun: Bun.version,
      platform: process.platform,
      arch: process.arch,
    },
    upstreamAgy: {
      path: upstreamPath,
      exists: upstreamExists,
      version: probeUpstreamAgyVersion(upstreamPath),
      readOnlySignedUpstream: true,
      pe: upstreamPe,
    },
    repoDigest: repoHasher.digest("hex"),
    totalTrackedFiles,
    totalTrackedBytes,
    modules,
    artifacts,
  };
}

export function writeTargetManifest(root: string, manifest: TargetManifest): string {
  const cfg = loadTargetsConfig(root);
  const manifestPath = resolve(root, cfg.agent.manifestPath);
  const targetDir = resolve(root, cfg.agent.targetDir);
  mkdirSync(targetDir, { recursive: true });
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  writeFileSync(
    join(targetDir, "target-profile.json"),
    `${JSON.stringify(
      {
        product: "fw",
        agent: cfg.agent,
        repoDigest: manifest.repoDigest,
        gitHead: manifest.gitHead,
        generatedAt: manifest.generatedAt,
        artifacts: Object.fromEntries(
          Object.entries(manifest.artifacts).map(([k, v]) => [
            k,
            { path: v.path, exists: v.exists, bytes: v.bytes, profile: v.profile, sha256: v.sha256 },
          ]),
        ),
      },
      null,
      2,
    )}\n`,
    "utf8",
  );
  return manifestPath;
}

export function readTargetManifest(root: string): TargetManifest | null {
  const cfg = loadTargetsConfig(root);
  const manifestPath = resolve(root, cfg.agent.manifestPath);
  if (!existsSync(manifestPath)) return null;
  try {
    return JSON.parse(readFileSync(manifestPath, "utf8")) as TargetManifest;
  } catch {
    return null;
  }
}
