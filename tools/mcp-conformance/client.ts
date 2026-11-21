// Minimal MCP clients for the conformance tooling: stdio and Streamable HTTP, specification 2026-07-28 and the
// earlier initialize-based versions. Every message is recorded so a session can be shown as a transcript.

export const MODERN = "2026-07-28";
const META = "io.modelcontextprotocol";

export type Json = Record<string, any>;
export type Entry = { dir: "->" | "<-"; message: Json };

export interface Client {
  era: "modern" | "legacy";
  transcript: Entry[];
  request(method: string, params?: Json, extraMeta?: Json): Promise<Json>;
  notify(method: string, params?: Json): Promise<void>;
  close(): Promise<void>;
}

const modernMeta = (extra: Json = {}): Json => ({
  [`${META}/protocolVersion`]: MODERN,
  [`${META}/clientInfo`]: { name: "fw-mcp-conformance", version: "1" },
  [`${META}/clientCapabilities`]: {},
  ...extra,
});

function body(era: Client["era"], id: number, method: string, params: Json, extraMeta?: Json): Json {
  const withMeta = era === "modern" ? { ...params, _meta: modernMeta(extraMeta) } : extraMeta ? { ...params, _meta: extraMeta } : params;
  return { jsonrpc: "2.0", id, method, ...(method === "initialize" || era === "modern" || Object.keys(withMeta).length ? { params: withMeta } : {}) };
}

export async function stdioClient(command: string[], options: { cwd?: string; env?: Record<string, string>; era?: Client["era"] } = {}): Promise<Client> {
  const era = options.era ?? "modern";
  const proc = Bun.spawn(command, { cwd: options.cwd, env: { ...process.env, ...options.env }, stdin: "pipe", stdout: "pipe", stderr: "pipe" });
  const transcript: Entry[] = [];
  const waiting = new Map<number, { resolve: (v: Json) => void; notifications: Json[] }>();
  const orphans: Json[] = [];
  let nextId = 1;
  (async () => {
    const decoder = new TextDecoder();
    let buffer = "";
    for await (const chunk of proc.stdout) {
      buffer += decoder.decode(chunk, { stream: true });
      for (let nl = buffer.indexOf("\n"); nl >= 0; nl = buffer.indexOf("\n")) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (!line) continue;
        const message = JSON.parse(line) as Json;
        transcript.push({ dir: "<-", message });
        const pending = message.id === undefined ? undefined : waiting.get(message.id);
        if (pending) {
          waiting.delete(message.id);
          pending.resolve({ ...message, notifications: pending.notifications });
        } else if (message.method) {
          // Request-scoped notifications belong to the one request still waiting when only one is; otherwise keep them aside.
          const [only] = waiting.values();
          if (waiting.size === 1 && only) only.notifications.push(message);
          else orphans.push(message);
        }
      }
    }
  })();
  const send = (message: Json) => {
    transcript.push({ dir: "->", message });
    proc.stdin.write(`${JSON.stringify(message)}\n`);
    proc.stdin.flush();
  };
  return {
    era,
    transcript,
    request(method, params = {}, extraMeta) {
      const id = nextId++;
      return new Promise<Json>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`no response to ${method} (id ${id}) within 60 s`)), 60_000);
        waiting.set(id, { resolve: (v) => (clearTimeout(timer), resolve(v)), notifications: [] });
        send(body(era, id, method, params, extraMeta));
      });
    },
    async notify(method, params) {
      send({ jsonrpc: "2.0", method, ...(params ? { params } : {}) });
    },
    async close() {
      proc.stdin.end();
      const exited = await Promise.race([proc.exited, Bun.sleep(5000).then(() => "timeout" as const)]);
      if (exited === "timeout") proc.kill();
    },
  };
}

function* sseMessages(text: string): Generator<Json> {
  for (const block of text.split(/\n\n/)) {
    const data = block.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).trimStart()).join("\n");
    if (data) yield JSON.parse(data) as Json;
  }
}

export async function httpClient(url: string, options: { era?: Client["era"]; extraHeaders?: Record<string, string> } = {}): Promise<Client> {
  const era = options.era ?? "modern";
  const transcript: Entry[] = [];
  let nextId = 1;
  let legacyVersion = "2025-11-25";
  const post = async (message: Json): Promise<Json> => {
    transcript.push({ dir: "->", message });
    const headers: Record<string, string> = { "content-type": "application/json", accept: "application/json, text/event-stream", ...options.extraHeaders };
    if (era === "modern") {
      headers["mcp-protocol-version"] = MODERN;
      headers["mcp-method"] = message.method;
      const name = message.params?.name ?? message.params?.uri;
      if (name !== undefined && ["tools/call", "prompts/get", "resources/read"].includes(message.method)) headers["mcp-name"] = String(name);
    } else if (message.method !== "initialize") headers["mcp-protocol-version"] = legacyVersion;
    const res = await fetch(url, { method: "POST", headers, body: JSON.stringify(message) });
    const text = await res.text();
    const messages = res.headers.get("content-type")?.includes("text/event-stream") ? [...sseMessages(text)] : text ? [JSON.parse(text) as Json] : [];
    for (const m of messages) transcript.push({ dir: "<-", message: m });
    const final = messages.find((m) => m.id !== undefined) ?? {};
    return { ...final, notifications: messages.filter((m) => m.method), httpStatus: res.status };
  };
  return {
    era,
    transcript,
    async request(method, params = {}, extraMeta) {
      const result = await post(body(era, nextId++, method, params, extraMeta));
      if (method === "initialize" && result.result?.protocolVersion) legacyVersion = result.result.protocolVersion;
      return result;
    },
    async notify(method, params) {
      await post({ jsonrpc: "2.0", method, ...(params ? { params } : {}) });
    },
    async close() {},
  };
}

/** Compact one-line-per-message transcript for reports. */
export function formatTranscript(entries: Entry[], limit = 220): string {
  return entries
    .map(({ dir, message }) => {
      const text = JSON.stringify(message);
      return `${dir} ${text.length > limit ? `${text.slice(0, limit)}…(${text.length} bytes)` : text}`;
    })
    .join("\n");
}
