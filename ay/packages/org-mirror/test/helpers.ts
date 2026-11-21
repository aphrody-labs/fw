import { mkdtempSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { git } from "../src/git.ts";
import type { FetchLike } from "../src/github.ts";

export const fixture = (name: string) => readFileSync(join(import.meta.dir, "fixtures", name), "utf8");
export const json = <T = any>(name: string): T => JSON.parse(fixture(name));

export const reply = (body: unknown, init: ResponseInit = {}) => new Response(typeof body === "string" ? body : JSON.stringify(body), { status: 200, ...init });

/** A fetch that answers from a queue (functions get the URL and request) and records the calls. */
export function queueFetch(...answers: (Response | ((url: string, init?: RequestInit) => Response | Promise<Response>))[]): FetchLike & { calls: { url: string; body?: any }[] } {
  const queue = [...answers];
  const calls: { url: string; body?: any }[] = [];
  const f = (async (url: string, init?: RequestInit) => {
    calls.push({ url, body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined });
    const next = queue.shift();
    if (!next) throw new Error(`unexpected request ${url}`);
    return typeof next === "function" ? next(url, init) : next;
  }) as FetchLike & { calls: typeof calls };
  f.calls = calls;
  return f;
}

export const noSleep = async () => {};

/** A temp directory. */
export const tmp = (prefix = "org-mirror-") => mkdtempSync(join(tmpdir(), prefix));

/** Build a real git repository from `{path: content}` commits; returns its directory. */
export async function makeRepo(commits: { message: string; files: Record<string, string>; tag?: string; date?: string }[]): Promise<string> {
  const dir = tmp("org-mirror-repo-");
  const env = { GIT_AUTHOR_NAME: "Test Author", GIT_AUTHOR_EMAIL: "author@example.test", GIT_COMMITTER_NAME: "Test Committer", GIT_COMMITTER_EMAIL: "committer@example.test" };
  await git(["init", "-q", "-b", "main"], { cwd: dir });
  for (const c of commits) {
    for (const [p, text] of Object.entries(c.files)) {
      mkdirSync(dirname(join(dir, p)), { recursive: true });
      writeFileSync(join(dir, p), text);
    }
    const d = c.date ?? "2026-01-01T00:00:00Z";
    await git(["add", "-A"], { cwd: dir });
    await git(["commit", "-q", "-m", c.message], { cwd: dir, env: { ...env, GIT_AUTHOR_DATE: d, GIT_COMMITTER_DATE: d } });
    if (c.tag) await git(["tag", c.tag], { cwd: dir });
  }
  return dir;
}
