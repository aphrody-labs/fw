import { describe, expect, test } from "bun:test";
import { openDb } from "../src/db.ts";
import { GitHub, GitHubError } from "../src/github.ts";
import { eventData, syncItems } from "../src/items.ts";
import { getState } from "../src/state.ts";
import { json, noSleep, queueFetch, reply } from "./helpers.ts";

type Page = { data: { repository: { items: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: any[] } } } };
const pages = (name: string) => json<Page[]>(name);
const nodesOf = (ps: Page[]) => ps.flatMap(p => p.data.repository.items.nodes);
const client = (f: ReturnType<typeof queueFetch>) => new GitHub({ token: "t", fetch: f, sleep: noSleep, retries: 0 });

describe("issues from recorded GraphQL pages", () => {
  const ps = pages("graphql-issues.json");
  const nodes = nodesOf(ps);

  test("two pages are stored with comments, labels and the state reason", async () => {
    const db = openDb(":memory:");
    const fetch = queueFetch(reply(ps[0]), reply(ps[1]));
    const r = await syncItems(client(fetch), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 });
    expect(r.stored).toBe(nodes.length);
    expect(db.query("SELECT count(*) AS n FROM issues").get()).toEqual({ n: nodes.length });
    const first = nodes[0];
    expect(db.query("SELECT title, state, author, comments_count FROM issues WHERE number = ?").get(first.number)).toEqual({ title: first.title, state: first.state, author: first.author?.login ?? null, comments_count: first.comments.totalCount });
    expect((db.query("SELECT count(*) AS n FROM comments").get() as { n: number }).n).toBe(nodes.reduce((a, n) => a + n.comments.nodes.length, 0));
    expect((db.query("SELECT count(*) AS n FROM item_labels").get() as { n: number }).n).toBe(nodes.reduce((a, n) => a + n.labels.nodes.length, 0));
    expect(fetch.calls[0]!.body.variables).toMatchObject({ owner: "anomalyco", name: "opentunnel", first: 3, after: null });
    expect(fetch.calls[1]!.body.variables.after).toBe(ps[0]!.data.repository.items.pageInfo.endCursor);
    const state = getState(db, "anomalyco/opentunnel", "issues");
    expect(state).toMatchObject({ status: "idle", cursor: null });
    expect(state.until).toBe(nodes.map(n => n.updatedAt).sort().at(-1));
  });

  test("an interrupted crawl resumes from the stored cursor", async () => {
    const db = openDb(":memory:");
    const broken = queueFetch(reply(ps[0]), () => { throw new GitHubError("network down", 0); });
    await expect(syncItems(client(broken), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 })).rejects.toThrow("network down");
    const mid = getState(db, "anomalyco/opentunnel", "issues");
    expect(mid).toMatchObject({ status: "running", cursor: ps[0]!.data.repository.items.pageInfo.endCursor });
    const stored = (db.query("SELECT count(*) AS n FROM issues").get() as { n: number }).n;
    expect(stored).toBe(ps[0]!.data.repository.items.nodes.length);

    const resume = queueFetch(reply(ps[1]));
    await syncItems(client(resume), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 });
    expect(resume.calls[0]!.body.variables.after).toBe(mid.cursor!);
    expect((db.query("SELECT count(*) AS n FROM issues").get() as { n: number }).n).toBe(nodes.length);
    expect(getState(db, "anomalyco/opentunnel", "issues").status).toBe("idle");
  });

  test("the next crawl stops at the watermark of the previous one", async () => {
    const db = openDb(":memory:");
    await syncItems(client(queueFetch(reply(ps[0]), reply(ps[1]))), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 });
    const again = queueFetch(reply(ps[0]), reply(ps[1]));
    await syncItems(client(again), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 });
    expect(again.calls).toHaveLength(1);
  });

  test("--since goes further back than the watermark but does not advance it when it is newer", async () => {
    const db = openDb(":memory:");
    const f = queueFetch(reply(ps[0]));
    await syncItems(client(f), db, "anomalyco", "opentunnel", "issue", { pageSize: 3, since: "2999-01-01T00:00:00Z" });
    expect(f.calls).toHaveLength(1);
    expect(getState(db, "anomalyco/opentunnel", "issues").until).toBeNull();
  });

  test("a connection that overflows its first page is drained through node()", async () => {
    const db = openDb(":memory:");
    const copy = structuredClone(ps);
    const item = copy[0]!.data.repository.items.nodes[0];
    item.comments.pageInfo = { hasNextPage: true, endCursor: "c1" };
    const extra = { id: "IC_extra", author: { login: "late" }, body: "late comment", createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z", url: "https://example.test/c" };
    const fetch = queueFetch(reply(copy[0]), reply({ data: { node: { c: { pageInfo: { hasNextPage: false, endCursor: null }, nodes: [extra] } }, rateLimit: { cost: 1 } } }), reply(copy[1]));
    await syncItems(client(fetch), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 });
    expect(fetch.calls[1]!.body.variables).toEqual({ id: item.id, after: "c1" });
    expect(db.query("SELECT author, body FROM comments WHERE id = 'IC_extra'").get()).toEqual({ author: "late", body: "late comment" });
  });
});

describe("pull requests from recorded GraphQL pages", () => {
  const ps = pages("graphql-pulls.json");
  const nodes = nodesOf(ps);

  test("pull fields, reviews and timeline events are stored", async () => {
    const db = openDb(":memory:");
    await syncItems(client(queueFetch(reply(ps[0]), reply(ps[1]))), db, "anomalyco", "opentunnel", "pr", { pageSize: 3 });
    expect((db.query("SELECT count(*) AS n FROM pulls").get() as { n: number }).n).toBe(nodes.length);
    const n = nodes[0];
    expect(db.query("SELECT title, state, merged, base_ref, head_ref, additions, deletions FROM pulls WHERE number = ?").get(n.number)).toEqual({
      title: n.title, state: n.state, merged: +n.merged, base_ref: n.baseRefName, head_ref: n.headRefName, additions: n.additions, deletions: n.deletions,
    });
    expect((db.query("SELECT count(*) AS n FROM reviews").get() as { n: number }).n).toBe(nodes.reduce((a, x) => a + x.reviews.nodes.length, 0));
    expect((db.query("SELECT count(*) AS n FROM events WHERE kind = 'pr'").get() as { n: number }).n).toBe(nodes.reduce((a, x) => a + x.timelineItems.nodes.length, 0));
    expect(db.query("SELECT DISTINCT type FROM events WHERE kind = 'pr' AND type = 'MergedEvent'").all().length).toBe(nodes.some(x => x.timelineItems.nodes.some((e: any) => e.__typename === "MergedEvent")) ? 1 : 0);
  });

  test("re-storing an item replaces its children instead of duplicating them", async () => {
    const db = openDb(":memory:");
    for (let i = 0; i < 2; i++) await syncItems(client(queueFetch(reply(ps[0]), reply(ps[1]))), db, "anomalyco", "opentunnel", "pr", { pageSize: 3, since: "1970-01-01T00:00:00Z" });
    expect((db.query("SELECT count(*) AS n FROM comments WHERE kind = 'pr'").get() as { n: number }).n).toBe(nodes.reduce((a, x) => a + x.comments.nodes.length, 0));
  });
});

test("eventData keeps the type specific fields only", () => {
  expect(eventData({ __typename: "LabeledEvent", id: "1", createdAt: "x", label: { name: "bug" } })).toEqual({ label: "bug" });
  expect(eventData({ __typename: "CrossReferencedEvent", id: "1", createdAt: "x", isCrossRepository: false, source: { __typename: "PullRequest", number: 7, repository: { nameWithOwner: "o/r" } } })).toEqual({ isCrossRepository: false, source: { type: "PullRequest", number: 7, repo: "o/r" } });
  expect(eventData({ __typename: "ClosedEvent", id: "1", createdAt: "x" })).toEqual({});
});

describe("items GitHub cannot fully resolve", () => {
  const ps = pages("graphql-issues.json");

  test("a broken item is isolated by shrinking the page, skipped alone, and the page size recovers", async () => {
    const [a, b, c, d, e] = nodesOf(ps).slice(0, 5);
    const all = [a, b, c, d, e];
    const broken = c.number;
    const sizes: number[] = [];
    const server = async (_url: string, init?: RequestInit) => {
      const { variables } = JSON.parse(String(init!.body));
      sizes.push(variables.first);
      const start = variables.after ? Number(variables.after) : 0;
      const slice = all.slice(start, start + variables.first);
      const hit = slice.some(n => n.number === broken);
      const end = start + slice.length;
      return reply({
        data: { repository: { items: { pageInfo: { hasNextPage: end < all.length, endCursor: String(end) }, nodes: slice.map(n => (n.number === broken ? null : n)) } }, rateLimit: { cost: 1 } },
        ...(hit ? { errors: [{ type: "INTERNAL", message: "Something went wrong while executing your query", path: ["repository", "items", "nodes", 0] }] } : {}),
      });
    };
    const db = openDb(":memory:");
    const lines: string[] = [];
    await syncItems(new GitHub({ token: "t", fetch: server, sleep: noSleep }), db, "anomalyco", "opentunnel", "issue", { pageSize: 4, log: l => lines.push(l) });
    expect(db.query("SELECT number FROM issues ORDER BY number").all().map((r: any) => r.number)).toEqual([a, b, d, e].map(n => n.number).sort((x, y) => x - y));
    expect(lines.some(l => l.includes("skipped an item GitHub cannot resolve"))).toBe(true);
    expect(sizes).toContain(1);
    expect(getState(db, "anomalyco/opentunnel", "issues").status).toBe("idle");
  });

  test("null comments, labels and events inside an item are dropped, the rest is kept", async () => {
    const copy = structuredClone(ps);
    const n = copy[0]!.data.repository.items.nodes[0];
    n.comments.nodes.push(null);
    n.labels.nodes.push(null);
    n.timelineItems.nodes.push(null);
    const db = openDb(":memory:");
    await syncItems(client(queueFetch(reply(copy[0]), reply(copy[1]))), db, "anomalyco", "opentunnel", "issue", { pageSize: 3 });
    expect((db.query("SELECT count(*) AS n FROM comments WHERE number = ?").get(n.number) as { n: number }).n).toBe(n.comments.nodes.filter(Boolean).length);
  });
});
