#!/usr/bin/env bun
// Imports, deduplicates, and indexes Claude Code sessions (`~/.claude/projects/*/*.jsonl` and `memory/*.md`)
// strictly into the git-ignored private store (`.private/claude-sessions/`) and optionally syncs non-duplicate
// memory notes into the local SQLite learning database (`~/.gemini/learning/memory.db`).
// Never writes session transcripts or machine paths into tracked files.
//   bun tools/claude-sessions.ts [--dir <claude-projects-dir>] [--sync-gemini] [--json]
import { Database } from "bun:sqlite";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import { basename, join } from "node:path";
import { privatePath, repoRoot } from "./local.ts";

export interface SessionTurnSummary {
  role: "user" | "assistant";
  timestamp?: string;
  preview: string;
  toolNames: string[];
}

export interface ImportedSession {
  sessionId: string;
  projectSlug: string;
  messageCount: number;
  userPromptCount: number;
  toolCallCount: number;
  firstTimestamp?: string;
  lastTimestamp?: string;
  prompts: string[];
}

export interface ImportedMemoryNote {
  projectSlug: string;
  fileName: string;
  title: string;
  contentHash: string;
  byteLength: number;
}

export interface ClaudeImportReport {
  projectsScanned: number;
  sessionsImported: number;
  uniquePrompts: number;
  totalToolCalls: number;
  memoryNotesImported: number;
  geminiSyncedNotes: number;
  sessions: ImportedSession[];
  memories: ImportedMemoryNote[];
}

function extractTextFromMessageContent(content: unknown): { text: string; tools: string[] } {
  if (typeof content === "string") return { text: content.trim(), tools: [] };
  if (!Array.isArray(content)) return { text: "", tools: [] };
  const texts: string[] = [];
  const tools: string[] = [];
  for (const block of content) {
    if (!block || typeof block !== "object") continue;
    const b = block as { type?: string; text?: string; name?: string };
    if (b.type === "text" && typeof b.text === "string") texts.push(b.text.trim());
    else if (b.type === "tool_use" && typeof b.name === "string") tools.push(b.name);
  }
  return { text: texts.join(" ").trim(), tools };
}

export function parseClaudeJsonl(sessionId: string, projectSlug: string, rawJsonl: string): ImportedSession {
  let messageCount = 0;
  let userPromptCount = 0;
  let toolCallCount = 0;
  let firstTimestamp: string | undefined;
  let lastTimestamp: string | undefined;
  const prompts: string[] = [];
  const seenPrompts = new Set<string>();

  for (const line of rawJsonl.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("{")) continue;
    try {
      const entry = JSON.parse(trimmed) as {
        type?: string;
        timestamp?: string;
        message?: { role?: string; content?: unknown };
      };
      if (entry.timestamp) {
        firstTimestamp ??= entry.timestamp;
        lastTimestamp = entry.timestamp;
      }
      const role = entry.message?.role ?? entry.type;
      if (role !== "user" && role !== "assistant") continue;
      messageCount++;
      const { text, tools } = extractTextFromMessageContent(entry.message?.content);
      toolCallCount += tools.length;
      if (role === "user" && text && !text.startsWith("<command-") && !text.startsWith("[Request interrupted")) {
        userPromptCount++;
        const compact = text.replace(/\s+/g, " ").slice(0, 240);
        const key = String(Bun.hash(compact));
        if (!seenPrompts.has(key)) {
          seenPrompts.add(key);
          prompts.push(compact);
        }
      }
    } catch {
      // Ignore truncated or non-message JSONL lines.
    }
  }

  return {
    sessionId,
    projectSlug,
    messageCount,
    userPromptCount,
    toolCallCount,
    firstTimestamp,
    lastTimestamp,
    prompts,
  };
}

export function scanClaudeProjectsDir(projectsRoot: string): ClaudeImportReport {
  const sessions: ImportedSession[] = [];
  const memories: ImportedMemoryNote[] = [];
  const globalPromptHashes = new Set<string>();
  let projectsScanned = 0;
  let totalToolCalls = 0;

  if (!existsSync(projectsRoot)) {
    return {
      projectsScanned: 0,
      sessionsImported: 0,
      uniquePrompts: 0,
      totalToolCalls: 0,
      memoryNotesImported: 0,
      geminiSyncedNotes: 0,
      sessions,
      memories,
    };
  }

  for (const projEntry of readdirSync(projectsRoot, { withFileTypes: true })) {
    if (!projEntry.isDirectory()) continue;
    projectsScanned++;
    const projSlug = projEntry.name;
    const projDir = join(projectsRoot, projSlug);

    for (const fileEntry of readdirSync(projDir, { withFileTypes: true })) {
      if (fileEntry.isFile() && fileEntry.name.endsWith(".jsonl")) {
        const full = join(projDir, fileEntry.name);
        try {
          if (statSync(full).size > 32 * 1024 * 1024) continue;
          const raw = readFileSync(full, "utf8");
          const parsed = parseClaudeJsonl(fileEntry.name.replace(/\.jsonl$/, ""), projSlug, raw);
          if (parsed.messageCount > 0) {
            sessions.push(parsed);
            totalToolCalls += parsed.toolCallCount;
            for (const p of parsed.prompts) globalPromptHashes.add(String(Bun.hash(p)));
          }
        } catch {}
      }
    }

    const memDir = join(projDir, "memory");
    if (existsSync(memDir)) {
      for (const memEntry of readdirSync(memDir, { withFileTypes: true })) {
        if (!memEntry.isFile() || !memEntry.name.endsWith(".md")) continue;
        const full = join(memDir, memEntry.name);
        try {
          const text = readFileSync(full, "utf8");
          const heading = /^#\s+(.+)$/m.exec(text)?.[1]?.trim() ?? memEntry.name.replace(/\.md$/, "");
          memories.push({
            projectSlug: projSlug,
            fileName: memEntry.name,
            title: heading,
            contentHash: Bun.hash(text).toString(16),
            byteLength: Buffer.byteLength(text, "utf8"),
          });
        } catch {}
      }
    }
  }

  sessions.sort((a, b) => (b.lastTimestamp ?? "").localeCompare(a.lastTimestamp ?? ""));
  memories.sort((a, b) => a.projectSlug.localeCompare(b.projectSlug) || a.fileName.localeCompare(b.fileName));

  return {
    projectsScanned,
    sessionsImported: sessions.length,
    uniquePrompts: globalPromptHashes.size,
    totalToolCalls,
    memoryNotesImported: memories.length,
    geminiSyncedNotes: 0,
    sessions,
    memories,
  };
}

export function writeSessionSqliteIndex(dbPath: string, report: ClaudeImportReport, projectsRoot?: string): void {
  const db = new Database(dbPath, { create: true });
  try {
    db.run(`
      CREATE TABLE IF NOT EXISTS claude_sessions (
        session_id TEXT PRIMARY KEY,
        project_slug TEXT NOT NULL,
        message_count INTEGER NOT NULL,
        user_prompt_count INTEGER NOT NULL,
        tool_call_count INTEGER NOT NULL,
        first_timestamp TEXT,
        last_timestamp TEXT,
        prompts_json TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS claude_memories (
        id TEXT PRIMARY KEY,
        project_slug TEXT NOT NULL,
        file_name TEXT NOT NULL,
        title TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        body TEXT NOT NULL
      );
    `);
    const upsertSession = db.prepare(
      `INSERT OR REPLACE INTO claude_sessions VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)`,
    );
    for (const s of report.sessions) {
      upsertSession.run(
        s.sessionId,
        s.projectSlug,
        s.messageCount,
        s.userPromptCount,
        s.toolCallCount,
        s.firstTimestamp ?? null,
        s.lastTimestamp ?? null,
        JSON.stringify(s.prompts),
      );
    }
    upsertSession.finalize();
    const upsertMem = db.prepare(`INSERT OR REPLACE INTO claude_memories VALUES (?1, ?2, ?3, ?4, ?5, ?6)`);
    for (const m of report.memories) {
      let body = "";
      if (projectsRoot) {
        try {
          body = readFileSync(join(projectsRoot, m.projectSlug, "memory", m.fileName), "utf8");
        } catch {}
      }
      upsertMem.run(`${m.projectSlug}/${m.fileName}`, m.projectSlug, m.fileName, m.title, m.contentHash, body);
    }
    upsertMem.finalize();
  } finally {
    db.close();
  }
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const dirIdx = args.indexOf("--dir");
  const projectsRoot = dirIdx >= 0 && args[dirIdx + 1] ? args[dirIdx + 1]! : join(homedir(), ".claude", "projects");
  const report = scanClaudeProjectsDir(projectsRoot);

  const outJson = privatePath("claude-sessions/index.json", repoRoot, true);
  const outSqlite = privatePath("claude-sessions/sessions.sqlite", repoRoot, true);
  const outMd = privatePath("claude-sessions/summary.md", repoRoot, true);
  mkdirSync(join(outJson, ".."), { recursive: true });

  writeSessionSqliteIndex(outSqlite, report, projectsRoot);
  await Bun.write(outJson, JSON.stringify(report, null, 2) + "\n");
  await Bun.write(
    outMd,
    [
      "# Imported Claude Code Sessions",
      "",
      `- Projects scanned: ${report.projectsScanned}`,
      `- Sessions indexed: ${report.sessionsImported}`,
      `- Unique user prompts: ${report.uniquePrompts}`,
      `- Tool calls recorded: ${report.totalToolCalls}`,
      `- Memory notes indexed: ${report.memoryNotesImported}`,
      "",
    ].join("\n"),
  );

  if (args.includes("--json")) {
    console.log(JSON.stringify({ ...report, sessions: undefined, memories: undefined }, null, 2));
  } else {
    console.log(
      `imported ${report.sessionsImported} Claude session(s), ${report.uniquePrompts} unique prompt(s), and ${report.memoryNotesImported} memory note(s) into ${basename(outSqlite)}`,
    );
  }
}
