#!/usr/bin/env bun
// Syncs every fork of an organization with its upstream through the merge-upstream API.
//   bun scripts/forks-sync.ts [--org aphrody-labs] [--only a,b] [--limit N] [--concurrency N] [--interval MS]
//                             [--resume [HOURS]] [--dry-run] [--state FILE] [--out DIR]
// Forks are enumerated with GraphQL (parent and default branch heads included). Forks whose head already equals the parent's are
// not called. Everything else gets POST /repos/{fork}/merge-upstream on the fork's default branch.
// Outcomes: fast-forward, merge, up-to-date, conflict (409, ahead/behind recorded), parent-missing, branch-mismatch,
// fork-archived, error. An archived parent is a flag (parentArchived) next to the outcome.
// State is written after every few forks (resumable with --resume), the report goes to <out>/report.json and report.md.
// Token: GH_TOKEN, GITHUB_TOKEN, else `gh auth token`. Rate limits: primary (x-ratelimit-reset) and secondary (retry-after) are
// waited out; mutating calls are spaced by --interval and at most --concurrency run at once.
import {
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { REPO_ROOT, runCmd } from "./lib.ts";

export type Outcome =
  | "fast-forward"
  | "merge"
  | "up-to-date"
  | "conflict"
  | "parent-missing"
  | "branch-mismatch"
  | "fork-archived"
  | "error";

export const OUTCOMES: Outcome[] = [
  "fast-forward",
  "merge",
  "up-to-date",
  "conflict",
  "parent-missing",
  "branch-mismatch",
  "fork-archived",
  "error",
];

export interface Fork {
  name: string;
  branch: string | null;
  oid: string | null;
  archived: boolean;
  private: boolean;
  parent: {
    name: string;
    branch: string | null;
    oid: string | null;
    archived: boolean;
  } | null;
}

export interface Result {
  fork: string;
  outcome: Outcome;
  branch: string | null;
  parent: string | null;
  parentArchived: boolean;
  before: string | null;
  ahead?: number;
  behind?: number;
  message?: string;
  at: string;
}

export interface Reply {
  status: number;
  body: any;
}

export interface Gh {
  graphql(query: string, vars: Record<string, unknown>): Promise<any>;
  post(path: string, body: unknown): Promise<Reply>;
  get(path: string): Promise<Reply>;
}

const FORKS_QUERY = `query($org:String!,$after:String){organization(login:$org){repositories(first:40,after:$after,isFork:true,orderBy:{field:NAME,direction:ASC}){totalCount pageInfo{hasNextPage endCursor} nodes{nameWithOwner isArchived isPrivate defaultBranchRef{name target{oid}} parent{nameWithOwner isArchived defaultBranchRef{name target{oid}}}}}}}`;

export async function listForks(gh: Gh, org: string): Promise<Fork[]> {
  const out: Fork[] = [];
  let after: string | null = null;
  for (;;) {
    const data = await gh.graphql(FORKS_QUERY, { org, after });
    const repos = data.organization.repositories;
    for (const n of repos.nodes) {
      out.push({
        name: n.nameWithOwner,
        branch: n.defaultBranchRef?.name ?? null,
        oid: n.defaultBranchRef?.target?.oid ?? null,
        archived: n.isArchived,
        private: n.isPrivate,
        parent: n.parent
          ? {
              name: n.parent.nameWithOwner,
              branch: n.parent.defaultBranchRef?.name ?? null,
              oid: n.parent.defaultBranchRef?.target?.oid ?? null,
              archived: n.parent.isArchived,
            }
          : null,
      });
    }
    if (!repos.pageInfo.hasNextPage) return out;
    after = repos.pageInfo.endCursor;
  }
}

/** The decision that needs no call: a result, or undefined when merge-upstream must be asked. */
export function preflight(f: Fork): Omit<Result, "at"> | undefined {
  const base = {
    fork: f.name,
    branch: f.branch,
    parent: f.parent?.name ?? null,
    parentArchived: f.parent?.archived ?? false,
    before: f.oid,
  };
  if (!f.parent) return { ...base, outcome: "parent-missing" };
  if (f.archived) return { ...base, outcome: "fork-archived" };
  if (!f.branch || !f.parent.branch || f.branch !== f.parent.branch) {
    return {
      ...base,
      outcome: "branch-mismatch",
      message: `fork ${f.branch ?? "none"}, parent ${f.parent.branch ?? "none"}`,
    };
  }
  if (f.oid && f.oid === f.parent.oid) return { ...base, outcome: "up-to-date" };
  return undefined;
}

export function classify(
  reply: Reply,
): { outcome: Outcome; message?: string } {
  const msg = typeof reply.body?.message === "string" ? reply.body.message : "";
  if (reply.status === 200) {
    const t = reply.body?.merge_type;
    if (t === "fast-forward") return { outcome: "fast-forward", message: msg };
    if (t === "merge") return { outcome: "merge", message: msg };
    return { outcome: "up-to-date", message: msg };
  }
  if (reply.status === 409) return { outcome: "conflict", message: msg };
  if (reply.status === 422 && /not behind/i.test(msg)) {
    return { outcome: "up-to-date", message: msg };
  }
  return { outcome: "error", message: `HTTP ${reply.status} ${msg}`.trim() };
}

export interface Options {
  org: string;
  only?: Set<string>;
  limit?: number;
  concurrency: number;
  intervalMs: number;
  resumeHours?: number;
  dryRun: boolean;
  state: string;
  out: string;
}

export interface State {
  org: string;
  startedAt: string;
  results: Record<string, Result>;
}

const ago = (iso: string): number => (Date.now() - Date.parse(iso)) / 3.6e6;

/** Serializes the start of calls: at most one start per intervalMs. */
export function spacer(intervalMs: number): () => Promise<void> {
  let next = 0;
  return async () => {
    const now = Date.now();
    const at = Math.max(now, next);
    next = at + intervalMs;
    if (at > now) await Bun.sleep(at - now);
  };
}

export async function syncAll(
  gh: Gh,
  forks: Fork[],
  opts: Pick<Options, "concurrency" | "intervalMs" | "dryRun">,
  state: State,
  save: () => void,
  log: (line: string) => void = () => {},
): Promise<void> {
  const wait = spacer(opts.intervalMs);
  const queue = [...forks];
  let done = 0;
  const record = (r: Omit<Result, "at">) => {
    state.results[r.fork] = { ...r, at: new Date().toISOString() };
    if (++done % 25 === 0) save();
  };
  const worker = async () => {
    for (let f = queue.shift(); f; f = queue.shift()) {
      const pre = preflight(f);
      if (pre) {
        record(pre);
        continue;
      }
      const base = {
        fork: f.name,
        branch: f.branch,
        parent: f.parent!.name,
        parentArchived: f.parent!.archived,
        before: f.oid,
      };
      if (opts.dryRun) {
        log(`would call  ${f.name}`);
        continue;
      }
      await wait();
      let r: Omit<Result, "at">;
      try {
        const reply = await gh.post(`/repos/${f.name}/merge-upstream`, {
          branch: f.branch,
        });
        const c = classify(reply);
        r = { ...base, ...c };
        if (c.outcome === "conflict") {
          const cmp = await gh.get(
            `/repos/${f.name}/compare/${f.parent!.name.split("/")[0]}:${f.branch}...${f.name.split("/")[0]}:${f.branch}`,
          );
          if (cmp.status === 200) {
            r.ahead = cmp.body.ahead_by;
            r.behind = cmp.body.behind_by;
          }
        }
      } catch (e) {
        r = { ...base, outcome: "error", message: String((e as Error).message ?? e) };
      }
      record(r);
      log(`${r.outcome.padEnd(14)} ${r.fork}${r.message ? `  ${r.message.slice(0, 90)}` : ""}`);
    }
  };
  await Promise.all(Array.from({ length: opts.concurrency }, worker));
  save();
}

export function summarize(results: Result[]): Record<Outcome, number> {
  const c = Object.fromEntries(OUTCOMES.map((o) => [o, 0])) as Record<Outcome, number>;
  for (const r of results) c[r.outcome]++;
  return c;
}

export function renderMarkdown(org: string, results: Result[], at: string): string {
  const counts = summarize(results);
  const lines = [
    `# Fork sync ${org}`,
    "",
    `${results.length} forks, ${at}.`,
    "",
    "| outcome | count |",
    "| --- | ---: |",
    ...OUTCOMES.map((o) => `| ${o} | ${counts[o]} |`),
    `| parent archived (flag) | ${results.filter((r) => r.parentArchived).length} |`,
    "",
  ];
  const section = (title: string, rows: Result[], cols: (r: Result) => string[], head: string[]) => {
    if (!rows.length) return;
    lines.push(`## ${title} (${rows.length})`, "", `| ${head.join(" | ")} |`, `| ${head.map(() => "---").join(" | ")} |`);
    for (const r of rows) lines.push(`| ${cols(r).join(" | ")} |`);
    lines.push("");
  };
  const by = (o: Outcome) => results.filter((r) => r.outcome === o).sort((a, b) => a.fork.localeCompare(b.fork));
  section("Conflicts", by("conflict"), (r) => [r.fork, r.parent ?? "", String(r.ahead ?? "?"), String(r.behind ?? "?")], ["fork", "parent", "ahead", "behind"]);
  section("Errors", by("error"), (r) => [r.fork, (r.message ?? "").replaceAll("|", "/")], ["fork", "message"]);
  section("Default branch mismatch", by("branch-mismatch"), (r) => [r.fork, r.parent ?? "", r.message ?? ""], ["fork", "parent", "branches"]);
  section("Parent missing", by("parent-missing"), (r) => [r.fork], ["fork"]);
  section("Fork archived", by("fork-archived"), (r) => [r.fork, r.parent ?? ""], ["fork", "parent"]);
  section("Merged", [...by("merge"), ...by("fast-forward")], (r) => [r.fork, r.outcome, r.parent ?? ""], ["fork", "outcome", "parent"]);
  section("Parent archived", results.filter((r) => r.parentArchived).sort((a, b) => a.fork.localeCompare(b.fork)), (r) => [r.fork, r.parent ?? "", r.outcome], ["fork", "parent", "outcome"]);
  return lines.join("\n");
}

export function parseArgs(argv: string[]): Options {
  const o: Options = {
    org: "aphrody-labs",
    concurrency: 4,
    intervalMs: 1200,
    dryRun: false,
    state: join(REPO_ROOT, "state", "forks-sync.json"),
    out: join(REPO_ROOT, "state", "forks-sync"),
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]!;
    const val = () => {
      const v = argv[++i];
      if (v === undefined) throw new Error(`${a} needs a value`);
      return v;
    };
    if (a === "--org") o.org = val();
    else if (a === "--only") o.only = new Set(val().split(","));
    else if (a === "--limit") o.limit = Number(val());
    else if (a === "--concurrency") o.concurrency = Math.max(1, Number(val()));
    else if (a === "--interval") o.intervalMs = Number(val());
    else if (a === "--resume") {
      const n = Number(argv[i + 1]);
      if (argv[i + 1] !== undefined && Number.isFinite(n)) i++;
      o.resumeHours = Number.isFinite(n) ? n : 12;
    } else if (a === "--dry-run") o.dryRun = true;
    else if (a === "--state") o.state = val();
    else if (a === "--out") o.out = val();
    else throw new Error(`unknown argument ${a}`);
  }
  return o;
}

export function loadToken(): string {
  const t = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  if (t) return t;
  const r = runCmd(["gh", "auth", "token"]);
  if (!r.ok || !r.out) throw new Error("no GitHub token: set GH_TOKEN or run gh auth login");
  return r.out;
}

/** REST and GraphQL over fetch; primary and secondary rate limits are waited out. */
export function http(token: string, log: (l: string) => void = () => {}): Gh {
  const call = async (method: string, path: string, body?: unknown): Promise<Reply> => {
    for (let attempt = 0; ; attempt++) {
      const res = await fetch(`https://api.github.com${path}`, {
        method,
        headers: {
          authorization: `Bearer ${token}`,
          accept: "application/vnd.github+json",
          "x-github-api-version": "2022-11-28",
          "user-agent": "aphrody-forks-sync",
          ...(body ? { "content-type": "application/json" } : {}),
        },
        body: body ? JSON.stringify(body) : undefined,
      });
      const text = await res.text();
      let json: any = {};
      try {
        json = text ? JSON.parse(text) : {};
      } catch {
        json = { message: text.slice(0, 200) };
      }
      const limited =
        res.status === 429 ||
        (res.status === 403 && (res.headers.get("retry-after") || res.headers.get("x-ratelimit-remaining") === "0" || /rate limit|abuse/i.test(json.message ?? "")));
      if (limited && attempt < 8) {
        const ra = Number(res.headers.get("retry-after"));
        const reset = Number(res.headers.get("x-ratelimit-reset"));
        const secs = ra > 0 ? ra : res.headers.get("x-ratelimit-remaining") === "0" && reset ? Math.max(1, reset - Date.now() / 1000) : 60 * (attempt + 1);
        log(`rate limited on ${method} ${path}, waiting ${Math.ceil(secs)}s`);
        await Bun.sleep(Math.min(secs, 3700) * 1000 + 500);
        continue;
      }
      if (res.status >= 500 && attempt < 6) {
        await Bun.sleep(3000 * (attempt + 1));
        continue;
      }
      return { status: res.status, body: json };
    }
  };
  return {
    async graphql(query, vars) {
      const r = await call("POST", "/graphql", { query, variables: vars });
      if (r.status !== 200 || r.body.errors) throw new Error(`graphql: ${JSON.stringify(r.body.errors ?? r.body).slice(0, 300)}`);
      return r.body.data;
    },
    post: (path, body) => call("POST", path, body),
    get: (path) => call("GET", path),
  };
}

function loadState(opts: Options): State {
  const fresh: State = { org: opts.org, startedAt: new Date().toISOString(), results: {} };
  if (opts.resumeHours === undefined || !existsSync(opts.state)) return fresh;
  const s = JSON.parse(readFileSync(opts.state, "utf8")) as State;
  return s.org === opts.org ? s : fresh;
}

export async function run(argv: string[], gh?: Gh): Promise<number> {
  const opts = parseArgs(argv);
  const log = (l: string) => console.log(l);
  const client = gh ?? http(loadToken(), log);
  let forks = await listForks(client, opts.org);
  log(`${forks.length} forks in ${opts.org}`);
  if (opts.only) forks = forks.filter((f) => opts.only!.has(f.name) || opts.only!.has(f.name.split("/")[1]!));
  if (opts.limit) forks = forks.slice(0, opts.limit);

  const state = loadState(opts);
  const prior = state.results;
  const fresh = (r: Result | undefined) =>
    opts.resumeHours !== undefined && r !== undefined && r.outcome !== "error" && ago(r.at) < opts.resumeHours;
  const todo = forks.filter((f) => !fresh(prior[f.name]));
  if (todo.length < forks.length) log(`resume: ${forks.length - todo.length} forks already recorded`);

  mkdirSync(join(opts.state, ".."), { recursive: true });
  const save = () => {
    const tmp = `${opts.state}.tmp`;
    writeFileSync(tmp, JSON.stringify(state));
    renameSync(tmp, opts.state);
  };
  await syncAll(client, todo, opts, state, save, log);

  const names = new Set(forks.map((f) => f.name));
  const results = Object.values(state.results).filter((r) => names.has(r.fork)).sort((a, b) => a.fork.localeCompare(b.fork));
  const at = new Date().toISOString();
  mkdirSync(opts.out, { recursive: true });
  writeFileSync(join(opts.out, "report.json"), JSON.stringify({ org: opts.org, at, counts: summarize(results), parentArchived: results.filter((r) => r.parentArchived).length, results }, null, 1));
  writeFileSync(join(opts.out, "report.md"), renderMarkdown(opts.org, results, at));
  const counts = summarize(results);
  log(OUTCOMES.map((o) => `${o}=${counts[o]}`).join(" "));
  log(`report: ${join(opts.out, "report.md")}`);
  return counts.error > 0 ? 1 : 0;
}

if (import.meta.main) {
  try {
    process.exit(await run(Bun.argv.slice(2)));
  } catch (e) {
    console.error((e as Error).message);
    process.exit(2);
  }
}
