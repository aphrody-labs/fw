/** The read API and the minimal pages over a replica: every page has a JSON twin under /api serving the same data. */
import type { Database } from "bun:sqlite";
import { join } from "node:path";
import { TABLES, type Row } from "./db.ts";
import { layout, type Page } from "./html.ts";
import * as v from "./views.ts";

export interface AppOptions {
  db: Database;
  org: string;
  /** The repository root, where `brand/` lives. */
  root?: string;
}

type Bind = string | number | null;
interface Ctx extends v.View {
  db: Database;
  org: string;
  page: number;
  per: number;
}
interface Hit {
  json: unknown;
  page(): Page;
}
interface Route {
  path: string;
  doc: string;
  parts: string[];
  run(c: Ctx): Hit | undefined;
}

const all = <T>(db: Database, sql: string, ...p: Bind[]) => db.query(sql).all(...p) as T[];
const one = <T>(db: Database, sql: string, ...p: Bind[]) => db.query(sql).get(...p) as T | null;
const count = (db: Database, sql: string, ...p: Bind[]) => one<{ n: number }>(db, sql, ...p)?.n ?? 0;
const like = (s: string) => `%${s.replace(/[\\%_]/g, "\\$&")}%`;
const split = (s: string | null | undefined, sep: string | RegExp = /[\s,]+/) => (s ?? "").split(sep).filter(Boolean);
const num = (s: string | null, d: number) => {
  const n = Number.parseInt(s ?? "", 10);
  return n > 0 ? n : d;
};

/** Author emails never leave the database: bare and bracketed addresses are dropped from free text. */
const EMAIL = /\s*<?[\w.+-]+@[\w-]+(?:\.[\w-]+)*\.[A-Za-z]{2,}>?/g;
const scrub = (s: string | null) => (s == null ? s : s.replace(EMAIL, ""));
function clean(x: unknown): unknown {
  if (typeof x === "string") return scrub(x);
  if (Array.isArray(x)) return x.map(clean);
  if (x && typeof x === "object") return Object.fromEntries(Object.entries(x).filter(([k]) => !/e-?mail/i.test(k)).map(([k, y]) => [k, clean(y)]));
  return x;
}
function parseData(text: string | null): unknown {
  if (text == null) return null;
  try {
    return clean(JSON.parse(text));
  } catch {
    return scrub(text);
  }
}

/** A WHERE clause assembled from fixed fragments; every value is bound, never interpolated. */
function cond() {
  const parts: string[] = [];
  const args: Bind[] = [];
  return {
    args,
    add(sql: string, ...a: Bind[]) {
      parts.push(sql);
      args.push(...a);
    },
    sql: () => parts.join(" AND ") || "1",
  };
}
type Cond = ReturnType<typeof cond>;

/** One page of `from`; select, from and order are code constants, the filters in `w` are bound. */
function list<T>(c: Ctx, select: string, from: string, w: Cond, order: string): v.Paged<T> {
  const total = count(c.db, `SELECT count(*) AS n FROM ${from} WHERE ${w.sql()}`, ...w.args);
  const items = all<T>(c.db, `SELECT ${select} FROM ${from} WHERE ${w.sql()} ORDER BY ${order} LIMIT ? OFFSET ?`, ...w.args, c.per, (c.page - 1) * c.per);
  return { total, page: c.page, per: c.per, items };
}

const param = (c: Ctx, k: string) => c.url.searchParams.get(k)?.trim() || null;

function repoItems(c: Ctx): v.Paged<v.RepoItem> {
  const w = cond();
  w.add("r.org = ?", c.org);
  const q = param(c, "q");
  if (q) w.add("(r.name LIKE ? ESCAPE '\\' OR r.description LIKE ? ESCAPE '\\')", like(q), like(q));
  const tag = param(c, "tag");
  if (tag) w.add("EXISTS (SELECT 1 FROM repo_tags t WHERE t.repo = r.full_name AND t.tag = ?)", tag);
  const p = list<Omit<v.RepoItem, "tags"> & { tags: string | null }>(
    c,
    "r.full_name, r.name, r.description, r.stars, r.primary_language, r.pushed_at, r.open_issues, r.open_prs, r.is_fork, r.is_archived, (SELECT group_concat(t.tag, char(31)) FROM repo_tags t WHERE t.repo = r.full_name) AS tags",
    "repos r",
    w,
    "r.pushed_at DESC, r.name",
  );
  return { ...p, items: p.items.map(r => ({ ...r, tags: split(r.tags, "\u001f").sort() })) };
}

const OVERVIEW_TABLES = ["repos", "commits", "issues", "pulls", "comments", "releases", "packages"] as const;

function overview(c: Ctx): v.Overview {
  const totals = Object.fromEntries(OVERVIEW_TABLES.map(t => [t, count(c.db, `SELECT count(*) AS n FROM ${t}`)]));
  const tags = all<{ tag: string; n: number }>(c.db, "SELECT tag, count(*) AS n FROM repo_tags WHERE repo IN (SELECT full_name FROM repos WHERE org = ?) GROUP BY tag ORDER BY n DESC, tag", c.org);
  return { org: c.org, totals, tags, ...repoItems(c) };
}

function repoDetail(c: Ctx, full: string): v.RepoDetail {
  const { db } = c;
  const repo = one<Row>(db, "SELECT * FROM repos WHERE full_name = ?", full)!;
  const refs = Object.fromEntries(all<{ kind: string; n: number }>(db, "SELECT kind, count(*) AS n FROM refs WHERE repo = ? GROUP BY kind", full).map(r => [r.kind, r.n]));
  const stored = (table: string) => count(db, `SELECT count(*) AS n FROM ${table} WHERE repo = ?`, full);
  const github = (k: string) => (repo[k] == null ? null : Number(repo[k]));
  return {
    repo,
    languages: all<Row>(db, "SELECT language, bytes, share FROM repo_languages WHERE repo = ? ORDER BY share DESC, language", full),
    topics: all<{ topic: string }>(db, "SELECT topic FROM repo_topics WHERE repo = ? ORDER BY topic", full).map(t => t.topic),
    tags: all<Row>(db, "SELECT tag, via, share FROM repo_tags WHERE repo = ? ORDER BY tag", full),
    refs: { branches: refs.branch ?? 0, tags: refs.tag ?? 0 },
    releases: all<Row>(db, "SELECT tag, name, is_prerelease, published_at FROM releases WHERE repo = ? ORDER BY published_at DESC, tag DESC LIMIT 5", full),
    counts: {
      issues: { stored: stored("issues"), github: github("issues_total") },
      pulls: { stored: stored("pulls"), github: github("prs_total") },
      releases: { stored: stored("releases"), github: github("releases_total") },
      commits: { git: github("default_commits_git"), github: github("default_commits_github"), stored: stored("commits") },
    },
  };
}

function commitList(c: Ctx, full: string): v.Paged<v.CommitItem> {
  const w = cond();
  w.add("repo = ?", full);
  const q = param(c, "q");
  if (q) w.add("(subject LIKE ? ESCAPE '\\' OR author_name LIKE ? ESCAPE '\\')", like(q), like(q));
  return list(c, "sha, subject, author_name, author_date, committer_date, files, additions, deletions", "commits", w, "committer_date DESC, sha");
}

function commitDetail(c: Ctx, full: string): v.CommitDetail | undefined {
  const sha = c.params.sha!.toLowerCase();
  if (!/^[0-9a-f]{4,64}$/.test(sha)) return;
  const rows = all<Row>(c.db, "SELECT sha, parents, author_name, author_date, committer_name, committer_date, subject, body, files, additions, deletions FROM commits WHERE repo = ? AND sha LIKE ? ORDER BY sha = ? DESC LIMIT 2", full, `${sha}%`, sha);
  const row = rows.length === 1 || rows[0]?.sha === sha ? rows[0] : undefined;
  if (!row) return;
  return { commit: { ...row, body: scrub(row.body as string | null) }, parents: split(row.parents as string) };
}

const KIND = { issue: { table: "issues", state: "lower(t.state)" }, pr: { table: "pulls", state: "CASE WHEN t.merged = 1 THEN 'merged' ELSE lower(t.state) END" } } as const;

function threadList(c: Ctx, full: string, kind: "issue" | "pr"): v.Paged<v.ThreadItem> {
  const { table, state: stateSql } = KIND[kind];
  const w = cond();
  w.add("t.repo = ?", full);
  const asked = param(c, "state");
  const state = v.STATES.includes(asked ?? "") ? asked! : "open";
  if (state !== "all") w.add(`${stateSql} = ?`, state);
  const label = param(c, "label");
  if (label) w.add("EXISTS (SELECT 1 FROM item_labels l WHERE l.repo = t.repo AND l.kind = ? AND l.number = t.number AND l.label = ?)", kind, label);
  const q = param(c, "q");
  if (q) w.add("t.title LIKE ? ESCAPE '\\'", like(q));
  const draft = kind === "pr" ? ", t.is_draft" : "";
  const p = list<Omit<v.ThreadItem, "labels"> & { labels: string | null }>(
    c,
    `t.number, t.title, ${stateSql} AS state, t.author, t.created_at, t.updated_at, t.comments_count${draft}, (SELECT group_concat(l.label, char(31)) FROM item_labels l WHERE l.repo = t.repo AND l.kind = '${kind}' AND l.number = t.number) AS labels`,
    `${table} t`,
    w,
    "t.updated_at DESC, t.number DESC",
  );
  return { ...p, items: p.items.map(t => ({ ...t, labels: split(t.labels, "\u001f").sort() })) };
}

function threadDetail(c: Ctx, full: string, kind: "issue" | "pr"): v.ThreadDetail | undefined {
  const { db } = c;
  const n = Number(c.params.n);
  if (!Number.isInteger(n)) return;
  const item = one<Row>(db, `SELECT t.*, ${KIND[kind].state} AS effective_state FROM ${KIND[kind].table} t WHERE t.repo = ? AND t.number = ?`, full, n);
  if (!item) return;
  const { effective_state: state, ...rest } = item;
  const thread = [full, kind, n] as const;
  return {
    kind,
    state: String(state),
    item: rest,
    labels: all<{ label: string }>(db, "SELECT label FROM item_labels WHERE repo = ? AND kind = ? AND number = ? ORDER BY label", ...thread).map(l => l.label),
    milestone: rest.milestone == null ? null : one<Row>(db, "SELECT number, title, state, due_on FROM milestones WHERE repo = ? AND number = ?", full, rest.milestone),
    comments: all<Row>(db, "SELECT id, author, body, created_at, updated_at, url FROM comments WHERE repo = ? AND kind = ? AND number = ? ORDER BY created_at, id", ...thread),
    reviews: kind === "pr" ? all<Row>(db, "SELECT id, author, state, body, submitted_at FROM reviews WHERE repo = ? AND number = ? ORDER BY submitted_at, id", full, n) : [],
    events: all<{ id: string; type: string; actor: string | null; created_at: string | null; data: string | null }>(db, "SELECT id, type, actor, created_at, data FROM events WHERE repo = ? AND kind = ? AND number = ? ORDER BY created_at, id", ...thread).map(e => ({ ...e, data: parseData(e.data) })),
  };
}

function releaseList(c: Ctx, full: string): v.Paged<v.ReleaseItem> {
  const w = cond();
  w.add("r.repo = ?", full);
  return list(
    c,
    "r.tag, r.name, r.is_prerelease, r.is_draft, r.is_latest, r.published_at, r.author, (SELECT count(*) FROM release_assets a WHERE a.repo = r.repo AND a.tag = r.tag) AS assets, (SELECT coalesce(sum(a.downloads), 0) FROM release_assets a WHERE a.repo = r.repo AND a.tag = r.tag) AS downloads",
    "releases r",
    w,
    "r.published_at DESC, r.tag DESC",
  );
}

function releaseDetail(c: Ctx, full: string): v.ReleaseDetail | undefined {
  const release = one<Row>(c.db, "SELECT * FROM releases WHERE repo = ? AND tag = ?", full, c.params.tag!);
  if (!release) return;
  return { release, assets: all<Row>(c.db, "SELECT name, size, downloads, content_type, created_at, url FROM release_assets WHERE repo = ? AND tag = ? ORDER BY name", full, c.params.tag!) };
}

function changelog(c: Ctx, full: string): v.ChangelogPage {
  const files = all<v.ChangelogPage["files"][number]>(
    c.db,
    `SELECT p.path,
  (SELECT count(*) FROM changelog_entries e WHERE e.repo = ?1 AND e.path = p.path) AS entries,
  (SELECT count(*) FROM changelog_files s WHERE s.repo = ?1 AND s.path = p.path AND s.ref LIKE 'tag:%') AS snapshots,
  (SELECT e.version FROM changelog_entries e WHERE e.repo = ?1 AND e.path = p.path ORDER BY e.date IS NULL, e.date DESC, e.position LIMIT 1) AS latest
FROM (SELECT DISTINCT path FROM changelog_files WHERE repo = ?1) p ORDER BY p.path`,
    full,
  );
  const w = cond();
  w.add("repo = ?", full);
  const path = param(c, "path");
  if (path) w.add("path = ?", path);
  return { files, ...list<Row>(c, "path, version, date, body, source_ref, position", "changelog_entries", w, "path, date IS NULL, date DESC, position") };
}

function packageList(c: Ctx): v.PackagesPage {
  const w = cond();
  const eco = param(c, "ecosystem");
  if (eco) w.add("ecosystem = ?", eco);
  const q = param(c, "q");
  if (q) w.add("name LIKE ? ESCAPE '\\'", like(q));
  return {
    ecosystems: all<{ ecosystem: string; n: number }>(c.db, "SELECT ecosystem, count(*) AS n FROM packages GROUP BY ecosystem ORDER BY ecosystem"),
    ...list<Row>(c, "ecosystem, name, repo, status, latest, last_published, versions_count, downloads_recent, downloads_window", "packages", w, "ecosystem, status NOT IN ('ok', 'git'), downloads_recent DESC, name"),
  };
}

function packageDetail(c: Ctx): v.PackageDetail | undefined {
  const { ecosystem, name } = c.params;
  const pkg = one<Row>(c.db, "SELECT * FROM packages WHERE ecosystem = ? AND name = ?", ecosystem!, name!);
  if (!pkg) return;
  const w = cond();
  w.add("ecosystem = ? AND name = ?", ecosystem!, name!);
  return { package: pkg, ...list<Row>(c, "version, published_at, downloads, yanked", "package_versions", w, "published_at DESC, version DESC") };
}

function route<T>(path: string, doc: string, data: (c: Ctx) => T | undefined, view: (d: T, c: Ctx) => Page): Route {
  return {
    path,
    doc,
    parts: path.split("/").filter(Boolean),
    run(c) {
      const d = data(c);
      return d === undefined ? undefined : { json: d, page: () => view(d, c) };
    },
  };
}

/** A route under /repos/:name that answers only when the repository exists; `f` receives its full name. */
const inRepo =
  <T>(f: (c: Ctx, full: string) => T | undefined) =>
  (c: Ctx) => {
    const full = `${c.org}/${c.params.name}`;
    return one(c.db, "SELECT 1 FROM repos WHERE full_name = ?", full) ? f(c, full) : undefined;
  };

const ROUTES: Route[] = [
  route("/", "Organization overview: totals, ecosystem tags and repositories by last push (?tag, ?q).", overview, v.overview),
  route("/repos", "Repositories by last push (?tag, ?q on name and description).", repoItems, v.repos),
  route("/repos/:name", "One repository: metadata, languages, topics, tags, ref counts, stored vs GitHub counts.", inRepo(repoDetail), v.repo),
  route("/repos/:name/commits", "Commits, newest first (?q on subject and author name).", inRepo(commitList), v.commits),
  route("/repos/:name/commits/:sha", "One commit with its full message, parents and stats (a unique sha prefix works).", inRepo(commitDetail), v.commit),
  route("/repos/:name/pulls", "Pull requests, newest update first (?state=open|closed|merged|all, ?label, ?q on title).", inRepo((c, f) => threadList(c, f, "pr")), (d, c) => v.threads(d, c, "pr")),
  route("/repos/:name/pulls/:n", "One pull request with comments, reviews and timeline events.", inRepo((c, f) => threadDetail(c, f, "pr")), v.thread),
  route("/repos/:name/issues", "Issues, newest update first (?state=open|closed|all, ?label, ?q on title).", inRepo((c, f) => threadList(c, f, "issue")), (d, c) => v.threads(d, c, "issue")),
  route("/repos/:name/issues/:n", "One issue with comments and timeline events.", inRepo((c, f) => threadDetail(c, f, "issue")), v.thread),
  route("/repos/:name/releases", "Releases, newest first.", inRepo(releaseList), v.releases),
  route("/repos/:name/releases/:tag", "One release with its notes and assets.", inRepo(releaseDetail), v.release),
  route("/repos/:name/changelog", "Changelog files and their parsed versions, newest first (?path).", inRepo(changelog), v.changelog),
  route("/packages", "Packages by ecosystem: npm, crates, pypi, zig (?ecosystem, ?q on name).", packageList, v.packages),
  route("/packages/:ecosystem/:name", "One package with its versions; URL-encode scoped names such as %40scope%2Fname.", packageDetail, v.packageDetail),
];

const apiOf = (path: string) => (path === "/" ? "/api/overview" : `/api${path}`);

function matchRoute(segs: string[]): { route: Route; params: Record<string, string> } | undefined {
  for (const route of ROUTES) {
    if (route.parts.length !== segs.length) continue;
    const params: Record<string, string> = {};
    if (route.parts.every((p, i) => (p.startsWith(":") ? ((params[p.slice(1)] = segs[i]!), true) : p === segs[i]))) return { route, params };
  }
}

const ASSETS: Record<string, [string, string]> = {
  "/assets/m3-tokens.css": ["brand/tokens/m3-tokens.css", "text/css; charset=utf-8"],
  "/assets/icon.svg": ["brand/assets/icon.svg", "image/svg+xml"],
};

export function createApp({ db, org, root = join(import.meta.dir, "../../..") }: AppOptions): { fetch(req: Request): Response | Promise<Response> } {
  const jsonRes = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json; charset=utf-8" } });
  const pageRes = (p: Page, status = 200) => new Response(layout(p, org), { status, headers: { "content-type": "text/html; charset=utf-8" } });
  const endpoints: v.Endpoint[] = [...ROUTES.map(r => ({ api: apiOf(r.path), page: r.path, doc: r.doc })), { api: "/api/stats", page: "/api/stats", doc: "Row count of every table." }];

  async function respond(req: Request): Promise<Response> {
    const url = new URL(req.url);
    const asset = ASSETS[url.pathname];
    if (asset) return new Response(Bun.file(join(root, asset[0])), { headers: { "content-type": asset[1], "cache-control": "public, max-age=3600" } });

    let segs: string[];
    try {
      segs = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);
    } catch {
      return url.pathname.startsWith("/api/") ? jsonRes({ error: "malformed path" }, 400) : pageRes(v.notFound(url.pathname, url.pathname), 404);
    }
    const isApi = segs[0] === "api";
    if (isApi && segs.length === 1) return pageRes(v.apiIndex(endpoints, { url, params: {} }));
    if (isApi && segs.length === 2 && segs[1] === "stats") return jsonRes({ org, tables: Object.fromEntries(TABLES.map(t => [t, count(db, `SELECT count(*) AS n FROM ${t}`)])) });
    const rest = isApi ? (segs[1] === "overview" && segs.length === 2 ? [] : segs.slice(1)) : segs;

    const m = matchRoute(rest);
    const per = Math.min(num(url.searchParams.get("per"), 50), 100);
    const page = Math.min(num(url.searchParams.get("page"), 1), 1_000_000);
    const hit = m?.route.run({ db, org, url, params: m.params, page, per });
    if (!hit) return isApi ? jsonRes({ error: `not found: ${url.pathname}` }, 404) : pageRes(v.notFound(url.pathname, url.pathname), 404);
    return isApi ? jsonRes(hit.json) : pageRes(hit.page());
  }

  return {
    async fetch(req) {
      if (req.method !== "GET" && req.method !== "HEAD") return new Response("method not allowed", { status: 405, headers: { allow: "GET, HEAD" } });
      let res: Response;
      try {
        res = await respond(req);
      } catch (e) {
        res = new Response(JSON.stringify({ error: e instanceof Error ? e.message : String(e) }), { status: 500, headers: { "content-type": "application/json; charset=utf-8" } });
      }
      if (req.method !== "HEAD") return res;
      const body = await res.arrayBuffer();
      const headers = new Headers(res.headers);
      headers.set("content-length", String(body.byteLength));
      return new Response(null, { status: res.status, headers });
    },
  };
}

export function serve(db: Database, org: string, { port, hostname }: { port: number; hostname: string }) {
  return Bun.serve({ port, hostname, fetch: createApp({ db, org }).fetch, idleTimeout: 60 });
}
