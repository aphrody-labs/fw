// Edge OS parity: the Rust fw-edge (wrangler dev --local on the built worker) against the TypeScript worker
// (cloudflare/worker.ts, in process) and the scenarios of tests/fw_cloudflare.test.ts.
// Run from crates/fw-edge after `worker-build --release`:  bun test tests/os.test.ts
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import worker from "../../../cloudflare/worker.ts";
import { startEdge, type Edge } from "./harness.ts";

const pkg = JSON.parse(readFileSync(new URL("../../../package.json", import.meta.url), "utf8")) as { version: string };
const cargo = readFileSync(new URL("../Cargo.toml", import.meta.url), "utf8");
const wrangler = readFileSync(new URL("../wrangler.jsonc", import.meta.url), "utf8");

let edge: Edge;
let persist: string;

beforeAll(async () => {
  persist = mkdtempSync(join(tmpdir(), "fw-edge-os-"));
  edge = await startEdge({ persistTo: persist });
}, 180_000);

afterAll(async () => {
  await edge?.stop();
  rmSync(persist, { recursive: true, force: true });
});

const ts = (path: string, init?: RequestInit) => worker.fetch(new Request(`https://edge.fw.test${path}`, init), {});
const rs = (path: string, init?: RequestInit) => fetch(`${edge.url}${path}`, init);
const post = (body: unknown, headers: Record<string, string> = {}): RequestInit => ({
  method: "POST",
  headers: { "content-type": "application/json", ...headers },
  body: JSON.stringify(body),
});

/** Strips what legitimately differs between runs and runtimes: timestamps, generated ids, hosts. */
function normalize(text: string): unknown {
  return JSON.parse(
    text
      .replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z/g, "<ts>")
      .replace(/\b(learn|exec|rule|task|acp|vec)_[0-9a-z]+_[0-9a-f-]{12}\b/g, "$1_<id>")
      .replace(/"storage": "memory"/g, '"storage": "sqlite"')
      .replace(/"storageBackend": "memory"/g, '"storageBackend": "sqlite"')
      .replace(/"sha256": "[0-9a-f]{64}"/g, '"sha256": "<sha>"'),
  );
}

describe("version and config", () => {
  test("Cargo version equals package.json version and wrangler compatibility date equals the TS constant", () => {
    expect(/^version = "([^"]+)"/m.exec(cargo)?.[1]).toBe(pkg.version);
    expect(/"compatibility_date":\s*"([^"]+)"/.exec(wrangler)?.[1]).toBe("2026-11-19");
  });
});

describe("static documents are byte-identical to the TS worker", () => {
  for (const path of ["/os/catalog", "/os/catalog?layer=data-stream", "/os/catalog?layer=nope", "/openapi.json", "/claw/status", "/claw/ranking"]) {
    test(path, async () => {
      const [a, b] = await Promise.all([rs(path), ts(path)]);
      expect(a.status).toBe(b.status);
      expect(a.headers.get("content-type")).toBe(b.headers.get("content-type"));
      expect(a.headers.get("access-control-allow-origin")).toBe("*");
      const [rustText, tsText] = [await a.text(), await b.text()];
      // the openapi servers[0].url carries the request origin
      expect(rustText.replaceAll(edge.url, "https://edge.fw.test")).toBe(tsText);
    });
  }

  test("/claw/run", async () => {
    for (const body of [{}, { goal: "ship it" }]) {
      const [a, b] = await Promise.all([rs("/claw/run", post(body)), ts("/claw/run", post(body))]);
      expect(await a.text()).toBe(await b.text());
    }
  });

  test("/mcp tools/list, initialize and ping", async () => {
    for (const method of ["tools/list", "initialize", "ping", "nope"]) {
      const body = { jsonrpc: "2.0", id: 7, method };
      const [a, b] = await Promise.all([rs("/mcp", post(body)), ts("/mcp", post(body))]);
      expect(await a.text()).toBe(await b.text());
    }
  });

  test("/os/tiers lists the same tiers; bindings are the Rust worker's declared ones", async () => {
    const a = (await (await rs("/os/tiers")).json()) as { tiers: unknown; bindings: Record<string, boolean>; version: string };
    const b = (await (await ts("/os/tiers")).json()) as { tiers: unknown; version: string };
    expect(a.tiers).toEqual(b.tiers);
    expect(a.version).toBe(b.version);
    expect(a.bindings.FW_OS).toBe(true);
    for (const absent of ["LOADER", "VFS_BLOBS", "META_DB", "MEMORY_VEC", "BROWSER", "EVENT_LOG", "TELEMETRY_PIPELINE", "SANDBOX"]) {
      expect(a.bindings[absent]).toBe(false);
    }
  });

  test("agent card matches except workersAi (the Rust worker declares the AI binding)", async () => {
    const a = (await (await rs("/.well-known/agent.json")).json()) as { url: string; capabilities: Record<string, unknown> };
    const b = (await (await ts("/.well-known/agent.json")).json()) as { url: string; capabilities: Record<string, unknown> };
    expect(a.url).toBe(`${edge.url}/a2a`);
    expect({ ...a.capabilities, workersAi: false }).toEqual(b.capabilities);
    expect({ ...a, url: "", capabilities: 0 }).toEqual({ ...b, url: "", capabilities: 0 });
  });
});

describe("CORS, health, 404", () => {
  test("OPTIONS preflight", async () => {
    const a = await rs("/os/catalog", { method: "OPTIONS" });
    const b = await ts("/os/catalog", { method: "OPTIONS" });
    expect(a.status).toBe(204);
    for (const h of ["access-control-allow-origin", "access-control-allow-methods", "access-control-allow-headers"]) {
      expect(a.headers.get(h)).toBe(b.headers.get(h));
    }
  });

  test("/health shape and bindings", async () => {
    const a = (await (await rs("/health")).json()) as Record<string, unknown> & { bindings: Record<string, boolean> };
    expect(Object.keys(a)).toEqual(["ok", "product", "version", "runtime", "compatibilityDate", "env", "deployment", "bindings"]);
    expect(a.ok).toBe(true);
    expect(a.product).toBe("fw-edge");
    expect(a.version).toBe(pkg.version);
    expect(a.runtime).toBe("cloudflare-workers");
    expect(a.compatibilityDate).toBe("2026-11-19");
    expect(a.env).toBe("production");
    expect(Object.keys(a.bindings)).toEqual(["AI", "FW_OS", "LOADER", "VFS_BLOBS", "META_DB", "MEMORY_VEC", "BROWSER", "EVENT_LOG", "TELEMETRY_PIPELINE", "SANDBOX"]);
    expect(a.bindings.LOADER).toBe(false);
  });

  test("404 body and CORS header", async () => {
    for (const path of ["/does-not-exist", "/os/vfs", "/v1/chat/completions-x"]) {
      const a = await rs(path);
      const b = await ts(path);
      expect(a.status).toBe(404);
      expect(a.headers.get("access-control-allow-origin")).toBe("*");
      expect(await a.text()).toBe(await b.text());
    }
  });

  test("pretty JSON with 2-space indent and the UTF-8 content type", async () => {
    const res = await rs("/claw/status");
    expect(res.headers.get("content-type")).toBe("application/json; charset=utf-8");
    expect((await res.text()).startsWith('{\n  "ok": true,\n  "agent"')).toBe(true);
  });
});

/** A scripted Edge OS session replayed against both workers; replies must agree after normalisation. */
const script: Array<{ name: string; path: string; init?: RequestInit }> = (() => {
  const h = { "x-fw-tenant": "diff-tenant" };
  const exec = (body: Record<string, unknown>, headers: Record<string, string> = {}) => ({ path: "/os/vfs/exec", init: post(body, { ...h, ...headers }) });
  return [
    { name: "ls root", path: "/os/vfs/ls", init: { headers: h } },
    { name: "read seed manifest", path: "/os/vfs/read?path=/etc/fw.json", init: { headers: h } },
    { name: "stat seed", path: "/os/vfs/stat?path=/etc/fw.json", init: { headers: h } },
    { name: "write json", path: "/os/vfs/write", init: post({ path: "/workspace/kernel.json", content: '{"kernel":"sqlite"}' }, h) },
    { name: "write text with metadata", path: "/os/vfs/write", init: post({ path: "/workspace/a/b/c.txt", content: "héllo\nwörld\n", metadata: { k: 1 } }, h) },
    { name: "write missing content", path: "/os/vfs/write", init: post({ path: "/x" }, h) },
    { name: "write root", path: "/os/vfs/write", init: post({ path: "/", content: "x" }, h) },
    { name: "write secrets denied", path: "/os/vfs/write", init: post({ path: "/etc/secrets/token", content: "bad" }, h) },
    { name: "read traversal denied", path: "/os/vfs/read?path=/workspace/../../etc/passwd", init: { headers: h } },
    { name: "read missing", path: "/os/vfs/read?path=/nope", init: { headers: h } },
    { name: "read dir", path: "/os/vfs/read?path=/workspace", init: { headers: h } },
    { name: "read without path", path: "/os/vfs/read", init: { headers: h } },
    { name: "read post body", path: "/os/vfs/read", init: post({ path: "/workspace/kernel.json" }, h) },
    { name: "stat ghost", path: "/os/vfs/stat?path=/ghost", init: { headers: h } },
    { name: "stat file", path: "/os/vfs/stat?path=/workspace/a/b/c.txt", init: { headers: h } },
    { name: "ls workspace", path: "/os/vfs/ls?prefix=/workspace", init: { headers: h } },
    { name: "ls guest traversal", path: "/os/vfs/ls?prefix=/../x", init: { headers: h } },
    { name: "tier0 chain", ...exec({ tier: 0, command: "mkdir -p /workspace/src && echo alpha-beta > /workspace/src/note.txt && grep alpha /workspace/src/note.txt && wc /workspace/src/note.txt" }) },
    { name: "tier0 append head tail", ...exec({ command: "echo one > l.txt; echo two >> l.txt; echo three >> l.txt; head -n 2 l.txt; tail -n 2 l.txt; wc l.txt" }) },
    { name: "tier0 builtins", ...exec({ command: "pwd && whoami && uname && echo 'a  b' c && cd /etc && pwd && help" }) },
    { name: "tier0 env", ...exec({ command: "env", env: { FOO: "bar" } }) },
    { name: "tier0 cp mv rm", ...exec({ command: "cp l.txt m.txt && mv m.txt n.txt && cat n.txt && rm n.txt && cat n.txt" }) },
    { name: "tier0 errors", ...exec({ command: "cat nope; cd /nope; nonsense; grep onlypattern; stat; rm ghost" }) },
    { name: "tier0 ls find stat", ...exec({ command: "ls /workspace; find /workspace/a; stat /workspace/l.txt" }) },
    { name: "tier0 denied write", ...exec({ command: "echo x > /etc/secrets/k" }) },
    { name: "tier1 simulated", ...exec({ tier: "auto", code: "return 21 * 2;" }) },
    { name: "tier1 prefix", ...exec({ command: "js:return 1" }) },
    { name: "tier2 simulated argv", ...exec({ tier: "auto", argv: ["python3", "-c", "print(6*7)"] }) },
    { name: "tier2 simulated command", ...exec({ tier: 2, command: "ls -la" }) },
    { name: "tier2 guest denied", ...exec({ tier: 2, argv: ["python3", "--version"] }, { "x-fw-principal": "guest" }) },
    { name: "tier1 egress denied", ...exec({ tier: 1, code: "return 1;", allowNetwork: true }) },
    { name: "gatekeeper list", path: "/os/gatekeeper", init: { headers: h } },
    { name: "gatekeeper check allow", path: "/os/gatekeeper", init: post({ capability: "vfs:read", path: "/workspace" }, h) },
    { name: "gatekeeper check deny", path: "/os/gatekeeper", init: post({ capability: "vfs:write", path: "/proc/keys/x", principal: "admin" }, h) },
    { name: "gatekeeper check missing capability", path: "/os/gatekeeper", init: post({}, h) },
    { name: "gatekeeper upsert", path: "/os/gatekeeper", init: post({ action: "upsert", id: "deny-locked-dir", capability: "vfs:write", effect: "deny", principal: "*", pathPrefix: "/workspace/locked", reason: "Locked by policy test" }, h) },
    { name: "gatekeeper upsert invalid", path: "/os/gatekeeper", init: post({ action: "upsert", capability: "x" }, h) },
    { name: "locked write denied", path: "/os/vfs/write", init: post({ path: "/workspace/locked/file.txt", content: "x" }, h) },
    { name: "gatekeeper list after", path: "/os/gatekeeper", init: { headers: h } },
    { name: "learn post", path: "/api/learn/events", init: post({ kind: "recovery", source: "fw-edge-test", summary: "Promoted verified Cloudflare Edge OS topology", tags: ["cloudflare", 7], payload: { tier: 0 } }, h) },
    { name: "learn post defaults", path: "/api/learn/events", init: post({}, h) },
    { name: "learn list", path: "/api/learn/events?limit=10", init: { headers: h } },
    { name: "learn status", path: "/api/learn/status", init: { headers: h } },
    { name: "do route miss", path: "/os/vfs/nope", init: { headers: h } },
  ];
})();

describe("Edge OS differential against the TS worker", () => {
  // The TS worker keeps an in-isolate memory VFS per tenant; the Rust worker uses the FwOSAgent Durable Object (SQLite).
  // Replies are replayed in order so both sides see the same state.
  for (const step of script) {
    test(step.name, async () => {
      const [a, b] = [await rs(step.path, step.init), await ts(step.path, step.init)];
      const [rustText, tsText] = [await a.text(), await b.text()];
      expect(a.status).toBe(b.status);
      expect(normalize(rustText)).toEqual(normalize(tsText));
    });
  }

  test("learn events are ordered newest first and counted by kind", async () => {
    const status = (await (await rs("/api/learn/status", { headers: { "x-fw-tenant": "diff-tenant" } })).json()) as { totalEvents: number; byKind: Record<string, number>; execAuditCount: number; sinks: Record<string, boolean> };
    expect(status.totalEvents).toBe(2);
    expect(status.byKind).toEqual({ observation: 1, recovery: 1 });
    expect(status.execAuditCount).toBeGreaterThan(10);
    expect(status.sinks).toEqual({ sqlite: true, k2: false, basinPipeline: false, d1: false });
  });
});

describe("tenant routing and persistence", () => {
  test("x-fw-tenant and ?tenant= select separate Durable Objects", async () => {
    await rs("/os/vfs/write", post({ path: "/workspace/who.txt", content: "alpha" }, { "x-fw-tenant": "alpha" }));
    const viaHeader = await (await rs("/os/vfs/read?path=/workspace/who.txt", { headers: { "x-fw-tenant": "alpha" } })).json();
    const viaQuery = await (await rs("/os/vfs/read?path=/workspace/who.txt&tenant=alpha")).json();
    const other = await rs("/os/vfs/read?path=/workspace/who.txt", { headers: { "x-fw-tenant": "beta" } });
    const def = await rs("/os/vfs/read?path=/workspace/who.txt");
    expect((viaHeader as { content: string }).content).toBe("alpha");
    expect((viaQuery as { content: string }).content).toBe("alpha");
    expect(other.status).toBe(404);
    expect(def.status).toBe(404);
  });

  test("the SQLite schema and seeds survive a Durable Object restart (same tables, 14 seed rules)", async () => {
    const rules = (await (await rs("/os/gatekeeper", { headers: { "x-fw-tenant": "fresh" } })).json()) as { count: number; rules: Array<{ id: string }> };
    expect(rules.count).toBe(14);
    expect(rules.rules[0]?.id).toBe("allow-admin-all");
    const again = (await (await rs("/os/gatekeeper", { headers: { "x-fw-tenant": "fresh" } })).json()) as { count: number };
    expect(again.count).toBe(14);
  });
});

describe("ACP and A2A (isolate-local state, no Workers AI call)", () => {
  test("acp initialize, session/new, ping, unknown method", async () => {
    const h = { "x-fw-tenant": "acp-tenant" };
    const rpc = async (body: unknown) => (await rs("/acp", post(body, h))).json() as Promise<{ result?: Record<string, unknown>; error?: { code: number; message: string } }>;
    const init = await rpc({ jsonrpc: "2.0", id: "a1", method: "initialize" });
    const tsInit = (await (await ts("/acp", post({ jsonrpc: "2.0", id: "a1", method: "initialize" }))).json()) as typeof init;
    expect(init).toEqual(tsInit);
    const session = await rpc({ jsonrpc: "2.0", id: "a2", method: "session/new", params: { cwd: "/workspace/../x" } });
    expect(String(session.result?.sessionId).startsWith("acp_")).toBe(true);
    expect(session.result?.cwd).toBe("/x");
    expect(session.result?.mode).toBe("edge-os");
    expect(session.result?.turns).toBe(0);
    expect((await rpc({ id: 1, method: "ping" })).result).toEqual({ protocol: "acp", method: "ping", edge: true, ok: true });
    expect((await rpc({ id: 1, method: "nope" })).error).toEqual({ code: -32601, message: "Method not found: nope" });
  });

  test("a2a tasks/get unknown, ping, unknown method", async () => {
    const rpc = async (body: unknown) => (await rs("/a2a", post(body, { "x-fw-tenant": "a2a-tenant" }))).json() as Promise<{ result?: unknown; error?: { code: number; message: string } }>;
    expect((await rpc({ id: "t3", method: "tasks/get", params: { id: "missing" } })).error).toEqual({ code: -32001, message: "Task not found: missing" });
    expect((await rpc({ id: 1, method: "ping" })).result).toEqual({ protocol: "a2a", method: "ping", edge: true, ok: true });
    expect((await rpc({ id: 1, method: "tasks/nope" })).error?.code).toBe(-32601);
  });
});

describe("MCP over HTTP against the tenant Durable Object", () => {
  test("vfs_write, vfs_read, vfs_exec, gatekeeper_check, learn_log, unknown tool", async () => {
    const h = { "x-fw-tenant": "mcp-tenant" };
    const call = async (name: string, args: unknown) =>
      (await (await rs("/mcp", post({ jsonrpc: "2.0", id: 3, method: "tools/call", params: { name, arguments: args } }, h))).json()) as {
        result?: { isError: boolean; structuredContent: Record<string, unknown>; content: Array<{ text: string }> };
        error?: { code: number };
      };
    const write = await call("vfs_write", { path: "/workspace/mcp.txt", content: "mcp-persisted" });
    expect(write.result?.isError).toBe(false);
    expect(write.result?.structuredContent.ok).toBe(true);
    expect(write.result?.content[0]?.text).toBe(JSON.stringify(write.result?.structuredContent));
    expect((await call("vfs_read", { path: "/workspace/mcp.txt" })).result?.structuredContent.content).toBe("mcp-persisted");
    expect((await call("vfs_read", {})).result?.isError).toBe(true);
    expect((await call("vfs_exec", { command: "cat /workspace/mcp.txt" })).result?.structuredContent.stdout).toBe("mcp-persisted");
    expect((await call("gatekeeper_check", { capability: "vfs:write", path: "/etc/secrets/x" })).result?.structuredContent.decision).toMatchObject({ allowed: false, matchedRuleId: "deny-secrets-write" });
    expect((await call("os_catalog", { layer: "data-stream" })).result?.structuredContent.count).toBe(2);
    expect((await call("learn_log", { kind: "mcp", summary: "via mcp" })).result?.structuredContent.ok).toBe(true);
    expect((await call("learn_status", {})).result?.structuredContent.totalEvents).toBe(1);
    expect((await call("nonexistent_tool", {})).error?.code).toBe(-32602);
  });
});

describe("static assets and /webgpu", () => {
  test("GET / is served from the ASSETS binding (run_worker_first)", async () => {
    const res = await rs("/");
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/html");
    const ts = await rs("/llms.txt");
    expect([200, 404]).toContain(ts.status);
  });

  test("/webgpu rewrites to /webgpu.html", async () => {
    const res = await rs("/webgpu");
    expect([200, 404]).toContain(res.status);
    if (res.status === 200) expect(res.headers.get("content-type")).toContain("text/html");
  });
});
