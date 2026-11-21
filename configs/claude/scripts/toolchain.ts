#!/usr/bin/env bun
/**
 * Applies toolchain.toml: pinned CLIs, fixed-output downloads, one install directory.
 *
 *   bun scripts/toolchain.ts check [--group g] [name...]     # exit 1 when a tool is missing, not at its pinned version, or drifts from its lock
 *   bun scripts/toolchain.ts install [--group g] [name...]   # download, verify sha256, install the tools that fail `check`
 *
 * Without names, every tool that has an entry for this platform. Install never touches a tool that already passes.
 */

import { createHash } from "node:crypto";
import {
  chmodSync,
  copyFileSync,
  createReadStream,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

export const ARCHIVES = ["tar.gz", "tar.xz", "zip"] as const;

export interface Source {
  url: string;
  sha256: string;
  archive?: (typeof ARCHIVES)[number];
  /** The binary inside the archive; with `dir` the folder to keep; with `cargo` the crate's source folder. */
  member?: string;
  /** Keep the `member` folder as ~/.local/opt/<dir> (SDK trees) instead of one binary in ~/.local/bin. */
  dir?: string;
  /** Build the extracted crate with `cargo install --locked` (crates that ship no release binary). */
  cargo?: boolean;
  /** Commands run inside the installed `dir` (argv lists), for example an SDK's own download step. */
  post?: string[][];
}

/** A value pinned elsewhere that must equal the tool's version: the first capture group of `pattern` in `file`. */
export interface Lock {
  /** Relative to the workspace root (the folder holding this repository and its siblings). */
  file: string;
  pattern: string;
}

export interface Tool {
  version: string;
  check: string[];
  role: string;
  /** What `check` must print when it differs from `version` (a build number, an install path). */
  expect?: string;
  group?: string;
  lock?: Lock;
  [platform: string]: Source | Lock | string | string[] | undefined;
}

export type Manifest = Record<string, Tool>;

/** This host as a manifest platform key, for example "windows-x86_64". */
export function platformKey(
  platform: string = process.platform,
  arch: string = process.arch,
): string {
  const os =
    platform === "win32"
      ? "windows"
      : platform === "darwin"
        ? "macos"
        : platform;
  const cpu = arch === "x64" ? "x86_64" : arch === "arm64" ? "aarch64" : arch;
  return `${os}-${cpu}`;
}

export function substitute(text: string, version: string): string {
  return text.replaceAll("{version}", version);
}

const optDir = join(homedir(), ".local", "opt");

/** `check` and `post` arguments: `{version}`, `{opt}` (the SDK tree folder) and `{home}`. */
export function expand(text: string, version: string): string {
  return substitute(text, version)
    .replaceAll("{opt}", optDir)
    .replaceAll("{home}", homedir());
}

/** The source of `tool` for `platform`, with `{version}` resolved, or undefined when the tool has none there. */
export function sourceFor(tool: Tool, platform: string): Source | undefined {
  const raw = tool[platform];
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return undefined;
  const src = raw as Source;
  return {
    ...src,
    url: substitute(src.url, tool.version),
    member: src.member && substitute(src.member, tool.version),
    dir: src.dir && substitute(src.dir, tool.version),
  };
}

/** Every problem of a manifest, as messages. Empty when valid. */
export function problems(manifest: Manifest): string[] {
  const out: string[] = [];
  for (const [name, tool] of Object.entries(manifest)) {
    if (!/^\d+(\.\d+)*$/.test(tool.version ?? ""))
      out.push(`${name}: version "${tool.version}" is not an exact version`);
    if (!Array.isArray(tool.check) || tool.check.length === 0)
      out.push(`${name}: no check command`);
    if (!tool.role?.trim()) out.push(`${name}: no role`);
    if (tool.lock && (!tool.lock.file || !tool.lock.pattern))
      out.push(`${name}: a lock names its file and pattern`);
    for (const [key, value] of Object.entries(tool)) {
      if (key === "lock" || typeof value !== "object" || Array.isArray(value))
        continue;
      const src = value as Source;
      if (!/^https:\/\//.test(src.url))
        out.push(`${name}.${key}: url is not https`);
      if (!/^[0-9a-f]{64}$/.test(src.sha256 ?? ""))
        out.push(`${name}.${key}: sha256 is not 64 hex digits`);
      if (src.archive && !src.member)
        out.push(`${name}.${key}: an archive names its member`);
      if (src.archive && !ARCHIVES.includes(src.archive))
        out.push(`${name}.${key}: unknown archive ${src.archive}`);
      if ((src.dir || src.cargo) && (!src.archive || !src.member))
        out.push(`${name}.${key}: a dir or cargo source names its archive and member`);
      if (src.dir && src.cargo)
        out.push(`${name}.${key}: dir and cargo are exclusive`);
      if (src.post && !src.dir)
        out.push(`${name}.${key}: post runs inside a dir install`);
    }
  }
  return out;
}

export const bin = (dir: string, name: string, platform: string) =>
  join(dir, platform.startsWith("windows") ? `${name}.exe` : name);

// ---------------------------------------------------------------------------------------------------------------------

const root = resolve(import.meta.dirname, "..");
const installDir = join(homedir(), ".local", "bin");

export async function load(
  file = join(root, "toolchain.toml"),
): Promise<Manifest> {
  const manifest = Bun.TOML.parse(
    await Bun.file(file).text(),
  ) as unknown as Manifest;
  const bad = problems(manifest);
  if (bad.length) throw new Error(`toolchain.toml:\n  ${bad.join("\n  ")}`);
  return manifest;
}

const workspaceRoot = resolve(root, "..", "..", "..");

/** The value pinned by the tool's lock (a sibling repository's lockfile or workflow), or undefined when that file is absent here. */
export function lockedVersion(tool: Tool, base = workspaceRoot): string | undefined {
  if (!tool.lock) return undefined;
  const file = resolve(base, tool.lock.file);
  if (!existsSync(file)) return undefined;
  return new RegExp(tool.lock.pattern).exec(readFileSync(file, "utf8"))?.[1];
}

/** Whether the tool on PATH prints its pinned version, with its first line of output. */
async function passes(tool: Tool): Promise<{ ok: boolean; seen: string }> {
  try {
    const argv = tool.check.map((arg) => expand(arg, tool.version));
    const p = Bun.spawnSync(argv, { stdout: "pipe", stderr: "pipe" });
    const text = `${p.stdout.toString()}${p.stderr.toString()}`.trim();
    const needle = expand(tool.expect ?? tool.version, tool.version);
    return {
      ok: p.exitCode === 0 && text.includes(needle),
      seen: text.split("\n")[0] ?? "",
    };
  } catch {
    return { ok: false, seen: "not found" };
  }
}

const downloads = new Map<string, string>();
let downloadDir: string | undefined;

async function sha256Of(file: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}

/**
 * The asset at `src.url` as a file, checked against its pinned digest; archives shared by several tools download once.
 * curl streams to disk with retries: large SDK archives do not fit comfortably in memory and Bun's fetch stalls on them.
 */
async function download(name: string, src: Source): Promise<string> {
  const known = downloads.get(src.url);
  if (known) return known;
  if (!downloadDir) {
    downloadDir = mkdtempSync(join(tmpdir(), "toolchain-"));
    process.once("exit", cleanDownloads);
  }
  const file = join(downloadDir, `${downloads.size}.download`);
  const p = Bun.spawnSync(
    ["curl", "-fsSL", "--retry", "3", "--connect-timeout", "20", "-o", file, src.url],
    { stdout: "inherit", stderr: "pipe" },
  );
  if (p.exitCode !== 0)
    throw new Error(`${name}: ${src.url} failed: ${p.stderr.toString().trim()}`);
  const digest = await sha256Of(file);
  if (digest !== src.sha256)
    throw new Error(
      `${name}: sha256 ${digest} does not match the pinned ${src.sha256}`,
    );
  downloads.set(src.url, file);
  return file;
}

function run(name: string, argv: string[], cwd?: string): void {
  const p = Bun.spawnSync(argv, { cwd, stdout: "inherit", stderr: "pipe" });
  if (p.exitCode !== 0)
    throw new Error(`${name}: ${argv[0]} failed: ${p.stderr.toString().trim()}`);
}

function unpack(name: string, archive: string, into: string, platform: string): void {
  const zipOnUnix = archive.endsWith(".zip") && !platform.startsWith("windows");
  run(name, zipOnUnix ? ["unzip", "-q", archive, "-d", into] : ["tar", "-xf", archive, "-C", into]);
}

async function install(
  name: string,
  tool: Tool,
  platform: string,
): Promise<void> {
  const src = sourceFor(tool, platform);
  if (!src) throw new Error(`${name}: no source for ${platform}`);
  const file = await download(name, src);

  const parent = src.dir ? optDir : installDir;
  mkdirSync(parent, { recursive: true });
  // Staged next to the target: a rename across file systems fails, and a rename on one replaces the target atomically.
  const work = mkdtempSync(join(parent, `.toolchain-${name}-`));
  try {
    let built = join(work, "download");
    if (src.archive) {
      unpack(name, file, work, platform);
      built = join(work, src.member!);
    } else {
      copyFileSync(file, built);
    }
    if (src.cargo) {
      const root = join(work, "root");
      run(name, ["cargo", "install", "--locked", "--path", built, "--root", root, "--target-dir", join(work, "target")]);
      built = bin(join(root, "bin"), name, platform);
    }
    if (src.dir) {
      const dest = join(optDir, src.dir);
      rmSync(dest, { recursive: true, force: true });
      renameSync(built, dest);
      for (const argv of src.post ?? [])
        run(name, argv.map((arg) => expand(arg, tool.version)), dest);
      console.log(`${name} ${tool.version} -> ${dest}`);
      return;
    }
    const target = bin(installDir, name, platform);
    chmodSync(built, 0o755);
    renameSync(built, target);
    console.log(`${name} ${tool.version} -> ${target}`);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

function cleanDownloads(): void {
  if (downloadDir) rmSync(downloadDir, { recursive: true, force: true });
}

if (import.meta.main) {
  const argv = process.argv.slice(2);
  const flag = argv.indexOf("--group");
  const group = flag >= 0 ? argv.splice(flag, 2)[1] : undefined;
  const [step = "check", ...names] = argv;
  if (step !== "check" && step !== "install") {
    console.error(
      "usage: bun scripts/toolchain.ts <check|install> [--group g] [name...]",
    );
    process.exit(2);
  }
  const manifest = await load();
  const platform = platformKey();
  const unknown = names.filter((n) => !manifest[n]);
  if (unknown.length)
    throw new Error(`not in toolchain.toml: ${unknown.join(", ")}`);
  const selected = (names.length ? names : Object.keys(manifest))
    .filter((n) => !group || manifest[n]!.group === group)
    .filter((n) => sourceFor(manifest[n]!, platform));
  let failed = 0;
  for (const name of selected) {
    const tool = manifest[name]!;
    const locked = lockedVersion(tool);
    if (locked !== undefined && locked !== tool.version) {
      console.log(
        `drift    ${name} pinned ${tool.version}, ${tool.lock!.file} says ${locked}`,
      );
      failed++;
      continue;
    }
    const before = await passes(tool);
    if (before.ok) {
      console.log(`ok       ${name} ${tool.version}`);
      continue;
    }
    if (step === "check") {
      console.log(
        `missing  ${name} ${tool.version} (found: ${before.seen || "nothing"})`,
      );
      failed++;
      continue;
    }
    await install(name, tool, platform);
    const after = await passes(tool);
    if (!after.ok) {
      console.log(
        `shadowed ${name}: installed in ${installDir}, but PATH resolves to a build printing "${after.seen}"`,
      );
      failed++;
    }
  }
  process.exit(failed ? 1 : 0);
}
