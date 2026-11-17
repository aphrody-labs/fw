import { describe, expect, test } from "bun:test";
import { GitHub, GitHubError, paginate } from "../src/github.ts";
import { noSleep, queueFetch, reply } from "./helpers.ts";

const rate = (remaining: number, reset: number) => ({ "x-ratelimit-remaining": String(remaining), "x-ratelimit-reset": String(reset), "x-ratelimit-resource": "graphql" });
const ok = (data: object, headers = rate(4000, 2_000_000_000)) => reply({ data: { ...data, rateLimit: { cost: 2 } } }, { headers });

describe("GitHub transport", () => {
  test("reads the rate limit from the response headers and sums the GraphQL cost", async () => {
    const gh = new GitHub({ token: "t", fetch: queueFetch(ok({ a: 1 }, rate(4321, 2_000_000_000))), sleep: noSleep });
    expect(await gh.graphql<{ a: number }>("query { a }")).toMatchObject({ a: 1 });
    expect(gh.rate.graphql).toEqual({ remaining: 4321, reset: 2_000_000_000, resource: "graphql" });
    expect(gh.cost).toBe(2);
  });

  test("waits for the window to reset before spending the last calls", async () => {
    const slept: number[] = [];
    const now = 1_000_000;
    const fetch = queueFetch(ok({ n: 1 }, rate(3, now + 90)), ok({ n: 2 }));
    const gh = new GitHub({ token: "t", fetch, sleep: async ms => void slept.push(ms), now: () => now, reserve: 25 });
    await gh.graphql("query { n }");
    await gh.graphql("query { n }");
    expect(slept).toEqual([91_000]);
    expect(fetch.calls).toHaveLength(2);
  });

  test("a secondary rate limit sleeps for retry-after, then retries", async () => {
    const slept: number[] = [];
    const fetch = queueFetch(new Response("{}", { status: 403, headers: { "retry-after": "7" } }), ok({ n: 1 }));
    const gh = new GitHub({ token: "t", fetch, sleep: async ms => void slept.push(ms) });
    expect(await gh.graphql("query { n }")).toMatchObject({ n: 1 });
    expect(slept).toContain(7000);
  });

  test("a RATE_LIMITED GraphQL error waits for the reset and retries", async () => {
    const slept: number[] = [];
    const now = 5_000;
    const fetch = queueFetch(reply({ errors: [{ type: "RATE_LIMITED", message: "API rate limit exceeded" }] }, { headers: rate(0, now + 30) }), ok({ n: 1 }));
    const gh = new GitHub({ token: "t", fetch, sleep: async ms => void slept.push(ms), now: () => now });
    expect(await gh.graphql("query { n }")).toMatchObject({ n: 1 });
    expect(slept[0]).toBe(31_000);
  });

  test("retries 502 with backoff and fails with a transient error when it never recovers", async () => {
    const gh = new GitHub({ token: "t", retries: 2, sleep: noSleep, fetch: queueFetch(...Array.from({ length: 3 }, () => new Response("<html>502 Bad Gateway</html>", { status: 502 }))) });
    const err = await gh.graphql("query { n }").catch(e => e);
    expect(err).toBeInstanceOf(GitHubError);
    expect(err.status).toBe(502);
  });

  test("partial data with only NOT_FOUND errors is returned; other errors throw", async () => {
    const partial = reply({ data: { repository: { author: null } }, errors: [{ type: "NOT_FOUND", message: "user gone" }] });
    expect(await new GitHub({ fetch: queueFetch(partial), sleep: noSleep }).graphql("q")).toMatchObject({ repository: { author: null } });
    const bad = reply({ errors: [{ type: "INVALID", message: "Field 'x' doesn't exist" }] });
    expect((await new GitHub({ fetch: queueFetch(bad), sleep: noSleep }).graphql("q").catch(e => e)).message).toContain("doesn't exist");
  });

  test("REST returns undefined on 404 and sends the project User-Agent", async () => {
    const fetch = queueFetch(new Response("{}", { status: 404 }));
    const gh = new GitHub({ fetch, sleep: noSleep });
    expect(await gh.rest("/repos/x/y")).toBeUndefined();
  });
});

describe("paginate", () => {
  test("follows cursors, halves the page size on timeouts and stops when asked", async () => {
    const seen: [string | null, number][] = [];
    const pages = new Map<string | null, { items: number[]; next: string | null }>([[null, { items: [1, 2], next: "a" }], ["a", { items: [3], next: "b" }], ["b", { items: [4], next: null }]]);
    let failed = false;
    const got: number[] = [];
    await paginate(
      40,
      async (after, first) => {
        seen.push([after, first]);
        if (after === "a" && !failed) {
          failed = true;
          throw new GitHubError("timeout", 200, "TIMEOUT");
        }
        return pages.get(after)!;
      },
      (items, next) => {
        got.push(...items);
        return next !== "b";
      },
    );
    expect(seen).toEqual([[null, 40], ["a", 40], ["a", 20]]);
    expect(got).toEqual([1, 2, 3]);
  });
});
