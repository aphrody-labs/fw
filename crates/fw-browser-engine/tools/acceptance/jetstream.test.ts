import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { mkdirp, mkdtemp, rmrf, tmpdir } from "./sys";
import { ISOLATION_HEADERS, jetstream, mimeOf, parseResults, serveStatic } from "./jetstream";

describe("static server", () => {
  const base = join(tmpdir(), `jetstream-test-${crypto.randomUUID().slice(0, 8)}`);
  const root = join(base, "root");
  let server: ReturnType<typeof serveStatic>;

  beforeAll(async () => {
    await mkdirp(join(root, "sub")); // Bun.write creates the directories of a file, mkdirp is for the empty one
    await Bun.write(join(root, "index.html"), "<title>x</title>");
    await Bun.write(join(root, "sub", "index.html"), "<title>sub</title>");
    await Bun.write(join(root, "a.wasm"), new Uint8Array([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]));
    await Bun.write(join(root, "m.mjs"), "export default 1");
    await Bun.write(join(root, "blob.js.z"), "zz");
    await Bun.write(join(base, "secret.txt"), "secret");
    server = serveStatic(root);
  });
  afterAll(async () => {
    server.stop();
    await rmrf(base);
  });

  test("binds 127.0.0.1 on an ephemeral port", () => {
    expect(server.url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
  });

  test("MIME types", async () => {
    const type = async (path: string) => (await fetch(`${server.url}${path}`)).headers.get("content-type");
    expect(await type("/a.wasm")).toBe("application/wasm");
    expect(await type("/m.mjs")).toBe("text/javascript; charset=utf-8");
    expect(await type("/index.html")).toBe("text/html; charset=utf-8");
    expect(await type("/blob.js.z")).toBe("application/octet-stream");
    expect(mimeOf("x.JSON")).toBe("application/json; charset=utf-8");
    expect(mimeOf("noextension")).toBe("application/octet-stream");
  });

  test("cross-origin isolation headers on every response", async () => {
    for (const path of ["/index.html", "/missing"]) {
      const response = await fetch(`${server.url}${path}`);
      for (const [name, value] of Object.entries(ISOLATION_HEADERS)) expect(response.headers.get(name)).toBe(value);
    }
    expect(ISOLATION_HEADERS["Cross-Origin-Opener-Policy"]).toBe("same-origin");
    expect(ISOLATION_HEADERS["Cross-Origin-Embedder-Policy"]).toBe("require-corp");
  });

  test("bytes, directory index, 404, HEAD, no escape from the root", async () => {
    const wasm = new Uint8Array(await (await fetch(`${server.url}/a.wasm`)).arrayBuffer());
    expect([...wasm]).toEqual([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]);
    expect(await (await fetch(`${server.url}/`)).text()).toBe("<title>x</title>");
    expect(await (await fetch(`${server.url}/sub/`)).text()).toBe("<title>sub</title>");
    expect(await (await fetch(`${server.url}/sub`)).text()).toBe("<title>sub</title>");
    expect((await fetch(`${server.url}/nope.js`)).status).toBe(404);
    const head = await fetch(`${server.url}/a.wasm`, { method: "HEAD" });
    expect(head.status).toBe(200);
    expect(head.headers.get("content-length")).toBe("8");
    expect((await fetch(`${server.url}/%2e%2e/secret.txt`)).status).toBe(404);
    expect((await fetch(`${server.url}/index.html`, { method: "POST" })).status).toBe(405);
  });
});

describe("result parsing", () => {
  const simple = {
    "a-wasm": { Score: 100, First: 50, Worst: 80, Average: 120 },
    "b-js": { Score: 400, First: 300 },
  };

  test("per-workload scores and the suite's own summary", () => {
    const parsed = parseResults(simple, "200.00");
    expect(parsed.tests).toEqual({ "a-wasm": 100, "b-js": 400 });
    expect(parsed.score).toBe(200);
    expect(parsed.failed).toEqual([]);
  });

  test("geometric mean when the page shows no summary", () => {
    expect(parseResults(simple, null).score).toBeCloseTo(200, 9);
    expect(parseResults(simple, "").score).toBeCloseTo(200, 9);
    expect(parseResults(simple, "not a number").score).toBeCloseTo(200, 9);
  });

  test("failed workloads are reported, not scored", () => {
    const parsed = parseResults({ ...simple, "c-bad": "FAILED" });
    expect(parsed.failed).toEqual(["c-bad"]);
    expect(parsed.tests).toEqual({ "a-wasm": 100, "b-js": 400 });
    expect(parsed.score).toBeNull();
  });

  test("empty results have no score", () => {
    expect(parseResults({}, null)).toEqual({ score: null, tests: {}, failed: [] });
  });
});

describe("failure never throws", () => {
  test("not a checkout", async () => {
    const result = await jetstream("ws://127.0.0.1:1/devtools/browser/x", tmpdir(), { timeout_ms: 5000 });
    expect(result.score).toBeNull();
    expect(result.error).toContain("no index.html");
  });

  test("no engine behind the endpoint", async () => {
    const dir = await mkdtemp("jetstream-test-");
    await Bun.write(join(dir, "index.html"), "");
    const result = await jetstream("ws://127.0.0.1:1/devtools/browser/x", dir, { timeout_ms: 5000 });
    await rmrf(dir);
    expect(result.score).toBeNull();
    expect(result.error).toBeTruthy();
    expect(result.ms).toBeLessThan(20_000);
  });
});

test.skipIf(!process.env.JETSTREAM_WS)(
  "real run on the engine behind JETSTREAM_WS",
  async () => {
    const dir = process.env.JETSTREAM_DIR ?? "";
    const tests = (process.env.JETSTREAM_TESTS ?? "").split(",").filter(Boolean);
    const result = await jetstream(process.env.JETSTREAM_WS!, dir, { tests, timeout_ms: 20 * 60_000 });
    expect(result.error).toBeUndefined();
    expect(result.score).toBeGreaterThan(0);
    expect(Object.keys(result.tests).length).toBeGreaterThan(0);
  },
  30 * 60_000,
);
