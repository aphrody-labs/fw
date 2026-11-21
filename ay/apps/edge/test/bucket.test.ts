import { beforeEach, describe, expect, test } from "bun:test";
import { loadEdgeConfig, bucketVars } from "../../../tools/edge-config.ts";
import bucketService, { keyOfPath } from "../src/services/bucket.ts";
import type { EdgeEnv } from "../src/types.ts";
import { bucket } from "./r2-fake.ts";

const cfg = loadEdgeConfig();
const app = cfg.domains.app!;
const root = cfg.domains.root!;
const vars = bucketVars(cfg);

let env: EdgeEnv;
let video: ReturnType<typeof bucket>;
let dist: ReturnType<typeof bucket>;

beforeEach(() => {
  video = bucket();
  video.put("index.html", "<h1>video</h1>", "text/html; charset=utf-8");
  video.put("showcase/clip.mp4", "0123456789abcdef", "video/mp4");
  video.put("showcase/index.html", "<h1>show</h1>", "text/html");
  dist = bucket();
  dist.put("aphrody/SHA256SUMS.txt", "abc  file\n", "text/plain; charset=utf-8", "public, max-age=60");
  env = { ...vars, VIDEO: video, DIST: dist, ASSETS: bucket(), BUN_LIB: bucket() } as unknown as EdgeEnv;
});

const call = (host: string, path: string, init: RequestInit = {}) => {
  const url = new URL(`https://${host}${path}`);
  return bucketService({ request: new Request(url, init), url, path: url.pathname, origin: url.origin, env, ctx: {} as ExecutionContext });
};

describe("bucket service", () => {
  test("the host selects the bucket and / answers index.html", async () => {
    expect(await (await call(`video.${app}`, "/")).text()).toBe("<h1>video</h1>");
    expect(await (await call(`dist.${app}`, "/aphrody/SHA256SUMS.txt")).text()).toBe("abc  file\n");
    expect((await call(`dist.${app}`, "/index.html")).status).toBe(404);
    expect(await (await call(`video.${app}`, "/showcase/")).text()).toBe("<h1>show</h1>");
  });

  test("an unknown host, a missing key and traversal are 404", async () => {
    expect((await call("nope.example", "/")).status).toBe(404);
    expect((await call(`video.${app}`, "/missing")).status).toBe(404);
    expect(keyOfPath("/a/../b")).toBeUndefined();
    expect(keyOfPath("/a%00b")).toBeUndefined();
    expect(keyOfPath("/%E0%A4%A")).toBeUndefined();
  });

  test("ranges answer 206 with Content-Range, HEAD has a length and no body", async () => {
    const part = await call(`video.${app}`, "/showcase/clip.mp4", { headers: { range: "bytes=2-5" } });
    expect(part.status).toBe(206);
    expect(part.headers.get("content-range")).toBe("bytes 2-5/16");
    expect(await part.text()).toBe("2345");
    const head = await call(`video.${app}`, "/showcase/clip.mp4", { method: "HEAD" });
    expect([head.status, head.headers.get("content-length"), head.headers.get("accept-ranges")]).toEqual([200, "16", "bytes"]);
  });

  test("validators: a repeated ETag is 304", async () => {
    const etag = (await call(`video.${app}`, "/showcase/clip.mp4")).headers.get("etag")!;
    expect((await call(`video.${app}`, "/showcase/clip.mp4", { headers: { "if-none-match": etag } })).status).toBe(304);
  });

  test("the object's own Cache-Control wins, with none stored the response carries none", async () => {
    expect((await call(`dist.${app}`, "/aphrody/SHA256SUMS.txt")).headers.get("cache-control")).toBe("public, max-age=60");
    expect((await call(`video.${app}`, "/")).headers.get("cache-control")).toBeNull();
  });

  test("CORS follows the rule of the bucket: allowed origins are echoed with Vary, others get a plain answer", async () => {
    const ok = await call(`video.${app}`, "/", { headers: { origin: `https://${root}` } });
    expect(ok.headers.get("access-control-allow-origin")).toBe(`https://${root}`);
    expect(ok.headers.get("access-control-expose-headers")).toContain("Content-Range");
    expect(ok.headers.get("vary")).toBe("Origin");
    const other = await call(`video.${app}`, "/", { headers: { origin: "https://evil.example" } });
    expect(other.headers.get("access-control-allow-origin")).toBeNull();
    expect(other.headers.get("vary")).toBe("Origin");
    expect((await call(`bun-lib.${app}`, "/", { headers: { origin: `https://${root}` } })).headers.get("access-control-allow-origin")).toBeNull();
  });

  test("preflight: allowed origin, method and headers answer 200, anything else 403", async () => {
    const ask = (origin: string, method: string, headers: string) =>
      call(`video.${app}`, "/", { method: "OPTIONS", headers: { origin, "access-control-request-method": method, "access-control-request-headers": headers } });
    const ok = await ask(`https://${root}`, "GET", "range");
    expect(ok.status).toBe(200);
    expect(ok.headers.get("access-control-allow-methods")).toBe("GET,HEAD");
    expect(ok.headers.get("access-control-allow-headers")).toBe("range");
    expect(ok.headers.get("access-control-max-age")).toBe("86400");
    expect((await ask(`https://${root}`, "PUT", "range")).status).toBe(403);
    expect((await ask(`https://${root}`, "GET", "x-secret")).status).toBe(403);
    expect((await ask("https://evil.example", "GET", "range")).status).toBe(403);
  });

  test("writes are refused", async () => {
    expect((await call(`video.${app}`, "/", { method: "POST" })).status).toBe(405);
  });
});
