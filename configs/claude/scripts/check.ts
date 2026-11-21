#!/usr/bin/env bun
/**
 * Gate of the Claude config and the aphrody plugin of this repository.
 *
 *   bun configs/claude/scripts/check.ts
 *
 * Steps: retired surfaces (configs/claude/retired.toml), the unit tests of the config scripts, settings hygiene and stale-path
 * scan, the claude-bridge MCP server over stdio, and `claude plugin validate` when the claude CLI is installed.
 * Exit 1 when any step fails.
 */
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { FORBIDDEN, type Settings, undeclaredPlugins } from "./hygiene.ts";
import { check as findRetired } from "./retired.ts";

const CONFIG_ROOT = join(import.meta.dir, "..");
const FW_ROOT = join(CONFIG_ROOT, "..", "..");
let failed = 0;

function pass(label: string): void {
  console.log(`PASS [${label}]`);
}
function fail(label: string, detail: string): void {
  failed++;
  console.error(`FAIL [${label}]:\n${detail}`);
}

function step(label: string, cmd: string[], cwd = FW_ROOT, stdin?: string): string | undefined {
  const run = Bun.spawnSync(cmd, {
    cwd,
    stdout: "pipe",
    stderr: "pipe",
    stdin: stdin === undefined ? "ignore" : new TextEncoder().encode(stdin),
  });
  if (run.exitCode === 0) {
    pass(label);
    return run.stdout.toString();
  }
  fail(label, `${run.stdout.toString().trim()}\n${run.stderr.toString().trim()}`);
  return undefined;
}

// 1. Retired surfaces: the relay hub, fr2 and agy tooling stay out of every tracked file but the history notes.
const retired = findRetired(FW_ROOT, join(CONFIG_ROOT, "retired.toml"));
if (retired.length === 0) pass("retired surfaces");
else
  fail(
    "retired surfaces",
    `${retired.slice(0, 40).map((hit) => `${hit.file}:${hit.line}: [${hit.term}] ${hit.text}`).join("\n")}\n${retired.length} line(s) in total`,
  );

// 2. Unit tests of the config scripts.
step("config script tests", ["bun", "test", "./configs/claude/scripts"]);

// 3. Settings hygiene and stale literals.
const settings = JSON.parse(readFileSync(join(CONFIG_ROOT, "settings.json"), "utf8")) as Settings;
if (settings.permissions && "deny" in settings.permissions) fail("hygiene", "settings.json must not declare permissions.deny");
const undeclared = undeclaredPlugins(settings);
if (undeclared.length > 0) fail("hygiene", `enabledPlugins from an undeclared marketplace: ${undeclared.join(", ")}`);

function textFiles(path: string, out: string[]): string[] {
  if (!existsSync(path)) return out;
  if (statSync(path).isFile()) {
    if (/\.(ts|json|md|toml)$/i.test(path)) out.push(path);
    return out;
  }
  for (const name of readdirSync(path)) if (name !== "node_modules" && name !== ".git") textFiles(join(path, name), out);
  return out;
}
const selfRules = new Set(["scripts/check.ts", "scripts/hygiene.ts", "scripts/hygiene.test.ts", "scripts/merge-brain.ts"]);
let staleHits = 0;
for (const file of textFiles(CONFIG_ROOT, [])) {
  const rel = relative(CONFIG_ROOT, file).replaceAll("\\", "/");
  if (selfRules.has(rel) || rel.startsWith("docs/")) continue;
  const text = readFileSync(file, "utf8");
  for (const rule of FORBIDDEN) {
    if (rule.re.test(text)) {
      staleHits++;
      fail(`hygiene ${rule.label}`, rel);
    }
  }
}
if (staleHits === 0) pass("hygiene scan");

// 4. The claude-bridge MCP server answers initialize and tools/list over stdio.
const frames = [
  { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "check", version: "1" } } },
  { jsonrpc: "2.0", method: "notifications/initialized" },
  { jsonrpc: "2.0", id: 2, method: "tools/list" },
]
  .map((frame) => JSON.stringify(frame))
  .join("\n");
const bridge = step("claude-bridge MCP stdio", ["bun", "--no-install", "--no-env-file", join(FW_ROOT, "plugins", "aphrody", "mcp", "claude-bridge.ts")], FW_ROOT, `${frames}\n`);
if (bridge !== undefined) {
  const replies = bridge.trim().split("\n").map((line) => JSON.parse(line) as { id: number; result?: { tools?: Array<{ name: string }> } });
  const names = replies.find((reply) => reply.id === 2)?.result?.tools?.map((tool) => tool.name) ?? [];
  if (!names.includes("claude_inject") || !names.includes("claude_sessions")) fail("claude-bridge tools", `listed: ${names.join(", ")}`);
}

// 5. Plugin and marketplace validation by the claude CLI itself.
if (Bun.which("claude")) {
  step("claude validate marketplace", ["claude", "plugin", "validate", FW_ROOT]);
  step("claude validate aphrody", ["claude", "plugin", "validate", join(FW_ROOT, "plugins", "aphrody")]);
} else console.log("SKIP [claude plugin validate]: claude CLI not installed");

process.exit(failed > 0 ? 1 : 0);
