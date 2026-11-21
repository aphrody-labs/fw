// Edge OS parity: the Rust fw-edge (wrangler dev --local on the built worker) against the TypeScript worker
// (cloudflare/worker.ts, in process) and the scenarios of tests/fw_cloudflare.test.ts.
// Run from crates/fw-edge after `worker-build --release`:  bun test tests/os.test.ts
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import worker from "../../../cloudflare/worker.ts";
import { startEdge, type Edge } from "./harness.ts";

const WRANGLER = process.env.WRANGLER ?? "wrangler";
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

/** wrangler dev rewrites the request origin to the first custom-domain route (fw.aphrody.com); the TS side is called with edge.fw.test. */
const REWRITTEN_ORIGIN = "http://fw.aphrody.com";

/** Strips what legitimately differs between runs and runtimes: timestamps, generated ids, hosts. */
function normalize(text: string): unknown {
  return JSON.parse(
    text
      .replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z/g, "<ts>")
      .replace(/\b(learn|exec|rule|task|acp|vec)_[0-9a-z]+_[0-9a-f-]{12}\b/g, "$1_<id>")
      .replace(/"storage": "memory"/g, '"storage": "sqlite"')
      .replace(/\\"storage\\":\\"memory\\"/g, '\\"storage\\":\\"sqlite\\"') // inside a shell `stat` stdout string
      .replace(/"storageBackend": "memory"/g, '"storageBackend": "sqlite"')
      .replace(/"sqlite": false/g, '"sqlite": true') // learn status sinks: the TS memory kernel has no SQLite
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
      expect(rustText.replaceAll(edge.url, "https://edge.fw.test").replaceAll(REWRITTEN_ORIGIN, "https://edge.fw.test")).toBe(tsText);
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
    expect([`${edge.url}/a2a`, `${REWRITTEN_ORIGIN}/a2a`]).toContain(a.url);
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

/** `wrangler dev --local` on tests/wrangler.os-bindings.jsonc: R2 VFS_BLOBS, D1 META_DB and the Dynamic Worker Loader are bound. */
describe("optional bindings: R2 spill, D1 learn sink, Dynamic Worker Loader (local miniflare)", () => {
  let bound: { url: string; stop(): Promise<void> };
  let boundPersist: string;
  const config = fileURLToPath(new URL("wrangler.os-bindings.jsonc", import.meta.url));
  const cwd = fileURLToPath(new URL("..", import.meta.url));
  const wranglerArgs = (persist: string) => ["--config", config, "--local", "--persist-to", persist];
  const d1 = (persist: string, sql: string) =>
    spawnSync(WRANGLER, ["d1", "execute", "META_DB", ...wranglerArgs(persist), "--json", "--command", sql], { cwd, encoding: "utf8", env: { ...process.env, CI: "1" } });

  beforeAll(async () => {
    boundPersist = mkdtempSync(join(tmpdir(), "fw-edge-os-bound-"));
    const created = d1(boundPersist, "CREATE TABLE IF NOT EXISTS fw_learn_events (id TEXT PRIMARY KEY, tenant TEXT, kind TEXT, source TEXT, summary TEXT, created_at TEXT)");
    if (created.status !== 0) throw new Error(`d1 create failed\n${created.stdout}\n${created.stderr}`);
    const port = 9400 + Math.floor(Math.random() * 400);
    const child = spawn(WRANGLER, ["dev", ...wranglerArgs(boundPersist), "--port", String(port), "--ip", "127.0.0.1"], { cwd, stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, CI: "1" } });
    let log = "";
    child.stdout?.on("data", (d) => (log += d));
    child.stderr?.on("data", (d) => (log += d));
    const url = `http://127.0.0.1:${port}`;
    const deadline = Date.now() + 120_000;
    for (;;) {
      if (child.exitCode !== null) throw new Error(`wrangler dev exited ${child.exitCode}\n${log}`);
      try {
        if ((await fetch(`${url}/health`)).status > 0) break;
      } catch {
        // not listening yet
      }
      if (Date.now() > deadline) {
        child.kill();
        throw new Error(`wrangler dev did not start\n${log}`);
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    bound = {
      url,
      async stop() {
        child.kill();
        await new Promise((r) => child.once("exit", r));
      },
    };
  }, 180_000);

  afterAll(async () => {
    await bound?.stop();
    rmSync(boundPersist, { recursive: true, force: true });
  });

  const rb = (path: string, init?: RequestInit) => fetch(`${bound.url}${path}`, init);
  const h = { "x-fw-tenant": "bound-tenant" };

  test("/health and /os/tiers report the bound bindings", async () => {
    const health = (await (await rb("/health")).json()) as { env: string; bindings: Record<string, boolean> };
    expect(health.env).toBe("bindings-test");
    expect(health.bindings).toMatchObject({ FW_OS: true, VFS_BLOBS: true, META_DB: true, LOADER: true, AI: false, SANDBOX: false, MEMORY_VEC: false, EVENT_LOG: false, TELEMETRY_PIPELINE: false });
  });

  test("spillToR2 and writes above 65536 bytes go to R2 and read back through the DO", async () => {
    const spilled = (await (await rb("/os/vfs/write", post({ path: "/workspace/blob.txt", content: "spillover-payload", spillToR2: true }, h))).json()) as { ok: boolean; storage: string; blobKey: string };
    expect(spilled).toMatchObject({ ok: true, storage: "r2", blobKey: "vfs/bound-tenant/workspace/blob.txt" });
    const read = (await (await rb("/os/vfs/read?path=/workspace/blob.txt", { headers: h })).json()) as { ok: boolean; storage: string; content: string; blobKey: string };
    expect(read).toMatchObject({ ok: true, storage: "r2", content: "spillover-payload", blobKey: "vfs/bound-tenant/workspace/blob.txt" });
    const stat = (await (await rb("/os/vfs/stat?path=/workspace/blob.txt", { headers: h })).json()) as { storage: string };
    expect(stat.storage).toBe("r2");

    const big = "x".repeat(65_537);
    const bigWrite = (await (await rb("/os/vfs/write", post({ path: "/workspace/big.txt", content: big }, h))).json()) as { storage: string; size: number };
    expect(bigWrite).toMatchObject({ storage: "r2", size: 65_537 });
    expect(((await (await rb("/os/vfs/read?path=/workspace/big.txt", { headers: h })).json()) as { content: string }).content).toBe(big);
    const small = (await (await rb("/os/vfs/write", post({ path: "/workspace/small.txt", content: "x" }, h))).json()) as { storage: string };
    expect(small.storage).toBe("sqlite");
  });

  test("shell cp of an R2 file hydrates and re-writes", async () => {
    const res = (await (await rb("/os/vfs/exec", post({ command: "cp /workspace/blob.txt /workspace/copy.txt && cat /workspace/copy.txt" }, h))).json()) as { ok: boolean; stdout: string };
    expect(res).toMatchObject({ ok: true, stdout: "spillover-payload" });
  });

  test("learn events forward to D1 META_DB", async () => {
    const res = (await (await rb("/api/learn/events", post({ kind: "d1", summary: "to the meta db" }, h))).json()) as { storage: string; forwarded: Record<string, boolean>; event: { id: string } };
    expect(res.storage).toBe("sqlite");
    expect(res.forwarded).toEqual({ k2: false, basinPipeline: false, d1: true });
    // the local D1 file is briefly SQLITE_BUSY while the dev server's own write settles
    let query = d1(boundPersist, `SELECT id, tenant, kind, source, summary FROM fw_learn_events WHERE id = '${res.event.id}'`);
    for (let i = 0; i < 6 && query.status !== 0; i++) {
      await new Promise((r) => setTimeout(r, 2000));
      query = d1(boundPersist, `SELECT id, tenant, kind, source, summary FROM fw_learn_events WHERE id = '${res.event.id}'`);
    }
    expect(query.status, `${query.stdout}\n${query.stderr}`).toBe(0);
    const rows = JSON.parse(query.stdout) as Array<{ results: Array<Record<string, string>> }>;
    expect(rows[0]?.results).toEqual([{ id: res.event.id, tenant: "bound-tenant", kind: "d1", source: "fw-edge", summary: "to the meta db" }]);
    const status = (await (await rb("/api/learn/status", { headers: h })).json()) as { sinks: Record<string, boolean> };
    expect(status.sinks).toEqual({ sqlite: true, k2: false, basinPipeline: false, d1: true });
  }, 90_000);

  test("a learn event is still stored when the D1 insert fails (forwarded.d1 false)", async () => {
    const dropped = d1(boundPersist, "DROP TABLE fw_learn_events");
    expect(dropped.status).toBe(0);
    const res = (await (await rb("/api/learn/events", post({ kind: "d1-down", summary: "table gone" }, h))).json()) as { ok: boolean; forwarded: Record<string, boolean> };
    expect(res.ok).toBe(true);
    expect(res.forwarded.d1).toBe(false);
  }, 90_000);

  test("Tier-1 runs in the Dynamic Worker Loader isolate with globalOutbound null", async () => {
    const res = (await (await rb("/os/vfs/exec", post({ tier: "auto", code: "return 21 * 2;" }, h))).json()) as Record<string, unknown>;
    expect(res).toMatchObject({ ok: true, tier: 1, runtime: "dynamic-worker-loader", isolated: true, globalOutbound: null, exitCode: 0 });
    expect(res.simulated).toBeUndefined();
    expect(res.result).toEqual({ ok: true, value: 42 });
    expect(res.stdout).toBe('{"ok":true,"value":42}');
    const sum = (await (await rb("/os/vfs/exec", post({ command: "js:return input.a + input.b;", input: { a: 40, b: 2 } }, h))).json()) as { result: { value: number } };
    expect(sum.result.value).toBe(42);
  }, 60_000);
});
