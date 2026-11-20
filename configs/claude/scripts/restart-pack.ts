#!/usr/bin/env bun
// Restart pack: everything a fresh session needs to resume the plan after the usage limit or a crash, written to a git-ignored
// state directory (default ~/.claude/state/restart, override CLAUDE_RESTART_DIR). Generic code: it names no machine and no work item;
// the data it gathers comes from the workspace at run time.
//   restart-pack.ts refresh [--remote]   rebuild restart.md (fast, local only; --remote also reads the build host over ssh)
//   restart-pack.ts brief                print the SessionStart hook output that injects the pack (silent when absent or stale)
//   restart-pack.ts save <name> [file]   store the brief of a background agent (stdin when no file) so it can be relaunched
//   restart-pack.ts list                 list the saved briefs
// Workspace root: WORKSPACE_ROOT, else the parent of the git repository of the current directory.
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";

const HOME = homedir();
const CLAUDE = join(HOME, ".claude");
export const stateDir = (): string =>
  resolve(Bun.env.CLAUDE_RESTART_DIR ?? join(CLAUDE, "state", "restart"));
const MAX_AGE_MS = 72 * 3600_000;

const run = (cmd: string[], cwd?: string, timeoutMs = 8000): string => {
  const p = Bun.spawnSync(cmd, {
    cwd,
    stdout: "pipe",
    stderr: "ignore",
    timeout: timeoutMs,
  });
  return p.exitCode === 0 ? p.stdout.toString().trim() : "";
};

export function workspaceRoot(cwd = process.cwd()): string {
  if (Bun.env.WORKSPACE_ROOT) return resolve(Bun.env.WORKSPACE_ROOT);
  const top = run(["git", "rev-parse", "--show-toplevel"], cwd);
  return top ? dirname(top) : cwd;
}

export type RepoState = {
  name: string;
  branch: string;
  ahead: number;
  behind: number;
  dirty: number;
  last: string;
};

export function repoStates(root: string): RepoState[] {
  const out: RepoState[] = [];
  for (const e of readdirSync(root, { withFileTypes: true })) {
    if (!e.isDirectory() || !existsSync(join(root, e.name, ".git"))) continue;
    const dir = join(root, e.name);
    const [behind = "0", ahead = "0"] = (
      run(["git", "rev-list", "--left-right", "--count", "@{u}...HEAD"], dir) ||
      "0\t0"
    ).split(/\s+/);
    out.push({
      name: e.name,
      branch: run(["git", "rev-parse", "--abbrev-ref", "HEAD"], dir),
      ahead: Number(ahead),
      behind: Number(behind),
      dirty: run(["git", "status", "--porcelain"], dir)
        .split("\n")
        .filter(Boolean).length,
      last: run(["git", "log", "-1", "--format=%h %s"], dir).slice(0, 90),
    });
  }
  return out.sort((a, b) => a.name.localeCompare(b.name)).slice(0, 60);
}

type Item = {
  id: string;
  title: string;
  status: string;
  owner?: string;
  next?: string;
};

function workLedger(root: string): Item[] {
  const dir = Bun.env.AY_PRIVATE_DIR
    ? resolve(Bun.env.AY_PRIVATE_DIR)
    : join(root, "ay", ".private");
  const file = join(dir, "plan", "work.toml");
  if (!existsSync(file)) return [];
  try {
    return (
      (Bun.TOML.parse(readFileSync(file, "utf8")) as { item?: Item[] }).item ??
      []
    ).filter((i) => i.status !== "done");
  } catch {
    return [];
  }
}

function newestPlan(): { name: string; text: string } | undefined {
  const dir = join(CLAUDE, "plans");
  if (!existsSync(dir)) return undefined;
  const files = readdirSync(dir)
    .filter((f) => f.endsWith(".md") && !f.includes("-agent-"))
    .map((f) => ({ f, t: statSync(join(dir, f)).mtimeMs }))
    .sort((a, b) => b.t - a.t);
  return files[0]
    ? { name: files[0].f, text: readFileSync(join(dir, files[0].f), "utf8") }
    : undefined;
}

export function briefs(): { name: string; first: string }[] {
  const dir = join(stateDir(), "briefs");
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => f.endsWith(".md"))
    .sort()
    .map((f) => ({
      name: f.slice(0, -3),
      first:
        readFileSync(join(dir, f), "utf8")
          .split("\n")
          .find((l) => l.trim()) ?? "",
    }));
}

function remoteJobs(root: string): string {
  const local = join(root, "ay", "ay.local.toml");
  if (!existsSync(local)) return "";
  const alias = (
    Bun.TOML.parse(readFileSync(local, "utf8")) as {
      hosts?: { build?: string };
    }
  ).hosts?.build;
  if (!alias || alias.startsWith("<")) return "";
  return run(
    [
      "ssh",
      "-o",
      "BatchMode=yes",
      "-o",
      "ConnectTimeout=5",
      alias,
      "ps -eo etimes,args --sort=-etimes | grep -E 'cargo|rustc|make|qemu|bun |docker|pkgctl|makepkg' | grep -v grep | head -12 | cut -c1-140",
    ],
    undefined,
    15000,
  );
}

export function render(
  root: string,
  repos: RepoState[],
  items: Item[],
  plan: { name: string; text: string } | undefined,
  saved: { name: string; first: string }[],
  remote: string,
): string {
  const out = [
    "# Restart pack",
    "",
    `Written ${new Date().toISOString()} for the workspace at \`${root}\`. Read this first in a new session.`,
    "",
  ];
  out.push(
    "## What to do first",
    "1. Read the plan below and the memory index; do not redo finished work.",
    "2. Recreate the task list from the ledger (TaskCreate) and mark what the repositories already prove as done.",
    "3. Background agents die with the session: for each saved brief, relaunch it with the Agent tool (same name, the brief as prompt, preface: resume from the repository state, do not repeat committed units) or SendMessage if it is still alive (`ListAgents`).",
    "4. Inventory the repositories below: uncommitted work belongs to a session or an agent; ask `claude_sessions` before touching a repository another session holds.",
    "",
  );
  out.push(
    "## Open work (ledger)",
    ...(items.length
      ? [
          "| Item | Status | Owner | Next |",
          "|---|---|---|---|",
          ...items.map(
            (i) =>
              `| ${i.title} | ${i.status} | ${i.owner ?? ""} | ${(i.next ?? "").replaceAll("|", "/")} |`,
          ),
        ]
      : ["(no ledger found: use the plan and the repositories)"]),
    "",
  );
  out.push(
    "## Saved agent briefs",
    ...(saved.length
      ? saved.map(
          (b) =>
            `- \`${b.name}\`: ${b.first.slice(0, 120)}  (file: briefs/${b.name}.md)`,
        )
      : ["(none)"]),
    "",
  );
  out.push(
    "## Repositories",
    "| Repo | Branch | Ahead | Behind | Dirty | Last commit |",
    "|---|---|---:|---:|---:|---|",
    ...repos.map(
      (r) =>
        `| ${r.name} | ${r.branch} | ${r.ahead} | ${r.behind} | ${r.dirty} | ${r.last.replaceAll("|", "/")} |`,
    ),
    "",
  );
  if (remote)
    out.push(
      "## Running jobs on the build host (elapsed seconds, command)",
      "```",
      remote,
      "```",
      "",
    );
  if (plan)
    out.push(
      `## Plan (${plan.name})`,
      "",
      plan.text.split("\n").slice(0, 160).join("\n"),
      "",
    );
  return out.join("\n");
}

export function refresh(opts: { remote?: boolean; cwd?: string } = {}): string {
  const root = workspaceRoot(opts.cwd);
  const text = render(
    root,
    repoStates(root),
    workLedger(root),
    newestPlan(),
    briefs(),
    opts.remote ? remoteJobs(root) : "",
  );
  mkdirSync(stateDir(), { recursive: true });
  writeFileSync(join(stateDir(), "restart.md"), text);
  return text;
}

const BRIEF_MAX = 4000;

// The injected brief is the pack without its instructions block, cut to BRIEF_MAX characters; the full file stays on disk.
export function condense(text: string, file: string): string {
  const body = text.replace(/## What to do first[\s\S]*?(?=\n## )/, "").trim();
  return body.length <= BRIEF_MAX
    ? body
    : `${body.slice(0, BRIEF_MAX)}\n[cut: full pack in ${file}]`;
}

export function briefOutput(now = Date.now()): string {
  const file = join(stateDir(), "restart.md");
  if (!existsSync(file) || now - statSync(file).mtimeMs > MAX_AGE_MS) return "";
  const text = condense(readFileSync(file, "utf8"), file);
  return JSON.stringify({
    hookSpecificOutput: {
      hookEventName: "SessionStart",
      additionalContext: `RESTART BRIEF (previous session state; verify with the repositories before acting; relaunch saved briefs with Agent):\n\n${text}`,
    },
  });
}

export async function save(name: string, file?: string): Promise<string> {
  if (!/^[\w.-]+$/.test(name))
    throw new Error("brief name: letters, digits, dot, dash, underscore");
  const text = file
    ? readFileSync(file, "utf8")
    : await new Response(Bun.stdin.stream()).text();
  const dir = join(stateDir(), "briefs");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, `${name}.md`), text);
  return join(dir, `${name}.md`);
}

if (import.meta.main) {
  const [cmd, ...rest] = process.argv.slice(2);
  if (cmd === "refresh") {
    const text = refresh({ remote: rest.includes("--remote") });
    console.log(`restart pack written (${text.split("\n").length} lines)`);
  } else if (cmd === "brief") {
    const out = briefOutput();
    if (out) process.stdout.write(out);
  } else if (cmd === "save") {
    if (!rest[0]) (console.error("usage: save <name> [file]"), process.exit(2));
    console.log(await save(rest[0], rest[1]));
  } else if (cmd === "list") {
    for (const b of briefs()) console.log(`${b.name}: ${b.first}`);
  } else {
    console.error(
      "usage: restart-pack.ts refresh [--remote] | brief | save <name> [file] | list",
    );
    process.exit(2);
  }
}
