import { Database, type Statement } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

// ============================================================================
// Pillar 1: Runtime TS/TSX AI Tool & Agent Plugin Compiler (Bun.Transpiler)
// ============================================================================

export type ToolLoader = "ts" | "tsx" | "js" | "jsx";

export interface ScannedImport {
  path: string;
  kind: string;
}

export interface CompileToolOptions {
  name: string;
  source: string;
  loader?: ToolLoader;
  sandbox?: boolean;
  description?: string;
}

export interface CompileToolResult {
  ok: boolean;
  name: string;
  version: number;
  loader: ToolLoader;
  sandbox: boolean;
  imports: ScannedImport[];
  blockedImports: string[];
  blockedGlobals: string[];
  jsCode: string;
  codeHash: string;
  compileMs: number;
  errors: string[];
}

export interface ToolExecutionResult {
  ok: boolean;
  name: string;
  version: number;
  output?: unknown;
  error?: string;
  execMs: number;
}

export interface ToolTelemetry {
  name: string;
  version: number;
  loader: ToolLoader;
  sandbox: boolean;
  description: string;
  codeHash: string;
  compileMs: number;
  invocations: number;
  totalExecMs: number;
  avgExecMs: number;
  lastExecMs: number;
  lastError: string | null;
  registeredAt: string;
}

export interface VNode {
  type: string;
  props: Record<string, unknown>;
  children: unknown[];
}

const FORBIDDEN_SANDBOX_MODULES = new Set([
  "fs",
  "node:fs",
  "fs/promises",
  "node:fs/promises",
  "net",
  "node:net",
  "http",
  "node:http",
  "https",
  "node:https",
  "http2",
  "node:http2",
  "tls",
  "node:tls",
  "dgram",
  "node:dgram",
  "dns",
  "node:dns",
  "child_process",
  "node:child_process",
  "cluster",
  "node:cluster",
  "worker_threads",
  "node:worker_threads",
  "vm",
  "node:vm",
  "os",
  "node:os",
  "process",
  "node:process",
  "bun",
  "bun:ffi",
  "bun:sqlite",
  "bun:dns",
  "bun:jsc",
]);

const ALLOWED_SYNTHETIC_JSX_IMPORTS = new Set([
  "react/jsx-runtime",
  "react/jsx-dev-runtime",
]);

const FORBIDDEN_SANDBOX_GLOBAL_PATTERNS: ReadonlyArray<{ label: string; pattern: RegExp }> = [
  { label: "Bun.spawn", pattern: /\bBun\s*\.\s*spawn(?:Sync)?\b/ },
  { label: "Bun.write", pattern: /\bBun\s*\.\s*write\b/ },
  { label: "Bun.file", pattern: /\bBun\s*\.\s*file\b/ },
  { label: "Bun.connect", pattern: /\bBun\s*\.\s*(?:connect|listen|serve)\b/ },
  { label: "fetch", pattern: /\bfetch\s*\(/ },
  { label: "WebSocket", pattern: /\bnew\s+WebSocket\b/ },
  { label: "process.env", pattern: /\bprocess\s*\.\s*(?:env|exit|kill|binding|dlopen)\b/ },
  { label: "eval/Function", pattern: /\b(?:eval\s*\(|new\s+Function\s*\()/ },
  { label: "globalThis", pattern: /\bglobalThis\b/ },
];

const SANDBOX_SHADOWED_GLOBALS = [
  "Bun",
  "fetch",
  "WebSocket",
  "XMLHttpRequest",
  "process",
  "require",
  "globalThis",
  "global",
  "window",
  "self",
  "Deno",
] as const;

const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor as new (
  ...args: string[]
) => (...fnArgs: unknown[]) => Promise<unknown>;

function sha256Hex(input: string | Uint8Array): string {
  const hasher = new Bun.CryptoHasher("sha256");
  hasher.update(input);
  return hasher.digest("hex");
}

function createVNode(
  type: unknown,
  props: Record<string, unknown> | null | undefined,
  ...children: unknown[]
): VNode | unknown {
  const flatChildren = children.flat(Infinity).filter((c) => c !== null && c !== undefined && c !== false);
  if (typeof type === "function") {
    return (type as (p: Record<string, unknown>) => unknown)({
      ...(props ?? {}),
      children: flatChildren.length === 1 ? flatChildren[0] : flatChildren,
    });
  }
  return {
    type: String(type),
    props: props ? { ...props } : {},
    children: flatChildren,
  };
}

function jsxRuntimeFactory(type: unknown, props: Record<string, unknown> | null | undefined): VNode | unknown {
  const { children, ...rest } = props ?? {};
  const childList = children === undefined ? [] : Array.isArray(children) ? children : [children];
  return createVNode(type, rest, ...childList);
}

const JSX_FRAGMENT = "Fragment";

function isForbiddenImportSpecifier(specifier: string): boolean {
  const normalized = specifier.trim().toLowerCase();
  if (ALLOWED_SYNTHETIC_JSX_IMPORTS.has(normalized)) {
    return false;
  }
  if (FORBIDDEN_SANDBOX_MODULES.has(normalized)) {
    return true;
  }
  if (
    normalized.startsWith("node:") ||
    normalized.startsWith("bun:") ||
    normalized.startsWith("file:") ||
    normalized.startsWith("http:") ||
    normalized.startsWith("https:") ||
    normalized.startsWith("../") ||
    normalized.includes("/../") ||
    normalized.startsWith("..\\")
  ) {
    return true;
  }
  return false;
}

function scanBlockedGlobals(source: string): string[] {
  const hits: string[] = [];
  for (const rule of FORBIDDEN_SANDBOX_GLOBAL_PATTERNS) {
    if (rule.pattern.test(source)) {
      hits.push(rule.label);
    }
  }
  return hits;
}

function transformEsmToInProcessBody(jsCode: string): string {
  const exportedNames: Array<{ local: string; exported: string }> = [];

  // Strip synthetic JSX runtime imports emitted by Bun.Transpiler for TSX.
  let body = jsCode.replace(
    /^\s*import\s+[^;]*?from\s+["']react\/jsx(?:-dev)?-runtime["'];?\s*$/gm,
    "",
  );

  // Rewrite `export default`
  body = body.replace(/\bexport\s+default\s+/g, "__exports.default = ");

  // Rewrite `export async function name` and `export function name`
  body = body.replace(
    /\bexport\s+(async\s+)?function\s+([A-Za-z_$][\w$]*)\s*\(/g,
    (_m, asyncKw: string | undefined, name: string) => {
      exportedNames.push({ local: name, exported: name });
      return `${asyncKw ?? ""}function ${name}(`;
    },
  );

  // Rewrite `export const|let|var name =`
  body = body.replace(
    /\bexport\s+(const|let|var)\s+([A-Za-z_$][\w$]*)\b/g,
    (_m, kind: string, name: string) => {
      exportedNames.push({ local: name, exported: name });
      return `${kind} ${name}`;
    },
  );

  // Rewrite `export { a, b as c }`
  body = body.replace(/\bexport\s*\{([^}]+)\}\s*;?/g, (_m, list: string) => {
    const assignments: string[] = [];
    for (const part of list.split(",")) {
      const trimmed = part.trim();
      if (!trimmed) continue;
      const asMatch = /^([A-Za-z_$][\w$]*)\s+as\s+([A-Za-z_$][\w$]*)$/.exec(trimmed);
      if (asMatch) {
        assignments.push(`__exports[${JSON.stringify(asMatch[2])}] = ${asMatch[1]};`);
      } else {
        assignments.push(`__exports[${JSON.stringify(trimmed)}] = ${trimmed};`);
      }
    }
    return assignments.join("\n");
  });

  const exportTail = exportedNames
    .map(({ local, exported }) => `__exports[${JSON.stringify(exported)}] = ${local};`)
    .join("\n");

  return `"use strict";\n${body}\n${exportTail}\nreturn __exports;`;
}

interface CompiledToolRecord {
  telemetry: ToolTelemetry;
  handler: (args: unknown, ctx: Record<string, unknown>) => Promise<unknown>;
  jsCode: string;
}

export class RuntimeToolRegistry {
  private readonly tools = new Map<string, CompiledToolRecord>();

  compileAndRegister(options: CompileToolOptions): CompileToolResult {
    const startNs = Bun.nanoseconds();
    const loader: ToolLoader = options.loader ?? "ts";
    const sandbox = options.sandbox ?? true;
    const prev = this.tools.get(options.name);
    const nextVersion = prev ? prev.telemetry.version + 1 : 1;
    const codeHash = sha256Hex(`${loader}:${options.source}`);
    const errors: string[] = [];

    const transpiler = new Bun.Transpiler({
      loader,
      deadCodeElimination: true,
      trimUnusedImports: true,
      ...(loader === "tsx" || loader === "jsx" ? { tsconfig: JSON.stringify({ compilerOptions: { jsx: "react", jsxFactory: "__fwJsx", jsxFragmentFactory: "__fwFragment" } }) } : {}),
    });

    let imports: ScannedImport[] = [];
    try {
      imports = transpiler.scanImports(options.source).map((imp) => ({
        path: imp.path,
        kind: imp.kind,
      }));
    } catch (err) {
      errors.push(`scanImports failed: ${err instanceof Error ? err.message : String(err)}`);
    }

    const blockedImports = sandbox
      ? imports.filter((imp) => isForbiddenImportSpecifier(imp.path)).map((imp) => imp.path)
      : [];
    const blockedGlobals = sandbox ? scanBlockedGlobals(options.source) : [];

    if (blockedImports.length > 0) {
      errors.push(`Sandbox blocked forbidden imports: ${blockedImports.join(", ")}`);
    }
    if (blockedGlobals.length > 0) {
      errors.push(`Sandbox blocked forbidden globals: ${blockedGlobals.join(", ")}`);
    }

    let jsCode = "";
    if (errors.length === 0) {
      try {
        jsCode = transpiler.transformSync(options.source);
      } catch (err) {
        errors.push(`Transpile failed: ${err instanceof Error ? err.message : String(err)}`);
      }
    }

    if (errors.length > 0) {
      const compileMs = Number(((Bun.nanoseconds() - startNs) / 1e6).toFixed(4));
      return {
        ok: false,
        name: options.name,
        version: prev?.telemetry.version ?? 0,
        loader,
        sandbox,
        imports,
        blockedImports,
        blockedGlobals,
        jsCode,
        codeHash,
        compileMs,
        errors,
      };
    }

    const body = transformEsmToInProcessBody(jsCode);
    const paramNames = [
      "__exports",
      "ctx",
      "__fwJsx",
      "__fwFragment",
      "jsx",
      "jsxs",
      "jsxDEV",
      "Fragment",
      "React",
      ...(sandbox ? [...SANDBOX_SHADOWED_GLOBALS] : []),
    ];

    let moduleFactory: (...fnArgs: unknown[]) => Promise<unknown>;
    try {
      moduleFactory = new AsyncFunction(...paramNames, body);
    } catch (err) {
      const compileMs = Number(((Bun.nanoseconds() - startNs) / 1e6).toFixed(4));
      return {
        ok: false,
        name: options.name,
        version: prev?.telemetry.version ?? 0,
        loader,
        sandbox,
        imports,
        blockedImports,
        blockedGlobals,
        jsCode,
        codeHash,
        compileMs,
        errors: [`Function instantiation failed: ${err instanceof Error ? err.message : String(err)}`],
      };
    }

    const handler = async (args: unknown, ctx: Record<string, unknown>): Promise<unknown> => {
      const exportsObj: Record<string, unknown> = {};
      const reactCompat = { createElement: createVNode, Fragment: JSX_FRAGMENT };
      const shadowValues = sandbox ? SANDBOX_SHADOWED_GLOBALS.map(() => undefined) : [];
      await moduleFactory(
        exportsObj,
        ctx,
        createVNode,
        JSX_FRAGMENT,
        jsxRuntimeFactory,
        jsxRuntimeFactory,
        jsxRuntimeFactory,
        JSX_FRAGMENT,
        reactCompat,
        ...shadowValues,
      );

      const candidate =
        exportsObj.run ??
        exportsObj.execute ??
        exportsObj.default ??
        Object.values(exportsObj).find((v) => typeof v === "function");

      if (typeof candidate !== "function") {
        throw new Error(
          `Tool "${options.name}" must export a callable function (run, execute, or default)`,
        );
      }
      return await (candidate as (a: unknown, c: Record<string, unknown>) => unknown)(args, ctx);
    };

    const compileMs = Number(((Bun.nanoseconds() - startNs) / 1e6).toFixed(4));
    this.tools.set(options.name, {
      jsCode,
      handler,
      telemetry: {
        name: options.name,
        version: nextVersion,
        loader,
        sandbox,
        description: options.description ?? `Runtime ${loader.toUpperCase()} tool (${options.name})`,
        codeHash,
        compileMs,
        invocations: prev?.telemetry.invocations ?? 0,
        totalExecMs: prev?.telemetry.totalExecMs ?? 0,
        avgExecMs: prev?.telemetry.avgExecMs ?? 0,
        lastExecMs: prev?.telemetry.lastExecMs ?? 0,
        lastError: null,
        registeredAt: new Date().toISOString(),
      },
    });

    return {
      ok: true,
      name: options.name,
      version: nextVersion,
      loader,
      sandbox,
      imports,
      blockedImports,
      blockedGlobals,
      jsCode,
      codeHash,
      compileMs,
      errors: [],
    };
  }

  async execute(
    name: string,
    args: unknown = {},
    ctx: Record<string, unknown> = {},
  ): Promise<ToolExecutionResult> {
    const rec = this.tools.get(name);
    if (!rec) {
      return {
        ok: false,
        name,
        version: 0,
        error: `Tool "${name}" is not registered`,
        execMs: 0,
      };
    }

    const startNs = Bun.nanoseconds();
    try {
      const output = await rec.handler(args, ctx);
      const execMs = Number(((Bun.nanoseconds() - startNs) / 1e6).toFixed(4));
      rec.telemetry.invocations += 1;
      rec.telemetry.totalExecMs = Number((rec.telemetry.totalExecMs + execMs).toFixed(4));
      rec.telemetry.avgExecMs = Number((rec.telemetry.totalExecMs / rec.telemetry.invocations).toFixed(4));
      rec.telemetry.lastExecMs = execMs;
      rec.telemetry.lastError = null;
      return {
        ok: true,
        name,
        version: rec.telemetry.version,
        output,
        execMs,
      };
    } catch (err) {
      const execMs = Number(((Bun.nanoseconds() - startNs) / 1e6).toFixed(4));
      const message = err instanceof Error ? err.message : String(err);
      rec.telemetry.invocations += 1;
      rec.telemetry.totalExecMs = Number((rec.telemetry.totalExecMs + execMs).toFixed(4));
      rec.telemetry.avgExecMs = Number((rec.telemetry.totalExecMs / rec.telemetry.invocations).toFixed(4));
      rec.telemetry.lastExecMs = execMs;
      rec.telemetry.lastError = message;
      return {
        ok: false,
        name,
        version: rec.telemetry.version,
        error: message,
        execMs,
      };
    }
  }

  listTools(): ToolTelemetry[] {
    return Array.from(this.tools.values()).map((r) => ({ ...r.telemetry }));
  }

  getTool(name: string): ToolTelemetry | null {
    const rec = this.tools.get(name);
    return rec ? { ...rec.telemetry } : null;
  }

  removeTool(name: string): boolean {
    return this.tools.delete(name);
  }
}

// ============================================================================
// Pillar 2: Persistent llm-d / vLLM Prefix-Aware KV-Cache & Hybrid FTS5/Vector RAG
// ============================================================================

export type ClusterNodeId = "fr1" | "prd" | "local-rtx4070";

export interface ClusterNodeSpec {
  id: ClusterNodeId;
  role: "prefill" | "decode" | "prefill-decode";
  accelerator: string;
  baseTtftMs: number;
  msPerPrefillToken: number;
  msPerDecodeToken: number;
  maxKvBlocks: number;
}

// Latency parameters below are parameters of the routing simulation, not measurements of these machines.
export const DEFAULT_CLUSTER_NODES: readonly ClusterNodeSpec[] = [
  {
    id: "local-rtx4070",
    role: "prefill-decode",
    accelerator: "NVIDIA GeForce RTX 4070 (sm_89, 12GB VRAM)",
    baseTtftMs: 14,
    msPerPrefillToken: 0.35,
    msPerDecodeToken: 6.5,
    maxKvBlocks: 4096,
  },
  {
    id: "fr1",
    role: "prefill",
    accelerator: "12 vCPU Intel Haswell, 49.3 GB RAM, no GPU (cluster/build.toml, docs/HARDWARE_MATRIX.md)",
    baseTtftMs: 28,
    msPerPrefillToken: 0.55,
    msPerDecodeToken: 12.0,
    maxKvBlocks: 16384,
  },
  {
    id: "prd",
    role: "decode",
    accelerator: "8 vCPU AMD EPYC 9354P, 33.6 GB RAM, no GPU (cluster/build.toml, docs/HARDWARE_MATRIX.md)",
    baseTtftMs: 24,
    msPerPrefillToken: 0.9,
    msPerDecodeToken: 8.5,
    maxKvBlocks: 8192,
  },
] as const;

export const KV_BLOCK_TOKEN_SIZE = 16;
export const DEFAULT_EMBEDDING_DIMS = 64;

export interface PrefixBlockHash {
  blockIndex: number;
  hash: string;
  parentHash: string;
  tokenCount: number;
}

export interface KvRouteRequest {
  prompt: string;
  model?: string;
  disaggregated?: boolean;
  preferredPrefillNode?: ClusterNodeId;
  preferredDecodeNode?: ClusterNodeId;
}

export interface KvRouteDecision {
  ok: true;
  model: string;
  disaggregated: boolean;
  prefillNode: ClusterNodeId;
  decodeNode: ClusterNodeId;
  totalTokens: number;
  totalBlocks: number;
  matchedBlocks: number;
  savedPrefillTokens: number;
  uncachedPrefillTokens: number;
  kvCacheHit: boolean;
  prefixHitRatio: number;
  baselineTtftMs: number;
  estimatedTtftMs: number;
  ttftReductionMs: number;
  ttftReductionPct: number;
  prefixBlockHashes: string[];
}

export interface RagDocumentInput {
  docKey: string;
  title: string;
  content: string;
  metadata?: Record<string, unknown>;
  embedding?: Float32Array | number[];
}

export interface HybridSearchOptions {
  limit?: number;
  ftsWeight?: number;
  vectorWeight?: number;
  queryEmbedding?: Float32Array | number[];
}

export interface HybridSearchHit {
  docKey: string;
  title: string;
  content: string;
  metadata: Record<string, unknown>;
  ftsScore: number;
  vectorScore: number;
  hybridScore: number;
}

export interface KvStoreStats {
  totalRoutes: number;
  totalCacheHits: number;
  cacheHitRate: number;
  totalSavedPrefillTokens: number;
  avgTtftReductionPct: number;
  cachedKvBlocks: number;
  ragDocuments: number;
  journalMode: string;
  nodes: Array<{
    id: ClusterNodeId;
    role: ClusterNodeSpec["role"];
    cachedBlocks: number;
    prefillCount: number;
    decodeCount: number;
  }>;
}

export interface SnapshotExportResult {
  format: "fw-kv-rag-v1+gzip";
  createdAt: string;
  rawBytes: number;
  compressedBytes: number;
  compressionRatio: number;
  sha256: string;
  kvBlockCount: number;
  ragDocCount: number;
  payloadBase64: string;
}

export function tokenizePrompt(text: string): string[] {
  const matches = text.match(/[\p{L}\p{N}_-]+|[^\s\p{L}\p{N}_-]/gu);
  return matches ? matches : [];
}

export function computeChainedPrefixBlocks(
  prompt: string,
  model: string,
  blockSize = KV_BLOCK_TOKEN_SIZE,
): { tokens: string[]; blocks: PrefixBlockHash[] } {
  const tokens = tokenizePrompt(prompt);
  const blocks: PrefixBlockHash[] = [];
  if (tokens.length === 0) {
    return { tokens, blocks };
  }

  let parentHash = sha256Hex(`fw-kv-root-v1:${model}`);
  const fullBlocks = Math.floor(tokens.length / blockSize);
  const blockCount = fullBlocks > 0 ? fullBlocks : 1;

  for (let i = 0; i < blockCount; i++) {
    const start = i * blockSize;
    const end = fullBlocks > 0 ? start + blockSize : tokens.length;
    const slice = tokens.slice(start, end);
    const hasher = new Bun.CryptoHasher("sha256");
    hasher.update(parentHash);
    hasher.update(":");
    hasher.update(String(i));
    hasher.update(":");
    hasher.update(slice.join("\u001f"));
    const hash = hasher.digest("hex");
    blocks.push({
      blockIndex: i,
      hash,
      parentHash,
      tokenCount: slice.length,
    });
    parentHash = hash;
  }

  return { tokens, blocks };
}

export function embedTextDeterministic(text: string, dims = DEFAULT_EMBEDDING_DIMS): Float32Array {
  const vec = new Float32Array(dims);
  const tokens = tokenizePrompt(text.toLowerCase());
  if (tokens.length === 0) {
    return vec;
  }

  for (let idx = 0; idx < tokens.length; idx++) {
    const tok = tokens[idx]!;
    const hasher = new Bun.CryptoHasher("sha256");
    hasher.update(tok);
    const digest = hasher.digest();

    // Project each token onto 4 signed buckets for dense, collision-resistant similarity.
    for (let k = 0; k < 4; k++) {
      const bucket = digest[k]! % dims;
      const sign = (digest[k + 4]! & 1) === 0 ? 1 : -1;
      vec[bucket] = vec[bucket]! + sign;
    }

    // Add bigram signal when available.
    if (idx + 1 < tokens.length) {
      const biHasher = new Bun.CryptoHasher("sha256");
      biHasher.update(`${tok}_${tokens[idx + 1]!}`);
      const biDigest = biHasher.digest();
      const bucket = biDigest[0]! % dims;
      const sign = (biDigest[1]! & 1) === 0 ? 0.5 : -0.5;
      vec[bucket] = vec[bucket]! + sign;
    }
  }

  let normSq = 0;
  for (let i = 0; i < dims; i++) {
    normSq += vec[i]! * vec[i]!;
  }
  const norm = Math.sqrt(normSq);
  if (norm > 0) {
    for (let i = 0; i < dims; i++) {
      vec[i] = vec[i]! / norm;
    }
  }
  return vec;
}

export function cosineSimilarity(a: Float32Array, b: Float32Array): number {
  const len = Math.min(a.length, b.length);
  if (len === 0) return 0;
  let dot = 0;
  let normA = 0;
  let normB = 0;
  for (let i = 0; i < len; i++) {
    const av = a[i]!;
    const bv = b[i]!;
    dot += av * bv;
    normA += av * av;
    normB += bv * bv;
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom > 0 ? dot / denom : 0;
}

function toFloat32Array(input: Float32Array | number[] | Uint8Array): Float32Array {
  if (input instanceof Float32Array) {
    return input;
  }
  if (input instanceof Uint8Array) {
    const copy = new Uint8Array(input.byteLength);
    copy.set(input);
    return new Float32Array(copy.buffer);
  }
  return new Float32Array(input);
}

function float32ToBytes(vec: Float32Array): Uint8Array {
  return new Uint8Array(vec.buffer, vec.byteOffset, vec.byteLength);
}

function buildFtsQuery(raw: string): string {
  const tokens = tokenizePrompt(raw)
    .map((t) => t.replace(/["'*^:()]/g, "").trim())
    .filter((t) => t.length > 0);
  if (tokens.length === 0) {
    return "";
  }
  return tokens.map((t) => `"${t}"`).join(" OR ");
}

export class PrefixKvRagStore {
  private readonly db: Database;
  private readonly nodes = new Map<ClusterNodeId, ClusterNodeSpec>();
  private readonly statements: Statement[] = [];
  private closed = false;

  // Prepared statements (all registered in `this.statements` for strict Windows `.finalize()` hygiene)
  private readonly stmtLookupBlock: Statement;
  private readonly stmtTouchBlock: Statement;
  private readonly stmtUpsertBlock: Statement;
  private readonly stmtInsertRouteEvent: Statement;
  private readonly stmtIncrementNodePrefill: Statement;
  private readonly stmtIncrementNodeDecode: Statement;
  private readonly stmtListNodeStats: Statement;
  private readonly stmtRouteAggregate: Statement;
  private readonly stmtCountBlocks: Statement;
  private readonly stmtCountDocs: Statement;
  private readonly stmtSelectDocId: Statement;
  private readonly stmtInsertDoc: Statement;
  private readonly stmtUpdateDoc: Statement;
  private readonly stmtDeleteFtsRow: Statement;
  private readonly stmtInsertFtsRow: Statement;
  private readonly stmtFtsSearch: Statement;
  private readonly stmtAllDocs: Statement;
  private readonly stmtAllBlocks: Statement;

  constructor(dbPath = ":memory:", nodeSpecs: readonly ClusterNodeSpec[] = DEFAULT_CLUSTER_NODES) {
    if (dbPath !== ":memory:") {
      mkdirSync(dirname(dbPath), { recursive: true });
    }
    this.db = new Database(dbPath, { create: true, strict: false });
    this.db.exec("PRAGMA journal_mode = WAL;");
    this.db.exec("PRAGMA synchronous = NORMAL;");
    this.db.exec("PRAGMA foreign_keys = ON;");

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS cluster_nodes (
        id TEXT PRIMARY KEY,
        role TEXT NOT NULL,
        accelerator TEXT NOT NULL,
        prefill_count INTEGER NOT NULL DEFAULT 0,
        decode_count INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS kv_blocks (
        block_hash TEXT NOT NULL,
        model TEXT NOT NULL,
        node_id TEXT NOT NULL,
        block_index INTEGER NOT NULL,
        parent_hash TEXT NOT NULL,
        token_count INTEGER NOT NULL,
        hits INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        last_hit_at INTEGER NOT NULL,
        PRIMARY KEY (block_hash, model)
      );

      CREATE INDEX IF NOT EXISTS idx_kv_blocks_node ON kv_blocks(node_id);

      CREATE TABLE IF NOT EXISTS kv_route_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        model TEXT NOT NULL,
        prefill_node TEXT NOT NULL,
        decode_node TEXT NOT NULL,
        total_tokens INTEGER NOT NULL,
        matched_blocks INTEGER NOT NULL,
        saved_prefill_tokens INTEGER NOT NULL,
        kv_cache_hit INTEGER NOT NULL,
        ttft_reduction_pct REAL NOT NULL,
        created_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS rag_docs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        doc_key TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        content TEXT NOT NULL,
        metadata_json TEXT NOT NULL,
        embedding_blob BLOB NOT NULL,
        token_count INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );

      CREATE VIRTUAL TABLE IF NOT EXISTS rag_docs_fts USING fts5(
        doc_key UNINDEXED,
        title,
        content,
        tokenize = 'unicode61'
      );
    `);

    const seedNode = this.db.prepare(`
      INSERT OR IGNORE INTO cluster_nodes (id, role, accelerator, prefill_count, decode_count)
      VALUES (?, ?, ?, 0, 0)
    `);
    for (const spec of nodeSpecs) {
      this.nodes.set(spec.id, spec);
      seedNode.run(spec.id, spec.role, spec.accelerator);
    }
    seedNode.finalize();

    this.stmtLookupBlock = this.track(
      this.db.prepare(
        `SELECT block_hash, node_id, block_index, token_count, hits
         FROM kv_blocks WHERE block_hash = ? AND model = ?`,
      ),
    );
    this.stmtTouchBlock = this.track(
      this.db.prepare(
        `UPDATE kv_blocks SET hits = hits + 1, last_hit_at = ?
         WHERE block_hash = ? AND model = ?`,
      ),
    );
    this.stmtUpsertBlock = this.track(
      this.db.prepare(
        `INSERT INTO kv_blocks (block_hash, model, node_id, block_index, parent_hash, token_count, hits, created_at, last_hit_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?)
         ON CONFLICT(block_hash, model) DO UPDATE SET last_hit_at = excluded.last_hit_at`,
      ),
    );
    this.stmtInsertRouteEvent = this.track(
      this.db.prepare(
        `INSERT INTO kv_route_events
         (model, prefill_node, decode_node, total_tokens, matched_blocks, saved_prefill_tokens, kv_cache_hit, ttft_reduction_pct, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ),
    );
    this.stmtIncrementNodePrefill = this.track(
      this.db.prepare(`UPDATE cluster_nodes SET prefill_count = prefill_count + 1 WHERE id = ?`),
    );
    this.stmtIncrementNodeDecode = this.track(
      this.db.prepare(`UPDATE cluster_nodes SET decode_count = decode_count + 1 WHERE id = ?`),
    );
    this.stmtListNodeStats = this.track(
      this.db.prepare(`
        SELECT n.id, n.role, n.prefill_count, n.decode_count,
               (SELECT COUNT(*) FROM kv_blocks b WHERE b.node_id = n.id) AS cached_blocks
        FROM cluster_nodes n
        ORDER BY n.id ASC
      `),
    );
    this.stmtRouteAggregate = this.track(
      this.db.prepare(`
        SELECT
          COUNT(*) AS total_routes,
          COALESCE(SUM(kv_cache_hit), 0) AS total_hits,
          COALESCE(SUM(saved_prefill_tokens), 0) AS total_saved_tokens,
          COALESCE(AVG(ttft_reduction_pct), 0) AS avg_ttft_reduction_pct
        FROM kv_route_events
      `),
    );
    this.stmtCountBlocks = this.track(this.db.prepare(`SELECT COUNT(*) AS cnt FROM kv_blocks`));
    this.stmtCountDocs = this.track(this.db.prepare(`SELECT COUNT(*) AS cnt FROM rag_docs`));
    this.stmtSelectDocId = this.track(this.db.prepare(`SELECT id FROM rag_docs WHERE doc_key = ?`));
    this.stmtInsertDoc = this.track(
      this.db.prepare(
        `INSERT INTO rag_docs (doc_key, title, content, metadata_json, embedding_blob, token_count, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ),
    );
    this.stmtUpdateDoc = this.track(
      this.db.prepare(
        `UPDATE rag_docs
         SET title = ?, content = ?, metadata_json = ?, embedding_blob = ?, token_count = ?, updated_at = ?
         WHERE id = ?`,
      ),
    );
    this.stmtDeleteFtsRow = this.track(this.db.prepare(`DELETE FROM rag_docs_fts WHERE rowid = ?`));
    this.stmtInsertFtsRow = this.track(
      this.db.prepare(`INSERT INTO rag_docs_fts (rowid, doc_key, title, content) VALUES (?, ?, ?, ?)`),
    );
    this.stmtFtsSearch = this.track(
      this.db.prepare(
        `SELECT rowid, doc_key, bm25(rag_docs_fts) AS bm25_score
         FROM rag_docs_fts
         WHERE rag_docs_fts MATCH ?
         ORDER BY bm25_score ASC
         LIMIT ?`,
      ),
    );
    this.stmtAllDocs = this.track(
      this.db.prepare(
        `SELECT id, doc_key, title, content, metadata_json, embedding_blob, token_count, updated_at
         FROM rag_docs`,
      ),
    );
    this.stmtAllBlocks = this.track(
      this.db.prepare(
        `SELECT block_hash, model, node_id, block_index, parent_hash, token_count, hits, created_at, last_hit_at
         FROM kv_blocks ORDER BY created_at ASC`,
      ),
    );
  }

  private track<T extends Statement>(stmt: T): T {
    this.statements.push(stmt);
    return stmt;
  }

  private pickLeastLoadedNode(
    predicate: (spec: ClusterNodeSpec) => boolean,
    excludeId?: ClusterNodeId,
  ): ClusterNodeId {
    const rows = this.stmtListNodeStats.all() as Array<{
      id: ClusterNodeId;
      prefill_count: number;
      decode_count: number;
      cached_blocks: number;
    }>;

    const candidates = rows
      .filter((r) => {
        const spec = this.nodes.get(r.id);
        return spec !== undefined && predicate(spec) && r.id !== excludeId;
      })
      .sort((a, b) => {
        const loadA = a.prefill_count + a.decode_count;
        const loadB = b.prefill_count + b.decode_count;
        if (loadA !== loadB) return loadA - loadB;
        return a.id.localeCompare(b.id);
      });

    if (candidates.length > 0) {
      return candidates[0]!.id;
    }
    return "local-rtx4070";
  }

  routePrefix(req: KvRouteRequest): KvRouteDecision {
    const model = req.model ?? "fw-qwen3-8b-gguf";
    const disaggregated = req.disaggregated ?? true;
    const now = Date.now();
    const { tokens, blocks } = computeChainedPrefixBlocks(req.prompt, model, KV_BLOCK_TOKEN_SIZE);

    let matchedBlocks = 0;
    let savedPrefillTokens = 0;
    let affinityNode: ClusterNodeId | null = null;

    for (const block of blocks) {
      const found = this.stmtLookupBlock.get(block.hash, model) as
        | { block_hash: string; node_id: ClusterNodeId; block_index: number; token_count: number; hits: number }
        | null;
      if (!found) {
        break;
      }
      matchedBlocks += 1;
      savedPrefillTokens += found.token_count;
      affinityNode = found.node_id;
      this.stmtTouchBlock.run(now, block.hash, model);
    }

    const prefillNode: ClusterNodeId =
      affinityNode ??
      req.preferredPrefillNode ??
      (tokens.length >= 32
        ? this.pickLeastLoadedNode((n) => n.role === "prefill" || n.role === "prefill-decode")
        : "local-rtx4070");

    const decodeNode: ClusterNodeId =
      req.preferredDecodeNode ??
      (disaggregated
        ? this.pickLeastLoadedNode((n) => n.role === "decode" || n.role === "prefill-decode", prefillNode)
        : prefillNode);

    // Persist newly computed blocks onto the selected prefill node.
    for (let i = matchedBlocks; i < blocks.length; i++) {
      const b = blocks[i]!;
      this.stmtUpsertBlock.run(
        b.hash,
        model,
        prefillNode,
        b.blockIndex,
        b.parentHash,
        b.tokenCount,
        now,
        now,
      );
    }

    const prefillSpec = this.nodes.get(prefillNode) ?? DEFAULT_CLUSTER_NODES[0]!;
    const totalTokens = tokens.length;
    const uncachedPrefillTokens = Math.max(0, totalTokens - savedPrefillTokens);
    const baselineTtftMs = Number(
      (prefillSpec.baseTtftMs + totalTokens * prefillSpec.msPerPrefillToken).toFixed(3),
    );
    const kvTransferOverheadMs = disaggregated && prefillNode !== decodeNode ? 1.5 : 0;
    const estimatedTtftMs = Number(
      (
        prefillSpec.baseTtftMs +
        uncachedPrefillTokens * prefillSpec.msPerPrefillToken +
        kvTransferOverheadMs
      ).toFixed(3),
    );
    const ttftReductionMs = Number(Math.max(0, baselineTtftMs - estimatedTtftMs).toFixed(3));
    const ttftReductionPct =
      baselineTtftMs > 0 ? Number(((ttftReductionMs / baselineTtftMs) * 100).toFixed(2)) : 0;
    const prefixHitRatio =
      totalTokens > 0 ? Number((savedPrefillTokens / totalTokens).toFixed(4)) : 0;
    const kvCacheHit = matchedBlocks > 0;

    this.stmtIncrementNodePrefill.run(prefillNode);
    this.stmtIncrementNodeDecode.run(decodeNode);
    this.stmtInsertRouteEvent.run(
      model,
      prefillNode,
      decodeNode,
      totalTokens,
      matchedBlocks,
      savedPrefillTokens,
      kvCacheHit ? 1 : 0,
      ttftReductionPct,
      now,
    );

    return {
      ok: true,
      model,
      disaggregated,
      prefillNode,
      decodeNode,
      totalTokens,
      totalBlocks: blocks.length,
      matchedBlocks,
      savedPrefillTokens,
      uncachedPrefillTokens,
      kvCacheHit,
      prefixHitRatio,
      baselineTtftMs,
      estimatedTtftMs,
      ttftReductionMs,
      ttftReductionPct,
      prefixBlockHashes: blocks.map((b) => b.hash),
    };
  }

  upsertDocument(input: RagDocumentInput): { ok: true; docKey: string; tokenCount: number } {
    const now = Date.now();
    const metadataJson = JSON.stringify(input.metadata ?? {});
    const embedding = input.embedding
      ? toFloat32Array(input.embedding)
      : embedTextDeterministic(`${input.title}\n${input.content}`);
    const embeddingBytes = float32ToBytes(embedding);
    const tokenCount = tokenizePrompt(`${input.title} ${input.content}`).length;

    const existing = this.stmtSelectDocId.get(input.docKey) as { id: number } | null;
    let rowId: number;
    if (existing) {
      rowId = existing.id;
      this.stmtUpdateDoc.run(
        input.title,
        input.content,
        metadataJson,
        embeddingBytes,
        tokenCount,
        now,
        rowId,
      );
      this.stmtDeleteFtsRow.run(rowId);
    } else {
      const info = this.stmtInsertDoc.run(
        input.docKey,
        input.title,
        input.content,
        metadataJson,
        embeddingBytes,
        tokenCount,
        now,
      );
      rowId = Number(info.lastInsertRowid);
    }

    this.stmtInsertFtsRow.run(rowId, input.docKey, input.title, input.content);
    return { ok: true, docKey: input.docKey, tokenCount };
  }

  hybridSearch(query: string, options: HybridSearchOptions = {}): HybridSearchHit[] {
    const limit = Math.max(1, options.limit ?? 5);
    const ftsWeight = options.ftsWeight ?? 0.55;
    const vectorWeight = options.vectorWeight ?? 0.45;
    const queryVec = options.queryEmbedding
      ? toFloat32Array(options.queryEmbedding)
      : embedTextDeterministic(query);

    const ftsScores = new Map<string, number>();
    const ftsExpr = buildFtsQuery(query);
    if (ftsExpr) {
      const ftsRows = this.stmtFtsSearch.all(ftsExpr, limit * 4) as Array<{
        doc_key: string;
        bm25_score: number;
      }>;
      for (const row of ftsRows) {
        // SQLite bm25() returns negative scores (more negative = better match).
        const normalized = 1 / (1 + Math.exp(row.bm25_score));
        ftsScores.set(row.doc_key, normalized);
      }
    }

    const docs = this.stmtAllDocs.all() as Array<{
      doc_key: string;
      title: string;
      content: string;
      metadata_json: string;
      embedding_blob: Uint8Array;
    }>;

    const scored: HybridSearchHit[] = docs.map((doc) => {
      const docVec = toFloat32Array(doc.embedding_blob);
      const rawCosine = cosineSimilarity(queryVec, docVec);
      const vectorScore = Number(Math.max(0, (rawCosine + 1) / 2).toFixed(6));
      const ftsScore = Number((ftsScores.get(doc.doc_key) ?? 0).toFixed(6));
      const hybridScore = Number((ftsWeight * ftsScore + vectorWeight * vectorScore).toFixed(6));
      return {
        docKey: doc.doc_key,
        title: doc.title,
        content: doc.content,
        metadata: JSON.parse(doc.metadata_json) as Record<string, unknown>,
        ftsScore,
        vectorScore,
        hybridScore,
      };
    });

    scored.sort((a, b) => b.hybridScore - a.hybridScore || a.docKey.localeCompare(b.docKey));
    return scored.slice(0, limit);
  }

  getStats(): KvStoreStats {
    const agg = this.stmtRouteAggregate.get() as {
      total_routes: number;
      total_hits: number;
      total_saved_tokens: number;
      avg_ttft_reduction_pct: number;
    };
    const blockCnt = (this.stmtCountBlocks.get() as { cnt: number }).cnt;
    const docCnt = (this.stmtCountDocs.get() as { cnt: number }).cnt;
    const modeStmt = this.db.prepare("PRAGMA journal_mode;");
    const modeRow = modeStmt.get() as { journal_mode: string } | null;
    modeStmt.finalize();

    const nodeRows = this.stmtListNodeStats.all() as Array<{
      id: ClusterNodeId;
      role: ClusterNodeSpec["role"];
      prefill_count: number;
      decode_count: number;
      cached_blocks: number;
    }>;

    return {
      totalRoutes: agg.total_routes,
      totalCacheHits: agg.total_hits,
      cacheHitRate:
        agg.total_routes > 0 ? Number((agg.total_hits / agg.total_routes).toFixed(4)) : 0,
      totalSavedPrefillTokens: agg.total_saved_tokens,
      avgTtftReductionPct: Number(agg.avg_ttft_reduction_pct.toFixed(2)),
      cachedKvBlocks: blockCnt,
      ragDocuments: docCnt,
      journalMode: modeRow?.journal_mode ?? "unknown",
      nodes: nodeRows.map((r) => ({
        id: r.id,
        role: r.role,
        cachedBlocks: r.cached_blocks,
        prefillCount: r.prefill_count,
        decodeCount: r.decode_count,
      })),
    };
  }

  exportSnapshotBytes(): {
    gzBytes: Uint8Array;
    meta: Omit<SnapshotExportResult, "payloadBase64">;
  } {
    const blocks = this.stmtAllBlocks.all();
    const docs = (
      this.stmtAllDocs.all() as Array<{
        doc_key: string;
        title: string;
        content: string;
        metadata_json: string;
        token_count: number;
        updated_at: number;
      }>
    ).map((d) => ({
      docKey: d.doc_key,
      title: d.title,
      content: d.content,
      metadata: JSON.parse(d.metadata_json) as Record<string, unknown>,
      tokenCount: d.token_count,
      updatedAt: d.updated_at,
    }));

    const createdAt = new Date().toISOString();
    const payloadObj = {
      format: "fw-kv-rag-v1",
      createdAt,
      stats: this.getStats(),
      kvBlocks: blocks,
      ragDocs: docs,
    };
    const rawBytes = new TextEncoder().encode(JSON.stringify(payloadObj));
    const gzBytes = Bun.gzipSync(rawBytes, { level: 6 });
    const sha256 = sha256Hex(gzBytes);

    return {
      gzBytes,
      meta: {
        format: "fw-kv-rag-v1+gzip",
        createdAt,
        rawBytes: rawBytes.byteLength,
        compressedBytes: gzBytes.byteLength,
        compressionRatio:
          rawBytes.byteLength > 0
            ? Number((gzBytes.byteLength / rawBytes.byteLength).toFixed(4))
            : 1,
        sha256,
        kvBlockCount: blocks.length,
        ragDocCount: docs.length,
      },
    };
  }

  exportSnapshot(): SnapshotExportResult {
    const { gzBytes, meta } = this.exportSnapshotBytes();
    return {
      ...meta,
      payloadBase64: Buffer.from(gzBytes).toString("base64"),
    };
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    for (const stmt of this.statements) {
      stmt.finalize();
    }
    this.statements.length = 0;
    this.db.close();
  }

  [Symbol.dispose](): void {
    this.close();
  }
}

// ============================================================================
// Pillar 3: Zero-Copy Streaming SSE / OpenAI / MCP Disaggregated P/D Router
// ============================================================================

export interface BunNativeAiHandlerOptions {
  store?: PrefixKvRagStore;
  tools?: RuntimeToolRegistry;
  defaultModel?: string;
}

export interface ChatMessage {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
}

function jsonResponse(data: unknown, status = 200, extraHeaders?: Record<string, string>): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "access-control-allow-origin": "*",
      ...extraHeaders,
    },
  });
}

async function readJsonBody<T>(req: Request): Promise<T | null> {
  try {
    return (await req.json()) as T;
  } catch {
    return null;
  }
}

export function createBunNativeAiHandler(options: BunNativeAiHandlerOptions = {}) {
  const store = options.store ?? new PrefixKvRagStore(":memory:");
  const tools = options.tools ?? new RuntimeToolRegistry();
  const defaultModel = options.defaultModel ?? "fw-qwen3-8b-gguf";

  return async (req: Request): Promise<Response> => {
    const url = new URL(req.url);

    if (req.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: {
          "access-control-allow-origin": "*",
          "access-control-allow-methods": "GET, POST, OPTIONS",
          "access-control-allow-headers": "Content-Type, Authorization, Accept",
        },
      });
    }

    // 1. GET /v1/ai/health
    if (req.method === "GET" && url.pathname === "/v1/ai/health") {
      const kvStats = store.getStats();
      return jsonResponse({
        ok: true,
        runtime: "bun",
        bunVersion: Bun.version,
        defaultModel,
        pillars: [
          "runtime-ts-tsx-tool-compiler",
          "llmd-vllm-prefix-kv-fts5-vector-rag",
          "zero-copy-sse-openai-mcp-pd-router",
        ],
        toolCount: tools.listTools().length,
        kvStats,
      });
    }

    // 2. POST /v1/ai/kv/route
    if (req.method === "POST" && url.pathname === "/v1/ai/kv/route") {
      const body = await readJsonBody<
        KvRouteRequest & { ragQuery?: string; ragLimit?: number }
      >(req);
      if (!body || typeof body.prompt !== "string") {
        return jsonResponse({ ok: false, error: "Request body must include string `prompt`" }, 400);
      }

      const decision = store.routePrefix({
        prompt: body.prompt,
        model: body.model ?? defaultModel,
        disaggregated: body.disaggregated,
        preferredPrefillNode: body.preferredPrefillNode,
        preferredDecodeNode: body.preferredDecodeNode,
      });

      const ragContext = body.ragQuery
        ? store.hybridSearch(body.ragQuery, { limit: body.ragLimit ?? 3 })
        : undefined;

      return jsonResponse(
        ragContext ? { ...decision, ragContext } : decision,
        200,
        {
          "x-fw-prefill-node": decision.prefillNode,
          "x-fw-decode-node": decision.decodeNode,
          "x-fw-kv-hit": String(decision.kvCacheHit),
          "x-fw-saved-tokens": String(decision.savedPrefillTokens),
        },
      );
    }

    // 3. POST /v1/ai/tools/compile
    if (req.method === "POST" && url.pathname === "/v1/ai/tools/compile") {
      const body = await readJsonBody<
        CompileToolOptions & { execute?: boolean; args?: unknown; ctx?: Record<string, unknown> }
      >(req);
      if (!body || typeof body.name !== "string" || typeof body.source !== "string") {
        return jsonResponse(
          { ok: false, error: "Request body must include string `name` and `source`" },
          400,
        );
      }

      const compiled = tools.compileAndRegister({
        name: body.name,
        source: body.source,
        loader: body.loader ?? "ts",
        sandbox: body.sandbox ?? true,
        description: body.description,
      });

      if (!compiled.ok) {
        const status =
          compiled.blockedImports.length > 0 || compiled.blockedGlobals.length > 0 ? 403 : 400;
        return jsonResponse(compiled, status);
      }

      if (body.execute) {
        const execution = await tools.execute(body.name, body.args ?? {}, body.ctx ?? {});
        return jsonResponse({
          ...compiled,
          execution,
          telemetry: tools.getTool(body.name),
        });
      }

      return jsonResponse({
        ...compiled,
        telemetry: tools.getTool(body.name),
      });
    }

    // 4. POST /v1/ai/chat/stream (OpenAI-compatible SSE + P/D KV routing)
    if (req.method === "POST" && url.pathname === "/v1/ai/chat/stream") {
      const body = await readJsonBody<{
        model?: string;
        messages?: ChatMessage[];
        prompt?: string;
        stream?: boolean;
        disaggregated?: boolean;
        toolName?: string;
        toolArgs?: unknown;
      }>(req);

      if (!body) {
        return jsonResponse({ ok: false, error: "Invalid JSON body" }, 400);
      }

      const model = body.model ?? defaultModel;
      const promptText =
        body.prompt ??
        (Array.isArray(body.messages)
          ? body.messages.map((m) => `${m.role}: ${m.content}`).join("\n")
          : "");

      if (!promptText) {
        return jsonResponse({ ok: false, error: "Provide `prompt` or non-empty `messages`" }, 400);
      }

      const route = store.routePrefix({
        prompt: promptText,
        model,
        disaggregated: body.disaggregated ?? true,
      });

      let toolOutputSummary = "";
      if (body.toolName) {
        const exec = await tools.execute(body.toolName, body.toolArgs ?? {});
        toolOutputSummary = exec.ok
          ? ` [tool:${body.toolName}=${JSON.stringify(exec.output)}]`
          : ` [tool:${body.toolName}:error=${exec.error}]`;
      }

      const completionId = `chatcmpl-fw-${sha256Hex(`${Date.now()}:${promptText}`).slice(0, 12)}`;
      const created = Math.floor(Date.now() / 1000);
      const replyTokens = [
        `[${route.prefillNode}->${route.decodeNode}]`,
        route.kvCacheHit
          ? ` kv_hit=${route.savedPrefillTokens}tok`
          : ` kv_miss=${route.totalTokens}tok`,
        ` ttft=${route.estimatedTtftMs}ms`,
        toolOutputSummary,
      ].filter((t) => t.length > 0);

      const commonHeaders = {
        "access-control-allow-origin": "*",
        "x-fw-prefill-node": route.prefillNode,
        "x-fw-decode-node": route.decodeNode,
        "x-fw-kv-hit": String(route.kvCacheHit),
        "x-fw-saved-tokens": String(route.savedPrefillTokens),
      };

      if (body.stream === false) {
        return jsonResponse(
          {
            id: completionId,
            object: "chat.completion",
            created,
            model,
            choices: [
              {
                index: 0,
                message: { role: "assistant", content: replyTokens.join("") },
                finish_reason: "stop",
              },
            ],
            fw: route,
          },
          200,
          commonHeaders,
        );
      }

      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          // Role delta chunk
          const roleChunk = {
            id: completionId,
            object: "chat.completion.chunk",
            created,
            model,
            choices: [{ index: 0, delta: { role: "assistant" }, finish_reason: null }],
            fw: {
              prefillNode: route.prefillNode,
              decodeNode: route.decodeNode,
              kvCacheHit: route.kvCacheHit,
              savedPrefillTokens: route.savedPrefillTokens,
            },
          };
          controller.enqueue(encoder.encode(`data: ${JSON.stringify(roleChunk)}\n\n`));

          // Token delta chunks
          for (const token of replyTokens) {
            const chunk = {
              id: completionId,
              object: "chat.completion.chunk",
              created,
              model,
              choices: [{ index: 0, delta: { content: token }, finish_reason: null }],
            };
            controller.enqueue(encoder.encode(`data: ${JSON.stringify(chunk)}\n\n`));
          }

          // Final stop chunk + [DONE]
          const stopChunk = {
            id: completionId,
            object: "chat.completion.chunk",
            created,
            model,
            choices: [{ index: 0, delta: {}, finish_reason: "stop" }],
          };
          controller.enqueue(encoder.encode(`data: ${JSON.stringify(stopChunk)}\n\n`));
          controller.enqueue(encoder.encode("data: [DONE]\n\n"));
          controller.close();
        },
      });

      return new Response(stream, {
        status: 200,
        headers: {
          ...commonHeaders,
          "content-type": "text/event-stream; charset=utf-8",
          "cache-control": "no-cache",
          connection: "keep-alive",
        },
      });
    }

    // 5. POST /v1/ai/snapshot (Bun.gzipSync compressed KV + FTS5 export)
    if (req.method === "POST" && url.pathname === "/v1/ai/snapshot") {
      const wantRawGzip =
        url.searchParams.get("raw") === "1" ||
        (req.headers.get("accept") ?? "").includes("application/gzip");

      if (wantRawGzip) {
        const { gzBytes, meta } = store.exportSnapshotBytes();
        return new Response(gzBytes, {
          status: 200,
          headers: {
            "content-type": "application/gzip",
            "content-encoding": "gzip",
            "access-control-allow-origin": "*",
            "x-fw-snapshot-sha256": meta.sha256,
            "x-fw-snapshot-raw-bytes": String(meta.rawBytes),
            "x-fw-snapshot-kv-blocks": String(meta.kvBlockCount),
            "x-fw-snapshot-rag-docs": String(meta.ragDocCount),
          },
        });
      }

      return jsonResponse({ ok: true, ...store.exportSnapshot() });
    }

    // 6. POST /v1/ai/rag/upsert & POST /v1/ai/rag/search
    if (req.method === "POST" && url.pathname === "/v1/ai/rag/upsert") {
      const body = await readJsonBody<RagDocumentInput>(req);
      if (!body || !body.docKey || !body.title || typeof body.content !== "string") {
        return jsonResponse({ ok: false, error: "Require `docKey`, `title`, and `content`" }, 400);
      }
      return jsonResponse(store.upsertDocument(body));
    }

    if (req.method === "POST" && url.pathname === "/v1/ai/rag/search") {
      const body = await readJsonBody<{ query: string } & HybridSearchOptions>(req);
      if (!body || typeof body.query !== "string") {
        return jsonResponse({ ok: false, error: "Require string `query`" }, 400);
      }
      return jsonResponse({
        ok: true,
        query: body.query,
        hits: store.hybridSearch(body.query, body),
      });
    }

    // 7. POST /v1/ai/mcp (JSON-RPC 2.0 MCP bridge for registered tools & KV/RAG)
    if (req.method === "POST" && url.pathname === "/v1/ai/mcp") {
      const rpc = await readJsonBody<{
        jsonrpc?: string;
        id?: string | number | null;
        method?: string;
        params?: Record<string, unknown>;
      }>(req);

      if (!rpc || rpc.jsonrpc !== "2.0" || typeof rpc.method !== "string") {
        return jsonResponse(
          { jsonrpc: "2.0", id: rpc?.id ?? null, error: { code: -32600, message: "Invalid JSON-RPC 2.0 request" } },
          400,
        );
      }

      if (rpc.method === "tools/list") {
        return jsonResponse({
          jsonrpc: "2.0",
          id: rpc.id ?? null,
          result: {
            tools: tools.listTools().map((t) => ({
              name: t.name,
              description: t.description,
              version: t.version,
              sandbox: t.sandbox,
            })),
          },
        });
      }

      if (rpc.method === "tools/call") {
        const name = String(rpc.params?.name ?? "");
        const args = rpc.params?.arguments ?? {};
        const exec = await tools.execute(name, args);
        return jsonResponse({
          jsonrpc: "2.0",
          id: rpc.id ?? null,
          result: exec,
        });
      }

      return jsonResponse(
        { jsonrpc: "2.0", id: rpc.id ?? null, error: { code: -32601, message: `Method not found: ${rpc.method}` } },
        404,
      );
    }

    return jsonResponse({ ok: false, error: `Not found: ${req.method} ${url.pathname}` }, 404);
  };
}
