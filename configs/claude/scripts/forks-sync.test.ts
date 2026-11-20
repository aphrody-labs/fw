import { expect, test } from "bun:test";
import {
  classify,
  type Fork,
  type Gh,
  listForks,
  parseArgs,
  preflight,
  type Reply,
  renderMarkdown,
  type Result,
  type State,
  summarize,
  syncAll,
} from "./forks-sync.ts";

const fork = (over: Partial<Fork> = {}): Fork => ({
  name: "o/a",
  branch: "main",
  oid: "1",
  archived: false,
  private: false,
  parent: { name: "up/a", branch: "main", oid: "2", archived: false },
  ...over,
});

test("preflight decides without a call", () => {
  expect(preflight(fork({ parent: null }))?.outcome).toBe("parent-missing");
  expect(preflight(fork({ archived: true }))?.outcome).toBe("fork-archived");
  expect(
    preflight(fork({ parent: { name: "up/a", branch: "master", oid: "2", archived: false } }))?.outcome,
  ).toBe("branch-mismatch");
  expect(
    preflight(fork({ oid: "2" }))?.outcome,
  ).toBe("up-to-date");
  expect(preflight(fork())).toBeUndefined();
  expect(
    preflight(fork({ oid: "2", parent: { name: "up/a", branch: "main", oid: "2", archived: true } }))?.parentArchived,
  ).toBe(true);
});

test("classify maps merge-upstream replies", () => {
  const r = (status: number, body: unknown): Reply => ({ status, body });
  expect(classify(r(200, { merge_type: "fast-forward" })).outcome).toBe("fast-forward");
  expect(classify(r(200, { merge_type: "merge" })).outcome).toBe("merge");
  expect(classify(r(200, { merge_type: "none" })).outcome).toBe("up-to-date");
  expect(classify(r(409, { message: "There are merge conflicts" })).outcome).toBe("conflict");
  expect(classify(r(422, { message: "This branch is not behind the upstream x:main." })).outcome).toBe("up-to-date");
  expect(classify(r(404, { message: "Not Found" })).outcome).toBe("error");
});

test("listForks paginates", async () => {
  const node = (n: string) => ({
    nameWithOwner: `o/${n}`,
    isArchived: false,
    isPrivate: false,
    defaultBranchRef: { name: "main", target: { oid: "1" } },
    parent: null,
  });
  const pages = [
    { hasNextPage: true, endCursor: "c1", nodes: [node("a")] },
    { hasNextPage: false, endCursor: null, nodes: [node("b")] },
  ];
  const seen: unknown[] = [];
  const gh = {
    graphql: async (_q: string, v: Record<string, unknown>) => {
      seen.push(v.after);
      const p = pages[seen.length - 1]!;
      return { organization: { repositories: { totalCount: 2, pageInfo: p, nodes: p.nodes } } };
    },
  } as unknown as Gh;
  expect((await listForks(gh, "o")).map((f) => f.name)).toEqual(["o/a", "o/b"]);
  expect(seen).toEqual([null, "c1"]);
});

test("syncAll posts only when needed, records conflicts with divergence, bounds concurrency", async () => {
  const forks = [
    fork({ name: "o/ff" }),
    fork({ name: "o/cf" }),
    fork({ name: "o/same", oid: "2" }),
    fork({ name: "o/boom" }),
  ];
  const posts: string[] = [];
  let live = 0;
  let peak = 0;
  const gh: Gh = {
    graphql: async () => ({}),
    post: async (path) => {
      posts.push(path);
      live++;
      peak = Math.max(peak, live);
      await Bun.sleep(5);
      live--;
      if (path.includes("/o/ff/")) return { status: 200, body: { merge_type: "fast-forward" } };
      if (path.includes("/o/cf/")) return { status: 409, body: { message: "There are merge conflicts" } };
      throw new Error("network");
    },
    get: async () => ({ status: 200, body: { ahead_by: 3, behind_by: 7 } }),
  };
  const state: State = { org: "o", startedAt: "", results: {} };
  let saves = 0;
  await syncAll(gh, forks, { concurrency: 2, intervalMs: 0, dryRun: false }, state, () => saves++);
  expect(posts.sort()).toEqual(["/repos/o/boom/merge-upstream", "/repos/o/cf/merge-upstream", "/repos/o/ff/merge-upstream"]);
  expect(peak).toBeLessThanOrEqual(2);
  expect(state.results["o/ff"]!.outcome).toBe("fast-forward");
  expect(state.results["o/same"]!.outcome).toBe("up-to-date");
  expect(state.results["o/boom"]!.outcome).toBe("error");
  expect(state.results["o/cf"]).toMatchObject({ outcome: "conflict", ahead: 3, behind: 7 });
  expect(saves).toBeGreaterThan(0);
});

test("report counts and lists conflicts", () => {
  const rs: Result[] = [
    { fork: "o/a", outcome: "conflict", branch: "main", parent: "u/a", parentArchived: false, before: "1", ahead: 2, behind: 5, at: "t" },
    { fork: "o/b", outcome: "up-to-date", branch: "main", parent: "u/b", parentArchived: true, before: "1", at: "t" },
  ];
  expect(summarize(rs).conflict).toBe(1);
  const md = renderMarkdown("o", rs, "now");
  expect(md).toContain("## Conflicts (1)");
  expect(md).toContain("| o/a | u/a | 2 | 5 |");
  expect(md).toContain("| parent archived (flag) | 1 |");
});

test("parseArgs", () => {
  expect(parseArgs(["--resume", "--only", "a,b"]).resumeHours).toBe(12);
  expect(parseArgs(["--resume", "3"]).resumeHours).toBe(3);
  expect(parseArgs(["--only", "a,b", "--concurrency", "6"]).concurrency).toBe(6);
  expect(() => parseArgs(["--nope"])).toThrow();
});
