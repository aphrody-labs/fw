#!/usr/bin/env bun
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { startFairwindApiServer } from "./api.ts";
import {
  installService,
  renderCrossPlatformServices,
  runDaemonLoop,
  serviceStatus,
  uninstallService,
} from "./daemon.ts";
import { runLearnBridge, startMcpStdioServer } from "./mcp_server.ts";
import { pluginStatus, syncFairwindPlugin } from "./plugin_sync.ts";
import {
  buildTargetManifest,
  inspectPeWithAbi,
  parseAstWithAbi,
  probeRustAbi,
  readTargetManifest,
  resolveUpstreamAgyPath,
  writeTargetManifest,
} from "./targets.ts";
import { applyUpgrade, buildAgyUpgradePlan, checkUpgrade, runBenchmarks } from "./upgrade.ts";

const TREE_SITTER_GRAMMARS = [
  "rust",
  "typescript",
  "tsx",
  "python",
  "c",
  "cpp",
  "java",
  "json",
  "bash",
] as const;

const SUBCOMMANDS = [
  "status",
  "heretic",
  "neon",
  "ghidra",
  "craft",
  "ast",
  "inspect",
  "target",
  "upgrade",
  "bench",
  "mcp",
  "api",
  "daemon",
  "service",
  "plugin",
  "learn",
] as const;

type Subcommand = (typeof SUBCOMMANDS)[number];

function resolveRoot(): string {
  const candidates = [
    process.env.FAIRWIND_ROOT,
    process.cwd(),
    dirname(dirname(process.execPath)),
    dirname(dirname(dirname(process.execPath))),
    "C:\\src\\fairwind",
  ].filter((v): v is string => Boolean(v));
  for (const dir of candidates) {
    if (
      existsSync(join(dir, "fairwind.targets.json")) ||
      existsSync(join(dir, "core")) ||
      existsSync(join(dir, "native", "fairwind-abi"))
    ) {
      return resolve(dir);
    }
  }
  return resolve(process.cwd());
}

function countFiles(dir: string): number {
  if (!existsSync(dir)) return 0;
  let total = 0;
  const stack = [dir];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    for (const entry of readdirSync(cur, { withFileTypes: true })) {
      const full = join(cur, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile()) total++;
    }
  }
  return total;
}

function runSubprocess(cmd: string[], cwd: string): number {
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "inherit", stderr: "inherit", stdin: "inherit" });
  return proc.exitCode ?? 1;
}

function printJson(payload: unknown): void {
  console.log(JSON.stringify(payload, null, 2));
}

function cmdStatus(root: string): number {
  const abi = probeRustAbi(root);
  const manifest = readTargetManifest(root);
  const isWin = process.platform === "win32";
  const agyTargetPath = join(root, "target", "agy", isWin ? "agy.exe" : "agy");
  printJson({
    ok: true,
    product: "fairwind",
    name: "fairwind",
    runtime: { bun: Bun.version, platform: process.platform, arch: process.arch },
    root,
    subcommands: SUBCOMMANDS,
    abiLoaded: abi.loaded,
    tree_sitter_grammars: TREE_SITTER_GRAMMARS,
    ltoAot: {
      cliProfile: "bun-compile-aot-trim",
      abiProfile: abi.health?.lto === "fat" ? "rust-fat-lto" : (abi.health?.profile ?? "unknown"),
      agyTargetBinary: agyTargetPath,
      agyTargetReady: existsSync(agyTargetPath),
      manifestReady: Boolean(manifest),
      repoDigest: manifest?.repoDigest ?? null,
    },
    plugin: pluginStatus(root),
    absorbed: {
      heretic: { path: "core/py/heretic", files: countFiles(join(root, "core", "py", "heretic")) },
      neon: { path: "core/ts/neon", files: countFiles(join(root, "core", "ts", "neon")) },
      craftLibs: { path: "core/rs/craft-libs", files: countFiles(join(root, "core", "rs", "craft-libs")) },
      craftAnything: { path: "core/rs/craft-anything", files: countFiles(join(root, "core", "rs", "craft-anything")) },
    },
    external: {
      ghidra: { path: "packages/ghidra", present: existsSync(join(root, "packages", "ghidra")) },
    },
    abi,
  });
  return 0;
}

function cmdHeretic(root: string, args: string[]): number {
  const dir = join(root, "core", "py", "heretic");
  if (args.length === 0 || args[0] === "--info") {
    printJson({
      ok: true,
      bridge: "uv/PyO3",
      path: dir,
      present: existsSync(dir),
      files: countFiles(dir),
      usage: "fairwind heretic <args...>",
    });
    return 0;
  }
  return runSubprocess(["uv", "run", "heretic", ...args], root);
}

function cmdNeon(root: string, args: string[]): number {
  const dir = join(root, "core", "ts", "neon");
  if (args.length === 0 || args[0] === "--info") {
    printJson({
      ok: true,
      bridge: "bun-ts",
      path: dir,
      present: existsSync(dir),
      files: countFiles(dir),
      usage: "fairwind neon <script|args...>",
    });
    return 0;
  }
  return runSubprocess(["bun", ...args], existsSync(dir) ? dir : root);
}

function cmdGhidra(root: string, args: string[]): number {
  const dir = join(root, "packages", "ghidra");
  const gradlew = join(dir, process.platform === "win32" ? "gradlew.bat" : "gradlew");
  if (args.length === 0 || args[0] === "--info") {
    printJson({
      ok: true,
      bridge: "jvm-gradle-headless",
      path: dir,
      present: existsSync(dir),
      gradlew: existsSync(gradlew),
      usage: "fairwind ghidra <args...>",
    });
    return 0;
  }
  return runSubprocess(existsSync(gradlew) ? [gradlew, ...args] : ["gradle", ...args], existsSync(dir) ? dir : root);
}

function cmdCraft(root: string, args: string[]): number {
  printJson({
    ok: true,
    bridge: "rust-c-abi",
    args,
    absorbed: {
      craftLibs: countFiles(join(root, "core", "rs", "craft-libs")),
      craftAnything: countFiles(join(root, "core", "rs", "craft-anything")),
    },
    abi: probeRustAbi(root),
  });
  return 0;
}

function cmdAst(root: string, args: string[]): number {
  if (args.length >= 2) {
    const res = parseAstWithAbi(root, args[0], resolve(root, args[1]));
    printJson({ bridge: "tree-sitter-c-abi", ...res });
    return res.ok ? 0 : 1;
  }
  const target = resolve(root, args[0] ?? "core");
  const exts: Record<string, number> = {};
  let fileCount = 0;
  if (existsSync(target)) {
    const st = statSync(target);
    if (st.isFile()) {
      fileCount = 1;
      const e = extname(target) || "(none)";
      exts[e] = 1;
    } else if (st.isDirectory()) {
      for (const entry of readdirSync(target, { withFileTypes: true })) {
        if (entry.isFile()) {
          fileCount++;
          const e = extname(entry.name) || "(none)";
          exts[e] = (exts[e] ?? 0) + 1;
        }
      }
    }
  }
  printJson({ ok: true, bridge: "tree-sitter", target, exists: existsSync(target), fileCount, extensions: exts });
  return 0;
}

function cmdInspect(root: string, args: string[]): number {
  const targetPath = args[0] ? resolve(root, args[0]) : resolveUpstreamAgyPath();
  if (!targetPath) {
    printJson({ ok: false, error: "No PE binary specified and upstream agy.exe was not found" });
    return 1;
  }
  const report = inspectPeWithAbi(root, targetPath);
  printJson({ bridge: "craft-pe-fat-lto-abi", ...report });
  return report.ok ? 0 : 1;
}

function cmdTarget(root: string, args: string[]): number {
  const action = args[0] ?? "status";
  switch (action) {
    case "scan":
    case "init": {
      const manifest = buildTargetManifest(root);
      const manifestPath = writeTargetManifest(root, manifest);
      printJson({ ok: true, action, manifestPath, manifest });
      return 0;
    }
    case "status": {
      const existing = readTargetManifest(root);
      if (existing) {
        printJson({ ok: true, action: "status", cached: true, manifest: existing });
        return 0;
      }
      const manifest = buildTargetManifest(root);
      const manifestPath = writeTargetManifest(root, manifest);
      printJson({ ok: true, action: "status", cached: false, manifestPath, manifest });
      return 0;
    }
    case "verify": {
      const check = checkUpgrade(root);
      const ok = !check.upgradeRequired;
      printJson({ ...check, ok, action: "verify" });
      return ok ? 0 : 1;
    }
    default:
      printJson({ ok: false, error: `Unknown target action: ${action}`, usage: "fairwind target <status|scan|init|verify>" });
      return 1;
  }
}

function cmdUpgrade(root: string, args: string[]): number {
  const apply = args.includes("--apply");
  const skipTests = args.includes("--skip-tests");
  const withAgy = args.includes("--with-agy");
  const dryRun = args.includes("--dry-run");

  if (withAgy) {
    const plan = buildAgyUpgradePlan(root);
    if (dryRun || !plan.command) {
      printJson({ ...plan, mode: "with-agy-dry-run" });
      return plan.command ? 0 : 1;
    }
    const res = Bun.spawnSync(plan.command, { cwd: root, stdout: "inherit", stderr: "inherit" });
    return res.exitCode ?? 1;
  }

  if (apply) {
    const report = applyUpgrade(root, { skipTests });
    printJson(report);
    return report.ok ? 0 : 1;
  }

  const report = checkUpgrade(root);
  printJson(report);
  return 0;
}

function cmdBench(root: string, args: string[]): number {
  const iterArg = args.find((a) => a.startsWith("--iterations="));
  const iterations = iterArg ? Math.max(1, Math.min(25, Number(iterArg.split("=")[1]) || 5)) : 5;
  printJson(runBenchmarks(root, iterations));
  return 0;
}

async function cmdApi(root: string, args: string[]): Promise<number> {
  const portArg = args.find((a) => a.startsWith("--port="));
  const hostArg = args.find((a) => a.startsWith("--host="));
  const port = portArg ? Number(portArg.split("=")[1]) || 4379 : 4379;
  const host = hostArg ? hostArg.split("=")[1] || "127.0.0.1" : "127.0.0.1";
  const server = startFairwindApiServer(root, host, port);
  printJson({ ok: true, service: "fairwind-api", url: `http://${server.hostname}:${server.port}` });
  await new Promise<void>((res) => {
    process.on("SIGINT", () => {
      server.stop(true);
      res();
    });
    process.on("SIGTERM", () => {
      server.stop(true);
      res();
    });
  });
  return 0;
}

async function cmdDaemon(root: string, args: string[]): Promise<number> {
  const action = args[0] ?? "run";
  if (action === "status") {
    printJson(serviceStatus(root));
    return 0;
  }
  const rest = action === "run" ? args.slice(1) : args;
  const intervalArg = rest.find((a) => a.startsWith("--interval="));
  const intervalSec = intervalArg ? Math.max(5, Number(intervalArg.split("=")[1]) || 300) : 300;
  const once = rest.includes("--once");
  const autoApply = rest.includes("--auto-apply");
  const withLearn = !rest.includes("--no-learn");
  const syncPlugin = rest.includes("--sync-plugin");
  const skipTests = rest.includes("--skip-tests");
  return runDaemonLoop(root, { intervalSec, once, autoApply, withLearn, syncPlugin, skipTests });
}

function cmdService(root: string, args: string[]): number {
  const action = args[0] ?? "status";
  switch (action) {
    case "render":
      printJson(renderCrossPlatformServices(root));
      return 0;
    case "install": {
      const res = installService(root);
      printJson(res);
      return res.ok ? 0 : 1;
    }
    case "uninstall": {
      const res = uninstallService(root);
      printJson(res);
      return res.ok ? 0 : 1;
    }
    case "status":
      printJson(serviceStatus(root));
      return 0;
    default:
      printJson({ ok: false, error: `Unknown service action: ${action}`, usage: "fairwind service <status|render|install|uninstall>" });
      return 1;
  }
}

function cmdPlugin(root: string, args: string[]): number {
  const action = args[0] ?? "status";
  if (action === "sync") {
    const res = syncFairwindPlugin(root, {
      runInstall: args.includes("--install"),
      validate: !args.includes("--no-validate"),
    });
    printJson(res);
    return res.ok ? 0 : 1;
  }
  printJson(pluginStatus(root));
  return 0;
}

function cmdLearn(args: string[]): number {
  const action = args[0] ?? "status";
  const query = args.slice(1).join(" ");
  const res = runLearnBridge(action, query || undefined);
  printJson(res);
  return res.ok ? 0 : 1;
}

async function main(): Promise<number> {
  const [sub, ...rest] = Bun.argv.slice(2);
  const root = resolveRoot();
  if (!sub || sub === "--help" || sub === "-h") {
    printJson({
      ok: true,
      product: "fairwind",
      usage: `fairwind <${SUBCOMMANDS.join("|")}> [args...]`,
      subcommands: SUBCOMMANDS,
    });
    return 0;
  }
  switch (sub as Subcommand) {
    case "status":
      return cmdStatus(root);
    case "heretic":
      return cmdHeretic(root, rest);
    case "neon":
      return cmdNeon(root, rest);
    case "ghidra":
      return cmdGhidra(root, rest);
    case "craft":
      return cmdCraft(root, rest);
    case "ast":
      return cmdAst(root, rest);
    case "inspect":
      return cmdInspect(root, rest);
    case "target":
      return cmdTarget(root, rest);
    case "upgrade":
      return cmdUpgrade(root, rest);
    case "bench":
      return cmdBench(root, rest);
    case "mcp":
      await startMcpStdioServer(root);
      return 0;
    case "api":
      return cmdApi(root, rest);
    case "daemon":
      return cmdDaemon(root, rest);
    case "service":
      return cmdService(root, rest);
    case "plugin":
      return cmdPlugin(root, rest);
    case "learn":
      return cmdLearn(rest);
    default:
      console.error(JSON.stringify({ ok: false, error: `Unknown subcommand: ${sub}` }));
      return 1;
  }
}

process.exit(await main());
