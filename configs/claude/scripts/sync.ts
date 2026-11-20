import {
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { spawn } from "node:child_process";
import { join } from "node:path";
import {
  discoverWorkspaceRoot,
  HOME,
  REPO_ROOT,
  runCmd,
  writeIfChanged,
} from "./lib.ts";

/** Claude Code's project directory name of a path: every character outside [A-Za-z0-9] becomes "-". */
export const projectSlug = (path: string): string =>
  path.replace(/[^A-Za-z0-9]/g, "-");

const GEMINI_ROOT = join(HOME, ".gemini");
const WS_BEGIN =
  "<!-- ws-memory:begin (generated from the shared workspace memory; edit that source) -->";
const WS_END = "<!-- ws-memory:end -->";
/** What agy's own check forbids in its files: a reference to Claude Code's home. */
const COUPLED =
  /(?:~|HOME|\$HOME|%USERPROFILE%)[/\\]\.claude\b|Users[/\\][^/\\]+[/\\]\.claude\b|CLAUDE_HOME|CLAUDE_PLUGIN_ROOT/i;

/** Memory directories of the projects, each real directory once (a linked memory directory is not read twice). */
export function memoryDirs(
  projRoot = join(REPO_ROOT, "projects"),
): { proj: string; dir: string }[] {
  const seen = new Set<string>();
  const out: { proj: string; dir: string }[] = [];
  if (!existsSync(projRoot)) return out;
  for (const proj of readdirSync(projRoot).sort()) {
    const dir = join(projRoot, proj, "memory");
    if (!existsSync(dir)) continue;
    const real = realpathSync(dir);
    if (seen.has(real)) continue;
    seen.add(real);
    out.push({ proj, dir });
  }
  return out;
}

/** Workspace-wide memory (the project directory of the workspace root, e.g. C--src for C:/src). */
export function workspaceMemoryDir(root = discoverWorkspaceRoot()): string {
  return join(REPO_ROOT, "projects", projectSlug(root), "memory");
}

/** Projects below the workspace root without a memory of their own share the workspace memory through a directory link. */
export function linkWorkspaceMemory(root = discoverWorkspaceRoot()): string[] {
  const ws = workspaceMemoryDir(root);
  if (!existsSync(ws)) return [];
  const prefix = `${projectSlug(root)}-`;
  const linked: string[] = [];
  for (const proj of readdirSync(join(REPO_ROOT, "projects"))) {
    if (!proj.startsWith(prefix)) continue;
    const dir = join(REPO_ROOT, "projects", proj, "memory");
    if (existsSync(dir) && readdirSync(dir).length > 0) continue;
    if (existsSync(dir)) rmSync(dir, { recursive: true, force: true });
    else if (lstatSync(dir, { throwIfNoEntry: false }))
      rmSync(dir, { force: true });
    mkdirSync(join(REPO_ROOT, "projects", proj), { recursive: true });
    symlinkSync(ws, dir, "junction");
    linked.push(proj);
  }
  return linked;
}

/** CLAUDE.md for agy: bullets about Claude Code itself stay out (agy keeps no reference to ~/.claude). */
export function agyGlobalRules(globalMd: string): string {
  return globalMd
    .split(/\r?\n/)
    .filter((l) => !/\.claude|claude_inject|claude\.exe|claude code/i.test(l))
    .join("\n");
}

/**
 * Hands the shared brain to agy as one-way generated copies in the files agy owns: rules/00-global.md (CLAUDE.md) and
 * memory/ws-*.md (workspace memory) with their MEMORY.md block; `~/.gemini/scripts/sync.ts` then rebuilds GEMINI.md.
 */
export function syncAgy(
  gemini = GEMINI_ROOT,
  claudeMd = join(REPO_ROOT, "CLAUDE.md"),
  wsDir = workspaceMemoryDir(),
): number {
  if (!existsSync(gemini)) return 0;
  let changed = 0;
  if (existsSync(claudeMd)) {
    const body = agyGlobalRules(readFileSync(claudeMd, "utf8").trim());
    const head =
      "<!-- Generated from the shared global rules (CLAUDE.md of the Claude config); edit that source instead. -->";
    if (
      writeIfChanged(
        join(gemini, "rules", "00-global.md"),
        `${head}\n\n${body}\n`,
      )
    )
      changed++;
  }
  const memDir = join(gemini, "memory");
  if (existsSync(wsDir) && existsSync(memDir)) {
    const names = readdirSync(wsDir).filter(
      (n) =>
        n.endsWith(".md") &&
        n !== "MEMORY.md" &&
        !n.startsWith("learned-") &&
        !COUPLED.test(readFileSync(join(wsDir, n), "utf8")),
    );
    for (const n of names) {
      const body = readFileSync(join(wsDir, n), "utf8");
      if (writeIfChanged(join(memDir, `ws-${n}`), body)) changed++;
    }
    for (const n of readdirSync(memDir))
      if (n.startsWith("ws-") && !names.includes(n.slice(3))) {
        rmSync(join(memDir, n), { force: true });
        changed++;
      }
    const idxPath = join(wsDir, "MEMORY.md");
    const index = existsSync(idxPath)
      ? readFileSync(idxPath, "utf8")
          .trim()
          .split(/\r?\n/)
          .filter((l) => {
            const m = l.match(/\]\(([^)#\s]+\.md)\)/);
            return !m || names.includes(m[1] as string);
          })
          .join("\n")
          .replace(
            /\]\((?!https?:|file:|\/|~)([^)#\s]+\.md)\)/g,
            "](~/.gemini/memory/ws-$1)",
          )
      : "";
    const mem = join(memDir, "MEMORY.md");
    const cur = existsSync(mem) ? readFileSync(mem, "utf8") : "";
    const block = `${WS_BEGIN}\n${index}\n${WS_END}`;
    const a = cur.search(/<!-- ws-memory:begin[^>]*-->/);
    const b = cur.indexOf(WS_END);
    const next =
      a >= 0 && b > a
        ? `${cur.slice(0, a)}${block}${cur.slice(b + WS_END.length)}`
        : `${cur.trimEnd()}\n${block}\n`;
    if (writeIfChanged(mem, next)) changed++;
  }
  const geminiSync = join(gemini, "scripts", "sync.ts");
  if (existsSync(geminiSync))
    runCmd(["bun", "--no-install", "--no-env-file", geminiSync]);
  return changed;
}

export function buildUnifiedInstructions(): string {
  const globalMd = existsSync(join(REPO_ROOT, "CLAUDE.md"))
    ? readFileSync(join(REPO_ROOT, "CLAUDE.md"), "utf8").trim()
    : "";

  const memSections: string[] = [];
  {
    for (const { proj, dir } of memoryDirs()) {
      const idx = join(dir, "MEMORY.md");
      if (!existsSync(idx)) continue;
      const body = readFileSync(idx, "utf8")
        .trim()
        .replace(
          /\]\((?!https?:|file:|\/|~)([^)#\s]+\.md)\)/g,
          `](~/.claude/projects/${proj}/memory/$1)`,
        );
      if (body) memSections.push(`## Memory (${proj})\n\n${body}`);
    }
  }

  return [
    "<!-- Generated by ~/.claude/scripts/sync.ts from CLAUDE.md and consolidated MEMORY.md; edit those sources instead. -->",
    "",
    globalMd,
    "",
    ...memSections,
    "",
  ]
    .join("\n")
    .replace(/\n{3,}/g, "\n\n");
}

export function syncInstructionFiles(): number {
  const content = buildUnifiedInstructions();
  const targets = [
    join(REPO_ROOT, "packages", "aphrody", "rules", "AGENTS.md"),
    join(HOME, ".codex", "AGENTS.md"),
    join(HOME, ".config", "opencode", "AGENTS.md"),
    join(HOME, ".pi", "agent", "AGENTS.md"),
  ];
  let changed = 0;
  for (const t of targets) {
    if (writeIfChanged(t, content)) changed++;
  }
  linkWorkspaceMemory();
  return changed + syncAgy();
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const changed = syncInstructionFiles();
  runCmd(["bun", "--no-install", join(REPO_ROOT, "scripts", "merge-brain.ts")]);

  if (args.includes("--no-git")) {
    console.log(`Synced ${changed} instruction file(s) and SQLite memories.`);
    process.exit(0);
  }

  const isRepo = runCmd([
    "git",
    "-C",
    REPO_ROOT,
    "rev-parse",
    "--is-inside-work-tree",
  ]).ok;
  if (!isRepo) process.exit(0);

  runCmd(["git", "-C", REPO_ROOT, "add", "-A"]);
  const clean = runCmd([
    "git",
    "-C",
    REPO_ROOT,
    "diff",
    "--cached",
    "--quiet",
  ]).ok;
  if (!clean) {
    const commitMsg = args.find((a) => !a.startsWith("-")) ?? "config: sync";
    const c = runCmd(["git", "-C", REPO_ROOT, "commit", "-m", commitMsg]);
    if (!c.ok) {
      console.error(`sync commit failed: ${c.err}`);
      process.exit(c.code || 1);
    }
    const hasRemote = runCmd(["git", "-C", REPO_ROOT, "remote"]).out.length > 0;
    if (hasRemote) {
      if (args.includes("--detached")) {
        spawn("git", ["-C", REPO_ROOT, "push", "--quiet"], {
          detached: true,
          stdio: "ignore",
          windowsHide: true,
        }).unref();
      } else {
        const p = runCmd(["git", "-C", REPO_ROOT, "push"]);
        if (!p.ok) {
          console.error(`sync push failed: ${p.err}`);
          process.exit(p.code || 1);
        }
      }
    }
  }
  console.log(`Sync complete (instructions updated: ${changed}).`);
}
