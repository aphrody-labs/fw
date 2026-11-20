#!/usr/bin/env bun
// Per-project MCP enablement without regression. Claude Code keeps `disabledMcpServers` per project in ~/.claude.json; with 20+ servers and
// 600+ tools, every session pays for tools its project never calls. This tool decides, per project, which servers to disable:
//   keep     = baseline servers (always on) + every server the project's own transcripts show it called in the last DAYS days
//              + servers whose use the project's manifest implies (EXTRAS: a rule per marker file or name token)
//   disable  = every other known server
// Evidence, not guesses: nothing a project used recently can be disabled. Settings take effect at the next session start.
//   mcp-profiles.ts plan [project...]     print the decision and its evidence per project (default: all projects of ~/.claude.json)
//   mcp-profiles.ts apply [project...]    write disabledMcpServers (backup of ~/.claude.json in ~/.claude/state/backups first)
//   mcp-profiles.ts auto                  apply for the current directory's project only (SessionStart hook; silent)
// The server universe comes from `claude mcp list`, cached for a day in ~/.claude/state/mcp-universe.json.
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { defaultEnv, globalProfile, repoProfilePath } from "./profile.ts";

const HOME = homedir();
const CLAUDE_JSON = join(HOME, ".claude.json");
const STATE = join(HOME, ".claude", "state");
const DAYS = Number(Bun.env.MCP_PROFILE_DAYS ?? 60);
/** Servers every project keeps: `[mcp] baseline` of ~/.claude/profile.toml (scripts/profile.ts). */
export const BASELINE = globalProfile(defaultEnv()).mcp?.baseline ?? [];

/** Claude Code names a server's tools `mcp__<server with every character outside [A-Za-z0-9_-] replaced by _>__<tool>`. */
export const toolPrefix = (server: string): string =>
  `mcp__${server.replace(/[^A-Za-z0-9_-]/g, "_")}__`;

/** Marker files or name tokens of a project that imply extra servers (regexes on the project path and its top-level entries). */
export const EXTRAS: { when: RegExp; servers: RegExp; why: string }[] = [
  {
    when: /(^|[\\/])(ay|aphrody-m3|figma)([\\/]|$)|design/i,
    servers: /Figma|Google (Docs|Drive|Sheets|Slides)|Claude Docs/,
    why: "design package and docs mirrors",
  },
  {
    when: /(^|[\\/])(rosegriffon|shenron|aysh)([\\/]|$)/i,
    servers: /hostinger|ovh/,
    why: "sites hosted on the fleet",
  },
];

export function decide(
  universe: string[],
  used: Map<string, number>,
  path: string,
  entries: string[],
): { keep: string[]; disable: string[]; evidence: Record<string, string> } {
  const evidence: Record<string, string> = {};
  const keep = new Set<string>();
  for (const s of universe) {
    if (BASELINE.includes(s)) (keep.add(s), (evidence[s] = "baseline"));
    else if ((used.get(s) ?? 0) > 0)
      (keep.add(s),
        (evidence[s] = `used ${used.get(s)} times in ${DAYS} days`));
    else {
      const rule = EXTRAS.find(
        (r) =>
          (r.when.test(path) || entries.some((e) => r.when.test(e))) &&
          r.servers.test(s),
      );
      if (rule) (keep.add(s), (evidence[s] = rule.why));
    }
  }
  return {
    keep: [...keep].sort(),
    disable: universe.filter((s) => !keep.has(s)).sort(),
    evidence,
  };
}

/** Directory name Claude Code gives a project's transcripts: every character outside [A-Za-z0-9-] becomes a dash. */
export const transcriptDir = (project: string): string =>
  project.replace(/[^A-Za-z0-9-]/g, "-");

export function usage(
  project: string,
  universe: string[],
  days = DAYS,
  base = join(HOME, ".claude", "projects"),
): Map<string, number> {
  const out = new Map<string, number>();
  const dir = join(base, transcriptDir(project));
  if (!existsSync(dir)) return out;
  const since = Date.now() - days * 86400_000;
  const prefixes = universe.map((s) => [s, toolPrefix(s)] as const);
  for (const f of readdirSync(dir).filter((f) => f.endsWith(".jsonl"))) {
    const p = join(dir, f);
    if (statSync(p).mtimeMs < since) continue;
    const text = readFileSync(p, "utf8");
    for (const [server, prefix] of prefixes) {
      let i = -1,
        n = 0;
      while ((i = text.indexOf(`"name":"${prefix}`, i + 1)) >= 0) n++;
      if (n) out.set(server, (out.get(server) ?? 0) + n);
    }
  }
  return out;
}

/** Server names reconstructed from tool prefixes seen in transcripts (claude.ai connectors and built-ins do not appear in `claude mcp list`). */
export function serverFromPrefix(prefix: string): string {
  const raw = prefix.replace(/^mcp__/, "").replace(/__$/, "");
  if (raw.startsWith("claude_ai_"))
    return `claude.ai ${raw.slice(10).replaceAll("_", " ")}`;
  const m = /^plugin_([^_]+)_(.+)$/.exec(raw);
  return m ? `plugin:${m[1]}:${m[2]}` : raw;
}

function observedServers(base = join(HOME, ".claude", "projects")): string[] {
  const seen = new Set<string>();
  const since = Date.now() - DAYS * 86400_000;
  for (const d of existsSync(base) ? readdirSync(base) : []) {
    const dir = join(base, d);
    if (!statSync(dir).isDirectory()) continue;
    for (const f of readdirSync(dir).filter((f) => f.endsWith(".jsonl"))) {
      const p = join(dir, f);
      if (statSync(p).mtimeMs < since) continue;
      for (const m of readFileSync(p, "utf8").matchAll(
        /"name":"(mcp__[A-Za-z0-9_-]+?__)[A-Za-z0-9_-]+"/g,
      ))
        seen.add(serverFromPrefix(m[1]!));
    }
  }
  return [...seen];
}

async function universe(): Promise<string[]> {
  const cache = join(STATE, "mcp-universe.json");
  if (existsSync(cache) && Date.now() - statSync(cache).mtimeMs < 86400_000)
    return JSON.parse(readFileSync(cache, "utf8"));
  const proc = Bun.spawnSync(["claude", "mcp", "list"], {
    stdout: "pipe",
    stderr: "ignore",
    timeout: 90_000,
  });
  const names = new Set<string>();
  for (const line of proc.stdout.toString().split("\n")) {
    const m =
      /^(.+?):\s+(\S.*?)\s+-\s+[✔✘✓✗!⚠]/u.exec(line.trim()) ??
      /^(.+?):\s+(?:https?:|bun |node |npx )/.exec(line.trim());
    if (m) names.add(m[1]!.trim());
  }
  for (const s of observedServers()) names.add(s);
  const list = [...names].sort();
  if (list.length)
    (mkdirSync(STATE, { recursive: true }),
      writeFileSync(cache, JSON.stringify(list)));
  return list;
}

type ClaudeJson = {
  projects?: Record<string, { disabledMcpServers?: string[] }>;
};

function apply(decisions: Map<string, string[]>): void {
  mkdirSync(join(STATE, "backups"), { recursive: true });
  copyFileSync(
    CLAUDE_JSON,
    join(STATE, "backups", `claude.json.${Date.now()}`),
  );
  const cfg = JSON.parse(readFileSync(CLAUDE_JSON, "utf8")) as ClaudeJson; // read again right before writing: merge, do not clobber
  for (const [project, disabled] of decisions)
    if (cfg.projects?.[project])
      cfg.projects[project]!.disabledMcpServers = disabled;
  const tmp = `${CLAUDE_JSON}.tmp-${process.pid}`;
  writeFileSync(tmp, JSON.stringify(cfg, null, 2));
  renameSync(tmp, CLAUDE_JSON);
}

if (import.meta.main) {
  const [cmd = "plan", ...rest] = process.argv.slice(2);
  const cfg = JSON.parse(readFileSync(CLAUDE_JSON, "utf8")) as ClaudeJson;
  const known = Object.keys(cfg.projects ?? {});
  const all = await universe();
  const wanted =
    cmd === "auto"
      ? known.filter(
          (p) =>
            resolve(p).toLowerCase() === resolve(process.cwd()).toLowerCase(),
        )
      : rest.length
        ? rest
        : known.filter(
            (p) => existsSync(p) && !/AppData|Windows|cc-pipe/.test(p),
          );
  const decisions = new Map<string, string[]>();
  for (const project of wanted) {
    // A repository that declares its own profile owns its toggles (scripts/profile.ts applies them).
    if (existsSync(project) && repoProfilePath(project)) {
      if (cmd === "plan")
        console.log(
          `${project}: declared profile, see bun scripts/profile.ts ${project}`,
        );
      continue;
    }
    const entries = existsSync(project)
      ? readdirSync(project).slice(0, 80)
      : [];
    const d = decide(all, usage(project, all), project, entries);
    // No transcript of this project in the window: there is no evidence, so nothing is disabled (no regression for new projects).
    const evidence = existsSync(
      join(HOME, ".claude", "projects", transcriptDir(project)),
    );
    if (!evidence) ((d.keep = [...all]), (d.disable = []));
    decisions.set(project, d.disable);
    if (cmd === "plan")
      console.log(
        `${project}: keep ${d.keep.length}, disable ${d.disable.length}\n${d.keep.map((s) => `  + ${s}: ${d.evidence[s]}`).join("\n")}`,
      );
  }
  if (cmd === "apply" || cmd === "auto") {
    if (!all.length)
      cmd === "auto"
        ? process.exit(0)
        : (console.error("no server list: claude mcp list returned nothing"),
          process.exit(1));
    apply(decisions);
    if (cmd === "apply")
      console.log(
        `disabledMcpServers written for ${decisions.size} project(s)`,
      );
  }
}
