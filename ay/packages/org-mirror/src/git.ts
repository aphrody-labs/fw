/** The git side of the replica: `git clone --mirror` / fetch, refs, and the commit history parsed from `git log`. Commits never come from the API. */
import type { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { tx } from "./db.ts";

export class GitError extends Error {
  constructor(readonly args: string[], readonly code: number, readonly stderr: string) {
    super(`git ${args.slice(0, 3).join(" ")} exited ${code}: ${stderr.trim().slice(0, 300)}`);
    this.name = "GitError";
  }
}

export interface RunOptions {
  cwd?: string;
  stdin?: string;
  env?: Record<string, string>;
}

/** Run git and return stdout as text. */
export async function git(args: string[], o: RunOptions = {}): Promise<string> {
  const p = Bun.spawn(["git", ...args], { cwd: o.cwd, stdin: o.stdin === undefined ? "ignore" : new Blob([o.stdin]), stdout: "pipe", stderr: "pipe", env: { ...process.env, GIT_TERMINAL_PROMPT: "0", ...o.env } });
  const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  if (code !== 0) throw new GitError(args, code, err);
  return out;
}

/** Authentication for private repositories travels through the environment, never through argv. */
function authEnv(token: string | undefined): Record<string, string> {
  if (!token) return {};
  const basic = Buffer.from(`x-access-token:${token}`).toString("base64");
  return { GIT_CONFIG_COUNT: "1", GIT_CONFIG_KEY_0: "http.extraheader", GIT_CONFIG_VALUE_0: `Authorization: Basic ${basic}` };
}

/** Clone the repository as a bare mirror, or fetch into an existing one (pruning deleted refs). */
export async function ensureMirror(url: string, dir: string, token?: string): Promise<{ cloned: boolean }> {
  const env = authEnv(token);
  if (existsSync(`${dir}/HEAD`)) {
    await git(["remote", "update", "--prune"], { cwd: dir, env });
    return { cloned: false };
  }
  mkdirSync(dirname(dir), { recursive: true });
  await git(["clone", "--mirror", "--quiet", url, dir], { env });
  return { cloned: true };
}

export interface Ref {
  kind: "branch" | "tag";
  name: string;
  sha: string;
  date: string | null;
}

/** Branches and tags with the commit they point at (annotated tags are peeled) and their date. */
export async function listRefs(dir: string): Promise<Ref[]> {
  const out = await git(["for-each-ref", "--format=%(refname)%09%(objectname)%09%(*objectname)%09%(creatordate:iso-strict)", "refs/heads", "refs/tags"], { cwd: dir });
  const refs: Ref[] = [];
  for (const line of out.split("\n")) {
    if (!line) continue;
    const [ref, sha, peeled, date] = line.split("\t") as [string, string, string, string];
    const branch = ref.startsWith("refs/heads/");
    refs.push({ kind: branch ? "branch" : "tag", name: ref.slice(branch ? 11 : 10), sha: peeled || sha, date: date || null });
  }
  return refs;
}

export async function countCommits(dir: string, rev: string): Promise<number | null> {
  try {
    return Number((await git(["rev-list", "--count", rev], { cwd: dir })).trim());
  } catch {
    return null;
  }
}

export interface CommitRow {
  sha: string;
  parents: string;
  authorName: string;
  authorEmail: string;
  authorDate: string;
  committerName: string;
  committerEmail: string;
  committerDate: string;
  subject: string;
  body: string;
  files: number | null;
  additions: number | null;
  deletions: number | null;
}

const FORMAT = "%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%s%x1f%b%x1d";
const iso = (unix: string) => new Date(Number(unix) * 1000).toISOString();
const BODY_CAP = 20_000;

/** One chunk of `git log --format=FORMAT --shortstat` output, starting after the record separator. */
export function parseCommit(chunk: string): CommitRow | undefined {
  const [head = "", stat = ""] = chunk.split("\x1d");
  const f = head.split("\x1f");
  if (f.length < 10 || !f[0]) return undefined;
  const num = (re: RegExp) => Number(stat.match(re)?.[1] ?? 0);
  const hasStat = /files? changed/.test(stat);
  return {
    sha: f[0], parents: f[1]!, authorName: f[2]!, authorEmail: f[3]!, authorDate: iso(f[4]!), committerName: f[5]!, committerEmail: f[6]!, committerDate: iso(f[7]!),
    subject: f[8]!, body: f.slice(9).join("\x1f").trim().slice(0, BODY_CAP),
    files: hasStat ? num(/(\d+) files? changed/) : null, additions: hasStat ? num(/(\d+) insertions?\(\+\)/) : null, deletions: hasStat ? num(/(\d+) deletions?\(-\)/) : null,
  };
}

/** Which of `shas` the repository still has (a force push can drop a previous tip). */
async function existing(dir: string, shas: string[]): Promise<string[]> {
  if (!shas.length) return [];
  const out = await git(["cat-file", "--batch-check"], { cwd: dir, stdin: shas.join("\n") + "\n" });
  return out.split("\n").filter(l => l && !l.endsWith("missing")).map(l => l.split(" ")[0]!);
}

/**
 * Stream the commits reachable from `tips` but not from `known` (the tips of the previous sync) and hand them over in batches.
 * Returns how many commits were read.
 */
export async function logCommits(dir: string, tips: string[], known: string[], each: (batch: CommitRow[]) => void, batchSize = 2000): Promise<number> {
  const old = await existing(dir, known);
  const revs = [...new Set(tips), ...old.map(s => `^${s}`)].join("\n") + "\n";
  if (!tips.length) return 0;
  const p = Bun.spawn(["git", "log", "--stdin", `--format=${FORMAT}`, "--shortstat", "--no-renames"], { cwd: dir, stdin: new Blob([revs]), stdout: "pipe", stderr: "pipe" });
  const errText = new Response(p.stderr).text();
  const decoder = new TextDecoder();
  let pending = "";
  let batch: CommitRow[] = [];
  let total = 0;
  const flush = (final: boolean) => {
    const parts = pending.split("\x1e");
    pending = final ? "" : parts.pop()!;
    for (const part of parts) {
      const row = part ? parseCommit(part) : undefined;
      if (row) batch.push(row);
    }
    if (batch.length >= batchSize || (final && batch.length)) {
      total += batch.length;
      each(batch);
      batch = [];
    }
  };
  for await (const chunk of p.stdout as unknown as AsyncIterable<Uint8Array>) {
    pending += decoder.decode(chunk, { stream: true });
    flush(false);
  }
  pending += decoder.decode();
  flush(true);
  const code = await p.exited;
  if (code !== 0) throw new GitError(["log"], code, await errText);
  return total;
}

export function storeRefs(db: Database, repo: string, refs: Ref[]): void {
  tx(db, () => {
    db.query("DELETE FROM refs WHERE repo = ?").run(repo);
    const ins = db.query("INSERT OR REPLACE INTO refs (repo, name, kind, sha, date) VALUES (?, ?, ?, ?, ?)");
    for (const r of refs) ins.run(repo, r.name, r.kind, r.sha, r.date);
  });
}

export function storeCommits(db: Database, repo: string, rows: CommitRow[]): void {
  const ins = db.query(
    `INSERT OR IGNORE INTO commits (repo, sha, parents, author_name, author_email, author_date, committer_name, committer_email, committer_date, subject, body, files, additions, deletions)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  );
  tx(db, () => {
    for (const c of rows) ins.run(repo, c.sha, c.parents, c.authorName, c.authorEmail, c.authorDate, c.committerName, c.committerEmail, c.committerDate, c.subject, c.body, c.files, c.additions, c.deletions);
  });
}

export interface GitSyncResult {
  cloned: boolean;
  commits: number;
  branches: number;
  tags: number;
  defaultBranchCommits: number | null;
}

/** Mirror the repository, record refs, then read only the commits the previous sync had not seen. */
export async function syncGit(db: Database, repo: string, url: string, dir: string, defaultBranch: string | null, token?: string): Promise<GitSyncResult> {
  const { cloned } = await ensureMirror(url, dir, token);
  const refs = await listRefs(dir);
  const known = db.query<{ sha: string }, [string]>("SELECT DISTINCT sha FROM refs WHERE repo = ?").all(repo).map(r => r.sha);
  const commits = await logCommits(dir, [...new Set(refs.map(r => r.sha))], known, rows => storeCommits(db, repo, rows));
  storeRefs(db, repo, refs);
  const defaultBranchCommits = defaultBranch ? await countCommits(dir, `refs/heads/${defaultBranch}`) : null;
  db.query("UPDATE repos SET default_commits_git = ? WHERE full_name = ?").run(defaultBranchCommits, repo);
  return { cloned, commits, branches: refs.filter(r => r.kind === "branch").length, tags: refs.filter(r => r.kind === "tag").length, defaultBranchCommits };
}

/** Every file path of a tree, recursively. */
export async function lsTree(dir: string, rev: string): Promise<string[]> {
  return (await git(["ls-tree", "-r", "--name-only", "-z", rev], { cwd: dir })).split("\0").filter(Boolean);
}

/** Blob ids of `rev:path` specs (undefined when the path does not exist at that rev). */
export async function blobIds(dir: string, specs: string[]): Promise<(string | undefined)[]> {
  if (!specs.length) return [];
  const out = await git(["cat-file", "--batch-check"], { cwd: dir, stdin: specs.join("\n") + "\n" });
  return out.trimEnd().split("\n").map(l => (l.endsWith(" missing") ? undefined : l.split(" ")[0]));
}

/** Contents of blobs by id, as text (null for binary-looking or over `max` bytes). */
export async function readBlobs(dir: string, ids: string[], max = 5_000_000): Promise<Map<string, string | null>> {
  const out = new Map<string, string | null>();
  if (!ids.length) return out;
  const p = Bun.spawn(["git", "cat-file", "--batch"], { cwd: dir, stdin: new Blob([ids.join("\n") + "\n"]), stdout: "pipe", stderr: "pipe" });
  const buf = new Uint8Array(await new Response(p.stdout).arrayBuffer());
  let pos = 0;
  const dec = new TextDecoder();
  for (const id of ids) {
    const nl = buf.indexOf(10, pos);
    const [, , size] = dec.decode(buf.subarray(pos, nl)).split(" ");
    const n = Number(size);
    pos = nl + 1;
    const data = buf.subarray(pos, pos + n);
    out.set(id, n > max || data.includes(0) ? null : dec.decode(data));
    pos += n + 1;
  }
  await p.exited;
  return out;
}
