/** Row counts per table and the replica set against GitHub's own totals (recorded from the repository query). */
import type { Database } from "bun:sqlite";
import { TABLES } from "./db.ts";

export interface RepoCompare {
  repo: string;
  issues: [number, number];
  pulls: [number, number];
  releases: [number, number];
  labels: [number, number];
  milestones: [number, number];
  /** Default-branch commits: `git rev-list --count` on the mirror against GitHub's history count. */
  commits: [number | null, number | null];
  stored_commits: number;
}

export interface Stats {
  org: string;
  tables: Record<string, number>;
  compare: RepoCompare[];
  totals: { issues: [number, number]; pulls: [number, number]; releases: [number, number] };
}

const n = (db: Database, sql: string, ...p: string[]) => db.query<{ c: number }, string[]>(sql).get(...p)!.c;

export function stats(db: Database, org: string): Stats {
  const tables: Record<string, number> = {};
  for (const t of TABLES) tables[t] = n(db, `SELECT count(*) AS c FROM ${t}`);
  const repos = db.query<{ full_name: string; issues_total: number; prs_total: number; releases_total: number; labels_total: number; milestones_total: number; default_commits_github: number | null; default_commits_git: number | null }, [string]>(
    "SELECT full_name, issues_total, prs_total, releases_total, labels_total, milestones_total, default_commits_github, default_commits_git FROM repos WHERE org = ? AND full_name IN (SELECT repo FROM sync_state UNION SELECT repo FROM refs) ORDER BY pushed_at DESC",
  ).all(org);
  const compare = repos.map<RepoCompare>(r => ({
    repo: r.full_name,
    issues: [n(db, "SELECT count(*) AS c FROM issues WHERE repo = ?", r.full_name), r.issues_total],
    pulls: [n(db, "SELECT count(*) AS c FROM pulls WHERE repo = ?", r.full_name), r.prs_total],
    releases: [n(db, "SELECT count(*) AS c FROM releases WHERE repo = ?", r.full_name), r.releases_total],
    labels: [n(db, "SELECT count(*) AS c FROM labels WHERE repo = ?", r.full_name), r.labels_total],
    milestones: [n(db, "SELECT count(*) AS c FROM milestones WHERE repo = ?", r.full_name), r.milestones_total],
    commits: [r.default_commits_git, r.default_commits_github],
    stored_commits: n(db, "SELECT count(*) AS c FROM commits WHERE repo = ?", r.full_name),
  }));
  const sum = (k: "issues" | "pulls" | "releases"): [number, number] => [compare.reduce((a, r) => a + r[k][0], 0), compare.reduce((a, r) => a + r[k][1], 0)];
  return { org, tables, compare, totals: { issues: sum("issues"), pulls: sum("pulls"), releases: sum("releases") } };
}

/** The stats as a fixed-width text report. */
export function formatStats(s: Stats): string {
  const w = Math.max(...Object.keys(s.tables).map(k => k.length));
  const lines = [`${s.org}: rows per table`, ...Object.entries(s.tables).map(([k, v]) => `  ${k.padEnd(w)}  ${String(v).padStart(9)}`)];
  lines.push("", "replica / GitHub per repo (issues, pulls, releases, labels, default-branch commits git / GitHub)");
  const cell = (p: [number | null, number | null]) => `${p[0] ?? "-"}/${p[1] ?? "-"}`;
  for (const r of s.compare) lines.push(`  ${r.repo.padEnd(34)} ${cell(r.issues).padStart(13)} ${cell(r.pulls).padStart(13)} ${cell(r.releases).padStart(9)} ${cell(r.labels).padStart(7)} ${cell(r.commits).padStart(13)}`);
  const t = s.totals;
  lines.push(`  total issues ${t.issues[0]}/${t.issues[1]}, pulls ${t.pulls[0]}/${t.pulls[1]}, releases ${t.releases[0]}/${t.releases[1]}`);
  return lines.join("\n");
}
