/** The research dataset summary of a replica as TOML: counts, repos by last push, languages, tags and packages. No bulk data. */
import type { Database } from "bun:sqlite";
import { stats } from "./stats.ts";

/** Everything after this line in an existing file is kept when the file is regenerated (hand-written sections: sites, APIs). */
export const KEEP_MARKER = "# --- hand-written below: kept by `export` ---";

const str = (s: string) => JSON.stringify(s);
const key = (k: string) => (/^[A-Za-z0-9_-]+$/.test(k) ? k : str(k));
const val = (v: unknown): string => (v === null || v === undefined ? str("") : typeof v === "string" ? str(v) : Array.isArray(v) ? `[${v.map(val).join(", ")}]` : String(v));
const inline = (o: Record<string, unknown>) => `{ ${Object.entries(o).filter(([, v]) => v !== null && v !== undefined).map(([k, v]) => `${key(k)} = ${val(v)}`).join(", ")} }`;
const day = (s: string | null) => (s ?? "").slice(0, 10);

export function exportToml(db: Database, org: string, now = new Date()): string {
  const s = stats(db, org);
  const all = (sql: string, ...p: (string | number)[]) => db.query(sql).all(...p) as Record<string, string | number | null>[];
  const repoCount = db.query<{ total: number; forks: number; archived: number; active: number }, [string, string]>(
    "SELECT count(*) AS total, sum(is_fork) AS forks, sum(is_archived) AS archived, sum(is_fork = 0 AND is_archived = 0 AND pushed_at >= ?) AS active FROM repos WHERE org = ?",
  ).get(new Date(now.getTime() - 90 * 86_400_000).toISOString(), org)!;
  const out: string[] = [
    `# ${org} organization replica, ${day(now.toISOString())} (bun packages/org-mirror/src/cli.ts export ${org} --toml; summary only, the data stays in the sqlite replica)`,
    "version = 1",
    `retrieved = ${str(day(now.toISOString()))}`,
    `sources = [${str(`https://github.com/${org}`)}, "https://registry.npmjs.org", "https://crates.io", "https://pypi.org"]`,
    "",
    "[org]",
    `repos = ${repoCount.total}`,
    `forks = ${repoCount.forks ?? 0}`,
    `archived = ${repoCount.archived ?? 0}`,
    `active_90d = ${repoCount.active ?? 0}`,
    `deep_synced = ${s.compare.length}`,
    "",
    "[rows]",
    ...Object.entries(s.tables).map(([k, v]) => `${key(k)} = ${v}`),
    "",
    "[github_vs_replica]",
    `issues = ${inline({ replica: s.totals.issues[0], github: s.totals.issues[1] })}`,
    `pulls = ${inline({ replica: s.totals.pulls[0], github: s.totals.pulls[1] })}`,
    `releases = ${inline({ replica: s.totals.releases[0], github: s.totals.releases[1] })}`,
    "",
    "[languages]",
    ...all("SELECT language, sum(bytes) AS bytes, count(*) AS repos FROM repo_languages WHERE repo LIKE ? GROUP BY language ORDER BY bytes DESC LIMIT 20", `${org}/%`).map(r => `${key(String(r.language))} = ${inline({ bytes: r.bytes, repos: r.repos })}`),
    "",
    "[tags]",
    ...all("SELECT tag, group_concat(substr(repo, length(?) + 2), ',') AS repos FROM repo_tags WHERE repo LIKE ? GROUP BY tag ORDER BY tag", org, `${org}/%`).map(r => `${key(String(r.tag))} = ${val(String(r.repos).split(","))}`),
    "",
  ];
  const compare = new Map(s.compare.map(c => [c.repo, c]));
  for (const r of all("SELECT full_name, name, stars, primary_language, license, pushed_at, is_fork, is_archived, description FROM repos WHERE org = ? ORDER BY pushed_at DESC", org)) {
    const c = compare.get(String(r.full_name));
    const tags = all("SELECT tag FROM repo_tags WHERE repo = ? ORDER BY tag", String(r.full_name)).map(t => String(t.tag));
    out.push(
      "[[repo]]",
      `name = ${val(r.name)}`,
      `stars = ${r.stars}`,
      `language = ${val(r.primary_language)}`,
      `pushed = ${val(day(String(r.pushed_at)))}`,
      `license = ${val(r.license)}`,
      `fork = ${!!r.is_fork}`,
      `archived = ${!!r.is_archived}`,
      `description = ${val(String(r.description ?? "").slice(0, 200))}`,
    );
    if (tags.length) out.push(`tags = ${val(tags)}`);
    if (c) out.push(`replica = ${inline({ issues: c.issues[0], pulls: c.pulls[0], releases: c.releases[0], commits: c.stored_commits })}`, `github = ${inline({ issues: c.issues[1], pulls: c.pulls[1], releases: c.releases[1], default_branch_commits: c.commits[1] })}`);
    out.push("");
  }
  for (const p of all("SELECT ecosystem, name, repo, found_by, status, latest, last_published, versions_count, downloads_recent, downloads_window, downloads_total FROM packages ORDER BY ecosystem, downloads_recent DESC, name LIMIT 300")) {
    out.push(
      "[[package]]",
      `ecosystem = ${val(p.ecosystem)}`,
      `name = ${val(p.name)}`,
      `status = ${val(p.status)}`,
      `found_by = ${val(p.found_by)}`,
      ...(p.repo ? [`repo = ${val(p.repo)}`] : []),
      ...(p.latest ? [`latest = ${val(p.latest)}`] : []),
      ...(p.last_published ? [`last_published = ${val(day(String(p.last_published)))}`] : []),
      `versions = ${p.versions_count}`,
      ...(p.downloads_recent != null ? [`downloads = ${inline({ recent: p.downloads_recent, window: p.downloads_window })}`] : []),
      ...(p.downloads_total != null ? [`downloads_total = ${p.downloads_total}`] : []),
      "",
    );
  }
  return out.join("\n");
}

/** The generated text, followed by whatever an existing file kept after KEEP_MARKER. */
export function withKept(generated: string, existing: string | undefined): string {
  const i = existing?.indexOf(KEEP_MARKER) ?? -1;
  return i < 0 ? generated.trimEnd() + "\n" : `${generated.trimEnd()}\n\n${existing!.slice(i).trimEnd()}\n`;
}
