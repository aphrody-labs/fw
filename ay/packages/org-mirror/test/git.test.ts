import { describe, expect, setDefaultTimeout, test } from "bun:test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { openDb } from "../src/db.ts";
import { blobIds, ensureMirror, git, listRefs, logCommits, parseCommit, readBlobs, syncGit, lsTree } from "../src/git.ts";
import { makeRepo, tmp } from "./helpers.ts";
setDefaultTimeout(60_000);

describe("git side", () => {
  test("parseCommit reads fields, multi-line bodies and shortstat", () => {
    const chunk = ["c0ffee", "p1 p2", "Ann", "ann@example.test", "1700000000", "Bob", "bob@example.test", "1700000100", "Fix the thing", "Body line 1\n\nBody line 2\n"].join("\x1f") + "\x1d\n\n 3 files changed, 10 insertions(+), 2 deletions(-)\n";
    expect(parseCommit(chunk)).toMatchObject({ sha: "c0ffee", parents: "p1 p2", authorName: "Ann", committerName: "Bob", authorDate: "2023-11-14T22:13:20.000Z", subject: "Fix the thing", body: "Body line 1\n\nBody line 2", files: 3, additions: 10, deletions: 2 });
    expect(parseCommit(chunk.replace(/ 3 files.*$/s, ""))).toMatchObject({ files: null, additions: null });
  });

  test("mirror clone, refs, commits, and an incremental fetch that reads only new commits", async () => {
    const origin = await makeRepo([
      { message: "first", files: { "a.txt": "1\n" }, tag: "v0.1.0", date: "2026-01-01T00:00:00Z" },
      { message: "second\n\nwith a body", files: { "a.txt": "1\n2\n", "b/c.txt": "x\n" }, date: "2026-01-02T00:00:00Z" },
    ]);
    const db = openDb(":memory:");
    const dir = join(tmp(), "mirror.git");
    const first = await syncGit(db, "o/r", origin, dir, "main");
    expect(first).toMatchObject({ cloned: true, commits: 2, branches: 1, tags: 1, defaultBranchCommits: 2 });
    const rows = db.query("SELECT subject, body, files, additions, deletions, author_name FROM commits ORDER BY committer_date DESC").all();
    expect(rows).toEqual([
      { subject: "second", body: "with a body", files: 2, additions: 2, deletions: 0, author_name: "Test Author" },
      { subject: "first", body: "", files: 1, additions: 1, deletions: 0, author_name: "Test Author" },
    ]);
    expect(db.query("SELECT name, kind FROM refs ORDER BY kind, name").all()).toEqual([{ name: "main", kind: "branch" }, { name: "v0.1.0", kind: "tag" }]);
    expect(db.query("SELECT default_commits_git FROM repos WHERE full_name = 'o/r'").get()).toBeNull();

    writeFileSync(join(origin, "d.txt"), "new\n");
    await git(["add", "-A"], { cwd: origin });
    await git(["-c", "user.name=T", "-c", "user.email=t@example.test", "commit", "-q", "-m", "third"], { cwd: origin });
    await git(["tag", "v0.2.0"], { cwd: origin });
    const second = await syncGit(db, "o/r", origin, dir, "main");
    expect(second).toMatchObject({ cloned: false, commits: 1, tags: 2, defaultBranchCommits: 3 });
    expect(db.query("SELECT count(*) AS n FROM commits").get()).toEqual({ n: 3 });
  });

  test("a force push that drops a previous tip does not break the next sync", async () => {
    const origin = await makeRepo([{ message: "one", files: { "a": "1" } }, { message: "two", files: { "a": "2" } }]);
    const db = openDb(":memory:");
    const dir = join(tmp(), "m.git");
    await syncGit(db, "o/r", origin, dir, "main");
    await git(["reset", "-q", "--hard", "HEAD~1"], { cwd: origin });
    writeFileSync(join(origin, "a"), "3");
    await git(["-c", "user.name=T", "-c", "user.email=t@example.test", "commit", "-q", "-am", "replacement"], { cwd: origin });
    await git(["gc", "-q", "--prune=now"], { cwd: origin });
    await ensureMirror(origin, dir);
    const out = await syncGit(db, "o/r", origin, dir, "main");
    expect(out.commits).toBeGreaterThanOrEqual(1);
    expect(db.query("SELECT count(*) AS n FROM commits WHERE subject = 'replacement'").get()).toEqual({ n: 1 });
  });

  test("lsTree, blobIds and readBlobs read files from a ref; binary files are skipped", async () => {
    const origin = await makeRepo([{ message: "m", files: { "README.md": "hello\n", "dir/x.txt": "x" } }]);
    mkdirSync(join(origin, "bin"));
    writeFileSync(join(origin, "bin/blob"), Buffer.from([0, 1, 2, 3]));
    await git(["add", "-A"], { cwd: origin });
    await git(["-c", "user.name=T", "-c", "user.email=t@example.test", "commit", "-q", "-m", "bin"], { cwd: origin });
    expect((await lsTree(origin, "main")).sort()).toEqual(["README.md", "bin/blob", "dir/x.txt"]);
    const ids = await blobIds(origin, ["main:README.md", "main:nope", "main:bin/blob"]);
    expect(ids[1]).toBeUndefined();
    const texts = await readBlobs(origin, [ids[0]!, ids[2]!]);
    expect(texts.get(ids[0]!)).toBe("hello\n");
    expect(texts.get(ids[2]!)).toBeNull();
    expect((await listRefs(origin)).map(r => r.name)).toEqual(["main"]);
  });

  test("logCommits on an empty tip list reads nothing", async () => {
    const origin = await makeRepo([{ message: "m", files: { a: "1" } }]);
    expect(await logCommits(origin, [], [], () => {})).toBe(0);
  });
});
