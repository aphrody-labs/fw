#!/usr/bin/env bun
/**
 * Copies the Claude config sources of this repository into the live config directory (~/.claude or CLAUDE_CONFIG_DIR),
 * as listed in configs/claude/deploy.toml. Writes only files whose content differs.
 *
 *   bun configs/claude/scripts/deploy.ts [--check] [--home dir]
 *     --check  write nothing, exit 1 when the live copy differs from the sources
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { claudeHome } from "./lib/claude-peer.ts";

export interface CopyRule {
  src: string;
  dest: string;
  exclude?: string[];
}

export function parseRules(text: string): CopyRule[] {
  return ((Bun.TOML.parse(text) as { copy?: CopyRule[] }).copy ?? []).map((rule) => ({ ...rule }));
}

function listFiles(root: string): string[] {
  if (statSync(root).isFile()) return [""];
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else out.push(relative(root, full).replaceAll("\\", "/"));
    }
  };
  walk(root);
  return out.sort();
}

/** Files of `rule` whose destination is missing or different; with `write`, they are written. */
export function deploy(sourceRoot: string, home: string, rules: CopyRule[], write: boolean): string[] {
  const changed: string[] = [];
  for (const rule of rules) {
    const source = join(sourceRoot, rule.src);
    if (!existsSync(source)) throw new Error(`deploy: source missing: ${rule.src}`);
    const excluded = (rule.exclude ?? []).map((pattern) => new Bun.Glob(pattern));
    for (const file of listFiles(source)) {
      if (excluded.some((glob) => glob.match(file))) continue;
      const from = file === "" ? source : join(source, file);
      const to = file === "" ? join(home, rule.dest) : join(home, rule.dest, file);
      const bytes = readFileSync(from);
      if (existsSync(to) && readFileSync(to).equals(bytes)) continue;
      changed.push(relative(home, to).replaceAll("\\", "/"));
      if (write) {
        mkdirSync(dirname(to), { recursive: true });
        writeFileSync(to, bytes);
      }
    }
  }
  return changed;
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const homeFlag = args.indexOf("--home");
  const home = homeFlag >= 0 ? (args[homeFlag + 1] ?? claudeHome()) : claudeHome();
  const sourceRoot = join(import.meta.dir, "..");
  const rules = parseRules(readFileSync(join(sourceRoot, "deploy.toml"), "utf8"));
  const check = args.includes("--check");
  const changed = deploy(sourceRoot, home, rules, !check);
  console.log(JSON.stringify({ home, check, changed }, null, 2));
  process.exit(check && changed.length > 0 ? 1 : 0);
}
