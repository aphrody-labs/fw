// SPDX-License-Identifier: Apache-2.0
/**
 * Host roles and their ssh aliases. Code and tracked data name a host by role (`build`, `test`, `prod`); the alias a role
 * stands for is machine configuration and never enters the repository: `AY_HOST_<ROLE>` in the environment, else `[hosts]`
 * of `ay.local.toml` over `ay.toml` in `AY_ROOT` (default: the repository above this package), else the role itself, which
 * is an ssh `Host` entry on a machine that names its hosts by role.
 */
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

export type HostRole = "build" | "test" | "prod";
export const HOST_ROLES: readonly HostRole[] = ["build", "test", "prod"];

const root = (env: Record<string, string | undefined>) => resolve(env.AY_ROOT ?? join(import.meta.dir, "..", "..", ".."));

function hostsOf(file: string): Record<string, string> {
  if (!existsSync(file)) return {};
  try {
    return ((Bun.TOML.parse(readFileSync(file, "utf8")) as { hosts?: Record<string, string> }).hosts ?? {}) as Record<string, string>;
  } catch {
    return {};
  }
}

/** The ssh alias of each role: environment over `ay.local.toml` over `ay.toml`, the role itself when nothing names it. */
export function hostAliases(env: Record<string, string | undefined> = process.env): Record<HostRole, string> {
  const dir = root(env);
  const cfg = { ...hostsOf(join(dir, "ay.toml")), ...hostsOf(join(dir, "ay.local.toml")) };
  const out = {} as Record<HostRole, string>;
  for (const r of HOST_ROLES) out[r] = env[`AY_HOST_${r.toUpperCase()}`] || (typeof cfg[r] === "string" && !cfg[r].startsWith("<") && cfg[r]) || r;
  return out;
}

/** The ssh alias of a role; a name that is not a role (`pc`, a literal alias) is returned unchanged. */
export function hostAlias(name: string, env: Record<string, string | undefined> = process.env): string {
  return HOST_ROLES.includes(name as HostRole) ? hostAliases(env)[name as HostRole] : name;
}

/** The role a role name or an alias stands for; undefined when it is neither. */
export function hostRole(name: string, env: Record<string, string | undefined> = process.env): HostRole | undefined {
  if (HOST_ROLES.includes(name as HostRole)) return name as HostRole;
  return HOST_ROLES.find((r) => hostAliases(env)[r] === name);
}

// CLI for shells and just recipes:  bun hosts.ts alias <role|alias>   the ssh alias       bun hosts.ts role <role|alias>   the role
if (import.meta.main) {
  const [cmd, name] = Bun.argv.slice(2);
  const out = name && (cmd === "alias" ? hostAlias(name) : cmd === "role" ? hostRole(name) : undefined);
  if (!out) {
    console.error("usage: hosts.ts alias|role <build|test|prod|alias>");
    process.exit(2);
  }
  console.log(out);
}
