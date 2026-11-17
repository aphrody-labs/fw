import { Database } from "bun:sqlite";
import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseClaudeJsonl, scanClaudeProjectsDir, writeSessionSqliteIndex } from "./claude-sessions.ts";

describe("Claude Code session & memory importer (tools/claude-sessions.ts)", () => {
  test("parses JSONL turns, deduplicates repeated prompts, and counts tool_use blocks", () => {
    const sample = [
      JSON.stringify({ timestamp: "2026-11-17T10:00:00Z", message: { role: "user", content: "unify crates and submodules" } }),
      JSON.stringify({ timestamp: "2026-11-17T10:00:05Z", message: { role: "assistant", content: [{ type: "tool_use", name: "Bash" }, { type: "text", text: "done" }] } }),
      JSON.stringify({ timestamp: "2026-11-17T10:01:00Z", message: { role: "user", content: "unify crates and submodules" } }),
    ].join("\n");

    const parsed = parseClaudeJsonl("sess-1", "proj-a", sample);
    expect(parsed.messageCount).toBe(3);
    expect(parsed.userPromptCount).toBe(2);
    expect(parsed.prompts).toEqual(["unify crates and submodules"]);
    expect(parsed.toolCallCount).toBe(1);
  });

  test("scans project directories, indexes memory markdown, and persists to bun:sqlite", () => {
    const tmp = mkdtempSync(join(tmpdir(), "ay-claude-import-"));
    try {
      const proj = join(tmp, "project-alpha");
      mkdirSync(join(proj, "memory"), { recursive: true });
      writeFileSync(
        join(proj, "s1.jsonl"),
        JSON.stringify({ timestamp: "2026-11-17T12:00:00Z", message: { role: "user", content: "migrate cloudflare-os to bun" } }) + "\n",
      );
      writeFileSync(join(proj, "memory", "MEMORY.md"), "# Project Alpha Memory\n\nAlways build on the remote host.\n");

      const report = scanClaudeProjectsDir(tmp);
      expect(report.projectsScanned).toBe(1);
      expect(report.sessionsImported).toBe(1);
      expect(report.memoryNotesImported).toBe(1);

      const dbFile = join(tmp, "out.sqlite");
      writeSessionSqliteIndex(dbFile, report, tmp);
      const db = new Database(dbFile, { readonly: true });
      const stmt = db.prepare("SELECT title FROM claude_memories LIMIT 1");
      const row = stmt.get() as { title: string };
      stmt.finalize();
      expect(row.title).toBe("Project Alpha Memory");
      db.close();
    } finally {
      rmSync(tmp, { recursive: true, force: true });
    }
  });
});
