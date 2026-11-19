import { Database } from "bun:sqlite";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

export interface RouterProviderEntry {
  id: string;
  kind: "local-gguf" | "openai-compatible" | "cloudflare-ai" | "antigravity-agy" | "ollama";
  baseUrl: string;
  models: string[];
  priority: number;
  capabilities: string[];
}

export const DEFAULT_ROUTER_PROVIDERS: RouterProviderEntry[] = [
  {
    id: "fw-llmd-local",
    kind: "local-gguf",
    baseUrl: "http://127.0.0.1:8080/v1",
    models: ["fw-local", "gguf-default", "whisper-local"],
    priority: 1,
    capabilities: ["chat", "embeddings", "audio", "gguf"],
  },
  {
    id: "ollama-local",
    kind: "ollama",
    baseUrl: "http://127.0.0.1:11434/v1",
    models: ["qwen3:8b", "llama3.3", "deepseek-r1"],
    priority: 2,
    capabilities: ["chat", "embeddings"],
  },
  {
    id: "antigravity-agy",
    kind: "antigravity-agy",
    baseUrl: "local://agy",
    models: ["gemini-4-neon", "gemini-3.8-flash"],
    priority: 3,
    capabilities: ["chat", "agent", "acp", "mcp"],
  },
  {
    id: "cloudflare-workers-ai",
    kind: "cloudflare-ai",
    baseUrl: "https://api.cloudflare.com/client/v4/accounts",
    models: ["@cf/meta/llama-3.3-70b-instruct-fp8-fast", "@cf/openai/whisper"],
    priority: 4,
    capabilities: ["chat", "audio", "edge"],
  },
];

export function loadSoul(root: string): Record<string, unknown> {
  const soulPath = resolve(root, "fw.soul.json");
  if (!existsSync(soulPath)) {
    return { identity: { name: "Fw", version: "2.4.0" }, principles: [], capabilities: [] };
  }
  return JSON.parse(readFileSync(soulPath, "utf8")) as Record<string, unknown>;
}

export function routeModelRequest(requestedModel?: string, capability = "chat") {
  const target = (requestedModel ?? "").toLowerCase();
  const matching = DEFAULT_ROUTER_PROVIDERS.filter(
    (p) =>
      p.capabilities.includes(capability) &&
      (!target || p.models.some((m) => m.toLowerCase().includes(target)) || p.id.toLowerCase().includes(target)),
  ).sort((a, b) => a.priority - b.priority);

  const selected = matching[0] ?? DEFAULT_ROUTER_PROVIDERS[0];
  return {
    ok: true,
    requestedModel: requestedModel ?? "auto",
    capability,
    selectedProvider: selected.id,
    kind: selected.kind,
    baseUrl: selected.baseUrl,
    model: requestedModel || selected.models[0],
    fallbacks: DEFAULT_ROUTER_PROVIDERS.filter((p) => p.id !== selected.id).map((p) => p.id),
  };
}

function openBrainDb(root: string): Database {
  const dbPath = resolve(root, "target", "fw-brain.db");
  mkdirSync(dirname(dbPath), { recursive: true });
  const db = new Database(dbPath);
  db.exec(`
    CREATE VIRTUAL TABLE IF NOT EXISTS brain_docs USING fts5(
      doc_id,
      kind,
      title,
      body
    );
  `);
  return db;
}

export function brainUpsert(root: string, docId: string, kind: string, title: string, body: string) {
  const db = openBrainDb(root);
  try {
    const del = db.query("DELETE FROM brain_docs WHERE doc_id = ?");
    const ins = db.query("INSERT INTO brain_docs (doc_id, kind, title, body) VALUES (?, ?, ?, ?)");
    del.run(docId);
    ins.run(docId, kind, title, body);
    del.finalize();
    ins.finalize();
  } finally {
    db.close();
  }
  return { ok: true, docId, kind, title };
}

export function brainIndexRepo(root: string) {
  const docs: Array<{ id: string; kind: string; title: string; path: string }> = [
    { id: "soul", kind: "soul", title: "Fw Soul", path: "fw.soul.json" },
    { id: "targets", kind: "config", title: "Fw Targets", path: "fw.targets.json" },
    { id: "agents", kind: "rule", title: "Fw AGENTS", path: "AGENTS.md" },
    { id: "readme", kind: "doc", title: "Fw README", path: "README.md" },
    { id: "plugin-fw", kind: "skill", title: "Fw Skill", path: "plugin/skills/fw/SKILL.md" },
    { id: "plugin-agy", kind: "skill", title: "Antigravity Knowledge", path: "plugin/skills/agy-knowledge/SKILL.md" },
    { id: "plugin-learn", kind: "skill", title: "Autonomous Learn", path: "plugin/skills/learn/SKILL.md" },
    { id: "plugin-lowlevel", kind: "skill", title: "Low-Level LTO AOT", path: "plugin/skills/low-level-lto-aot/SKILL.md" },
  ];
  let indexed = 0;
  for (const d of docs) {
    const abs = join(root, d.path);
    if (existsSync(abs)) {
      brainUpsert(root, d.id, d.kind, d.title, readFileSync(abs, "utf8"));
      indexed++;
    }
  }
  return { ok: true, indexed };
}

export function brainSearch(root: string, query: string, limit = 8) {
  const db = openBrainDb(root);
  try {
    const clean = query.replace(/[^\w\s-]/g, " ").trim();
    if (!clean) return { ok: true, query, hits: [] };
    const q = db.query(
      "SELECT doc_id, kind, title, snippet(brain_docs, 3, '[', ']', '...', 24) as snippet FROM brain_docs WHERE brain_docs MATCH ? LIMIT ?",
    );
    const hits = q.all(clean, limit) as Array<{ doc_id: string; kind: string; title: string; snippet: string }>;
    q.finalize();
    return { ok: true, query, count: hits.length, hits };
  } catch {
    return { ok: true, query, count: 0, hits: [] };
  } finally {
    db.close();
  }
}

export function buildOpenApiSpec(baseUrl = "http://127.0.0.1:4379") {
  return {
    openapi: "3.1.0",
    info: {
      title: "Fw Unified AI, Binary & VFS OS API",
      version: "2.4.0",
      description:
        "OpenAI-compatible, MCP, ACP, A2A, VFS OS, Fat-LTO PE/AST/Disasm/GGUF, AI Router/Brain/Soul, and Continuous Upgrade API.",
    },
    servers: [{ url: baseUrl }],
    paths: {
      "/health": { get: { summary: "Service and Fat-LTO ABI v4 health" } },
      "/status": { get: { summary: "Full monorepo, plugin, dedupe, and daemon status" } },
      "/openapi.json": { get: { summary: "OpenAPI 3.1 machine-readable specification" } },
      "/.well-known/agent.json": { get: { summary: "A2A AgentCard discovery document" } },
      "/v1/models": { get: { summary: "OpenAI-compatible model catalog from AI Router" } },
      "/v1/chat/completions": { post: { summary: "OpenAI-compatible chat completions via Fw Router/LLMD" } },
      "/v1/embeddings": { post: { summary: "OpenAI-compatible embeddings" } },
      "/mcp": { post: { summary: "Streamable HTTP MCP JSON-RPC 2.0 endpoint" } },
      "/acp": { post: { summary: "Agent Client Protocol (ACP) JSON-RPC 2.0 endpoint" } },
      "/a2a": { post: { summary: "Agent2Agent (A2A) JSON-RPC 2.0 endpoint" } },
      "/vfs/ls": { get: { summary: "List VFS OS directory entries" } },
      "/vfs/read": { get: { summary: "Read file from VFS OS" } },
      "/brain/search": { get: { summary: "Search Fw SQLite FTS5 AI Brain" } },
      "/soul": { get: { summary: "Inspect Fw AI Soul constitution" } },
      "/router": { get: { summary: "Inspect AI Router providers and route decision" } },
      "/lowlevel/matrix": { get: { summary: "Cross-language cross-architecture LTO/AOT build matrix" } },
      "/inspect": { get: { summary: "Fat-LTO craft-pe PE/COFF binary inspection" } },
      "/ast": { get: { summary: "Tree-sitter 11-grammar AST summary/symbols/query" } },
    },
  };
}

export function buildA2AAgentCard(baseUrl = "http://127.0.0.1:4379") {
  return {
    name: "Fw",
    description:
      "Unified polyglot Fat-LTO/AOT reverse-engineering, low-level build, VFS OS, and autonomous self-upgrading agent runtime.",
    url: `${baseUrl}/a2a`,
    version: "2.4.0",
    protocolVersion: "1.0.1",
    capabilities: { streaming: false, pushNotifications: false, stateTransitionHistory: true },
    defaultInputModes: ["text/plain", "application/json"],
    defaultOutputModes: ["text/plain", "application/json"],
    skills: [
      { id: "pe-inspect", name: "PE/COFF & Disassembly", description: "craft-pe + iced-x86 Fat-LTO binary inspection" },
      { id: "ast-polyglot", name: "Tree-sitter 11-Grammar AST", description: "Parse, extract symbols, and run queries" },
      { id: "lowlevel-lto-aot", name: "Low-Level LTO/AOT Builder", description: "Rust/C/C++/Asm/Wasm/CUDA/WGSL/Go/Java/Bun" },
      { id: "self-upgrade", name: "Continuous agy Auto-Upgrade", description: "Gated target/agy promotion & learning loop" },
    ],
  };
}

export function handleAcpMessage(root: string, req: { id?: string | number | null; method?: string; params?: Record<string, unknown> }) {
  const id = req.id ?? null;
  switch (req.method) {
    case "initialize":
      return {
        jsonrpc: "2.0",
        id,
        result: {
          protocolVersion: "1.33.0",
          agentInfo: { name: "fw", version: "2.4.0" },
          capabilities: { vfs: true, mcp: true, lowlevel: true, brain: true },
        },
      };
    case "session/new":
      return { jsonrpc: "2.0", id, result: { sessionId: `fw-acp-${Date.now()}`, cwd: root } };
    case "session/prompt": {
      const prompt = String(req.params?.prompt ?? "");
      // Harden ACP embedded blobs omitting mimeType (opencode#64800)
      const rawBlobs = Array.isArray(req.params?.blobs) ? (req.params?.blobs as Array<Record<string, unknown>>) : [];
      const blobs = rawBlobs.map((b) => ({
        ...b,
        mimeType: typeof b.mimeType === "string" && b.mimeType ? b.mimeType : "application/octet-stream",
      }));
      const route = routeModelRequest(undefined, "agent");
      const brain = brainSearch(root, prompt, 3);
      return {
        jsonrpc: "2.0",
        id,
        result: {
          stopReason: "end_turn",
          route,
          blobCount: blobs.length,
          brainHits: brain.hits,
          reply: `Fw ACP routed prompt via ${route.selectedProvider} (${route.model}) with ${brain.hits.length} brain context hits.`,
        },
      };
    }
    default:
      return { jsonrpc: "2.0", id, error: { code: -32601, message: `Unknown ACP method: ${req.method ?? ""}` } };
  }
}

export function handleA2AMessage(root: string, req: { id?: string | number | null; method?: string; params?: Record<string, unknown> }) {
  const id = req.id ?? null;
  switch (req.method) {
    case "tasks/send": {
      const taskId = String(req.params?.id ?? `fw-task-${Date.now()}`);
      const text = String((req.params?.message as { text?: string } | undefined)?.text ?? req.params?.prompt ?? "");
      const soul = loadSoul(root);
      return {
        jsonrpc: "2.0",
        id,
        result: {
          id: taskId,
          status: { state: "completed", timestamp: new Date().toISOString() },
          artifacts: [
            {
              name: "fw-response",
              parts: [{ type: "text", text: `Fw (${(soul.identity as { version?: string } | undefined)?.version ?? "2.4.0"}) handled: ${text}` }],
            },
          ],
        },
      };
    }
    case "tasks/get":
      return { jsonrpc: "2.0", id, result: { id: req.params?.id ?? null, status: { state: "completed" } } };
    default:
      return { jsonrpc: "2.0", id, error: { code: -32601, message: `Unknown A2A method: ${req.method ?? ""}` } };
  }
}

export function handleOpenAiChatCompletion(
  root: string,
  body: {
    model?: string;
    messages?: Array<{ role: string; content: string; tool_calls?: unknown[] }>;
    tools?: unknown[];
  },
) {
  const route = routeModelRequest(body.model, "chat");
  const messages = body.messages ?? [];
  const lastMsg = messages[messages.length - 1];
  const lastUser = [...messages].reverse().find((m) => m.role === "user")?.content ?? "";
  const brain = lastUser ? brainSearch(root, lastUser, 3) : { hits: [] };
  // Preserve multi-turn continuation after tool responses (opencode#14973)
  const continuingAfterTool = lastMsg?.role === "tool";
  const reply = [
    `[fw-router -> ${route.selectedProvider}:${route.model}]`,
    continuingAfterTool ? "Continuing after tool result." : lastUser ? `Prompt received (${lastUser.length} chars).` : "Ready.",
    brain.hits.length > 0 ? `Brain hits: ${brain.hits.map((h) => h.title).join(", ")}.` : "No local brain hits.",
  ].join(" ");

  return {
    id: `chatcmpl-fw-${Date.now()}`,
    object: "chat.completion",
    created: Math.floor(Date.now() / 1000),
    model: route.model,
    provider: route.selectedProvider,
    choices: [
      {
        index: 0,
        message: { role: "assistant", content: reply },
        finish_reason: "stop",
      },
    ],
    usage: { prompt_tokens: Math.ceil(lastUser.length / 4), completion_tokens: Math.ceil(reply.length / 4), total_tokens: Math.ceil((lastUser.length + reply.length) / 4) },
  };
}

/**
 * Typed Fw Provider & SDK Client (compatible with OpenAI/AI-SDK patterns).
 */
export class FwProvider {
  constructor(
    public readonly root: string,
    public readonly baseUrl = "http://127.0.0.1:4379",
  ) {}

  listModels() {
    return {
      object: "list",
      data: DEFAULT_ROUTER_PROVIDERS.flatMap((p) =>
        p.models.map((id) => ({ id, object: "model", owned_by: p.id, capabilities: p.capabilities })),
      ),
    };
  }

  chat(prompt: string, model?: string) {
    return handleOpenAiChatCompletion(this.root, {
      model,
      messages: [{ role: "user", content: prompt }],
    });
  }

  route(model?: string, capability = "chat") {
    return routeModelRequest(model, capability);
  }
}
