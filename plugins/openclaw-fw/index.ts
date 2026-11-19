import { existsSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import {
  PrefixKvRagStore,
  RuntimeToolRegistry,
  type ToolLoader,
} from "../../src/bun_native_ai.ts";

export interface OpenClawFwPluginConfig {
  fwBinary?: string;
  cwd?: string;
  sandbox?: boolean;
  maxMatches?: number;
  enableMcpBridge?: boolean;
}

export interface OpenClawToolRegistration {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  execute: (args: Record<string, unknown>) => Promise<unknown>;
}

export interface OpenClawPluginApiLike {
  config?: Record<string, unknown>;
  registerTool?: (tool: OpenClawToolRegistration) => void;
  registerCommand?: (cmd: { name: string; description: string; handler: (args: unknown) => Promise<unknown> }) => void;
  logger?: { info?: (msg: string) => void; warn?: (msg: string) => void };
}

export interface FwLsEntry {
  name: string;
  kind: "file" | "dir" | "other";
  size: number;
}

export interface FwGrepMatch {
  file: string;
  line: number;
  text: string;
}

export interface FwBenchResult {
  ok: true;
  label: string;
  iterations: number;
  minMs: number;
  avgMs: number;
  p95Ms: number;
  maxMs: number;
  opsPerSec: number;
}

const DEFAULT_ALLOWED_CLAW_BINARIES = new Set([
  "rg",
  "fd",
  "ast-grep",
  "jaq",
  "xh",
  "nu",
  "jj",
  "difft",
  "just",
  "tokei",
  "hyperfine",
  "cargo",
  "bun",
  "fw",
]);

const DENIED_CLAW_PATTERNS = ["rm -rf /", "mkfs", "dd if=", "; rm ", "&& rm ", "$(", "`"];

export function validateClawSandbox(binary: string, args: string[] = []): { allowed: boolean; reason?: string } {
  const base = binary.replace(/\.exe$/i, "");
  if (!DEFAULT_ALLOWED_CLAW_BINARIES.has(base)) {
    return { allowed: false, reason: `sandbox denied binary: ${binary}` };
  }
  const joined = `${binary} ${args.join(" ")}`;
  for (const pat of DENIED_CLAW_PATTERNS) {
    if (joined.includes(pat)) {
      return { allowed: false, reason: `sandbox denied pattern: ${pat}` };
    }
  }
  return { allowed: true };
}

export function createOpenClawFwPlugin(config: OpenClawFwPluginConfig = {}) {
  const baseCwd = resolve(config.cwd ?? process.cwd());
  const sandboxDefault = config.sandbox ?? true;
  const maxMatchesDefault = config.maxMatches ?? 200;
  const store = new PrefixKvRagStore(":memory:");
  const runtimeTools = new RuntimeToolRegistry();

  const resolveSafePath = (target?: string): string => {
    const candidate = resolve(baseCwd, target ?? ".");
    return candidate;
  };

  async function fwLs(args: { path?: string; includeHidden?: boolean } = {}) {
    const dir = resolveSafePath(args.path);
    if (!existsSync(dir)) {
      return { ok: false, error: `Directory not found: ${dir}`, entries: [] as FwLsEntry[] };
    }
    const entries: FwLsEntry[] = [];
    for (const name of readdirSync(dir)) {
      if (!args.includeHidden && name.startsWith(".")) continue;
      const full = join(dir, name);
      const st = statSync(full);
      entries.push({
        name,
        kind: st.isDirectory() ? "dir" : st.isFile() ? "file" : "other",
        size: st.isFile() ? st.size : 0,
      });
    }
    entries.sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === "dir" ? -1 : 1));
    return { ok: true, path: dir, count: entries.length, entries };
  }

  async function fwFind(args: { pattern?: string; path?: string; limit?: number } = {}) {
    const dir = resolveSafePath(args.path);
    const pattern = args.pattern ?? "**/*";
    const limit = Math.max(1, args.limit ?? maxMatchesDefault);
    const glob = new Bun.Glob(pattern);
    const files: string[] = [];
    for await (const match of glob.scan({ cwd: dir, onlyFiles: true })) {
      files.push(match.replace(/\\/g, "/"));
      if (files.length >= limit) break;
    }
    files.sort();
    return { ok: true, cwd: dir, pattern, count: files.length, files };
  }

  async function fwGrep(args: {
    pattern: string;
    path?: string;
    glob?: string;
    caseInsensitive?: boolean;
    limit?: number;
  }) {
    if (!args?.pattern) {
      return { ok: false, error: "Require non-empty `pattern`", matches: [] as FwGrepMatch[] };
    }
    const dir = resolveSafePath(args.path);
    const limit = Math.max(1, args.limit ?? maxMatchesDefault);
    const regex = new RegExp(args.pattern, args.caseInsensitive ? "i" : "");
    const matches: FwGrepMatch[] = [];

    const st = existsSync(dir) ? statSync(dir) : null;
    if (!st) {
      return { ok: false, error: `Path not found: ${dir}`, matches };
    }

    const scanFile = async (absFile: string) => {
      const content = await Bun.file(absFile).text();
      const lines = content.split(/\r?\n/);
      const rel = relative(baseCwd, absFile).replace(/\\/g, "/");
      for (let i = 0; i < lines.length; i++) {
        if (regex.test(lines[i]!)) {
          matches.push({ file: rel, line: i + 1, text: lines[i]!.trim() });
          if (matches.length >= limit) return true;
        }
      }
      return false;
    };

    if (st.isFile()) {
      await scanFile(dir);
    } else {
      const fileGlob = new Bun.Glob(args.glob ?? "**/*.{ts,tsx,js,rs,json,toml,md}");
      for await (const relFile of fileGlob.scan({ cwd: dir, onlyFiles: true })) {
        if (relFile.includes("node_modules") || relFile.includes("target")) continue;
        const done = await scanFile(join(dir, relFile));
        if (done) break;
      }
    }

    return { ok: true, pattern: args.pattern, count: matches.length, matches };
  }

  async function fwBench(args: { label?: string; iterations?: number; workSize?: number } = {}): Promise<FwBenchResult> {
    const label = args.label ?? "fw-sha256-microbench";
    const iterations = Math.max(1, Math.min(500, args.iterations ?? 25));
    const workSize = Math.max(64, Math.min(65536, args.workSize ?? 1024));
    const payload = new Uint8Array(workSize);
    payload.fill(0x5a);

    const samples: number[] = [];
    for (let i = 0; i < iterations; i++) {
      const t0 = Bun.nanoseconds();
      const hasher = new Bun.CryptoHasher("sha256");
      hasher.update(payload);
      hasher.digest("hex");
      samples.push((Bun.nanoseconds() - t0) / 1e6);
    }

    samples.sort((a, b) => a - b);
    const sum = samples.reduce((acc, v) => acc + v, 0);
    const avgMs = sum / samples.length;
    const p95Idx = Math.min(samples.length - 1, Math.floor(samples.length * 0.95));
    return {
      ok: true,
      label,
      iterations,
      minMs: Number(samples[0]!.toFixed(4)),
      avgMs: Number(avgMs.toFixed(4)),
      p95Ms: Number(samples[p95Idx]!.toFixed(4)),
      maxMs: Number(samples[samples.length - 1]!.toFixed(4)),
      opsPerSec: avgMs > 0 ? Math.round(1000 / avgMs) : 0,
    };
  }

  async function fwClaw(args: {
    action?: "status" | "sandbox-check" | "kv-route" | "tool-compile";
    binary?: string;
    binaryArgs?: string[];
    prompt?: string;
    model?: string;
    toolName?: string;
    source?: string;
    loader?: ToolLoader;
  } = {}) {
    const action = args.action ?? "status";
    if (action === "status") {
      return {
        ok: true,
        plugin: "openclaw-fw",
        runtime: "bun",
        bunVersion: Bun.version,
        sandbox: sandboxDefault,
        kvStats: store.getStats(),
        registeredRuntimeTools: runtimeTools.listTools().length,
      };
    }
    if (action === "sandbox-check") {
      const check = validateClawSandbox(args.binary ?? "", args.binaryArgs ?? []);
      return { ok: check.allowed, binary: args.binary ?? "", ...check };
    }
    if (action === "kv-route") {
      if (!args.prompt) return { ok: false, error: "Require `prompt` for kv-route" };
      return store.routePrefix({ prompt: args.prompt, model: args.model });
    }
    if (action === "tool-compile") {
      if (!args.toolName || !args.source) {
        return { ok: false, error: "Require `toolName` and `source` for tool-compile" };
      }
      return runtimeTools.compileAndRegister({
        name: args.toolName,
        source: args.source,
        loader: args.loader ?? "ts",
        sandbox: sandboxDefault,
      });
    }
    return { ok: false, error: `Unknown fw_claw action: ${String(action)}` };
  }

  const toolMap: Record<string, OpenClawToolRegistration> = {
    fw_ls: {
      name: "fw_ls",
      description: "List directory entries with type and size.",
      inputSchema: {
        type: "object",
        properties: { path: { type: "string" }, includeHidden: { type: "boolean" } },
      },
      execute: (a) => fwLs(a as Parameters<typeof fwLs>[0]),
    },
    fw_grep: {
      name: "fw_grep",
      description: "Search files by regex or literal pattern.",
      inputSchema: {
        type: "object",
        required: ["pattern"],
        properties: {
          pattern: { type: "string" },
          path: { type: "string" },
          glob: { type: "string" },
          caseInsensitive: { type: "boolean" },
          limit: { type: "integer" },
        },
      },
      execute: (a) => fwGrep(a as Parameters<typeof fwGrep>[0]),
    },
    fw_find: {
      name: "fw_find",
      description: "Find files matching a glob pattern via Bun.Glob.",
      inputSchema: {
        type: "object",
        properties: { pattern: { type: "string" }, path: { type: "string" }, limit: { type: "integer" } },
      },
      execute: (a) => fwFind(a as Parameters<typeof fwFind>[0]),
    },
    fw_bench: {
      name: "fw_bench",
      description: "Run micro-benchmarks with Bun.nanoseconds() percentiles.",
      inputSchema: {
        type: "object",
        properties: {
          label: { type: "string" },
          iterations: { type: "integer" },
          workSize: { type: "integer" },
        },
      },
      execute: (a) => fwBench(a as Parameters<typeof fwBench>[0]),
    },
    fw_claw: {
      name: "fw_claw",
      description: "Run Fw Hermes/OpenClaw actions (status, sandbox-check, kv-route, tool-compile).",
      inputSchema: {
        type: "object",
        properties: {
          action: { type: "string", enum: ["status", "sandbox-check", "kv-route", "tool-compile"] },
          binary: { type: "string" },
          prompt: { type: "string" },
          toolName: { type: "string" },
          source: { type: "string" },
        },
      },
      execute: (a) => fwClaw(a as Parameters<typeof fwClaw>[0]),
    },
  };

  async function handleMcpRequest(rpc: {
    jsonrpc: "2.0";
    id: string | number | null;
    method: string;
    params?: Record<string, unknown>;
  }) {
    if (rpc.method === "initialize") {
      return {
        jsonrpc: "2.0" as const,
        id: rpc.id,
        result: {
          protocolVersion: "2025-06-18",
          serverInfo: { name: "openclaw-fw", version: "4.2.0" },
          capabilities: { tools: {} },
        },
      };
    }
    if (rpc.method === "tools/list") {
      return {
        jsonrpc: "2.0" as const,
        id: rpc.id,
        result: {
          tools: Object.values(toolMap).map((t) => ({
            name: t.name,
            description: t.description,
            inputSchema: t.inputSchema,
          })),
        },
      };
    }
    if (rpc.method === "tools/call") {
      const name = String(rpc.params?.name ?? "");
      const tool = toolMap[name];
      if (!tool) {
        return {
          jsonrpc: "2.0" as const,
          id: rpc.id,
          error: { code: -32602, message: `Unknown tool: ${name}` },
        };
      }
      const out = await tool.execute((rpc.params?.arguments ?? {}) as Record<string, unknown>);
      return {
        jsonrpc: "2.0" as const,
        id: rpc.id,
        result: {
          content: [{ type: "text", text: JSON.stringify(out) }],
          structuredContent: out,
        },
      };
    }
    return {
      jsonrpc: "2.0" as const,
      id: rpc.id,
      error: { code: -32601, message: `Method not found: ${rpc.method}` },
    };
  }

  toolMap.fw_mcp = {
    name: "fw_mcp",
    description: "Dispatch a JSON-RPC 2.0 MCP request to openclaw-fw.",
    inputSchema: {
      type: "object",
      required: ["method"],
      properties: {
        id: { type: ["string", "number", "null"] },
        method: { type: "string" },
        params: { type: "object" },
      },
    },
    execute: (a) =>
      handleMcpRequest({
        jsonrpc: "2.0",
        id: (a.id as string | number | null | undefined) ?? 1,
        method: String(a.method ?? ""),
        params: (a.params as Record<string, unknown> | undefined) ?? {},
      }),
  };

  return {
    id: "openclaw-fw",
    name: "fw Bun-Native Runtime",
    version: "4.2.0",
    tools: toolMap,
    handleMcpRequest,
    register(api: OpenClawPluginApiLike) {
      if (typeof api.registerTool === "function") {
        for (const tool of Object.values(toolMap)) {
          api.registerTool(tool);
        }
      }
      api.logger?.info?.("[openclaw-fw] Registered 6 Bun-native fw tools (ls, grep, find, bench, claw, mcp)");
    },
    close() {
      store.close();
    },
  };
}

const defaultPlugin = {
  id: "openclaw-fw",
  name: "fw Bun-Native Runtime",
  description: "Bun-native fw plugin exposing ls, grep, find, bench, claw, and MCP JSON-RPC 2.0.",
  version: "4.2.0",
  register(api: OpenClawPluginApiLike) {
    const instance = createOpenClawFwPlugin((api.config ?? {}) as OpenClawFwPluginConfig);
    instance.register(api);
    return instance;
  },
};

export default defaultPlugin;
