#!/usr/bin/env bun
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import {
  brainIndexRepo,
  brainSearch,
  buildA2AAgentCard,
  buildOpenApiSpec,
  DEFAULT_ROUTER_PROVIDERS,
  handleA2AMessage,
  handleAcpMessage,
  loadSoul,
  routeModelRequest,
} from "./ai_stack.ts";
import { startFwApiServer } from "./api.ts";
import {
  installService,
  renderCrossPlatformServices,
  runDaemonLoop,
  serviceStatus,
  uninstallService,
} from "./daemon.ts";
import { evaluateDecisions, runSelfHostedAiWorkflow } from "./decisions.ts";
import { LOW_LEVEL_LTO_AOT_MATRIX, verifyLowLevelToolchains } from "./lowlevel.ts";
import { runLearnBridge, startMcpStdioServer } from "./mcp_server.ts";
import { auditDeduplication, pluginStatus, syncFwPlugin } from "./plugin_sync.ts";
import {
  astQueryWithAbi,
  astSymbolsWithAbi,
  buildTargetManifest,
  cryptoDigestWithAbi,
  disasmX64WithAbi,
  gitInspectWithAbi,
  identifyFileWithAbi,
  inspectPeWithAbi,
  parseAstWithAbi,
  probeRustAbi,
  readTargetManifest,
  resolveUpstreamAgyPath,
  writeTargetManifest,
} from "./targets.ts";
import { generateFwWtaConfig, getTerminalTuiStatus, runTuiTestVerify, verifyFwSelfTui } from "./terminal_tui.ts";
import { renderFwTui } from "./tui.ts";
import { applyUpgrade, buildAgyUpgradePlan, checkUpgrade, runBenchmarks } from "./upgrade.ts";
import { listVfsMounts, vfsList, vfsRead, vfsSnapshot, vfsSnapshots, vfsWrite } from "./vfs.ts";

const TREE_SITTER_GRAMMARS = [
  "rust",
  "typescript",
  "tsx",
  "javascript",
  "python",
  "c",
  "cpp",
  "java",
  "go",
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
  "identify",
  "disasm",
  "target",
  "upgrade",
  "bench",
  "mcp",
  "acp",
  "a2a",
  "api",
  "web",
  "tui",
  "vfs",
  "brain",
  "soul",
  "router",
  "decide",
  "workflow",
  "lowlevel",
  "train",
  "crypto",
  "git",
  "dedupe",
  "daemon",
  "service",
  "plugin",
  "learn",
] as const;

type Subcommand = (typeof SUBCOMMANDS)[number];

function resolveRoot(): string {
  const candidates = [
    process.env.FW_ROOT,
    process.env.FAIRWIND_ROOT, // legacy fallback for units installed before the rename
    process.cwd(),
    dirname(dirname(process.execPath)),
    dirname(dirname(dirname(process.execPath))),
    "C:\\src\\fw",
  ].filter((v): v is string => Boolean(v));
  for (const dir of candidates) {
    if (
      existsSync(join(dir, "fw.targets.json")) ||
      existsSync(join(dir, "core")) ||
      existsSync(join(dir, "native", "fw-abi"))
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
    product: "fw",
    name: "fw",
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
      tuiTest: { path: "core/rs/tui-test", files: countFiles(join(root, "core", "rs", "tui-test")) },
      wta: { path: "core/rs/wta", files: countFiles(join(root, "core", "rs", "wta")) },
      nativePe: { path: "native/fw-pe/src", files: countFiles(join(root, "native", "fw-pe", "src")) },
      nativeSniff: { path: "native/fw-sniff/src", files: countFiles(join(root, "native", "fw-sniff", "src")) },
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
      usage: "fw heretic <args...>",
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
      usage: "fw neon <script|args...>",
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
      usage: "fw ghidra <args...>",
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
  if (args[0] === "symbols" && args.length >= 3) {
    const res = astSymbolsWithAbi(root, args[1], resolve(root, args[2]));
    printJson({ bridge: "tree-sitter-symbols-c-abi", ...res });
    return res.ok ? 0 : 1;
  }
  if (args[0] === "query" && args.length >= 4) {
    const res = astQueryWithAbi(root, args[1], resolve(root, args[2]), args.slice(3).join(" "));
    printJson({ bridge: "tree-sitter-query-c-abi", ...res });
    return res.ok ? 0 : 1;
  }
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
      printJson({ ok: false, error: `Unknown target action: ${action}`, usage: "fw target <status|scan|init|verify>" });
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
  const server = startFwApiServer(root, host, port);
  printJson({ ok: true, service: "fw-api", url: `http://${server.hostname}:${server.port}` });
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
      printJson({ ok: false, error: `Unknown service action: ${action}`, usage: "fw service <status|render|install|uninstall>" });
      return 1;
  }
}

function cmdPlugin(root: string, args: string[]): number {
  const action = args[0] ?? "status";
  if (action === "sync") {
    const res = syncFwPlugin(root, {
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
      product: "fw",
      usage: `fw <${SUBCOMMANDS.join("|")}> [args...]`,
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
    case "identify": {
      const target = rest[0] ? resolve(root, rest[0]) : resolveUpstreamAgyPath();
      if (!target) {
        printJson({ ok: false, error: "no target file specified" });
        return 1;
      }
      const res = identifyFileWithAbi(root, target);
      printJson(res);
      return res.ok ? 0 : 1;
    }
    case "disasm": {
      const target = rest[0] ? resolve(root, rest[0]) : resolveUpstreamAgyPath();
      const max = rest[1] ? Number(rest[1]) || 12 : 12;
      if (!target) {
        printJson({ ok: false, error: "no PE binary specified" });
        return 1;
      }
      const res = disasmX64WithAbi(root, target, max);
      printJson(res);
      return res.ok ? 0 : 1;
    }
    case "target":
      return cmdTarget(root, rest);
    case "upgrade":
      return cmdUpgrade(root, rest);
    case "bench":
      return cmdBench(root, rest);
    case "mcp":
      await startMcpStdioServer(root);
      return 0;
    case "acp": {
      const method = rest[0] ?? "initialize";
      const prompt = rest.slice(1).join(" ");
      printJson(handleAcpMessage(root, { id: 1, method, params: prompt ? { prompt } : {} }));
      return 0;
    }
    case "a2a": {
      if (rest[0] === "card" || rest.length === 0) {
        printJson(buildA2AAgentCard());
        return 0;
      }
      printJson(handleA2AMessage(root, { id: 1, method: rest[0], params: { prompt: rest.slice(1).join(" ") } }));
      return 0;
    }
    case "api":
    case "web":
      return cmdApi(root, rest);
    case "tui": {
      const act = rest[0];
      if (act === "status") {
        printJson(getTerminalTuiStatus(root));
        return 0;
      }
      if (act === "wta-config") {
        printJson(generateFwWtaConfig(root));
        return 0;
      }
      if (act === "verify-self") {
        const res = verifyFwSelfTui(root);
        printJson(res);
        return res.ok ? 0 : 1;
      }
      if (act === "verify") {
        const expectText = rest[1] ?? "fw";
        const sepIdx = rest.indexOf("--");
        const cmd = sepIdx >= 0 ? rest.slice(sepIdx + 1) : rest.slice(2);
        if (cmd.length === 0) {
          printJson({ ok: false, error: "Usage: fw tui verify <expectText> -- <command...>" });
          return 1;
        }
        const res = runTuiTestVerify(root, cmd, expectText);
        printJson(res);
        return res.ok ? 0 : 1;
      }
      console.log(renderFwTui(root));
      return 0;
    }
    case "vfs": {
      const act = rest[0] ?? "mounts";
      if (act === "mounts") {
        printJson(listVfsMounts(root));
        return 0;
      }
      if (act === "ls") {
        printJson(vfsList(root, rest[1] ?? "/"));
        return 0;
      }
      if (act === "read" && rest[1]) {
        const r = vfsRead(root, rest[1]);
        printJson(r);
        return r.ok ? 0 : 1;
      }
      if (act === "write" && rest[1]) {
        printJson(vfsWrite(root, rest[1], rest.slice(2).join(" ")));
        return 0;
      }
      if (act === "snapshot") {
        printJson(vfsSnapshot(root, rest[1]));
        return 0;
      }
      if (act === "snapshots") {
        printJson(vfsSnapshots(root));
        return 0;
      }
      printJson({ ok: false, error: `Unknown vfs action: ${act}` });
      return 1;
    }
    case "brain": {
      const act = rest[0] ?? "search";
      if (act === "index") {
        printJson(brainIndexRepo(root));
        return 0;
      }
      printJson(brainSearch(root, rest.slice(1).join(" ") || "fw"));
      return 0;
    }
    case "soul":
      printJson({ ok: true, soul: loadSoul(root), openapi: buildOpenApiSpec().info });
      return 0;
    case "router":
      printJson({
        ok: true,
        providers: DEFAULT_ROUTER_PROVIDERS,
        decision: routeModelRequest(rest[0], rest[1] ?? "chat"),
      });
      return 0;
    case "decide": {
      const input = rest.join(" ") || "Build and verify Fw Rust Fat-LTO binary";
      const res = await evaluateDecisions({
        model: "gemini-decision-1",
        input,
        mode: process.env.GEMINI_API_KEY ? "auto" : "local",
        questions: {
          needs_build: { type: "noul", instructions: "Does this input request building, compiling, or testing code?" },
          route: {
            type: "choice",
            instructions: "Choose the optimal handler queue.",
            criteria: {
              lowlevel: "Rust, C, C++, Assembly, WebAssembly, CUDA, WebGPU, PE/ELF binary work.",
              cloud: "Cloudflare Workers, Vercel Functions, Durable Objects, R2, D1, edge deployment.",
              ai_train: "LLM training, fine-tuning, PEFT LoRA, Gemma, Gemini, datasets, abliteration.",
              general: "General documentation, inspection, or status query.",
            },
          },
        },
      });
      printJson(res);
      return 0;
    }
    case "workflow": {
      const input = rest.join(" ") || "Optimize and verify Fw end-to-end self-hosted AI workflow";
      printJson(await runSelfHostedAiWorkflow(root, input, "local"));
      return 0;
    }
    case "lowlevel": {
      const act = rest[0] ?? "matrix";
      if (act === "verify") {
        const res = await verifyLowLevelToolchains(root);
        printJson(res);
        return res.ok ? 0 : 1;
      }
      printJson({ ok: true, matrix: LOW_LEVEL_LTO_AOT_MATRIX });
      return 0;
    }
    case "train": {
      const act = rest[0] ?? "plan";
      return runSubprocess(["uv", "run", "python", "core/py/fw_ml/pipeline.py", act, ...rest.slice(1)], root);
    }
    case "crypto": {
      const res = cryptoDigestWithAbi(root, rest.join(" ") || "fw");
      printJson(res);
      return res.ok ? 0 : 1;
    }
    case "git": {
      const res = gitInspectWithAbi(root, rest[0] ? resolve(root, rest[0]) : root);
      printJson(res);
      return res.ok ? 0 : 1;
    }
    case "dedupe": {
      const res = auditDeduplication(root);
      printJson(res);
      return res.ok ? 0 : 1;
    }
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
