import { describe, expect, test } from "bun:test";
import { openDb } from "../src/db.ts";
import { GitHub } from "../src/github.ts";
import { syncReleases } from "../src/releases.ts";
import { fetchRepos, storeRepo, syncLabelsAndMilestones } from "../src/repos.ts";
import { json, noSleep, queueFetch, reply } from "./helpers.ts";

const gh = (f: ReturnType<typeof queueFetch>) => new GitHub({ token: "t", fetch: f, sleep: noSleep, retries: 0 });

describe("repositories from a recorded GraphQL page", () => {
  const page = json("graphql-repos.json");
  const nodes = page.data.repositoryOwner.repositories.nodes;

  test("fetchRepos pages through the owner's repositories", async () => {
    const f = queueFetch(reply(page));
    const repos = await fetchRepos(gh(f), "anomalyco");
    expect(repos.map(r => r.name)).toEqual(nodes.map((n: any) => n.name));
    expect(f.calls[0]!.body.variables).toMatchObject({ owner: "anomalyco", first: 25, after: null });
  });

  test("storeRepo keeps metadata, GitHub's totals, language shares and topics", async () => {
    const db = openDb(":memory:");
    const repos = await fetchRepos(gh(queueFetch(reply(page))), "anomalyco");
    for (const r of repos) storeRepo(db, "anomalyco", r);
    const n = nodes[0];
    expect(db.query("SELECT name, stars, is_fork, issues_total, prs_total, releases_total, primary_language, pushed_at FROM repos WHERE full_name = ?").get(n.nameWithOwner)).toEqual({
      name: n.name, stars: n.stargazerCount, is_fork: +n.isFork, issues_total: n.allIssues.totalCount, prs_total: n.allPrs.totalCount, releases_total: n.releases.totalCount, primary_language: n.primaryLanguage?.name ?? null, pushed_at: n.pushedAt,
    });
    const share = db.query("SELECT sum(share) AS s FROM repo_languages WHERE repo = ?").get(n.nameWithOwner) as { s: number | null };
    if (n.languages.edges.length) expect(share.s).toBeGreaterThan(0.9);
    expect((db.query("SELECT count(*) AS n FROM repo_topics").get() as { n: number }).n).toBe(nodes.reduce((a: number, x: any) => a + x.repositoryTopics.nodes.length, 0));
  });

  test("re-storing a repository replaces its language rows and keeps a known git commit count", async () => {
    const db = openDb(":memory:");
    const [r] = await fetchRepos(gh(queueFetch(reply(page))), "anomalyco");
    storeRepo(db, "anomalyco", r!);
    db.query("UPDATE repos SET default_commits_github = 41, default_commits_git = 41").run();
    const smaller = structuredClone(r!);
    smaller.languages = { totalSize: 10, edges: [{ size: 10, node: { name: "Zig" } }] };
    storeRepo(db, "anomalyco", smaller);
    expect(db.query("SELECT language, share FROM repo_languages").all()).toEqual([{ language: "Zig", share: 1 }]);
    expect(db.query("SELECT default_commits_github AS g FROM repos").get()).toEqual({ g: 41 });
  });

  test("labels and milestones are replaced as a whole", async () => {
    const db = openDb(":memory:");
    const labels = (names: string[]) => reply({ data: { repository: { labels: { pageInfo: { hasNextPage: false, endCursor: null }, nodes: names.map(name => ({ name, color: "ffffff", description: null })) } }, rateLimit: { cost: 1 } } });
    const ms = () => reply({ data: { repository: { milestones: { pageInfo: { hasNextPage: false, endCursor: null }, nodes: [{ number: 1, title: "v1", state: "OPEN", description: null, dueOn: null, createdAt: "2026-01-01T00:00:00Z", closedAt: null, issues: { totalCount: 5 }, closed: { totalCount: 2 } }] } }, rateLimit: { cost: 1 } } });
    await syncLabelsAndMilestones(gh(queueFetch(labels(["a", "b"]), ms())), db, "o", "r");
    await syncLabelsAndMilestones(gh(queueFetch(labels(["b"]), ms())), db, "o", "r");
    expect(db.query("SELECT name FROM labels").all()).toEqual([{ name: "b" }]);
    expect(db.query("SELECT title, open_issues, closed_issues FROM milestones").all()).toEqual([{ title: "v1", open_issues: 3, closed_issues: 2 }]);
  });
});

describe("releases from a recorded GraphQL page", () => {
  const page = json("graphql-releases.json");
  const nodes = page.data.repository.releases.nodes;

  test("notes, flags and asset metadata are stored; a second sync replaces rather than duplicates", async () => {
    const db = openDb(":memory:");
    for (let i = 0; i < 2; i++) expect(await syncReleases(gh(queueFetch(reply(page))), db, "anomalyco", "opentunnel")).toBe(nodes.length);
    expect((db.query("SELECT count(*) AS n FROM releases").get() as { n: number }).n).toBe(nodes.length);
    const r = nodes[0];
    expect(db.query("SELECT name, body, is_prerelease, published_at, author FROM releases WHERE tag = ?").get(r.tagName)).toEqual({ name: r.name, body: r.description, is_prerelease: +r.isPrerelease, published_at: r.publishedAt, author: r.author?.login ?? null });
    expect((db.query("SELECT count(*) AS n FROM release_assets").get() as { n: number }).n).toBe(nodes.reduce((a: number, x: any) => a + x.releaseAssets.nodes.length, 0));
    const rel = nodes.find((x: any) => x.releaseAssets.nodes.length);
    const a = rel?.releaseAssets.nodes[0];
    if (a) expect(db.query("SELECT size, downloads, content_type FROM release_assets WHERE tag = ? AND name = ?").get(rel.tagName, a.name)).toEqual({ size: a.size, downloads: a.downloadCount, content_type: a.contentType });
  });
});

test("every stored repository gets language tags from its byte shares; a later language change replaces them", async () => {
  const db = openDb(":memory:");
  const [r] = await fetchRepos(gh(queueFetch(reply(json("graphql-repos.json")))), "anomalyco");
  const mixed = structuredClone(r!);
  mixed.languages = { totalSize: 100, edges: [{ size: 60, node: { name: "Rust" } }, { size: 35, node: { name: "Zig" } }, { size: 5, node: { name: "Shell" } }] };
  storeRepo(db, "anomalyco", mixed);
  expect(db.query("SELECT tag, via FROM repo_tags ORDER BY tag").all()).toEqual([{ tag: "rust", via: "language" }, { tag: "zig", via: "language" }]);
  mixed.languages = { totalSize: 10, edges: [{ size: 10, node: { name: "Python" } }] };
  storeRepo(db, "anomalyco", mixed);
  expect(db.query("SELECT tag FROM repo_tags").all()).toEqual([{ tag: "python" }]);
});
