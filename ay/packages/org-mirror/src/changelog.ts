/** Changelog files (CHANGELOG*, HISTORY*, CHANGES*, NEWS*) parsed into versions, at the default branch and at every tag. */
import type { Database } from "bun:sqlite";
import { blobIds, lsTree, readBlobs } from "./git.ts";
import { tx } from "./db.ts";

export interface Entry {
  version: string;
  date: string | null;
  body: string;
  /** 0 is the newest entry of the file. */
  position: number;
}

export const CHANGELOG_FILE = /^(changelog|history|changes|news|releases?)([-_]\w+)*(\.(md|markdown|mdx|txt|rst))?$/i;

/** Changelog paths among a list of repository paths. */
export function changelogPaths(paths: string[]): string[] {
  return paths.filter(p => {
    const base = p.split("/").pop()!;
    if (!CHANGELOG_FILE.test(base)) return false;
    return !/(^|\/)(node_modules|vendor|third_party|\.git)\//.test(p);
  });
}

const MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/** A date written in a heading, as YYYY-MM-DD, or null. */
export function headingDate(text: string): string | null {
  const iso = text.match(/\b(\d{4})[-/.](\d{1,2})[-/.](\d{1,2})\b/);
  if (iso) return `${iso[1]}-${iso[2]!.padStart(2, "0")}-${iso[3]!.padStart(2, "0")}`;
  const dmy = text.match(/\b(\d{1,2})[./](\d{1,2})[./](\d{4})\b/);
  if (dmy) return `${dmy[3]}-${dmy[2]!.padStart(2, "0")}-${dmy[1]!.padStart(2, "0")}`;
  const long = text.match(/\b(\d{1,2})(?:st|nd|rd|th)?\s+([A-Za-z]{3})[a-z]*\.?,?\s+(\d{4})\b/) ?? undefined;
  if (long) {
    const m = MONTHS.indexOf(long[2]!.toLowerCase());
    if (m >= 0) return `${long[3]}-${String(m + 1).padStart(2, "0")}-${long[1]!.padStart(2, "0")}`;
  }
  const us = text.match(/\b([A-Za-z]{3})[a-z]*\.?\s+(\d{1,2})(?:st|nd|rd|th)?,?\s+(\d{4})\b/);
  if (us) {
    const m = MONTHS.indexOf(us[1]!.toLowerCase());
    if (m >= 0) return `${us[3]}-${String(m + 1).padStart(2, "0")}-${us[2]!.padStart(2, "0")}`;
  }
  return null;
}

const VERSION = /^(?:[\w@/.-]+[\s@]+)?(?:version\s+|release\s+)?v?(\d+\.\d+(?:\.\d+)*(?:[-+][0-9A-Za-z.+-]+)?)(?![\w.]*[a-df-z])/i;

/** The version a heading names ("[1.2.3] - 2024-01-01", "v1.2.3", "@scope/pkg@1.2.3", "Unreleased"), or null. */
export function headingVersion(text: string): string | null {
  const t = text.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/[`*_]/g, "").replace(/^\[|\]/g, "").trim();
  if (/^unreleased\b/i.test(t)) return "Unreleased";
  const m = t.match(VERSION);
  return m ? m[1]! : null;
}

interface Heading { level: number; text: string; line: number }

function headings(lines: string[]): Heading[] {
  const out: Heading[] = [];
  let fence = false;
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i]!;
    if (/^\s*(```|~~~)/.test(l)) fence = !fence;
    if (fence) continue;
    const atx = l.match(/^(#{1,6})\s+(.+?)\s*#*\s*$/);
    if (atx) {
      out.push({ level: atx[1]!.length, text: atx[2]!, line: i });
      continue;
    }
    const under = lines[i + 1]?.match(/^(=+|-{3,})\s*$/);
    if (under && l.trim() && !/^\s*[-*+>]/.test(l)) out.push({ level: under[1]![0] === "=" ? 1 : 2, text: l.trim(), line: i });
  }
  return out;
}

/** Parse a changelog into one entry per version heading; the version level is the most common level among version headings. */
export function parseChangelog(text: string): Entry[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const hs = headings(lines);
  const versions = hs.map(h => ({ h, v: headingVersion(h.text) })).filter((x): x is { h: Heading; v: string } => x.v !== null);
  if (!versions.length) return [];
  const count = new Map<number, number>();
  for (const x of versions) count.set(x.h.level, (count.get(x.h.level) ?? 0) + 1);
  const level = [...count.entries()].sort((a, b) => b[1] - a[1] || a[0] - b[0])[0]![0];
  const marks = versions.filter(x => x.h.level === level);
  const entries: Entry[] = [];
  const seen = new Set<string>();
  marks.forEach((m, i) => {
    if (seen.has(m.v)) return;
    seen.add(m.v);
    const start = m.h.line + (/^(=+|-{3,})\s*$/.test(lines[m.h.line + 1] ?? "") ? 2 : 1);
    const nextMark = marks[i + 1]?.h.line ?? lines.length;
    const stop = hs.find(h => h.line >= start && h.level <= level && h.line <= nextMark)?.line ?? nextMark;
    entries.push({ version: m.v, date: headingDate(m.h.text), body: lines.slice(start, Math.min(stop, nextMark)).join("\n").trim(), position: entries.length });
  });
  return entries;
}

/** Highest-precedence label of the newest released entry (skipping Unreleased). */
export const latestVersion = (entries: Entry[]) => entries.find(e => e.version !== "Unreleased")?.version ?? null;

/**
 * Record the changelogs of a repository: the files at the default branch and, for every tag not seen before, the same paths
 * at that tag. Each distinct blob is parsed once; the default branch wins when a version appears in several.
 */
export async function syncChangelogs(db: Database, repo: string, dir: string, defaultBranch: string | null): Promise<{ files: number; entries: number }> {
  const head = defaultBranch ? `refs/heads/${defaultBranch}` : "HEAD";
  const paths = changelogPaths(await lsTree(dir, head).catch(() => []));
  if (!paths.length) return { files: 0, entries: 0 };
  const done = new Set(db.query<{ ref: string }, [string]>("SELECT DISTINCT ref FROM changelog_files WHERE repo = ?").all(repo).map(r => r.ref));
  const tags = db.query<{ name: string }, [string]>("SELECT name FROM refs WHERE repo = ? AND kind = 'tag' ORDER BY date").all(repo).map(r => r.name).filter(t => !done.has(`tag:${t}`));
  const targets = [{ ref: `branch:${defaultBranch ?? "HEAD"}`, rev: head }, ...tags.map(t => ({ ref: `tag:${t}`, rev: `refs/tags/${t}` }))];
  const specs = targets.flatMap(t => paths.map(p => ({ ...t, path: p, spec: `${t.rev}:${p}` })));
  const ids = await blobIds(dir, specs.map(s => s.spec));
  const fresh = [...new Set(ids.filter((id): id is string => !!id))];
  const texts = await readBlobs(dir, fresh);
  const parsed = new Map(fresh.map(id => [id, texts.get(id) ? parseChangelog(texts.get(id)!) : []]));
  let entries = 0;
  tx(db, () => {
    db.query("DELETE FROM changelog_files WHERE repo = ? AND ref LIKE 'branch:%'").run(repo);
    const file = db.query("INSERT OR REPLACE INTO changelog_files (repo, ref, path, blob, entries, latest) VALUES (?, ?, ?, ?, ?, ?)");
    const entry = db.query("INSERT OR IGNORE INTO changelog_entries (repo, path, version, date, body, source_ref, position) VALUES (?, ?, ?, ?, ?, ?, ?)");
    const replace = db.query("INSERT OR REPLACE INTO changelog_entries (repo, path, version, date, body, source_ref, position) VALUES (?, ?, ?, ?, ?, ?, ?)");
    specs.forEach((s, i) => {
      const id = ids[i];
      if (!id) return;
      const es = parsed.get(id);
      file.run(repo, s.ref, s.path, id, es?.length ?? 0, es ? latestVersion(es) : null);
      if (!es) return;
      const head = s.ref.startsWith("branch:");
      for (const e of es) {
        (head ? replace : entry).run(repo, s.path, e.version, e.date, e.body, s.ref, e.position);
        entries++;
      }
    });
  });
  return { files: paths.length, entries };
}
