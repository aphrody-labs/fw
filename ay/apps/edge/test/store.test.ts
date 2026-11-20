import { beforeEach, describe, expect, test } from "bun:test";
import { loadEdgeConfig, renderWrangler } from "../../../tools/edge-config.ts";
import { dispatch } from "../src/index.ts";
import { guard, isAllowed, withStores } from "../src/store.ts";
import type { EdgeEnv } from "../src/types.ts";
import { bucket } from "./r2-fake.ts";

const cfg = loadEdgeConfig();
const app = cfg.domains.app!;
const root = cfg.domains.root!;
const vars = renderWrangler(cfg).vars as Record<string, unknown>;

const INVOICE = "auto/auto/assets/022d3795-d406-4439-aaac-383635a7b634/artifacts/787ab51f/facturx-9c557253b0ee.pdf";
const PUBLIC = ["brand/icon.svg", "v1/icon/" + "a".repeat(64) + ".svg", "craft/app/index.js", "index.html", "manifest.json", "sha256/abcd"];
const PRIVATE = [INVOICE, "learn/bundle/latest.json", "learn/bundle/manifest.json", "auto/anything", "Auto/x", "secret.txt", "brandx/icon.svg"];

let assets: ReturnType<typeof bucket>;
let env: EdgeEnv;

beforeEach(() => {
  assets = bucket();
  for (const k of [...PUBLIC, ...PRIVATE]) assets.put(k, `body:${k}`, "application/octet-stream");
  env = { ...vars, ASSETS: assets, RELEASES: bucket(), VIDEO: bucket(), CHROMIUM: bucket(), DIST: bucket(), BUN_LIB: bucket() } as unknown as EdgeEnv;
});

const ctx = { waitUntil() {}, passThroughOnException() {} } as unknown as ExecutionContext;
const get = (host: string, path: string, init: RequestInit = {}) => dispatch(new Request(`https://${host}${path}`, init), env, ctx);

describe("allow-list", () => {
  test("a prefix ends in a slash, anything else is one exact key", () => {
    expect(isAllowed(["brand/", "index.html"], "brand/icon.svg")).toBe(true);
    expect(isAllowed(["brand/", "index.html"], "brandx/icon.svg")).toBe(false);
    expect(isAllowed(["brand/", "index.html"], "index.html")).toBe(true);
    expect(isAllowed(["brand/", "index.html"], "index.html.bak")).toBe(false);
    expect(isAllowed(undefined, "anything")).toBe(true);
    expect(isAllowed([], "anything")).toBe(false);
  });

  test("the guard hides keys outside the list and moves the rest under the prefix", async () => {
    const raw = bucket();
    raw.put("assets/a/1", "one");
    raw.put("assets/b/2", "two");
    const view = guard(raw, { binding: "X", prefix: "assets/", allow: ["a/"] });
    expect(await (await view.get("a/1"))!.text()).toBe("one");
    expect(await view.get("b/2")).toBeNull();
    expect(await view.head("b/2")).toBeNull();
    expect(await view.get("assets/a/1")).toBeNull();
  });

  test("withStores is rebuilt once per env and leaves the other bindings alone", () => {
    expect(withStores(env)).toBe(withStores(env));
    expect(withStores(env).EDGE_ROUTES).toBe(env.EDGE_ROUTES);
  });
});

describe("private keys of the assets bucket", () => {
  for (const key of PRIVATE) {
    test(`${key.split("/").slice(0, 2).join("/")} is 404 on every path that reaches the bucket`, async () => {
      const paths: Array<[string, string]> = [
        [`assets.${root}`, `/r2-assets/${key}`],
        [root, `/r2-assets/${key}`],
        [`assets.${app}`, `/${key}`],
        [`assets.${root}`, `/v1/icon/${key}`],
        [root, `/brand/../${key}`],
      ];
      for (const [host, path] of paths) {
        for (const method of ["GET", "HEAD"]) {
          const res = await get(host, path, { method });
          // the gateway refuses a key that is not content-addressed with 400 before it reads the bucket; everything else is a plain 404
          expect([host + path, method, res.status === 400 && path.startsWith("/v1/") ? 404 : res.status]).toEqual([host + path, method, 404]);
          expect(await res.text()).not.toContain("body:");
        }
      }
      const ranged = await get(`assets.${app}`, `/${key}`, { headers: { range: "bytes=0-3" } });
      expect(ranged.status).toBe(404);
      const preview = await get("ay-edge.example.workers.dev", `/_/bucket/${key}`);
      expect(preview.status).toBe(404);
    });
  }

  test("public keys keep serving, with their bytes, validators and ranges", async () => {
    for (const key of PUBLIC) {
      const res = await get(`assets.${app}`, `/${key}`);
      expect([key, res.status]).toEqual([key, 200]);
      expect(await res.text()).toBe(`body:${key}`);
      expect(res.headers.get("etag")).toBeTruthy();
    }
    const plain = await get(root, "/r2-assets/brand/icon.svg");
    expect(plain.status).toBe(200);
    const part = await get(`assets.${app}`, "/brand/icon.svg", { headers: { range: "bytes=0-3" } });
    expect([part.status, await part.text()]).toEqual([206, "body"]);
    expect((await get(`assets.${app}`, "/brand/icon.svg", { method: "HEAD" })).status).toBe(200);
    expect((await get(`assets.${app}`, "/")).status).toBe(200);
  });

  test("the releases brand path reads the same guarded store", async () => {
    expect((await get(`releases.${root}`, "/brand/icon.svg")).status).toBe(200);
    expect((await get(`releases.${root}`, `/brand/${INVOICE}`)).status).toBe(404);
  });
});
