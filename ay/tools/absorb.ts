#!/usr/bin/env bun
// Absorb the community's work into our forks: rank open upstream PRs and active forks, then merge only what builds
// and passes the tests. Hosts come from ay.toml [hosts], the work volume from tools/disks.ts, every decision is
// appended to docs/research/absorbed.toml.
//   bun tools/absorb.ts candidates <owner/repo> [--green] [--rust] [--fix] [--max 90] [--forks 10] [--top 15] [--json]
//   bun tools/absorb.ts try <fork-checkout> <pr-number|fork:branch> --test "<cmd>" [--host fr1] [--timeout 5400]
//   bun tools/absorb.ts auto <owner/repo> --test "<cmd>" [--top 5] [--green] [--host fr1] [--fork <name>]
//   bun tools/absorb.ts skip <owner/repo> <pr-number> --reason "<why>"
// <fork-checkout> is a path on the build host, or a bare repository name (placed under <work volume>/absorb/<name>).
// The checkout is merged on its default branch with FETCH_HEAD; no branch, no force-push, no stash, no reset.
import { appendFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { bestForWork, parseDf } from "./disks.ts";
import { hostWork, loadHosts, type Role } from "./hosts.ts";

const root = resolve(import.meta.dir, "..");
const LEDGER = join(root, "docs/research/absorbed.toml");
const SINCE = "2026-01-01";

export type Ci = "success" | "failure" | "pending" | "none";
export type Kind = "fix" | "feature" | "other";
export type Candidate = {
  number: number;
  title: string;
  url: string;
  author: string;
  updatedAt: string;
  draft: boolean;
  mergeable: "MERGEABLE" | "CONFLICTING" | "UNKNOWN";
  ci: Ci;
  review: "APPROVED" | "CHANGES_REQUESTED" | "REVIEW_REQUIRED" | "none";
  approvals: number;
  reactions: number;
  labels: string[];
  additions: number;
  deletions: number;
  files: number;
  rust: boolean;
  kind: Kind;
  score: number;
};
export type ForkAhead = { fork: string; branch: string; pushed: string; ahead: number; behind: number; url: string };
export type Target = { kind: "pr"; number: number } | { kind: "branch"; repo: string | null; owner: string; branch: string };
export type Row = {
  repo: string;
  fork: string;
  source: string;
  title: string;
  result: "merged" | "conflict" | "tests-failed" | "skipped";
  test: string;
  date: string;
  detail?: string;
};

// ---------- pure parts ----------

/** `123`, `#123` or a PR URL -> PR; `owner:branch` or `owner/repo:branch` -> a fork branch. */
export function parseTarget(spec: string): Target {
  const pr = spec.match(/^#?(\d+)$/) ?? spec.match(/\/pull\/(\d+)/);
  if (pr) return { kind: "pr", number: Number(pr[1]) };
  const m = spec.match(/^([\w.-]+)(?:\/([\w.-]+))?:(.+)$/);
  if (!m) throw new Error(`not a PR number or fork:branch: ${spec}`);
  return { kind: "branch", owner: m[1]!, repo: m[2] ?? null, branch: m[3]! };
}

export function classify(title: string, labels: string[]): Kind {
  const text = `${title} ${labels.join(" ")}`;
  if (/^(docs?|ci|chore|style|release)(\(.*?\))?:/i.test(title) || /\btypo\b/i.test(title)) return "other";
  if (/\b(fix(es|ed)?|bug|crash(es)?|leak|panic|segfault|regression|overflow|race|deadlock|ub)\b/i.test(text)) return "fix";
  if (/\b(feat(ure)?|add|support|implement|introduce|new)\b/i.test(text)) return "feature";
  return "other";
}

export function ciState(rollup: string | null | undefined): Ci {
  switch (rollup) {
    case "SUCCESS":
      return "success";
    case "FAILURE":
    case "ERROR":
      return "failure";
    case "PENDING":
    case "EXPECTED":
      return "pending";
    default:
      return "none";
  }
}

/** Bug fixes with green CI, approvals or reactions and a small diff rank first; drafts and conflicts rank last. */
export function score(c: Omit<Candidate, "score">): number {
  let s = 0;
  if (c.kind === "fix") s += 4;
  s += { success: 4, pending: 0, none: -1, failure: -6 }[c.ci];
  if (c.review === "APPROVED") s += 3;
  if (c.review === "CHANGES_REQUESTED") s -= 4;
  s += Math.min(c.approvals, 3);
  s += Math.min(c.reactions, 5);
  if (c.mergeable === "MERGEABLE") s += 2;
  if (c.mergeable === "CONFLICTING") s -= 8;
  if (c.draft) s -= 10;
  s -= 1.5 * Math.log10(1 + c.additions + c.deletions);
  return Math.round(s * 100) / 100;
}

export function rank(cs: Omit<Candidate, "score">[]): Candidate[] {
  return cs
    .map(c => ({ ...c, score: score(c) }))
    .sort((a, b) => b.score - a.score || b.updatedAt.localeCompare(a.updatedAt));
}

export function shq(s: string): string {
  return `'${s.replaceAll("'", `'\\''`)}'`;
}

export function tomlString(s: string): string {
  const esc = s
    .replaceAll("\\", "\\\\")
    .replaceAll('"', '\\"')
    .replaceAll("\n", "\\n")
    .replaceAll("\r", "\\r")
    .replaceAll("\t", "\\t");
  return `"${esc}"`;
}

export function ledgerRow(r: Row): string {
  const lines = [
    "[[merge]]",
    `repo = ${tomlString(r.repo)}`,
    `fork = ${tomlString(r.fork)}`,
    `source = ${tomlString(r.source)}`,
    `title = ${tomlString(r.title)}`,
    `result = ${tomlString(r.result)}`,
    `test = ${tomlString(r.test)}`,
    `date = ${r.date}`,
  ];
  if (r.detail) lines.push(`detail = ${tomlString(r.detail)}`);
  return `\n${lines.join("\n")}\n`;
}

export function appendLedger(row: Row, path = LEDGER): void {
  if (!existsSync(path)) writeFileSync(path, "# Every absorb decision, appended by tools/absorb.ts.\nversion = 1\n");
  appendFileSync(path, ledgerRow(row));
}

export function mergeMessage(upstream: string, t: { number?: number; title: string; url: string }): string {
  const head = t.number ? `Merge ${upstream}#${t.number}: ${t.title}` : `Merge ${t.title}`;
  return `${head}\n\n${t.url}\n`;
}

/** Last non-empty lines of a build log, for the ledger. */
export function tail(text: string, n = 6): string {
  return text.trim().split("\n").filter(l => l.trim()).slice(-n).join("\n");
}

export type Plan = {
  dir: string;
  forkUrl: string;
  base: string;
  srcUrl: string;
  srcRef: string;
  test: string;
  message: string;
  timeout: number;
};

/** The bash script of one try: sync, fetch to FETCH_HEAD, merge on the default branch, test, commit and push. */
export function buildScript(p: Plan): string {
  const git = `git -c credential.helper='!gh auth git-credential'`;
  return `
set -u
export CARGO_INCREMENTAL=0
export ABSORB=${shq(p.dir.replace(/\/[^/]*$/, ""))}
export CARGO_TARGET_DIR=${shq(`${p.dir}.target`)}
mkdir -p "$(dirname ${shq(p.dir)})"
[ -d ${shq(p.dir)}/.git ] || ${git} clone ${shq(p.forkUrl)} ${shq(p.dir)} >&2 || { echo "@@RESULT skipped clone failed"; exit 0; }
cd ${shq(p.dir)}
git checkout ${shq(p.base)} >&2 || { echo "@@RESULT skipped checkout failed"; exit 0; }
${git} fetch origin ${shq(p.base)} >&2 && git merge --ff-only FETCH_HEAD >&2 || { echo "@@RESULT skipped fork ${p.base} cannot fast-forward from origin"; exit 0; }
${git} fetch ${shq(p.srcUrl)} ${shq(p.srcRef)} >&2 || { echo "@@RESULT skipped fetch of the candidate failed"; exit 0; }
CAND=$(git rev-parse FETCH_HEAD)
if git merge-base --is-ancestor "$CAND" HEAD; then echo "@@RESULT skipped already contained in ${p.base}"; exit 0; fi
if ! git merge --no-ff --no-commit "$CAND" >&2; then
  git merge --abort >&2; git checkout -- . >&2
  echo "@@RESULT conflict"; exit 0
fi
timeout ${p.timeout} bash -c ${shq(p.test)} >&2
RC=$?
if [ $RC -ne 0 ]; then
  git merge --abort >&2; git checkout -- . >&2
  echo "@@RESULT tests-failed exit $RC"; exit 0
fi
git commit -q -F - >&2 <<'MSG'
${p.message}
MSG
if ! ${git} push origin HEAD:${shq(p.base)} >&2; then echo "@@RESULT skipped tests passed but the push was rejected"; exit 0; fi
echo "@@RESULT merged"
`;
}

// ---------- gh and ssh ----------

async function run(cmd: string[], stdin?: string, onLine?: (l: string) => void): Promise<{ out: string; err: string; code: number }> {
  const p = Bun.spawn(cmd, { stdin: stdin === undefined ? "ignore" : "pipe", stdout: "pipe", stderr: "pipe" });
  if (stdin !== undefined) {
    p.stdin!.write(stdin);
    await p.stdin!.end();
  }
  const pump = async (s: ReadableStream<Uint8Array>) => {
    let acc = "";
    let buf = "";
    for await (const chunk of s as unknown as AsyncIterable<Uint8Array>) {
      const t = new TextDecoder().decode(chunk);
      acc += t;
      if (onLine) {
        buf += t;
        const parts = buf.split("\n");
        buf = parts.pop()!;
        parts.forEach(onLine);
      }
    }
    if (onLine && buf) onLine(buf);
    return acc;
  };
  const [out, err, code] = await Promise.all([pump(p.stdout), pump(p.stderr), p.exited]);
  return { out, err, code };
}

async function gh<T>(args: string[]): Promise<T> {
  const r = await run(["gh", ...args]);
  if (r.code !== 0) throw new Error(`gh ${args.join(" ")}: ${r.err.trim()}`);
  return JSON.parse(r.out) as T;
}

function config() {
  const ay = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as {
    orgs: { labs: string };
    hosts: { test: string; build: string };
  };
  return ay;
}

// ---------- candidates ----------

const QUERY = `query($q:String!,$after:String){search(query:$q,type:ISSUE,first:30,after:$after){pageInfo{hasNextPage endCursor}nodes{...on PullRequest{
number title url isDraft mergeable additions deletions changedFiles updatedAt reviewDecision author{login}
labels(first:10){nodes{name}} reactions{totalCount} latestOpinionatedReviews(first:10){nodes{state}}
commits(last:1){nodes{commit{statusCheckRollup{state}}}} files(first:100){nodes{path}}}}}}`;

type Node = {
  number: number; title: string; url: string; isDraft: boolean; mergeable: Candidate["mergeable"];
  additions: number; deletions: number; changedFiles: number; updatedAt: string; reviewDecision: string | null;
  author: { login: string } | null; labels: { nodes: { name: string }[] }; reactions: { totalCount: number };
  latestOpinionatedReviews: { nodes: { state: string }[] };
  commits: { nodes: { commit: { statusCheckRollup: { state: string } | null } }[] };
  files: { nodes: { path: string }[] };
};

export function toCandidate(n: Node): Omit<Candidate, "score"> {
  const labels = n.labels.nodes.map(l => l.name);
  const decision = n.reviewDecision as Candidate["review"] | null;
  return {
    number: n.number,
    title: n.title,
    url: n.url,
    author: n.author?.login ?? "ghost",
    updatedAt: n.updatedAt,
    draft: n.isDraft,
    mergeable: n.mergeable,
    ci: ciState(n.commits.nodes[0]?.commit.statusCheckRollup?.state),
    review: decision ?? "none",
    approvals: n.latestOpinionatedReviews.nodes.filter(r => r.state === "APPROVED").length,
    reactions: n.reactions.totalCount,
    labels,
    additions: n.additions,
    deletions: n.deletions,
    files: n.changedFiles,
    rust: n.files.nodes.some(f => /\.rs$|(^|\/)Cargo\.(toml|lock)$/.test(f.path)),
    kind: classify(n.title, labels),
  };
}

export async function fetchPrs(repo: string, opts: { green: boolean; max: number }): Promise<Candidate[]> {
  const q = `repo:${repo} is:pr is:open draft:false updated:>=${SINCE} sort:updated-desc${opts.green ? " status:success" : ""}`;
  const nodes: Node[] = [];
  let after: string | null = null;
  while (nodes.length < opts.max) {
    const args = ["api", "graphql", "-f", `query=${QUERY}`, "-f", `q=${q}`];
    if (after) args.push("-f", `after=${after}`);
    const r: { data: { search: { pageInfo: { hasNextPage: boolean; endCursor: string }; nodes: Node[] } } } = await gh(args);
    nodes.push(...r.data.search.nodes.filter(n => n?.number));
    if (!r.data.search.pageInfo.hasNextPage) break;
    after = r.data.search.pageInfo.endCursor;
  }
  return rank(nodes.slice(0, opts.max).map(toCandidate));
}

type ForkInfo = { full_name: string; owner: { login: string }; pushed_at: string; default_branch: string; stargazers_count: number };

export async function fetchForks(repo: string, limit: number): Promise<ForkAhead[]> {
  const info = await gh<{ default_branch: string }>(["api", `repos/${repo}`]);
  const forks = await gh<ForkInfo[]>(["api", `repos/${repo}/forks?per_page=100&sort=stargazers`]);
  const recent = forks
    .filter(f => f.pushed_at >= SINCE && !/^aphrody-/.test(f.owner.login))
    .sort((a, b) => b.pushed_at.localeCompare(a.pushed_at))
    .slice(0, limit);
  const out = await Promise.all(
    recent.map(async f => {
      try {
        const c = await gh<{ ahead_by: number; behind_by: number }>([
          "api",
          `repos/${repo}/compare/${info.default_branch}...${f.owner.login}:${f.default_branch}`,
        ]);
        return { fork: f.full_name, branch: f.default_branch, pushed: f.pushed_at, ahead: c.ahead_by, behind: c.behind_by, url: `https://github.com/${f.full_name}/tree/${f.default_branch}` };
      } catch {
        return null;
      }
    }),
  );
  return out.filter((f): f is ForkAhead => !!f && f.ahead > 0).sort((a, b) => b.ahead - a.ahead);
}

function fmt(c: Candidate): string {
  const flags = [c.kind, c.rust ? "rs" : "", c.mergeable.toLowerCase()].filter(Boolean).join(",");
  return `${String(c.score).padStart(6)}  #${c.number}  ci:${c.ci}  review:${c.review}${c.approvals ? `(${c.approvals})` : ""}  +${c.additions}/-${c.deletions}  react:${c.reactions}  [${flags}]  ${c.title}\n        ${c.url}`;
}

// ---------- try ----------

/** The work volume of a host: [paths.<role>].work of ay.toml for the role the alias serves, else the live df. */
async function workRoot(host: string): Promise<string> {
  const cfg = loadHosts();
  const role = (Object.keys(cfg.hosts) as Role[]).find(r => cfg.hosts[r] === host && cfg.paths[r]?.work);
  if (role) return hostWork(role, cfg);
  const r = await run(["ssh", "-o", "BatchMode=yes", host, "df -B1 --output=source,fstype,size,used,avail,target"]);
  if (r.code !== 0) throw new Error(`ssh ${host}: ${r.err.trim()}`);
  const best = bestForWork(parseDf(r.out));
  if (!best) throw new Error(`no work volume on ${host}`);
  return best.target;
}

export type TryOpts = { test: string; host: string; timeout: number };

export async function tryOne(checkout: string, spec: string, o: TryOpts): Promise<Row> {
  const cfg = config();
  const name = checkout.split("/").filter(Boolean).pop()!;
  const forkFull = `${cfg.orgs.labs}/${name}`;
  const fork = await gh<{ parent: { full_name: string } | null; default_branch: string }>(["api", `repos/${forkFull}`]);
  if (!fork.parent) throw new Error(`${forkFull} is not a fork`);
  const upstream = fork.parent.full_name;
  const dir = checkout.includes("/") ? checkout : `${await workRoot(o.host)}/absorb/${checkout}`;

  const t = parseTarget(spec);
  let source: string, title: string, srcUrl: string, srcRef: string, number: number | undefined;
  if (t.kind === "pr") {
    const pr = await gh<{ title: string; html_url: string; state: string }>(["api", `repos/${upstream}/pulls/${t.number}`]);
    source = pr.html_url;
    title = pr.title;
    srcUrl = `https://github.com/${upstream}.git`;
    srcRef = `pull/${t.number}/head`;
    number = t.number;
  } else {
    const repo = t.repo ?? `${t.owner}/${upstream.split("/")[1]}`;
    source = `https://github.com/${repo.includes("/") ? repo : `${t.owner}/${repo}`}/tree/${t.branch}`;
    title = `${t.owner}:${t.branch}`;
    srcUrl = `https://github.com/${repo.includes("/") ? repo : `${t.owner}/${repo}`}.git`;
    srcRef = t.branch;
  }

  const base: Omit<Row, "result"> = { repo: upstream, fork: forkFull, source, title, test: o.test, date: new Date().toISOString().slice(0, 10) };
  const script = buildScript({
    dir,
    forkUrl: `https://github.com/${forkFull}.git`,
    base: fork.default_branch,
    srcUrl,
    srcRef,
    test: o.test,
    message: mergeMessage(upstream, { number, title, url: source }),
    timeout: o.timeout,
  });
  console.error(`[absorb] ${upstream} ${number ? `#${number}` : spec} -> ${forkFull}:${fork.default_branch} on ${o.host}:${dir}`);
  const log: string[] = [];
  const r = await run(["ssh", "-o", "BatchMode=yes", o.host, "bash -s"], script, l => {
    if (!l.startsWith("@@RESULT")) console.error(`  | ${l}`);
    log.push(l);
  });
  const line = log.concat(r.out.split("\n")).find(l => l.startsWith("@@RESULT"));
  if (!line) throw new Error(`no result from ${o.host} (exit ${r.code}): ${tail(r.err)}`);
  const [result, ...rest] = line.slice("@@RESULT ".length).split(" ");
  const detail = rest.join(" ");
  const row: Row =
    result === "skipped"
      ? { ...base, result: "skipped", detail }
      : result === "tests-failed"
        ? { ...base, result: "tests-failed", detail: `${detail}\n${tail(log.filter(l => !l.startsWith("@@")).join("\n"))}` }
        : { ...base, result: result as Row["result"] };
  appendLedger(row);
  console.error(`[absorb] ${row.result}${row.detail ? `: ${row.detail.split("\n")[0]}` : ""}  ${source}`);
  return row;
}

// ---------- cli ----------

function flag(args: string[], name: string): string | undefined {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
}
const has = (args: string[], name: string) => args.includes(`--${name}`);
const positional = (args: string[]) => {
  const out: string[] = [];
  for (let i = 0; i < args.length; i++) {
    if (args[i]!.startsWith("--")) {
      if (!["green", "rust", "json", "fix"].includes(args[i]!.slice(2))) i++;
    } else out.push(args[i]!);
  }
  return out;
};

async function main(argv: string[]) {
  const [cmd, ...args] = argv;
  const pos = positional(args);
  const tryOpts = (): TryOpts => {
    const test = flag(args, "test");
    if (!test) throw new Error("--test \"<cmd>\" is required");
    return { test, host: flag(args, "host") ?? config().hosts.test, timeout: Number(flag(args, "timeout") ?? 5400) };
  };

  if (cmd === "candidates" || cmd === "auto") {
    const repo = pos[0];
    if (!repo?.includes("/")) throw new Error("usage: absorb.ts candidates <owner/repo>");
    const green = has(args, "green");
    const rustOnly = has(args, "rust");
    const [prsAll, forks] = await Promise.all([
      fetchPrs(repo, { green, max: Number(flag(args, "max") ?? 90) }),
      cmd === "candidates" ? fetchForks(repo, Number(flag(args, "forks") ?? 10)) : Promise.resolve([]),
    ]);
    const prs = (rustOnly ? prsAll.filter(c => c.rust) : prsAll).filter(c => !has(args, "fix") || c.kind === "fix").filter(c => c.mergeable !== "CONFLICTING" && c.ci !== "failure");
    const top = prs.slice(0, Number(flag(args, "top") ?? (cmd === "auto" ? 5 : 15)));
    if (cmd === "candidates") {
      if (has(args, "json")) return console.log(JSON.stringify({ repo, prs: top, forks }, null, 2));
      console.log(`${repo}: ${prsAll.length} open PRs updated since ${SINCE} (${prs.length} not failing or conflicting)\n`);
      top.forEach(c => console.log(fmt(c)));
      console.log(`\nforks ahead of upstream (${forks.length}):`);
      forks.forEach(f => console.log(`  +${f.ahead}/-${f.behind}  pushed ${f.pushed.slice(0, 10)}  ${f.url}`));
      return;
    }
    const name = flag(args, "fork") ?? repo.split("/")[1]!;
    const o = tryOpts();
    for (const c of top) {
      try {
        await tryOne(name, String(c.number), o);
      } catch (e) {
        console.error(`[absorb] #${c.number} error: ${(e as Error).message}`);
      }
    }
    return;
  }
  if (cmd === "try") {
    if (pos.length < 2) throw new Error("usage: absorb.ts try <fork-checkout> <pr-number|fork:branch> --test \"<cmd>\"");
    const row = await tryOne(pos[0]!, pos[1]!, tryOpts());
    process.exitCode = row.result === "merged" ? 0 : 1;
    return;
  }
  if (cmd === "skip") {
    const [repo, n] = pos;
    const reason = flag(args, "reason");
    if (!repo?.includes("/") || !n || !reason) throw new Error("usage: absorb.ts skip <owner/repo> <pr-number> --reason \"<why>\"");
    const pr = await gh<{ title: string; html_url: string }>(["api", `repos/${repo}/pulls/${n}`]);
    appendLedger({
      repo, fork: `${config().orgs.labs}/${repo.split("/")[1]}`, source: pr.html_url, title: pr.title, result: "skipped",
      test: flag(args, "test") ?? "", date: new Date().toISOString().slice(0, 10), detail: reason,
    });
    console.log(`skipped ${pr.html_url}`);
    return;
  }
  console.error(readFileSync(import.meta.path, "utf8").split("\n").slice(1, 11).map(l => l.replace(/^\/\/ ?/, "")).join("\n"));
  process.exitCode = 2;
}

if (import.meta.main) {
  main(process.argv.slice(2)).catch(e => {
    console.error((e as Error).message);
    process.exit(1);
  });
}
