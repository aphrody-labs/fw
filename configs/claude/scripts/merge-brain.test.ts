import { Database } from "bun:sqlite";
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseFrontmatter } from "./merge-brain.ts";

const SCRIPT = join(import.meta.dir, "merge-brain.ts");

describe("parseFrontmatter", () => {
  test("parses name, description, type and body", () => {
    const raw = `---\nname: "My Note"\ndescription: 'short desc'\ntype: feedback\n---\n\nBody line\n`;
    expect(parseFrontmatter(raw, "fallback")).toEqual({
      name: "My Note",
      description: "short desc",
      type: "feedback",
      body: "Body line",
    });
  });

  test("handles CRLF line endings", () => {
    const raw = "---\r\nname: crlf\r\ntype: user\r\n---\r\nbody\r\n";
    const r = parseFrontmatter(raw, "x");
    expect(r.name).toBe("crlf");
    expect(r.type).toBe("user");
    expect(r.body).toBe("body");
  });

  test("falls back when frontmatter is missing or partial", () => {
    expect(parseFrontmatter("  just text \n", "slug")).toEqual({
      name: "slug",
      description: "",
      type: "project",
      body: "just text",
    });
    const partial = parseFrontmatter("---\ndescription: d\n---\nb", "slug");
    expect(partial.name).toBe("slug");
    expect(partial.type).toBe("project");
    expect(partial.description).toBe("d");
  });
});

describe("merge-brain CLI (sandboxed HOME)", () => {
  let home: string;
  beforeAll(() => {
    home = mkdtempSync(join(tmpdir(), "claude-brain-test-"));
  });
  afterAll(() => {
    rmSync(home, { recursive: true, force: true });
  });

  const run = () =>
    Bun.spawnSync(["bun", "--no-install", "--no-env-file", SCRIPT], {
      cwd: home,
      env: {
        ...process.env,
        HOME: home,
        USERPROFILE: home,
        ANTIGRAVITY_CONVERSATION_ID: "",
      },
      stdout: "pipe",
      stderr: "pipe",
      stdin: "ignore",
    });

  test("exits 0 and writes nothing when no databases exist", () => {
    const r = run();
    expect(r.exitCode).toBe(0);
    expect(r.stdout.toString()).toMatch(/^Synced \d+ memories to SQLite\.$/m);
    expect(readdirSync(home).filter((f) => f !== ".bun")).toEqual([]);
    expect(existsSync(join(home, ".bun", "agent"))).toBe(false);
  });

  test("syncs memories into both sandboxed SQLite stores", () => {
    mkdirSync(join(home, ".agent"), { recursive: true });
    mkdirSync(join(home, ".bun", "agent"), { recursive: true });
    const agent = new Database(join(home, ".agent", "memory.sqlite"));
    agent.exec(
      "CREATE TABLE entries(id TEXT, source TEXT, scope TEXT, title TEXT, text TEXT, symbols TEXT, at INTEGER)",
    );
    agent.exec(
      "INSERT INTO entries VALUES ('md:stale', 'memory', 'user', 't', 'x', '', 0), ('keep', 'other', 'user', 't', 'x', '', 0)",
    );
    agent.close();
    const bun = new Database(join(home, ".bun", "agent", "memory.db"));
    bun.exec(
      "CREATE TABLE memories(project TEXT, name TEXT, description TEXT, type TEXT, body TEXT, source TEXT, updated INTEGER, PRIMARY KEY(project, name))",
    );
    bun.close();

    const r = run();
    expect(r.exitCode).toBe(0);
    const count = Number(
      /Synced (\d+) memories/.exec(r.stdout.toString())?.[1],
    );

    const a = new Database(join(home, ".agent", "memory.sqlite"), {
      readonly: true,
    });
    const rows = a
      .query<{ id: string }, []>("SELECT id FROM entries ORDER BY id")
      .all()
      .map((x) => x.id);
    a.close();
    expect(rows).toContain("keep");
    expect(rows).not.toContain("md:stale");
    expect(rows.filter((id) => id.startsWith("md:")).length).toBe(count);

    const b = new Database(join(home, ".bun", "agent", "memory.db"), {
      readonly: true,
    });
    const n = b
      .query<{ n: number }, []>(
        "SELECT count(*) AS n FROM memories WHERE source = 'claude-memory'",
      )
      .get()!.n;
    b.close();
    expect(n).toBe(count);
  });
});
