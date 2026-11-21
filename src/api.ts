import { resolve } from "node:path";
import pkg from "../package.json" with { type: "json" };
import {
  brainIndexRepo,
  brainSearch,
  buildA2AAgentCard,
  buildOpenApiSpec,
  DEFAULT_ROUTER_PROVIDERS,
  FwProvider,
  handleA2AMessage,
  handleAcpMessage,
  handleOpenAiChatCompletion,
  loadSoul,
  routeModelRequest,
} from "./ai_stack.ts";
import {
  evaluateDecisions,
  runSelfHostedAiWorkflow,
  type DecisionsRequest,
} from "./decisions.ts";
import { serviceStatus } from "./daemon.ts";
import { LOW_LEVEL_LTO_AOT_MATRIX } from "./lowlevel.ts";
import { handleJsonRpcMessage, type JsonRpcRequest, runLearnBridge } from "./mcp_server.ts";
import { auditDeduplication, pluginStatus } from "./plugin_sync.ts";
import {
  astQueryWithAbi,
  astSymbolsWithAbi,
  buildTargetManifest,
  disasmX64WithAbi,
  identifyFileWithAbi,
  inspectPeWithAbi,
  parseAstWithAbi,
  probeRustAbi,
  readTargetManifest,
} from "./targets.ts";
import { checkUpgrade, runBenchmarks } from "./upgrade.ts";
import { listVfsMounts, vfsList, vfsRead, vfsSnapshots, vfsWrite } from "./vfs.ts";
import { generateFwWtaConfig, getTerminalTuiStatus, verifyFwSelfTui } from "./terminal_tui.ts";
import { readFwWebAsset } from "./web_ui.ts";

export function createFwApiHandler(root: string) {
  const provider = new FwProvider(root);

  return async (req: Request): Promise<Response> => {
    const url = new URL(req.url);
    const json = (data: unknown, status = 200) =>
      new Response(JSON.stringify(data, null, 2), {
        status,
        headers: {
          "content-type": "application/json; charset=utf-8",
          "access-control-allow-origin": "*",
        },
      });

    if (req.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: {
          "access-control-allow-origin": "*",
          "access-control-allow-methods": "GET, POST, OPTIONS",
          "access-control-allow-headers": "Content-Type, Authorization",
        },
      });
    }

    const webAsset = readFwWebAsset(url.pathname);
    if (webAsset) {
      return new Response(webAsset.body, {
        status: 200,
        headers: {
          "content-type": webAsset.contentType,
          "x-content-type-options": "nosniff",
          "referrer-policy": "strict-origin-when-cross-origin",
        },
      });
    }
    if (url.pathname === "/health") {
      const abi = probeRustAbi(root);
      return json({ ok: true, product: "fw", version: pkg.version, abiLoaded: abi.loaded, abiVersion: abi.version });
    }
    if (url.pathname === "/openapi.json") {
      return json(buildOpenApiSpec(url.origin));
    }
    if (url.pathname === "/.well-known/agent.json") {
      return json(buildA2AAgentCard(url.origin));
    }
    if (url.pathname === "/status") {
      const abi = probeRustAbi(root);
      const manifest = readTargetManifest(root);
      return json({
        ok: true,
        product: "fw",
        version: "2.4.0",
        root,
        abi,
        manifestReady: Boolean(manifest),
        repoDigest: manifest?.repoDigest ?? null,
        plugin: pluginStatus(root),
        dedupe: auditDeduplication(root),
        service: serviceStatus(root),
      });
    }
    if (url.pathname === "/targets") {
      const existing = readTargetManifest(root);
      return json({ ok: true, cached: Boolean(existing), manifest: existing ?? buildTargetManifest(root) });
    }
    if (url.pathname === "/inspect") {
      const p = url.searchParams.get("path");
      if (!p) return json({ ok: false, error: "path query parameter required" }, 400);
      const target = resolve(root, p);
      const report = inspectPeWithAbi(root, target);
      return json(report, report.ok ? 200 : 400);
    }
    if (url.pathname === "/identify") {
      const p = url.searchParams.get("path") ?? "fw.targets.json";
      const report = identifyFileWithAbi(root, resolve(root, p));
      return json(report, report.ok ? 200 : 400);
    }
    if (url.pathname === "/disasm") {
      const p = url.searchParams.get("path");
      if (!p) return json({ ok: false, error: "path query parameter required" }, 400);
      const target = resolve(root, p);
      const max = Math.max(1, Math.min(64, Number(url.searchParams.get("max")) || 12));
      const report = disasmX64WithAbi(root, target, max);
      return json(report, report.ok ? 200 : 400);
    }
    if (url.pathname === "/ast") {
      const lang = url.searchParams.get("lang") ?? "";
      const file = url.searchParams.get("file") ?? "";
      const mode = url.searchParams.get("mode") ?? "summary";
      const query = url.searchParams.get("query") ?? "";
      if (!lang || !file) return json({ ok: false, error: "lang and file query params are required" }, 400);
      const abs = resolve(root, file);
      const res =
        mode === "symbols"
          ? astSymbolsWithAbi(root, lang, abs)
          : mode === "query"
            ? astQueryWithAbi(root, lang, abs, query)
            : parseAstWithAbi(root, lang, abs);
      return json(res, res.ok ? 200 : 400);
    }
    if (url.pathname === "/v1/models") {
      return json(provider.listModels());
    }
    if (url.pathname === "/v1/chat/completions" && req.method === "POST") {
      const body = (await req.json().catch(() => ({}))) as {
        model?: string;
        messages?: Array<{ role: string; content: string }>;
      };
      return json(handleOpenAiChatCompletion(root, body));
    }
    if (url.pathname === "/v1/embeddings" && req.method === "POST") {
      const body = (await req.json().catch(() => ({}))) as { input?: string; model?: string };
      const text = String(body.input ?? "");
      const vec = Array.from({ length: 8 }, (_, i) => Number((((text.charCodeAt(i % Math.max(1, text.length)) || 0) % 97) / 97).toFixed(4)));
      return json({
        object: "list",
        model: body.model ?? "fw-embed-local",
        data: [{ object: "embedding", index: 0, embedding: vec }],
      });
    }
    if ((url.pathname === "/v1beta/decisions" || url.pathname === "/decisions") && req.method === "POST") {
      const body = (await req.json().catch(() => ({}))) as DecisionsRequest;
      return json(await evaluateDecisions(body));
    }
    if (url.pathname === "/workflow" && req.method === "POST") {
      const body = (await req.json().catch(() => ({}))) as { input?: string; mode?: "local" | "auto" };
      return json(await runSelfHostedAiWorkflow(root, String(body.input ?? ""), body.mode ?? "local"));
    }
    if (url.pathname === "/mcp" && req.method === "POST") {
      const rpc = (await req.json().catch(() => ({}))) as JsonRpcRequest;
      const resp = handleJsonRpcMessage(root, rpc);
      return json(resp ?? { jsonrpc: "2.0", id: rpc.id ?? null, result: {} });
    }
    if (url.pathname === "/acp" && req.method === "POST") {
      const rpc = (await req.json().catch(() => ({}))) as JsonRpcRequest;
      return json(handleAcpMessage(root, rpc));
    }
    if (url.pathname === "/a2a" && req.method === "POST") {
      const rpc = (await req.json().catch(() => ({}))) as JsonRpcRequest;
      return json(handleA2AMessage(root, rpc));
    }
    if (url.pathname === "/vfs/mounts") return json(listVfsMounts(root));
    if (url.pathname === "/vfs/ls") return json(vfsList(root, url.searchParams.get("path") ?? "/"));
    if (url.pathname === "/vfs/read") {
      const res = vfsRead(root, url.searchParams.get("path") ?? "/");
      return json(res, res.ok ? 200 : 404);
    }
    if (url.pathname === "/vfs/write" && req.method === "POST") {
      const body = (await req.json().catch(() => ({}))) as { path?: string; content?: string };
      if (!body.path) return json({ ok: false, error: "path required" }, 400);
      return json(vfsWrite(root, body.path, body.content ?? ""));
    }
    if (url.pathname === "/vfs/snapshots") return json(vfsSnapshots(root));
    if (url.pathname === "/brain/index") return json(brainIndexRepo(root));
    if (url.pathname === "/brain/search") return json(brainSearch(root, url.searchParams.get("q") ?? "fw"));
    if (url.pathname === "/soul") return json({ ok: true, soul: loadSoul(root) });
    if (url.pathname === "/router") {
      return json({
        ok: true,
        providers: DEFAULT_ROUTER_PROVIDERS,
        decision: routeModelRequest(url.searchParams.get("model") ?? undefined, url.searchParams.get("cap") ?? "chat"),
      });
    }
    if (url.pathname === "/lowlevel/matrix") return json({ ok: true, matrix: LOW_LEVEL_LTO_AOT_MATRIX });
    if (url.pathname === "/upgrade/check") return json(checkUpgrade(root));
    if (url.pathname === "/bench") {
      const iters = Math.max(1, Math.min(10, Number(url.searchParams.get("iterations")) || 3));
      return json(runBenchmarks(root, iters));
    }
    if (url.pathname === "/learn/search") return json(runLearnBridge("search", url.searchParams.get("q") ?? "fw"));
    if (url.pathname === "/daemon/status") return json(serviceStatus(root));
    if (url.pathname === "/api/tui/status") return json(getTerminalTuiStatus(root));
    if (url.pathname === "/api/tui/wta-config") return json(generateFwWtaConfig(root));
    if (url.pathname === "/api/tui/verify") return json(verifyFwSelfTui(root));

    return json(
      {
        ok: false,
        error: "Not found",
        endpoints: [
          "/",
          "/health",
          "/status",
          "/openapi.json",
          "/.well-known/agent.json",
          "/v1/models",
          "/v1/chat/completions",
          "/v1/embeddings",
          "/mcp",
          "/acp",
          "/a2a",
          "/vfs/mounts",
          "/vfs/ls",
          "/vfs/read",
          "/vfs/write",
          "/brain/index",
          "/brain/search",
          "/soul",
          "/router",
          "/lowlevel/matrix",
          "/inspect",
          "/identify",
          "/disasm",
          "/ast",
          "/upgrade/check",
          "/bench",
          "/daemon/status",
        ],
      },
      404,
    );
  };
}

export function startFwApiServer(root: string, host = "127.0.0.1", port = 4379) {
  return Bun.serve({
    hostname: host,
    port,
    fetch: createFwApiHandler(root),
  });
}
