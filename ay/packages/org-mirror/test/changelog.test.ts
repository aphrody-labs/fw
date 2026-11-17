import { describe, expect, setDefaultTimeout, test } from "bun:test";
import { join } from "node:path";
import { changelogPaths, headingDate, headingVersion, latestVersion, parseChangelog, syncChangelogs } from "../src/changelog.ts";
import { openDb } from "../src/db.ts";
import { ensureMirror } from "../src/git.ts";
import { fixture, makeRepo, tmp } from "./helpers.ts";

setDefaultTimeout(60_000);

describe("changelog parsing on real files", () => {
  test("Keep a Changelog: bracketed versions, dates, Unreleased, sub-headings stay in the body", () => {
    const e = parseChangelog(fixture("changelog-keepachangelog.md"));
    expect(e.map(x => [x.version, x.date])).toEqual([["Unreleased", null], ["2.0.0", "2026-06-07"], ["1.1.2", "2024-09-27"]]);
    expect(e[1]!.body).toContain("### Removed");
    expect(e[1]!.body).not.toContain("## [1.1.2]");
    expect(latestVersion(e)).toBe("2.0.0");
  });

  test("Changesets: plain version headings, empty bodies for empty releases, Patch Changes kept", () => {
    const e = parseChangelog(fixture("changelog-changesets.md"));
    expect(e[0]).toMatchObject({ version: "0.6.2", body: "", position: 0 });
    expect(e[1]!.version).toBe("0.6.1");
    expect(e[1]!.body).toContain("### Patch Changes");
    expect(e.map(x => x.version)).toEqual([...e.map(x => x.version)].sort((a, b) => b.localeCompare(a, undefined, { numeric: true })));
  });

  test("Express History.md: setext headings (version over a line of =)", () => {
    const e = parseChangelog(fixture("changelog-history.md"));
    expect(e[0]).toMatchObject({ version: "5.3.0" });
    expect(e[0]!.body).toContain("Bug fixes");
    expect(e[0]!.body).not.toContain("=====");
  });

  test("headingVersion and headingDate understand the common shapes", () => {
    for (const [h, v] of [["[1.2.3] - 2024-01-31", "1.2.3"], ["v2.0.0-rc.1 (2023-05-01)", "2.0.0-rc.1"], ["@scope/pkg@3.4.5", "3.4.5"], ["Version 1.0", "1.0"], ["[Unreleased]", "Unreleased"], ["1.2.3 / 2014-01-01", "1.2.3"], ["`0.9.0`", "0.9.0"]] as const) expect(headingVersion(h)).toBe(v);
    for (const h of ["Node 18 support", "Breaking changes", "Added", "Q3 2024 plans"]) expect(headingVersion(h)).toBeNull();
    expect(headingDate("1.2.3 - 31.01.2024")).toBe("2024-01-31");
    expect(headingDate("v1 - 5 March 2023")).toBe("2023-03-05");
    expect(headingDate("v1 - Mar 5, 2023")).toBe("2023-03-05");
  });

  test("fenced code blocks do not produce headings, and text without versions parses to nothing", () => {
    expect(parseChangelog("# Notes\n\n```\n## 9.9.9\n```\n").length).toBe(0);
    expect(parseChangelog("hello")).toEqual([]);
  });

  test("changelogPaths finds CHANGELOG*, HISTORY*, NEWS* but not source files or vendored copies", () => {
    expect(changelogPaths(["CHANGELOG.md", "History.md", "packages/a/CHANGELOG.md", "NEWS", "docs/changes.ts", "src/news.rs", "node_modules/x/CHANGELOG.md", "CHANGELOG-v2.md", "newsletter.md", "README.md"]).sort()).toEqual(["CHANGELOG-v2.md", "CHANGELOG.md", "History.md", "NEWS", "packages/a/CHANGELOG.md"]);
  });
});

describe("changelogs of a repository at the default branch and at tags", () => {
  test("versions deleted from the file later are still recovered from the tag that had them", async () => {
    const v1 = "# Changelog\n\n## 1.0.0 - 2026-01-01\n\n- first\n";
    const v2 = "# Changelog\n\n## 2.0.0 - 2026-02-01\n\n- second\n\n## 1.0.0 - 2026-01-01\n\n- first\n";
    const v3 = "# Changelog\n\n## 3.0.0 - 2026-03-01\n\n- third, older history dropped\n";
    const origin = await makeRepo([
      { message: "v1", files: { "CHANGELOG.md": v1 }, tag: "v1.0.0" },
      { message: "v2", files: { "CHANGELOG.md": v2, "pkg/HISTORY.md": "1.0.0\n=====\n\n* init\n" }, tag: "v2.0.0" },
      { message: "v3", files: { "CHANGELOG.md": v3 } },
    ]);
    const dir = join(tmp(), "m.git");
    await ensureMirror(origin, dir);
    const db = openDb(":memory:");
    const refs = db.query("INSERT INTO refs (repo, name, kind, sha) VALUES (?, ?, ?, 'x')");
    refs.run("o/r", "v1.0.0", "tag");
    refs.run("o/r", "v2.0.0", "tag");
    const out = await syncChangelogs(db, "o/r", dir, "main");
    expect(out.files).toBe(2);
    const entries = db.query("SELECT path, version, source_ref, date FROM changelog_entries ORDER BY path, version").all();
    expect(entries).toEqual([
      { path: "CHANGELOG.md", version: "1.0.0", source_ref: "tag:v1.0.0", date: "2026-01-01" },
      { path: "CHANGELOG.md", version: "2.0.0", source_ref: "tag:v2.0.0", date: "2026-02-01" },
      { path: "CHANGELOG.md", version: "3.0.0", source_ref: "branch:main", date: "2026-03-01" },
      { path: "pkg/HISTORY.md", version: "1.0.0", source_ref: "branch:main", date: null },
    ]);
    expect(db.query("SELECT ref, path, entries, latest FROM changelog_files WHERE path = 'CHANGELOG.md' ORDER BY ref").all()).toEqual([
      { ref: "branch:main", path: "CHANGELOG.md", entries: 1, latest: "3.0.0" },
      { ref: "tag:v1.0.0", path: "CHANGELOG.md", entries: 1, latest: "1.0.0" },
      { ref: "tag:v2.0.0", path: "CHANGELOG.md", entries: 2, latest: "2.0.0" },
    ]);
    // a second sync only looks at the default branch again
    const again = await syncChangelogs(db, "o/r", dir, "main");
    expect(again.files).toBe(2);
    expect((db.query("SELECT count(*) AS n FROM changelog_files").get() as { n: number }).n).toBe(5);
  });

  test("a repository without changelog files records nothing", async () => {
    const origin = await makeRepo([{ message: "m", files: { "README.md": "x" } }]);
    const dir = join(tmp(), "m.git");
    await ensureMirror(origin, dir);
    expect(await syncChangelogs(openDb(":memory:"), "o/r", dir, "main")).toEqual({ files: 0, entries: 0 });
  });
});
