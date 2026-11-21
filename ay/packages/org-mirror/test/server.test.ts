import { beforeAll, describe, expect, test } from "bun:test";
import type { Database } from "bun:sqlite";
import { createApp } from "../src/server.ts";
import { EMAILS, ORG, XSS_TITLE, seed, sha } from "./seed.ts";

let db: Database;
let app: ReturnType<typeof createApp>;
beforeAll(() => {
  db = seed();
  app = createApp({ db, org: ORG });
});

const get = async (path: string, method = "GET") => app.fetch(new Request(`http://mirror.test${path}`, { method }));
const page = async (path: string) => {
  const res = await get(path);
  expect(res.status).toBe(200);
  expect(res.headers.get("content-type")).toContain("text/html");
  return res.text();
};
// Test helper: bodies are arbitrary JSON, so the result is deliberately untyped.
const api = async (path: string): Promise<any> => {
  const res = await get(path);
  expect(res.status).toBe(200);
  expect(res.headers.get("content-type")).toContain("application/json");
  return res.json();
};

describe("overview", () => {
  test("HTML shows totals, ecosystem chips and repos by last push", async () => {
    const body = await page("/");
    expect(body).toContain("<title>acme - acme mirror</title>");
    expect(body).toContain("/assets/m3-tokens.css");
    for (const name of ["forge", "zigpkg", "pytools"]) expect(body).toContain(`href="/repos/${name}"`);
    expect(body.indexOf("/repos/forge")).toBeLessThan(body.indexOf("/repos/pytools"));
    expect(body.indexOf("/repos/pytools")).toBeLessThan(body.indexOf("/repos/zigpkg"));
    expect(body).toContain('href="/repos?tag=rust"');
    expect(body).toContain("1,200");
    expect(body).toContain("archived");
    expect(body).toContain("A build tool for &lt;everything&gt;");
  });

  test("JSON twin has totals, tags and a paged repo list", async () => {
    const d = await api("/api/overview");
    expect(d.org).toBe(ORG);
    expect(d.totals).toMatchObject({ repos: 3, commits: 63, issues: 3, pulls: 3, comments: 3, releases: 3, packages: 4 });
    expect(d.tags.map((t: { tag: string }) => t.tag).sort()).toEqual(["bun", "python", "rust", "zig"]);
    expect(d).toMatchObject({ total: 3, page: 1, per: 50 });
    expect(d.items.map((r: { name: string }) => r.name)).toEqual(["forge", "pytools", "zigpkg"]);
    expect(d.items[0].tags).toEqual(["bun", "rust"]);
  });
});

describe("repos", () => {
  test("list filters by tag and query", async () => {
    expect((await api("/api/repos?tag=zig")).items.map((r: { name: string }) => r.name)).toEqual(["zigpkg"]);
    expect((await api("/api/repos?q=python")).total).toBe(1);
    expect((await api("/api/repos?q=%25")).total).toBe(0);
    expect(await page("/repos?tag=zig")).toContain("Clear the zig filter");
  });

  test("detail shows languages, topics, ref counts and stored vs GitHub", async () => {
    const body = await page("/repos/forge");
    expect(body).toContain('class="langbar"');
    expect(body).toContain("TypeScript");
    expect(body).toContain("80.0%");
    expect(body).toContain("build-tool");
    expect(body).toContain("Forge 1.1");
    expect(body).toContain('href="/repos/forge/pulls"');
    expect(body).toContain("complete");
    expect(body).toContain("-2");
    const d = await api("/api/repos/forge");
    expect(d.refs).toEqual({ branches: 2, tags: 2 });
    expect(d.counts.issues).toEqual({ stored: 3, github: 5 });
    expect(d.counts.pulls).toEqual({ stored: 3, github: 3 });
    expect(d.counts.releases).toEqual({ stored: 3, github: 3 });
    expect(d.counts.commits).toEqual({ git: 120, github: 123, stored: 63 });
    expect(d.languages.map((l: { language: string }) => l.language)).toEqual(["Rust", "TypeScript", "Shell"]);
    expect(d.releases[0].tag).toBe("v1.1.0");
  });
});

describe("commits", () => {
  test("list is newest first, paged and searchable", async () => {
    const d = await api("/api/repos/forge/commits");
    expect(d).toMatchObject({ total: 63, page: 1, per: 50 });
    expect(d.items).toHaveLength(50);
    expect(d.items[0].subject).toBe("chore: bump dependency 63");
    const q = await api("/api/repos/forge/commits?q=lexer");
    expect(q.items.map((c: { subject: string }) => c.subject)).toEqual(["Fix the lexer"]);
    expect((await api("/api/repos/forge/commits?q=lee")).total).toBe(1);
    const last = await api("/api/repos/forge/commits?page=2");
    expect(last.items).toHaveLength(13);
    const html = await page("/repos/forge/commits?page=2");
    expect(html).toContain("Page 2 of 2");
    expect(html).toContain('href="/repos/forge/commits"');
  });

  test("detail has the full body, linked parents and stats", async () => {
    const full = sha(3);
    const html = await page(`/repos/forge/commits/${full}`);
    expect(html).toContain("Add the cache layer");
    expect(html).toContain("Speeds up rebuilds by 3x.");
    expect(html).toContain(`href="/repos/forge/commits/${sha(2)}"`);
    expect(html).toContain("7 files, +300 -40");
    const d = await api(`/api/repos/forge/commits/${full.slice(0, 10)}`);
    expect(d.parents).toEqual([sha(2)]);
    expect(d.commit.author_name).toBe("Dana Dev");
    expect(d.commit.body).toContain("Co-authored-by: Casey Co");
  });

  test("no response contains an author email", async () => {
    const paths = ["/", "/api/overview", "/repos/forge/commits", "/api/repos/forge/commits", `/repos/forge/commits/${sha(3)}`, `/api/repos/forge/commits/${sha(3)}`, `/api/repos/forge/commits/${sha(1)}`, "/repos/forge/pulls/10", "/api/repos/forge/pulls/10", "/api/repos/forge", "/api/stats"];
    for (const path of paths) {
      const body = await (await get(path)).text();
      for (const email of EMAILS) expect(body).not.toContain(email);
      expect(body).not.toMatch(/[\w.+-]+@[\w-]+\.[a-z]{2,}/i);
      expect(body).not.toContain("author_email");
      expect(body).not.toContain("committer_email");
    }
  });

  test("unknown sha is a 404", async () => {
    expect((await get("/repos/forge/commits/ffffffff")).status).toBe(404);
    expect((await get("/api/repos/forge/commits/not-hex")).status).toBe(404);
  });
});

describe("pulls and issues", () => {
  test("issues default to open, with label chips and comment counts", async () => {
    const d = await api("/api/repos/forge/issues");
    expect(d.items.map((i: { number: number }) => i.number)).toEqual([1, 2]);
    expect(d.items[0]).toMatchObject({ labels: ["bug"], comments_count: 2, state: "open" });
    const html = await page("/repos/forge/issues");
    expect(html).toContain('class="chip label" href="/repos/forge/issues?label=bug"');
    expect(html).toContain("2 comments");
    expect(html).not.toContain("Old regression");
  });

  test("state, label and query filters", async () => {
    expect((await api("/api/repos/forge/issues?state=all")).total).toBe(3);
    expect((await api("/api/repos/forge/issues?state=closed")).items.map((i: { number: number }) => i.number)).toEqual([3]);
    expect((await api("/api/repos/forge/issues?state=all&label=enhancement")).items.map((i: { number: number }) => i.number)).toEqual([2]);
    expect((await api("/api/repos/forge/issues?q=dark")).total).toBe(1);
    expect((await api("/api/repos/forge/pulls?state=merged")).items.map((i: { number: number }) => i.number)).toEqual([10]);
    expect((await api("/api/repos/forge/pulls?state=open")).items[0]).toMatchObject({ number: 11, is_draft: 1 });
    expect((await api("/api/repos/forge/pulls?state=closed")).items.map((i: { number: number }) => i.number)).toEqual([12]);
    expect((await api("/api/repos/forge/pulls?state=all")).total).toBe(3);
    expect(await page("/repos/forge/pulls?state=all&label=enhancement")).toContain("clear");
  });

  test("a script tag in a title is escaped everywhere", async () => {
    for (const path of ["/repos/forge/issues", "/repos/forge/issues/1"]) {
      const html = await page(path);
      expect(html).not.toContain("<script>alert");
      expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt; broken parser");
    }
    expect((await api("/api/repos/forge/issues/1")).item.title).toBe(XSS_TITLE);
  });

  test("pull request detail has facts, comments, reviews and timeline", async () => {
    const html = await page("/repos/forge/pulls/10");
    expect(html).toContain("merged");
    expect(html).toContain("by lee on Nov 10, 2026");
    expect(html).toContain("300");
    expect(html).toContain("APPROVED");
    expect(html).toContain("Looks great, merging after CI.");
    expect(html).toContain("Ship it.");
    expect(html).toContain("label: enhancement");
    expect(html).toContain("committer: ");
    const d = await api("/api/repos/forge/pulls/10");
    expect(d).toMatchObject({ kind: "pr", state: "merged", labels: ["enhancement"] });
    expect(d.item).toMatchObject({ base_ref: "main", head_ref: "cache", changed_files: 7, review_decision: "APPROVED" });
    expect(d.events.map((e: { type: string }) => e.type)).toEqual(["labeled", "merged"]);
    expect(d.events[1].data.committer).toEqual({ name: "Lee" });
    expect(d.reviews).toHaveLength(1);
  });

  test("issue detail renders the body pre-wrapped, the milestone and assignees", async () => {
    const html = await page("/repos/forge/issues/1");
    expect(html).toContain('<pre class="body">Parser crashes on &lt;b&gt;bold&lt;/b&gt; input.\nSecond line.</pre>');
    expect(html).toContain("v1.1 (open)");
    expect(html).toContain("Reproduced on Windows.");
    expect(html).toContain("assigned");
    expect((await get("/repos/forge/issues/999")).status).toBe(404);
    expect((await get("/repos/forge/issues/abc")).status).toBe(404);
  });
});

describe("releases and changelog", () => {
  test("release list and detail with assets", async () => {
    const d = await api("/api/repos/forge/releases");
    expect(d.items.map((r: { tag: string }) => r.tag)).toEqual(["v1.1.0", "v1.1.0-rc.1", "v1.0.0"]);
    expect(d.items[0]).toMatchObject({ assets: 2, downloads: 444, is_latest: 1 });
    const html = await page("/repos/forge/releases/v1.1.0");
    expect(html).toContain("forge-linux-x64.tar.gz");
    expect(html).toContain("forge-windows-x64.zip");
    expect(html).toContain("5.0 MiB");
    expect(html).toContain("321");
    expect(html).toContain("&lt;fast&gt; rebuilds");
    expect(html).toContain('class="body"');
    expect((await api("/api/repos/forge/releases/v1.1.0")).assets).toHaveLength(2);
    expect((await get("/repos/forge/releases/v9")).status).toBe(404);
  });

  test("changelog lists files, snapshot counts and versions newest first", async () => {
    const d = await api("/api/repos/forge/changelog");
    expect(d.files).toEqual([{ path: "CHANGELOG.md", entries: 2, snapshots: 2, latest: "1.1.0" }]);
    expect(d.items.map((e: { version: string }) => e.version)).toEqual(["1.1.0", "1.0.0"]);
    const html = await page("/repos/forge/changelog");
    expect(html).toContain("CHANGELOG.md");
    expect(html).toContain("Added the cache layer");
    expect(html.indexOf("1.1.0")).toBeLessThan(html.indexOf("1.0.0"));
  });
});

describe("packages", () => {
  test("table grouped by ecosystem with recent downloads and window", async () => {
    const html = await page("/packages");
    for (const eco of ["crates", "npm", "pypi", "zig"]) expect(html).toContain(`<h2>${eco}</h2>`);
    expect(html).toContain('href="/packages/npm/%40acme%2Fforge"');
    expect(html).toContain("5,400 / 30d");
    expect(html).toContain("missing");
    const d = await api("/api/packages?ecosystem=npm");
    expect(d.total).toBe(1);
    expect(d.ecosystems).toHaveLength(4);
    expect(d.items[0]).toMatchObject({ name: "@acme/forge", downloads_recent: 5400, downloads_window: "30d" });
  });

  test("scoped package detail with versions", async () => {
    const html = await page("/packages/npm/%40acme%2Fforge");
    expect(html).toContain("@acme/forge");
    expect(html).toContain("Oct 2, 2026");
    expect(html.indexOf("1.1.0</code>")).toBeLessThan(html.indexOf("1.0.0</code>"));
    const d = await api("/api/packages/npm/%40acme%2Fforge");
    expect(d.package.latest).toBe("1.1.0");
    expect(d.items.map((v: { version: string }) => v.version)).toEqual(["1.1.0", "1.0.0"]);
    expect((await get("/packages/npm/nope")).status).toBe(404);
  });
});

describe("api, pagination and errors", () => {
  test("index lists every JSON endpoint and stats counts every table", async () => {
    const html = await page("/api");
    for (const p of ["/api/overview", "/api/repos/:name/commits/:sha", "/api/packages/:ecosystem/:name", "/api/stats"]) expect(html).toContain(p);
    const s = await api("/api/stats");
    expect(s.tables).toMatchObject({ repos: 3, commits: 63, issues: 3, pulls: 3, releases: 3, release_assets: 2, packages: 4, package_versions: 3, changelog_entries: 2 });
  });

  test("per is capped at 100 and pages carry total", async () => {
    expect((await api("/api/repos/forge/commits?per=1000")).per).toBe(100);
    const p2 = await api("/api/repos/forge/commits?per=10&page=3");
    expect(p2).toMatchObject({ total: 63, page: 3, per: 10 });
    expect(p2.items).toHaveLength(10);
    expect(Object.keys(p2).sort()).toEqual(["items", "page", "per", "total"]);
    expect((await api("/api/repos/forge/commits?per=-4&page=zz")).per).toBe(50);
  });

  test("unknown repo and page return a styled 404, JSON 404 under /api", async () => {
    for (const path of ["/repos/ghost", "/repos/forge/nothing", "/nope", "/repos/ghost/commits"]) {
      const res = await get(path);
      expect(res.status).toBe(404);
      expect(res.headers.get("content-type")).toContain("text/html");
      const body = await res.text();
      expect(body).toContain("Not found");
      expect(body).toContain('class="bar"');
    }
    const res = await get("/api/repos/ghost");
    expect(res.status).toBe(404);
    expect(await res.json()).toEqual({ error: "not found: /api/repos/ghost" });
    expect((await get("/repos/%E0%A4%A")).status).toBe(404);
  });

  test("HEAD returns headers without a body, other methods are refused", async () => {
    const head = await get("/repos/forge", "HEAD");
    expect(head.status).toBe(200);
    expect(Number(head.headers.get("content-length"))).toBeGreaterThan(1000);
    expect(await head.text()).toBe("");
    expect((await get("/repos/ghost", "HEAD")).status).toBe(404);
    expect((await get("/repos/forge", "POST")).status).toBe(405);
  });

  test("assets come from the brand folder", async () => {
    const css = await get("/assets/m3-tokens.css");
    expect(css.headers.get("content-type")).toContain("text/css");
    expect(await css.text()).toContain("--md-sys-color-primary");
    const icon = await get("/assets/icon.svg");
    expect(icon.headers.get("content-type")).toBe("image/svg+xml");
    expect(await icon.text()).toContain("<svg");
  });
});
