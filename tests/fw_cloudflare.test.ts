import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import worker, {
  CLOUDFLARE_EDGE_OS_CATALOG,
  FwOSAgent,
  type BasinPipelineLike,
  type D1DatabaseLike,
  type DurableObjectNamespaceLike,
  type DurableObjectStateLike,
  type DynamicWorkerLoaderLike,
  type FwWorkerEnv,
  type K2StreamLike,
  type R2BucketLike,
  type SqlStorageLike,
  type VectorizeIndexLike,
} from "../cloudflare/worker.ts";

function createSqliteDOContext(name = "sqlite-tenant"): {
  ctx: DurableObjectStateLike;
  db: Database;
} {
  const db = new Database(":memory:");
  const sql: SqlStorageLike = {
    exec<T = Record<string, unknown>>(query: string, ...bindings: unknown[]) {
      const stmt = db.query(query);
      const isRead = /^\s*(SELECT|PRAGMA|WITH)/i.test(query);
      let rows: T[] = [];
      try {
        if (isRead) {
          rows = stmt.all(...(bindings as never[])) as T[];
        } else {
          stmt.run(...(bindings as never[]));
        }
      } finally {
        stmt.finalize();
      }
      return {
        toArray: () => rows,
        one: () => {
          if (rows.length === 0) throw new Error("Expected one row");
          return rows[0]!;
        },
      };
    },
  };

  const ctx: DurableObjectStateLike = {
    id: { name, toString: () => `do:${name}` },
    storage: { sql },
    blockConcurrencyWhile: async <T>(cb: () => Promise<T>) => cb(),
  };

  return { ctx, db };
}

function createMockR2(): { bucket: R2BucketLike; store: Map<string, string> } {
  const store = new Map<string, string>();
  const bucket: R2BucketLike = {
    async get(key) {
      const value = store.get(key);
      if (value === undefined) return null;
      return {
        size: new TextEncoder().encode(value).byteLength,
        async text() {
          return value;
        },
        async arrayBuffer() {
          return new TextEncoder().encode(value).buffer;
        },
      };
    },
    async put(key, value) {
      const text = typeof value === "string" ? value : new TextDecoder().decode(value);
      store.set(key, text);
      return { key };
    },
    async delete(key) {
      store.delete(key);
    },
  };
  return { bucket, store };
}

describe("Fw Cloudflare Edge OS Worker (Nov 2026 consolidation)", () => {
  test("CORS preflight, /health, /openapi.json, /.well-known/agent.json, and 404", async () => {
    const preflight = await worker.fetch(new Request("https://edge.fw.test/os/catalog", { method: "OPTIONS" }), {});
    expect(preflight.status).toBe(204);
    expect(preflight.headers.get("access-control-allow-origin")).toBe("*");

    const health = (await (
      await worker.fetch(new Request("https://edge.fw.test/health"), { FW_ENV: "staging" })
    ).json()) as {
      ok: boolean;
      product: string;
      env: string;
      compatibilityDate: string;
      bindings: Record<string, boolean>;
    };
    expect(health.ok).toBe(true);
    expect(health.product).toBe("fw-edge");
    expect(health.env).toBe("staging");
    expect(health.compatibilityDate).toBe("2026-11-12");
    expect(health.bindings.AI).toBe(false);

    const openapi = (await (await worker.fetch(new Request("https://edge.fw.test/openapi.json"), {})).json()) as {
      openapi: string;
      paths: Record<string, unknown>;
    };
    expect(openapi.openapi).toBe("3.1.0");
    expect(openapi.paths["/v1/embeddings"]).toBeDefined();
    expect(openapi.paths["/os/catalog"]).toBeDefined();
    expect(openapi.paths["/os/vfs/exec"]).toBeDefined();
    expect(openapi.paths["/os/gatekeeper"]).toBeDefined();
    expect(openapi.paths["/api/learn/events"]).toBeDefined();

    const card = (await (
      await worker.fetch(new Request("https://edge.fw.test/.well-known/agent.json"), {})
    ).json()) as {
      name: string;
      url: string;
      protocolVersion: string;
      skills: Array<{ id: string }>;
      capabilities: { executionTiers: number[]; durableObjectVfs: boolean };
    };
    expect(card.name).toBe("Fw Edge");
    expect(card.url).toBe("https://edge.fw.test/a2a");
    expect(card.protocolVersion).toBe("1.0.1");
    expect(card.capabilities.durableObjectVfs).toBe(true);
    expect(card.capabilities.executionTiers).toEqual([0, 1, 2]);
    expect(card.skills.length).toBeGreaterThanOrEqual(4);

    const missing = await worker.fetch(new Request("https://edge.fw.test/does-not-exist"), {});
    expect(missing.status).toBe(404);
  });

  test("OpenAI-compatible /v1/models, /v1/chat/completions (fallback & Workers AI), and /v1/embeddings (+ Vectorize)", async () => {
    const models = (await (await worker.fetch(new Request("https://edge.fw.test/v1/models"), {})).json()) as {
      object: string;
      data: Array<{ id: string }>;
    };
    expect(models.object).toBe("list");
    expect(models.data.some((m) => m.id === "@cf/meta/llama-3.3-70b-instruct-fp8-fast")).toBe(true);
    expect(models.data.some((m) => m.id === "@cf/baai/bge-base-en-v1.5")).toBe(true);

    // Fallback chat without AI binding
    const fallbackChat = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/v1/chat/completions", {
          method: "POST",
          body: JSON.stringify({ messages: [{ role: "user", content: "hello edge" }] }),
        }),
        {},
      )
    ).json()) as { object: string; choices: Array<{ message: { content: string } }> };
    expect(fallbackChat.object).toBe("chat.completion");
    expect(fallbackChat.choices[0]?.message.content).toBe("[fw-edge] hello edge");

    // Live Workers AI mock
    const aiCalls: Array<{ model: string; input: Record<string, unknown> }> = [];
    const upsertedVectors: Array<{ id: string; values: number[]; metadata?: Record<string, unknown> }> = [];
    const env: FwWorkerEnv = {
      AI: {
        async run(model, input) {
          aiCalls.push({ model, input });
          if (model.includes("bge")) {
            return { data: [[0.1, 0.2, 0.3, 0.4]] };
          }
          return { response: "workers-ai-live-reply" };
        },
      },
      MEMORY_VEC: {
        async upsert(vectors) {
          upsertedVectors.push(...vectors);
          return { count: vectors.length };
        },
        async query() {
          return { matches: [] };
        },
      } satisfies VectorizeIndexLike,
    };

    const aiChat = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/v1/chat/completions", {
          method: "POST",
          body: JSON.stringify({
            model: "@cf/meta/llama-3.3-70b-instruct-fp8-fast",
            messages: [{ role: "user", content: "ping ai" }],
          }),
        }),
        env,
      )
    ).json()) as { object: string; choices: Array<{ message: { content: string } }>; raw: { response: string } };
    expect(aiChat.object).toBe("chat.completion");
    expect(aiChat.choices[0]?.message.content).toBe("workers-ai-live-reply");
    expect(aiChat.raw.response).toBe("workers-ai-live-reply");

    // Embeddings with Workers AI + Vectorize upsert
    const embAi = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/v1/embeddings", {
          method: "POST",
          headers: { "x-fw-tenant": "vec-tenant" },
          body: JSON.stringify({
            input: "fw edge os",
            upsertVectorize: true,
            ids: ["doc-1"],
          }),
        }),
        env,
      )
    ).json()) as {
      object: string;
      source: string;
      vectorizeUpserted: number;
      data: Array<{ embedding: number[] }>;
    };
    expect(embAi.object).toBe("list");
    expect(embAi.source).toBe("workers-ai");
    expect(embAi.vectorizeUpserted).toBe(1);
    expect(embAi.data[0]?.embedding).toEqual([0.1, 0.2, 0.3, 0.4]);
    expect(upsertedVectors[0]?.id).toBe("doc-1");

    // Embeddings deterministic fallback without AI binding
    const embFallback = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/v1/embeddings", {
          method: "POST",
          body: JSON.stringify({ input: ["alpha", "beta"] }),
        }),
        {},
      )
    ).json()) as { source: string; data: Array<{ embedding: number[] }> };
    expect(embFallback.source).toBe("deterministic-fallback");
    expect(embFallback.data.length).toBe(2);
    expect(embFallback.data[0]?.embedding.length).toBe(64);
    expect(aiCalls.length).toBe(2);
  });

  test("Streamable HTTP MCP JSON-RPC 2.0 (/mcp): initialize, tools/list, tools/call", async () => {
    const tenantHeaders = {
      "content-type": "application/json",
      "x-fw-tenant": "mcp-tenant",
    };

    const init = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/mcp", {
          method: "POST",
          headers: tenantHeaders,
          body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize" }),
        }),
        {},
      )
    ).json()) as { result: { protocolVersion: string; serverInfo: { name: string } } };
    expect(init.result.protocolVersion).toBe("2025-11-25");
    expect(init.result.serverInfo.name).toBe("fw-edge");

    const list = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/mcp", {
          method: "POST",
          headers: tenantHeaders,
          body: JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/list" }),
        }),
        {},
      )
    ).json()) as { result: { tools: Array<{ name: string }> } };
    const names = list.result.tools.map((t) => t.name);
    expect(names).toContain("vfs_write");
    expect(names).toContain("vfs_read");
    expect(names).toContain("vfs_exec");
    expect(names).toContain("gatekeeper_check");
    expect(names).toContain("os_catalog");
    expect(names).toContain("learn_log");

    const writeCall = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/mcp", {
          method: "POST",
          headers: tenantHeaders,
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: 3,
            method: "tools/call",
            params: { name: "vfs_write", arguments: { path: "/workspace/mcp.txt", content: "mcp-persisted" } },
          }),
        }),
        {},
      )
    ).json()) as { result: { isError: boolean; structuredContent: { ok: boolean; path: string } } };
    expect(writeCall.result.isError).toBe(false);
    expect(writeCall.result.structuredContent.ok).toBe(true);

    const readCall = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/mcp", {
          method: "POST",
          headers: tenantHeaders,
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: 4,
            method: "tools/call",
            params: { name: "vfs_read", arguments: { path: "/workspace/mcp.txt" } },
          }),
        }),
        {},
      )
    ).json()) as { result: { structuredContent: { ok: boolean; content: string } } };
    expect(readCall.result.structuredContent.content).toBe("mcp-persisted");

    const catCall = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/mcp", {
          method: "POST",
          headers: tenantHeaders,
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: 5,
            method: "tools/call",
            params: { name: "os_catalog", arguments: { layer: "data-stream" } },
          }),
        }),
        {},
      )
    ).json()) as { result: { structuredContent: { count: number } } };
    expect(catCall.result.structuredContent.count).toBe(2);

    const unknownTool = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/mcp", {
          method: "POST",
          headers: tenantHeaders,
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: 6,
            method: "tools/call",
            params: { name: "nonexistent_tool", arguments: {} },
          }),
        }),
        {},
      )
    ).json()) as { error: { code: number } };
    expect(unknownTool.error.code).toBe(-32602);
  });

  test("ACP (/acp) and A2A (/a2a) JSON-RPC 2.0 full lifecycles", async () => {
    const acpInit = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/acp", {
          method: "POST",
          body: JSON.stringify({ jsonrpc: "2.0", id: "a1", method: "initialize" }),
        }),
        {},
      )
    ).json()) as { result: { protocolVersion: string; agentCapabilities: { edgeTiers: number[] } } };
    expect(acpInit.result.protocolVersion).toBe("1.33.0");
    expect(acpInit.result.agentCapabilities.edgeTiers).toEqual([0, 1, 2]);

    const acpSession = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/acp", {
          method: "POST",
          headers: { "x-fw-tenant": "acp-tenant" },
          body: JSON.stringify({ jsonrpc: "2.0", id: "a2", method: "session/new", params: { cwd: "/workspace" } }),
        }),
        {},
      )
    ).json()) as { result: { sessionId: string; cwd: string } };
    expect(acpSession.result.sessionId.startsWith("acp_")).toBe(true);
    expect(acpSession.result.cwd).toBe("/workspace");

    const acpPrompt = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/acp", {
          method: "POST",
          headers: { "x-fw-tenant": "acp-tenant" },
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: "a3",
            method: "session/prompt",
            params: { sessionId: acpSession.result.sessionId, prompt: "summarize edge os" },
          }),
        }),
        {},
      )
    ).json()) as { result: { stopReason: string; turns: number; output: { text: string } } };
    expect(acpPrompt.result.stopReason).toBe("end_turn");
    expect(acpPrompt.result.turns).toBe(1);
    expect(acpPrompt.result.output.text).toContain("summarize edge os");

    const a2aSend = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/a2a", {
          method: "POST",
          headers: { "x-fw-tenant": "a2a-tenant" },
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: "t1",
            method: "tasks/send",
            params: { id: "task-edge-42", prompt: "audit worker" },
          }),
        }),
        {},
      )
    ).json()) as { result: { id: string; status: { state: string }; artifacts: Array<{ parts: Array<{ text: string }> }> } };
    expect(a2aSend.result.id).toBe("task-edge-42");
    expect(a2aSend.result.status.state).toBe("completed");
    expect(a2aSend.result.artifacts[0]?.parts[0]?.text).toContain("audit worker");

    const a2aGet = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/a2a", {
          method: "POST",
          headers: { "x-fw-tenant": "a2a-tenant" },
          body: JSON.stringify({ jsonrpc: "2.0", id: "t2", method: "tasks/get", params: { id: "task-edge-42" } }),
        }),
        {},
      )
    ).json()) as { result: { id: string; status: { state: string } } };
    expect(a2aGet.result.id).toBe("task-edge-42");
    expect(a2aGet.result.status.state).toBe("completed");

    const a2aMissing = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/a2a", {
          method: "POST",
          headers: { "x-fw-tenant": "a2a-tenant" },
          body: JSON.stringify({ jsonrpc: "2.0", id: "t3", method: "tasks/get", params: { id: "missing" } }),
        }),
        {},
      )
    ).json()) as { error: { code: number } };
    expect(a2aMissing.error.code).toBe(-32001);
  });

  test("/os/catalog and /os/tiers expose the complete verified Nov 2026 Cloudflare Edge OS ecosystem", async () => {
    const res = (await (await worker.fetch(new Request("https://edge.fw.test/os/catalog"), {})).json()) as {
      ok: boolean;
      count: number;
      items: Array<{ id: string; name: string; version: string; url: string }>;
    };
    expect(res.ok).toBe(true);
    expect(res.count).toBe(CLOUDFLARE_EDGE_OS_CATALOG.length);
    expect(res.count).toBeGreaterThanOrEqual(15);

    const joined = res.items.map((i) => `${i.name}@${i.version}`).join(" | ");
    expect(joined).toContain("cloudflare/cloudflare-os");
    expect(joined).toContain("cloudflare/computer");
    expect(joined).toContain("@cloudflare/dofs");
    expect(joined).toContain("cloudflare/agents@0.33.0");
    expect(joined).toContain("@cloudflare/codemode@0.5.6");
    expect(joined).toContain("@cloudflare/sandbox@1.0.3");
    expect(joined).toContain("@cloudflare/containers@0.3.7");
    expect(joined).toContain("worker@0.9.0");
    expect(joined).toContain("Browser Run + Kitesurf");
    expect(joined).toContain("cloudflare/vibesdk");
    expect(joined).toContain("cloudflare/moltworker");
    expect(joined).toContain("hono@4.13");
    expect(joined).toContain("just-bash@3.6.1");
    expect(joined).toContain("@modelcontextprotocol/server@2.x");
    expect(joined).toContain("Cloudflare Basin");
    expect(joined).toContain("Cloudflare K2");

    const tiers = (await (await worker.fetch(new Request("https://edge.fw.test/os/tiers"), {})).json()) as {
      ok: boolean;
      tiers: Array<{ tier: number; id: string }>;
    };
    expect(tiers.ok).toBe(true);
    expect(tiers.tiers.map((t) => t.tier)).toEqual([0, 1, 2]);
  });

  test("3-tier Edge OS VFS, Gatekeeper policy checks, CodeMode LOADER, and Sandbox microVM dispatch", async () => {
    const headers = { "content-type": "application/json", "x-fw-tenant": "tiers-tenant" };
    const { bucket, store } = createMockR2();

    const loaderConfigs: Array<{ compatibilityDate: string; globalOutbound: unknown }> = [];
    const mockLoader: DynamicWorkerLoaderLike = {
      load(cfg) {
        loaderConfigs.push({ compatibilityDate: cfg.compatibilityDate, globalOutbound: cfg.globalOutbound });
        return {
          getEntrypoint() {
            return {
              async fetch() {
                return new Response(JSON.stringify({ ok: true, value: 42 }), {
                  headers: { "content-type": "application/json" },
                });
              },
            };
          },
        };
      },
    };

    const sandboxCalls: Array<string[]> = [];
    const mockSandbox: DurableObjectNamespaceLike = {
      getByName() {
        return {
          async fetch() {
            return new Response(JSON.stringify({ stdout: "http-fallback", exitCode: 0 }));
          },
          async exec(argv) {
            sandboxCalls.push(argv);
            return {
              async output() {
                return { stdout: `sandbox-ran:${argv.join(" ")}`, stderr: "", exitCode: 0 };
              },
            };
          },
        };
      },
    };

    const env: FwWorkerEnv = {
      VFS_BLOBS: bucket,
      LOADER: mockLoader,
      SANDBOX: mockSandbox,
    };

    // R2 spillover write + transparent read hydration
    const writeR2 = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/os/vfs/write", {
          method: "POST",
          headers,
          body: JSON.stringify({ path: "/workspace/blob.txt", content: "spillover-payload", spillToR2: true }),
        }),
        env,
      )
    ).json()) as { ok: boolean; storage: string; blobKey: string };
    expect(writeR2.ok).toBe(true);
    expect(writeR2.storage).toBe("r2");
    expect(store.size).toBe(1);

    const readR2 = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/os/vfs/read?path=/workspace/blob.txt", { headers }),
        env,
      )
    ).json()) as { ok: boolean; content: string; storage: string };
    expect(readR2.ok).toBe(true);
    expect(readR2.storage).toBe("r2");
    expect(readR2.content).toBe("spillover-payload");

    // Tier-0 POSIX shell execution with redirection, chaining, grep, wc
    const tier0 = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/os/vfs/exec", {
          method: "POST",
          headers,
          body: JSON.stringify({
            tier: 0,
            command: "mkdir -p /workspace/src && echo alpha-beta > /workspace/src/note.txt && grep alpha /workspace/src/note.txt && wc /workspace/src/note.txt",
          }),
        }),
        env,
      )
    ).json()) as { ok: boolean; tier: number; runtime: string; exitCode: number; stdout: string };
    expect(tier0.ok).toBe(true);
    expect(tier0.tier).toBe(0);
    expect(tier0.runtime).toBe("do-posix-shell");
    expect(tier0.stdout).toContain("alpha-beta");
    expect(tier0.stdout).toContain("/workspace/src/note.txt");

    // Tier-1 Dynamic Worker Loader CodeMode (auto-selected via `code`)
    const tier1 = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/os/vfs/exec", {
          method: "POST",
          headers,
          body: JSON.stringify({ tier: "auto", code: "return 21 * 2;" }),
        }),
        env,
      )
    ).json()) as { ok: boolean; tier: number; runtime: string; isolated: boolean; globalOutbound: unknown; result: { value: number } };
    expect(tier1.ok).toBe(true);
    expect(tier1.tier).toBe(1);
    expect(tier1.runtime).toBe("dynamic-worker-loader");
    expect(tier1.isolated).toBe(true);
    expect(tier1.globalOutbound).toBeNull();
    expect(tier1.result.value).toBe(42);
    expect(loaderConfigs[0]?.globalOutbound).toBeNull();

    // Tier-1 simulated fallback when LOADER is unbound
    const tier1Sim = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/os/vfs/exec", {
          method: "POST",
          headers,
          body: JSON.stringify({ tier: 1, code: "return 1;" }),
        }),
        {},
      )
    ).json()) as { ok: boolean; tier: number; simulated: boolean; bindingRequired: string };
    expect(tier1Sim.ok).toBe(true);
    expect(tier1Sim.simulated).toBe(true);
    expect(tier1Sim.bindingRequired).toBe("LOADER");

    // Tier-2 Sandbox 1.0 microVM (auto-selected for python3 argv)
    const tier2 = (await (
      await worker.fetch(
        new Request("https://edge.fw.test/os/vfs/exec", {
          method: "POST",
          headers,
          body: JSON.stringify({ tier: "auto", argv: ["python3", "-c", "print(6*7)"] }),
        }),
        env,
      )
    ).json()) as { ok: boolean; tier: number; runtime: string; stdout: string };
    expect(tier2.ok).toBe(true);
    expect(tier2.tier).toBe(2);
    expect(tier2.runtime).toBe("cloudflare-sandbox-microvm");
    expect(tier2.stdout).toBe("sandbox-ran:python3 -c print(6*7)");
    expect(sandboxCalls.length).toBe(1);

    // Gatekeeper denials: protected path, traversal, guest Tier-2, default net:egress
    const deniedSecret = await worker.fetch(
      new Request("https://edge.fw.test/os/vfs/write", {
        method: "POST",
        headers,
        body: JSON.stringify({ path: "/etc/secrets/token", content: "bad" }),
      }),
      env,
    );
    expect(deniedSecret.status).toBe(403);

    const deniedTraversal = await worker.fetch(
      new Request("https://edge.fw.test/os/vfs/read?path=/workspace/../../etc/passwd", { headers }),
      env,
    );
    expect(deniedTraversal.status).toBe(403);

    const deniedGuestTier2 = await worker.fetch(
      new Request("https://edge.fw.test/os/vfs/exec", {
        method: "POST",
        headers: { ...headers, "x-fw-principal": "guest" },
        body: JSON.stringify({ tier: 2, argv: ["python3", "--version"] }),
      }),
      env,
    );
    expect(deniedGuestTier2.status).toBe(403);

    const deniedEgress = await worker.fetch(
      new Request("https://edge.fw.test/os/vfs/exec", {
        method: "POST",
        headers,
        body: JSON.stringify({ tier: 1, code: "return 1;", allowNetwork: true }),
      }),
      env,
    );
    expect(deniedEgress.status).toBe(403);

    // Upsert custom Gatekeeper rule and verify enforcement
    const upsertRes = await worker.fetch(
      new Request("https://edge.fw.test/os/gatekeeper", {
        method: "POST",
        headers,
        body: JSON.stringify({
          action: "upsert",
          id: "deny-locked-dir",
          capability: "vfs:write",
          effect: "deny",
          principal: "*",
          pathPrefix: "/workspace/locked",
          reason: "Locked by policy test",
        }),
      }),
      env,
    );
    expect(upsertRes.status).toBe(200);

    const checkLocked = await worker.fetch(
      new Request("https://edge.fw.test/os/gatekeeper", {
        method: "POST",
        headers,
        body: JSON.stringify({ capability: "vfs:write", path: "/workspace/locked/file.txt" }),
      }),
      env,
    );
    expect(checkLocked.status).toBe(403);

    const policies = (await (
      await worker.fetch(new Request("https://edge.fw.test/os/gatekeeper", { headers }), env)
    ).json()) as { ok: boolean; rules: Array<{ id: string }> };
    expect(policies.ok).toBe(true);
    expect(policies.rules.some((r) => r.id === "deny-locked-dir")).toBe(true);
  });

  test("FwOSAgent DO SQLite kernel (real bun:sqlite adapter) + K2 / Basin Pipelines / D1 learning sinks + DO namespace routing", async () => {
    const { ctx, db } = createSqliteDOContext("prod-tenant");
    const k2Records: unknown[] = [];
    const basinRecords: unknown[] = [];
    const d1Bindings: unknown[][] = [];

    const k2: K2StreamLike = {
      async produce(records) {
        k2Records.push(...records);
        return { accepted: records.length };
      },
    };
    const basin: BasinPipelineLike = {
      async send(records) {
        basinRecords.push(...records);
      },
    };
    const d1: D1DatabaseLike = {
      prepare() {
        return {
          bind(...values) {
            d1Bindings.push(values);
            return {
              async all() {
                return { results: [], success: true };
              },
              async first() {
                return null;
              },
              async run() {
                return { success: true };
              },
            };
          },
        };
      },
    };

    try {
      const doEnv: FwWorkerEnv = {
        EVENT_LOG: k2,
        TELEMETRY_PIPELINE: basin,
        META_DB: d1,
      };
      const agent = new FwOSAgent(ctx, doEnv);

      // Route Worker traffic through env.FW_OS DurableObjectNamespace binding
      const workerEnv: FwWorkerEnv = {
        ...doEnv,
        FW_OS: {
          getByName: () => ({
            fetch: (input, init) => agent.fetch(input instanceof Request ? input : new Request(input, init)),
          }),
        },
      };

      const w = (await (
        await worker.fetch(
          new Request("https://edge.fw.test/os/vfs/write", {
            method: "POST",
            headers: { "content-type": "application/json", "x-fw-tenant": "prod-tenant" },
            body: JSON.stringify({ path: "/workspace/kernel.json", content: JSON.stringify({ kernel: "sqlite" }) }),
          }),
          workerEnv,
        )
      ).json()) as { ok: boolean; storage: string; sha256: string };
      expect(w.ok).toBe(true);
      expect(w.storage).toBe("sqlite");
      expect(w.sha256.length).toBe(64);

      const stat = (await (
        await worker.fetch(
          new Request("https://edge.fw.test/os/vfs/stat?path=/workspace/kernel.json", {
            headers: { "x-fw-tenant": "prod-tenant" },
          }),
          workerEnv,
        )
      ).json()) as { ok: boolean; exists: boolean; storage: string };
      expect(stat.ok).toBe(true);
      expect(stat.exists).toBe(true);
      expect(stat.storage).toBe("sqlite");

      const ls = (await (
        await worker.fetch(
          new Request("https://edge.fw.test/os/vfs/ls?prefix=/workspace", {
            headers: { "x-fw-tenant": "prod-tenant" },
          }),
          workerEnv,
        )
      ).json()) as { ok: boolean; storage: string; entries: Array<{ path: string }> };
      expect(ls.storage).toBe("sqlite");
      expect(ls.entries.some((e) => e.path === "/workspace/kernel.json")).toBe(true);

      // Autonomous learning log append -> SQLite + K2 + Basin Pipelines + D1
      const learnPost = await worker.fetch(
        new Request("https://edge.fw.test/api/learn/events", {
          method: "POST",
          headers: { "content-type": "application/json", "x-fw-tenant": "prod-tenant" },
          body: JSON.stringify({
            kind: "recovery",
            source: "fw-edge-test",
            summary: "Promoted verified Cloudflare Edge OS topology",
            tags: ["cloudflare", "edge-os", "k2", "basin"],
            payload: { tier: 0 },
          }),
        }),
        workerEnv,
      );
      expect(learnPost.status).toBe(201);
      const learnBody = (await learnPost.json()) as {
        ok: boolean;
        storage: string;
        forwarded: { k2: boolean; basinPipeline: boolean; d1: boolean };
      };
      expect(learnBody.ok).toBe(true);
      expect(learnBody.storage).toBe("sqlite");
      expect(learnBody.forwarded).toEqual({ k2: true, basinPipeline: true, d1: true });
      expect(k2Records.length).toBe(1);
      expect(basinRecords.length).toBe(1);
      expect(d1Bindings.length).toBe(1);

      const learnEvents = (await (
        await worker.fetch(
          new Request("https://edge.fw.test/api/learn/events", {
            headers: { "x-fw-tenant": "prod-tenant" },
          }),
          workerEnv,
        )
      ).json()) as { ok: boolean; storage: string; count: number; events: Array<{ summary: string }> };
      expect(learnEvents.storage).toBe("sqlite");
      expect(learnEvents.count).toBe(1);
      expect(learnEvents.events[0]?.summary).toContain("Cloudflare Edge OS");

      const learnStatus = (await (
        await worker.fetch(
          new Request("https://edge.fw.test/api/learn/status", {
            headers: { "x-fw-tenant": "prod-tenant" },
          }),
          workerEnv,
        )
      ).json()) as {
        ok: boolean;
        storageBackend: string;
        totalEvents: number;
        byKind: Record<string, number>;
        sinks: { sqlite: boolean; k2: boolean; basinPipeline: boolean; d1: boolean };
      };
      expect(learnStatus.ok).toBe(true);
      expect(learnStatus.storageBackend).toBe("sqlite");
      expect(learnStatus.totalEvents).toBe(1);
      expect(learnStatus.byKind.recovery).toBe(1);
      expect(learnStatus.sinks).toEqual({ sqlite: true, k2: true, basinPipeline: true, d1: true });
    } finally {
      db.close();
    }
  });

  test("FwOSAgent standalone in-memory VFS works without Durable Object SQLite storage", async () => {
    const agent = new FwOSAgent();
    const w = await agent.vfsWrite("/workspace/mem.txt", "in-memory-vfs");
    expect(w.ok).toBe(true);
    expect(w.storage).toBe("memory");

    const r = await agent.vfsRead("/workspace/mem.txt");
    expect(r.ok).toBe(true);
    expect(r.content).toBe("in-memory-vfs");
    expect(r.storage).toBe("memory");

    const execRes = await agent.vfsExec({ tier: 0, command: "cat /workspace/mem.txt" });
    expect(execRes.ok).toBe(true);
    expect(execRes.stdout).toBe("in-memory-vfs");
  });
});
