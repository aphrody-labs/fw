/** A small realistic organization inserted through plain SQL, for the server tests. */
import type { Database } from "bun:sqlite";
import { openDb } from "../src/db.ts";

export const ORG = "acme";
/** Addresses present in the database that no page and no API response may ever show. */
export const EMAILS = ["dana.private@example.com", "lee.hidden@example.org", "co.author@example.net"];
export const XSS_TITLE = "<script>alert(1)</script> broken parser";

type Rec = Record<string, string | number | null>;

function put(db: Database, table: string, ...rows: Rec[]) {
  for (const r of rows) {
    const cols = Object.keys(r);
    db.query(`INSERT INTO ${table} (${cols.join(", ")}) VALUES (${cols.map(() => "?").join(", ")})`).run(...Object.values(r));
  }
}

/** A stable 40-hex object name for commit number `n`. */
export const sha = (n: number) => new Bun.CryptoHasher("sha1").update(`commit-${n}`).digest("hex");

export function seed(db: Database = openDb(":memory:")): Database {
  const full = (name: string) => `${ORG}/${name}`;
  const repo = (name: string, extra: Rec) => put(db, "repos", { full_name: full(name), org: ORG, name, url: `https://github.com/${ORG}/${name}`, default_branch: "main", ...extra });

  repo("forge", {
    description: "A build tool for <everything>", homepage: "https://forge.example.dev", stars: 1200, forks: 80, watchers: 40, open_issues: 2, open_prs: 1,
    issues_total: 5, prs_total: 3, releases_total: 3, default_commits_git: 120, default_commits_github: 123, disk_kb: 2048, license: "MIT",
    primary_language: "Rust", created_at: "2024-01-05T10:00:00Z", pushed_at: "2026-11-15T12:00:00Z", synced_at: "2026-11-16T08:00:00Z",
  });
  repo("zigpkg", { description: "Zig package index", stars: 30, primary_language: "Zig", pushed_at: "2026-10-01T09:00:00Z", open_issues: 0, open_prs: 0 });
  repo("pytools", { description: "Python helpers", stars: 5, primary_language: "Python", pushed_at: "2026-11-01T09:00:00Z", is_archived: 1 });

  put(db, "repo_languages",
    { repo: full("forge"), language: "Rust", bytes: 800000, share: 0.8 },
    { repo: full("forge"), language: "TypeScript", bytes: 150000, share: 0.15 },
    { repo: full("forge"), language: "Shell", bytes: 50000, share: 0.05 });
  put(db, "repo_topics", { repo: full("forge"), topic: "build-tool" }, { repo: full("forge"), topic: "cli" });
  put(db, "repo_tags",
    { repo: full("forge"), tag: "rust", via: "language", share: 0.8 },
    { repo: full("forge"), tag: "bun", via: "manifest", share: null },
    { repo: full("zigpkg"), tag: "zig", via: "language", share: 0.9 },
    { repo: full("pytools"), tag: "python", via: "language", share: 0.95 });
  put(db, "refs",
    { repo: full("forge"), name: "main", kind: "branch", sha: sha(3) },
    { repo: full("forge"), name: "next", kind: "branch", sha: sha(2) },
    { repo: full("forge"), name: "v1.0.0", kind: "tag", sha: sha(1) },
    { repo: full("forge"), name: "v1.1.0", kind: "tag", sha: sha(3) });

  put(db, "commits",
    { repo: full("forge"), sha: sha(1), parents: "", author_name: "Dana Dev", author_email: EMAILS[0]!, author_date: "2026-10-01T10:00:00Z", committer_name: "Dana Dev", committer_email: EMAILS[0]!, committer_date: "2026-10-01T10:00:00Z", subject: "Initial import", body: "First commit.", files: 4, additions: 100, deletions: 0 },
    { repo: full("forge"), sha: sha(2), parents: sha(1), author_name: "Lee Lint", author_email: EMAILS[1]!, author_date: "2026-10-20T10:00:00Z", committer_name: "Lee Lint", committer_email: EMAILS[1]!, committer_date: "2026-10-20T10:00:00Z", subject: "Fix the lexer", body: "Handle unicode escapes.", files: 2, additions: 20, deletions: 5 },
    { repo: full("forge"), sha: sha(3), parents: sha(2), author_name: "Dana Dev", author_email: EMAILS[0]!, author_date: "2026-11-10T10:00:00Z", committer_name: "Dana Dev", committer_email: EMAILS[0]!, committer_date: "2026-11-10T10:00:00Z", subject: "Add the cache layer", body: `Speeds up rebuilds by 3x.\n\nCo-authored-by: Casey Co <${EMAILS[2]}>\nReported by ${EMAILS[1]}`, files: 7, additions: 300, deletions: 40 });
  for (let i = 4; i < 64; i++) {
    put(db, "commits", { repo: full("forge"), sha: sha(i), parents: sha(i - 1), author_name: "Bot", author_email: EMAILS[0]!, author_date: "2026-11-11T10:00:00Z", committer_name: "Bot", committer_email: EMAILS[0]!, committer_date: `2026-11-11T10:${String(i - 4).padStart(2, "0")}:00Z`, subject: `chore: bump dependency ${i}`, body: "", files: 1, additions: 1, deletions: 1 });
  }

  put(db, "labels", { repo: full("forge"), name: "bug", color: "d73a4a", description: "Something is broken" }, { repo: full("forge"), name: "enhancement", color: "a2eeef", description: null });
  put(db, "milestones", { repo: full("forge"), number: 1, title: "v1.1", state: "open", description: null, due_on: "2026-12-01T00:00:00Z", open_issues: 1, closed_issues: 3 });

  put(db, "issues",
    { repo: full("forge"), number: 1, title: XSS_TITLE, body: "Parser crashes on <b>bold</b> input.\nSecond line.", state: "open", author: "dana", created_at: "2026-11-01T10:00:00Z", updated_at: "2026-11-14T10:00:00Z", milestone: 1, comments_count: 2, assignees: "lee,dana", url: `https://github.com/${ORG}/forge/issues/1` },
    { repo: full("forge"), number: 2, title: "Add dark mode", body: "Please.", state: "open", author: "lee", created_at: "2026-11-02T10:00:00Z", updated_at: "2026-11-03T10:00:00Z", comments_count: 0 },
    { repo: full("forge"), number: 3, title: "Old regression", body: "", state: "closed", state_reason: "completed", author: "dana", created_at: "2026-09-02T10:00:00Z", updated_at: "2026-09-04T10:00:00Z", closed_at: "2026-09-04T10:00:00Z", comments_count: 1 });
  put(db, "pulls",
    { repo: full("forge"), number: 10, title: "Add the cache layer", body: "Implements the cache.\nSee #1.", state: "closed", author: "dana", created_at: "2026-11-05T10:00:00Z", updated_at: "2026-11-10T11:00:00Z", closed_at: "2026-11-10T11:00:00Z", comments_count: 1, merged: 1, merged_at: "2026-11-10T11:00:00Z", merged_by: "lee", base_ref: "main", head_ref: "cache", additions: 300, deletions: 40, changed_files: 7, commits_count: 3, review_decision: "APPROVED", reviews_count: 1, url: `https://github.com/${ORG}/forge/pull/10` },
    { repo: full("forge"), number: 11, title: "WIP: new parser", body: "Draft work.", state: "open", author: "lee", created_at: "2026-11-12T10:00:00Z", updated_at: "2026-11-15T10:00:00Z", comments_count: 0, is_draft: 1, base_ref: "main", head_ref: "parser", additions: 10, deletions: 2, changed_files: 1, review_decision: "REVIEW_REQUIRED" },
    { repo: full("forge"), number: 12, title: "Abandoned experiment", body: null, state: "closed", author: "dana", created_at: "2026-08-01T10:00:00Z", updated_at: "2026-08-02T10:00:00Z", closed_at: "2026-08-02T10:00:00Z", comments_count: 0 });
  put(db, "item_labels",
    { repo: full("forge"), kind: "issue", number: 1, label: "bug" },
    { repo: full("forge"), kind: "issue", number: 2, label: "enhancement" },
    { repo: full("forge"), kind: "pr", number: 10, label: "enhancement" });
  put(db, "comments",
    { repo: full("forge"), kind: "issue", number: 1, id: "c1", author: "lee", body: "Reproduced on Windows.", created_at: "2026-11-02T10:00:00Z" },
    { repo: full("forge"), kind: "issue", number: 1, id: "c2", author: "dana", body: "Fix incoming in #10.", created_at: "2026-11-03T10:00:00Z" },
    { repo: full("forge"), kind: "pr", number: 10, id: "c3", author: "lee", body: "Looks great, merging after CI.", created_at: "2026-11-09T10:00:00Z" });
  put(db, "reviews", { repo: full("forge"), number: 10, id: "r1", author: "lee", state: "APPROVED", body: "Ship it.", submitted_at: "2026-11-09T12:00:00Z" });
  put(db, "events",
    { repo: full("forge"), kind: "pr", number: 10, id: "e1", type: "labeled", actor: "dana", created_at: "2026-11-05T10:05:00Z", data: JSON.stringify({ label: "enhancement" }) },
    { repo: full("forge"), kind: "pr", number: 10, id: "e2", type: "merged", actor: "lee", created_at: "2026-11-10T11:00:00Z", data: JSON.stringify({ commit: sha(3), committer: { name: "Lee", email: EMAILS[1] } }) },
    { repo: full("forge"), kind: "issue", number: 1, id: "e3", type: "assigned", actor: "dana", created_at: "2026-11-01T10:30:00Z", data: JSON.stringify({ assignee: "lee" }) });

  put(db, "releases",
    { repo: full("forge"), tag: "v1.1.0", name: "Forge 1.1", body: "Highlights:\n- cache layer\n- <fast> rebuilds", is_latest: 1, created_at: "2026-11-10T12:00:00Z", published_at: "2026-11-10T12:00:00Z", author: "lee", url: `https://github.com/${ORG}/forge/releases/tag/v1.1.0` },
    { repo: full("forge"), tag: "v1.0.0", name: "Forge 1.0", body: "First release.", created_at: "2026-10-02T12:00:00Z", published_at: "2026-10-02T12:00:00Z", author: "dana" },
    { repo: full("forge"), tag: "v1.1.0-rc.1", name: "Forge 1.1 RC", body: "", is_prerelease: 1, created_at: "2026-11-01T12:00:00Z", published_at: "2026-11-01T12:00:00Z", author: "lee" });
  put(db, "release_assets",
    { repo: full("forge"), tag: "v1.1.0", name: "forge-linux-x64.tar.gz", size: 5_242_880, downloads: 321, content_type: "application/gzip", url: `https://github.com/${ORG}/forge/releases/download/v1.1.0/forge-linux-x64.tar.gz` },
    { repo: full("forge"), tag: "v1.1.0", name: "forge-windows-x64.zip", size: 6_000_000, downloads: 123, content_type: "application/zip", url: null });

  put(db, "changelog_files",
    { repo: full("forge"), ref: "branch:main", path: "CHANGELOG.md", blob: "b1", entries: 2, latest: "1.1.0" },
    { repo: full("forge"), ref: "tag:v1.0.0", path: "CHANGELOG.md", blob: "b0", entries: 1, latest: "1.0.0" },
    { repo: full("forge"), ref: "tag:v1.1.0", path: "CHANGELOG.md", blob: "b1", entries: 2, latest: "1.1.0" });
  put(db, "changelog_entries",
    { repo: full("forge"), path: "CHANGELOG.md", version: "1.0.0", date: "2026-10-02", body: "- Initial release", source_ref: "branch:main", position: 1 },
    { repo: full("forge"), path: "CHANGELOG.md", version: "1.1.0", date: "2026-11-10", body: "- Added the cache layer\n- Fixed the lexer", source_ref: "branch:main", position: 0 });

  put(db, "manifests", { repo: full("forge"), path: "package.json", ecosystem: "npm", name: "@acme/forge", version: "1.1.0" });
  put(db, "packages",
    { ecosystem: "npm", name: "@acme/forge", repo: full("forge"), manifest_path: "package.json", found_by: "manifest", status: "ok", description: "Forge for Node", latest: "1.1.0", license: "MIT", last_published: "2026-11-10T12:00:00Z", versions_count: 2, downloads_recent: 5400, downloads_window: "30d", downloads_total: 20000, url: "https://www.npmjs.com/package/@acme/forge" },
    { ecosystem: "crates", name: "forge-core", repo: full("forge"), found_by: "manifest", status: "ok", latest: "0.9.0", last_published: "2026-11-01T12:00:00Z", versions_count: 1, downloads_recent: 900, downloads_window: "90d" },
    { ecosystem: "pypi", name: "pytools", repo: full("pytools"), found_by: "name", status: "missing", versions_count: 0 },
    { ecosystem: "zig", name: "zigpkg", repo: full("zigpkg"), found_by: "manifest", status: "git", latest: "0.2.0", versions_count: 1 });
  put(db, "package_versions",
    { ecosystem: "npm", name: "@acme/forge", version: "1.0.0", published_at: "2026-10-02T12:00:00Z", downloads: 100 },
    { ecosystem: "npm", name: "@acme/forge", version: "1.1.0", published_at: "2026-11-10T12:00:00Z", downloads: 5300 },
    { ecosystem: "crates", name: "forge-core", version: "0.9.0", published_at: "2026-11-01T12:00:00Z" });
  return db;
}
