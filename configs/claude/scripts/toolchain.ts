#!/usr/bin/env bun
/**
 * Applies toolchain.toml: pinned CLIs, fixed-output downloads, one install directory.
 *
 *   bun scripts/toolchain.ts check [name...]     # exit 1 when a tool is missing or not at its pinned version
 *   bun scripts/toolchain.ts install [name...]   # download, verify sha256, install the tools that fail `check`
 *
 * Without names, every tool that has an entry for this platform. Install never touches a tool that already passes.
 */

import { chmodSync, mkdirSync, mkdtempSync, renameSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

export interface Source {
  url: string;
  sha256: string;
  archive?: "tar.gz" | "zip";
  member?: string;
}

export interface Tool {
  version: string;
  check: string[];
  role: string;
  [platform: string]: Source | string | string[];
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

/** The source of `tool` for `platform`, with `{version}` resolved, or undefined when the tool has none there. */
export function sourceFor(tool: Tool, platform: string): Source | undefined {
  const raw = tool[platform];
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return undefined;
  return {
    ...raw,
    url: substitute(raw.url, tool.version),
    member: raw.member && substitute(raw.member, tool.version),
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
    for (const [key, value] of Object.entries(tool)) {
      if (typeof value !== "object" || Array.isArray(value)) continue;
      const src = value as Source;
      if (!/^https:\/\//.test(src.url))
        out.push(`${name}.${key}: url is not https`);
      if (!/^[0-9a-f]{64}$/.test(src.sha256 ?? ""))
        out.push(`${name}.${key}: sha256 is not 64 hex digits`);
      if (src.archive && !src.member)
        out.push(`${name}.${key}: an archive names its member`);
      if (src.archive && !["tar.gz", "zip"].includes(src.archive))
        out.push(`${name}.${key}: unknown archive ${src.archive}`);
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

/** Whether the tool on PATH prints its pinned version, with what it printed. */
async function passes(tool: Tool): Promise<{ ok: boolean; seen: string }> {
  try {
    const p = Bun.spawnSync(tool.check, { stdout: "pipe", stderr: "pipe" });
    const seen =
      `${p.stdout.toString()}${p.stderr.toString()}`.trim().split("\n")[0] ??
      "";
    return { ok: p.exitCode === 0 && seen.includes(tool.version), seen };
  } catch {
    return { ok: false, seen: "not found" };
  }
}

async function install(
  name: string,
  tool: Tool,
  platform: string,
): Promise<void> {
  const src = sourceFor(tool, platform);
  if (!src) throw new Error(`${name}: no source for ${platform}`);
  const res = await fetch(src.url);
  if (!res.ok) throw new Error(`${name}: ${src.url} answered ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  const digest = new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
  if (digest !== src.sha256)
    throw new Error(
      `${name}: sha256 ${digest} does not match the pinned ${src.sha256}`,
    );

  mkdirSync(installDir, { recursive: true });
  const target = bin(installDir, name, platform);
  // Staged next to the target: a rename across file systems fails, and a rename on one replaces the target atomically.
  const work = mkdtempSync(join(installDir, `.toolchain-${name}-`));
  try {
    let built = join(work, "download");
    await Bun.write(built, bytes);
    if (src.archive) {
      const unpack =
        src.archive === "zip" && !platform.startsWith("windows")
          ? ["unzip", "-q", built, "-d", work]
          : ["tar", "-xf", built, "-C", work];
      const p = Bun.spawnSync(unpack, { stdout: "ignore", stderr: "pipe" });
      if (p.exitCode !== 0)
        throw new Error(
          `${name}: ${unpack[0]} failed: ${p.stderr.toString().trim()}`,
        );
      built = join(work, src.member!);
    }
    chmodSync(built, 0o755);
    renameSync(built, target);
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`${name} ${tool.version} -> ${target}`);
}

if (import.meta.main) {
  const [step = "check", ...names] = process.argv.slice(2);
  if (step !== "check" && step !== "install") {
    console.error("usage: bun scripts/toolchain.ts <check|install> [name...]");
    process.exit(2);
  }
  const manifest = await load();
  const platform = platformKey();
  const unknown = names.filter((n) => !manifest[n]);
  if (unknown.length)
    throw new Error(`not in toolchain.toml: ${unknown.join(", ")}`);
  const selected = (names.length ? names : Object.keys(manifest)).filter((n) =>
    sourceFor(manifest[n]!, platform),
  );
  let failed = 0;
  for (const name of selected) {
    const tool = manifest[name]!;
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
