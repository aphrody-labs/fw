#!/usr/bin/env bun
// Runs the MCP conformance gate: the official suite (pinned in suite.json) against the fixture server for each protocol
// version, and the tool-by-tool tests (tools.test.ts) against the built `fw`. Exit code 0 only when everything passes.
//
//   bun tools/mcp-conformance/run.ts --fw target/debug/fw --fixture target/debug/examples/mcp_fixture_server \
//       [--out target/mcp-conformance] [--write-report docs/research/mcp-conformance.md]

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { formatTranscript, httpClient, stdioClient } from "./client.ts";

const here = import.meta.dir;
const ROOT = resolve(here, "..", "..");
const arg = (name: string): string | undefined => {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
};
const fw = resolve(arg("fw") ?? join(ROOT, "target", "debug", process.platform === "win32" ? "fw.exe" : "fw"));
const fixture = resolve(arg("fixture") ?? join(ROOT, "target", "debug", "examples", process.platform === "win32" ? "mcp_fixture_server.exe" : "mcp_fixture_server"));
const out = resolve(arg("out") ?? join(ROOT, "target", "mcp-conformance"));
mkdirSync(out, { recursive: true });

const suite = JSON.parse(readFileSync(join(here, "suite.json"), "utf8")) as { package: string; version: string; runs: { specVersion: string; expectedFailures?: string }[] };

async function freePort(): Promise<number> {
  const probe = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
  const port = probe.port;
  probe.stop(true);
  return port;
}

const summaries: { specVersion: string; passed: number; failed: number; scenarios: number; exit: number; log: string }[] = [];
const port = await freePort();
const server = Bun.spawn([fixture, `127.0.0.1:${port}`], { stdout: "ignore", stderr: "inherit" });
try {
  for (let i = 0; i < 100; i++) {
    try {
      await fetch(`http://127.0.0.1:${port}/health`);
      break;
    } catch {
      await Bun.sleep(100);
    }
  }
  for (const run of suite.runs) {
    const args = ["bunx", `${suite.package}@${suite.version}`, "server", "--url", `http://127.0.0.1:${port}/mcp`, "--spec-version", run.specVersion, "--suite", "all"];
    if (run.expectedFailures) args.push("--expected-failures", join(here, run.expectedFailures));
    const proc = Bun.spawn(args, { cwd: out, stdout: "pipe", stderr: "pipe" });
    const [stdout, stderr, exit] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
    const log = join(out, `conformance-${run.specVersion}.txt`);
    writeFileSync(log, stdout + stderr);
    const total = /Total: (\d+) passed, (\d+) failed/.exec(stdout);
    summaries.push({ specVersion: run.specVersion, passed: Number(total?.[1] ?? 0), failed: Number(total?.[2] ?? 0), scenarios: (stdout.match(/^[✓✗] /gmu) ?? []).length, exit, log });
    console.log(`conformance ${run.specVersion}: ${total?.[0] ?? "no summary"} (exit ${exit})`);
  }
} finally {
  server.kill();
}

const tableFile = join(out, "tools-table.json");
const tools = Bun.spawn(["bun", "test", "./tools/mcp-conformance/tools.test.ts"], { cwd: ROOT, env: { ...process.env, FW_BIN: fw, MCP_TABLE_OUT: tableFile }, stdout: "pipe", stderr: "pipe" });
const [toolOut, toolErr, toolExit] = await Promise.all([new Response(tools.stdout).text(), new Response(tools.stderr).text(), tools.exited]);
writeFileSync(join(out, "tools-test.txt"), toolOut + toolErr);
const counts = /(\d+) pass\s+(\d+) fail/.exec(toolOut + toolErr);
console.log(`tools.test.ts: ${counts?.[1] ?? "?"} pass, ${counts?.[2] ?? "?"} fail (exit ${toolExit})`);

// Real sessions, recorded as transcripts: `fw mcp` over stdio and `fw serve` over HTTP.
const workspace = ROOT;
const stdio = await stdioClient([fw, "mcp"], { cwd: workspace });
await stdio.request("server/discover");
await stdio.request("tools/list");
await stdio.request("tools/call", { name: "linux_kernel_mcp", arguments: { action: "probe" } });
await stdio.request("tools/call", { name: "crates_info", arguments: {} });
await stdio.request("tools/call", { name: "no_such_tool", arguments: {} });
await stdio.close();
const servePort = await freePort();
const serve = Bun.spawn([fw, "serve", `127.0.0.1:${servePort}`], { cwd: workspace, stdout: "ignore", stderr: "ignore" });
let http = "";
try {
  for (let i = 0; i < 100; i++) {
    try {
      await fetch(`http://127.0.0.1:${servePort}/health`);
      break;
    } catch {
      await Bun.sleep(100);
    }
  }
  const client = await httpClient(`http://127.0.0.1:${servePort}/mcp`);
  await client.request("server/discover");
  await client.request("tools/call", { name: "wasm_kernel_mcp", arguments: { action: "probe" } });
  await client.request("tools/list", {}, { "io.modelcontextprotocol/protocolVersion": "1900-01-01" });
  http = formatTranscript(client.transcript);
} finally {
  serve.kill();
}
writeFileSync(join(out, "transcript-stdio.txt"), formatTranscript(stdio.transcript));
writeFileSync(join(out, "transcript-http.txt"), http);

const table = JSON.parse(readFileSync(tableFile, "utf8")) as { tools: string[]; rows: { transport: string; tool: string; kind: string; result: string; note: string }[] };
const failed = summaries.some((s) => s.exit !== 0) || toolExit !== 0;

const report = arg("write-report");
if (report) {
  const lines: string[] = [];
  lines.push("# fw MCP conformance", "", "Generated by `bun tools/mcp-conformance/run.ts --write-report`. Do not edit by hand: the spec gap table is `tools/mcp-conformance/gaps.md`.", "");
  lines.push(`Specification: 2026-07-28 (latest in modelcontextprotocol/modelcontextprotocol \`docs/specification/\`). Suite: ${suite.package}@${suite.version}.`, "");
  lines.push("## Official suite against the fixture catalog", "", "| Spec version | Scenarios | Checks passed | Checks failed | Baseline |", "|---|---|---|---|---|");
  for (const s of summaries) {
    const base = suite.runs.find((r) => r.specVersion === s.specVersion)?.expectedFailures;
    const listed = base ? readFileSync(join(here, base), "utf8").split("\n").filter((l) => l.startsWith("  - ")).length : 0;
    lines.push(`| ${s.specVersion} | ${s.scenarios} | ${s.passed} | ${s.failed} | ${base ? `\`${base}\`: ${listed} scenarios listed; those that apply to the version fail as expected` : "none"} |`);
  }
  lines.push("", `## Tools of \`fw mcp\` and \`fw serve\` (${table.tools.length} listed)`, "", "| Tool | Transport | Valid payload | Invalid payload |", "|---|---|---|---|");
  for (const tool of table.tools) {
    for (const transport of ["stdio", "http"]) {
      const cell = (kind: string) => table.rows.find((r) => r.tool === tool && r.transport === transport && r.kind === kind)?.result ?? "missing";
      lines.push(`| \`${tool}\` | ${transport} | ${cell("valid")} | ${cell("invalid")} |`);
    }
  }
  lines.push("", `Test file: \`tools/mcp-conformance/tools.test.ts\` (${counts?.[1] ?? "?"} pass, ${counts?.[2] ?? "?"} fail). A row reads \`pass (binary absent)\` when the tool shells out to a program that is not installed where the table was produced and answered with a tool error naming it.`, "");
  lines.push(readFileSync(join(here, "gaps.md"), "utf8").trimEnd(), "");
  writeFileSync(resolve(report), lines.join("\n"));
}
process.exit(failed ? 1 : 0);
