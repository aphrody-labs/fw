import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { get, head } from "../src/http-curl.ts";

// curl_get is synchronous (it blocks the event loop), so the test server must run in a
// separate process to be able to answer the requests.

let serverProc: ReturnType<typeof Bun.spawn>;
let baseUrl: string;

beforeAll(async () => {
  const port = 19827;
  serverProc = Bun.spawn(
    [
      process.execPath,
      "-e",
      `
const s = Bun.serve({
  port: ${port},
  fetch(req) {
    const p = new URL(req.url).pathname;
    if (p === "/404") return new Response("not found", { status: 404 });
    if (p === "/json") return Response.json({ ok: true });
    return new Response("hello curl", { status: 200 });
  },
});
`,
    ],
    { stdout: "ignore", stderr: "ignore" }
  );
  baseUrl = `http://127.0.0.1:${port}`;
  // Wait for the server to be ready
  for (let i = 0; i < 20; i++) {
    try {
      await fetch(baseUrl + "/");
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 100));
    }
  }
});

afterAll(() => {
  serverProc?.kill();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("http-curl: get()", () => {
  test("1. returns an object { body: string, status: number }", async () => {
    const result = await get(`${baseUrl}/`);
    expect(result).toHaveProperty("body");
    expect(result).toHaveProperty("status");
    expect(typeof result.body).toBe("string");
    expect(typeof result.status).toBe("number");
  }, 10_000);

  test("2. status is 200 for a valid URL", async () => {
    const { status } = await get(`${baseUrl}/`);
    expect(status).toBe(200);
  }, 10_000);

  test("3. body is a non-empty string", async () => {
    const { body } = await get(`${baseUrl}/`);
    expect(body.length).toBeGreaterThan(0);
    expect(body).toBe("hello curl");
  }, 10_000);

  test("4. returns 404 for a missing route", async () => {
    const { status } = await get(`${baseUrl}/404`);
    expect(status).toBe(404);
  }, 10_000);

  test("get() returns a JSON body untouched", async () => {
    const { body } = await get(`${baseUrl}/json`);
    expect(JSON.parse(body)).toEqual({ ok: true });
  }, 10_000);
});

describe("http-curl: head()", () => {
  test("5. head() returns { status: 200 } for a valid URL", async () => {
    const result = await head(`${baseUrl}/`);
    expect(result).toHaveProperty("status");
    expect(result.status).toBe(200);
  }, 10_000);

  test("6. head() does not throw on a valid URL", async () => {
    await expect(head(`${baseUrl}/`)).resolves.toBeDefined();
  }, 10_000);

  test("7. invalid URL (non-existent host) -> explicit error", async () => {
    await expect(get("http://this-host-does-not-exist-xyz-42.invalid/")).rejects.toThrow("curl GET failed");
  }, 10_000);

  test("head() reports 404 and rejects an unreachable host", async () => {
    expect((await head(`${baseUrl}/404`)).status).toBe(404);
    await expect(head("http://this-host-does-not-exist-xyz-42.invalid/")).rejects.toThrow("curl HEAD failed");
  }, 10_000);
});
