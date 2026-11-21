import { describe, expect, test } from "bun:test";
import worker from "../worker.ts";
import { etagMatches, sha256Hex } from "../src/http.ts";

// A bucket holding a handful of objects, enough to exercise routing and cache headers without the built tree.
function bucket(objects: Record<string, string | Uint8Array>): Env {
  const get = async (key: string) => {
    const value = objects[key];
    if (value === undefined) return null;
    const bytes = typeof value === "string" ? new TextEncoder().encode(value) : value;
    return {
      size: bytes.length,
      body: new Response(bytes).body,
      httpEtag: '"etag"',
      uploaded: new Date(0),
      arrayBuffer: async () => bytes.slice().buffer,
      text: async () => new TextDecoder().decode(bytes),
      json: async () => JSON.parse(new TextDecoder().decode(bytes)),
    };
  };
  return { MATERIAL: { get } } as unknown as Env;
}
const ctx = { waitUntil() {}, passThroughOnException() {}, props: {} } as unknown as ExecutionContext;
const call = (env: Env, path: string, init?: RequestInit) => worker.fetch(new Request(`https://material.aphrody.com${path}`, init), env, ctx);

const env = bucket({
  "v1/assets/ui/1.2.0/sheet.png": "png",
  "v1/assets/ui/1.2.0/model.glb": "glb",
  "v1/fonts/google-sans-flex.css": "/* full */",
  "v1/fonts/google-sans-flex/latin.0123abcd.woff2": "woff",
  "v1/src/roboto-flex.0123abcd.ttf": "ttf",
  "v1/index.json": "{}",
  "v1/icons/outlined/home.svg": '<svg><path d="M0"/></svg>',
});

describe("static tree", () => {
  test("versioned assets and content-addressed files are immutable, with the right types", async () => {
    for (const [path, type] of [
      ["/v1/assets/ui/1.2.0/sheet.png", "image/png"],
      ["/v1/assets/ui/1.2.0/model.glb", "model/gltf-binary"],
      ["/v1/fonts/google-sans-flex/latin.0123abcd.woff2", "font/woff2"],
      ["/v1/src/roboto-flex.0123abcd.ttf", "font/ttf"],
    ] as const) {
      const res = await call(env, path);
      expect(res.status).toBe(200);
      expect(res.headers.get("Cache-Control")).toBe("public, max-age=31536000, immutable");
      expect(res.headers.get("Content-Type")).toBe(type);
    }
  });

  test("mutable files get a short TTL; the full font css stays static without parameters", async () => {
    const index = await call(env, "/v1/index.json");
    expect(index.headers.get("Cache-Control")).toContain("stale-while-revalidate");
    const css = await call(env, "/v1/fonts/google-sans-flex.css");
    expect(await css.text()).toBe("/* full */");
  });

  test("a stale ?v pin is a 404, a matching one immutable", async () => {
    const sum = await sha256Hex(new TextEncoder().encode('<svg><path d="M0"/></svg>'));
    expect((await call(env, `/v1/icons/outlined/home.svg?v=${sum.slice(0, 12)}`)).headers.get("Cache-Control")).toContain("immutable");
    expect((await call(env, "/v1/icons/outlined/home.svg?v=deadbeef")).status).toBe(404);
  });

  test("rejects traversal, other methods and unknown paths", async () => {
    expect((await call(env, "/v1/../secret")).status).toBe(404);
    expect((await call(env, "/v1/index.json", { method: "DELETE" })).status).toBe(405);
    expect((await call(env, "/v1/index.json", { method: "POST" })).status).toBe(405);
    expect((await call(env, "/v1/missing.json")).status).toBe(404);
    expect((await call(env, "/v1/icons.svg?icons=home", { method: "OPTIONS" })).status).toBe(204);
  });

  test("the root lists the à-la-carte outputs", async () => {
    const root = (await (await call(env, "/v1/")).json()) as { aLaCarte: Record<string, string>; families: string[] };
    expect(Object.keys(root.aLaCarte)).toContain("icons.woff2");
    expect(root.families).toContain("roboto-flex");
  });
});

describe("http helpers", () => {
  test("etag comparison", () => {
    expect(etagMatches('"a", W/"b"', '"b"')).toBe(true);
    expect(etagMatches("*", '"x"')).toBe(true);
    expect(etagMatches('"a"', '"b"')).toBe(false);
    expect(etagMatches(null, '"b"')).toBe(false);
  });
});
