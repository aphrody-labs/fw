#!/usr/bin/env bun
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import {
  brainIndexRepo,
  brainSearch,
  DEFAULT_ROUTER_PROVIDERS,
  loadSoul,
  routeModelRequest,
} from "./ai_stack.ts";
import {
  installService,
  renderCrossPlatformServices,
  runDaemonCycle,
  serviceStatus,
  uninstallService,
} from "./daemon.ts";
import { LOW_LEVEL_LTO_AOT_MATRIX } from "./lowlevel.ts";
import { auditDeduplication, pluginStatus, syncFairwindPlugin } from "./plugin_sync.ts";
import { generateFairwindWtaConfig, getTerminalTuiStatus, verifyFairwindSelfTui } from "./terminal_tui.ts";
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
  resolveUpstreamAgyPath,
  writeTargetManifest,
} from "./targets.ts";
import { applyUpgrade, buildAgyUpgradePlan, checkUpgrade, runBenchmarks } from "./upgrade.ts";
import { listVfsMounts, vfsList, vfsRead, vfsWrite } from "./vfs.ts";

export interface JsonRpcRequest {
  jsonrpc?: string;
  id?: string | number | null;
  method?: string;
  params?: Record<string, unknown>;
}

export interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: string | number | null;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

export const FAIRWIND_MCP_TOOLS = [
  {
    name: "fairwind_status",
    description:
      "Return unified Fairwind monorepo status, Fat-LTO Rust C ABI v3 health, absorbed first/third-party modules, and target/agy readiness.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "fairwind_ast_parse",
    description:
      "Parse a source file using Fairwind's Fat-LTO Rust Tree-sitter C ABI across 9 grammars (rust, typescript, tsx, python, c, cpp, java, json, bash).",
    inputSchema: {
      type: "object",
      properties: {
        lang: { type: "string", description: "Grammar name (rust, typescript, tsx, python, c, cpp, java, json, bash)." },
        file: { type: "string", description: "Relative or absolute path to source file." },
      },
      required: ["lang", "file"],
    },
  },
  {
    name: "fairwind_pe_inspect",
    description:
      "Inspect a Windows PE/COFF binary (defaults to read-only signed upstream agy.exe) using craft-pe compiled into Fairwind's Fat-LTO ABI.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Optional binary path; defaults to upstream agy.exe." },
      },
    },
  },
  {
    name: "fairwind_target_scan",
    description:
      "Scan all registered in-repo Fairwind modules (heretic, neon, craft-libs, craft-anything, native/*, src, tests), run AST probes, and update target/manifest.json.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "fairwind_upgrade",
    description:
      "Check or apply Fairwind's gated LTO/AOT self-upgrade pipeline across target/agy/ and dist/, or produce an agy headless upgrade plan.",
    inputSchema: {
      type: "object",
      properties: {
        mode: {
          type: "string",
          enum: ["check", "apply", "agy_plan"],
          description: "Upgrade mode (default: check).",
        },
        skipTests: { type: "boolean", description: "Skip full test suite during apply (default: false)." },
      },
    },
  },
  {
    name: "fairwind_bench",
    description:
      "Benchmark binary size and cold-start latency for target/agy/agy (Rust Fat-LTO), dist/fairwind (Bun AOT+trim), and upstream agy.exe.",
    inputSchema: {
      type: "object",
      properties: {
        iterations: { type: "integer", minimum: 1, maximum: 25, description: "Runs per binary (default: 5)." },
      },
    },
  },
  {
    name: "fairwind_learn",
    description:
      "Run Fairwind-integrated autonomous learning operations via ~/.gemini/scripts/learn.ts (search, distill, cloud-sync, status).",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["search", "distill", "cloud-sync", "status"], description: "Learning action." },
        query: { type: "string", description: "Search query when action is 'search'." },
      },
      required: ["action"],
    },
  },
  {
    name: "fairwind_service",
    description:
      "Manage Fairwind's persistent, boot-startup, auto-restart cross-platform continuous agy improvement daemon and plugin sync.",
    inputSchema: {
      type: "object",
      properties: {
        action: {
          type: "string",
          enum: ["status", "render", "install", "uninstall", "run_once", "plugin_sync", "plugin_status"],
          description: "Service or plugin management action.",
        },
        autoApply: { type: "boolean", description: "For run_once: auto-apply upgrade when drift is detected." },
      },
      required: ["action"],
    },
  },
  {
    name: "fairwind_bridge",
    description:
      "Inspect or dispatch to Fairwind's absorbed first/third-party subsystems (heretic, neon/REA, craft-libs, craft-anything, ghidra).",
    inputSchema: {
      type: "object",
      properties: {
        subsystem: { type: "string", enum: ["heretic", "neon", "craft", "ghidra"], description: "Subsystem name." },
      },
      required: ["subsystem"],
    },
  },
  {
    name: "fairwind_disasm",
    description:
      "Disassemble PE entry-point x86-64 instructions using craft-pe + iced-x86 Fat-LTO ABI, or identify any file via craft-sniff.",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["disasm", "identify"], description: "Operation mode." },
        path: { type: "string", description: "Optional binary/file path (defaults to upstream agy.exe)." },
        maxInsns: { type: "integer", minimum: 1, maximum: 128, description: "Instruction limit." },
      },
    },
  },
  {
    name: "fairwind_vfs",
    description: "Interact with Fairwind SQLite CAS VFS OS (mounts, ls, read, write).",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["mounts", "ls", "read", "write"] },
        path: { type: "string" },
        content: { type: "string" },
      },
      required: ["action"],
    },
  },
  {
    name: "fairwind_ai",
    description: "Query Fairwind AI Brain (FTS5), Soul constitution, Router providers, or Low-Level LTO/AOT matrix.",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["brain_index", "brain_search", "soul", "router", "lowlevel_matrix", "dedupe"] },
        query: { type: "string" },
        model: { type: "string" },
      },
      required: ["action"],
    },
  },
  {
    name: "fairwind_tui_terminal",
    description: "Inspect and run Fairwind-optimized Microsoft Intelligent Terminal (WTA) and tui-test source-only workflows.",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["status", "wta_config", "verify_self"] },
      },
      required: ["action"],
    },
  },
] as const;

function textResult(
  payload: unknown,
  isError = false,
): { content: Array<{ type: "text"; text: string }>; isError?: boolean } {
  const text = typeof payload === "string" ? payload : JSON.stringify(payload, null, 2);
  return isError ? { content: [{ type: "text", text }], isError: true } : { content: [{ type: "text", text }] };
}

export function runLearnBridge(action: string, query?: string) {
  const learnScript = join(homedir(), ".gemini", "scripts", "learn.ts");
  if (!existsSync(learnScript)) {
    return { ok: false, error: `learn.ts not found at ${learnScript}` };
  }
  const cmd =
    action === "search"
      ? ["bun", learnScript, "search", ...(query ? query.trim().split(/\s+/) : ["fairwind"])]
      : action === "distill"
        ? ["bun", learnScript, "distill-quiet"]
        : action === "cloud-sync"
          ? ["bun", learnScript, "cloud-sync"]
          : ["bun", learnScript, "status"];
  const proc = Bun.spawnSync(cmd, { stdout: "pipe", stderr: "pipe" });
  const stdout = proc.stdout.toString().trim();
  const stderr = proc.stderr.toString().trim();
  let parsed: unknown = stdout;
  try {
    if (stdout.startsWith("{") || stdout.startsWith("[")) parsed = JSON.parse(stdout);
  } catch {
    // Keep raw text output
  }
  return {
    ok: proc.exitCode === 0,
    action,
    exitCode: proc.exitCode ?? 1,
    output: parsed,
    stderr: stderr || undefined,
  };
}

export function handleFairwindToolCall(
  root: string,
  name: string,
  args: Record<string, unknown> = {},
): { content: Array<{ type: "text"; text: string }>; isError?: boolean } {
  switch (name) {
    case "fairwind_status": {
      const abi = probeRustAbi(root);
      const manifest = readTargetManifest(root);
      const isWin = process.platform === "win32";
      const agyTarget = join(root, "target", "agy", isWin ? "agy.exe" : "agy");
      return textResult({
        ok: true,
        product: "fairwind",
        root,
        abiLoaded: abi.loaded,
        abi,
        agyTargetReady: existsSync(agyTarget),
        manifestReady: Boolean(manifest),
        repoDigest: manifest?.repoDigest ?? null,
        plugin: pluginStatus(root),
      });
    }
    case "fairwind_ast_parse": {
      const lang = typeof args.lang === "string" ? args.lang : "";
      const file = typeof args.file === "string" ? args.file : "";
      const mode = typeof args.mode === "string" ? args.mode : "summary";
      const query = typeof args.query === "string" ? args.query : "";
      if (!lang || !file) return textResult({ ok: false, error: "lang and file are required" }, true);
      const abs = resolve(root, file);
      const res =
        mode === "symbols"
          ? astSymbolsWithAbi(root, lang, abs)
          : mode === "query"
            ? astQueryWithAbi(root, lang, abs, query)
            : parseAstWithAbi(root, lang, abs);
      return textResult(res, !res.ok);
    }
    case "fairwind_pe_inspect": {
      const target = typeof args.path === "string" && args.path.trim() ? resolve(root, args.path) : resolveUpstreamAgyPath();
      if (!target) return textResult({ ok: false, error: "no PE binary path resolved" }, true);
      const res = inspectPeWithAbi(root, target);
      return textResult(res, !res.ok);
    }
    case "fairwind_target_scan": {
      const manifest = buildTargetManifest(root);
      const manifestPath = writeTargetManifest(root, manifest);
      return textResult({ ok: true, manifestPath, manifest });
    }
    case "fairwind_upgrade": {
      const mode = typeof args.mode === "string" ? args.mode : "check";
      if (mode === "apply") {
        const res = applyUpgrade(root, { skipTests: Boolean(args.skipTests) });
        return textResult(res, !res.ok);
      }
      if (mode === "agy_plan") {
        return textResult(buildAgyUpgradePlan(root));
      }
      return textResult(checkUpgrade(root));
    }
    case "fairwind_bench": {
      const iterations = typeof args.iterations === "number" ? Math.max(1, Math.min(25, Math.trunc(args.iterations))) : 5;
      return textResult(runBenchmarks(root, iterations));
    }
    case "fairwind_learn": {
      const action = typeof args.action === "string" ? args.action : "status";
      const query = typeof args.query === "string" ? args.query : undefined;
      const res = runLearnBridge(action, query);
      return textResult(res, !res.ok);
    }
    case "fairwind_service": {
      const action = typeof args.action === "string" ? args.action : "status";
      switch (action) {
        case "render":
          return textResult(renderCrossPlatformServices(root));
        case "install": {
          const res = installService(root);
          return textResult(res, !res.ok);
        }
        case "uninstall": {
          const res = uninstallService(root);
          return textResult(res, !res.ok);
        }
        case "run_once": {
          const res = runDaemonCycle(root, { autoApply: Boolean(args.autoApply), withLearn: true, syncPlugin: true });
          return textResult(res, !res.ok);
        }
        case "plugin_sync": {
          const res = syncFairwindPlugin(root);
          return textResult(res, !res.ok);
        }
        case "plugin_status":
          return textResult(pluginStatus(root));
        default:
          return textResult(serviceStatus(root));
      }
    }
    case "fairwind_bridge": {
      const sub = typeof args.subsystem === "string" ? args.subsystem : "";
      const map: Record<string, { path: string; runtime: string }> = {
        heretic: { path: join(root, "core", "py", "heretic"), runtime: "uv/python" },
        neon: { path: join(root, "core", "ts", "neon"), runtime: "bun/ts-rea" },
        craft: { path: join(root, "core", "rs", "craft-anything"), runtime: "rust/craft-libs+craft-anything" },
        ghidra: { path: join(root, "packages", "ghidra"), runtime: "jvm/gradle-headless" },
      };
      const entry = map[sub];
      if (!entry) return textResult({ ok: false, error: `Unknown subsystem: ${sub}` }, true);
      return textResult({
        ok: true,
        subsystem: sub,
        path: entry.path,
        present: existsSync(entry.path),
        runtime: entry.runtime,
        abi: sub === "craft" ? probeRustAbi(root) : undefined,
      });
    }
    case "fairwind_disasm": {
      const action = typeof args.action === "string" ? args.action : "disasm";
      const target =
        typeof args.path === "string" && args.path.trim() ? resolve(root, args.path) : resolveUpstreamAgyPath();
      if (!target) return textResult({ ok: false, error: "no target binary resolved" }, true);
      if (action === "identify") {
        const res = identifyFileWithAbi(root, target);
        return textResult(res, !res.ok);
      }
      const max = typeof args.maxInsns === "number" ? args.maxInsns : 12;
      const res = disasmX64WithAbi(root, target, max);
      return textResult(res, !res.ok);
    }
    case "fairwind_vfs": {
      const action = typeof args.action === "string" ? args.action : "mounts";
      const p = typeof args.path === "string" ? args.path : "/";
      if (action === "ls") return textResult(vfsList(root, p));
      if (action === "read") {
        const r = vfsRead(root, p);
        return textResult(r, !r.ok);
      }
      if (action === "write") return textResult(vfsWrite(root, p, typeof args.content === "string" ? args.content : ""));
      return textResult(listVfsMounts(root));
    }
    case "fairwind_ai": {
      const action = typeof args.action === "string" ? args.action : "soul";
      if (action === "brain_index") return textResult(brainIndexRepo(root));
      if (action === "brain_search") return textResult(brainSearch(root, typeof args.query === "string" ? args.query : "fairwind"));
      if (action === "router") {
        return textResult({
          ok: true,
          providers: DEFAULT_ROUTER_PROVIDERS,
          decision: routeModelRequest(typeof args.model === "string" ? args.model : undefined),
        });
      }
      if (action === "lowlevel_matrix") return textResult({ ok: true, matrix: LOW_LEVEL_LTO_AOT_MATRIX });
      if (action === "dedupe") return textResult(auditDeduplication(root));
      return textResult({ ok: true, soul: loadSoul(root) });
    }
    case "fairwind_tui_terminal": {
      const action = typeof args.action === "string" ? args.action : "status";
      if (action === "wta_config") return textResult(generateFairwindWtaConfig(root));
      if (action === "verify_self") {
        const res = verifyFairwindSelfTui(root);
        return textResult(res, !res.ok);
      }
      return textResult(getTerminalTuiStatus(root));
    }
    default:
      return textResult({ ok: false, error: `Unknown tool: ${name}` }, true);
  }
}

export function handleJsonRpcMessage(root: string, req: JsonRpcRequest): JsonRpcResponse | null {
  const id = req.id ?? null;
  const method = req.method ?? "";
  if (method === "notifications/initialized" || method === "notifications/cancelled") {
    return null;
  }
  if (method === "initialize") {
    return {
      jsonrpc: "2.0",
      id,
      result: {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: { name: "fairwind", version: "2.3.0" },
      },
    };
  }
  if (method === "ping") {
    return { jsonrpc: "2.0", id, result: {} };
  }
  if (method === "tools/list") {
    return { jsonrpc: "2.0", id, result: { tools: FAIRWIND_MCP_TOOLS } };
  }
  if (method === "tools/call") {
    const params = req.params ?? {};
    const toolName = typeof params.name === "string" ? params.name : "";
    const toolArgs = (params.arguments && typeof params.arguments === "object"
      ? params.arguments
      : {}) as Record<string, unknown>;
    return {
      jsonrpc: "2.0",
      id,
      result: handleFairwindToolCall(root, toolName, toolArgs),
    };
  }
  return {
    jsonrpc: "2.0",
    id,
    error: { code: -32601, message: `Method not found: ${method}` },
  };
}

export async function startMcpStdioServer(root: string): Promise<void> {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of Bun.stdin.stream()) {
    buffer += decoder.decode(chunk, { stream: true });
    while (true) {
      const nl = buffer.indexOf("\n");
      if (nl === -1) break;
      const line = buffer.slice(0, nl).trim();
      buffer = buffer.slice(nl + 1);
      if (!line) continue;
      try {
        const req = JSON.parse(line) as JsonRpcRequest;
        const resp = handleJsonRpcMessage(root, req);
        if (resp) {
          process.stdout.write(`${JSON.stringify(resp)}\n`);
        }
      } catch (err) {
        const errResp: JsonRpcResponse = {
          jsonrpc: "2.0",
          id: null,
          error: { code: -32700, message: err instanceof Error ? err.message : String(err) },
        };
        process.stdout.write(`${JSON.stringify(errResp)}\n`);
      }
    }
  }
}
