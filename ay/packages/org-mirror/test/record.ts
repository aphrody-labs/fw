#!/usr/bin/env bun
/** Re-record the fixtures from the live services: `bun packages/org-mirror/test/record.ts`. Responses are real, trimmed to a few nodes. */
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { openDb } from "../src/db.ts";
import { GitHub, discoverToken, type FetchLike } from "../src/github.ts";
import { syncItems } from "../src/items.ts";
import { fetchRepos } from "../src/repos.ts";
import { syncReleases } from "../src/releases.ts";
import { USER_AGENT } from "../src/github.ts";

const dir = join(import.meta.dir, "fixtures");
mkdirSync(dir, { recursive: true });
const save = (name: string, data: unknown) => writeFileSync(join(dir, name), JSON.stringify(data, null, 1) + "\n");
const clip = (s: unknown, n = 400) => (typeof s === "string" && s.length > n ? s.slice(0, n) : s);

/** Trim a GraphQL body: long text cut, connection node lists cut to `n` entries. */
function trim(v: unknown, n: number): unknown {
  if (Array.isArray(v)) return v.slice(0, n).map(x => trim(x, n));
  if (v && typeof v === "object") return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, k === "body" || k === "description" ? clip(x, 400) : trim(x, n)]));
  return v;
}

/** A fetch that records the first `pages` GraphQL bodies and ends the crawl by clearing hasNextPage on the last one. */
function recorder(sink: unknown[], pages: number, nodes: number): FetchLike {
  return async (url, init) => {
    const res = await fetch(url, init);
    const body = await res.clone().json();
    if (sink.length < pages) {
      const t = trim(body, nodes) as { data?: Record<string, unknown> };
      sink.push(t);
      if (sink.length === pages) JSON.stringify(t, (k, x) => { if (k === "pageInfo" && x?.hasNextPage) x.hasNextPage = false; return x; });
    }
    return res;
  };
}

const token = discoverToken();
const org = "anomalyco";

// repos (one page, 3 repos)
{
  const sink: unknown[] = [];
  await fetchRepos(new GitHub({ token, fetch: recorder(sink, 1, 3) }), org).catch(() => {});
  const page = sink[0] as { data: { repositoryOwner: { repositories: { pageInfo: { hasNextPage: boolean } } } } };
  page.data.repositoryOwner.repositories.pageInfo.hasNextPage = false;
  save("graphql-repos.json", page);
}

// issues and pull requests of a small repo, two pages each of 3 items
for (const [kind, file] of [["issue", "graphql-issues.json"], ["pr", "graphql-pulls.json"]] as const) {
  const sink: unknown[] = [];
  const db = openDb(":memory:");
  const gh = new GitHub({ token, fetch: async (url, init) => {
    if (sink.length >= 2) throw new Error("recorded enough");
    return recorder(sink, 2, 3)(url, init);
  } });
  await syncItems(gh, db, org, "opentunnel", kind, { pageSize: 3 }).catch(() => {});
  (sink[1] as { data: { repository: { items: { pageInfo: { hasNextPage: boolean } } } } }).data.repository.items.pageInfo.hasNextPage = false;
  save(file, sink);
}

// releases
{
  const sink: unknown[] = [];
  const db = openDb(":memory:");
  await syncReleases(new GitHub({ token, fetch: recorder(sink, 1, 3) }), db, org, "opentunnel").catch(() => {});
  (sink[0] as { data: { repository: { releases: { pageInfo: { hasNextPage: boolean } } } } }).data.repository.releases.pageInfo.hasNextPage = false;
  save("graphql-releases.json", sink[0]);
}

const get = async (url: string) => (await (await fetch(url, { headers: { "user-agent": USER_AGENT } })).json()) as Record<string, any>;
const text = async (url: string) => (await fetch(url, { headers: { "user-agent": USER_AGENT } })).text();
/** A file of a repository at its default branch, through the contents API. */
const repoFile = async (repo: string, path: string) => {
  const res = await fetch(`https://api.github.com/repos/${repo}/contents/${path}`, { headers: { "user-agent": USER_AGENT, accept: "application/vnd.github.raw", ...(token ? { authorization: `Bearer ${token}` } : {}) } });
  if (!res.ok) throw new Error(`${res.status} ${repo}/${path}`);
  return res.text();
};
const head = (s: string, n: number) => s.split("\n").slice(0, n).join("\n") + "\n";

// npm packument (3 newest versions) and its download count
{
  const doc = await get("https://registry.npmjs.org/opentunnel");
  const keep = Object.keys(doc.versions).slice(-3);
  doc.versions = Object.fromEntries(keep.map(v => [v, { name: doc.name, version: v, ...(doc.versions[v].deprecated ? { deprecated: doc.versions[v].deprecated } : {}) }]));
  doc.time = Object.fromEntries(Object.entries(doc.time).filter(([k]) => ["created", "modified", ...keep].includes(k)));
  delete doc.readme;
  delete doc.maintainers;
  save("npm-opentunnel.json", doc);
  save("npm-downloads.json", await get("https://api.npmjs.org/downloads/point/last-month/opentunnel"));
  const s = await get("https://registry.npmjs.org/-/v1/search?text=%40opentunnel&size=250");
  save("npm-search-scope.json", { total: s.total, objects: s.objects.slice(0, 5).map((o: any) => ({ package: { name: o.package.name } })) });
}

// crates.io (3 newest versions)
{
  const doc = await get("https://crates.io/api/v1/crates/opentunnel");
  save("crates-opentunnel.json", { crate: Object.fromEntries(Object.entries(doc.crate).filter(([, v]) => typeof v !== "object" || v === null)), versions: doc.versions.slice(0, 3).map((v: any) => ({ num: v.num, created_at: v.created_at, downloads: v.downloads, yanked: v.yanked, license: v.license })) });
}

// PyPI (a real small package; releases cut to 3) and pypistats
{
  const doc = await get("https://pypi.org/pypi/tomli/json");
  const keep = Object.keys(doc.releases).slice(-3);
  save("pypi-tomli.json", { info: { name: doc.info.name, summary: doc.info.summary, version: doc.info.version, home_page: doc.info.home_page, license: doc.info.license, license_expression: doc.info.license_expression, project_urls: doc.info.project_urls }, releases: Object.fromEntries(keep.map(v => [v, doc.releases[v].slice(0, 1).map((f: any) => ({ upload_time_iso_8601: f.upload_time_iso_8601, yanked: f.yanked }))])) });
  save("pypistats-tomli.json", await get("https://pypistats.org/api/packages/tomli/recent").catch(() => ({ data: { last_month: 0 } })));
}

// changelogs in three real styles
writeFileSync(join(dir, "changelog-changesets.md"), head(await repoFile("anomalyco/opentunnel", "packages/cli/CHANGELOG.md"), 60));
writeFileSync(join(dir, "changelog-keepachangelog.md"), head(await text("https://raw.githubusercontent.com/olivierlacan/keep-a-changelog/main/CHANGELOG.md"), 90));
writeFileSync(join(dir, "changelog-history.md"), head(await text("https://raw.githubusercontent.com/expressjs/express/master/History.md"), 60));

// manifests
writeFileSync(join(dir, "package.json"), await repoFile("anomalyco/opentunnel", "packages/client/package.json"));
writeFileSync(join(dir, "Cargo.toml"), await repoFile("anomalyco/opentunnel", "crates/opentunnel-cli/Cargo.toml"));
writeFileSync(join(dir, "pyproject.toml"), await text("https://raw.githubusercontent.com/pallets/click/main/pyproject.toml"));
writeFileSync(join(dir, "build.zig.zon"), await repoFile("anomalyco/opentui", "packages/native/build.zig.zon"));
console.log("recorded into", dir);
