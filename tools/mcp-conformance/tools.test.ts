// Every tool `fw mcp` (stdio) and `fw serve` (/mcp) lists, called with a valid and an invalid payload, with assertions on the
// result bodies. FW_BIN names the fw binary (default: target/debug/fw, then target/release/fw). When MCP_TABLE_OUT is set
// the rows are written there as JSON for run.ts.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { buildCases, type Case } from "./cases.ts";
import { type Client, httpClient, type Json, stdioClient } from "./client.ts";

const ROOT = resolve(import.meta.dir, "..", "..");
const exe = process.platform === "win32" ? "fw.exe" : "fw";
const FW = process.env.FW_BIN ?? [join(ROOT, "target", "debug", exe), join(ROOT, "target", "release", exe)].find(existsSync) ?? "";
const rows: { transport: string; tool: string; kind: string; result: string; note: string }[] = [];

let workspace = "";
let pages: ReturnType<typeof Bun.serve>;
let server: ReturnType<typeof Bun.spawn> | undefined;
const cases: Record<string, Record<string, Case>> = {};
const clients: Record<string, Client> = {};
let listed: string[] = [];
let httpUrl = "";

const text = (result: Json): string => (result.content ?? []).map((c: Json) => c.text ?? "").join("");

async function freePort(): Promise<number> {
  const probe = Bun.listen({ hostname: "127.0.0.1", port: 0, socket: { data() {} } });
  const port = probe.port;
  probe.stop(true);
  return port;
}

beforeAll(async () => {
  expect(FW, "set FW_BIN to a built fw binary").not.toBe("");
  workspace = mkdtempSync(join(tmpdir(), "fw-mcp-tools-"));
  writeFileSync(join(workspace, "notes.txt"), "line one\nneedle in a haystack\n");
  for (const tag of ["stdio", "http"]) writeFileSync(join(workspace, `edit-${tag}.txt`), "before\n");
  writeFileSync(join(workspace, "main.rs"), "fn main() {}\n");
  pages = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response("<p>served by the test</p>", { headers: { "content-type": "text/html" } }) });
  for (const tag of ["stdio", "http"]) cases[tag] = buildCases({ workspace, url: `http://127.0.0.1:${pages.port}`, tag });

  const env = { FW_ROOT: ROOT };
  clients.stdio = await stdioClient([FW, "mcp"], { cwd: workspace, env });
  const port = await freePort();
  server = Bun.spawn([FW, "serve", `127.0.0.1:${port}`], { cwd: workspace, env: { ...process.env, ...env }, stdout: "ignore", stderr: "ignore" });
  httpUrl = `http://127.0.0.1:${port}/mcp`;
  for (let i = 0; i < 100; i++) {
    try {
      await fetch(`http://127.0.0.1:${port}/health`);
      break;
    } catch {
      await Bun.sleep(100);
    }
  }
  clients.http = await httpClient(httpUrl);
  const list = await clients.stdio.request("tools/list");
  listed = list.result.tools.map((t: Json) => t.name);
}, 60_000);

afterAll(async () => {
  await clients.stdio?.close();
  server?.kill();
  pages?.stop(true);
  if (workspace) rmSync(workspace, { recursive: true, force: true });
  if (process.env.MCP_TABLE_OUT) writeFileSync(process.env.MCP_TABLE_OUT, JSON.stringify({ fw: FW, tools: listed, rows }, null, 2));
});

function record(transport: string, tool: string, kind: string, result: "pass" | "fail" | "pass (binary absent)", note = "") {
  rows.push({ transport, tool, kind, result, note });
}

describe("tool list", () => {
  test("stdio and HTTP list the same tools, in a deterministic order, each with an object inputSchema", async () => {
    const [a, b] = [await clients.stdio.request("tools/list"), await clients.http.request("tools/list")];
    expect(a.result.resultType).toBe("complete");
    expect(a.result.tools).toEqual(b.result.tools);
    const names: string[] = a.result.tools.map((t: Json) => t.name);
    expect(names).toEqual([...names].sort());
    for (const tool of a.result.tools) {
      expect(tool.inputSchema.type, tool.name).toBe("object");
      expect(tool.description.length, tool.name).toBeGreaterThan(10);
      expect(typeof tool.annotations.readOnlyHint, tool.name).toBe("boolean");
    }
  });

  test("every listed tool has a case and every case names a listed tool", () => {
    expect(listed.filter((name) => !(name in cases.stdio))).toEqual([]);
    expect(Object.keys(cases.stdio).filter((name) => !listed.includes(name))).toEqual([]);
  });
});

for (const transport of ["stdio", "http"] as const) {
  describe(`${transport}: tools/call`, () => {
    test("discover answers with versions, capabilities, identity and cache hints", async () => {
      const { result } = await clients[transport].request("server/discover");
      expect(result.supportedVersions[0]).toBe("2026-07-28");
      expect(result.capabilities.tools).toBeDefined();
      expect(result._meta["io.modelcontextprotocol/serverInfo"].name).toBe("fw");
      expect(result.ttlMs).toBeGreaterThanOrEqual(0);
    });

    test("an unknown tool is a -32602 protocol error", async () => {
      const res = await clients[transport].request("tools/call", { name: "no_such_tool", arguments: {} });
      expect(res.error.code).toBe(-32602);
      expect(res.error.message).toContain("no_such_tool");
    });

    // One test per case; the "has a case" test above ties the cases to the live tools/list.
    for (const tool of Object.keys(buildCases({ workspace: "", url: "", tag: "" }))) {
      test(`${tool}: valid payload`, async () => {
        const c = cases[transport][tool];
        const res = await clients[transport].request("tools/call", { name: tool, arguments: c.valid });
        expect(res.error, JSON.stringify(res.error)).toBeUndefined();
        expect(res.result.resultType).toBe("complete");
        if (res.result.isError && c.needsBinary && /not found|no such file|os error 2|cannot find|program/i.test(text(res.result))) {
          record(transport, tool, "valid", "pass (binary absent)", `${c.needsBinary} is not installed: ${text(res.result).slice(0, 80)}`);
          return;
        }
        const problem = res.result.isError ? `isError: ${text(res.result).slice(0, 160)}` : c.check(res.result);
        record(transport, tool, "valid", problem ? "fail" : "pass", problem ?? "");
        expect(problem).toBeUndefined();
      });

      test(`${tool}: invalid payload`, async () => {
        const c = cases[transport][tool];
        const res = await clients[transport].request("tools/call", { name: tool, arguments: c.invalid });
        expect(res.error, JSON.stringify(res.error)).toBeUndefined();
        const failed = res.result.isError === true && c.problem.test(text(res.result));
        record(transport, tool, "invalid", failed ? "pass" : "fail", failed ? text(res.result).slice(0, 90) : `expected isError matching ${c.problem}, got ${JSON.stringify(res.result).slice(0, 160)}`);
        expect(res.result.isError).toBe(true);
        expect(text(res.result)).toMatch(c.problem);
      });
    }
  });
}

describe("legacy clients", () => {
  test("initialize, ping and tools/list work over stdio and HTTP", async () => {
    for (const transport of ["stdio", "http"] as const) {
      const old = transport === "stdio" ? await stdioClient([FW, "mcp"], { cwd: workspace, env: { FW_ROOT: ROOT }, era: "legacy" }) : await httpClient(httpUrl, { era: "legacy" });
      const init = await old.request("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "old", version: "1" } });
      expect(init.result.protocolVersion).toBe("2025-11-25");
      expect(init.result.serverInfo.name).toBe("fw");
      await old.notify("notifications/initialized");
      const list = await old.request("tools/list");
      expect(list.result.tools.map((t: Json) => t.name)).toEqual(listed);
      expect(list.result.resultType).toBeUndefined();
      const call = await old.request("tools/call", { name: "linux_kernel_mcp", arguments: { action: "probe" } });
      expect(call.result.isError).toBe(false);
      await old.close();
    }
  });
});
