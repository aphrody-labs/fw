#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";

function resolveFwRoot(): string {
  const candidates = [
    process.env.FW_ROOT,
    resolve(dirname(import.meta.path), "..", "..", ".."),
    process.env.WORKSPACE_ROOT ? join(process.env.WORKSPACE_ROOT, "fw") : undefined,
    join(homedir(), "src", "fw"),
    process.cwd(),
  ].filter((v): v is string => Boolean(v));
  for (const dir of candidates) {
    if (existsSync(join(dir, "fw.targets.json")) || existsSync(join(dir, "src", "fw_cli.ts"))) {
      return resolve(dir);
    }
  }
  return resolve(process.cwd());
}

const root = resolveFwRoot();
const exe = process.platform === "win32" ? "fw.exe" : "fw";

// The Rust binary is the MCP server (2026-07-28 and earlier versions, see crates/fw-proto/src/mcp.rs); the Bun
// implementation in src/mcp_server.ts serves only when no fw binary can be found.
const probe = JSON.stringify({
  jsonrpc: "2.0",
  id: 1,
  method: "server/discover",
  params: { _meta: { "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} } },
});

// An fw built before `fw mcp` became a server prints `{}` and exits; only a binary that answers server/discover serves.
function servesMcp(binary: string): boolean {
  const run = Bun.spawnSync([binary, "mcp"], { stdin: new TextEncoder().encode(`${probe}\n`), stdout: "pipe", stderr: "ignore", timeout: 10_000 });
  return run.stdout.toString().includes("supportedVersions");
}

const fwBinary = [process.env.FW_BIN, Bun.which("fw"), join(root, "target", "release", exe), join(root, "target", "debug", exe)].find(
  (candidate): candidate is string => Boolean(candidate) && existsSync(candidate as string) && servesMcp(candidate as string),
);
const cmd = fwBinary ? [fwBinary, "mcp"] : [process.execPath, join(root, "src", "fw_cli.ts"), "mcp"];

const proc = Bun.spawn(cmd, {
  cwd: root,
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
  env: { ...process.env, FW_ROOT: root },
});

process.exit(await proc.exited);
