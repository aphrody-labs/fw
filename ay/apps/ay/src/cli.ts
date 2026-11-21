#!/usr/bin/env bun
/**
 * Unified `ay` CLI entrypoint.
 *
 * Dispatches the workspace tools, apps and built-in commands listed in
 * registry.ts.
 */

import { join, resolve } from "node:path";
import { COMMANDS, resolveCommand, type CommandCategory, type CommandEntry } from "./registry.ts";

import pkg from "../package.json" with { type: "json" };

const AY_VERSION: string = pkg.version;

const CATEGORY_ORDER: readonly CommandCategory[] = [
  "core",
  "runtime",
  "cloud",
  "infra",
  "quality",
  "data",
  "meta",
];

/** True in a `bun build --compile` binary, where import.meta.dir is a virtual path and process.execPath is `ay` itself. */
const COMPILED = import.meta.dir.includes("$bunfs") || import.meta.dir.includes("~BUN");

/** The bun that runs tool scripts: this process in source mode, the one on PATH in a compiled binary. */
function bunExecutable(): string {
  if (!COMPILED) return process.execPath;
  const bun = Bun.which("bun");
  if (!bun) throw new Error("bun is not on PATH: tool commands run through `bun run`");
  return bun;
}

function findRepoRoot(startDir: string): string {
  let current = resolve(startDir);
  while (true) {
    if (
      Bun.file(join(current, "package.json")).size > 0 &&
      Bun.file(join(current, "Cargo.toml")).size > 0
    ) {
      return current;
    }
    const parent = resolve(current, "..");
    if (parent === current) {
      return resolve(startDir);
    }
    current = parent;
  }
}

/** Machine-readable command table for agents (`ay --help --json`): one row per command, like `fw status` and `uv --help` for tools. */
export function commandsJson(): { version: string; bun: string; commands: Pick<CommandEntry, "name" | "aliases" | "category" | "kind" | "target" | "summary">[] } {
  return {
    version: AY_VERSION,
    bun: Bun.version,
    commands: COMMANDS.map(({ name, aliases, category, kind, target, summary }) => ({ name, aliases, category, kind, target, summary })),
  };
}

function distance(a: string, b: string): number {
  const row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    let prev = row[0]!;
    row[0] = i;
    for (let j = 1; j <= b.length; j++) {
      const tmp = row[j]!;
      row[j] = Math.min(row[j]! + 1, row[j - 1]! + 1, prev + (a[i - 1] === b[j - 1] ? 0 : 1));
      prev = tmp;
    }
  }
  return row[b.length]!;
}

/** The closest command names to a mistyped one (prefix matches first, then small edit distance), as `uv` and `bun` suggest. */
export function suggest(input: string, limit = 3): string[] {
  const names = COMMANDS.flatMap((c) => [c.name, ...(c.aliases ?? [])]);
  const q = input.toLowerCase();
  const scored = names
    .map((n) => ({ n, d: n.startsWith(q) || q.startsWith(n) ? 0 : distance(q, n) }))
    .filter((x) => x.d <= Math.max(2, Math.floor(q.length / 3)))
    .sort((a, b) => a.d - b.d || a.n.localeCompare(b.n));
  return [...new Set(scored.map((x) => x.n))].slice(0, limit);
}

export function formatHelp(): string {
  const lines: string[] = [
    `ay ${AY_VERSION} — Unified Aphrody CLI (Bun ${Bun.version})`,
    "",
    "Usage:",
    "  ay <command> [args...]",
    "  ay --help | -h [--json]",
    "  ay --version | -v [--json]",
    "  ay <command> --help",
    "",
    "Commands:",
  ];

  for (const category of CATEGORY_ORDER) {
    const items = COMMANDS.filter((c) => c.category === category);
    if (items.length === 0) continue;
    lines.push(`  [${category}]`);
    for (const item of items) {
      const aliasText = item.aliases?.length
        ? ` (${item.aliases.join(", ")})`
        : "";
      lines.push(`    ${item.name.padEnd(20)} ${item.summary}${aliasText}`);
    }
  }

  return lines.join("\n");
}

export function generateCompletions(shell: string): string {
  const names = COMMANDS.map((c) => c.name);
  switch (shell.toLowerCase()) {
    case "bash":
      return [
        "_ay_completions() {",
        `  local cur="\${COMP_WORDS[COMP_CWORD]}"`,
        `  COMPREPLY=( $(compgen -W "${names.join(" ")}" -- "$cur") )`,
        "}",
        "complete -F _ay_completions ay",
      ].join("\n");
    case "zsh":
      return [
        "#compdef ay",
        "_ay() {",
        "  local -a commands",
        "  commands=(",
        ...COMMANDS.map((c) => `    '${c.name}:${c.summary.replace(/'/g, "")}'`),
        "  )",
        "  _describe 'command' commands",
        "}",
        "_ay",
      ].join("\n");
    case "fish":
      return COMMANDS.map(
        (c) =>
          `complete -c ay -f -n "__fish_use_subcommand" -a "${c.name}" -d "${c.summary.replace(/"/g, '\\"')}"`,
      ).join("\n");
    case "nu":
    case "nushell":
      return [
        `export def "nu-complete ay" [] {`,
        `  [${names.map((n) => `"${n}"`).join(" ")}]`,
        `}`,
        `export extern ay [command?: string@"nu-complete ay", ...args: string]`,
      ].join("\n");
    case "pwsh":
    case "powershell":
      return [
        `Register-ArgumentCompleter -Native -CommandName ay -ScriptBlock {`,
        `  param($wordToComplete, $commandAst, $cursorPosition)`,
        `  @(${names.map((n) => `'${n}'`).join(", ")}) | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {`,
        `    [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)`,
        `  }`,
        `}`,
      ].join("\n");
    default:
      throw new Error(
        `Unsupported shell '${shell}'. Expected: bash, zsh, fish, nu, pwsh.`,
      );
  }
}

async function runDoctor(repoRoot: string, args: readonly string[] = []): Promise<number> {
  const missing: string[] = [];
  for (const cmd of COMMANDS) {
    if (cmd.kind === "builtin") continue;
    const fullPath = join(repoRoot, cmd.target);
    if (!(await Bun.file(fullPath).exists())) {
      missing.push(cmd.target);
    }
  }

  if (args.includes("--json")) {
    console.log(JSON.stringify({ ok: missing.length === 0, version: AY_VERSION, bun: Bun.version, root: repoRoot, commands: COMMANDS.length, missing }));
    return missing.length === 0 ? 0 : 1;
  }
  console.log(`ay ${AY_VERSION} | Bun ${Bun.version} | root=${repoRoot}`);
  console.log(`registered commands: ${COMMANDS.length}`);
  if (missing.length > 0) {
    console.error(`missing targets (${missing.length}): ${missing.join(", ")}`);
    return 1;
  }
  console.log("status: ok (all targets present)");
  return 0;
}

const git = (root: string, ...args: string[]) => {
  const p = Bun.spawnSync(["git", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
  return { code: p.exitCode ?? 1, out: p.stdout.toString().trim(), err: p.stderr.toString().trim() };
};

/** `ay upgrade [--check]`: fast-forward the checkout, install the locked dependencies, verify the registry (like `bun upgrade`, `uv self update`, `fw upgrade`). */
async function runUpgrade(root: string, args: readonly string[]): Promise<number> {
  const check = args.includes("--check");
  const json = args.includes("--json");
  const say = (o: Record<string, unknown>, text: string) => console.log(json ? JSON.stringify(o) : text);
  const fetched = git(root, "fetch", "-q", "origin");
  if (fetched.code !== 0) { say({ ok: false, error: fetched.err }, `fetch failed: ${fetched.err}`); return 1; }
  const behind = Number(git(root, "rev-list", "--count", "HEAD..@{u}").out || 0);
  const ahead = Number(git(root, "rev-list", "--count", "@{u}..HEAD").out || 0);
  if (check || behind === 0) {
    say({ ok: true, behind, ahead, upgraded: false }, behind === 0 ? "ay is up to date" : `${behind} commit(s) behind origin`);
    return 0;
  }
  if (git(root, "status", "--porcelain", "--untracked-files=no").out) {
    say({ ok: false, error: "tracked changes in the working tree" }, "upgrade refused: tracked changes in the working tree (commit them first)");
    return 1;
  }
  const pulled = git(root, "pull", "--ff-only", "-q");
  if (pulled.code !== 0) { say({ ok: false, error: pulled.err }, `pull failed: ${pulled.err}`); return 1; }
  const install = Bun.spawnSync([bunExecutable(), "install", "--frozen-lockfile"], { cwd: root, stdout: "inherit", stderr: "inherit" });
  if (install.exitCode !== 0) return install.exitCode ?? 1;
  say({ ok: true, behind, ahead, upgraded: true }, `upgraded by ${behind} commit(s)`);
  return await runDoctor(root, json ? ["--json"] : []);
}

async function runCompile(repoRoot: string, args: string[]): Promise<number> {
  const outIdx = args.indexOf("--outfile");
  const outfile =
    outIdx >= 0 && args[outIdx + 1]
      ? args[outIdx + 1]
      : join(
          repoRoot,
          "dist",
          process.platform === "win32" ? "ay.exe" : "ay",
        );

  const targetIdx = args.indexOf("--target");
  const target = targetIdx >= 0 && args[targetIdx + 1] ? [`--target=${args[targetIdx + 1]}`] : [];
  const proc = Bun.spawn(
    [
      bunExecutable(),
      "build",
      "--compile",
      "--minify",
      "--bytecode",
      ...target,
      join(repoRoot, "apps/ay/src/cli.ts"),
      "--outfile",
      outfile,
    ],
    {
      cwd: repoRoot,
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
    },
  );
  return await proc.exited;
}

/** The checkout that holds the tools: AY_ROOT, else the checkout of this file, else the one above the working directory. */
function repoRoot(): string {
  return findRepoRoot(Bun.env.AY_ROOT ?? (COMPILED ? process.cwd() : import.meta.dir));
}

/** Usage of one command: the registry row, and for a tool the leading comment of its script. Never runs the tool. */
async function commandHelp(cmd: CommandEntry, root: string): Promise<string> {
  const lines = [`ay ${cmd.name}${cmd.aliases?.length ? ` (${cmd.aliases.join(", ")})` : ""}  [${cmd.category}]`, cmd.summary];
  if (cmd.kind === "builtin") return lines.join("\n");
  lines.push(`script: ${cmd.target}`, "");
  const file = Bun.file(join(root, cmd.target));
  if (await file.exists()) {
    const head = (await file.slice(0, 4096).text()).replace(/^#!.*\r?\n/, "");
    const block = /^\s*\/\*\*([\s\S]*?)\*\//.exec(head)?.[1]?.split(/\r?\n/).map((l) => l.replace(/^\s*\* ?/, ""));
    const comment = block ?? head.split(/\r?\n/).filter((l) => l.startsWith("//") || l.startsWith("#")).map((l) => l.replace(/^(\/\/|#) ?/, ""));
    lines.push(...comment.join("\n").trim().split("\n"));
  }
  return lines.join("\n");
}

export async function runCli(argv: readonly string[]): Promise<number> {
  const [first, ...rest] = argv;

  if (!first || first === "--help" || first === "-h" || first === "help") {
    console.log(rest.includes("--json") ? JSON.stringify(commandsJson()) : formatHelp());
    return 0;
  }

  if (first === "--version" || first === "-v" || first === "version") {
    console.log(rest.includes("--json") ? JSON.stringify({ version: AY_VERSION, bun: Bun.version }) : `ay ${AY_VERSION} (Bun ${Bun.version})`);
    return 0;
  }

  const cmd = resolveCommand(first);
  if (!cmd) {
    const close = suggest(first);
    console.error(`Unknown command: ${first}${close.length ? `\nDid you mean: ${close.join(", ")}?` : ""}\nRun 'ay --help' for usage.`);
    return 1;
  }

  if (rest[0] === "--help" || rest[0] === "-h") {
    console.log(await commandHelp(cmd, repoRoot()));
    return 0;
  }

  if (cmd.kind === "builtin") {
    switch (cmd.name) {
      case "completions": {
        const shell = rest[0] ?? "pwsh";
        console.log(generateCompletions(shell));
        return 0;
      }
      case "doctor":
        return await runDoctor(repoRoot(), rest);
      case "upgrade":
        return await runUpgrade(repoRoot(), rest);
      case "compile":
        return await runCompile(repoRoot(), rest);
      case "wck":
        return await (await import("./wck.ts")).runWckCli(rest);
      default:
        console.error(`Unhandled builtin: ${cmd.name}`);
        return 1;
    }
  }

  const root = repoRoot();
  const proc = Bun.spawn([bunExecutable(), "run", join(root, cmd.target), ...rest], {
    cwd: root,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  return await proc.exited;
}

if (import.meta.main) {
  // No top-level await: `--bytecode` compiles to CommonJS.
  runCli(Bun.argv.slice(2)).then((code) => process.exit(code));
}
