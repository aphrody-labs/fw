/** HTML pages. Each view takes the same data object the JSON twin serves, so the two can never disagree. */
import type { Row } from "./db.ts";
import { ext, fmtBytes, fmtDate, fmtNum, html, pager, tabs, withParams, type Page, type Safe } from "./html.ts";

export interface Paged<T> {
  total: number;
  page: number;
  per: number;
  items: T[];
}
export interface RepoItem {
  full_name: string;
  name: string;
  description: string | null;
  stars: number;
  primary_language: string | null;
  pushed_at: string | null;
  open_issues: number;
  open_prs: number;
  is_fork: number;
  is_archived: number;
  tags: string[];
}
export interface Overview extends Paged<RepoItem> {
  org: string;
  totals: Record<string, number>;
  tags: { tag: string; n: number }[];
}
export interface Count {
  stored: number;
  github: number | null;
}
export interface RepoDetail {
  repo: Row;
  languages: Row[];
  topics: string[];
  tags: Row[];
  refs: { branches: number; tags: number };
  releases: Row[];
  counts: { issues: Count; pulls: Count; releases: Count; commits: { git: number | null; github: number | null; stored: number } };
}
export interface CommitItem {
  sha: string;
  subject: string | null;
  author_name: string | null;
  author_date: string | null;
  committer_date: string | null;
  files: number | null;
  additions: number | null;
  deletions: number | null;
}
export interface CommitDetail {
  commit: Row;
  parents: string[];
}
export interface ThreadItem {
  number: number;
  title: string | null;
  state: string;
  author: string | null;
  created_at: string | null;
  updated_at: string | null;
  comments_count: number;
  is_draft?: number;
  labels: string[];
}
export interface EventItem {
  id: string;
  type: string;
  actor: string | null;
  created_at: string | null;
  data: unknown;
}
export interface ThreadDetail {
  kind: "issue" | "pr";
  state: string;
  item: Row;
  labels: string[];
  milestone: Row | null;
  comments: Row[];
  reviews: Row[];
  events: EventItem[];
}
export interface ReleaseItem {
  tag: string;
  name: string | null;
  is_prerelease: number;
  is_draft: number;
  is_latest: number;
  published_at: string | null;
  author: string | null;
  assets: number;
  downloads: number;
}
export interface ReleaseDetail {
  release: Row;
  assets: Row[];
}
export interface ChangelogPage extends Paged<Row> {
  files: { path: string; entries: number; snapshots: number; latest: string | null }[];
}
export interface PackagesPage extends Paged<Row> {
  ecosystems: { ecosystem: string; n: number }[];
}
export interface PackageDetail extends Paged<Row> {
  package: Row;
}
export interface Endpoint {
  api: string;
  page: string;
  doc: string;
}
/** What a view needs from the request. */
export interface View {
  url: URL;
  params: Record<string, string>;
}

const enc = encodeURIComponent;
const str = (v: unknown) => (v == null ? "" : String(v));
const repoName = (full: string) => full.slice(full.indexOf("/") + 1);
const repoHref = (name: string, sub = "") => `/repos/${enc(name)}${sub}`;
const sha7 = (sha: string) => sha.slice(0, 7);
const flag = (v: unknown) => Number(v) === 1;
const tagChip = (tag: string) => html`<a class="chip tag" href="/repos?tag=${enc(tag)}">${tag}</a>`;
const labelChips = (labels: string[], url: URL) => labels.map(l => html` <a class="chip label" href="${withParams(url, { label: l, page: null })}">${l}</a>`);
const stateChip = (state: string) => html`<span class="chip ${state}">${state}</span>`;
const empty = (what: string) => html`<p class="muted">No ${what}.</p>`;
const dl = (rows: [string, unknown][]) => html`<dl class="kv">${rows.filter(([, v]) => v !== "" && v != null).map(([k, v]) => html`<dt>${k}</dt><dd>${v as Safe | string | number}</dd>`)}</dl>`;

function searchForm(url: URL, placeholder: string, keep: string[] = []): Safe {
  return html`<form class="search" method="get">
<input type="search" name="q" value="${url.searchParams.get("q") ?? ""}" placeholder="${placeholder}" aria-label="${placeholder}">
${keep.map(k => (url.searchParams.get(k) ? html`<input type="hidden" name="${k}" value="${url.searchParams.get(k)}">` : ""))}
<button type="submit">Search</button></form>`;
}

function repoTabs(name: string, current: string): Safe {
  const items: [string, string][] = [["", "Overview"], ["/commits", "Commits"], ["/pulls", "Pull requests"], ["/issues", "Issues"], ["/releases", "Releases"], ["/changelog", "Changelog"]];
  return tabs(items.map(([sub, label]) => ({ href: repoHref(name, sub), label, current: sub === current })));
}

function repoTable(items: RepoItem[]): Safe {
  if (!items.length) return empty("repositories");
  return html`<div class="scroll"><table><thead><tr><th>Repository</th><th class="num">Stars</th><th>Language</th><th>Tags</th><th>Pushed</th><th class="num">Issues</th><th class="num">PRs</th></tr></thead><tbody>
${items.map(r => html`<tr><td><a href="${repoHref(r.name)}">${r.name}</a>${flag(r.is_archived) ? html` <span class="chip">archived</span>` : ""}${flag(r.is_fork) ? html` <span class="chip">fork</span>` : ""}<div class="muted">${r.description}</div></td>
<td class="num">${fmtNum(r.stars)}</td><td>${r.primary_language}</td><td><div class="chips">${r.tags.map(tagChip)}</div></td><td>${fmtDate(r.pushed_at)}</td>
<td class="num"><a href="${repoHref(r.name, "/issues")}">${fmtNum(r.open_issues)}</a></td><td class="num"><a href="${repoHref(r.name, "/pulls")}">${fmtNum(r.open_prs)}</a></td></tr>`)}
</tbody></table></div>`;
}

export function overview(d: Overview, v: View): Page {
  const links: Record<string, string> = { repos: "/repos", packages: "/packages" };
  return {
    title: d.org,
    subtitle: "A replica of the GitHub organization: repositories, history, pull requests, issues, releases, changelogs and packages.",
    path: v.url.pathname,
    body: html`<div class="stats">${Object.entries(d.totals).map(([k, n]) => html`<div class="stat"><b>${fmtNum(n)}</b>${links[k] ? html`<a href="${links[k]}">${k}</a>` : html`<span>${k}</span>`}</div>`)}</div>
<h2>Ecosystems</h2>
<div class="chips">${d.tags.map(t => html`<a class="chip tag" href="/repos?tag=${enc(t.tag)}">${t.tag}<b>${fmtNum(t.n)}</b></a>`)}</div>
<h2>Repositories by last push</h2>
${repoTable(d.items)}${pager(v.url, d)}`,
  };
}

export function repos(d: Paged<RepoItem>, v: View): Page {
  const tag = v.url.searchParams.get("tag");
  return {
    title: "Repositories",
    subtitle: tag ? `Tagged ${tag}.` : undefined,
    path: v.url.pathname,
    body: html`${searchForm(v.url, "Name or description", ["tag"])}${tag ? html`<p><a href="${withParams(v.url, { tag: null, page: null })}">Clear the ${tag} filter</a></p>` : ""}${repoTable(d.items)}${pager(v.url, d)}`,
  };
}

function langBar(langs: Row[]): Safe {
  const sum = langs.reduce((a, l) => a + Number(l.share), 0) || 1;
  const segs = langs.map((l, i) => ({ name: str(l.language), pct: (Number(l.share) / sum) * 100, c: `l${i % 6}` }));
  return html`<div class="langbar" role="img" aria-label="Language share">${segs.map(s => html`<span class="${s.c}" style="width:${s.pct.toFixed(2)}%" title="${s.name}"></span>`)}</div>
<ul class="legend">${segs.map(s => html`<li><i class="${s.c}"></i>${s.name} <span class="muted">${s.pct.toFixed(1)}%</span></li>`)}</ul>`;
}

function compare(label: string, stored: number | null, github: number | null, href?: string): Safe {
  const delta = stored != null && github != null ? stored - github : null;
  const cell = delta === null ? "" : delta === 0 ? html`<span class="ok">complete</span>` : html`<span class="delta">${delta > 0 ? "+" : ""}${fmtNum(delta)}</span>`;
  return html`<tr><td>${href ? html`<a href="${href}">${label}</a>` : label}</td><td class="num">${fmtNum(stored)}</td><td class="num">${fmtNum(github)}</td><td>${cell}</td></tr>`;
}

export function repo(d: RepoDetail, v: View): Page {
  const r = d.repo;
  const name = str(r.name);
  const c = d.counts;
  return {
    title: name,
    subtitle: str(r.description),
    path: v.url.pathname,
    body: html`${repoTabs(name, "")}
<div class="chips">${flag(r.is_fork) ? html`<span class="chip">fork</span>` : ""}${flag(r.is_archived) ? html`<span class="chip">archived</span>` : ""}${flag(r.is_private) ? html`<span class="chip">private</span>` : ""}${flag(r.is_template) ? html`<span class="chip">template</span>` : ""}${d.tags.map(t => tagChip(str(t.tag)))}${d.topics.map(t => html`<span class="chip">${t}</span>`)}</div>
${dl([
  ["GitHub", ext(str(r.url), str(r.url))],
  ["Homepage", ext(str(r.homepage), str(r.homepage))],
  ["Default branch", r.default_branch],
  ["License", r.license],
  ["Stars", fmtNum(Number(r.stars))],
  ["Forks", fmtNum(Number(r.forks))],
  ["Watchers", fmtNum(Number(r.watchers))],
  ["Open issues", fmtNum(Number(r.open_issues))],
  ["Open pull requests", fmtNum(Number(r.open_prs))],
  ["Size", r.disk_kb == null ? "" : fmtBytes(Number(r.disk_kb) * 1024)],
  ["Branches", fmtNum(d.refs.branches)],
  ["Tags", fmtNum(d.refs.tags)],
  ["Created", fmtDate(str(r.created_at))],
  ["Pushed", fmtDate(str(r.pushed_at))],
  ["Last sync", fmtDate(str(r.synced_at))],
])}
<h2>Languages</h2>
${d.languages.length ? langBar(d.languages) : empty("language data")}
<h2>Stored vs GitHub</h2>
<div class="scroll"><table><thead><tr><th>Data</th><th class="num">Stored</th><th class="num">GitHub</th><th>Status</th></tr></thead><tbody>
${compare("Issues", c.issues.stored, c.issues.github, repoHref(name, "/issues?state=all"))}
${compare("Pull requests", c.pulls.stored, c.pulls.github, repoHref(name, "/pulls?state=all"))}
${compare("Releases", c.releases.stored, c.releases.github, repoHref(name, "/releases"))}
${compare("Default-branch commits (git)", c.commits.git, c.commits.github)}
${compare("Commits in the database", c.commits.stored, null, repoHref(name, "/commits"))}
</tbody></table></div>
<h2>Latest releases</h2>
${d.releases.length ? html`<ul class="items">${d.releases.map(x => html`<li class="item"><a class="title" href="${repoHref(name, `/releases/${enc(str(x.tag))}`)}">${str(x.name) || str(x.tag)}</a> <span class="mono muted">${x.tag}</span> ${flag(x.is_prerelease) ? html`<span class="chip">pre-release</span>` : ""}<div class="meta">${fmtDate(str(x.published_at))}</div></li>`)}</ul>` : empty("releases")}`,
  };
}

export function commits(d: Paged<CommitItem>, v: View): Page {
  const name = v.params.name!;
  return {
    title: `${name} commits`,
    path: v.url.pathname,
    body: html`${repoTabs(name, "/commits")}${searchForm(v.url, "Subject or author")}
${d.items.length ? html`<ul class="items">${d.items.map(x => html`<li class="item"><a class="title" href="${repoHref(name, `/commits/${x.sha}`)}">${x.subject}</a>
<div class="meta"><code>${sha7(x.sha)}</code> ${x.author_name} committed ${fmtDate(x.committer_date)}${x.files == null ? "" : ` - ${fmtNum(x.files)} files, +${fmtNum(x.additions)} -${fmtNum(x.deletions)}`}</div></li>`)}</ul>` : empty("commits")}
${pager(v.url, d)}`,
  };
}

export function commit(d: CommitDetail, v: View): Page {
  const name = v.params.name!;
  const c = d.commit;
  return {
    title: str(c.subject) || sha7(str(c.sha)),
    path: v.url.pathname,
    body: html`${repoTabs(name, "/commits")}
${dl([
  ["Commit", html`<code>${c.sha}</code>`],
  ["Author", `${str(c.author_name)} on ${fmtDate(str(c.author_date))}`],
  ["Committer", `${str(c.committer_name)} on ${fmtDate(str(c.committer_date))}`],
  ["Parents", d.parents.length ? d.parents.map(p => html`<a href="${repoHref(name, `/commits/${p}`)}"><code>${sha7(p)}</code></a> `) : ""],
  ["Changes", c.files == null ? "" : `${fmtNum(Number(c.files))} files, +${fmtNum(Number(c.additions))} -${fmtNum(Number(c.deletions))}`],
])}
<pre class="body">${str(c.body) || "(no message body)"}</pre>`,
  };
}

export const STATES = ["open", "closed", "merged", "all"];

export function threads(d: Paged<ThreadItem>, v: View, kind: "issue" | "pr"): Page {
  const name = v.params.name!;
  const sub = kind === "pr" ? "/pulls" : "/issues";
  const state = STATES.includes(v.url.searchParams.get("state") ?? "") ? v.url.searchParams.get("state")! : "open";
  const label = v.url.searchParams.get("label");
  return {
    title: `${name} ${kind === "pr" ? "pull requests" : "issues"}`,
    path: v.url.pathname,
    body: html`${repoTabs(name, sub)}
${tabs(STATES.filter(s => kind === "pr" || s !== "merged").map(s => ({ href: withParams(v.url, { state: s, page: null }), label: s, current: s === state })))}
${searchForm(v.url, "Title", ["state", "label"])}
${label ? html`<p>Label <span class="chip label">${label}</span> <a href="${withParams(v.url, { label: null, page: null })}">clear</a></p>` : ""}
${d.items.length ? html`<ul class="items">${d.items.map(x => html`<li class="item"><a class="title" href="${repoHref(name, `${sub}/${x.number}`)}">#${x.number} ${x.title}</a> ${stateChip(x.state)}${flag(x.is_draft) ? html` <span class="chip">draft</span>` : ""}${labelChips(x.labels, v.url)}
<div class="meta">${x.author} opened ${fmtDate(x.created_at)}, updated ${fmtDate(x.updated_at)} - ${fmtNum(x.comments_count)} comments</div></li>`)}</ul>` : empty(kind === "pr" ? "pull requests" : "issues")}
${pager(v.url, d)}`,
  };
}

const summarize = (data: unknown): string => {
  if (data === null || data === undefined) return "";
  if (typeof data !== "object") return String(data);
  const parts = Object.entries(data).filter(([, x]) => x !== null && x !== "").map(([k, x]) => `${k}: ${typeof x === "object" ? JSON.stringify(x) : x}`);
  const text = parts.join(", ");
  return text.length > 240 ? `${text.slice(0, 237)}...` : text;
};

const bodyOf = (b: unknown) => html`<pre class="body">${str(b) || "(empty)"}</pre>`;

export function thread(d: ThreadDetail, v: View): Page {
  const name = v.params.name!;
  const it = d.item;
  const pr = d.kind === "pr";
  const sub = pr ? "/pulls" : "/issues";
  return {
    title: `#${it.number} ${str(it.title)}`,
    path: v.url.pathname,
    body: html`${repoTabs(name, sub)}
<p>${stateChip(d.state)}${flag(it.is_draft) ? html` <span class="chip">draft</span>` : ""}${d.labels.map(l => html` <a class="chip label" href="${repoHref(name, `${sub}?state=all&label=${enc(l)}`)}">${l}</a>`)}</p>
${dl([
  ["Author", it.author],
  ["Opened", fmtDate(str(it.created_at))],
  ["Updated", fmtDate(str(it.updated_at))],
  ["Closed", fmtDate(str(it.closed_at))],
  ["Assignees", str(it.assignees)],
  ["Milestone", d.milestone ? `${str(d.milestone.title)} (${str(d.milestone.state)})` : ""],
  ["Comments", fmtNum(Number(it.comments_count))],
  ["Source", ext(str(it.url), str(it.url))],
  ...(pr
    ? ([
        ["Base", it.base_ref],
        ["Head", it.head_ref],
        ["Merged", flag(it.merged) ? `by ${str(it.merged_by)} on ${fmtDate(str(it.merged_at))}` : ""],
        ["Changes", it.changed_files == null ? "" : `${fmtNum(Number(it.changed_files))} files, +${fmtNum(Number(it.additions))} -${fmtNum(Number(it.deletions))}`],
        ["Commits", it.commits_count == null ? "" : fmtNum(Number(it.commits_count))],
        ["Review decision", it.review_decision],
      ] as [string, unknown][])
    : []),
])}
<h2>Description</h2>
${bodyOf(it.body)}
${pr ? html`<h2>Reviews</h2>${d.reviews.length ? d.reviews.map(r => html`<article class="comment"><header><b>${r.author}</b><span>${r.state}</span><span class="muted">${fmtDate(str(r.submitted_at))}</span></header>${r.body ? bodyOf(r.body) : ""}</article>`) : empty("reviews")}` : ""}
<h2>Comments</h2>
${d.comments.length ? d.comments.map(c => html`<article class="comment" id="comment-${c.id}"><header><b>${c.author}</b><span class="muted">${fmtDate(str(c.created_at))}</span></header>${bodyOf(c.body)}</article>`) : empty("comments")}
<h2>Timeline</h2>
${d.events.length ? html`<div class="scroll"><table><thead><tr><th>Event</th><th>Actor</th><th>Date</th><th>Details</th></tr></thead><tbody>${d.events.map(e => html`<tr><td>${e.type}</td><td>${e.actor}</td><td>${fmtDate(e.created_at)}</td><td>${summarize(e.data)}</td></tr>`)}</tbody></table></div>` : empty("timeline events")}`,
  };
}

export function releases(d: Paged<ReleaseItem>, v: View): Page {
  const name = v.params.name!;
  return {
    title: `${name} releases`,
    path: v.url.pathname,
    body: html`${repoTabs(name, "/releases")}
${d.items.length ? html`<ul class="items">${d.items.map(x => html`<li class="item"><a class="title" href="${repoHref(name, `/releases/${enc(x.tag)}`)}">${x.name || x.tag}</a> <code>${x.tag}</code>${flag(x.is_latest) ? html` <span class="chip tag">latest</span>` : ""}${flag(x.is_prerelease) ? html` <span class="chip">pre-release</span>` : ""}${flag(x.is_draft) ? html` <span class="chip">draft</span>` : ""}
<div class="meta">${fmtDate(x.published_at)} by ${x.author} - ${fmtNum(x.assets)} assets, ${fmtNum(x.downloads)} downloads</div></li>`)}</ul>` : empty("releases")}
${pager(v.url, d)}`,
  };
}

export function release(d: ReleaseDetail, v: View): Page {
  const name = v.params.name!;
  const r = d.release;
  return {
    title: str(r.name) || str(r.tag),
    path: v.url.pathname,
    body: html`${repoTabs(name, "/releases")}
${dl([
  ["Tag", html`<code>${r.tag}</code>`],
  ["Published", fmtDate(str(r.published_at))],
  ["Author", r.author],
  ["Source", ext(str(r.url), str(r.url))],
])}
<h2>Notes</h2>
${bodyOf(r.body)}
<h2>Assets</h2>
${d.assets.length ? html`<div class="scroll"><table><thead><tr><th>Name</th><th class="num">Size</th><th class="num">Downloads</th><th>Type</th></tr></thead><tbody>${d.assets.map(a => html`<tr><td>${ext(str(a.url), str(a.name))}</td><td class="num">${fmtBytes(Number(a.size))}</td><td class="num">${fmtNum(Number(a.downloads))}</td><td>${a.content_type}</td></tr>`)}</tbody></table></div>` : empty("assets")}`,
  };
}

export function changelog(d: ChangelogPage, v: View): Page {
  const name = v.params.name!;
  const path = v.url.searchParams.get("path");
  const paths = [...new Set(d.items.map(e => str(e.path)))];
  return {
    title: `${name} changelog`,
    path: v.url.pathname,
    body: html`${repoTabs(name, "/changelog")}
${d.files.length ? html`<div class="scroll"><table><thead><tr><th>File</th><th class="num">Versions</th><th class="num">Tag snapshots</th><th>Latest</th></tr></thead><tbody>${d.files.map(f => html`<tr><td><a href="${withParams(v.url, { path: f.path, page: null })}">${f.path}</a></td><td class="num">${fmtNum(f.entries)}</td><td class="num">${fmtNum(f.snapshots)}</td><td>${f.latest}</td></tr>`)}</tbody></table></div>` : empty("changelog files")}
${path ? html`<p><a href="${withParams(v.url, { path: null, page: null })}">Show every file</a></p>` : ""}
${paths.map(p => html`<h2>${p}</h2>${d.items.filter(e => e.path === p).map(e => html`<article class="comment"><header><b>${e.version}</b><span class="muted">${fmtDate(str(e.date))}</span></header>${bodyOf(e.body)}</article>`)}`)}
${pager(v.url, d)}`,
  };
}

const packageHref = (eco: string, name: string) => `/packages/${enc(eco)}/${enc(name)}`;
const downloads = (p: Row) => (p.downloads_recent == null ? "" : `${fmtNum(Number(p.downloads_recent))}${p.downloads_window ? ` / ${p.downloads_window}` : ""}`);

export function packages(d: PackagesPage, v: View): Page {
  const eco = v.url.searchParams.get("ecosystem");
  const groups = [...new Set(d.items.map(p => str(p.ecosystem)))];
  return {
    title: "Packages",
    subtitle: "Packages published from the repositories of the organization.",
    path: v.url.pathname,
    body: html`${tabs([{ href: withParams(v.url, { ecosystem: null, page: null }), label: "all", current: !eco }, ...d.ecosystems.map(e => ({ href: withParams(v.url, { ecosystem: e.ecosystem, page: null }), label: `${e.ecosystem} (${e.n})`, current: e.ecosystem === eco }))])}
${searchForm(v.url, "Package name", ["ecosystem"])}
${groups.map(g => html`<h2>${g}</h2><div class="scroll"><table><thead><tr><th>Name</th><th>Latest</th><th>Last published</th><th class="num">Versions</th><th class="num">Recent downloads</th><th>Status</th></tr></thead><tbody>
${d.items.filter(p => p.ecosystem === g).map(p => html`<tr><td><a href="${packageHref(g, str(p.name))}">${p.name}</a></td><td>${p.latest}</td><td>${fmtDate(str(p.last_published))}</td><td class="num">${fmtNum(Number(p.versions_count))}</td><td class="num">${downloads(p)}</td><td>${p.status}</td></tr>`)}</tbody></table></div>`)}
${d.items.length ? "" : empty("packages")}
${pager(v.url, d)}`,
  };
}

export function packageDetail(d: PackageDetail, v: View): Page {
  const p = d.package;
  return {
    title: str(p.name),
    subtitle: str(p.description),
    path: v.url.pathname,
    body: html`<p><a href="/packages?ecosystem=${enc(str(p.ecosystem))}">${p.ecosystem}</a></p>
${dl([
  ["Status", p.status],
  ["Latest", p.latest],
  ["Repository", p.repo ? html`<a href="${repoHref(repoName(str(p.repo)))}">${p.repo}</a>` : ""],
  ["Manifest", p.manifest_path],
  ["Found by", p.found_by],
  ["License", p.license],
  ["Homepage", ext(str(p.homepage), str(p.homepage))],
  ["Registry", ext(str(p.url), str(p.url))],
  ["Created", fmtDate(str(p.created_at))],
  ["Last published", fmtDate(str(p.last_published))],
  ["Recent downloads", downloads(p)],
  ["Total downloads", p.downloads_total == null ? "" : fmtNum(Number(p.downloads_total))],
  ["Error", p.error],
])}
<h2>Versions (${fmtNum(d.total)})</h2>
${d.items.length ? html`<div class="scroll"><table><thead><tr><th>Version</th><th>Published</th><th class="num">Downloads</th><th>Yanked</th></tr></thead><tbody>${d.items.map(x => html`<tr><td><code>${x.version}</code></td><td>${fmtDate(str(x.published_at))}</td><td class="num">${x.downloads == null ? "" : fmtNum(Number(x.downloads))}</td><td>${flag(x.yanked) ? "yanked" : ""}</td></tr>`)}</tbody></table></div>` : empty("versions")}
${pager(v.url, d)}`,
  };
}

export function apiIndex(endpoints: Endpoint[], v: View): Page {
  return {
    title: "API",
    subtitle: "Every page has a JSON twin under /api. Lists accept ?page=N&per=N (per is capped at 100) and answer { total, page, per, items }.",
    path: v.url.pathname,
    body: html`<div class="scroll"><table><thead><tr><th>JSON endpoint</th><th>Page</th><th>Description</th></tr></thead><tbody>${endpoints.map(e => html`<tr><td><code>${e.api}</code></td><td><code>${e.page}</code></td><td>${e.doc}</td></tr>`)}</tbody></table></div>`,
  };
}

export const notFound = (what: string, path: string): Page => ({ title: "Not found", path, body: html`<p class="lead">Nothing at ${what}.</p><p><a href="/">Back to the overview</a></p>` });
