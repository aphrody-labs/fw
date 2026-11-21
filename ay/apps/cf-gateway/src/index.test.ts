import { describe, expect, test } from "bun:test";
import { app, type GatewayEnv } from "./index.ts";

function fakeEnv(store: Record<string, Uint8Array>): GatewayEnv {
  const bucketFor = (prefix: string): R2Bucket =>
    ({
      async head(key: string) {
        const data = store[`${prefix}:${key}`];
        if (!data) return null;
        return {
          key,
          size: data.byteLength,
          httpEtag: `"etag-${key}"`,
          writeHttpMetadata(h: Headers) {
            h.set("Content-Type", "application/octet-stream");
          },
        } as unknown as R2Object;
      },
      async get(key: string, opts?: R2GetOptions) {
        const data = store[`${prefix}:${key}`];
        if (!data) return null;
        const etag = `"etag-${key}"`;
        if (opts?.onlyIf && "etagDoesNotMatch" in opts.onlyIf && opts.onlyIf.etagDoesNotMatch === etag) {
          return {
            key,
            size: data.byteLength,
            httpEtag: etag,
            writeHttpMetadata() {},
          } as unknown as R2Object;
        }
        const range = opts?.range as { offset?: number; length?: number } | undefined;
        if (range && typeof range.offset === "number") {
          const start = range.offset;
          const length = Math.min(range.length ?? data.byteLength - start, data.byteLength - start);
          const slice = data.subarray(start, start + length);
          return {
            key,
            size: data.byteLength,
            httpEtag: etag,
            range: { offset: start, length },
            body: new ReadableStream({
              start(ctrl) {
                ctrl.enqueue(slice);
                ctrl.close();
              },
            }),
            writeHttpMetadata(h: Headers) {
              h.set("Content-Type", "model/gltf-binary");
            },
          } as unknown as R2ObjectBody;
        }
        return {
          key,
          size: data.byteLength,
          httpEtag: etag,
          body: new ReadableStream({
            start(ctrl) {
              ctrl.enqueue(data);
              ctrl.close();
            },
          }),
          writeHttpMetadata(h: Headers) {
            h.set("Content-Type", "model/gltf-binary");
          },
        } as unknown as R2ObjectBody;
      },
    }) as unknown as R2Bucket;

  return { ASSETS: bucketFor("ASSETS"), VIDEO: bucketFor("VIDEO") };
}

describe("ay-cf-gateway Hono Worker", () => {
  const hash = "a".repeat(64);
  const sampleGlb = new Uint8Array([0x67, 0x6c, 0x54, 0x46, 2, 0, 0, 0, 20, 0, 0, 0]);
  const env = fakeEnv({
    [`ASSETS:v1/glb/${hash}.glb`]: sampleGlb,
  });

  test("serves /healthz and /.well-known/agent-skills/index.json", async () => {
    const h = await app.request("/healthz", {}, env);
    expect(h.status).toBe(200);
    expect((await h.json()) as { ok: boolean }).toMatchObject({ ok: true, service: "ay-cf-gateway" });

    const s = await app.request("/.well-known/agent-skills/index.json", {}, env);
    expect(s.status).toBe(200);
    const body = (await s.json()) as { skills: unknown[] };
    expect(body.skills.length).toBeGreaterThanOrEqual(20);
  });

  test("serves immutable full and 206 byte-range R2 objects with CORS", async () => {
    const res = await app.request(`/v1/glb/${hash}.glb`, {}, env);
    expect(res.status).toBe(200);
    expect(res.headers.get("Cache-Control")).toContain("immutable");
    expect(res.headers.get("Accept-Ranges")).toBe("bytes");

    const rangeRes = await app.request(
      `/v1/glb/${hash}.glb`,
      { headers: { Range: "bytes=0-3" } },
      env,
    );
    expect(rangeRes.status).toBe(206);
    expect(rangeRes.headers.get("Content-Range")).toBe(`bytes 0-3/${sampleGlb.byteLength}`);
    const bytes = new Uint8Array(await rangeRes.arrayBuffer());
    expect(Array.from(bytes)).toEqual([0x67, 0x6c, 0x54, 0x46]);

    const notMod = await app.request(
      `/v1/glb/${hash}.glb`,
      { headers: { "If-None-Match": `"etag-v1/glb/${hash}.glb"` } },
      env,
    );
    expect(notMod.status).toBe(304);
  });

  test("rejects malformed non-BLAKE3 paths", async () => {
    const bad = await app.request("/v1/glb/not-a-hash.glb", {}, env);
    expect(bad.status).toBe(400);
  });
});
