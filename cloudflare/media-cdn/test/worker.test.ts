import { beforeEach, describe, expect, test } from "bun:test";
import worker from "../worker.ts";
import { resetStore } from "../src/store.ts";
import { context, fakeEnv } from "./env.ts";

beforeEach(resetStore);

async function get(env: Env, path: string, init?: RequestInit, ctx = context()) {
  const response = await worker.fetch(new Request(`https://media.aphrody.com${path}`, init), env, ctx.ctx);
  await ctx.settle();
  return response;
}

describe("index and manifest", () => {
  test("the index lists the routes and counts; the manifest is served as stored", async () => {
    const { env } = fakeEnv();
    const index = (await (await get(env, "/v1/")).json()) as { revision: string; images: number; video: number; sprites: number; routes: { fonts: string } };
    expect([index.revision, index.images, index.video, index.sprites]).toEqual(["0badc0de", 2, 1, 1]);
    expect(index.routes.fonts).toBe("https://material.aphrody.com/v1/");
    const manifest = await get(env, "/v1/manifest.json");
    expect(manifest.headers.get("Content-Type")).toContain("application/json");
    expect(((await manifest.json()) as { revision: string }).revision).toBe("0badc0de");
  });

  test("an empty bucket answers 503", async () => {
    const { env, bucket } = fakeEnv();
    bucket.objects.delete("v1/manifest.json");
    expect((await get(env, "/v1/")).status).toBe(503);
  });

  test("methods other than GET, HEAD and OPTIONS are refused; CORS and timing headers are on every answer", async () => {
    const { env } = fakeEnv();
    expect((await get(env, "/v1/", { method: "POST" })).status).toBe(405);
    const options = await get(env, "/v1/", { method: "OPTIONS" });
    expect(options.status).toBe(204);
    const ok = await get(env, "/v1/");
    expect(ok.headers.get("Access-Control-Allow-Origin")).toBe("*");
    expect(ok.headers.get("Timing-Allow-Origin")).toBe("*");
    expect(ok.headers.get("Cross-Origin-Resource-Policy")).toBe("cross-origin");
  });
});

describe("images", () => {
  test("a pinned name with a format is immutable and says its canonical location", async () => {
    const { env } = fakeEnv();
    const res = await get(env, "/v1/img/test-photo.aaaa1111.avif=w600");
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("test-photo w600 avif");
    expect(res.headers.get("Content-Type")).toBe("image/avif");
    expect(res.headers.get("Cache-Control")).toBe("public, max-age=31536000, immutable");
    expect(res.headers.get("Content-Location")).toBe("/v1/img/test-photo.aaaa1111.avif=w600");
    expect(res.headers.get("Vary")).toBeNull();
    expect(res.headers.get("X-Media-Source")).toBe("r2");
  });

  test("without a format the Accept header chooses and the answer varies on it", async () => {
    const { env } = fakeEnv();
    const avif = await get(env, "/v1/img/test-photo=w600", { headers: { Accept: "image/avif,image/webp,*/*" } });
    expect(avif.headers.get("Content-Type")).toBe("image/avif");
    expect(avif.headers.get("Vary")).toBe("Accept");
    expect(avif.headers.get("Cache-Control")).toContain("s-maxage=3600");
    expect((await get(env, "/v1/img/test-photo=w600", { headers: { Accept: "image/webp,*/*" } })).headers.get("Content-Type")).toBe("image/webp");
    const fallback = await get(env, "/v1/img/test-photo=w600", { headers: { Accept: "*/*" } });
    expect(fallback.headers.get("Content-Type")).toBe("image/jpeg");
    expect(await fallback.text()).toBe("test-photo w600 jpg");
    const alpha = await get(env, "/v1/img/test-icon=s48", { headers: { Accept: "*/*" } });
    expect(alpha.headers.get("Content-Type")).toBe("image/png");
  });

  test("no parameter means the native width; a width above the source is clamped, never upscaled", async () => {
    const { env } = fakeEnv();
    expect(await (await get(env, "/v1/img/test-photo.webp")).text()).toBe("test-photo w4096 webp");
    const clamped = await get(env, "/v1/img/test-photo.webp=w3840");
    expect(clamped.headers.get("Content-Location")).toBe("/v1/img/test-photo.aaaa1111.webp=w3840");
    const above = await get(env, "/v1/img/test-icon.avif=w1024");
    expect(await above.text()).toBe("test-icon w512 avif");
    expect(above.headers.get("Content-Location")).toBe("/v1/img/test-icon.bbbb2222.avif=w512");
  });

  test("ETag revalidation answers 304", async () => {
    const { env } = fakeEnv();
    const first = await get(env, "/v1/img/test-photo.avif=w320");
    const again = await get(env, "/v1/img/test-photo.avif=w320", { headers: { "If-None-Match": first.headers.get("ETag")! } });
    expect(again.status).toBe(304);
  });

  test("a size outside the ladder is transformed once, kept under its canonical hash and then read from R2", async () => {
    const { env, bucket, images } = fakeEnv();
    const first = await get(env, "/v1/img/test-photo.avif=w777");
    expect(first.status).toBe(200);
    expect(first.headers.get("X-Media-Source")).toBe("built");
    expect(images.calls).toHaveLength(1);
    expect(images.calls[0]!.transform).toEqual({ width: 777, fit: "scale-down" });
    expect(images.calls[0]!.output).toEqual({ format: "image/avif", quality: 60 });
    const generated = [...bucket.objects.keys()].filter((k) => k.startsWith("v1/gen/"));
    expect(generated).toHaveLength(1);
    expect(generated[0]).toMatch(/^v1\/gen\/[0-9a-f]{20}\.avif$/);
    const second = await get(env, "/v1/img/test-photo.avif=q70-w777");
    expect(second.status).toBe(200);
    expect(images.calls).toHaveLength(2);
    const third = await get(env, "/v1/img/test-photo.avif=w777");
    expect(third.headers.get("X-Media-Source")).toBe("r2");
    expect(images.calls).toHaveLength(2);
  });

  test("width and height crop, s fits a square of its long edge", async () => {
    const { env, images } = fakeEnv();
    await get(env, "/v1/img/test-photo.webp=w500-h300");
    await get(env, "/v1/img/test-photo.webp=s100");
    expect(images.calls[0]!.transform).toEqual({ width: 500, height: 300, fit: "cover" });
    expect(images.calls[1]!.transform).toEqual({ width: 100, height: 100, fit: "scale-down" });
  });

  test("without the Images binding an off-ladder size is a 501, a ladder size still works", async () => {
    const { env } = fakeEnv(false);
    expect((await get(env, "/v1/img/test-photo.avif=w777")).status).toBe(501);
    expect((await get(env, "/v1/img/test-photo.avif=w600")).status).toBe(200);
  });

  test("an unknown name is 404 and a bad token is 400", async () => {
    const { env } = fakeEnv();
    expect((await get(env, "/v1/img/nothing.avif")).status).toBe(404);
    expect((await get(env, "/v1/img/test-photo.avif=w10")).status).toBe(400);
  });

  test("a stale pin resolves through the entry stored with its hash", async () => {
    const { env, bucket } = fakeEnv();
    await bucket.put("v1/img/test-photo.99999999/entry.json", JSON.stringify({ sha: "99999999", source: "png", width: 100, height: 50, alpha: false, fallback: "jpg", widths: [100], squares: [], bytes: {} }));
    await bucket.put("v1/img/test-photo.99999999/w100.webp", "old");
    const res = await get(env, "/v1/img/test-photo.99999999.webp=w100");
    expect(await res.text()).toBe("old");
    expect((await get(env, "/v1/img/test-photo.88888888.webp=w100")).status).toBe(404);
  });
});

describe("video", () => {
  test("a rung supports range requests and a pinned one is immutable", async () => {
    const { env } = fakeEnv();
    const res = await get(env, "/v1/video/test-wide.cccc3333/1920w-2x-av1.mp4", { headers: { Range: "bytes=4-9" } });
    expect(res.status).toBe(206);
    expect(res.headers.get("Content-Range")).toBe("bytes 4-9/16");
    expect(await res.text()).toBe("456789");
    expect(res.headers.get("Content-Type")).toBe("video/mp4");
    expect(res.headers.get("Cache-Control")).toContain("immutable");
    const logical = await get(env, "/v1/video/test-wide/1920w-2x-av1.mp4");
    expect(logical.status).toBe(200);
    expect(logical.headers.get("Cache-Control")).toContain("s-maxage=3600");
    expect(logical.headers.get("Content-Length")).toBe("16");
  });

  test("a missing rung is 404, an invalid one 400", async () => {
    const { env } = fakeEnv();
    expect((await get(env, "/v1/video/test-wide/600w-1x-h264.mp4")).status).toBe(404);
    expect((await get(env, "/v1/video/test-wide/800w-1x-h264.mp4")).status).toBe(404);
    expect((await get(env, "/v1/video/test-wide/600w-1x-av1.webm")).status).toBe(400);
  });

  test("the JSON form lists the sources with absolute URLs", async () => {
    const { env } = fakeEnv();
    const body = (await (await get(env, "/v1/video/test-wide.json")).json()) as { sha: string; poster: string; sources: { src: string; type: string; media?: string }[] };
    expect(body.sha).toBe("cccc3333");
    expect(body.poster).toBe("test-wide-poster");
    expect(body.sources[0]!.src).toBe("https://media.aphrody.com/v1/video/test-wide.cccc3333/600w-2x-av1.mp4");
    expect(body.sources).toHaveLength(12);
  });
});

describe("sprites", () => {
  test("pinned and logical names serve the same object", async () => {
    const { env } = fakeEnv();
    const pinned = await get(env, "/v1/sprite/ui.dddd4444.svg");
    expect(pinned.headers.get("Content-Type")).toBe("image/svg+xml");
    expect(pinned.headers.get("Cache-Control")).toContain("immutable");
    expect(pinned.headers.get("Content-Security-Policy")).toContain("sandbox");
    expect(await (await get(env, "/v1/sprite/ui.svg")).text()).toContain('<symbol id="home"');
    expect((await get(env, "/v1/sprite/none.svg")).status).toBe(404);
  });
});
