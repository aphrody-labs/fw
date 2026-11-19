#!/usr/bin/env bun
// Host facts from ay.toml: roles in [hosts] (ssh Host aliases) and per-role layouts in [paths.<role>].
//   bun tools/hosts.ts host build      the ssh alias of a role
//   bun tools/hosts.ts work build      the work volume of a role (builds, caches, mirrors)
import { readAy } from "./local.ts";
import { readFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";

export type Role = "build" | "test" | "prod";
/** `android_sdk` and `gradle_home`: optional per-role locations of the Android SDK and the Gradle user home (tools/compose-check.ts). */
export type HostPaths = { work?: string; android_sdk?: string; gradle_home?: string };
export type HostConfig = { hosts: Record<string, string>; paths: Record<string, HostPaths> };

const root = resolve(import.meta.dir, "..");

export function loadHosts(tomlPath = join(root, "ay.toml")): HostConfig {
  const t = (basename(tomlPath) === "ay.toml" ? readAy(dirname(tomlPath)) : Bun.TOML.parse(readFileSync(tomlPath, "utf8"))) as Partial<HostConfig>;
  return { hosts: t.hosts ?? {}, paths: t.paths ?? {} };
}

export function hostAlias(role: Role, cfg = loadHosts()): string {
  const h = cfg.hosts[role];
  if (!h) throw new Error(`ay.toml [hosts].${role} is not set`);
  return h;
}

/** The work volume of a role, a path on that host: `[paths.<role>].work` of ay.toml. */
export function hostWork(role: Role, cfg = loadHosts()): string {
  const w = cfg.paths[role]?.work;
  if (!w) throw new Error(`ay.toml [paths.${role}].work is not set`);
  return w.replace(/\/+$/, "");
}

if (import.meta.main) {
  const [what, role] = process.argv.slice(2) as [string, Role];
  if ((what !== "host" && what !== "work") || !role) {
    console.error("usage: bun tools/hosts.ts host|work <build|test|prod>");
    process.exit(2);
  }
  console.log(what === "host" ? hostAlias(role) : hostWork(role));
}
