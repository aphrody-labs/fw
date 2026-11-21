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
import { callCratesTool, CRATES_TOOL_NAMES, CRATES_TOOLS } from "./crates.ts";
import { LOW_LEVEL_LTO_AOT_MATRIX } from "./lowlevel.ts";
import { auditDeduplication, pluginStatus } from "./plugin_sync.ts";
import { generateFwWtaConfig, getTerminalTuiStatus, verifyFwSelfTui } from "./terminal_tui.ts";
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
  writeTargetManifest,
} from "./targets.ts";
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

export const FW_MCP_TOOLS = [
  {
    name: "fw_status",
    description:
      "Return Fw monorepo status, Rust C ABI (v4) health, absorbed modules and plugin status.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "fw_ast_parse",
    description:
      "Parse a source file using Fw's Fat-LTO Rust Tree-sitter C ABI across 12 grammars (rust, typescript, tsx, javascript, python, c, cpp, java, kotlin, go, json, bash).",
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
    name: "fw_pe_inspect",
    description:
      "Inspect a Windows PE/COFF binary using craft-pe compiled into Fw's Fat-LTO ABI.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string", description: "Binary path." },
      },
      required: ["path"],
    },
  },
  {
    name: "fw_target_scan",
    description:
      "Scan all registered in-repo Fw modules (heretic, neon, craft-libs, craft-anything, native/*, src, tests), run AST probes, and update target/manifest.json.",
    inputSchema: { type: "object", properties: {} },
  },
  {
    name: "fw_learn",
    description:
      "Run `fw learn` (status, harvest, distill, search, loop).",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["status", "harvest", "distill", "search", "loop"], description: "fw learn action." },
        query: { type: "string", description: "Search query when action is 'search'." },
      },
      required: ["action"],
    },
  },
  {
    name: "fw_bridge",
    description:
      "Inspect or dispatch to Fw's absorbed first/third-party subsystems (heretic, neon/REA, craft-libs, craft-anything, ghidra).",
    inputSchema: {
      type: "object",
      properties: {
        subsystem: { type: "string", enum: ["heretic", "neon", "craft", "ghidra"], description: "Subsystem name." },
      },
      required: ["subsystem"],
    },
  },
  {
    name: "fw_disasm",
    description:
      "Disassemble PE entry-point x86-64 instructions using craft-pe + iced-x86 Fat-LTO ABI, or identify any file via craft-sniff.",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["disasm", "identify"], description: "Operation mode." },
        path: { type: "string", description: "Binary or file path." },
        maxInsns: { type: "integer", minimum: 1, maximum: 128, description: "Instruction limit." },
      },
      required: ["path"],
    },
  },
  {
    name: "fw_vfs",
    description: "Interact with Fw SQLite CAS VFS OS (mounts, ls, read, write).",
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
    name: "fw_ai",
    description: "Query Fw AI Brain (FTS5), Soul constitution, Router providers, or Low-Level LTO/AOT matrix.",
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
    name: "fw_tui_terminal",
    description: "Inspect and run Fw-optimized Microsoft Intelligent Terminal (WTA) and tui-test source-only workflows.",
    inputSchema: {
      type: "object",
      properties: {
        action: { type: "string", enum: ["status", "wta_config", "verify_self"] },
      },
      required: ["action"],
    },
  },
  ...CRATES_TOOLS,
] as const;

function textResult(
  payload: unknown,
  isError = false,
): { content: Array<{ type: "text"; text: string }>; isError?: boolean } {
  const text = typeof payload === "string" ? payload : JSON.stringify(payload, null, 2);
  return isError ? { content: [{ type: "text", text }], isError: true } : { content: [{ type: "text", text }] };
}

export function runLearnBridge(action: string, query?: string) {
  const fw = process.env.FW_BIN || Bun.which("fw");
  if (!fw) return { ok: false, error: "the fw binary is not on PATH (set FW_BIN)" };
  const args =
    action === "search" ? ["learn", "search", ...(query ? query.trim().split(/\s+/) : ["fw"])] : ["learn", action];
  const proc = Bun.spawnSync([fw, ...args], { stdout: "pipe", stderr: "pipe" });
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

export function handleFwToolCall(
  root: string,
  name: string,
  args: Record<string, unknown> = {},
): { content: Array<{ type: "text"; text: string }>; isError?: boolean } {
  if (CRATES_TOOL_NAMES.includes(name)) {
    const res = callCratesTool(root, name, args);
    return textResult(res.payload, !res.ok);
  }
  switch (name) {
    case "fw_status": {
      const abi = probeRustAbi(root);
      const manifest = readTargetManifest(root);
      return textResult({
        ok: true,
        product: "fw",
        root,
        abiLoaded: abi.loaded,
        abi,
        manifestReady: Boolean(manifest),
        repoDigest: manifest?.repoDigest ?? null,
        plugin: pluginStatus(root),
      });
    }
    case "fw_ast_parse": {
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
    case "fw_pe_inspect": {
      if (typeof args.path !== "string" || !args.path.trim()) return textResult({ ok: false, error: "path is required" }, true);
      const target = resolve(root, args.path);
      const res = inspectPeWithAbi(root, target);
      return textResult(res, !res.ok);
    }
    case "fw_target_scan": {
      const manifest = buildTargetManifest(root);
      const manifestPath = writeTargetManifest(root, manifest);
      return textResult({ ok: true, manifestPath, manifest });
    }
    case "fw_learn": {
      const action = typeof args.action === "string" ? args.action : "status";
      const query = typeof args.query === "string" ? args.query : undefined;
      const res = runLearnBridge(action, query);
      return textResult(res, !res.ok);
    }
    case "fw_bridge": {
      const sub = typeof args.subsystem === "string" ? args.subsystem : "";
      const map: Record<string, { path: string; runtime: string }> = {
        heretic: { path: join(root, "configs", "heretic"), runtime: "uvx/heretic-llm (PyPI)" },
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
    case "fw_disasm": {
      const action = typeof args.action === "string" ? args.action : "disasm";
      if (typeof args.path !== "string" || !args.path.trim()) return textResult({ ok: false, error: "path is required" }, true);
      const target = resolve(root, args.path);
      if (action === "identify") {
        const res = identifyFileWithAbi(root, target);
        return textResult(res, !res.ok);
      }
      const max = typeof args.maxInsns === "number" ? args.maxInsns : 12;
      const res = disasmX64WithAbi(root, target, max);
      return textResult(res, !res.ok);
    }
    case "fw_vfs": {
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
    case "fw_ai": {
      const action = typeof args.action === "string" ? args.action : "soul";
      if (action === "brain_index") return textResult(brainIndexRepo(root));
      if (action === "brain_search") return textResult(brainSearch(root, typeof args.query === "string" ? args.query : "fw"));
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
    case "fw_tui_terminal": {
      const action = typeof args.action === "string" ? args.action : "status";
      if (action === "wta_config") return textResult(generateFwWtaConfig(root));
      if (action === "verify_self") {
        const res = verifyFwSelfTui(root);
        return textResult(res, !res.ok);
      }
      return textResult(getTerminalTuiStatus(root));
    }
    default:
      return textResult({ ok: false, error: `Unknown tool: ${name}` }, true);
  }
}

export const MODERN_VERSION = "2026-07-28";
export const LEGACY_VERSIONS = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const META_VERSION = "io.modelcontextprotocol/protocolVersion";
const META_CAPABILITIES = "io.modelcontextprotocol/clientCapabilities";
const META_SERVER = "io.modelcontextprotocol/serverInfo";
const SERVER_INFO = { name: "fw", version: "2.3.0" };

/** Set by `initialize`: the process then also serves requests that carry no per-request `_meta` (the earlier, session-scoped versions). */
let legacySession = false;

const rpcError = (id: JsonRpcResponse["id"], code: number, message: string, data?: unknown): JsonRpcResponse => ({
  jsonrpc: "2.0",
  id,
  error: { code, message, ...(data === undefined ? {} : { data }) },
});

function validateArguments(schema: unknown, args: Record<string, unknown>): string | null {
  const s = schema as { required?: string[]; properties?: Record<string, { type?: string; enum?: unknown[] }> };
  for (const key of s.required ?? []) if (!(key in args)) return `Missing required property '${key}'`;
  for (const [key, value] of Object.entries(args)) {
    const prop = s.properties?.[key];
    if (!prop) continue;
    const kind = Array.isArray(value) ? "array" : value === null ? "null" : typeof value;
    const ok = !prop.type || kind === prop.type || (prop.type === "integer" && Number.isInteger(value));
    if (!ok) return `${key}: expected ${prop.type} but got ${kind}`;
    if (prop.enum && !prop.enum.includes(value)) return `${key}: ${JSON.stringify(value)} is not one of the allowed values`;
  }
  return null;
}

function callTool(root: string, params: Record<string, unknown>) {
  const name = typeof params.name === "string" ? params.name : "";
  if (params.arguments !== undefined && (params.arguments === null || typeof params.arguments !== "object" || Array.isArray(params.arguments))) {
    return { error: "Invalid params for tools/call: 'arguments' must be an object" };
  }
  const args = (params.arguments ?? {}) as Record<string, unknown>;
  const tool = FW_MCP_TOOLS.find((t) => t.name === name);
  if (!tool) return { error: `Unknown tool: ${name}` };
  const problem = validateArguments(tool.inputSchema, args);
  if (problem) return { result: textResult(`Invalid arguments for tool ${name}: ${problem}`, true) };
  return { result: handleFwToolCall(root, name, args) };
}

/**
 * One JSON-RPC message. Requests that carry `_meta["io.modelcontextprotocol/protocolVersion"]` are served under
 * 2026-07-28 (stateless, `server/discover`, `resultType`, cache hints); a process that has seen `initialize` also
 * serves the earlier session-scoped versions.
 */
export function handleJsonRpcMessage(root: string, req: JsonRpcRequest): JsonRpcResponse | null {
  const id = req.id ?? null;
  const method = req.method ?? "";
  if (req.id === undefined) return null;
  const params = (req.params ?? {}) as Record<string, unknown>;
  const meta = params._meta as Record<string, unknown> | undefined;

  if (method === "initialize") {
    legacySession = true;
    const requested = typeof params.protocolVersion === "string" ? params.protocolVersion : "";
    return {
      jsonrpc: "2.0",
      id,
      result: {
        protocolVersion: LEGACY_VERSIONS.includes(requested) ? requested : LEGACY_VERSIONS[0],
        capabilities: { tools: { listChanged: false } },
        serverInfo: SERVER_INFO,
      },
    };
  }

  const modern = meta !== undefined && meta[META_VERSION] !== undefined;
  if (!modern) {
    if (!legacySession) {
      return rpcError(id, -32602, "Invalid params: _meta with io.modelcontextprotocol/protocolVersion and clientCapabilities is required");
    }
    if (method === "ping") return { jsonrpc: "2.0", id, result: {} };
    if (method === "tools/list") return { jsonrpc: "2.0", id, result: { tools: FW_MCP_TOOLS } };
    if (method === "tools/call") {
      const call = callTool(root, params);
      return call.error ? rpcError(id, -32602, call.error) : { jsonrpc: "2.0", id, result: call.result };
    }
    return rpcError(id, -32601, `Method not found: ${method}`);
  }

  const version = meta[META_VERSION];
  if (typeof version !== "string" || typeof meta[META_CAPABILITIES] !== "object" || meta[META_CAPABILITIES] === null) {
    return rpcError(id, -32602, "Invalid params: _meta.io.modelcontextprotocol/protocolVersion and clientCapabilities are required");
  }
  if (version !== MODERN_VERSION) {
    return rpcError(id, -32022, "Unsupported protocol version", { supported: [MODERN_VERSION, ...LEGACY_VERSIONS], requested: version });
  }
  const done = (result: Record<string, unknown>, cacheable = false): JsonRpcResponse => ({
    jsonrpc: "2.0",
    id,
    result: { resultType: "complete", ...result, ...(cacheable ? { ttlMs: 300000, cacheScope: "public" } : {}), _meta: { [META_SERVER]: SERVER_INFO } },
  });
  if (method === "server/discover") {
    return done({ supportedVersions: [MODERN_VERSION, ...LEGACY_VERSIONS], capabilities: { tools: { listChanged: false } } }, true);
  }
  if (method === "tools/list") return done({ tools: FW_MCP_TOOLS }, true);
  if (method === "tools/call") {
    const call = callTool(root, params);
    return call.error ? rpcError(id, -32602, call.error) : done(call.result as Record<string, unknown>);
  }
  return rpcError(id, -32601, `Method not found: ${method}`);
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
      let resp: JsonRpcResponse | null;
      try {
        resp = handleJsonRpcMessage(root, JSON.parse(line) as JsonRpcRequest);
      } catch (err) {
        resp = rpcError(null, -32700, `Parse error: ${err instanceof Error ? err.message : String(err)}`);
      }
      if (resp) process.stdout.write(`${JSON.stringify(resp)}\n`);
    }
  }
}
