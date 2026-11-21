import { describe, expect, test } from "bun:test";
import { loadEdgeConfig, routeRows } from "../../../tools/edge-config.ts";
import { applyHints, withEdgeCache } from "../src/hints.ts";
import { matchRoute } from "../src/router.ts";

const get = (url = "https://docs.example/a") => new Request(url);
const ok = (headers: HeadersInit = {}) => new Response("body", { headers });

/** A Cache API stand-in: keyed by URL, stores the response it is given. */
function memoryCache() {
  const map = new Map<string, Response>();
  const store = {
    match: async (req: Request) => map.get(req.url)?.clone(),
    put: async (req: Request, res: Response) => void map.set(req.url, res),
  } as unknown as Cache;
  return { store, map };
}

describe("applyHints", () => {
  test("adds s-maxage and stale-while-revalidate only when the service set no cache-control", () => {
    const res = applyHints(get(), ok(), { cache: { ttl: 300, swr: 3600 } });
    expect(res.headers.get("cache-control")).toBe("public, max-age=0, s-maxage=300, stale-while-revalidate=3600");
    expect(applyHints(get(), ok({ "cache-control": "no-store" }), { cache: { ttl: 300 } }).headers.get("cache-control")).toBe("no-store");
  });

  test("never caches a non-GET, an error, a cookie or an authorized request", () => {
    const hints = { cache: { ttl: 60 } };
    expect(applyHints(new Request("https://x/", { method: "POST" }), ok(), hints).headers.has("cache-control")).toBe(false);
    expect(applyHints(get(), new Response("x", { status: 500 }), hints).headers.has("cache-control")).toBe(false);
    expect(applyHints(get(), ok({ "set-cookie": "a=b" }), hints).headers.has("cache-control")).toBe(false);
    expect(applyHints(new Request("https://x/", { headers: { authorization: "Bearer t" } }), ok(), hints).headers.has("cache-control")).toBe(false);
  });

  test("early hints become Link headers once", () => {
    const link = "</brand/icon.svg>; rel=preload; as=image";
    const first = applyHints(get(), ok(), { early_hints: [link] });
    expect(first.headers.get("link")).toBe(link);
    expect(applyHints(get(), first, { early_hints: [link] }).headers.get("link")).toBe(link);
  });

  test("no hints returns the same response", () => {
    const res = ok();
    expect(applyHints(get(), res, undefined)).toBe(res);
  });
});

describe("withEdgeCache", () => {
  test("second GET is served from the cache, the producer runs once", async () => {
    const { store, map } = memoryCache();
    const waits: Promise<unknown>[] = [];
    const ctx = { waitUntil: (p: Promise<unknown>) => void waits.push(p) };
    let calls = 0;
    const produce = async () => (calls++, ok());
    const hints = { cache: { ttl: 60 } };
    const a = await withEdgeCache(get(), hints, ctx, produce, store);
    expect(a.headers.get("cache-control")).toContain("s-maxage=60");
    await Promise.all(waits);
    expect(map.size).toBe(1);
    const b = await withEdgeCache(get(), hints, ctx, produce, store);
    expect(await b.text()).toBe("body");
    expect(calls).toBe(1);
  });

  test("range requests, other methods and routes without a cache hint bypass the cache", async () => {
    const { store, map } = memoryCache();
    const ctx = { waitUntil: () => {} };
    await withEdgeCache(new Request("https://x/", { headers: { range: "bytes=0-1" } }), { cache: { ttl: 60 } }, ctx, async () => ok(), store);
    await withEdgeCache(get(), undefined, ctx, async () => ok(), store);
    expect(map.size).toBe(0);
  });
});

describe("rendered table", () => {
  const cfg = loadEdgeConfig();
  const rows = routeRows(cfg);

  test("hints of edge.toml reach the route the router resolves", () => {
    expect(matchRoute(rows, `docs.${cfg.domains.root}`, "/guide")?.hints?.cache).toEqual({ ttl: 300, swr: 3600 });
    expect(matchRoute(rows, `cdn.${cfg.domains.shenron}`, "/a.png")?.hints?.cache?.ttl).toBe(3600);
    expect(matchRoute(rows, cfg.domains.root!, "/download")?.hints?.early_hints).toEqual(["</brand/icon.svg>; rel=preload; as=image"]);
    expect(matchRoute(rows, cfg.domains.root!, "/github")?.hints).toBeUndefined();
  });
});
