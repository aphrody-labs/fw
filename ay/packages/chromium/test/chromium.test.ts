import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { inflateSync } from "node:zlib";
import { join } from "node:path";
import { cefVersion, evaluate, screenshot, shutdown } from "../src/index.ts";

const page = `<!doctype html><html><head><title>Ay Chromium Test</title></head>
<body style="margin:0;background:#ff0000"><h1 id="h">hello</h1>
<script>window.answer = 6 * 7; window.later = new Promise(r => setTimeout(() => r("done"), 50));</script></body></html>`;

let server: ReturnType<typeof Bun.serve>;
let url: string;
const dir = mkdtempSync(join(tmpdir(), "ay-chromium-test-"));

beforeAll(() => {
  server = Bun.serve({ port: 0, fetch: () => new Response(page, { headers: { "content-type": "text/html" } }) });
  url = `http://127.0.0.1:${server.port}/`;
});

afterAll(() => {
  server.stop(true);
  shutdown();
});

describe("@aphrody/chromium", () => {
  test("reports the CEF build", () => {
    expect(cefVersion()).toMatch(/^\d+\.\d+\.\d+\+g[0-9a-f]+\+chromium-\d+\./);
  });

  test("evaluate returns JSON from the page", async () => {
    expect(await evaluate(url, "document.title")).toBe("Ay Chromium Test");
    expect(await evaluate(url, "window.answer")).toBe(42);
    expect(await evaluate(url, "({a: [1, 2], b: document.getElementById('h').textContent})")).toEqual({ a: [1, 2], b: "hello" });
    expect(await evaluate(url, "window.later")).toBe("done");
  });

  test("evaluate rejects with the page's exception", async () => {
    await expect(evaluate(url, "nope.nope")).rejects.toThrow(/ReferenceError/);
  });

  test("screenshot writes a PNG of the requested size with the page's pixels", async () => {
    const out = join(dir, "shot.png");
    const result = await screenshot(url, out, { width: 640, height: 360 });
    expect(result).toEqual({ path: out, width: 640, height: 360 });
    const png = readFileSync(out);
    expect(png.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
    expect(png.readUInt32BE(16)).toBe(640);
    expect(png.readUInt32BE(20)).toBe(360);
    expect(statSync(out).size).toBeGreaterThan(200);
    // First pixel of the first scanline is its raw bytes under every PNG filter type (no left or upper neighbour).
    const idat: Buffer[] = [];
    for (let at = 8; at < png.length; ) {
      const len = png.readUInt32BE(at);
      if (png.toString("latin1", at + 4, at + 8) === "IDAT") idat.push(png.subarray(at + 8, at + 8 + len));
      at += 12 + len;
    }
    const scan = inflateSync(Buffer.concat(idat));
    expect([...scan.subarray(1, 5)]).toEqual([255, 0, 0, 255]);
  });
});
