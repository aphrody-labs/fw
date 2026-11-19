import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import openclawFwPlugin, {
  createOpenClawFwPlugin,
  type OpenClawToolRegistration,
} from "../plugins/openclaw-fw/index.ts";
import {
  computeChainedPrefixBlocks,
  createBunNativeAiHandler,
  KV_BLOCK_TOKEN_SIZE,
  PrefixKvRagStore,
  RuntimeToolRegistry,
  type VNode,
} from "../src/bun_native_ai.ts";
import {
  migrateOpenClawToFw,
  OPENCLAW_BUN_RUNTIME_PATCH_NOTES,
} from "../src/migrate_openclaw_to_fw.ts";

const ROOT = resolve(import.meta.dir, "..");

const SHARED_SYSTEM_PREFIX =
  "You are Fairwind distributed inference router orchestrating disaggregated prefill and decode workers " +
  "across local RTX 4070 GPU and European VPS cluster nodes with deterministic prefix KV cache affinity " +
  "using sixteen token block chained SHA256 digests and hybrid SQLite FTS5 vector retrieval";

describe("Fairwind Bun Native AI Runtime — 3 Pillars (zero external deps)", () => {
  describe("Pillar 1: Runtime TS/TSX AI Tool Compiler & Hot-Swap Registry (Bun.Transpiler)", () => {
    test("compiles and executes typed TS tool in-process with latency telemetry", async () => {
      const registry = new RuntimeToolRegistry();
      const source = `
        interface CalcArgs { a: number; b: number; label?: string }
        export async function run(args: CalcArgs): Promise<{ sum: number; tag: string }> {
          return { sum: args.a + args.b, tag: (args.label ?? "fw").toUpperCase() };
        }
      `;

      const compiled = registry.compileAndRegister({
        name: "math_add",
        source,
        loader: "ts",
        sandbox: true,
        description: "Adds two numbers with an uppercase tag",
      });

      expect(compiled.ok).toBe(true);
      expect(compiled.version).toBe(1);
      expect(compiled.blockedImports).toEqual([]);
      expect(compiled.blockedGlobals).toEqual([]);
      expect(compiled.codeHash.length).toBe(64);
      expect(compiled.compileMs).toBeGreaterThanOrEqual(0);

      const exec1 = await registry.execute("math_add", { a: 19, b: 23, label: "kv" });
      expect(exec1.ok).toBe(true);
      expect(exec1.version).toBe(1);
      expect(exec1.output).toEqual({ sum: 42, tag: "KV" });
      expect(exec1.execMs).toBeGreaterThanOrEqual(0);

      const telemetry = registry.getTool("math_add");
      expect(telemetry).not.toBeNull();
      expect(telemetry!.invocations).toBe(1);
      expect(telemetry!.lastError).toBeNull();
    });

    test("compiles and executes TSX agent card component in-process", async () => {
      const registry = new RuntimeToolRegistry();
      const tsxSource = `
        interface Props { node: string; hitRate: number }
        export function run(props: Props) {
          return (
            <section data-node={props.node}>
              <h1>{props.node}</h1>
              <span>{String(props.hitRate)}%</span>
            </section>
          );
        }
      `;

      const compiled = registry.compileAndRegister({
        name: "cluster_card",
        source: tsxSource,
        loader: "tsx",
        sandbox: true,
      });

      expect(compiled.ok).toBe(true);
      const exec = await registry.execute("cluster_card", { node: "local-rtx4070", hitRate: 94 });
      expect(exec.ok).toBe(true);
      const vnode = exec.output as VNode;
      expect(vnode.type).toBe("section");
      expect(vnode.props["data-node"]).toBe("local-rtx4070");
      expect(vnode.children.length).toBe(2);
    });

    test("hot-swaps registered tool in-process and bumps version", async () => {
      const registry = new RuntimeToolRegistry();
      const v1 = registry.compileAndRegister({
        name: "greeter",
        source: `export function run(args: { who: string }) { return "v1:" + args.who; }`,
      });
      expect(v1.ok).toBe(true);
      expect(v1.version).toBe(1);
      expect((await registry.execute("greeter", { who: "fr1" })).output).toBe("v1:fr1");

      const v2 = registry.compileAndRegister({
        name: "greeter",
        source: `export function run(args: { who: string }) { return "v2:" + args.who.toUpperCase(); }`,
      });
      expect(v2.ok).toBe(true);
      expect(v2.version).toBe(2);
      const res2 = await registry.execute("greeter", { who: "fr1" });
      expect(res2.ok).toBe(true);
      expect(res2.version).toBe(2);
      expect(res2.output).toBe("v2:FR1");
      expect(registry.getTool("greeter")!.invocations).toBe(2);
    });

    test("sandbox blocks forbidden fs/net imports and global escapes", () => {
      const registry = new RuntimeToolRegistry();

      const fsEscape = registry.compileAndRegister({
        name: "bad_fs",
        source: `import { readFileSync } from "node:fs"; export function run() { return readFileSync("/etc/passwd", "utf8"); }`,
        sandbox: true,
      });
      expect(fsEscape.ok).toBe(false);
      expect(fsEscape.blockedImports).toContain("node:fs");

      const netEscape = registry.compileAndRegister({
        name: "bad_net",
        source: `import net from "net"; export function run() { return net.connect(80); }`,
        sandbox: true,
      });
      expect(netEscape.ok).toBe(false);
      expect(netEscape.blockedImports).toContain("net");

      const globalEscape = registry.compileAndRegister({
        name: "bad_global",
        source: `export async function run() { return await fetch("https://example.com"); }`,
        sandbox: true,
      });
      expect(globalEscape.ok).toBe(false);
      expect(globalEscape.blockedGlobals).toContain("fetch");
    });
  });

  describe("Pillar 2: Persistent llm-d / vLLM Prefix KV-Cache & Hybrid FTS5/Vector RAG (bun:sqlite)", () => {
    test("16-token block-chained SHA-256 prefix hashing matches shared prefix blocks", () => {
      const p1 = `${SHARED_SYSTEM_PREFIX} Answer with cluster telemetry for fr1 and fr2 nodes right now.`;
      const p2 = `${SHARED_SYSTEM_PREFIX} Summarize KV cache savings for local rtx4070 decode queue today.`;

      const h1 = computeChainedPrefixBlocks(p1, "qwen3-8b", KV_BLOCK_TOKEN_SIZE);
      const h2 = computeChainedPrefixBlocks(p2, "qwen3-8b", KV_BLOCK_TOKEN_SIZE);

      expect(h1.blocks.length).toBeGreaterThanOrEqual(3);
      expect(h2.blocks.length).toBeGreaterThanOrEqual(3);
      expect(h1.blocks[0]!.hash).toBe(h2.blocks[0]!.hash);
      expect(h1.blocks[1]!.hash).toBe(h2.blocks[1]!.hash);
      expect(h1.blocks[h1.blocks.length - 1]!.hash).not.toBe(
        h2.blocks[h2.blocks.length - 1]!.hash,
      );
    });

    test("WAL persistence, prefix-aware P/D placement, hybrid FTS5+vector RAG, and clean Windows close", () => {
      const tmpDir = mkdtempSync(join(tmpdir(), "fw-bun-ai-"));
      const dbPath = join(tmpDir, "kv-rag.sqlite");

      try {
        const store = new PrefixKvRagStore(dbPath);
        expect(store.getStats().journalMode.toLowerCase()).toBe("wal");

        const coldPrompt = `${SHARED_SYSTEM_PREFIX} Compute attention latency on fr1 prefill worker.`;
        const r1 = store.routePrefix({ prompt: coldPrompt, model: "qwen3-8b", disaggregated: true });
        expect(r1.kvCacheHit).toBe(false);
        expect(r1.matchedBlocks).toBe(0);
        expect(r1.savedPrefillTokens).toBe(0);
        expect(r1.prefillNode).not.toBe(r1.decodeNode);
        expect(["fr1", "fr2", "prd", "local-rtx4070"]).toContain(r1.prefillNode);
        expect(["fr1", "fr2", "prd", "local-rtx4070"]).toContain(r1.decodeNode);

        const warmPrompt = `${SHARED_SYSTEM_PREFIX} Stream decode tokens from edge worker with low TTFT.`;
        const r2 = store.routePrefix({ prompt: warmPrompt, model: "qwen3-8b", disaggregated: true });
        expect(r2.kvCacheHit).toBe(true);
        expect(r2.matchedBlocks).toBeGreaterThanOrEqual(2);
        expect(r2.savedPrefillTokens).toBeGreaterThanOrEqual(32);
        expect(r2.prefillNode).toBe(r1.prefillNode);
        expect(r2.ttftReductionMs).toBeGreaterThan(0);
        expect(r2.ttftReductionPct).toBeGreaterThan(0);

        store.upsertDocument({
          docKey: "doc-llmd",
          title: "llm-d Prefix Aware KV Routing",
          content:
            "Disaggregated prefill and decode placement uses 16-token chained prefix block hashes to maximize KV cache affinity and cut TTFT.",
          metadata: { tier: "cluster" },
        });
        store.upsertDocument({
          docKey: "doc-ggml",
          title: "GGML Quantization Guide",
          content:
            "Q4_K_M and Q8_0 GGUF tensors run efficiently on CUDA sm_89 and EPYC AVX-512 backends.",
          metadata: { tier: "compute" },
        });

        const hits = store.hybridSearch("prefix block KV cache TTFT", { limit: 2 });
        expect(hits.length).toBe(2);
        expect(hits[0]!.docKey).toBe("doc-llmd");
        expect(hits[0]!.ftsScore).toBeGreaterThan(0);
        expect(hits[0]!.vectorScore).toBeGreaterThan(0);
        expect(hits[0]!.hybridScore).toBeGreaterThan(hits[1]!.hybridScore);

        const snap = store.exportSnapshot();
        expect(snap.kvBlockCount).toBeGreaterThanOrEqual(2);
        expect(snap.ragDocCount).toBe(2);
        expect(snap.compressedBytes).toBeGreaterThan(0);
        const decompressed = JSON.parse(
          new TextDecoder().decode(Bun.gunzipSync(Buffer.from(snap.payloadBase64, "base64"))),
        ) as { kvBlocks: unknown[]; ragDocs: unknown[] };
        expect(decompressed.kvBlocks.length).toBe(snap.kvBlockCount);
        expect(decompressed.ragDocs.length).toBe(2);

        store.close();

        const reopened = new PrefixKvRagStore(dbPath);
        const r3 = reopened.routePrefix({ prompt: warmPrompt, model: "qwen3-8b", disaggregated: true });
        expect(r3.kvCacheHit).toBe(true);
        expect(r3.savedPrefillTokens).toBeGreaterThanOrEqual(32);
        expect(reopened.getStats().ragDocuments).toBe(2);
        reopened.close();
      } finally {
        rmSync(tmpDir, { recursive: true, force: true });
      }
    });
  });

  describe("Pillar 3: Zero-Copy Streaming SSE / OpenAI / MCP Disaggregated P/D Router", () => {
    test("serves all /v1/ai/* endpoints over live Bun.serve HTTP server", async () => {
      const store = new PrefixKvRagStore(":memory:");
      const tools = new RuntimeToolRegistry();
      const handler = createBunNativeAiHandler({ store, tools });

      store.upsertDocument({
        docKey: "fw-router",
        title: "Fairwind Disaggregated Router",
        content: "Pingora + llm-d prefix cache routing separates prefill on fr1 from decode on fr2 and prd.",
      });

      const server = Bun.serve({
        port: 0,
        fetch: handler,
      });

      try {
        const base = `http://127.0.0.1:${server.port}`;

        const healthRes = await fetch(`${base}/v1/ai/health`);
        expect(healthRes.status).toBe(200);
        const health = (await healthRes.json()) as {
          ok: boolean;
          runtime: string;
          bunVersion: string;
          pillars: string[];
        };
        expect(health.ok).toBe(true);
        expect(health.runtime).toBe("bun");
        expect(health.bunVersion).toBe(Bun.version);
        expect(health.pillars.length).toBe(3);

        const compileOkRes = await fetch(`${base}/v1/ai/tools/compile`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            name: "token_budget",
            loader: "ts",
            sandbox: true,
            execute: true,
            args: { tokens: 512, discount: 0.75 },
            source: `
              export function run(args: { tokens: number; discount: number }) {
                return { effectiveTokens: Math.round(args.tokens * (1 - args.discount)) };
              }
            `,
          }),
        });
        expect(compileOkRes.status).toBe(200);
        const compileOk = (await compileOkRes.json()) as {
          ok: boolean;
          execution: { ok: boolean; output: { effectiveTokens: number } };
        };
        expect(compileOk.ok).toBe(true);
        expect(compileOk.execution.ok).toBe(true);
        expect(compileOk.execution.output.effectiveTokens).toBe(128);

        const compileBlockedRes = await fetch(`${base}/v1/ai/tools/compile`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            name: "escape_attempt",
            loader: "ts",
            sandbox: true,
            source: `import fs from "node:fs"; export function run() { return fs.existsSync("."); }`,
          }),
        });
        expect(compileBlockedRes.status).toBe(403);
        const compileBlocked = (await compileBlockedRes.json()) as {
          ok: boolean;
          blockedImports: string[];
        };
        expect(compileBlocked.ok).toBe(false);
        expect(compileBlocked.blockedImports).toContain("node:fs");

        const route1Res = await fetch(`${base}/v1/ai/kv/route`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            prompt: `${SHARED_SYSTEM_PREFIX} Route first user turn.`,
            ragQuery: "Pingora disaggregated prefill decode",
          }),
        });
        expect(route1Res.status).toBe(200);
        expect(route1Res.headers.get("x-fw-kv-hit")).toBe("false");
        const route1 = (await route1Res.json()) as {
          kvCacheHit: boolean;
          prefillNode: string;
          ragContext: Array<{ docKey: string }>;
        };
        expect(route1.kvCacheHit).toBe(false);
        expect(route1.ragContext[0]?.docKey).toBe("fw-router");

        const route2Res = await fetch(`${base}/v1/ai/kv/route`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            prompt: `${SHARED_SYSTEM_PREFIX} Route second user turn with shared prefix.`,
          }),
        });
        expect(route2Res.status).toBe(200);
        expect(route2Res.headers.get("x-fw-kv-hit")).toBe("true");
        const route2 = (await route2Res.json()) as {
          kvCacheHit: boolean;
          prefillNode: string;
          savedPrefillTokens: number;
        };
        expect(route2.kvCacheHit).toBe(true);
        expect(route2.prefillNode).toBe(route1.prefillNode);
        expect(route2.savedPrefillTokens).toBeGreaterThanOrEqual(32);

        const sseRes = await fetch(`${base}/v1/ai/chat/stream`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            model: "fairwind-qwen3-8b-gguf",
            messages: [
              { role: "system", content: SHARED_SYSTEM_PREFIX },
              { role: "user", content: "Report streaming status." },
            ],
            toolName: "token_budget",
            toolArgs: { tokens: 100, discount: 0.5 },
          }),
        });
        expect(sseRes.status).toBe(200);
        expect(sseRes.headers.get("content-type")).toContain("text/event-stream");
        const sseBody = await sseRes.text();
        const frames = sseBody
          .split("\n\n")
          .map((f) => f.trim())
          .filter((f) => f.startsWith("data: "));
        expect(frames.length).toBeGreaterThanOrEqual(4);
        expect(frames[frames.length - 1]).toBe("data: [DONE]");
        expect(sseBody).toContain('"object":"chat.completion.chunk"');
        expect(sseBody).toContain("token_budget");

        const snapJsonRes = await fetch(`${base}/v1/ai/snapshot`, { method: "POST" });
        expect(snapJsonRes.status).toBe(200);
        const snapJson = (await snapJsonRes.json()) as {
          ok: boolean;
          format: string;
          kvBlockCount: number;
          ragDocCount: number;
          payloadBase64: string;
        };
        expect(snapJson.ok).toBe(true);
        expect(snapJson.format).toBe("fairwind-kv-rag-v1+gzip");
        expect(snapJson.kvBlockCount).toBeGreaterThanOrEqual(2);
        expect(snapJson.ragDocCount).toBe(1);

        const snapRawRes = await fetch(`${base}/v1/ai/snapshot?raw=1`, {
          method: "POST",
          headers: { accept: "application/gzip" },
        });
        expect(snapRawRes.status).toBe(200);
        expect(snapRawRes.headers.get("content-type")).toBe("application/gzip");
        const rawBytes = new Uint8Array(await snapRawRes.arrayBuffer());
        expect(snapRawRes.headers.get("x-fw-snapshot-sha256")?.length).toBe(64);
        expect(rawBytes.byteLength).toBeGreaterThan(0);

        const mcpListRes = await fetch(`${base}/v1/ai/mcp`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list" }),
        });
        expect(mcpListRes.status).toBe(200);
        const mcpList = (await mcpListRes.json()) as {
          result: { tools: Array<{ name: string }> };
        };
        expect(mcpList.result.tools.some((t) => t.name === "token_budget")).toBe(true);

        const mcpCallRes = await fetch(`${base}/v1/ai/mcp`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            jsonrpc: "2.0",
            id: 2,
            method: "tools/call",
            params: { name: "token_budget", arguments: { tokens: 200, discount: 0.25 } },
          }),
        });
        expect(mcpCallRes.status).toBe(200);
        const mcpCall = (await mcpCallRes.json()) as {
          result: { ok: boolean; output: { effectiveTokens: number } };
        };
        expect(mcpCall.result.ok).toBe(true);
        expect(mcpCall.result.output.effectiveTokens).toBe(150);
      } finally {
        server.stop(true);
        store.close();
      }
    });
  });

  describe("Phase 2: OpenClaw `openclaw-fw` Plugin & `migrateOpenClawToFw` Engine", () => {
    test("openclaw.plugin.json manifest and Bun-native plugin tools (ls, grep, find, bench, claw, mcp) work end-to-end", async () => {
      const manifestPath = join(ROOT, "plugins", "openclaw-fw", "openclaw.plugin.json");
      expect(existsSync(manifestPath)).toBe(true);
      const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as {
        id: string;
        runtime: string;
        tools: Array<{ name: string }>;
      };
      expect(manifest.id).toBe("openclaw-fw");
      expect(manifest.runtime).toBe("bun");
      expect(manifest.tools.map((t) => t.name)).toEqual([
        "fw_ls",
        "fw_grep",
        "fw_find",
        "fw_bench",
        "fw_claw",
        "fw_mcp",
      ]);

      const registered: OpenClawToolRegistration[] = [];
      const registeredInstance = openclawFwPlugin.register({
        config: { cwd: join(ROOT, "plugins", "openclaw-fw"), sandbox: true },
        registerTool: (t) => registered.push(t),
      });
      expect(registered.length).toBe(6);
      registeredInstance.close();

      const plugin = createOpenClawFwPlugin({ cwd: join(ROOT, "plugins", "openclaw-fw"), sandbox: true });
      try {
        const lsRes = (await plugin.tools.fw_ls!.execute({})) as {
          ok: boolean;
          entries: Array<{ name: string }>;
        };
        expect(lsRes.ok).toBe(true);
        expect(lsRes.entries.some((e) => e.name === "openclaw.plugin.json")).toBe(true);

        const findRes = (await plugin.tools.fw_find!.execute({ pattern: "*.ts" })) as {
          ok: boolean;
          files: string[];
        };
        expect(findRes.ok).toBe(true);
        expect(findRes.files).toContain("index.ts");

        const grepRes = (await plugin.tools.fw_grep!.execute({ pattern: "createOpenClawFwPlugin" })) as {
          ok: boolean;
          count: number;
        };
        expect(grepRes.ok).toBe(true);
        expect(grepRes.count).toBeGreaterThanOrEqual(1);

        const benchRes = (await plugin.tools.fw_bench!.execute({ iterations: 5, workSize: 256 })) as {
          ok: boolean;
          iterations: number;
          avgMs: number;
        };
        expect(benchRes.ok).toBe(true);
        expect(benchRes.iterations).toBe(5);
        expect(benchRes.avgMs).toBeGreaterThanOrEqual(0);

        const clawAllowed = (await plugin.tools.fw_claw!.execute({
          action: "sandbox-check",
          binary: "rg",
          binaryArgs: ["-n", "fairwind"],
        })) as { ok: boolean };
        expect(clawAllowed.ok).toBe(true);

        const clawDenied = (await plugin.tools.fw_claw!.execute({
          action: "sandbox-check",
          binary: "rm",
          binaryArgs: ["-rf", "/"],
        })) as { ok: boolean };
        expect(clawDenied.ok).toBe(false);

        const mcpInit = await plugin.handleMcpRequest({ jsonrpc: "2.0", id: 1, method: "initialize" });
        expect("result" in mcpInit && mcpInit.result.serverInfo.name).toBe("openclaw-fw");

        const mcpList = await plugin.handleMcpRequest({ jsonrpc: "2.0", id: 2, method: "tools/list" });
        expect("result" in mcpList && mcpList.result.tools.length).toBe(6);

        const mcpCall = await plugin.handleMcpRequest({
          jsonrpc: "2.0",
          id: 3,
          method: "tools/call",
          params: { name: "fw_claw", arguments: { action: "status" } },
        });
        expect("result" in mcpCall && (mcpCall.result.structuredContent as { ok: boolean }).ok).toBe(true);
      } finally {
        plugin.close();
      }
    });

    test("migrateOpenClawToFw converts channels, skills, MCP servers, and gateway in dry-run and apply modes", () => {
      const tmp = mkdtempSync(join(tmpdir(), "fw-openclaw-mig-"));
      const openclawDir = join(tmp, ".openclaw");
      const outDir = join(tmp, ".fairwind");

      try {
        mkdirSync(join(openclawDir, "skills", "custom-skill"), { recursive: true });
        writeFileSync(join(openclawDir, "skills", "custom-skill", "SKILL.md"), "# Custom Skill\n", "utf8");
        writeFileSync(
          join(openclawDir, "openclaw.json"),
          JSON.stringify(
            {
              gateway: { port: 19001, bind: "127.0.0.1" },
              agents: { defaults: { model: { primary: "anthropic/claude-opus-4-6" } } },
              channels: {
                telegram: { enabled: true, botToken: "redacted" },
                a2a: { enabled: true },
                cron: { enabled: true },
              },
              skills: {
                entries: {
                  "custom-skill": { enabled: true },
                },
              },
              mcpServers: {
                filesystem: {
                  command: "npx",
                  args: ["-y", "@modelcontextprotocol/server-filesystem", "."],
                  env: { LOG_LEVEL: "info", API_TOKEN: "secret-value" },
                },
                remote: {
                  url: "https://mcp.example.com/sse",
                  transport: "sse",
                },
              },
            },
            null,
            2,
          ),
          "utf8",
        );

        // 1. Dry-run writes zero files
        const dry = migrateOpenClawToFw({ openclawDir, outputDir: outDir, dryRun: true });
        expect(dry.ok).toBe(true);
        expect(dry.mode).toBe("dry-run");
        expect(dry.configFound).toBe(true);
        expect(dry.writtenFiles).toEqual([]);
        expect(existsSync(outDir)).toBe(false);
        expect(dry.gateway.port).toBe(19001);
        expect(dry.gateway.defaultModel).toBe("anthropic/claude-opus-4-6");
        expect(dry.channels.find((c) => c.sourceName === "a2a")?.fwChannelKind).toBe("Acp");
        expect(dry.channels.find((c) => c.sourceName === "telegram")?.adapter).toBe("http-webhook-bridge");
        expect(dry.parityGaps.some((g) => g.includes("telegram"))).toBe(true);
        expect(dry.skills.find((s) => s.name === "custom-skill")?.hasSkillMd).toBe(true);
        const fsMcp = dry.mcpServers.find((m) => m.name === "filesystem");
        expect(fsMcp?.command).toBe("bunx");
        expect(fsMcp?.bunRewritten).toBe(true);
        expect(fsMcp?.redactedSecretKeys).toContain("API_TOKEN");
        expect(dry.bunRuntimePatchNotes.length).toBe(OPENCLAW_BUN_RUNTIME_PATCH_NOTES.length);

        // 2. Apply mode writes all 5 converted config/report files
        const applied = migrateOpenClawToFw({ openclawDir, outputDir: outDir, apply: true });
        expect(applied.ok).toBe(true);
        expect(applied.mode).toBe("apply");
        expect(applied.writtenFiles.length).toBe(5);
        for (const f of applied.writtenFiles) {
          expect(existsSync(f)).toBe(true);
        }
        const writtenCfg = JSON.parse(readFileSync(join(outDir, "fw.config.json"), "utf8")) as {
          schema: string;
          plugin: string;
        };
        expect(writtenCfg.schema).toBe("fairwind/v4.2");
        expect(writtenCfg.plugin).toBe("openclaw-fw");
      } finally {
        rmSync(tmp, { recursive: true, force: true });
      }
    });
  });
});
