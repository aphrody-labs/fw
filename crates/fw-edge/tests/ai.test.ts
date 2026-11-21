// Workers AI gateway parity for the Rust fw-edge. `wrangler dev --local` cannot reach Workers AI (the binding is
// remote-only), so the model catalog, chat, SSE re-framing and embeddings semantics are proven by the native unit
// tests in src/ai.rs (mock Inference, the assertions of tests/fw_cloudflare.test.ts). Here: the request validation
// that runs before any AI call, compared byte for byte with the TypeScript worker, and the live checks against a
// deployed preview when FW_EDGE_PREVIEW_URL is set (the coordinator's preview step).
// Run from crates/fw-edge after `worker-build --release`:  bun test tests/ai.test.ts
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import worker, { type WorkersAILike } from "../../../cloudflare/worker.ts";
import { startEdge, type Edge } from "./harness.ts";

const preview = process.env.FW_EDGE_PREVIEW_URL?.replace(/\/$/, "");
let edge: Edge | undefined;
let persist = "";

beforeAll(async () => {
  persist = mkdtempSync(join(tmpdir(), "fw-edge-ai-"));
  edge = await startEdge({ persistTo: persist });
}, 180_000);

afterAll(async () => {
  await edge?.stop();
  rmSync(persist, { recursive: true, force: true });
});

const unusedAi: WorkersAILike = {
  async run() {
    throw new Error("validation must fail before any AI call");
  },
};
const ts = (path: string, init?: RequestInit) => worker.fetch(new Request(`https://edge.fw.test${path}`, init), { AI: unusedAi });
const rs = (path: string, init?: RequestInit) => fetch(`${edge!.url}${path}`, init);
const post = (body: string): RequestInit => ({ method: "POST", headers: { "content-type": "application/json" }, body });

describe("request validation (before any Workers AI call)", () => {
  const cases: Array<[string, string, string]> = [
    ["chat: empty object", "/v1/chat/completions", "{}"],
    ["chat: not JSON", "/v1/chat/completions", "not json"],
    ["chat: empty messages", "/v1/chat/completions", '{"messages":[]}'],
    ["chat: messages not an array", "/v1/chat/completions", '{"messages":"hi"}'],
    ["embeddings: empty object", "/v1/embeddings", "{}"],
    ["embeddings: empty array", "/v1/embeddings", '{"input":[]}'],
    ["embeddings: number input", "/v1/embeddings", '{"input":5}'],
  ];
  for (const [name, path, body] of cases) {
    test(name, async () => {
      const [a, b] = await Promise.all([rs(path, post(body)), ts(path, post(body))]);
      expect(a.status).toBe(400);
      expect(a.status).toBe(b.status);
      expect(a.headers.get("content-type")).toBe("application/json; charset=utf-8");
      expect(a.headers.get("access-control-allow-origin")).toBe("*");
      expect(await a.text()).toBe(await b.text());
    });
  }

  test("GET on the POST-only AI routes is the 404 JSON", async () => {
    for (const path of ["/v1/chat/completions", "/v1/embeddings"]) {
      const [a, b] = await Promise.all([rs(path), ts(path)]);
      expect(a.status).toBe(404);
      expect(await a.text()).toBe(await b.text());
    }
  });

  test("ACP session/prompt and A2A tasks/send surface an AI failure as JSON-RPC -32001, not HTTP 5xx", async () => {
    // wrangler dev --local has no reachable Workers AI: the call fails and must come back as a JSON-RPC error.
    const acp = (await (await rs("/acp", post(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "session/prompt", params: { model: "@cf/test/none", prompt: "x" } })))).json()) as { error?: { code: number } };
    const a2a = (await (await rs("/a2a", post(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tasks/send", params: { model: "@cf/test/none", prompt: "x" } })))).json()) as { error?: { code: number } };
    expect(acp.error?.code).toBe(-32001);
    expect(a2a.error?.code).toBe(-32001);
  }, 60_000);
});

describe.skipIf(!preview)("live Workers AI against the preview deployment (FW_EDGE_PREVIEW_URL)", () => {
  const live = (path: string, init?: RequestInit) => fetch(`${preview}${path}`, init);

  test("/v1/models: ranked catalog, tool-calling first, default is the largest free tool-calling model", async () => {
    const res = await live("/v1/models");
    expect(res.status).toBe(200);
    const body = (await res.json()) as { object: string; default: string; data: Array<{ id: string; function_calling: boolean; paid: boolean; context_window?: number }> };
    expect(body.object).toBe("list");
    expect(body.data.length).toBeGreaterThan(0);
    expect(body.data.every((m) => m.id.startsWith("@cf/"))).toBe(true);
    const firstPlain = body.data.findIndex((m) => !m.function_calling);
    if (firstPlain >= 0) expect(body.data.slice(firstPlain).some((m) => m.function_calling)).toBe(false);
    expect(body.default).toBe((body.data.find((m) => m.function_calling && !m.paid) ?? body.data[0])!.id);
  });

  test("/v1/chat/completions JSON and SSE", async () => {
    const msg = { messages: [{ role: "user", content: "Reply with the single word: pong" }], max_tokens: 16 };
    const json = (await (await live("/v1/chat/completions", post(JSON.stringify(msg)))).json()) as { object: string; choices: Array<{ message: { content: string } }> };
    expect(json.object).toBe("chat.completion");
    expect(json.choices[0]?.message.content.length).toBeGreaterThan(0);

    const sse = await live("/v1/chat/completions", post(JSON.stringify({ ...msg, stream: true })));
    expect(sse.headers.get("content-type")).toContain("text/event-stream");
    const text = await sse.text();
    expect(text.trim().endsWith("data: [DONE]")).toBe(true);
    const frames = text.split("\n\n").filter((f) => f.startsWith("data: {")).map((f) => JSON.parse(f.slice(6)));
    expect(frames[0].choices[0].delta.role).toBe("assistant");
    expect(frames.at(-1).choices[0].finish_reason).toBe("stop");
  }, 60_000);

  test("/v1/embeddings returns one vector per input", async () => {
    const res = (await (await live("/v1/embeddings", post(JSON.stringify({ input: ["alpha", "beta"] })))).json()) as { object: string; source: string; data: Array<{ embedding: number[] }> };
    expect(res.object).toBe("list");
    expect(res.source).toBe("workers-ai");
    expect(res.data.length).toBe(2);
    expect(res.data[0]!.embedding.length).toBeGreaterThan(8);
  }, 60_000);
});
