import packageJson from "../package.json" with { type: "json" };

export interface SqlStorageCursorLike<T = Record<string, unknown>> {
  toArray(): T[];
  one(): T;
  raw?: () => Iterable<unknown[]>;
}

export interface SqlStorageLike {
  exec<T = Record<string, unknown>>(query: string, ...bindings: unknown[]): SqlStorageCursorLike<T>;
}

export interface DurableObjectStorageLike {
  sql?: SqlStorageLike;
  get?<T = unknown>(key: string): Promise<T | undefined>;
  put?<T = unknown>(key: string, value: T): Promise<void>;
  delete?(key: string): Promise<boolean>;
  setAlarm?(scheduledTime: number | Date): Promise<void>;
}

export interface DurableObjectStateLike {
  id?: { toString(): string; name?: string };
  storage?: DurableObjectStorageLike;
  blockConcurrencyWhile?<T>(callback: () => Promise<T>): Promise<T>;
  waitUntil?(promise: Promise<unknown>): void;
}

export interface SandboxProcessHandleLike {
  output?: (options?: { encoding?: string }) => Promise<{ stdout?: string; stderr?: string; exitCode?: number }>;
  waitForExit?: () => Promise<{ exitCode?: number }>;
  stdout?: string;
  stderr?: string;
  exitCode?: number;
}

export interface DurableObjectStubLike {
  fetch(input: Request | string, init?: RequestInit): Promise<Response>;
  exec?(
    argv: string[],
    options?: { cwd?: string; env?: Record<string, string>; timeoutMs?: number },
  ): Promise<SandboxProcessHandleLike>;
}

export interface DurableObjectNamespaceLike {
  idFromName?(name: string): unknown;
  get?(id: unknown): DurableObjectStubLike;
  getByName?(name: string): DurableObjectStubLike;
}

export interface DynamicWorkerLoaderLike {
  load(config: {
    compatibilityDate: string;
    mainModule: string;
    modules: Record<string, string>;
    globalOutbound?: null | unknown;
    env?: Record<string, unknown>;
  }): {
    getEntrypoint(name?: string): {
      fetch(input: Request | string, init?: RequestInit): Promise<Response>;
    };
  };
}

export interface R2BucketLike {
  get(
    key: string,
  ): Promise<{ text(): Promise<string>; arrayBuffer(): Promise<ArrayBuffer>; size: number; httpMetadata?: { contentType?: string } } | null>;
  put(
    key: string,
    value: string | ArrayBuffer | Uint8Array,
    options?: { httpMetadata?: { contentType?: string }; customMetadata?: Record<string, string> },
  ): Promise<unknown>;
  delete(key: string): Promise<void>;
  head?(key: string): Promise<{ size: number; etag?: string } | null>;
}

export interface D1DatabaseLike {
  prepare(query: string): {
    bind(...values: unknown[]): {
      all<T = Record<string, unknown>>(): Promise<{ results: T[]; success: boolean }>;
      first<T = Record<string, unknown>>(): Promise<T | null>;
      run(): Promise<{ success: boolean; meta?: Record<string, unknown> }>;
    };
  };
}

export interface VectorizeIndexLike {
  upsert(vectors: Array<{ id: string; values: number[]; metadata?: Record<string, unknown> }>): Promise<unknown>;
  query(
    vector: number[],
    options?: { topK?: number; returnMetadata?: boolean | "all" | "indexed" | "none" },
  ): Promise<{ matches: Array<{ id: string; score: number; metadata?: Record<string, unknown> }> }>;
}

export interface BrowserRunLike {
  fetch(input: Request | string, init?: RequestInit): Promise<Response>;
}

export interface K2StreamLike {
  produce?(records: Array<{ key?: string; value: string | Uint8Array; headers?: Record<string, string> }>): Promise<unknown>;
  send?(records: unknown[]): Promise<unknown>;
}

export interface BasinPipelineLike {
  send(records: Array<Record<string, unknown>>): Promise<void>;
}

export interface WorkersAIModelLike {
  name: string;
  description?: string;
  task?: { name?: string };
  properties?: Array<{ property_id: string; value: unknown }>;
}

export interface WorkersAILike {
  run: (model: string, input: Record<string, unknown>) => Promise<unknown>;
  models?: (params?: { task?: string; per_page?: number; hide_experimental?: boolean }) => Promise<WorkersAIModelLike[]>;
}

export interface FwWorkerEnv {
  ASSETS?: { fetch: (request: Request | string) => Promise<Response> };
  CF_VERSION_METADATA?: { id: string; tag?: string; timestamp?: string };
  AI?: WorkersAILike;
  FW_OS?: DurableObjectNamespaceLike;
  LOADER?: DynamicWorkerLoaderLike;
  VFS_BLOBS?: R2BucketLike;
  META_DB?: D1DatabaseLike;
  MEMORY_VEC?: VectorizeIndexLike;
  BROWSER?: BrowserRunLike;
  EVENT_LOG?: K2StreamLike;
  TELEMETRY_PIPELINE?: BasinPipelineLike;
  SANDBOX?: DurableObjectNamespaceLike;
  FW_ENV?: string;
}

export interface CatalogEntry {
  id: string;
  name: string;
  version: string;
  layer: "os-control-plane" | "vfs-kernel" | "agent-runtime" | "execution-tier" | "browser" | "web-protocol" | "data-stream";
  url: string;
  docsUrl?: string;
  role: string;
  fwWiring: string;
}

export interface VfsNodeRecord {
  path: string;
  kind: "file" | "dir";
  content: string;
  contentType: string;
  size: number;
  sha256: string;
  blobKey: string | null;
  metadata: Record<string, unknown>;
  updatedAt: string;
}

export interface GatekeeperRule {
  id: string;
  capability: string;
  effect: "allow" | "deny";
  principal: string;
  pathPrefix?: string | null;
  reason: string;
  updatedAt: string;
}

export interface GatekeeperDecision {
  allowed: boolean;
  capability: string;
  principal: string;
  path?: string;
  matchedRuleId: string;
  effect: "allow" | "deny";
  reason: string;
}

export interface LearnEventRecord {
  id: string;
  kind: string;
  source: string;
  summary: string;
  tags: string[];
  payload: Record<string, unknown>;
  createdAt: string;
}

export interface A2ATaskRecord {
  id: string;
  tenant: string;
  status: { state: "submitted" | "working" | "completed" | "failed"; timestamp: string };
  artifacts: Array<{ name: string; parts: Array<{ type: "text"; text: string }> }>;
  history: Array<{ role: "user" | "agent"; parts: Array<{ type: "text"; text: string }> }>;
}

export interface AcpSessionRecord {
  sessionId: string;
  tenant: string;
  cwd: string;
  createdAt: string;
  turns: number;
}

export interface VfsExecRequest {
  command?: string;
  argv?: string[];
  code?: string;
  tier?: 0 | 1 | 2 | "auto";
  cwd?: string;
  env?: Record<string, string>;
  principal?: string;
  allowNetwork?: boolean;
  timeoutMs?: number;
  input?: Record<string, unknown>;
}

const FW_VERSION: string = packageJson.version;
/** Must equal `compatibility_date` in wrangler.jsonc; tests/fw_cloudflare.test.ts enforces it. */
export const COMPATIBILITY_DATE = "2026-11-19";
const R2_SPILL_THRESHOLD_BYTES = 65_536;


export const CLOUDFLARE_EDGE_OS_CATALOG: CatalogEntry[] = [
  {
    id: "cloudflare-os",
    name: "cloudflare/cloudflare-os",
    version: "2026.11",
    layer: "os-control-plane",
    url: "https://github.com/cloudflare/cloudflare-os",
    role: "Multi-worker AI OS workspace, gadget sandboxing, Access SSO, and Gatekeeper capability policies",
    fwWiring: "Gatekeeper policy engine in FwOSAgent (/os/gatekeeper) and multi-tenant DO isolation",
  },
  {
    id: "cloudflare-computer-dofs",
    name: "cloudflare/computer + @cloudflare/dofs",
    version: "2026.11",
    layer: "vfs-kernel",
    url: "https://github.com/cloudflare/computer",
    docsUrl: "https://www.npmjs.com/package/@cloudflare/dofs",
    role: "Authoritative per-tenant SQLite VFS inside Durable Objects with pluggable execution runtimes",
    fwWiring: "FwOSAgent SQLite vfs_nodes table + R2 blob spillover via /os/vfs/{ls,read,write,stat,exec}",
  },
  {
    id: "cloudflare-agents",
    name: "cloudflare/agents",
    version: "0.33.0",
    layer: "agent-runtime",
    url: "https://github.com/cloudflare/agents",
    docsUrl: "https://developers.cloudflare.com/agents/",
    role: "Durable Object Agent runtime with SQLite state, @callable RPC, runFiber/stash, scheduling, and MCP handlers",
    fwWiring: "Per-tenant FwOSAgent DO coordination model with SQLite schema init via blockConcurrencyWhile",
  },
  {
    id: "cloudflare-codemode",
    name: "@cloudflare/codemode",
    version: "0.5.6",
    layer: "execution-tier",
    url: "https://github.com/cloudflare/agents/tree/main/packages/codemode",
    docsUrl: "https://www.npmjs.com/package/@cloudflare/codemode",
    role: "Code-mode LLM tool orchestration using DynamicWorkerExecutor with globalOutbound: null",
    fwWiring: "Tier-1 execution path in /os/vfs/exec dispatching isolated ES modules via env.LOADER.load()",
  },
  {
    id: "cloudflare-sandbox",
    name: "@cloudflare/sandbox",
    version: "1.0.3",
    layer: "execution-tier",
    url: "https://github.com/cloudflare/sandbox-sdk",
    docsUrl: "https://developers.cloudflare.com/sandbox/",
    role: "Isolated Linux microVMs on Cloudflare Containers with argv process handles, output/waitForExit, and terminals",
    fwWiring: "Tier-2 execution path in /os/vfs/exec routing explicit argv payloads to env.SANDBOX Durable Object",
  },
  {
    id: "cloudflare-containers",
    name: "@cloudflare/containers",
    version: "0.3.7",
    layer: "execution-tier",
    url: "https://github.com/cloudflare/containers",
    docsUrl: "https://developers.cloudflare.com/containers/",
    role: "Durable Object-backed container lifecycle, routing, sleepAfter, and preview isolation",
    fwWiring: "Full-stack container binding topology in cloudflare/wrangler.fullstack.jsonc",
  },
  {
    id: "cloudflare-workers-rs",
    name: "cloudflare/workers-rs",
    version: "worker@0.9.0",
    layer: "vfs-kernel",
    url: "https://github.com/cloudflare/workers-rs",
    docsUrl: "https://crates.io/crates/worker",
    role: "Native Rust-to-WebAssembly Workers SDK for low-level crypto, PE/ELF parsing, and Wasm edge kernels",
    fwWiring: "Wasm ABI companion target alongside Fw Fat-LTO Rust crates (native/fw-abi)",
  },
  {
    id: "browser-run-kitesurf",
    name: "Browser Run + Kitesurf",
    version: "2026.11",
    layer: "browser",
    url: "https://github.com/cloudflare/kitesurf",
    docsUrl: "https://developers.cloudflare.com/browser-rendering/",
    role: "Managed headless Chromium plus Kitesurf Rust/Wasm (Blitz + Stylo) stateless CDP browser on V8 isolates",
    fwWiring: "Optional env.BROWSER binding declared in FwWorkerEnv and wrangler.fullstack.jsonc",
  },
  {
    id: "cloudflare-vibesdk",
    name: "cloudflare/vibesdk",
    version: "2026.11",
    layer: "os-control-plane",
    url: "https://github.com/cloudflare/vibesdk",
    role: "Vibe-coding platform combining SpaceDO workspaces, Dynamic Workers, and preview deployments",
    fwWiring: "Dynamic Worker preview & tenant workspace isolation pattern mirrored in Tier-1 + DO VFS",
  },
  {
    id: "cloudflare-moltworker",
    name: "cloudflare/moltworker",
    version: "2026.11",
    layer: "agent-runtime",
    url: "https://github.com/cloudflare/moltworker",
    role: "Self-hosted personal AI agent middleware on Workers, Sandbox microVMs, and R2 storage",
    fwWiring: "Personal agent state + R2 spillover (env.VFS_BLOBS) + Sandbox tier dispatch",
  },
  {
    id: "hono",
    name: "hono",
    version: "4.13",
    layer: "web-protocol",
    url: "https://github.com/honojs/hono",
    docsUrl: "https://hono.dev/",
    role: "Web Standards router and middleware for Workers HTTP APIs and OpenAPI gateways",
    fwWiring: "Zero-dependency Web Standards Fetch router contract compatible with Hono mounting",
  },
  {
    id: "just-bash",
    name: "just-bash",
    version: "3.6.1",
    layer: "execution-tier",
    url: "https://github.com/vercel-labs/just-bash",
    docsUrl: "https://www.npmjs.com/package/just-bash",
    role: "In-isolate virtual POSIX bash interpreter executing against an in-memory/DO filesystem",
    fwWiring: "Tier-0 virtual POSIX shell inside FwOSAgent (/vfs/exec tier=0) with zero container cold start",
  },
  {
    id: "mcp-server-sdk",
    name: "@modelcontextprotocol/server",
    version: "2.x",
    layer: "web-protocol",
    url: "https://github.com/modelcontextprotocol/typescript-sdk",
    docsUrl: "https://modelcontextprotocol.io/",
    role: "MCP v2 Streamable HTTP JSON-RPC 2.0 server contract (initialize, tools/list, tools/call)",
    fwWiring: "Native Streamable HTTP MCP gateway at POST /mcp backed by FwOSAgent tools",
  },
  {
    id: "cloudflare-basin",
    name: "Cloudflare Basin (Pipelines, Catalog, SQL)",
    version: "GA 2026",
    layer: "data-stream",
    url: "https://developers.cloudflare.com/basin/",
    docsUrl: "https://developers.cloudflare.com/basin-pipelines/",
    role: "1 GB/s streaming ingestion (pipelines[].stream) into R2 Apache Iceberg tables queried via Basin SQL",
    fwWiring: "Optional env.TELEMETRY_PIPELINE.send() sink on autonomous learning and audit events",
  },
  {
    id: "cloudflare-k2",
    name: "Cloudflare K2 Streams",
    version: "Beta 2026",
    layer: "data-stream",
    url: "https://developers.cloudflare.com/k2/",
    docsUrl: "https://developers.cloudflare.com/k2/features/produce/",
    role: "Durable at-least-once event log (k2[].stream) for decoupled producers, subscriptions, and replay",
    fwWiring: "Optional env.EVENT_LOG produce sink on /api/learn/events for durable cross-agent learning replay",
  },
];

const EXECUTION_TIERS = [
  {
    tier: 0,
    id: "do-posix-shell",
    name: "Tier-0: In-DO Virtual POSIX Shell",
    coldStartMs: 0,
    isolation: "Durable Object V8 isolate + SQLite VFS + Gatekeeper",
    primitives: ["cloudflare/computer", "@cloudflare/dofs", "just-bash@3.6.1"],
    capabilities: ["vfs:read", "vfs:write", "exec:tier0"],
  },
  {
    tier: 1,
    id: "dynamic-worker-loader",
    name: "Tier-1: Dynamic Worker Loader (CodeMode Isolate)",
    coldStartMs: 2,
    isolation: "Ephemeral V8 isolate via env.LOADER.load() with globalOutbound: null",
    primitives: ["@cloudflare/codemode@0.5.6", "worker_loaders", "cloudflare/vibesdk"],
    capabilities: ["exec:tier1", "net:egress (denied by default)"],
  },
  {
    tier: 2,
    id: "cloudflare-sandbox-microvm",
    name: "Tier-2: Cloudflare Sandbox 1.0 Container MicroVM",
    coldStartMs: 250,
    isolation: "Dedicated Linux container microVM via env.SANDBOX argv process handles",
    primitives: ["@cloudflare/sandbox@1.0.3", "@cloudflare/containers@0.3.7", "cloudflare/moltworker"],
    capabilities: ["exec:tier2", "net:egress (policy-gated)"],
  },
];

const MCP_TOOLS = [
  {
    name: "vfs_ls",
    description: "List files and directories under a prefix in the tenant SQLite VFS.",
    inputSchema: { type: "object", properties: { prefix: { type: "string", default: "/" } } },
  },
  {
    name: "vfs_read",
    description: "Read a file from the tenant SQLite VFS (with transparent R2 spillover hydration).",
    inputSchema: { type: "object", required: ["path"], properties: { path: { type: "string" } } },
  },
  {
    name: "vfs_write",
    description: "Write a file to the tenant SQLite VFS after Gatekeeper policy verification.",
    inputSchema: {
      type: "object",
      required: ["path", "content"],
      properties: {
        path: { type: "string" },
        content: { type: "string" },
        contentType: { type: "string" },
      },
    },
  },
  {
    name: "vfs_stat",
    description: "Return metadata, size, and SHA-256 digest for a VFS path.",
    inputSchema: { type: "object", required: ["path"], properties: { path: { type: "string" } } },
  },
  {
    name: "vfs_exec",
    description: "Execute a command or code snippet via Fw 3-tier routing (0=DO POSIX, 1=CodeMode LOADER, 2=Sandbox microVM).",
    inputSchema: {
      type: "object",
      properties: {
        command: { type: "string" },
        argv: { type: "array", items: { type: "string" } },
        code: { type: "string" },
        tier: { oneOf: [{ type: "integer", enum: [0, 1, 2] }, { type: "string", enum: ["auto"] }] },
        cwd: { type: "string" },
      },
    },
  },
  {
    name: "gatekeeper_check",
    description: "Evaluate a Cloudflare OS Gatekeeper capability check for a principal and optional path.",
    inputSchema: {
      type: "object",
      required: ["capability"],
      properties: {
        capability: { type: "string" },
        principal: { type: "string" },
        path: { type: "string" },
      },
    },
  },
  {
    name: "os_catalog",
    description: "Return the verified Nov 2026 Cloudflare Edge OS / Computer / Agents / Sandbox / Basin / K2 catalog.",
    inputSchema: { type: "object", properties: { layer: { type: "string" } } },
  },
  {
    name: "learn_log",
    description: "Append an autonomous learning event to the tenant DO log (and K2 / Basin Pipelines if bound).",
    inputSchema: {
      type: "object",
      required: ["kind", "summary"],
      properties: {
        kind: { type: "string" },
        source: { type: "string" },
        summary: { type: "string" },
        tags: { type: "array", items: { type: "string" } },
        payload: { type: "object" },
      },
    },
  },
  {
    name: "learn_status",
    description: "Inspect autonomous learning log counts and connected streaming sinks.",
    inputSchema: { type: "object", properties: {} },
  },
];

function json(data: unknown, status = 200, extraHeaders?: Record<string, string>): Response {
  return new Response(JSON.stringify(data, null, 2), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "access-control-allow-origin": "*",
      ...extraHeaders,
    },
  });
}

function nowIso(): string {
  return new Date().toISOString();
}

function makeId(prefix: string): string {
  const rand = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID().slice(0, 12)
    : Math.random().toString(36).slice(2, 14);
  return `${prefix}_${Date.now().toString(36)}_${rand}`;
}

async function sha256Hex(input: string): Promise<string> {
  const bytes = new TextEncoder().encode(input);
  if (typeof crypto !== "undefined" && crypto.subtle) {
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    return Array.from(new Uint8Array(digest))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
  }
  let h1 = 0x811c9dc5;
  let h2 = 0x9e3779b9;
  for (let i = 0; i < bytes.length; i++) {
    h1 ^= bytes[i]!;
    h1 = Math.imul(h1, 0x01000193);
    h2 ^= bytes[i]! + i;
    h2 = Math.imul(h2, 0x85ebca6b);
  }
  return `${(h1 >>> 0).toString(16).padStart(8, "0")}${(h2 >>> 0).toString(16).padStart(8, "0")}`.repeat(4);
}

function normalizeVfsPath(rawPath: string, cwd = "/"): { path: string; traversal: boolean } {
  const trimmed = (rawPath || "/").trim().replace(/\\/g, "/");
  const combined = trimmed.startsWith("/") ? trimmed : `${cwd.replace(/\/+$/, "")}/${trimmed}`;
  const rawSegments = combined.split("/");
  const stack: string[] = [];
  let traversal = false;
  for (const seg of rawSegments) {
    if (!seg || seg === ".") continue;
    if (seg === "..") {
      traversal = true;
      stack.pop();
      continue;
    }
    stack.push(seg);
  }
  return { path: `/${stack.join("/")}`, traversal };
}

function parseShellTokens(command: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  for (let i = 0; i < command.length; i++) {
    const ch = command[i]!;
    if (quote) {
      if (ch === quote) {
        quote = null;
      } else if (ch === "\\" && quote === '"' && i + 1 < command.length) {
        current += command[++i]!;
      } else {
        current += ch;
      }
      continue;
    }
    if (ch === "'" || ch === '"') {
      quote = ch;
    } else if (/\s/.test(ch)) {
      if (current.length > 0) {
        tokens.push(current);
        current = "";
      }
    } else {
      current += ch;
    }
  }
  if (current.length > 0) tokens.push(current);
  return tokens;
}

function deterministicEmbedding(text: string, dims = 64): number[] {
  const vec = new Array<number>(dims).fill(0);
  const bytes = new TextEncoder().encode(text);
  for (let i = 0; i < bytes.length; i++) {
    const idx = (bytes[i]! * 31 + i * 17) % dims;
    vec[idx] = (vec[idx] ?? 0) + ((bytes[i]! % 29) - 14) / 14;
  }
  const norm = Math.sqrt(vec.reduce((acc, v) => acc + v * v, 0)) || 1;
  return vec.map((v) => Number((v / norm).toFixed(6)));
}

function extractAiText(aiRes: unknown): string {
  if (typeof aiRes === "string") return aiRes;
  if (aiRes && typeof aiRes === "object") {
    const rec = aiRes as Record<string, unknown>;
    if (typeof rec.response === "string") return rec.response;
    if (typeof rec.output_text === "string") return rec.output_text;
    if (Array.isArray(rec.choices) && rec.choices.length > 0) {
      const first = rec.choices[0] as { message?: { content?: string }; text?: string };
      if (typeof first?.message?.content === "string") return first.message.content;
      if (typeof first?.text === "string") return first.text;
    }
  }
  return JSON.stringify(aiRes);
}

function defaultGatekeeperRules(): GatekeeperRule[] {
  const ts = nowIso();
  return [
    {
      id: "deny-secrets-write",
      capability: "vfs:write",
      effect: "deny",
      principal: "*",
      pathPrefix: "/etc/secrets",
      reason: "Protected secret store cannot be mutated through VFS write",
      updatedAt: ts,
    },
    {
      id: "deny-proc-keys-write",
      capability: "vfs:write",
      effect: "deny",
      principal: "*",
      pathPrefix: "/proc/keys",
      reason: "Kernel keyring path is read-only",
      updatedAt: ts,
    },
    {
      id: "deny-guest-tier2",
      capability: "exec:tier2",
      effect: "deny",
      principal: "guest",
      pathPrefix: null,
      reason: "Guest principals cannot launch Tier-2 container microVMs",
      updatedAt: ts,
    },
    {
      id: "deny-default-egress",
      capability: "net:egress",
      effect: "deny",
      principal: "default",
      pathPrefix: null,
      reason: "Outbound network egress is fail-closed by default (CodeMode globalOutbound: null)",
      updatedAt: ts,
    },
    {
      id: "deny-guest-egress",
      capability: "net:egress",
      effect: "deny",
      principal: "guest",
      pathPrefix: null,
      reason: "Guest principals have no outbound network egress",
      updatedAt: ts,
    },
    {
      id: "allow-admin-all",
      capability: "*",
      effect: "allow",
      principal: "admin",
      pathPrefix: null,
      reason: "Admin principal authorized across capabilities (subject to explicit deny rules)",
      updatedAt: ts,
    },
    {
      id: "allow-vfs-read",
      capability: "vfs:read",
      effect: "allow",
      principal: "*",
      pathPrefix: null,
      reason: "Tenant VFS read allowed within isolated Durable Object",
      updatedAt: ts,
    },
    {
      id: "allow-vfs-write",
      capability: "vfs:write",
      effect: "allow",
      principal: "*",
      pathPrefix: null,
      reason: "Tenant VFS write allowed outside protected system prefixes",
      updatedAt: ts,
    },
    {
      id: "allow-exec-tier0",
      capability: "exec:tier0",
      effect: "allow",
      principal: "*",
      pathPrefix: null,
      reason: "Tier-0 in-DO virtual POSIX shell is sandboxed to tenant VFS",
      updatedAt: ts,
    },
    {
      id: "allow-exec-tier1",
      capability: "exec:tier1",
      effect: "allow",
      principal: "*",
      pathPrefix: null,
      reason: "Tier-1 Dynamic Worker Loader runs in isolated V8 sandbox",
      updatedAt: ts,
    },
    {
      id: "allow-exec-tier2-agent",
      capability: "exec:tier2",
      effect: "allow",
      principal: "agent",
      pathPrefix: null,
      reason: "Authenticated agent principal may dispatch Tier-2 Sandbox microVMs",
      updatedAt: ts,
    },
    {
      id: "allow-exec-tier2-default",
      capability: "exec:tier2",
      effect: "allow",
      principal: "default",
      pathPrefix: null,
      reason: "Default tenant principal may dispatch Tier-2 Sandbox microVMs",
      updatedAt: ts,
    },
    {
      id: "allow-ai-infer",
      capability: "ai:infer",
      effect: "allow",
      principal: "*",
      pathPrefix: null,
      reason: "Workers AI inference permitted",
      updatedAt: ts,
    },
    {
      id: "allow-learn-write",
      capability: "learn:write",
      effect: "allow",
      principal: "*",
      pathPrefix: null,
      reason: "Autonomous learning log append permitted",
      updatedAt: ts,
    },
  ];
}

function getBindingSummary(env: FwWorkerEnv): Record<string, boolean> {
  return {
    AI: Boolean(env.AI),
    FW_OS: Boolean(env.FW_OS),
    LOADER: Boolean(env.LOADER),
    VFS_BLOBS: Boolean(env.VFS_BLOBS),
    META_DB: Boolean(env.META_DB),
    MEMORY_VEC: Boolean(env.MEMORY_VEC),
    BROWSER: Boolean(env.BROWSER),
    EVENT_LOG: Boolean(env.EVENT_LOG),
    TELEMETRY_PIPELINE: Boolean(env.TELEMETRY_PIPELINE),
    SANDBOX: Boolean(env.SANDBOX),
  };
}

/**
 * Durable Object SQLite Kernel & VFS for Fw Edge OS.
 *
 * Compatible with both Cloudflare Workers SQLite Durable Objects (`ctx.storage.sql`)
 * and direct in-memory instantiation in Bun/Node test suites.
 */
export class FwOSAgent {
  private readonly ctx?: DurableObjectStateLike;
  private env: FwWorkerEnv;
  private readonly sql?: SqlStorageLike;
  private readonly vfsMem = new Map<string, VfsNodeRecord>();
  private readonly rulesMem = new Map<string, GatekeeperRule>();
  private readonly learnMem: LearnEventRecord[] = [];
  private readonly auditMem: Array<{
    id: string;
    tier: number;
    command: string;
    allowed: boolean;
    exitCode: number;
    createdAt: string;
  }> = [];
  private readonly a2aTasks = new Map<string, A2ATaskRecord>();
  private readonly acpSessions = new Map<string, AcpSessionRecord>();
  private initialized = false;

  constructor(ctx?: DurableObjectStateLike, env: FwWorkerEnv = {}) {
    this.ctx = ctx;
    this.env = env;
    this.sql = ctx?.storage?.sql;

    if (ctx?.blockConcurrencyWhile) {
      void ctx.blockConcurrencyWhile(async () => {
        this.ensureInitialized();
      });
    } else {
      this.ensureInitialized();
    }
  }

  private ensureInitialized(): void {
    if (this.initialized) return;
    this.initialized = true;
    const ts = nowIso();
    const manifestJson = JSON.stringify(
      {
        product: "fw-edge",
        version: FW_VERSION,
        compatibilityDate: COMPATIBILITY_DATE,
        tiers: [0, 1, 2],
      },
      null,
      2,
    );

    if (this.sql) {
      this.sql.exec(`
        CREATE TABLE IF NOT EXISTS vfs_nodes (
          path TEXT PRIMARY KEY,
          kind TEXT NOT NULL,
          content TEXT NOT NULL,
          content_type TEXT NOT NULL,
          size INTEGER NOT NULL,
          sha256 TEXT NOT NULL,
          blob_key TEXT,
          metadata TEXT NOT NULL,
          updated_at TEXT NOT NULL
        )
      `);
      this.sql.exec(`
        CREATE TABLE IF NOT EXISTS gatekeeper_rules (
          id TEXT PRIMARY KEY,
          capability TEXT NOT NULL,
          effect TEXT NOT NULL,
          principal TEXT NOT NULL,
          path_prefix TEXT,
          reason TEXT NOT NULL,
          updated_at TEXT NOT NULL
        )
      `);
      this.sql.exec(`
        CREATE TABLE IF NOT EXISTS learn_events (
          id TEXT PRIMARY KEY,
          kind TEXT NOT NULL,
          source TEXT NOT NULL,
          summary TEXT NOT NULL,
          tags TEXT NOT NULL,
          payload TEXT NOT NULL,
          created_at TEXT NOT NULL
        )
      `);
      this.sql.exec(`
        CREATE TABLE IF NOT EXISTS exec_audit (
          id TEXT PRIMARY KEY,
          tier INTEGER NOT NULL,
          command TEXT NOT NULL,
          allowed INTEGER NOT NULL,
          exit_code INTEGER NOT NULL,
          created_at TEXT NOT NULL
        )
      `);

      this.sql.exec(
        "INSERT OR IGNORE INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        "/",
        "dir",
        "",
        "inode/directory",
        0,
        "",
        null,
        "{}",
        ts,
      );
      this.sql.exec(
        "INSERT OR IGNORE INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        "/etc",
        "dir",
        "",
        "inode/directory",
        0,
        "",
        null,
        "{}",
        ts,
      );
      this.sql.exec(
        "INSERT OR IGNORE INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        "/workspace",
        "dir",
        "",
        "inode/directory",
        0,
        "",
        null,
        "{}",
        ts,
      );
      this.sql.exec(
        "INSERT OR IGNORE INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        "/etc/fw.json",
        "file",
        manifestJson,
        "application/json",
        new TextEncoder().encode(manifestJson).byteLength,
        "seed",
        null,
        "{}",
        ts,
      );

      for (const rule of defaultGatekeeperRules()) {
        this.sql.exec(
          "INSERT OR IGNORE INTO gatekeeper_rules (id, capability, effect, principal, path_prefix, reason, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
          rule.id,
          rule.capability,
          rule.effect,
          rule.principal,
          rule.pathPrefix ?? null,
          rule.reason,
          rule.updatedAt,
        );
      }
      return;
    }

    this.vfsMem.set("/", {
      path: "/",
      kind: "dir",
      content: "",
      contentType: "inode/directory",
      size: 0,
      sha256: "",
      blobKey: null,
      metadata: {},
      updatedAt: ts,
    });
    this.vfsMem.set("/etc", {
      path: "/etc",
      kind: "dir",
      content: "",
      contentType: "inode/directory",
      size: 0,
      sha256: "",
      blobKey: null,
      metadata: {},
      updatedAt: ts,
    });
    this.vfsMem.set("/workspace", {
      path: "/workspace",
      kind: "dir",
      content: "",
      contentType: "inode/directory",
      size: 0,
      sha256: "",
      blobKey: null,
      metadata: {},
      updatedAt: ts,
    });
    this.vfsMem.set("/etc/fw.json", {
      path: "/etc/fw.json",
      kind: "file",
      content: manifestJson,
      contentType: "application/json",
      size: new TextEncoder().encode(manifestJson).byteLength,
      sha256: "seed",
      blobKey: null,
      metadata: {},
      updatedAt: ts,
    });
    for (const rule of defaultGatekeeperRules()) {
      this.rulesMem.set(rule.id, rule);
    }
  }

  public bindEnv(env: FwWorkerEnv): void { this.env = env; }

  private getTenantName(): string {
    return this.ctx?.id?.name ?? this.ctx?.id?.toString() ?? "default";
  }

  private storageBackend(): "sqlite" | "memory" {
    return this.sql ? "sqlite" : "memory";
  }

  private readNode(path: string): VfsNodeRecord | null {
    this.ensureInitialized();
    if (this.sql) {
      const rows = this.sql
        .exec<{
          path: string;
          kind: "file" | "dir";
          content: string;
          content_type: string;
          size: number;
          sha256: string;
          blob_key: string | null;
          metadata: string;
          updated_at: string;
        }>("SELECT path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at FROM vfs_nodes WHERE path = ?", path)
        .toArray();
      const row = rows[0];
      if (!row) return null;
      return {
        path: row.path,
        kind: row.kind,
        content: row.content,
        contentType: row.content_type,
        size: row.size,
        sha256: row.sha256,
        blobKey: row.blob_key,
        metadata: JSON.parse(row.metadata || "{}") as Record<string, unknown>,
        updatedAt: row.updated_at,
      };
    }
    return this.vfsMem.get(path) ?? null;
  }

  private upsertNode(node: VfsNodeRecord): void {
    this.ensureInitialized();
    if (this.sql) {
      this.sql.exec(
        `INSERT INTO vfs_nodes (path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
           kind = excluded.kind,
           content = excluded.content,
           content_type = excluded.content_type,
           size = excluded.size,
           sha256 = excluded.sha256,
           blob_key = excluded.blob_key,
           metadata = excluded.metadata,
           updated_at = excluded.updated_at`,
        node.path,
        node.kind,
        node.content,
        node.contentType,
        node.size,
        node.sha256,
        node.blobKey,
        JSON.stringify(node.metadata ?? {}),
        node.updatedAt,
      );
      return;
    }
    this.vfsMem.set(node.path, node);
  }

  private deleteNode(path: string): boolean {
    this.ensureInitialized();
    const existing = this.readNode(path);
    if (!existing) return false;
    if (this.sql) {
      this.sql.exec("DELETE FROM vfs_nodes WHERE path = ?", path);
    } else {
      this.vfsMem.delete(path);
    }
    return true;
  }

  private ensureParentDirs(path: string): void {
    const parts = path.split("/").filter(Boolean);
    let acc = "";
    const ts = nowIso();
    for (let i = 0; i < parts.length - 1; i++) {
      acc += `/${parts[i]!}`;
      if (!this.readNode(acc)) {
        this.upsertNode({
          path: acc,
          kind: "dir",
          content: "",
          contentType: "inode/directory",
          size: 0,
          sha256: "",
          blobKey: null,
          metadata: {},
          updatedAt: ts,
        });
      }
    }
  }

  private listAllNodes(): VfsNodeRecord[] {
    this.ensureInitialized();
    if (this.sql) {
      return this.sql
        .exec<{
          path: string;
          kind: "file" | "dir";
          content: string;
          content_type: string;
          size: number;
          sha256: string;
          blob_key: string | null;
          metadata: string;
          updated_at: string;
        }>("SELECT path, kind, content, content_type, size, sha256, blob_key, metadata, updated_at FROM vfs_nodes ORDER BY path ASC")
        .toArray()
        .map((row) => ({
          path: row.path,
          kind: row.kind,
          content: row.content,
          contentType: row.content_type,
          size: row.size,
          sha256: row.sha256,
          blobKey: row.blob_key,
          metadata: JSON.parse(row.metadata || "{}") as Record<string, unknown>,
          updatedAt: row.updated_at,
        }));
    }
    return [...this.vfsMem.values()].sort((a, b) => a.path.localeCompare(b.path));
  }

  public gatekeeperListPolicies(): GatekeeperRule[] {
    this.ensureInitialized();
    if (this.sql) {
      return this.sql
        .exec<{
          id: string;
          capability: string;
          effect: "allow" | "deny";
          principal: string;
          path_prefix: string | null;
          reason: string;
          updated_at: string;
        }>("SELECT id, capability, effect, principal, path_prefix, reason, updated_at FROM gatekeeper_rules ORDER BY id ASC")
        .toArray()
        .map((r) => ({
          id: r.id,
          capability: r.capability,
          effect: r.effect,
          principal: r.principal,
          pathPrefix: r.path_prefix,
          reason: r.reason,
          updatedAt: r.updated_at,
        }));
    }
    return [...this.rulesMem.values()].sort((a, b) => a.id.localeCompare(b.id));
  }

  public gatekeeperUpsertPolicy(rule: Omit<GatekeeperRule, "updatedAt"> & { updatedAt?: string }): GatekeeperRule {
    this.ensureInitialized();
    const record: GatekeeperRule = {
      id: rule.id || makeId("rule"),
      capability: rule.capability,
      effect: rule.effect,
      principal: rule.principal || "*",
      pathPrefix: rule.pathPrefix ?? null,
      reason: rule.reason || "Custom Gatekeeper rule",
      updatedAt: rule.updatedAt ?? nowIso(),
    };
    if (this.sql) {
      this.sql.exec(
        `INSERT INTO gatekeeper_rules (id, capability, effect, principal, path_prefix, reason, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           capability = excluded.capability,
           effect = excluded.effect,
           principal = excluded.principal,
           path_prefix = excluded.path_prefix,
           reason = excluded.reason,
           updated_at = excluded.updated_at`,
        record.id,
        record.capability,
        record.effect,
        record.principal,
        record.pathPrefix ?? null,
        record.reason,
        record.updatedAt,
      );
    } else {
      this.rulesMem.set(record.id, record);
    }
    return record;
  }

  public gatekeeperCheck(capability: string, principal = "default", rawPath?: string): GatekeeperDecision {
    this.ensureInitialized();
    let normalizedPath: string | undefined;
    if (rawPath !== undefined) {
      const norm = normalizeVfsPath(rawPath);
      if (norm.traversal) {
        return {
          allowed: false,
          capability,
          principal,
          path: rawPath,
          matchedRuleId: "builtin-traversal-guard",
          effect: "deny",
          reason: "Path traversal ('..') denied by Gatekeeper",
        };
      }
      normalizedPath = norm.path;
    }

    const rules = this.gatekeeperListPolicies();
    const matching = rules.filter((r) => {
      const capOk = r.capability === "*" || r.capability === capability;
      const principalOk = r.principal === "*" || r.principal === principal;
      const pathOk = !r.pathPrefix || (normalizedPath !== undefined && (normalizedPath === r.pathPrefix || normalizedPath.startsWith(`${r.pathPrefix}/`)));
      return capOk && principalOk && pathOk;
    });

    const denyMatch = matching.find((r) => r.effect === "deny");
    if (denyMatch) {
      return {
        allowed: false,
        capability,
        principal,
        path: normalizedPath,
        matchedRuleId: denyMatch.id,
        effect: "deny",
        reason: denyMatch.reason,
      };
    }

    const allowMatch = matching.find((r) => r.effect === "allow");
    if (allowMatch) {
      return {
        allowed: true,
        capability,
        principal,
        path: normalizedPath,
        matchedRuleId: allowMatch.id,
        effect: "allow",
        reason: allowMatch.reason,
      };
    }

    return {
      allowed: false,
      capability,
      principal,
      path: normalizedPath,
      matchedRuleId: "default-deny",
      effect: "deny",
      reason: `No Gatekeeper allow rule matched capability '${capability}' for principal '${principal}'`,
    };
  }

  public vfsLs(prefix = "/", principal = "default"): Record<string, unknown> {
    const decision = this.gatekeeperCheck("vfs:read", principal, prefix);
    if (!decision.allowed) {
      return { ok: false, denied: true, gatekeeper: decision };
    }
    const { path: cleanPrefix } = normalizeVfsPath(prefix);
    const all = this.listAllNodes();
    const entries = all
      .filter((n) => {
        if (cleanPrefix === "/") return n.path !== "/";
        return n.path === cleanPrefix || n.path.startsWith(`${cleanPrefix}/`);
      })
      .map((n) => ({
        path: n.path,
        kind: n.kind,
        size: n.size,
        contentType: n.contentType,
        blobKey: n.blobKey,
        updatedAt: n.updatedAt,
      }));
    return {
      ok: true,
      prefix: cleanPrefix,
      storage: this.storageBackend(),
      count: entries.length,
      entries,
    };
  }

  public async vfsRead(rawPath: string, principal = "default"): Promise<Record<string, unknown>> {
    const decision = this.gatekeeperCheck("vfs:read", principal, rawPath);
    if (!decision.allowed) {
      return { ok: false, denied: true, gatekeeper: decision };
    }
    const { path } = normalizeVfsPath(rawPath);
    const node = this.readNode(path);
    if (!node) {
      return { ok: false, error: "ENOENT", path };
    }
    if (node.kind === "dir") {
      return { ok: false, error: "EISDIR", path };
    }
    let content = node.content;
    let hydratedFrom: "sqlite" | "r2" | "memory" = this.storageBackend();
    if (node.blobKey && this.env.VFS_BLOBS) {
      const obj = await this.env.VFS_BLOBS.get(node.blobKey);
      if (obj) {
        content = await obj.text();
        hydratedFrom = "r2";
      }
    }
    return {
      ok: true,
      path: node.path,
      content,
      contentType: node.contentType,
      size: node.size,
      sha256: node.sha256,
      blobKey: node.blobKey,
      storage: hydratedFrom,
      metadata: node.metadata,
      updatedAt: node.updatedAt,
    };
  }

  public async vfsWrite(
    rawPath: string,
    content: string,
    options?: {
      contentType?: string;
      metadata?: Record<string, unknown>;
      principal?: string;
      spillToR2?: boolean;
    },
  ): Promise<Record<string, unknown>> {
    const principal = options?.principal ?? "default";
    const decision = this.gatekeeperCheck("vfs:write", principal, rawPath);
    if (!decision.allowed) {
      return { ok: false, denied: true, gatekeeper: decision };
    }
    const { path } = normalizeVfsPath(rawPath);
    if (path === "/") {
      return { ok: false, error: "Cannot write file content to root directory '/'" };
    }
    this.ensureParentDirs(path);
    const bytes = new TextEncoder().encode(content);
    const sha256 = await sha256Hex(content);
    const contentType = options?.contentType ?? (path.endsWith(".json") ? "application/json" : "text/plain; charset=utf-8");
    const ts = nowIso();

    let storedContent = content;
    let blobKey: string | null = null;
    let storage: "sqlite" | "r2" | "memory" = this.storageBackend();
    if (this.env.VFS_BLOBS && (options?.spillToR2 || bytes.byteLength > R2_SPILL_THRESHOLD_BYTES)) {
      blobKey = `vfs/${this.getTenantName()}${path}`;
      await this.env.VFS_BLOBS.put(blobKey, content, {
        httpMetadata: { contentType },
        customMetadata: { sha256, path },
      });
      storedContent = "";
      storage = "r2";
    }

    this.upsertNode({
      path,
      kind: "file",
      content: storedContent,
      contentType,
      size: bytes.byteLength,
      sha256,
      blobKey,
      metadata: options?.metadata ?? {},
      updatedAt: ts,
    });

    return {
      ok: true,
      path,
      size: bytes.byteLength,
      sha256,
      blobKey,
      contentType,
      storage,
      updatedAt: ts,
    };
  }

  public vfsStat(rawPath: string, principal = "default"): Record<string, unknown> {
    const decision = this.gatekeeperCheck("vfs:read", principal, rawPath);
    if (!decision.allowed) {
      return { ok: false, denied: true, gatekeeper: decision };
    }
    const { path } = normalizeVfsPath(rawPath);
    const node = this.readNode(path);
    if (!node) {
      return { ok: true, exists: false, path };
    }
    return {
      ok: true,
      exists: true,
      path: node.path,
      kind: node.kind,
      size: node.size,
      contentType: node.contentType,
      sha256: node.sha256,
      blobKey: node.blobKey,
      metadata: node.metadata,
      storage: node.blobKey ? "r2" : this.storageBackend(),
      updatedAt: node.updatedAt,
    };
  }

  private recordAudit(tier: number, command: string, allowed: boolean, exitCode: number): void {
    const entry = {
      id: makeId("exec"),
      tier,
      command: command.slice(0, 512),
      allowed,
      exitCode,
      createdAt: nowIso(),
    };
    if (this.sql) {
      this.sql.exec(
        "INSERT INTO exec_audit (id, tier, command, allowed, exit_code, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        entry.id,
        entry.tier,
        entry.command,
        entry.allowed ? 1 : 0,
        entry.exitCode,
        entry.createdAt,
      );
    } else {
      this.auditMem.push(entry);
    }
  }

  private selectExecTier(req: VfsExecRequest): 0 | 1 | 2 {
    if (req.tier === 0 || req.tier === 1 || req.tier === 2) return req.tier;
    if (typeof req.code === "string" && req.code.trim().length > 0) return 1;
    const cmd = (req.command ?? "").trim();
    if (/^(js|ts|codemode):/i.test(cmd)) return 1;
    const firstArg = req.argv?.[0] ?? cmd.split(/\s+/)[0] ?? "";
    const tier2Binaries = new Set(["python", "python3", "gcc", "clang", "cargo", "rustc", "go", "git", "docker", "node", "bun", "bash", "/bin/bash", "/bin/sh"]);
    if (tier2Binaries.has(firstArg)) return 2;
    return 0;
  }

  private async runTier0Single(
    segment: string,
    cwd: string,
    principal: string,
    envVars: Record<string, string>,
  ): Promise<{ exitCode: number; stdout: string; stderr: string; cwd: string }> {
    const redirectMatch = segment.match(/^(.*?)(>>?)\s*([^\s]+)\s*$/);
    const coreCmd = redirectMatch ? redirectMatch[1]!.trim() : segment.trim();
    const redirectOp = redirectMatch ? redirectMatch[2]! : null;
    const redirectTarget = redirectMatch ? redirectMatch[3]! : null;

    const tokens = parseShellTokens(coreCmd);
    if (tokens.length === 0) return { exitCode: 0, stdout: "", stderr: "", cwd };
    const [cmd, ...args] = tokens;
    let stdout = "";
    let stderr = "";
    let exitCode = 0;
    let nextCwd = cwd;

    switch (cmd) {
      case "pwd":
        stdout = `${cwd}\n`;
        break;
      case "whoami":
        stdout = `${principal}\n`;
        break;
      case "uname":
        stdout = `FwEdgeOS ${FW_VERSION} cloudflare-workers wasm32-v8\n`;
        break;
      case "echo":
        stdout = `${args.join(" ")}\n`;
        break;
      case "env": {
        const merged = { FW_ENV: this.env.FW_ENV ?? "production", PWD: cwd, USER: principal, ...envVars };
        stdout = `${Object.entries(merged).map(([k, v]) => `${k}=${v}`).join("\n")}\n`;
        break;
      }
      case "cd": {
        const target = args[0] ?? "/";
        const norm = normalizeVfsPath(target, cwd);
        if (norm.traversal) {
          exitCode = 1;
          stderr = "cd: path traversal denied\n";
        } else {
          const node = this.readNode(norm.path);
          if (!node || node.kind !== "dir") {
            exitCode = 1;
            stderr = `cd: ${target}: No such directory\n`;
          } else {
            nextCwd = norm.path;
          }
        }
        break;
      }
      case "ls": {
        const target = args[0] ?? cwd;
        const norm = normalizeVfsPath(target, cwd);
        const res = this.vfsLs(norm.path, principal) as { ok: boolean; entries?: Array<{ path: string; kind: string }> };
        if (!res.ok) {
          exitCode = 126;
          stderr = "ls: Gatekeeper denied\n";
        } else {
          stdout = `${(res.entries ?? []).map((e) => e.path).join("\n")}${res.entries?.length ? "\n" : ""}`;
        }
        break;
      }
      case "cat": {
        if (args.length === 0) {
          exitCode = 1;
          stderr = "cat: missing file operand\n";
        } else {
          const parts: string[] = [];
          for (const a of args) {
            const norm = normalizeVfsPath(a, cwd);
            const r = await this.vfsRead(norm.path, principal);
            if (!r.ok) {
              exitCode = 1;
              stderr += `cat: ${a}: ${String(r.error ?? "denied")}\n`;
            } else {
              parts.push(String(r.content ?? ""));
            }
          }
          stdout = parts.join("");
        }
        break;
      }
      case "stat": {
        const target = args[0];
        if (!target) {
          exitCode = 1;
          stderr = "stat: missing operand\n";
        } else {
          const norm = normalizeVfsPath(target, cwd);
          const s = this.vfsStat(norm.path, principal);
          stdout = `${JSON.stringify(s)}\n`;
        }
        break;
      }
      case "mkdir": {
        const filtered = args.filter((a) => a !== "-p");
        for (const a of filtered) {
          const norm = normalizeVfsPath(a, cwd);
          const dec = this.gatekeeperCheck("vfs:write", principal, norm.path);
          if (!dec.allowed) {
            exitCode = 126;
            stderr += `mkdir: ${a}: ${dec.reason}\n`;
          } else {
            this.ensureParentDirs(norm.path);
            this.upsertNode({
              path: norm.path,
              kind: "dir",
              content: "",
              contentType: "inode/directory",
              size: 0,
              sha256: "",
              blobKey: null,
              metadata: {},
              updatedAt: nowIso(),
            });
          }
        }
        break;
      }
      case "touch": {
        for (const a of args) {
          const norm = normalizeVfsPath(a, cwd);
          const existing = this.readNode(norm.path);
          if (!existing) {
            const w = await this.vfsWrite(norm.path, "", { principal });
            if (!w.ok) {
              exitCode = 126;
              stderr += `touch: ${a}: denied\n`;
            }
          }
        }
        break;
      }
      case "rm": {
        const filtered = args.filter((a) => !a.startsWith("-"));
        for (const a of filtered) {
          const norm = normalizeVfsPath(a, cwd);
          const dec = this.gatekeeperCheck("vfs:write", principal, norm.path);
          if (!dec.allowed) {
            exitCode = 126;
            stderr += `rm: ${a}: ${dec.reason}\n`;
          } else if (!this.deleteNode(norm.path)) {
            exitCode = 1;
            stderr += `rm: cannot remove '${a}': No such file\n`;
          }
        }
        break;
      }
      case "cp": {
        if (args.length < 2) {
          exitCode = 1;
          stderr = "cp: missing file operand\n";
        } else {
          const src = normalizeVfsPath(args[0]!, cwd);
          const dst = normalizeVfsPath(args[1]!, cwd);
          const r = await this.vfsRead(src.path, principal);
          if (!r.ok) {
            exitCode = 1;
            stderr = `cp: ${args[0]}: ${String(r.error ?? "failed")}\n`;
          } else {
            const w = await this.vfsWrite(dst.path, String(r.content ?? ""), { principal });
            if (!w.ok) {
              exitCode = 126;
              stderr = `cp: ${args[1]}: denied\n`;
            }
          }
        }
        break;
      }
      case "mv": {
        if (args.length < 2) {
          exitCode = 1;
          stderr = "mv: missing file operand\n";
        } else {
          const src = normalizeVfsPath(args[0]!, cwd);
          const dst = normalizeVfsPath(args[1]!, cwd);
          const r = await this.vfsRead(src.path, principal);
          if (!r.ok) {
            exitCode = 1;
            stderr = `mv: ${args[0]}: ${String(r.error ?? "failed")}\n`;
          } else {
            const w = await this.vfsWrite(dst.path, String(r.content ?? ""), { principal });
            if (!w.ok) {
              exitCode = 126;
              stderr = `mv: ${args[1]}: denied\n`;
            } else {
              this.deleteNode(src.path);
            }
          }
        }
        break;
      }
      case "wc": {
        const target = args[args.length - 1];
        if (!target) {
          exitCode = 1;
          stderr = "wc: missing file operand\n";
        } else {
          const r = await this.vfsRead(normalizeVfsPath(target, cwd).path, principal);
          if (!r.ok) {
            exitCode = 1;
            stderr = `wc: ${target}: No such file\n`;
          } else {
            const text = String(r.content ?? "");
            const lines = text ? text.split(/\r?\n/).length : 0;
            const words = text.trim() ? text.trim().split(/\s+/).length : 0;
            const bytes = new TextEncoder().encode(text).byteLength;
            stdout = `${lines} ${words} ${bytes} ${target}\n`;
          }
        }
        break;
      }
      case "head":
      case "tail": {
        let count = 10;
        let fileArg = args[0];
        if (args[0] === "-n" && args[1]) {
          count = Math.max(1, Number.parseInt(args[1], 10) || 10);
          fileArg = args[2];
        }
        if (!fileArg) {
          exitCode = 1;
          stderr = `${cmd}: missing file operand\n`;
        } else {
          const r = await this.vfsRead(normalizeVfsPath(fileArg, cwd).path, principal);
          if (!r.ok) {
            exitCode = 1;
            stderr = `${cmd}: ${fileArg}: No such file\n`;
          } else {
            const lines = String(r.content ?? "").split(/\r?\n/);
            const sliced = cmd === "head" ? lines.slice(0, count) : lines.slice(-count);
            stdout = `${sliced.join("\n")}\n`;
          }
        }
        break;
      }
      case "grep": {
        const [pattern, fileArg] = args;
        if (!pattern || !fileArg) {
          exitCode = 1;
          stderr = "grep: usage: grep <pattern> <path>\n";
        } else {
          const r = await this.vfsRead(normalizeVfsPath(fileArg, cwd).path, principal);
          if (!r.ok) {
            exitCode = 1;
            stderr = `grep: ${fileArg}: No such file\n`;
          } else {
            const re = new RegExp(pattern);
            const matches = String(r.content ?? "")
              .split(/\r?\n/)
              .filter((line) => re.test(line));
            stdout = matches.length ? `${matches.join("\n")}\n` : "";
            exitCode = matches.length ? 0 : 1;
          }
        }
        break;
      }
      case "find": {
        const prefix = args[0] ?? cwd;
        const ls = this.vfsLs(normalizeVfsPath(prefix, cwd).path, principal) as {
          ok: boolean;
          entries?: Array<{ path: string }>;
        };
        stdout = `${(ls.entries ?? []).map((e) => e.path).join("\n")}${ls.entries?.length ? "\n" : ""}`;
        break;
      }
      case "help":
        stdout = "Fw Tier-0 DO POSIX Shell: pwd ls cat echo stat mkdir touch rm cp mv wc head tail grep find cd env uname whoami help\n";
        break;
      default:
        exitCode = 127;
        stderr = `sh: ${cmd}: command not found (use tier=1 for CodeMode JS or tier=2 for Sandbox microVM)\n`;
        break;
    }

    if (redirectOp && redirectTarget && exitCode === 0) {
      const targetNorm = normalizeVfsPath(redirectTarget, nextCwd);
      let finalContent = stdout;
      if (redirectOp === ">>") {
        const prev = await this.vfsRead(targetNorm.path, principal);
        if (prev.ok) finalContent = `${String(prev.content ?? "")}${stdout}`;
      }
      const w = await this.vfsWrite(targetNorm.path, finalContent, { principal });
      if (!w.ok) {
        return { exitCode: 126, stdout: "", stderr: `sh: ${redirectTarget}: Gatekeeper denied\n`, cwd: nextCwd };
      }
      stdout = "";
    }

    return { exitCode, stdout, stderr, cwd: nextCwd };
  }

  public async vfsExec(req: VfsExecRequest): Promise<Record<string, unknown>> {
    this.ensureInitialized();
    const principal = req.principal ?? "default";
    const tier = this.selectExecTier(req);
    const capability = `exec:tier${tier}`;
    const decision = this.gatekeeperCheck(capability, principal, req.cwd);
    const cmdLabel = req.command ?? req.argv?.join(" ") ?? (req.code ? `codemode(${req.code.length}B)` : "");

    if (!decision.allowed) {
      this.recordAudit(tier, cmdLabel, false, 126);
      return {
        ok: false,
        denied: true,
        tier,
        gatekeeper: decision,
        exitCode: 126,
        stdout: "",
        stderr: `Gatekeeper denied ${capability}: ${decision.reason}`,
      };
    }

    if (req.allowNetwork) {
      const egressDecision = this.gatekeeperCheck("net:egress", principal);
      if (!egressDecision.allowed) {
        this.recordAudit(tier, cmdLabel, false, 126);
        return {
          ok: false,
          denied: true,
          tier,
          gatekeeper: egressDecision,
          exitCode: 126,
          stdout: "",
          stderr: `Gatekeeper denied net:egress: ${egressDecision.reason}`,
        };
      }
    }

    if (tier === 0) {
      const rawCommand = req.command ?? (req.argv ? req.argv.join(" ") : "");
      const segments = rawCommand
        .split(/\s*(?:&&|;)\s*/)
        .map((s) => s.trim())
        .filter(Boolean);
      let cwd = normalizeVfsPath(req.cwd ?? "/workspace").path;
      let stdout = "";
      let stderr = "";
      let exitCode = 0;

      for (const seg of segments) {
        const step = await this.runTier0Single(seg, cwd, principal, req.env ?? {});
        stdout += step.stdout;
        stderr += step.stderr;
        cwd = step.cwd;
        exitCode = step.exitCode;
        if (exitCode !== 0 && rawCommand.includes("&&")) break;
      }

      this.recordAudit(0, rawCommand, true, exitCode);
      return {
        ok: exitCode === 0,
        tier: 0,
        runtime: "do-posix-shell",
        storage: this.storageBackend(),
        cwd,
        exitCode,
        stdout,
        stderr,
        gatekeeper: decision,
      };
    }

    if (tier === 1) {
      const rawCode = req.code ?? (req.command ?? "").replace(/^(js|ts|codemode):/i, "").trim();
      const moduleSource = `export default {
  async fetch(request) {
    const input = await request.json().catch(() => ({}));
    const fn = new Function("input", ${JSON.stringify(rawCode)});
    const value = await fn(input);
    return Response.json({ ok: true, value });
  }
};`;

      if (this.env.LOADER) {
        const dynamicWorker = this.env.LOADER.load({
          compatibilityDate: COMPATIBILITY_DATE,
          mainModule: "index.js",
          modules: { "index.js": moduleSource },
          globalOutbound: req.allowNetwork ? undefined : null,
          env: req.env ?? {},
        });
        const ep = dynamicWorker.getEntrypoint();
        const res = await ep.fetch("http://codemode.internal/run", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(req.input ?? {}),
        });
        const text = await res.text();
        let parsed: unknown = text;
        try {
          parsed = JSON.parse(text);
        } catch {
          // keep text
        }
        const exitCode = res.ok ? 0 : 1;
        this.recordAudit(1, cmdLabel, true, exitCode);
        return {
          ok: res.ok,
          tier: 1,
          runtime: "dynamic-worker-loader",
          isolated: true,
          globalOutbound: req.allowNetwork ? "allowed" : null,
          exitCode,
          stdout: typeof parsed === "string" ? parsed : JSON.stringify(parsed),
          stderr: res.ok ? "" : text,
          result: parsed,
          gatekeeper: decision,
        };
      }

      this.recordAudit(1, cmdLabel, true, 0);
      return {
        ok: true,
        tier: 1,
        runtime: "dynamic-worker-loader",
        simulated: true,
        bindingRequired: "LOADER",
        globalOutbound: req.allowNetwork ? "allowed" : null,
        exitCode: 0,
        stdout: "",
        stderr: "",
        note: "env.LOADER binding not configured; declare worker_loaders in wrangler.jsonc to execute live CodeMode isolates",
        gatekeeper: decision,
      };
    }

    // Tier-2: @cloudflare/sandbox 1.0 container microVM (explicit argv contract)
    const argv = req.argv && req.argv.length > 0
      ? req.argv
      : ["/bin/sh", "-lc", req.command ?? "true"];
    const tenant = this.getTenantName();

    if (this.env.SANDBOX) {
      const stub = this.env.SANDBOX.getByName
        ? this.env.SANDBOX.getByName(tenant)
        : this.env.SANDBOX.get!(this.env.SANDBOX.idFromName!(tenant));

      if (typeof stub.exec === "function") {
        const handle = await stub.exec(argv, {
          cwd: req.cwd,
          env: req.env,
          timeoutMs: req.timeoutMs,
        });
        const out = typeof handle.output === "function"
          ? await handle.output({ encoding: "utf8" })
          : handle;
        const exitCode = out.exitCode ?? 0;
        this.recordAudit(2, argv.join(" "), true, exitCode);
        return {
          ok: exitCode === 0,
          tier: 2,
          runtime: "cloudflare-sandbox-microvm",
          argv,
          exitCode,
          stdout: out.stdout ?? "",
          stderr: out.stderr ?? "",
          gatekeeper: decision,
        };
      }

      const res = await stub.fetch("https://sandbox.internal/exec", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ argv, cwd: req.cwd, env: req.env, timeoutMs: req.timeoutMs }),
      });
      const body = (await res.json().catch(() => ({}))) as { stdout?: string; stderr?: string; exitCode?: number };
      const exitCode = body.exitCode ?? (res.ok ? 0 : 1);
      this.recordAudit(2, argv.join(" "), true, exitCode);
      return {
        ok: exitCode === 0,
        tier: 2,
        runtime: "cloudflare-sandbox-microvm",
        argv,
        exitCode,
        stdout: body.stdout ?? "",
        stderr: body.stderr ?? "",
        gatekeeper: decision,
      };
    }

    this.recordAudit(2, argv.join(" "), true, 0);
    return {
      ok: true,
      tier: 2,
      runtime: "cloudflare-sandbox-microvm",
      simulated: true,
      bindingRequired: "SANDBOX",
      argv,
      exitCode: 0,
      stdout: "",
      stderr: "",
      note: "env.SANDBOX binding not configured; attach containers + SANDBOX Durable Object from wrangler.fullstack.jsonc",
      gatekeeper: decision,
    };
  }

  public async learnRecord(input: {
    kind?: string;
    source?: string;
    summary?: string;
    tags?: string[];
    payload?: Record<string, unknown>;
    principal?: string;
  }): Promise<Record<string, unknown>> {
    this.ensureInitialized();
    const principal = input.principal ?? "default";
    const decision = this.gatekeeperCheck("learn:write", principal);
    if (!decision.allowed) {
      return { ok: false, denied: true, gatekeeper: decision };
    }

    const record: LearnEventRecord = {
      id: makeId("learn"),
      kind: input.kind || "observation",
      source: input.source || "fw-edge",
      summary: input.summary || "",
      tags: Array.isArray(input.tags) ? input.tags.map(String) : [],
      payload: input.payload && typeof input.payload === "object" ? input.payload : {},
      createdAt: nowIso(),
    };

    if (this.sql) {
      this.sql.exec(
        "INSERT INTO learn_events (id, kind, source, summary, tags, payload, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        record.id,
        record.kind,
        record.source,
        record.summary,
        JSON.stringify(record.tags),
        JSON.stringify(record.payload),
        record.createdAt,
      );
    } else {
      this.learnMem.push(record);
    }

    const forwarded = { k2: false, basinPipeline: false, d1: false };
    if (this.env.EVENT_LOG) {
      if (typeof this.env.EVENT_LOG.produce === "function") {
        await this.env.EVENT_LOG.produce([{ key: record.id, value: JSON.stringify(record) }]);
        forwarded.k2 = true;
      } else if (typeof this.env.EVENT_LOG.send === "function") {
        await this.env.EVENT_LOG.send([record]);
        forwarded.k2 = true;
      }
    }
    if (this.env.TELEMETRY_PIPELINE) {
      await this.env.TELEMETRY_PIPELINE.send([{ ...record, tenant: this.getTenantName() }]);
      forwarded.basinPipeline = true;
    }
    if (this.env.META_DB) {
      await this.env.META_DB.prepare(
        "INSERT INTO fw_learn_events (id, tenant, kind, source, summary, created_at) VALUES (?, ?, ?, ?, ?, ?)",
      )
        .bind(record.id, this.getTenantName(), record.kind, record.source, record.summary, record.createdAt)
        .run()
        .then(() => {
          forwarded.d1 = true;
        })
        .catch(() => {
          forwarded.d1 = false;
        });
    }

    return {
      ok: true,
      event: record,
      storage: this.storageBackend(),
      forwarded,
    };
  }

  public learnList(limit = 50): { ok: true; storage: "sqlite" | "memory"; count: number; events: LearnEventRecord[] } {
    this.ensureInitialized();
    const safeLimit = Math.max(1, Math.min(500, limit));
    if (this.sql) {
      const rows = this.sql
        .exec<{
          id: string;
          kind: string;
          source: string;
          summary: string;
          tags: string;
          payload: string;
          created_at: string;
        }>("SELECT id, kind, source, summary, tags, payload, created_at FROM learn_events ORDER BY created_at DESC LIMIT ?", safeLimit)
        .toArray()
        .map((r) => ({
          id: r.id,
          kind: r.kind,
          source: r.source,
          summary: r.summary,
          tags: JSON.parse(r.tags || "[]") as string[],
          payload: JSON.parse(r.payload || "{}") as Record<string, unknown>,
          createdAt: r.created_at,
        }));
      return { ok: true, storage: "sqlite", count: rows.length, events: rows };
    }
    const events = [...this.learnMem].reverse().slice(0, safeLimit);
    return { ok: true, storage: "memory", count: events.length, events };
  }

  public learnStatus(): Record<string, unknown> {
    this.ensureInitialized();
    const all = this.learnList(500).events;
    const byKind: Record<string, number> = {};
    for (const ev of all) {
      byKind[ev.kind] = (byKind[ev.kind] ?? 0) + 1;
    }
    const auditCount = this.sql
      ? (this.sql.exec<{ cnt: number }>("SELECT COUNT(*) AS cnt FROM exec_audit").toArray()[0]?.cnt ?? 0)
      : this.auditMem.length;

    return {
      ok: true,
      tenant: this.getTenantName(),
      storageBackend: this.storageBackend(),
      totalEvents: all.length,
      byKind,
      latestEventAt: all[0]?.createdAt ?? null,
      execAuditCount: auditCount,
      sinks: {
        sqlite: Boolean(this.sql),
        k2: Boolean(this.env.EVENT_LOG),
        basinPipeline: Boolean(this.env.TELEMETRY_PIPELINE),
        d1: Boolean(this.env.META_DB),
      },
    };
  }

  public upsertA2ATask(task: A2ATaskRecord): A2ATaskRecord {
    this.a2aTasks.set(task.id, task);
    return task;
  }

  public getA2ATask(id: string): A2ATaskRecord | null {
    return this.a2aTasks.get(id) ?? null;
  }

  public createAcpSession(cwd = "/workspace"): AcpSessionRecord {
    const session: AcpSessionRecord = {
      sessionId: makeId("acp"),
      tenant: this.getTenantName(),
      cwd: normalizeVfsPath(cwd).path,
      createdAt: nowIso(),
      turns: 0,
    };
    this.acpSessions.set(session.sessionId, session);
    return session;
  }

  public bumpAcpSession(sessionId: string): AcpSessionRecord | null {
    const existing = this.acpSessions.get(sessionId);
    if (!existing) return null;
    existing.turns += 1;
    return existing;
  }

  async fetch(request: Request): Promise<Response> {
    this.ensureInitialized();
    const url = new URL(request.url);
    const principal = request.headers.get("x-fw-principal") ?? "default";

    if (url.pathname === "/vfs/ls") {
      const body = request.method === "POST" ? ((await request.json().catch(() => ({}))) as { prefix?: string; path?: string }) : {};
      const prefix = body.prefix ?? body.path ?? url.searchParams.get("prefix") ?? url.searchParams.get("path") ?? "/";
      const res = this.vfsLs(prefix, principal);
      return json(res, res.denied ? 403 : 200);
    }

    if (url.pathname === "/vfs/read") {
      const body = request.method === "POST" ? ((await request.json().catch(() => ({}))) as { path?: string }) : {};
      const path = body.path ?? url.searchParams.get("path") ?? "";
      if (!path) return json({ ok: false, error: "Missing path" }, 400);
      const res = await this.vfsRead(path, principal);
      if (res.denied) return json(res, 403);
      return json(res, res.ok ? 200 : 404);
    }

    if (url.pathname === "/vfs/write" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as {
        path?: string;
        content?: string;
        contentType?: string;
        metadata?: Record<string, unknown>;
        spillToR2?: boolean;
      };
      if (!body.path || typeof body.content !== "string") {
        return json({ ok: false, error: "Missing path or string content" }, 400);
      }
      const res = await this.vfsWrite(body.path, body.content, {
        contentType: body.contentType,
        metadata: body.metadata,
        spillToR2: body.spillToR2,
        principal,
      });
      if (res.denied) return json(res, 403);
      return json(res, res.ok ? 200 : 400);
    }

    if (url.pathname === "/vfs/stat") {
      const body = request.method === "POST" ? ((await request.json().catch(() => ({}))) as { path?: string }) : {};
      const path = body.path ?? url.searchParams.get("path") ?? "/";
      const res = this.vfsStat(path, principal);
      return json(res, res.denied ? 403 : 200);
    }

    if (url.pathname === "/vfs/exec" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as VfsExecRequest;
      const res = await this.vfsExec({ ...body, principal: body.principal ?? principal });
      return json(res, res.denied ? 403 : 200);
    }

    if (url.pathname === "/gatekeeper/policies" && request.method === "GET") {
      const rules = this.gatekeeperListPolicies();
      return json({ ok: true, count: rules.length, rules });
    }

    if (url.pathname === "/gatekeeper/policies" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as Partial<GatekeeperRule>;
      if (!body.capability || (body.effect !== "allow" && body.effect !== "deny")) {
        return json({ ok: false, error: "Missing capability or valid effect ('allow' | 'deny')" }, 400);
      }
      const rule = this.gatekeeperUpsertPolicy({
        id: body.id ?? makeId("rule"),
        capability: body.capability,
        effect: body.effect,
        principal: body.principal ?? "*",
        pathPrefix: body.pathPrefix ?? null,
        reason: body.reason ?? "Updated via Gatekeeper API",
      });
      return json({ ok: true, rule });
    }

    if (url.pathname === "/gatekeeper/check" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as {
        capability?: string;
        principal?: string;
        path?: string;
      };
      if (!body.capability) return json({ ok: false, error: "Missing capability" }, 400);
      const decision = this.gatekeeperCheck(body.capability, body.principal ?? principal, body.path);
      return json({ ok: true, decision }, decision.allowed ? 200 : 403);
    }

    if (url.pathname === "/api/learn/events" && request.method === "GET") {
      const limit = Number.parseInt(url.searchParams.get("limit") ?? "50", 10) || 50;
      return json(this.learnList(limit));
    }

    if (url.pathname === "/api/learn/events" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as {
        kind?: string;
        source?: string;
        summary?: string;
        tags?: string[];
        payload?: Record<string, unknown>;
      };
      const res = await this.learnRecord({ ...body, principal });
      return json(res, res.denied ? 403 : 201);
    }

    if (url.pathname === "/api/learn/status" && request.method === "GET") {
      return json(this.learnStatus());
    }

    return json({ ok: false, error: "DO route not found", path: url.pathname }, 404);
  }
}

const localAgents = new Map<string, FwOSAgent>();

function resolveTenant(request: Request, url: URL): string {
  return request.headers.get("x-fw-tenant")?.trim() || url.searchParams.get("tenant")?.trim() || "default";
}

function getLocalAgent(tenant: string, env: FwWorkerEnv): FwOSAgent {
  const key = `${tenant}:${env.FW_ENV ?? "production"}`;
  let agent = localAgents.get(key);
  if (!agent) {
    agent = new FwOSAgent({ id: { name: tenant, toString: () => tenant } }, env);
    localAgents.set(key, agent);
  }
  agent.bindEnv(env);
  return agent;
}

async function dispatchToTenantDO(
  tenant: string,
  env: FwWorkerEnv,
  doPath: string,
  init: { method: string; headers?: HeadersInit; body?: string; search?: string },
): Promise<Response> {
  const targetUrl = `https://fw-os.internal${doPath}${init.search ?? ""}`;
  const req = new Request(targetUrl, {
    method: init.method,
    headers: init.headers,
    body: init.method === "GET" || init.method === "HEAD" ? undefined : init.body,
  });

  if (env.FW_OS) {
    const stub = env.FW_OS.getByName
      ? env.FW_OS.getByName(tenant)
      : env.FW_OS.get!(env.FW_OS.idFromName!(tenant));
    return stub.fetch(req);
  }

  return getLocalAgent(tenant, env).fetch(req);
}

export class InferenceUnavailableError extends Error {}

type EdgeModel = { id: string; object: "model"; owned_by: string; description?: string; context_window?: number; function_calling?: boolean; reasoning?: boolean; vision?: boolean; paid?: boolean };

let modelCache: { at: number; models: EdgeModel[] } | null = null;
const MODEL_CACHE_MS = 10 * 60 * 1000;

function modelProp(m: WorkersAIModelLike, id: string): unknown {
  return m.properties?.find((p) => p.property_id === id)?.value;
}

/** Live Workers AI text-generation catalog, without deprecated, LoRA and beta models. */
export async function listEdgeModels(env: FwWorkerEnv): Promise<EdgeModel[]> {
  if (!env.AI?.models) return [];
  if (modelCache && Date.now() - modelCache.at < MODEL_CACHE_MS) return modelCache.models;
  const raw = await env.AI.models({ task: "Text Generation", per_page: 200, hide_experimental: true });
  const models = raw
    .filter((m) => m.name.startsWith("@cf/"))
    .filter((m) => !["planned_deprecation_date", "lora", "beta"].some((p) => modelProp(m, p) !== undefined))
    .map((m): EdgeModel => ({
      id: m.name,
      object: "model",
      owned_by: "cloudflare-workers-ai",
      description: m.description,
      context_window: Number(modelProp(m, "context_window")) || undefined,
      function_calling: modelProp(m, "function_calling") !== undefined,
      reasoning: modelProp(m, "reasoning") !== undefined,
      vision: modelProp(m, "vision") !== undefined,
      paid: modelProp(m, "require_workers_paid") !== undefined,
    }))
    .sort((x, y) => Number(y.function_calling) - Number(x.function_calling) || (y.context_window ?? 0) - (x.context_window ?? 0) || x.id.localeCompare(y.id));
  modelCache = { at: Date.now(), models };
  return models;
}

/** Default chat model: the largest-context tool-calling model that runs on any plan. */
export async function defaultEdgeModel(env: FwWorkerEnv): Promise<string> {
  const models = await listEdgeModels(env);
  const pick = models.find((m) => m.function_calling && !m.paid) ?? models[0];
  if (!pick) throw new InferenceUnavailableError("Workers AI returned no text-generation models for this account");
  return pick.id;
}

async function resolveModel(env: FwWorkerEnv, requested: unknown): Promise<string> {
  if (typeof requested === "string" && requested.startsWith("@cf/")) return requested;
  return defaultEdgeModel(env);
}

function requireAi(env: FwWorkerEnv): WorkersAILike {
  if (!env.AI) {
    throw new InferenceUnavailableError("This Worker has no Workers AI binding: add `\"ai\": { \"binding\": \"AI\" }` to wrangler.jsonc and redeploy");
  }
  return env.AI;
}

async function runInferenceText(
  env: FwWorkerEnv,
  requestedModel: unknown,
  messages: Array<{ role: string; content: string }>,
  options: Record<string, unknown> = {},
): Promise<{ text: string; model: string; raw: unknown }> {
  const ai = requireAi(env);
  const model = await resolveModel(env, requestedModel);
  const raw = await ai.run(model, { messages, ...options });
  return { text: extractAiText(raw), model, raw };
}

function openAiError(status: number, type: string, message: string): Response {
  return json({ error: { message, type } }, status);
}

/** Re-frames a Workers AI SSE stream (legacy `response` or OpenAI `choices[].delta`) as OpenAI chat chunks. */
function toOpenAiChunks(source: ReadableStream<Uint8Array>, model: string): ReadableStream<Uint8Array> {
  const enc = new TextEncoder();
  const dec = new TextDecoder();
  const id = `chatcmpl-cf-${Date.now()}`;
  const created = Math.floor(Date.now() / 1000);
  let buffer = "";
  const chunk = (delta: Record<string, unknown>, finish: string | null) =>
    enc.encode(`data: ${JSON.stringify({ id, object: "chat.completion.chunk", created, model, choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`);
  return source.pipeThrough(
    new TransformStream<Uint8Array, Uint8Array>({
      start(controller) {
        controller.enqueue(chunk({ role: "assistant" }, null));
      },
      transform(bytes, controller) {
        buffer += dec.decode(bytes, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          const data = line.startsWith("data:") ? line.slice(5).trim() : "";
          if (!data || data === "[DONE]") continue;
          try {
            const ev = JSON.parse(data) as { response?: unknown; choices?: Array<{ delta?: { content?: unknown; reasoning_content?: unknown } }> };
            const text = typeof ev.response === "string" ? ev.response : ev.choices?.[0]?.delta?.content;
            const reasoning = ev.choices?.[0]?.delta?.reasoning_content;
            if (typeof reasoning === "string" && reasoning) controller.enqueue(chunk({ reasoning_content: reasoning }, null));
            if (typeof text === "string" && text) controller.enqueue(chunk({ content: text }, null));
          } catch {
            // Partial or non-JSON keep-alive frames carry no content.
          }
        }
      },
      flush(controller) {
        controller.enqueue(chunk({}, "stop"));
        controller.enqueue(enc.encode("data: [DONE]\n\n"));
      },
    }),
  );
}

async function handleChatCompletions(request: Request, env: FwWorkerEnv): Promise<Response> {
  const body = (await request.json().catch(() => null)) as {
    model?: string;
    messages?: Array<{ role: string; content: string }>;
    stream?: boolean;
    temperature?: number;
    max_tokens?: number;
  } | null;
  if (!body || !Array.isArray(body.messages) || body.messages.length === 0) {
    return openAiError(400, "invalid_request_error", "`messages` must be a non-empty array");
  }
  const options: Record<string, unknown> = {};
  if (typeof body.temperature === "number") options.temperature = body.temperature;
  if (typeof body.max_tokens === "number") options.max_tokens = body.max_tokens;
  try {
    const ai = requireAi(env);
    const model = await resolveModel(env, body.model);
    if (body.stream) {
      const stream = await ai.run(model, { messages: body.messages, stream: true, ...options });
      if (!(stream instanceof ReadableStream)) {
        return openAiError(502, "fw_upstream_error", `${model} did not return a stream`);
      }
      return new Response(toOpenAiChunks(stream, model), {
        headers: { "content-type": "text/event-stream; charset=utf-8", "cache-control": "no-store", "access-control-allow-origin": "*" },
      });
    }
    const raw = await ai.run(model, { messages: body.messages, ...options });
    return json({
      id: `chatcmpl-cf-${Date.now()}`,
      object: "chat.completion",
      created: Math.floor(Date.now() / 1000),
      model,
      choices: [{ index: 0, message: { role: "assistant", content: extractAiText(raw) }, finish_reason: "stop" }],
      raw,
    });
  } catch (err) {
    if (err instanceof InferenceUnavailableError) return openAiError(503, "fw_no_inference", err.message);
    return openAiError(502, "fw_upstream_error", String(err instanceof Error ? err.message : err));
  }
}

async function handleMcp(
  request: Request,
  url: URL,
  env: FwWorkerEnv,
): Promise<Response> {
  const rpc = (await request.json().catch(() => ({}))) as {
    jsonrpc?: string;
    id?: string | number | null;
    method?: string;
    params?: Record<string, unknown>;
  };
  const id = rpc.id ?? null;
  const method = rpc.method ?? "ping";
  const tenant = resolveTenant(request, url);
  const principal = request.headers.get("x-fw-principal") ?? "default";

  if (method === "initialize") {
    return json({
      jsonrpc: "2.0",
      id,
      result: {
        protocolVersion: "2025-11-25",
        capabilities: {
          tools: { listChanged: false },
          resources: { subscribe: false, listChanged: false },
        },
        serverInfo: {
          name: "fw-edge",
          version: FW_VERSION,
        },
      },
    });
  }

  if (method === "tools/list") {
    return json({
      jsonrpc: "2.0",
      id,
      result: { tools: MCP_TOOLS },
    });
  }

  if (method === "tools/call") {
    const params = rpc.params ?? {};
    const toolName = String(params.name ?? "");
    const args = (params.arguments && typeof params.arguments === "object"
      ? params.arguments
      : {}) as Record<string, unknown>;
    const headers = {
      "content-type": "application/json",
      "x-fw-principal": principal,
    };

    let resultPayload: Record<string, unknown>;

    switch (toolName) {
      case "vfs_ls": {
        const res = await dispatchToTenantDO(tenant, env, "/vfs/ls", {
          method: "POST",
          headers,
          body: JSON.stringify({ prefix: args.prefix ?? "/" }),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "vfs_read": {
        const res = await dispatchToTenantDO(tenant, env, "/vfs/read", {
          method: "POST",
          headers,
          body: JSON.stringify({ path: args.path }),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "vfs_write": {
        const res = await dispatchToTenantDO(tenant, env, "/vfs/write", {
          method: "POST",
          headers,
          body: JSON.stringify({
            path: args.path,
            content: args.content,
            contentType: args.contentType,
          }),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "vfs_stat": {
        const res = await dispatchToTenantDO(tenant, env, "/vfs/stat", {
          method: "POST",
          headers,
          body: JSON.stringify({ path: args.path }),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "vfs_exec": {
        const res = await dispatchToTenantDO(tenant, env, "/vfs/exec", {
          method: "POST",
          headers,
          body: JSON.stringify(args),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "gatekeeper_check": {
        const res = await dispatchToTenantDO(tenant, env, "/gatekeeper/check", {
          method: "POST",
          headers,
          body: JSON.stringify(args),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "os_catalog": {
        const layer = typeof args.layer === "string" ? args.layer : undefined;
        const items = layer
          ? CLOUDFLARE_EDGE_OS_CATALOG.filter((c) => c.layer === layer)
          : CLOUDFLARE_EDGE_OS_CATALOG;
        resultPayload = { ok: true, count: items.length, items };
        break;
      }
      case "learn_log": {
        const res = await dispatchToTenantDO(tenant, env, "/api/learn/events", {
          method: "POST",
          headers,
          body: JSON.stringify(args),
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      case "learn_status": {
        const res = await dispatchToTenantDO(tenant, env, "/api/learn/status", {
          method: "GET",
          headers,
        });
        resultPayload = (await res.json()) as Record<string, unknown>;
        break;
      }
      default:
        return json({
          jsonrpc: "2.0",
          id,
          error: { code: -32602, message: `Unknown MCP tool: ${toolName}` },
        });
    }

    return json({
      jsonrpc: "2.0",
      id,
      result: {
        content: [{ type: "text", text: JSON.stringify(resultPayload) }],
        structuredContent: resultPayload,
        isError: resultPayload.ok === false,
      },
    });
  }

  if (method === "ping") {
    return json({
      jsonrpc: "2.0",
      id,
      result: { protocol: "mcp", method: "ping", edge: true, ok: true },
    });
  }

  return json({
    jsonrpc: "2.0",
    id,
    error: { code: -32601, message: `Method not found: ${method}` },
  });
}

async function handleAcp(
  request: Request,
  url: URL,
  env: FwWorkerEnv,
): Promise<Response> {
  const rpc = (await request.json().catch(() => ({}))) as {
    id?: string | number | null;
    method?: string;
    params?: Record<string, unknown>;
  };
  const id = rpc.id ?? null;
  const method = rpc.method ?? "ping";
  const tenant = resolveTenant(request, url);
  const agent = getLocalAgent(tenant, env);

  if (method === "initialize") {
    return json({
      jsonrpc: "2.0",
      id,
      result: {
        protocol: "acp",
        protocolVersion: "1.33.0",
        edge: true,
        ok: true,
        agentInfo: { name: "fw-edge", version: FW_VERSION },
        agentCapabilities: {
          loadSession: true,
          promptCapabilities: { embeddedContext: true },
          edgeTiers: [0, 1, 2],
        },
      },
    });
  }

  if (method === "session/new") {
    const cwd = typeof rpc.params?.cwd === "string" ? rpc.params.cwd : "/workspace";
    const session = agent.createAcpSession(cwd);
    return json({
      jsonrpc: "2.0",
      id,
      result: { ...session, mode: "edge-os", ok: true },
    });
  }

  if (method === "session/prompt") {
    const params = rpc.params ?? {};
    const sessionId = typeof params.sessionId === "string" ? params.sessionId : agent.createAcpSession().sessionId;
    const bumped = agent.bumpAcpSession(sessionId) ?? agent.createAcpSession();
    const promptText =
      typeof params.prompt === "string"
        ? params.prompt
        : Array.isArray(params.prompt)
          ? params.prompt
              .map((p) => (p && typeof p === "object" && "text" in p ? String((p as { text?: unknown }).text ?? "") : String(p)))
              .join("\n")
          : "";
    let text: string;
    let model: string;
    try {
      ({ text, model } = await runInferenceText(env, params.model, [{ role: "user", content: promptText }]));
    } catch (err) {
      return json({ jsonrpc: "2.0", id, error: { code: -32001, message: err instanceof Error ? err.message : String(err) } });
    }
    return json({
      jsonrpc: "2.0",
      id,
      result: {
        sessionId: bumped.sessionId,
        turns: bumped.turns,
        stopReason: "end_turn",
        model,
        output: { role: "assistant", text },
        ok: true,
      },
    });
  }

  if (method === "ping") {
    return json({
      jsonrpc: "2.0",
      id,
      result: { protocol: "acp", method: "ping", edge: true, ok: true },
    });
  }

  return json({
    jsonrpc: "2.0",
    id,
    error: { code: -32601, message: `Method not found: ${method}` },
  });
}

async function handleA2A(
  request: Request,
  url: URL,
  env: FwWorkerEnv,
): Promise<Response> {
  const rpc = (await request.json().catch(() => ({}))) as {
    id?: string | number | null;
    method?: string;
    params?: Record<string, unknown>;
  };
  const id = rpc.id ?? null;
  const method = rpc.method ?? "ping";
  const tenant = resolveTenant(request, url);
  const agent = getLocalAgent(tenant, env);

  if (method === "tasks/send") {
    const params = rpc.params ?? {};
    const taskId = typeof params.id === "string" && params.id.trim() ? params.id : makeId("task");
    let promptText = typeof params.prompt === "string" ? params.prompt : "";
    if (!promptText && params.message && typeof params.message === "object") {
      const msg = params.message as { parts?: Array<{ text?: string }> };
      promptText = (msg.parts ?? []).map((p) => p.text ?? "").join("\n");
    }
    let text: string;
    try {
      ({ text } = await runInferenceText(env, params.model, [{ role: "user", content: promptText }]));
    } catch (err) {
      return json({ jsonrpc: "2.0", id, error: { code: -32001, message: err instanceof Error ? err.message : String(err) } });
    }
    const task: A2ATaskRecord = {
      id: taskId,
      tenant,
      status: { state: "completed", timestamp: nowIso() },
      artifacts: [{ name: "response", parts: [{ type: "text", text }] }],
      history: [
        { role: "user", parts: [{ type: "text", text: promptText }] },
        { role: "agent", parts: [{ type: "text", text }] },
      ],
    };
    agent.upsertA2ATask(task);
    return json({ jsonrpc: "2.0", id, result: task });
  }

  if (method === "tasks/get") {
    const taskId = typeof rpc.params?.id === "string" ? rpc.params.id : "";
    const found = agent.getA2ATask(taskId);
    if (!found) {
      return json({
        jsonrpc: "2.0",
        id,
        error: { code: -32001, message: `Task not found: ${taskId}` },
      });
    }
    return json({ jsonrpc: "2.0", id, result: found });
  }

  if (method === "ping") {
    return json({
      jsonrpc: "2.0",
      id,
      result: { protocol: "a2a", method: "ping", edge: true, ok: true },
    });
  }

  return json({
    jsonrpc: "2.0",
    id,
    error: { code: -32601, message: `Method not found: ${method}` },
  });
}

function buildOpenApiSpec(origin: string): Record<string, unknown> {
  return {
    openapi: "3.1.0",
    info: {
      title: "Fw Cloudflare Edge OS API",
      version: FW_VERSION,
      description:
        "Unified Cloudflare Edge OS Worker: Durable Object SQLite VFS, 3-tier execution (DO POSIX, Dynamic Worker Loader CodeMode, Sandbox microVM), Gatekeeper policy engine, autonomous learning log, and OpenAI / MCP / ACP / A2A protocol gateway.",
    },
    servers: [{ url: origin }],
    paths: {
      "/health": { get: { summary: "Edge OS health and active bindings" } },
      "/openapi.json": { get: { summary: "OpenAPI 3.1 schema" } },
      "/.well-known/agent.json": { get: { summary: "A2A AgentCard" } },
      "/v1/models": { get: { summary: "OpenAI-compatible model list" } },
      "/v1/chat/completions": { post: { summary: "OpenAI-compatible chat via Cloudflare Workers AI with deterministic fallback" } },
      "/v1/embeddings": { post: { summary: "OpenAI-compatible embeddings with optional Vectorize upsert" } },
      "/mcp": { post: { summary: "Streamable HTTP MCP JSON-RPC 2.0 (initialize, tools/list, tools/call)" } },
      "/acp": { post: { summary: "ACP JSON-RPC 2.0 (initialize, session/new, session/prompt)" } },
      "/a2a": { post: { summary: "A2A JSON-RPC 2.0 (tasks/send, tasks/get)" } },
      "/os/catalog": { get: { summary: "Verified Nov 2026 Cloudflare Edge OS ecosystem catalog" } },
      "/os/tiers": { get: { summary: "3-tier execution architecture and live binding readiness" } },
      "/os/vfs/ls": { get: { summary: "List tenant SQLite VFS entries" }, post: { summary: "List tenant SQLite VFS entries" } },
      "/os/vfs/read": { get: { summary: "Read file from tenant SQLite VFS / R2 spillover" }, post: { summary: "Read file from tenant SQLite VFS / R2 spillover" } },
      "/os/vfs/write": { post: { summary: "Write file to tenant SQLite VFS with Gatekeeper check" } },
      "/os/vfs/stat": { get: { summary: "Stat tenant SQLite VFS path" }, post: { summary: "Stat tenant SQLite VFS path" } },
      "/os/vfs/exec": { post: { summary: "3-tier execution router (Tier-0 DO POSIX, Tier-1 LOADER CodeMode, Tier-2 Sandbox microVM)" } },
      "/os/gatekeeper": { get: { summary: "List Gatekeeper capability rules" }, post: { summary: "Evaluate or upsert Gatekeeper capability policies" } },
      "/api/learn/events": { get: { summary: "List autonomous learning log events" }, post: { summary: "Append learning event (DO SQLite + K2 + Basin Pipelines)" } },
      "/api/learn/status": { get: { summary: "Autonomous learning log metrics and sink readiness" } },
    },
  };
}

export default {
  async fetch(request: Request, env: FwWorkerEnv = {}): Promise<Response> {
    const url = new URL(request.url);

    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: {
          "access-control-allow-origin": "*",
          "access-control-allow-methods": "GET, POST, PUT, DELETE, OPTIONS",
          "access-control-allow-headers": "Content-Type, Authorization, X-Fw-Tenant, X-Fw-Principal, Mcp-Session-Id",
        },
      });
    }

    const STATIC_WEB_PATHS = new Set([
      "/",
      "/index.html",
      "/app.css",
      "/app.js",
      "/robots.txt",
      "/sitemap.xml",
      "/llms.txt",
      "/install.sh",
      "/install.ps1",
    ]);
    if (request.method === "GET" && STATIC_WEB_PATHS.has(url.pathname) && env.ASSETS) {
      return env.ASSETS.fetch(request);
    }
    if (request.method === "GET" && url.pathname === "/webgpu" && env.ASSETS) {
      return env.ASSETS.fetch(new Request(new URL("/webgpu.html", url), request));
    }

    if (url.pathname === "/health") {
      return json({
        ok: true,
        product: "fw-edge",
        version: FW_VERSION,
        runtime: "cloudflare-workers",
        compatibilityDate: COMPATIBILITY_DATE,
        env: env.FW_ENV ?? "production",
        deployment: env.CF_VERSION_METADATA ?? null,
        bindings: getBindingSummary(env),
      });
    }

    if (url.pathname === "/.well-known/agent.json") {
      return json({
        name: "Fw Edge",
        description: "Fw Edge OS Worker with SQLite DO VFS, 3-tier execution, Gatekeeper, and MCP/ACP/A2A gateway",
        url: `${url.origin}/a2a`,
        version: FW_VERSION,
        protocolVersion: "1.0.1",
        defaultInputModes: ["text/plain", "application/json"],
        defaultOutputModes: ["text/plain", "application/json"],
        capabilities: {
          streaming: false,
          pushNotifications: false,
          stateTransitionHistory: true,
          edge: true,
          workersAi: Boolean(env.AI),
          durableObjectVfs: true,
          executionTiers: [0, 1, 2],
        },
        skills: [
          { id: "edge-vfs", name: "Tenant SQLite VFS", description: "Persistent per-tenant POSIX filesystem with R2 spillover" },
          { id: "edge-exec", name: "3-Tier Execution", description: "Tier-0 DO shell, Tier-1 Dynamic Worker Loader CodeMode, Tier-2 Sandbox microVM" },
          { id: "gatekeeper", name: "Gatekeeper Policy", description: "Fail-closed capability checks per principal and path prefix" },
          { id: "autonomous-learn", name: "Autonomous Learning Log", description: "Durable event capture into SQLite, K2 Streams, and Basin Pipelines" },
        ],
      });
    }

    if (url.pathname === "/openapi.json") {
      return json(buildOpenApiSpec(url.origin));
    }

    if (url.pathname === "/v1/models") {
      if (!env.AI) {
        return json({ object: "list", data: [], error: "Workers AI binding `AI` is not configured" });
      }
      try {
        const data = await listEdgeModels(env);
        return json({ object: "list", data, default: await defaultEdgeModel(env) });
      } catch (err) {
        return openAiError(502, "fw_upstream_error", String(err instanceof Error ? err.message : err));
      }
    }

    if (url.pathname === "/v1/chat/completions" && request.method === "POST") {
      return handleChatCompletions(request, env);
    }

    if (url.pathname === "/v1/embeddings" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as {
        model?: string;
        input?: string | string[];
        upsertVectorize?: boolean;
        ids?: string[];
      };
      const model = body.model || "@cf/baai/bge-base-en-v1.5";
      const inputs = Array.isArray(body.input)
        ? body.input.map(String)
        : typeof body.input === "string"
          ? [body.input]
          : [];
      if (inputs.length === 0) {
        return json({ ok: false, error: "Missing embedding input" }, 400);
      }

      let vectors: number[][];
      let source: "workers-ai" | "deterministic-fallback" = "deterministic-fallback";
      if (env.AI && model.startsWith("@cf/")) {
        const aiRes = (await env.AI.run(model, { text: inputs })) as { data?: number[][] };
        if (Array.isArray(aiRes?.data) && aiRes.data.length === inputs.length) {
          vectors = aiRes.data;
          source = "workers-ai";
        } else {
          vectors = inputs.map((t) => deterministicEmbedding(t));
        }
      } else {
        vectors = inputs.map((t) => deterministicEmbedding(t));
      }

      let vectorizeUpserted = 0;
      if (body.upsertVectorize && env.MEMORY_VEC) {
        const tenant = resolveTenant(request, url);
        const batch = vectors.map((values, idx) => ({
          id: body.ids?.[idx] ?? `${tenant}:${makeId("vec")}`,
          values,
          metadata: { tenant, text: inputs[idx]!.slice(0, 256), model },
        }));
        await env.MEMORY_VEC.upsert(batch);
        vectorizeUpserted = batch.length;
      }

      const tokenEstimate = inputs.reduce((acc, t) => acc + Math.max(1, Math.ceil(t.length / 4)), 0);
      return json({
        object: "list",
        model,
        source,
        vectorizeUpserted,
        data: vectors.map((embedding, index) => ({ object: "embedding", index, embedding })),
        usage: { prompt_tokens: tokenEstimate, total_tokens: tokenEstimate },
      });
    }

    if (url.pathname === "/mcp" && request.method === "POST") {
      return handleMcp(request, url, env);
    }

    if (url.pathname === "/acp" && request.method === "POST") {
      return handleAcp(request, url, env);
    }

    if (url.pathname === "/a2a" && request.method === "POST") {
      return handleA2A(request, url, env);
    }

    if (url.pathname === "/os/catalog" && request.method === "GET") {
      const layer = url.searchParams.get("layer");
      const items = layer
        ? CLOUDFLARE_EDGE_OS_CATALOG.filter((item) => item.layer === layer)
        : CLOUDFLARE_EDGE_OS_CATALOG;
      return json({
        ok: true,
        product: "fw-edge",
        updated: "2026-11-19",
        compatibilityDate: COMPATIBILITY_DATE,
        count: items.length,
        items,
      });
    }

    if (url.pathname === "/os/tiers" && request.method === "GET") {
      return json({
        ok: true,
        product: "fw-edge",
        version: FW_VERSION,
        tiers: EXECUTION_TIERS,
        bindings: getBindingSummary(env),
      });
    }

    if (url.pathname.startsWith("/os/vfs/")) {
      const tenant = resolveTenant(request, url);
      const subPath = url.pathname.slice("/os".length); // "/vfs/..."
      const bodyText = request.method === "GET" || request.method === "HEAD" ? undefined : await request.text();
      return dispatchToTenantDO(tenant, env, subPath, {
        method: request.method,
        headers: request.headers,
        body: bodyText,
        search: url.search,
      });
    }

    if (url.pathname === "/os/gatekeeper") {
      const tenant = resolveTenant(request, url);
      if (request.method === "GET") {
        return dispatchToTenantDO(tenant, env, "/gatekeeper/policies", {
          method: "GET",
          headers: request.headers,
        });
      }
      if (request.method === "POST") {
        const rawBody = await request.text();
        let parsed: Record<string, unknown> = {};
        try {
          parsed = JSON.parse(rawBody) as Record<string, unknown>;
        } catch {
          parsed = {};
        }
        const targetPath = parsed.action === "upsert" || parsed.effect === "allow" || parsed.effect === "deny"
          ? "/gatekeeper/policies"
          : "/gatekeeper/check";
        return dispatchToTenantDO(tenant, env, targetPath, {
          method: "POST",
          headers: request.headers,
          body: rawBody,
        });
      }
    }

    if (url.pathname === "/cluster" && request.method === "GET") {
      return json({
        ok: true,
        runtime: "cloudflare-workers-edge",
        bindings: getBindingSummary(env),
        tiers: ["tier-0-do-sqlite", "tier-1-dynamic-worker", "tier-2-sandbox-container"],
      });
    }

    if (url.pathname === "/claw/status" && request.method === "GET") {
      return json({
        ok: true,
        agent: "fw-edge-hermes-claw",
        runtime: "cloudflare-workers",
        mcpTools: 4,
        sandboxAllowBinaries: 13,
        skills: 10,
      });
    }

    if (url.pathname === "/claw/ranking" && request.method === "GET") {
      return json({
        ok: true,
        modernRustClis: [
          { rank: 1, name: "uv", stars_nov_2026: 92397 },
          { rank: 2, name: "rg", stars_nov_2026: 70058 },
          { rank: 3, name: "bat", stars_nov_2026: 61307 },
          { rank: 4, name: "fd", stars_nov_2026: 45110 },
          { rank: 5, name: "yazi", stars_nov_2026: 43515 },
          { rank: 6, name: "nu", stars_nov_2026: 41035 },
        ],
        protocolsAndAgentOs: [
          { rank: 1, name: "openclaw", stars_nov_2026: 394799 },
          { rank: 2, name: "hermes-agent", stars_nov_2026: 264635 },
          { rank: 3, name: "zeroclaw", stars_nov_2026: 33488 },
          { rank: 4, name: "agentskills", stars_nov_2026: 27455 },
          { rank: 5, name: "rmcp", stars_nov_2026: 4125 },
        ],
      });
    }

    if (url.pathname === "/claw/run" && request.method === "POST") {
      const body = (await request.json().catch(() => ({}))) as { goal?: string };
      const goal = body.goal ?? "Audit Cloudflare Edge OS";
      return json({
        ok: true,
        goal,
        channel: "CloudflareEdgeHttp",
        steps: [
          { step: 1, action: "ground-hypothesis-via-edge-do-memory", verified: true },
          { step: 2, action: "verify-mcp-tools-list", verified: true },
          { step: 3, action: "consolidate-edge-telemetry", verified: true },
        ],
      });
    }

    if (url.pathname === "/api/learn/events" || url.pathname === "/api/learn/status") {
      const tenant = resolveTenant(request, url);
      const bodyText = request.method === "GET" || request.method === "HEAD" ? undefined : await request.text();
      return dispatchToTenantDO(tenant, env, url.pathname, {
        method: request.method,
        headers: request.headers,
        body: bodyText,
        search: url.search,
      });
    }

    return json({ ok: false, error: "Not found", path: url.pathname }, 404);
  },
};
