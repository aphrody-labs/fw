import { describe, expect, setDefaultTimeout, test } from "bun:test";
import { renameSync } from "node:fs";
import { join } from "node:path";
import { openDb } from "../src/db.ts";
import { exportToml, KEEP_MARKER, withKept } from "../src/export.ts";
import { GitHub } from "../src/github.ts";
import { formatStats, stats } from "../src/stats.ts";
import { syncOrg } from "../src/sync.ts";
import { json, makeRepo, noSleep, reply, tmp } from "./helpers.ts";

setDefaultTimeout(120_000);

const empty = (field: string) => ({ data: { repository: { [field]: { pageInfo: { hasNextPage: false, endCursor: null }, nodes: [] } } } });

/** A GitHub that answers from the recorded GraphQL pages and registries, routed by what the query asks for. */
function world(url: string) {
  const issues = json<any[]>("graphql-issues.json");
  const pulls = json<any[]>("graphql-pulls.json");
  const single = (ps: any[]) => { const p = structuredClone(ps[0]); p.data.repository.items.pageInfo.hasNextPage = false; return p; };
  const repo = structuredClone(json("graphql-repos.json").data.repositoryOwner.repositories.nodes[0]);
  Object.assign(repo, { name: "r", nameWithOwner: "o/r", url, defaultBranchRef: { name: "main", target: { history: { totalCount: 2 } } } });
  const calls: string[] = [];
  const fetch = async (u: string, init?: RequestInit) => {
    calls.push(u);
    if (!u.includes("api.github.com")) {
      if (u.includes("-/v1/search")) return reply({ total: 0, objects: [] });
      if (u.includes("api.npmjs.org")) return reply({ downloads: 7 });
      if (u.endsWith("/@o%2fpub") || u.endsWith("/pub")) return reply({ name: "pub", "dist-tags": { latest: "1.0.0" }, repository: { url: "git+https://github.com/o/r.git" }, time: { created: "2026-01-01T00:00:00Z", "1.0.0": "2026-01-02T00:00:00Z" }, versions: { "1.0.0": {} } });
      return new Response("{}", { status: 404 });
    }
    const q: string = JSON.parse(String(init!.body)).query;
    const wrap = (data: object) => reply({ data: { ...data, rateLimit: { cost: 1, remaining: 4000 } } }, { headers: { "x-ratelimit-remaining": "4000", "x-ratelimit-reset": "2000000000" } });
    if (q.includes("labels(first: $first")) return wrap({ repository: { labels: { pageInfo: { hasNextPage: false, endCursor: null }, nodes: [{ name: "bug", color: "d73a4a", description: "Something is broken" }] } } });
    if (q.includes("milestones(first: $first")) return wrap(empty("milestones").data);
    if (q.includes("releases(first: $first")) return wrap(json("graphql-releases.json").data);
    if (q.includes("issues(first: $first")) return wrap(single(issues).data);
    if (q.includes("pullRequests(first: $first")) return wrap(single(pulls).data);
    return wrap({ repository: repo });
  };
  return { fetch, calls };
}

describe("syncOrg end to end against a local origin and recorded API answers", () => {
  test("every step lands in the replica; a second sync is incremental and idempotent", async () => {
    const origin = await makeRepo([
      { message: "init", files: { "package.json": '{"name":"pub","version":"1.0.0"}', "CHANGELOG.md": "# Changelog\n\n## 1.0.0 - 2026-01-02\n\n- first release\n", "bun.lock": "{}" }, tag: "v1.0.0" },
      { message: "more", files: { "a.ts": "export {}\n" } },
    ]);
    const bare = origin + ".git";
    renameSync(origin, bare);
    const w = world(bare.slice(0, -4));
    const db = openDb(":memory:");
    const root = tmp("org-mirror-data-");
    const run = () => syncOrg({ org: "o", db, root, gh: new GitHub({ token: "t", fetch: w.fetch, sleep: noSleep }), repos: ["r"], registry: { fetch: w.fetch, sleep: noSleep } });

    const first = await run();
    expect(first.errors).toEqual([]);
    expect(first.packages).toEqual({ ok: 1, missing: 0, foreign: 0, error: 0 });
    const n = (t: string) => (db.query(`SELECT count(*) AS n FROM ${t}`).get() as { n: number }).n;
    expect({ commits: n("commits"), refs: n("refs"), issues: n("issues"), pulls: n("pulls"), releases: n("releases"), labels: n("labels"), changelog: n("changelog_entries"), packages: n("package_versions") }).toEqual({
      commits: 2, refs: 2, issues: json("graphql-issues.json")[0].data.repository.items.nodes.length, pulls: json("graphql-pulls.json")[0].data.repository.items.nodes.length,
      releases: json("graphql-releases.json").data.repository.releases.nodes.length, labels: 1, changelog: 1, packages: 1,
    });
    expect(db.query("SELECT tag FROM repo_tags ORDER BY tag").all().map((r: any) => r.tag)).toContain("bun");
    expect(db.query("SELECT default_commits_git AS g, default_commits_github AS h FROM repos").get()).toEqual({ g: 2, h: 2 });
    expect(db.query("SELECT status FROM sync_state WHERE kind IN ('issues','pulls')").all()).toEqual([{ status: "idle" }, { status: "idle" }]);

    const before = Object.fromEntries(["commits", "issues", "pulls", "comments", "events", "releases", "release_assets"].map(t => [t, n(t)]));
    const second = await run();
    expect(second.errors).toEqual([]);
    expect(Object.fromEntries(Object.keys(before).map(t => [t, n(t)]))).toEqual(before);

    const s = stats(db, "o");
    expect(s.compare).toHaveLength(1);
    expect(s.compare[0]!.commits).toEqual([2, 2]);
    expect(formatStats(s)).toContain("o/r");
  });

  test("a failing step is reported and recorded without stopping the others", async () => {
    const origin = await makeRepo([{ message: "init", files: { a: "1" } }]);
    const bare = origin + ".git";
    renameSync(origin, bare);
    const w = world(bare.slice(0, -4));
    const db = openDb(":memory:");
    const gh = new GitHub({ token: "t", sleep: noSleep, retries: 0, fetch: async (u, init) => (String(init!.body).includes("releases(first") ? new Response("boom", { status: 500 }) : w.fetch(u, init)) });
    const r = await syncOrg({ org: "o", db, root: tmp(), gh, repos: ["r"], steps: ["repos", "git", "items", "releases"] });
    expect(r.errors.map(e => e.step)).toEqual(["releases"]);
    expect((db.query("SELECT count(*) AS n FROM issues").get() as { n: number }).n).toBeGreaterThan(0);
    expect(db.query("SELECT status FROM sync_state WHERE kind = 'error:releases'").get()).toEqual({ status: "error" });
  });
});

describe("export", () => {
  test("the summary is valid TOML with counts and no bulk data; hand-written tails survive regeneration", async () => {
    const db = openDb(":memory:");
    db.query("INSERT INTO repos (full_name, org, name, stars, primary_language, pushed_at, license, description, issues_total) VALUES ('o/r', 'o', 'r', 12, 'Rust', '2026-11-01T00:00:00Z', 'MIT', 'A \"quoted\" description', 3)").run();
    db.query("INSERT INTO repo_languages (repo, language, bytes, share) VALUES ('o/r', 'C++', 100, 1)").run();
    db.query("INSERT INTO repo_tags (repo, tag, via) VALUES ('o/r', 'rust', 'language')").run();
    db.query("INSERT INTO issues (repo, number, title) VALUES ('o/r', 1, 'secret body must not be exported')").run();
    db.query("INSERT INTO refs (repo, name, kind, sha) VALUES ('o/r', 'main', 'branch', 'abc')").run();
    db.query("INSERT INTO packages (ecosystem, name, repo, found_by, status, latest, versions_count, downloads_recent, downloads_window) VALUES ('npm', '@o/pkg', 'o/r', 'manifest', 'ok', '1.0.0', 4, 99, '30d')").run();
    const text = exportToml(db, "o", new Date("2026-11-16T12:00:00Z"));
    const t = Bun.TOML.parse(text) as any;
    expect(t.org).toMatchObject({ repos: 1, deep_synced: 1 });
    expect(t.rows).toMatchObject({ repos: 1, issues: 1, packages: 1 });
    expect(t.languages["C++"]).toEqual({ bytes: 100, repos: 1 });
    expect(t.tags.rust).toEqual(["r"]);
    expect(t.repo[0]).toMatchObject({ name: "r", stars: 12, language: "Rust", pushed: "2026-11-01", description: 'A "quoted" description', tags: ["rust"] });
    expect(t.repo[0].github.issues).toBe(3);
    expect(t.package[0]).toMatchObject({ ecosystem: "npm", name: "@o/pkg", downloads: { recent: 99, window: "30d" } });
    expect(text).not.toContain("secret body");
    const kept = withKept(text, `old\n${KEEP_MARKER}\n[sites]\nopencode = "https://opencode.ai"\n`);
    expect((Bun.TOML.parse(kept) as any).sites.opencode).toBe("https://opencode.ai");
    expect(kept.match(/\[org\]/g)).toHaveLength(1);
    expect(withKept(text, undefined).endsWith("\n")).toBe(true);
  });
});

test("the CLI prints stats and writes the export of an existing replica", async () => {
  const dir = tmp("org-mirror-cli-");
  const dbPath = join(dir, "m.db");
  const db = openDb(dbPath);
  db.query("INSERT INTO repos (full_name, org, name, stars, pushed_at) VALUES ('o/r', 'o', 'r', 1, '2026-11-01T00:00:00Z')").run();
  db.close();
  const cli = join(import.meta.dir, "../src/cli.ts");
  const run = (...args: string[]) => Bun.spawnSync(["bun", cli, ...args], { stdout: "pipe", stderr: "pipe" });
  const s = run("stats", "o", "--db", dbPath, "--json");
  expect(JSON.parse(s.stdout.toString()).tables.repos).toBe(1);
  const out = join(dir, "o.toml");
  const e = run("export", "o", "--toml", "--db", dbPath, "--out", out);
  expect(e.exitCode).toBe(0);
  expect((Bun.TOML.parse(await Bun.file(out).text()) as any).org.repos).toBe(1);
  expect(run("export", "o", "--db", dbPath).exitCode).not.toBe(0);
  expect(run("sync", "o", "--steps", "nonsense", "--db", dbPath).stderr.toString()).toContain("unknown step");
  expect(run().exitCode).toBe(2);
});
