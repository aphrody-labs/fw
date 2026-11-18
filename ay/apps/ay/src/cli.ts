#!/usr/bin/env bun
import { join } from "node:path";
import {
  COMMAND_REGISTRY,
  findCommand,
  probeFastTools,
  readUnifiedStatus,
  resolveRepoRoot,
} from "./registry.ts";

function formatHelp(): string {
  const lines = [
    "ay — Unified Aphrody OS, SDK, WebOS, AI OS, Shell, RE, WASM & Cloud CLI",
    "",
    "Usage:",
    "  ay <command> [args...]",
    "  ay --list [--json]",
    "  ay --help",
    "",
    "Commands:",
  ];
  for (const c of COMMAND_REGISTRY) {
    const aliases = c.aliases.length ? ` (${c.aliases.join(", ")})` : "";
    lines.push(`  ${(c.name + aliases).padEnd(28)} ${c.summary}`);
  }
  return lines.join("\n");
}

async function runBuiltin(name: string, args: string[], root: string): Promise<number> {
  const asJson = args.includes("--json");
  if (name === "status") {
    const status = await readUnifiedStatus(root);
    console.log(JSON.stringify(status, null, 2));
    return 0;
  }
  if (name === "doctor") {
    const tools = probeFastTools();
    const entries = Object.entries(tools);
    const present = entries.filter(([, p]) => p !== null).length;
    const missing = entries.filter(([, p]) => p === null).map(([k]) => k);
    const report = {
      ok: Boolean(tools.bun && tools.cargo),
      presentCount: present,
      totalCount: entries.length,
      missing,
      tools,
      runtime: { bun: Bun.version, platform: process.platform, arch: process.arch },
    };
    if (asJson) {
      console.log(JSON.stringify(report, null, 2));
    } else {
      console.log(`ay doctor: Bun ${Bun.version} (${process.platform}/${process.arch}) — ${present}/${entries.length} fast CLI tools installed`);
      for (const [k, p] of entries) {
        console.log(`  ${k.padEnd(10)} ${p ?? "MISSING"}`);
      }
    }
    return report.ok ? 0 : 1;
  }
  if (name === "check") {
    const proc = Bun.spawn(["bun", "run", "check", ...args], {
      cwd: root,
      stdin: "inherit",
      stdout: "inherit",
      stderr: "inherit",
    });
    return await proc.exited;
  }
  return 1;
}

export async function runCli(argv: string[], root = resolveRepoRoot()): Promise<number> {
  const [first, ...rest] = argv;
  if (!first || first === "--help" || first === "-h" || first === "help") {
    console.log(formatHelp());
    return 0;
  }
  if (first === "--list") {
    if (rest.includes("--json")) {
      console.log(JSON.stringify(COMMAND_REGISTRY, null, 2));
    } else {
      for (const c of COMMAND_REGISTRY) console.log(`${c.name}\t${c.category}\t${c.summary}`);
    }
    return 0;
  }

  const cmd = findCommand(first);
  if (!cmd) {
    console.error(`ay: unknown command "${first}". Run "ay --help" for commands.`);
    return 2;
  }

  if (cmd.kind === "builtin") {
    return runBuiltin(cmd.name, rest, root);
  }

  const target = join(root, cmd.entry!);
  const proc = Bun.spawn([process.execPath, target, ...rest], {
    cwd: root,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  return await proc.exited;
}

if (import.meta.main) {
  process.exitCode = await runCli(Bun.argv.slice(2));
}
