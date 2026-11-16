import { describe, expect, test } from "bun:test";
import { Cache, cachedSource, defaultDbPath } from "../src/cache.ts";

const json = (v: unknown) => new Response(JSON.stringify(v));

describe("cache", () => {
  test("round-trips JSON and lists keys", () => {
    const c = new Cache(":memory:");
    c.write("a", { x: [1, 2] }, 1000);
    c.write("a", { x: [3] }, 2000);
    expect(c.read("a")).toEqual({ value: { x: [3] }, fetchedAt: 2000 });
    expect(c.read("missing")).toBeUndefined();
    expect(c.keys()).toEqual(["a"]);
  });

  test("prune removes only entries older than the age", () => {
    const c = new Cache(":memory:");
    c.write("old", 1, 1000);
    c.write("new", 2, 9000);
    expect(c.prune(5000, 10_000)).toBe(1);
    expect(c.keys()).toEqual(["new"]);
  });

  test("a fresh entry is served without touching the network", async () => {
    let calls = 0;
    const source = cachedSource(new Cache(":memory:"), { maxAgeMs: 60_000, fetch: async () => (calls++, json({ n: calls })) });
    expect((await source.get<{ n: number }>("https://x/a")).value.n).toBe(1);
    const again = await source.get<{ n: number }>("https://x/a");
    expect(again.value.n).toBe(1);
    expect(calls).toBe(1);
  });

  test("maxAge 0 forces a refresh", async () => {
    let calls = 0;
    const source = cachedSource(new Cache(":memory:"), { maxAgeMs: 60_000, fetch: async () => (calls++, json({ n: calls })) });
    await source.get("https://x/a");
    expect((await source.get<{ n: number }>("https://x/a", 0)).value.n).toBe(2);
  });

  test("when the upstream fails the stale copy is served and flagged", async () => {
    const cache = new Cache(":memory:");
    cache.write("https://x/a", { n: 7 }, 1);
    const source = cachedSource(cache, { maxAgeMs: 10, retries: 0, fetch: async () => new Response("down", { status: 503 }) });
    const got = await source.get<{ n: number }>("https://x/a");
    expect(got).toMatchObject({ value: { n: 7 }, stale: true, fetchedAt: 1 });
  });

  test("with nothing cached the upstream error surfaces", async () => {
    const source = cachedSource(new Cache(":memory:"), { maxAgeMs: 10, retries: 0, fetch: async () => new Response("down", { status: 503 }) });
    await expect(source.get("https://x/none")).rejects.toThrow("503");
  });

  test("the default path honours AY_CHROMIUM_RELEASES_DB and otherwise lives under the cli name from ay.toml", () => {
    const saved = process.env.AY_CHROMIUM_RELEASES_DB;
    process.env.AY_CHROMIUM_RELEASES_DB = "/tmp/x.sqlite";
    expect(defaultDbPath()).toBe("/tmp/x.sqlite");
    delete process.env.AY_CHROMIUM_RELEASES_DB;
    expect(defaultDbPath().replaceAll("\\", "/")).toMatch(/\/ay\/chromium-releases\.sqlite$/);
    if (saved) process.env.AY_CHROMIUM_RELEASES_DB = saved;
  });
});
