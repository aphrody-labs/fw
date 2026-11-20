import { describe, expect, test } from "bun:test";
import { openDb } from "../src/db.ts";
import { belongs, manifestCandidates, syncPackages } from "../src/packages.ts";
import { fetchCrate, fetchNpm, fetchPypi, npmScope, parseCrates, parseNpm, parsePypi } from "../src/registries.ts";
import { json, noSleep, queueFetch, reply } from "./helpers.ts";

describe("npm", () => {
  const doc = json("npm-opentunnel.json");
  const downloads = json("npm-downloads.json");

  test("packument and download count become one PackageInfo", () => {
    const info = parseNpm(doc, downloads);
    expect(info).toMatchObject({ status: "ok", latest: doc["dist-tags"].latest, downloadsRecent: downloads.downloads, downloadsWindow: "30d", url: "https://www.npmjs.com/package/opentunnel" });
    expect(info.versions.map(v => v.version).sort()).toEqual(Object.keys(doc.versions).sort());
    for (const v of info.versions) expect(v.publishedAt).toBe(doc.time[v.version]);
    expect(info.lastPublished).toBe(info.versions.map(v => v.publishedAt!).sort().at(-1)!);
    expect(info.createdAt).toBe(doc.time.created);
  });

  test("fetchNpm asks the registry and the downloads API; a scoped name is encoded; 404 is missing", async () => {
    const f = queueFetch(reply(doc), reply(downloads), new Response("{}", { status: 404 }));
    expect((await fetchNpm("@scope/name", { fetch: f, sleep: noSleep })).status).toBe("ok");
    expect(f.calls.map(c => c.url)).toEqual(["https://registry.npmjs.org/@scope%2fname", "https://api.npmjs.org/downloads/point/last-month/@scope/name"]);
    expect((await fetchNpm("nope", { fetch: f, sleep: noSleep })).status).toBe("missing");
  });

  test("scope search keeps only @scope/ names and stops at the first page without one", async () => {
    const search = json("npm-search-scope.json");
    const names = search.objects.map((o: any) => o.package.name);
    const f = queueFetch(reply(search));
    const got = await npmScope("opentunnel", { fetch: f, sleep: noSleep });
    expect(got).toEqual(names.filter((n: string) => n.startsWith("@opentunnel/")));
    expect(got.length).toBeGreaterThan(0);
    expect(f.calls[0]!.url).toContain("text=%40opentunnel");
  });
});

describe("crates.io", () => {
  const doc = json("crates-opentunnel.json");
  test("versions with dates, downloads and the 90 day window", () => {
    const info = parseCrates(doc);
    expect(info).toMatchObject({ status: "ok", latest: doc.crate.max_version, downloadsTotal: doc.crate.downloads, downloadsRecent: doc.crate.recent_downloads, downloadsWindow: "90d" });
    expect(info.versions).toHaveLength(doc.versions.length);
    expect(info.versions[0]).toEqual({ version: doc.versions[0].num, publishedAt: doc.versions[0].created_at, downloads: doc.versions[0].downloads, yanked: doc.versions[0].yanked });
  });

  test("requests carry the project User-Agent without an email", async () => {
    let ua = "";
    const info = await fetchCrate("opentunnel", { sleep: noSleep, fetch: async (_u, init) => { ua = (init!.headers as Record<string, string>)["user-agent"]!; return reply(doc); } });
    expect(info.status).toBe("ok");
    expect(ua).toContain("aphrody-org-mirror");
    expect(ua).toContain("github.com/aphrody-labs/ay");
    expect(ua).not.toContain("@");
  });
});

describe("PyPI", () => {
  const doc = json("pypi-tomli.json");
  const stats = json("pypistats-tomli.json");
  test("first upload time is the publish date; the repository is found in project_urls", () => {
    const info = parsePypi(doc, stats);
    expect(info.versions.map(v => v.version).sort()).toEqual(Object.keys(doc.releases).sort());
    expect(info.latest).toBe(doc.info.version);
    expect(info.repository ?? "").toMatch(/github\.com|^$/);
    expect(info.downloadsRecent).toBe(stats.data?.last_month ?? null);
  });

  test("pypistats failing does not fail the package", async () => {
    const info = await fetchPypi("tomli", { sleep: noSleep, fetch: queueFetch(reply(doc), new Response("{}", { status: 429 })) });
    expect(info.status).toBe("ok");
    expect(info.downloadsRecent).toBeNull();
  });
});

describe("package discovery", () => {
  test("belongs: the repo's own URL, a renamed owner after the repo was created, never an older namesake", () => {
    const base = { repository: null, versions: [] } as any;
    const at = (url: string | null, published: string) => ({ repository: url, versions: [{ version: "1.0.0", publishedAt: published, downloads: null, yanked: false }] }) as any;
    const created = "2025-04-30T00:00:00Z";
    expect(belongs(at("git+https://github.com/anomalyco/opencode.git", "2020-01-01T00:00:00Z"), "anomalyco/opencode", created)).toBe(true);
    expect(belongs(at("git+https://github.com/sst/opencode.git", "2025-06-01T00:00:00Z"), "anomalyco/opencode", created)).toBe(true);
    expect(belongs(at("https://github.com/rune-rs/rune", "2020-01-01T00:00:00Z"), "anomalyco/rune", created)).toBe(false);
    expect(belongs(at("https://github.com/someone/example", "2026-01-01T00:00:00Z"), "anomalyco/opencode", created)).toBe(false);
    expect(belongs(base, "anomalyco/opencode", created)).toBe(true);
  });

  test("manifest names become candidates; private and unnamed manifests do not", () => {
    const db = openDb(":memory:");
    const ins = db.query("INSERT INTO manifests (repo, path, ecosystem, name, version, private) VALUES (?, ?, ?, ?, ?, ?)");
    ins.run("o/a", "package.json", "npm", "pub", "1.0.0", 0);
    ins.run("o/a", "packages/x/package.json", "npm", "priv", "1.0.0", 1);
    ins.run("o/a", "Cargo.toml", "crates", "cr", "0.1.0", 0);
    ins.run("o/b", "package.json", "npm", "pub", "1.0.0", 0);
    ins.run("o/b", "go.mod", "go", "example.test/b", null, 0);
    expect(manifestCandidates(db).map(c => `${c.ecosystem}/${c.name}@${c.repo}`)).toEqual(["crates/cr@o/a", "npm/pub@o/a"]);
  });

  test("syncPackages stores versions, marks foreign names and missing ones, and builds Zig versions from tags", async () => {
    const db = openDb(":memory:");
    const m = db.query("INSERT INTO manifests (repo, path, ecosystem, name, version, private) VALUES (?, ?, ?, ?, ?, 0)");
    m.run("o/mine", "package.json", "npm", "mine", "1.0.0");
    m.run("o/mine", "sub/package.json", "npm", "common-name", "1.0.0");
    m.run("o/mine", "gone/package.json", "npm", "gone", "1.0.0");
    m.run("o/zig", "build.zig.zon", "zig", "zigpkg", "0.3.0");
    db.query("INSERT INTO refs (repo, name, kind, sha, date) VALUES (?, ?, 'tag', 'abc', ?)").run("o/zig", "v0.3.0", "2026-02-01T00:00:00Z");
    db.query("INSERT INTO refs (repo, name, kind, sha, date) VALUES (?, ?, 'tag', 'abc', ?)").run("o/zig", "nightly", "2026-02-02T00:00:00Z");
    const mine = { name: "mine", "dist-tags": { latest: "1.0.0" }, repository: { url: "git+https://github.com/o/mine.git" }, time: { created: "2026-01-01T00:00:00Z", "1.0.0": "2026-01-02T00:00:00Z" }, versions: { "1.0.0": {} } };
    const other = { name: "common-name", "dist-tags": { latest: "9.9.9" }, repository: { url: "https://github.com/stranger/common-name" }, time: { "9.9.9": "2020-01-01T00:00:00Z" }, versions: { "9.9.9": {} } };
    const fetch = async (url: string) => {
      if (url.includes("-/v1/search")) return reply({ total: 0, objects: [] });
      if (url.includes("api.npmjs.org")) return reply({ downloads: 5 });
      if (url.endsWith("/mine")) return reply(mine);
      if (url.endsWith("/common-name")) return reply(other);
      return new Response("{}", { status: 404 });
    };
    const tally = await syncPackages(db, "o", { fetch, sleep: noSleep });
    expect(tally).toEqual({ ok: 2, missing: 1, foreign: 1, error: 0 });
    expect(db.query("SELECT name, status, latest, downloads_recent, versions_count FROM packages ORDER BY name").all()).toEqual([
      { name: "common-name", status: "foreign", latest: "9.9.9", downloads_recent: 5, versions_count: 1 },
      { name: "gone", status: "missing", latest: null, downloads_recent: null, versions_count: 0 },
      { name: "mine", status: "ok", latest: "1.0.0", downloads_recent: 5, versions_count: 1 },
      { name: "zigpkg", status: "git", latest: "0.3.0", downloads_recent: null, versions_count: 1 },
    ]);
    expect(db.query("SELECT count(*) AS n FROM package_versions WHERE name = 'common-name'").get()).toEqual({ n: 0 });
    expect(db.query("SELECT version, published_at FROM package_versions WHERE name = 'zigpkg'").all()).toEqual([{ version: "0.3.0", published_at: "2026-02-01T00:00:00Z" }]);
  });
});

test("an entry without a recorded repository belongs only when published after the repo was created", () => {
  const info = (published: string) => ({ repository: null, versions: [{ version: "1.0.0", publishedAt: published, downloads: null, yanked: false }] }) as any;
  expect(belongs(info("2019-07-05T00:00:00Z"), "o/r", "2024-01-01T00:00:00Z")).toBe(false);
  expect(belongs(info("2025-07-05T00:00:00Z"), "o/r", "2024-01-01T00:00:00Z")).toBe(true);
  expect(belongs({ repository: null, versions: [] } as any, "o/r", "2024-01-01T00:00:00Z")).toBe(true);
});

test("packages named by hand are fetched without a manifest and without the repo check", async () => {
  const db = openDb(":memory:");
  const doc = { name: "cli-ai", "dist-tags": { latest: "2.0.0" }, repository: { url: "https://github.com/someone/else" }, time: { "2.0.0": "2026-01-01T00:00:00Z" }, versions: { "2.0.0": {} } };
  const fetch = async (url: string) => (url.includes("-/v1/search") ? reply({ total: 0, objects: [] }) : url.includes("api.npmjs.org") ? reply({ downloads: 3 }) : reply(doc));
  const tally = await syncPackages(db, "o", { fetch, sleep: noSleep, named: ["npm:cli-ai"] });
  expect(tally.ok).toBe(1);
  expect(db.query("SELECT name, found_by, status, downloads_recent FROM packages").all()).toEqual([{ name: "cli-ai", found_by: "named", status: "ok", downloads_recent: 3 }]);
  await expect(syncPackages(db, "o", { fetch, sleep: noSleep, named: ["nuget:x"] })).rejects.toThrow("--package expects");
});
