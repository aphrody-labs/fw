#!/usr/bin/env bun
import { CString, dlopen, FFIType, ptr as ffiPtr, suffix } from "bun:ffi";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";

const TREE_SITTER_GRAMMARS = ["rust","typescript","tsx","python","c","cpp","java","json","bash"] as const;
const SUBCOMMANDS = ["status", "heretic", "neon", "ghidra", "craft", "ast"] as const;
type Subcommand = (typeof SUBCOMMANDS)[number];

function resolveRoot(): string {
  const candidates = [process.env.FAIRWIND_ROOT, process.cwd(), dirname(dirname(process.execPath)), "C:\\src\\fairwind"].filter((v): v is string => Boolean(v));
  for (const dir of candidates) {
    if (existsSync(join(dir, "core")) || existsSync(join(dir, "native", "fairwind-abi"))) return resolve(dir);
  }
  return resolve(process.cwd());
}

function countFiles(dir: string): number {
  if (!existsSync(dir)) return 0;
  let total = 0;
  const stack = [dir];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    for (const entry of readdirSync(cur, { withFileTypes: true })) {
      const full = join(cur, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile()) total++;
    }
  }
  return total;
}

function findAbiLibrary(root: string): string | null {
  const libName = process.platform === "win32" ? `fairwind_abi.${suffix}` : `libfairwind_abi.${suffix}`;
  for (const p of [join(root, "native", "fairwind-abi", "target", "release", libName), join(root, "native", "fairwind-abi", "target", "debug", libName), join(root, "dist", libName)]) {
    if (existsSync(p)) return p;
  }
  return null;
}

function probeRustAbi(root: string) {
  const libPath = findAbiLibrary(root);
  if (!libPath) return { loaded: false, path: null, version: null, health: null };
  try {
    const lib = dlopen(libPath, { fairwind_abi_version: { args: [], returns: FFIType.u32 }, fairwind_health_json: { args: [], returns: FFIType.ptr } });
    const version = Number(lib.symbols.fairwind_abi_version());
    const ptr = lib.symbols.fairwind_health_json();
    const raw = ptr ? new CString(ptr).toString() : "{}";
    lib.close();
    return { loaded: true, path: libPath, version, health: JSON.parse(raw) };
  } catch (err) {
    return { loaded: false, path: libPath, version: null, health: null, error: err instanceof Error ? err.message : String(err) };
  }
}

function runSubprocess(cmd: string[], cwd: string): number {
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
  return proc.exitCode ?? 1;
}

function printJson(payload: unknown): void { console.log(JSON.stringify(payload, null, 2)); }

function cmdStatus(root: string): number {
  const abi = probeRustAbi(root);
  printJson({
    ok: true,
    product: "fairwind",
    name: "fairwind",
    runtime: { bun: Bun.version, platform: process.platform, arch: process.arch },
    root,
    subcommands: SUBCOMMANDS,
    abiLoaded: abi.loaded,
    tree_sitter_grammars: TREE_SITTER_GRAMMARS,
    absorbed: {
      heretic: { path: "core/py/heretic", files: countFiles(join(root, "core", "py", "heretic")) },
      neon: { path: "core/ts/neon", files: countFiles(join(root, "core", "ts", "neon")) },
      craftLibs: { path: "core/rs/craft-libs", files: countFiles(join(root, "core", "rs", "craft-libs")) },
      craftAnything: { path: "core/rs/craft-anything", files: countFiles(join(root, "core", "rs", "craft-anything")) },
    },
    external: {
      ghidra: { path: "packages/ghidra", present: existsSync(join(root, "packages", "ghidra")) },
    },
    abi,
  });
  return 0;
}

function cmdHeretic(root: string, args: string[]): number {
  const dir = join(root, "core", "py", "heretic");
  if (args.length === 0 || args[0] === "--info") { printJson({ ok: true, bridge: "uv/PyO3", path: dir, present: existsSync(dir), files: countFiles(dir), usage: "fairwind heretic <args...>" }); return 0; }
  return runSubprocess(["uv", "run", "heretic", ...args], root);
}

function cmdNeon(root: string, args: string[]): number {
  const dir = join(root, "core", "ts", "neon");
  if (args.length === 0 || args[0] === "--info") { printJson({ ok: true, bridge: "bun-ts", path: dir, present: existsSync(dir), files: countFiles(dir), usage: "fairwind neon <script|args...>" }); return 0; }
  return runSubprocess(["bun", ...args], existsSync(dir) ? dir : root);
}

function cmdGhidra(root: string, args: string[]): number {
  const dir = join(root, "packages", "ghidra");
  const gradlew = join(dir, process.platform === "win32" ? "gradlew.bat" : "gradlew");
  if (args.length === 0 || args[0] === "--info") { printJson({ ok: true, bridge: "jvm-gradle-headless", path: dir, present: existsSync(dir), gradlew: existsSync(gradlew), usage: "fairwind ghidra <args...>" }); return 0; }
  return runSubprocess(existsSync(gradlew) ? [gradlew, ...args] : ["gradle", ...args], existsSync(dir) ? dir : root);
}

function cmdCraft(root: string, args: string[]): number {
  printJson({ ok: true, bridge: "rust-c-abi", args, absorbed: { craftLibs: countFiles(join(root, "core", "rs", "craft-libs")), craftAnything: countFiles(join(root, "core", "rs", "craft-anything")) }, abi: probeRustAbi(root) });
  return 0;
}

function parseAstWithAbi(root: string, lang: string, filePath: string) { const libPath = findAbiLibrary(root); if (!libPath) return { ok: false, error: "abi not found" }; if (!existsSync(filePath)) return { ok: false, error: "file not found" }; const lb = new TextEncoder().encode(lang); const sb = new Uint8Array(readFileSync(filePath)); const ob = new Uint8Array(4096); const lib = dlopen(libPath, { fairwind_parse_summary: { args: [FFIType.ptr, FFIType.usize, FFIType.ptr, FFIType.usize, FFIType.ptr, FFIType.usize], returns: FFIType.usize } }); const n = Number(lib.symbols.fairwind_parse_summary(ffiPtr(lb), lb.byteLength, ffiPtr(sb), sb.byteLength, ffiPtr(ob), ob.byteLength)); lib.close(); if (n <= 0) return { ok: false, error: "parse failed" }; return { ...JSON.parse(new TextDecoder().decode(ob.subarray(0, n))), file: filePath, bytes: sb.byteLength }; }
function cmdAst(root: string, args: string[]): number {
  if (args.length >= 2) { const res = parseAstWithAbi(root, args[0], resolve(root, args[1])); printJson({ bridge: "tree-sitter-c-abi", ...res }); return res.ok ? 0 : 1; }
  const target = resolve(root, args[0] ?? "core");
  const exts: Record<string, number> = {};
  let fileCount = 0;
  if (existsSync(target)) {
    const st = statSync(target);
    if (st.isFile()) { fileCount = 1; const e = extname(target) || "(none)"; exts[e] = 1; }
    else if (st.isDirectory()) { for (const entry of readdirSync(target, { withFileTypes: true })) { if (entry.isFile()) { fileCount++; const e = extname(entry.name) || "(none)"; exts[e] = (exts[e] ?? 0) + 1; } } }
  }
  printJson({ ok: true, bridge: "tree-sitter", target, exists: existsSync(target), fileCount, extensions: exts });
  return 0;
}

function main(): number {
  const [sub, ...rest] = Bun.argv.slice(2);
  const root = resolveRoot();
  if (!sub || sub === "--help" || sub === "-h") { printJson({ ok: true, product: "fairwind", usage: "fairwind <status|heretic|neon|ghidra|craft|ast> [args...]", subcommands: SUBCOMMANDS }); return 0; }
  switch (sub as Subcommand) {
    case "status": return cmdStatus(root);
    case "heretic": return cmdHeretic(root, rest);
    case "neon": return cmdNeon(root, rest);
    case "ghidra": return cmdGhidra(root, rest);
    case "craft": return cmdCraft(root, rest);
    case "ast": return cmdAst(root, rest);
    default: console.error(JSON.stringify({ ok: false, error: `Unknown subcommand: ${sub}` })); return 1;
  }
}

process.exit(main());
