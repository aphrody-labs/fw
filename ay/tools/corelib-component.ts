#!/usr/bin/env bun
// Turn the corelib into WebAssembly components, compose them into one WASI command and run it on wasmtime.
//   bun tools/corelib-component.ts               build, componentize, compose, run with the wasmtime found (or installed) here
//   bun tools/corelib-component.ts --no-run      stop after composing
//   bun tools/corelib-component.ts --remote      sync the sources to the build host of ay.toml and do all of it there
//   options: --log PATH (where the transcript is written), --dir PATH (remote work dir, default /srv/xbuild/asm-guide/corelib-component)
// Pieces (wit/ay-corelib.wit): corelib (C built with -DAY_COMPONENT, wrapped by `wasm-tools component new`), app (hello.wat with
// renamed import and export), console and driver (hand-written WAT); `wac` wires them, wasmtime runs the result.
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";

import { buildWasm } from "./corelib.ts";

const root = resolve(import.meta.dir, "..");
const GOLDEN = "corelib/tests/golden/boot.txt";
const WIT = "corelib/wit";
const PKG = "0.1.0";

type Result = { status: number; out: string };

function sh(cmd: string[], opts: { cwd?: string; stdin?: Uint8Array } = {}): Result {
  const p = Bun.spawnSync(cmd, { cwd: opts.cwd ?? root, stdin: opts.stdin, stdout: "pipe", stderr: "pipe" });
  return { status: p.exitCode ?? -1, out: p.stdout.toString() + p.stderr.toString() };
}

function must(cmd: string[]): string {
  const r = sh(cmd);
  if (r.status !== 0) throw new Error(`${cmd.join(" ")} failed (${r.status}):\n${r.out}`);
  return r.out;
}

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

/** The app is hello.wat with its import and export renamed to the canonical ABI names of `world app`. */
export function appWat(hello: string): string {
  return hello
    .replace('(import "ay" "syscall"', `(import "ay:corelib/syscall@${PKG}" "call"`)
    .replace('(export "step"', `(export "ay:corelib/task@${PKG}#step"`);
}

/** The composition script: who plugs into whom. */
export function wacScript(): string {
  const i = (name: string) => `"ay:corelib/${name}@${PKG}"`;
  return [
    "package ay:os;",
    "",
    "let console = new ay:console { ... };",
    `let corelib = new ay:corelib { ${i("host")}: console[${i("host")}] };`,
    `let app = new ay:app { ${i("syscall")}: corelib[${i("syscall")}] };`,
    "let driver = new ay:driver {",
    `  ${i("scheduler")}: corelib[${i("scheduler")}],`,
    `  ${i("task")}: app[${i("task")}],`,
    "};",
    "",
    'export driver["wasi:cli/run@0.2.12"];',
    "",
  ].join("\n");
}

/** Empty when the transcript equals the golden one; otherwise the first difference. */
export function verdict(serial: string, golden: string): string[] {
  if (serial === golden) return [];
  const a = serial.split("\n");
  const b = golden.split("\n");
  const at = a.findIndex((l, i) => l !== b[i]);
  return [`transcript differs from golden at line ${at + 1}: ${JSON.stringify(a[at] ?? "")} vs ${JSON.stringify(b[at] ?? "")}`];
}

const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
const WASMTIME_ASSET: Record<string, string> = {
  linux: `wasmtime-%TAG%-${arch}-linux.tar.xz`,
  darwin: `wasmtime-%TAG%-${arch}-macos.tar.xz`,
  win32: `wasmtime-%TAG%-${arch}-windows.zip`,
};
const WAC_ASSET: Record<string, string> = {
  linux: `wac-cli-${arch}-unknown-linux-musl`,
  darwin: `wac-cli-${arch}-apple-darwin`,
  win32: "wac-cli-x86_64-pc-windows-gnu",
};

function onPath(name: string): string | undefined {
  const exts = process.platform === "win32" ? [".exe", ""] : [""];
  for (const dir of (process.env.PATH ?? "").split(delimiter)) {
    for (const ext of exts) if (dir && existsSync(join(dir, name + ext))) return join(dir, name + ext);
  }
  return undefined;
}

async function latest(repo: string): Promise<{ tag: string; assets: Map<string, string> }> {
  const res = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, { headers: { "user-agent": "ay-tools" } });
  if (!res.ok) throw new Error(`${repo}: latest release lookup failed (${res.status})`);
  const json = (await res.json()) as { tag_name: string; assets: { name: string; browser_download_url: string }[] };
  return { tag: json.tag_name, assets: new Map(json.assets.map(a => [a.name, a.browser_download_url])) };
}

async function download(url: string, to: string): Promise<void> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  mkdirSync(dirname(to), { recursive: true });
  writeFileSync(to, new Uint8Array(await res.arrayBuffer()));
}

/** `wac`: PATH first, else the latest GitHub release under target/tools. */
async function ensureWac(): Promise<string> {
  const found = onPath("wac");
  if (found) return found;
  const bin = join(root, "target/tools", process.platform === "win32" ? "wac.exe" : "wac");
  if (existsSync(bin)) return bin;
  const rel = await latest("bytecodealliance/wac");
  const url = rel.assets.get(WAC_ASSET[process.platform] ?? "");
  if (!url) throw new Error(`no wac release asset for ${process.platform}/${arch} in ${rel.tag}`);
  await download(url, bin);
  chmodSync(bin, 0o755);
  return bin;
}

/** `wasmtime`: --wasmtime PATH, PATH, else the latest GitHub release under target/tools. */
async function ensureWasmtime(): Promise<string> {
  const given = arg("--wasmtime") ?? onPath("wasmtime");
  if (given) return given;
  const rel = await latest("bytecodealliance/wasmtime");
  const name = (WASMTIME_ASSET[process.platform] ?? "").replace("%TAG%", rel.tag);
  const dir = join(root, "target/tools", rel.tag);
  const bin = join(dir, name.replace(/\.(tar\.xz|zip)$/, ""), process.platform === "win32" ? "wasmtime.exe" : "wasmtime");
  if (existsSync(bin)) return bin;
  const url = rel.assets.get(name);
  if (!url) throw new Error(`no wasmtime release asset ${name} in ${rel.tag}`);
  const archive = join(dir, name);
  await download(url, archive);
  must(["tar", "-xf", archive, "-C", dir]);
  return bin;
}

function componentize(world: string, source: string, out: string): void {
  const embedded = `${out}.embed`;
  must(["wasm-tools", "component", "embed", WIT, "--world", world, source, "-o", embedded]);
  must(["wasm-tools", "component", "new", embedded, "-o", out]);
}

/** Build and compose; returns the path of the composed command. */
async function compose(outDir: string): Promise<string> {
  mkdirSync(outDir, { recursive: true });
  const at = (f: string) => join(outDir, f);
  componentize("corelib", buildWasm("component"), at("corelib.wasm"));
  componentize("console", "corelib/wat/console.wat", at("console.wasm"));
  componentize("driver", "corelib/wat/driver.wat", at("driver.wasm"));
  writeFileSync(at("app.wat"), appWat(readFileSync(join(root, "corelib/wat/hello.wat"), "utf8")));
  componentize("app", at("app.wat"), at("app.wasm"));
  writeFileSync(at("ay-os.wac"), wacScript());
  const dep = (n: string) => ["--dep", `ay:${n}=${at(`${n}.wasm`)}`];
  must([await ensureWac(), "compose", at("ay-os.wac"), ...["console", "corelib", "app", "driver"].flatMap(dep), "-o", at("ay-os.wasm")]);
  must(["wasm-tools", "validate", at("ay-os.wasm")]);
  return at("ay-os.wasm");
}

async function runLocal(log: string): Promise<number> {
  const outDir = join(root, "target/corelib-component");
  const composed = await compose(outDir);
  console.log(`component: ${composed} (${readFileSync(composed).length} bytes)`);
  if (process.argv.includes("--no-run")) return 0;
  const wasmtime = await ensureWasmtime();
  console.log(`wasmtime: ${must([wasmtime, "--version"]).trim()}`);
  const run = sh([wasmtime, "run", composed]);
  mkdirSync(dirname(log), { recursive: true });
  writeFileSync(log, run.out);
  const bad = run.status === 0 ? verdict(run.out, readFileSync(join(root, GOLDEN), "utf8")) : [`wasmtime exit status ${run.status}`];
  for (const b of bad) console.error(`wasmtime: ${b}`);
  if (bad.length === 0) console.log(`wasmtime: boot transcript identical (${run.out.split("\n").length - 1} lines), log ${log}`);
  return bad.length === 0 ? 0 : 1;
}

function runRemote(log: string): number {
  const ay = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as { hosts: { build: string } };
  const host = ay.hosts.build;
  const dir = arg("--dir") ?? "/srv/xbuild/asm-guide/corelib-component";
  const files = ["ay.toml", "corelib", "tools/corelib.ts", "tools/corelib-component.ts"];
  const archive = Bun.spawnSync(["tar", "czf", "-", ...files], { cwd: root, stdout: "pipe" });
  if (archive.exitCode !== 0) return 1;
  const remoteLog = `${dir}/logs/wasmtime-boot.log`;
  const remote = `mkdir -p ${dir} && cd ${dir} && tar xzf - && bun tools/corelib-component.ts --log ${remoteLog}`;
  const run = sh(["ssh", host, remote], { stdin: new Uint8Array(archive.stdout) });
  console.log(run.out.trim());
  const fetched = sh(["ssh", host, `cat ${remoteLog}`]);
  mkdirSync(dirname(log), { recursive: true });
  writeFileSync(log, fetched.out);
  console.log(`wasmtime: ${host}:${remoteLog} copied to ${log}`);
  return run.status;
}

if (import.meta.main) {
  const log = resolve(arg("--log") ?? join(root, "target/corelib-component.log"));
  process.exit(process.argv.includes("--remote") ? runRemote(log) : await runLocal(log));
}
