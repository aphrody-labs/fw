#!/usr/bin/env bun
/**
 * Unified `ay` CLI entrypoint.
 *
 * Dispatches all workspace tools, apps, and built-in commands with zero
 * Node.js dependencies and sub-millisecond startup.
 */

import { join, resolve } from "node:path";
import { COMMANDS, resolveCommand, type CommandCategory, type CommandEntry } from "./registry.ts";

const AY_VERSION = "0.1.0";

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

export function formatHelp(): string {
  const lines: string[] = [
    `ay ${AY_VERSION} — Unified Aphrody CLI (Bun ${Bun.version})`,
    "",
    "Usage:",
    "  ay <command> [args...]",
    "  ay --help | -h",
    "  ay --version | -v",
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

async function runDoctor(repoRoot: string): Promise<number> {
  const missing: string[] = [];
  for (const cmd of COMMANDS) {
    if (cmd.kind === "builtin") continue;
    const fullPath = join(repoRoot, cmd.target);
    if (!(await Bun.file(fullPath).exists())) {
      missing.push(cmd.target);
    }
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
    console.log(formatHelp());
    return 0;
  }

  if (first === "--version" || first === "-v" || first === "version") {
    console.log(`ay ${AY_VERSION} (Bun ${Bun.version})`);
    return 0;
  }

  const cmd = resolveCommand(first);
  if (!cmd) {
    console.error(`Unknown command: ${first}\nRun 'ay --help' for usage.`);
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
        return await runDoctor(repoRoot());
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
