#!/usr/bin/env bun
// The private-data gate: tracked files must not carry data about this machine, the fleet or the owner. Patterns come from two
// places: the git-ignored ay.local.toml (the real host aliases and host work paths of this machine, so the tracked
// tree can never repeat them) and generic ones (a non-loopback IPv4 address, a Windows user profile path, a home directory
// path, an e-mail address). Tests, lock files, vendored trees and SVG are not scanned.
//   bun tools/private-gate.ts        exit 1 and list file:line when a tracked file leaks
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { readAy, repoRoot } from "./local.ts";

export type Leak = { file: string; line: number; why: string };

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const SKIP = /(^|\/)(bun\.lock|Cargo\.lock|third_party|node_modules|target)(\/|$)|\/(test|fixtures)\/|\.(test\.ts|svg|png|jpg|woff2?|wasm|lock)$/;
const IPV4 = /\b(?!127\.|0\.0\.0\.0|1\.1\.1\.1|1\.0\.0\.1|8\.8\.8\.8|8\.8\.4\.4|255\.|10\.0\.0\.|192\.0\.2\.|198\.51\.100\.|203\.0\.113\.)(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b/;

export function patterns(root = repoRoot): [string, RegExp][] {
  const out: [string, RegExp][] = [
    ["a Windows user profile path", /[A-Za-z]:\\Users\\[A-Za-z0-9._-]+/],
    ["a home directory path", /(^|[\s"'`(=])\/home\/[a-z][a-z0-9_-]{1,}\//],
    ["an e-mail address", /\b[\w.+-]+@(?!users\.noreply\.github\.com|github\.com:)[\w-]+\.(com|net|org|fr|io|dev)\b/],
  ];
  const cfg = (existsSync(join(root, "ay.toml")) ? readAy(root) : {}) as { hosts?: Record<string, string>; paths?: Record<string, { work?: string }>; recovery?: { scan?: string[] } };
  const secrets = new Set<string>([...Object.values(cfg.hosts ?? {}), ...Object.values(cfg.paths ?? {}).map((p) => p.work ?? "")].filter((s) => s.length >= 3 && !s.startsWith("<")));
  for (const s of secrets) out.push([`a value of ay.local.toml (${s.startsWith("/") ? "a host path" : "a host alias"})`, new RegExp(s.startsWith("/") ? `${escape(s)}(?![\\w-])` : `(?<![\\w-])${escape(s)}(?![\\w-])`)]);
  for (const s of cloudflareLiterals(root)) out.push(["a value of cf.local.toml (a Cloudflare id or a host address)", new RegExp(`(?<![\\w.-])${escape(s)}(?![\\w.-])`, "i")]);
  return out;
}

/** Account, zone and tunnel ids and the public addresses of the git-ignored cf.local.toml (packages/cf-control-plane): never tracked. */
export function cloudflareLiterals(root = repoRoot): string[] {
  const file = join(root, "cf.local.toml");
  if (!existsSync(file)) return [];
  try {
    const parsed = Bun.TOML.parse(readFileSync(file, "utf8")) as Record<string, unknown>;
    const vals: string[] = [];
    const walk = (v: unknown) => {
      if (typeof v === "string") vals.push(v);
      else if (Array.isArray(v)) v.forEach(walk);
      else if (v && typeof v === "object") Object.values(v).forEach(walk);
    };
    walk(parsed.cloudflare);
    for (const h of Object.values((parsed.hosts ?? {}) as Record<string, { public_ipv4?: string; public_ipv6?: string }>)) walk([h?.public_ipv4, h?.public_ipv6]);
    return vals.filter((v) => v.length >= 6 && !v.startsWith("<"));
  } catch {
    return [];
  }
}

export function scan(files: Map<string, string>, pats: [string, RegExp][]): Leak[] {
  const leaks: Leak[] = [];
  for (const [file, text] of files) {
    if (SKIP.test(file)) continue;
    text.split("\n").forEach((line, i) => {
      for (const [why, re] of pats) if (re.test(line)) leaks.push({ file, line: i + 1, why });
      const ip = IPV4.exec(line);
      if (ip && !/version|v\d|\d+\.\d+\.\d+\.\d+[-.]\d|\w+\/\d+\.\d+\.\d+\.\d+|Core \d+\.\d+\.\d+\.\d+|\d+\.\d+\.\d+\.\d+-[a-z]/i.test(line) && ip.slice(1).every((n) => Number(n) <= 255)) leaks.push({ file, line: i + 1, why: "an IPv4 address" });
    });
  }
  return leaks;
}

export async function trackedFiles(root = repoRoot): Promise<Map<string, string>> {
  const proc = Bun.spawn(["git", "-C", root, "ls-files", "-z"], { stdout: "pipe", stderr: "ignore" });
  const names = (await new Response(proc.stdout).text()).split("\0").filter(Boolean);
  const files = new Map<string, string>();
  for (const n of names) {
    const path = join(root, n);
    if (SKIP.test(n) || !existsSync(path)) continue;
    const text = await Bun.file(path).text().catch(() => "");
    if (!text.includes("\0")) files.set(n, text);
  }
  return files;
}

if (import.meta.main) {
  const leaks = scan(await trackedFiles(), patterns());
  for (const l of leaks.slice(0, 60)) console.error(`${l.file}:${l.line}: ${l.why}`);
  console.log(leaks.length ? `${leaks.length} leak(s) of machine, fleet or owner data in tracked files` : "no machine, fleet or owner data in tracked files");
  process.exit(leaks.length ? 1 : 0);
}
