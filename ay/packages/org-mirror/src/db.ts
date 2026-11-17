/** The sqlite schema of a replica. One database per organization; every table is keyed by the repository's full name. */
import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

export const SCHEMA_VERSION = 1;

/** Data directory of the ay tools: AY_DATA_DIR, else `ay` under XDG_DATA_HOME (LOCALAPPDATA on Windows, ~/.local/share elsewhere). */
export function dataDir(): string {
  if (process.env.AY_DATA_DIR) return process.env.AY_DATA_DIR;
  const base = process.env.XDG_DATA_HOME ?? (process.platform === "win32" ? process.env.LOCALAPPDATA : undefined) ?? join(homedir(), ".local", "share");
  return join(base, "ay");
}

/** Root of the replica data: AY_ORG_MIRROR_DIR, else `mirror` in the data directory. */
export function dataRoot(): string {
  return process.env.AY_ORG_MIRROR_DIR ?? join(dataDir(), "mirror");
}

export const orgDir = (org: string, root = dataRoot()) => join(root, org);
export const defaultDbPath = (org: string, root = dataRoot()) => join(orgDir(org, root), "mirror.db");
export const gitDir = (org: string, repo: string, root = dataRoot()) => join(orgDir(org, root), "git", `${repo}.git`);

const DDL = /* sql */ `
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS repos (
  full_name TEXT PRIMARY KEY, org TEXT NOT NULL, name TEXT NOT NULL, node_id TEXT,
  description TEXT, homepage TEXT, url TEXT, default_branch TEXT,
  is_fork INTEGER NOT NULL DEFAULT 0, is_archived INTEGER NOT NULL DEFAULT 0, is_private INTEGER NOT NULL DEFAULT 0, is_template INTEGER NOT NULL DEFAULT 0,
  stars INTEGER NOT NULL DEFAULT 0, forks INTEGER NOT NULL DEFAULT 0, watchers INTEGER NOT NULL DEFAULT 0,
  open_issues INTEGER NOT NULL DEFAULT 0, open_prs INTEGER NOT NULL DEFAULT 0,
  issues_total INTEGER NOT NULL DEFAULT 0, prs_total INTEGER NOT NULL DEFAULT 0, releases_total INTEGER NOT NULL DEFAULT 0,
  labels_total INTEGER NOT NULL DEFAULT 0, milestones_total INTEGER NOT NULL DEFAULT 0,
  default_commits_github INTEGER, default_commits_git INTEGER,
  disk_kb INTEGER, license TEXT, primary_language TEXT,
  created_at TEXT, updated_at TEXT, pushed_at TEXT, synced_at TEXT
);
CREATE INDEX IF NOT EXISTS repos_pushed ON repos (org, pushed_at DESC);

CREATE TABLE IF NOT EXISTS repo_languages (repo TEXT NOT NULL, language TEXT NOT NULL, bytes INTEGER NOT NULL, share REAL NOT NULL, PRIMARY KEY (repo, language)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS repo_topics (repo TEXT NOT NULL, topic TEXT NOT NULL, PRIMARY KEY (repo, topic)) WITHOUT ROWID;
-- Ecosystem tags (rust, zig, python, bun, typescript, go, ...) with the evidence that produced them.
CREATE TABLE IF NOT EXISTS repo_tags (repo TEXT NOT NULL, tag TEXT NOT NULL, via TEXT NOT NULL, share REAL, PRIMARY KEY (repo, tag)) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS refs (repo TEXT NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL CHECK (kind IN ('branch','tag')), sha TEXT NOT NULL, date TEXT, PRIMARY KEY (repo, kind, name)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS commits (
  repo TEXT NOT NULL, sha TEXT NOT NULL, parents TEXT NOT NULL DEFAULT '',
  author_name TEXT, author_email TEXT, author_date TEXT, committer_name TEXT, committer_email TEXT, committer_date TEXT,
  subject TEXT, body TEXT, files INTEGER, additions INTEGER, deletions INTEGER,
  PRIMARY KEY (repo, sha)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS commits_date ON commits (repo, committer_date DESC);

CREATE TABLE IF NOT EXISTS labels (repo TEXT NOT NULL, name TEXT NOT NULL, color TEXT, description TEXT, PRIMARY KEY (repo, name)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS milestones (repo TEXT NOT NULL, number INTEGER NOT NULL, title TEXT, state TEXT, description TEXT, due_on TEXT, created_at TEXT, closed_at TEXT, open_issues INTEGER, closed_issues INTEGER, PRIMARY KEY (repo, number)) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS issues (
  repo TEXT NOT NULL, number INTEGER NOT NULL, node_id TEXT, title TEXT, body TEXT, state TEXT, state_reason TEXT,
  author TEXT, created_at TEXT, updated_at TEXT, closed_at TEXT, locked INTEGER NOT NULL DEFAULT 0, milestone INTEGER,
  comments_count INTEGER NOT NULL DEFAULT 0, reactions INTEGER NOT NULL DEFAULT 0, url TEXT, assignees TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (repo, number)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS issues_updated ON issues (repo, updated_at DESC);
CREATE TABLE IF NOT EXISTS pulls (
  repo TEXT NOT NULL, number INTEGER NOT NULL, node_id TEXT, title TEXT, body TEXT, state TEXT,
  author TEXT, created_at TEXT, updated_at TEXT, closed_at TEXT, locked INTEGER NOT NULL DEFAULT 0, milestone INTEGER,
  comments_count INTEGER NOT NULL DEFAULT 0, reactions INTEGER NOT NULL DEFAULT 0, url TEXT, assignees TEXT NOT NULL DEFAULT '',
  is_draft INTEGER NOT NULL DEFAULT 0, merged INTEGER NOT NULL DEFAULT 0, merged_at TEXT, merged_by TEXT,
  base_ref TEXT, head_ref TEXT, head_sha TEXT, additions INTEGER, deletions INTEGER, changed_files INTEGER, commits_count INTEGER,
  review_decision TEXT, reviews_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (repo, number)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS pulls_updated ON pulls (repo, updated_at DESC);

-- kind is 'issue' or 'pr'; the three tables below hold the children of both.
CREATE TABLE IF NOT EXISTS item_labels (repo TEXT NOT NULL, kind TEXT NOT NULL, number INTEGER NOT NULL, label TEXT NOT NULL, PRIMARY KEY (repo, kind, number, label)) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS item_labels_label ON item_labels (repo, label);
CREATE TABLE IF NOT EXISTS comments (
  repo TEXT NOT NULL, kind TEXT NOT NULL, number INTEGER NOT NULL, id TEXT NOT NULL, author TEXT, body TEXT, created_at TEXT, updated_at TEXT, url TEXT,
  PRIMARY KEY (repo, kind, number, id)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS reviews (repo TEXT NOT NULL, number INTEGER NOT NULL, id TEXT NOT NULL, author TEXT, state TEXT, body TEXT, submitted_at TEXT, PRIMARY KEY (repo, number, id)) WITHOUT ROWID;
-- Timeline basics: closed, reopened, merged, labeled, renamed, assigned, cross-referenced, ... ; data holds the type specific JSON.
CREATE TABLE IF NOT EXISTS events (repo TEXT NOT NULL, kind TEXT NOT NULL, number INTEGER NOT NULL, id TEXT NOT NULL, type TEXT NOT NULL, actor TEXT, created_at TEXT, data TEXT, PRIMARY KEY (repo, kind, number, id)) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS releases (
  repo TEXT NOT NULL, tag TEXT NOT NULL, name TEXT, body TEXT, is_prerelease INTEGER NOT NULL DEFAULT 0, is_draft INTEGER NOT NULL DEFAULT 0, is_latest INTEGER NOT NULL DEFAULT 0,
  created_at TEXT, published_at TEXT, author TEXT, url TEXT, PRIMARY KEY (repo, tag)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS releases_pub ON releases (repo, published_at DESC);
CREATE TABLE IF NOT EXISTS release_assets (repo TEXT NOT NULL, tag TEXT NOT NULL, name TEXT NOT NULL, size INTEGER, downloads INTEGER, content_type TEXT, created_at TEXT, url TEXT, PRIMARY KEY (repo, tag, name)) WITHOUT ROWID;

-- Changelog files seen at the default branch and at each tag, and the versions parsed out of them.
CREATE TABLE IF NOT EXISTS changelog_files (repo TEXT NOT NULL, ref TEXT NOT NULL, path TEXT NOT NULL, blob TEXT NOT NULL, entries INTEGER NOT NULL, latest TEXT, PRIMARY KEY (repo, ref, path)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS changelog_entries (repo TEXT NOT NULL, path TEXT NOT NULL, version TEXT NOT NULL, date TEXT, body TEXT NOT NULL, source_ref TEXT NOT NULL, position INTEGER NOT NULL, PRIMARY KEY (repo, path, version)) WITHOUT ROWID;

-- Package manifests found in the repos (package.json, Cargo.toml, pyproject.toml, build.zig.zon, go.mod).
CREATE TABLE IF NOT EXISTS manifests (repo TEXT NOT NULL, path TEXT NOT NULL, ecosystem TEXT NOT NULL, name TEXT, version TEXT, private INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (repo, path)) WITHOUT ROWID;
-- Published packages. status: ok | missing (404 on the registry) | git (Zig: no registry, versions are git tags) | error.
CREATE TABLE IF NOT EXISTS packages (
  ecosystem TEXT NOT NULL, name TEXT NOT NULL, repo TEXT, manifest_path TEXT, found_by TEXT NOT NULL, status TEXT NOT NULL,
  description TEXT, latest TEXT, homepage TEXT, license TEXT, created_at TEXT, last_published TEXT, versions_count INTEGER NOT NULL DEFAULT 0,
  downloads_recent INTEGER, downloads_window TEXT, downloads_total INTEGER, url TEXT, fetched_at TEXT, error TEXT,
  PRIMARY KEY (ecosystem, name)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS package_versions (ecosystem TEXT NOT NULL, name TEXT NOT NULL, version TEXT NOT NULL, published_at TEXT, downloads INTEGER, yanked INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (ecosystem, name, version)) WITHOUT ROWID;

-- Resume state: one row per (repo, kind). cursor resumes an interrupted crawl, high is the newest updatedAt of the crawl in
-- progress, until is the watermark of the last complete crawl (the next one stops there).
CREATE TABLE IF NOT EXISTS sync_state (repo TEXT NOT NULL, kind TEXT NOT NULL, cursor TEXT, high TEXT, until TEXT, status TEXT NOT NULL DEFAULT 'idle', updated_at TEXT, note TEXT, PRIMARY KEY (repo, kind)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS sync_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, started_at TEXT NOT NULL, finished_at TEXT, args TEXT, note TEXT);
`;

export function openDb(path: string): Database {
  if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
  const db = new Database(path, { create: true });
  db.exec("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 15000; PRAGMA foreign_keys = OFF;");
  db.exec(DDL);
  db.query("INSERT INTO meta (key, value) VALUES ('schema_version', ?) ON CONFLICT(key) DO NOTHING").run(String(SCHEMA_VERSION));
  return db;
}

/** Every data table, in the order `stats` prints them. */
export const TABLES = [
  "repos", "repo_languages", "repo_topics", "repo_tags", "refs", "commits", "labels", "milestones", "issues", "pulls", "item_labels",
  "comments", "reviews", "events", "releases", "release_assets", "changelog_files", "changelog_entries", "manifests", "packages", "package_versions",
] as const;

export type Row = Record<string, string | number | null>;

/** Run `fn` in one transaction (nested calls join the outer one). */
export function tx<T>(db: Database, fn: () => T): T {
  return db.transaction(fn)();
}
