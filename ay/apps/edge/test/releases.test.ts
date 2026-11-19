import { beforeEach, describe, expect, test } from "bun:test";
import { type ReleaseManifest, releaseKeys, sha256sums, validateRelease } from "../src/manifest.ts";
import releases from "../src/services/releases.ts";
import type { EdgeEnv } from "../src/types.ts";
import { bucket } from "./r2-fake.ts";

const sha = (c: string) => c.repeat(64);
const manifest: ReleaseManifest = {
  schema: "ay.release.v1",
  product: "ay",
  version: "1.2.3",
  channel: "stable",
  tag: "v1.2.3",
  commit: "abc1234",
  date: "2026-11-19T00:00:00Z",
  assets: [
    { name: "ay-x86_64-linux", os: "linux", arch: "x86_64", abi: "musl", kind: "binary", size: 11, sha256: sha("a") },
    { name: "ay-x86_64-windows.exe", os: "windows", arch: "x86_64", kind: "binary", size: 12, sha256: sha("b") },
    { name: "corelib.wasm", os: "any", arch: "wasm32", kind: "wasm", size: 13, sha256: sha("c") },
  ],
};

let env: EdgeEnv;
let rel: ReturnType<typeof bucket>;

beforeEach(() => {
  rel = bucket();
  rel.put(releaseKeys.release("ay", "1.2.3"), JSON.stringify(manifest), "application/json");
  rel.put(releaseKeys.channel("ay", "stable"), JSON.stringify({ schema: "ay.release.v1", product: "ay", channel: "stable", version: "1.2.3", tag: "v1.2.3", date: manifest.date }));
  rel.put(releaseKeys.index(), JSON.stringify({ schema: "ay.releases.v1", generated: manifest.date, products: { ay: { stable: { product: "ay", channel: "stable", version: "1.2.3" } } } }));
  rel.put(releaseKeys.asset("ay", "1.2.3", "ay-x86_64-linux"), "linux-bytes");
  rel.put(releaseKeys.sums("ay", "1.2.3"), sha256sums(manifest));
  rel.put("scripts/install.sh", "#!/bin/sh\n");
  rel.put("channels/canary/latest.json", JSON.stringify({ version: "9", channel: "canary" }));
  const assets = bucket();
  assets.put("brand/icon.svg", "<svg/>", "image/svg+xml");
  const chromium = bucket();
  chromium.put("ay-tools/latest.json", JSON.stringify({ version: "t1" }), "application/json");
  env = { RELEASES: rel, ASSETS: assets, VIDEO: bucket(), CHROMIUM: chromium } as EdgeEnv;
});

const call = (path: string, init: RequestInit = {}) => {
  const url = new URL(`https://releases.example${path}`);
  return releases({ request: new Request(url, init), url, path: url.pathname, origin: url.origin, env, ctx: {} as ExecutionContext });
};

describe("releases service", () => {
  test("the fixture manifest is a valid release", () => expect(validateRelease(manifest)).toEqual([]));

  test("/manifest.json serves the index with revalidation", async () => {
    const res = await call("/manifest.json");
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("no-cache");
    expect((await res.json()).products.ay.stable.version).toBe("1.2.3");
  });

  test("latest manifest of a channel carries absolute asset urls", async () => {
    const body = await (await call("/api/releases/ay/stable/latest")).json();
    expect(body.version).toBe("1.2.3");
    expect(body.assets[0].url).toBe("https://releases.example/ay/1.2.3/ay-x86_64-linux");
  });

  test("/<product>/latest/<os>-<arch> redirects to the versioned asset", async () => {
    const res = await call("/ay/latest/windows-x86_64");
    expect(res.status).toBe(302);
    expect(res.headers.get("location")).toBe("https://releases.example/ay/1.2.3/ay-x86_64-windows.exe");
    expect(res.headers.get("x-release-version")).toBe("1.2.3");
  });

  test("a portable asset answers for any os; abi and kind narrow the pick", async () => {
    expect((await call("/ay/latest/browser-wasm32?kind=wasm")).headers.get("location")).toBe("https://releases.example/ay/1.2.3/corelib.wasm");
    expect((await call("/ay/latest/linux-x86_64?abi=musl")).status).toBe(302);
    expect((await call("/ay/latest/linux-x86_64?abi=gnu")).status).toBe(404);
    expect((await call("/ay/latest/macos-aarch64")).status).toBe(404);
  });

  test("a versioned asset is served immutable with its checksum list beside it", async () => {
    const res = await call("/ay/1.2.3/ay-x86_64-linux");
    expect(res.headers.get("cache-control")).toContain("immutable");
    expect(res.headers.get("content-disposition")).toContain("ay-x86_64-linux");
    expect(await res.text()).toBe("linux-bytes");
    expect(await (await call("/ay/1.2.3/SHA256SUMS")).text()).toContain(`${sha("a")}  ay-x86_64-linux`);
  });

  test("ranges answer 206 with Content-Range, HEAD has no body, a repeat ETag answers 304", async () => {
    const part = await call("/ay/1.2.3/ay-x86_64-linux", { headers: { range: "bytes=0-4" } });
    expect(part.status).toBe(206);
    expect(part.headers.get("content-range")).toBe("bytes 0-4/11");
    expect(await part.text()).toBe("linux");
    const head = await call("/ay/1.2.3/ay-x86_64-linux", { method: "HEAD" });
    expect(head.status).toBe(200);
    expect(head.headers.get("content-length")).toBe("11");
    const etag = (await call("/ay/1.2.3/ay-x86_64-linux")).headers.get("etag")!;
    expect((await call("/ay/1.2.3/ay-x86_64-linux", { headers: { "if-none-match": etag } })).status).toBe(304);
  });

  test("traversal and unknown objects are 404, writes are 405", async () => {
    expect((await call("/ay/1.2.3/nope")).status).toBe(404);
    expect((await call("/assets/../x")).status).toBe(404);
    expect((await call("/ay/1.2.3/ay-x86_64-linux", { method: "POST" })).status).toBe(405);
  });

  test("the portal lists the release and marks the visitor's platform", async () => {
    const res = await call("/", { headers: { "user-agent": "Mozilla/5.0 (X11; Linux x86_64)" } });
    const html = await res.text();
    expect(res.headers.get("content-type")).toContain("text/html");
    expect(html).toContain("ay-x86_64-linux");
    expect(html).toContain('class="mine"');
    expect(html).toContain("/brand/icon.svg");
  });

  test("legacy aphrody paths still read the old layout", async () => {
    expect((await call("/install.sh")).status).toBe(200);
    expect((await (await call("/api/releases/canary/latest")).json()).version).toBe("9");
    expect((await call("/api/channels")).status).toBe(200);
  });

  test("brand files come from the assets bucket and ay-tools from the chromium bucket", async () => {
    expect(await (await call("/brand/icon.svg")).text()).toBe("<svg/>");
    expect((await (await call("/ay-tools/latest.json")).json()).version).toBe("t1");
  });
});
