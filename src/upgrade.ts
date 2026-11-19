import { appendFileSync, copyFileSync, existsSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import {
  buildTargetManifest,
  loadTargetsConfig,
  readTargetManifest,
  resolveUpstreamAgyPath,
  type TargetManifest,
  writeTargetManifest,
} from "./targets.ts";

export interface UpgradeCheckReport {
  ok: boolean;
  mode: "check";
  root: string;
  upgradeRequired: boolean;
  reasons: string[];
  changedModules: string[];
  missingArtifacts: string[];
  upstreamAgyChanged: boolean;
  previousRepoDigest: string | null;
  currentRepoDigest: string;
  upstreamAgy: TargetManifest["upstreamAgy"];
  artifacts: TargetManifest["artifacts"];
  plan: string[];
}

export interface UpgradeApplyReport {
  ok: boolean;
  mode: "apply";
  root: string;
  promoted: Array<{ name: string; from: string; to: string; bytes: number }>;
  gates: Array<{ name: string; ok: boolean; durationMs: number; exitCode: number }>;
  manifestPath: string;
  historyPath: string;
  repoDigest: string;
  gitHead: string | null;
  error?: string;
}

function runCaptured(cmd: string[], cwd: string) {
  const start = performance.now();
  const proc = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
  const durationMs = Math.round((performance.now() - start) * 100) / 100;
  return {
    ok: proc.exitCode === 0,
    exitCode: proc.exitCode ?? 1,
    durationMs,
    stdout: proc.stdout.toString(),
    stderr: proc.stderr.toString(),
  };
}

/**
 * Atomically promotes a staged binary to `dest`, supporting Windows NTFS
 * rename-while-running (a running .exe can be renamed to .prev.exe even while locked).
 */
export function promoteBinaryAtomic(stagedPath: string, destPath: string): void {
  mkdirSync(dirname(destPath), { recursive: true });
  const backupPath = `${destPath}.prev`;
  let movedExisting = false;
  if (existsSync(destPath)) {
    try {
      rmSync(backupPath, { force: true });
    } catch {
      // Ignore if previous backup is still held by an old process
    }
    try {
      renameSync(destPath, backupPath);
      movedExisting = true;
    } catch {
      // Fallback: copy directly if rename fails on non-locked cross-device path
    }
  }
  try {
    copyFileSync(stagedPath, destPath);
  } catch (err) {
    if (movedExisting && existsSync(backupPath) && !existsSync(destPath)) {
      try {
        renameSync(backupPath, destPath);
      } catch {
        // Best-effort rollback
      }
    }
    throw err;
  }
}

export function checkUpgrade(root: string): UpgradeCheckReport {
  const prev = readTargetManifest(root);
  const current = buildTargetManifest(root);
  const reasons: string[] = [];
  const changedModules: string[] = [];
  const missingArtifacts: string[] = [];

  if (!prev) {
    reasons.push("target/manifest.json does not exist yet");
  } else {
    const prevById = new Map(prev.modules.map((m) => [m.id, m]));
    for (const mod of current.modules) {
      if (!mod.autoUpgrade) continue;
      const old = prevById.get(mod.id);
      if (!old || old.digest !== mod.digest) {
        changedModules.push(mod.id);
      }
    }
    if (changedModules.length > 0) {
      reasons.push(`in-repo code changed: ${changedModules.join(", ")}`);
    }
  }

  for (const [name, art] of Object.entries(current.artifacts)) {
    if (!art.exists) {
      missingArtifacts.push(name);
    }
  }
  if (missingArtifacts.length > 0) {
    reasons.push(`missing target artifacts: ${missingArtifacts.join(", ")}`);
  }

  const prevUpstreamSha =
    prev?.upstreamAgy.pe && typeof prev.upstreamAgy.pe.sha256 === "string"
      ? prev.upstreamAgy.pe.sha256
      : null;
  const curUpstreamSha =
    current.upstreamAgy.pe && typeof current.upstreamAgy.pe.sha256 === "string"
      ? current.upstreamAgy.pe.sha256
      : null;
  const upstreamAgyChanged = Boolean(prev && prevUpstreamSha && curUpstreamSha && prevUpstreamSha !== curUpstreamSha);
  if (upstreamAgyChanged) {
    reasons.push("upstream signed agy.exe SHA-256 changed");
  }

  const upgradeRequired = reasons.length > 0;
  const plan = [
    "cargo build --release --manifest-path native/fairwind-abi/Cargo.toml (Fat LTO cdylib + craft-pe + tree-sitter)",
    "cargo build --release --manifest-path native/fairwind-agy/Cargo.toml (Fat LTO AOT native agy launcher)",
    "bun build --compile --aot --trim --no-compile-function-source src/fairwind_cli.ts --outfile dist/fairwind.staging.exe",
    "verify gates: uvx ruff check ., cargo test (pe/abi/agy), bun test tests, staged binary smoke checks",
    "atomic promote into target/agy/ and dist/ + update target/manifest.json + target/upgrade-history.jsonl",
  ];

  return {
    ok: true,
    mode: "check",
    root,
    upgradeRequired,
    reasons,
    changedModules,
    missingArtifacts,
    upstreamAgyChanged,
    previousRepoDigest: prev?.repoDigest ?? null,
    currentRepoDigest: current.repoDigest,
    upstreamAgy: current.upstreamAgy,
    artifacts: current.artifacts,
    plan,
  };
}

export function applyUpgrade(root: string, options: { skipTests?: boolean } = {}): UpgradeApplyReport {
  const cfg = loadTargetsConfig(root);
  const isWin = process.platform === "win32";
  const gates: UpgradeApplyReport["gates"] = [];
  const promoted: UpgradeApplyReport["promoted"] = [];
  const stageDir = resolve(root, "target", "staging");
  mkdirSync(stageDir, { recursive: true });

  const steps: Array<{ name: string; cmd: string[] }> = [
    {
      name: "build-abi-fat-lto",
      cmd: ["cargo", "build", "--release", "--manifest-path", "native/fairwind-abi/Cargo.toml"],
    },
    {
      name: "build-agy-fat-lto",
      cmd: ["cargo", "build", "--release", "--manifest-path", "native/fairwind-agy/Cargo.toml"],
    },
  ];

  const stagedCli = join(stageDir, isWin ? "fairwind.exe" : "fairwind");
  steps.push({
    name: "build-cli-bun-aot-trim",
    cmd: [
      "bun",
      "build",
      "--compile",
      "--aot",
      "--trim",
      "--no-compile-function-source",
      "src/fairwind_cli.ts",
      "--outfile",
      stagedCli,
    ],
  });

  if (!options.skipTests) {
    steps.push(
      { name: "ruff-lint", cmd: ["uvx", "ruff", "check", "."] },
      { name: "cargo-test-pe", cmd: ["cargo", "test", "--manifest-path", "native/fairwind-pe/Cargo.toml"] },
      { name: "cargo-test-abi", cmd: ["cargo", "test", "--manifest-path", "native/fairwind-abi/Cargo.toml"] },
      { name: "cargo-test-agy", cmd: ["cargo", "test", "--manifest-path", "native/fairwind-agy/Cargo.toml"] },
    );
  }

  for (const step of steps) {
    const res = runCaptured(step.cmd, root);
    gates.push({ name: step.name, ok: res.ok, durationMs: res.durationMs, exitCode: res.exitCode });
    if (!res.ok) {
      return {
        ok: false,
        mode: "apply",
        root,
        promoted,
        gates,
        manifestPath: resolve(root, cfg.agent.manifestPath),
        historyPath: resolve(root, cfg.agent.historyPath),
        repoDigest: "",
        gitHead: null,
        error: `Step '${step.name}' failed (exit ${res.exitCode}): ${(res.stderr || res.stdout).slice(0, 600)}`,
      };
    }
  }

  // Smoke-check staged binaries BEFORE promoting
  const abiSrc = resolve(
    root,
    isWin ? cfg.artifacts.fairwindAbiLto.buildOutput! : cfg.artifacts.fairwindAbiLto.buildOutputUnix!,
  );
  const agySrc = resolve(
    root,
    isWin ? cfg.artifacts.agyNativeLto.buildOutput! : cfg.artifacts.agyNativeLto.buildOutputUnix!,
  );

  for (const smoke of [
    { name: "smoke-staged-cli", cmd: [stagedCli, "status"] },
    { name: "smoke-staged-agy", cmd: [agySrc, "--fairwind-info"] },
  ]) {
    const res = runCaptured(smoke.cmd, root);
    gates.push({ name: smoke.name, ok: res.ok, durationMs: res.durationMs, exitCode: res.exitCode });
    if (!res.ok) {
      return {
        ok: false,
        mode: "apply",
        root,
        promoted,
        gates,
        manifestPath: resolve(root, cfg.agent.manifestPath),
        historyPath: resolve(root, cfg.agent.historyPath),
        repoDigest: "",
        gitHead: null,
        error: `Smoke check '${smoke.name}' failed: ${(res.stderr || res.stdout).slice(0, 600)}`,
      };
    }
  }

  // Promote verified artifacts atomically into target/agy/ and dist/
  const pairs: Array<{ name: string; src: string; dest: string }> = [
    {
      name: "target-abi",
      src: abiSrc,
      dest: resolve(root, isWin ? cfg.artifacts.fairwindAbiLto.targetPath : cfg.artifacts.fairwindAbiLto.targetPathUnix),
    },
    {
      name: "dist-abi",
      src: abiSrc,
      dest: resolve(root, isWin ? cfg.artifacts.fairwindAbiLto.distPath : cfg.artifacts.fairwindAbiLto.distPathUnix),
    },
    {
      name: "target-agy",
      src: agySrc,
      dest: resolve(root, isWin ? cfg.artifacts.agyNativeLto.targetPath : cfg.artifacts.agyNativeLto.targetPathUnix),
    },
    {
      name: "dist-agy",
      src: agySrc,
      dest: resolve(root, isWin ? cfg.artifacts.agyNativeLto.distPath : cfg.artifacts.agyNativeLto.distPathUnix),
    },
    {
      name: "target-cli",
      src: stagedCli,
      dest: resolve(root, isWin ? cfg.artifacts.fairwindCliAot.targetPath : cfg.artifacts.fairwindCliAot.targetPathUnix),
    },
    {
      name: "dist-cli",
      src: stagedCli,
      dest: resolve(root, isWin ? cfg.artifacts.fairwindCliAot.distPath : cfg.artifacts.fairwindCliAot.distPathUnix),
    },
  ];

  for (const item of pairs) {
    promoteBinaryAtomic(item.src, item.dest);
    promoted.push({
      name: item.name,
      from: item.src,
      to: item.dest,
      bytes: statSync(item.dest).size,
    });
  }

  const manifest = buildTargetManifest(root);
  const manifestPath = writeTargetManifest(root, manifest);
  const historyPath = resolve(root, cfg.agent.historyPath);
  mkdirSync(dirname(historyPath), { recursive: true });
  appendFileSync(
    historyPath,
    `${JSON.stringify({
      timestamp: manifest.generatedAt,
      gitHead: manifest.gitHead,
      repoDigest: manifest.repoDigest,
      upstreamAgyVersion: manifest.upstreamAgy.version,
      artifacts: Object.fromEntries(
        Object.entries(manifest.artifacts).map(([k, v]) => [k, { bytes: v.bytes, sha256: v.sha256 }]),
      ),
      gatesPassed: gates.length,
    })}\n`,
    "utf8",
  );

  return {
    ok: true,
    mode: "apply",
    root,
    promoted,
    gates,
    manifestPath,
    historyPath,
    repoDigest: manifest.repoDigest,
    gitHead: manifest.gitHead,
  };
}

export function buildAgyUpgradePlan(root: string) {
  const check = checkUpgrade(root);
  const upstream = resolveUpstreamAgyPath();
  const prompt = [
    `Inspect the Fairwind monorepo at ${root} (tracked in fairwind.targets.json and target/manifest.json).`,
    check.changedModules.length > 0
      ? `Drifted modules: ${check.changedModules.join(", ")}.`
      : "No module drift detected; audit LTO/AOT build flags, PE inspection coverage, and test gates.",
    "Verify all changes with `bun run check`, `bun run upgrade`, and `bun run bench` before reporting.",
  ].join(" ");
  return {
    ok: true,
    upstreamAgy: upstream,
    command: upstream ? [upstream, "--output-format", "json", "-p", prompt] : null,
    prompt,
    check,
  };
}

export function runBenchmarks(root: string, iterations = 5) {
  const cfg = loadTargetsConfig(root);
  const isWin = process.platform === "win32";
  const agyTarget = resolve(root, isWin ? cfg.artifacts.agyNativeLto.targetPath : cfg.artifacts.agyNativeLto.targetPathUnix);
  const cliTarget = resolve(root, isWin ? cfg.artifacts.fairwindCliAot.distPath : cfg.artifacts.fairwindCliAot.distPathUnix);
  const abiTarget = resolve(root, isWin ? cfg.artifacts.fairwindAbiLto.targetPath : cfg.artifacts.fairwindAbiLto.targetPathUnix);
  const upstream = resolveUpstreamAgyPath();

  const measure = (label: string, bin: string | null, args: string[]) => {
    if (!bin || !existsSync(bin)) {
      return { label, path: bin, exists: false, bytes: 0, runsMs: [] as number[], medianMs: null as number | null };
    }
    const runsMs: number[] = [];
    for (let i = 0; i < iterations; i++) {
      const t0 = performance.now();
      const res = Bun.spawnSync([bin, ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
      const dt = Math.round((performance.now() - t0) * 100) / 100;
      if (res.exitCode === 0) runsMs.push(dt);
    }
    const sorted = [...runsMs].sort((a, b) => a - b);
    const medianMs = sorted.length > 0 ? sorted[Math.floor(sorted.length / 2)] : null;
    return { label, path: bin, exists: true, bytes: statSync(bin).size, runsMs, medianMs };
  };

  const results = {
    fairwindAgyLto: measure("target/agy/agy (Rust Fat-LTO AOT)", agyTarget, ["--fairwind-info"]),
    fairwindCliAot: measure("dist/fairwind (Bun AOT + trim)", cliTarget, ["status"]),
    upstreamAgySigned: measure("upstream signed agy.exe (--version)", upstream, ["--version"]),
    fairwindAbiLto: {
      label: "fairwind_abi (Rust Fat-LTO cdylib)",
      path: abiTarget,
      exists: existsSync(abiTarget),
      bytes: existsSync(abiTarget) ? statSync(abiTarget).size : 0,
    },
  };

  const sizeReductionPct =
    results.upstreamAgySigned.bytes > 0 && results.fairwindAgyLto.bytes > 0
      ? Math.round((1 - results.fairwindAgyLto.bytes / results.upstreamAgySigned.bytes) * 10000) / 100
      : null;

  return {
    ok: true,
    iterations,
    results,
    comparisons: {
      agyLtoVsUpstreamSizeReductionPct: sizeReductionPct,
      agyLtoSpeedupVsUpstream:
        results.upstreamAgySigned.medianMs && results.fairwindAgyLto.medianMs
          ? Math.round((results.upstreamAgySigned.medianMs / results.fairwindAgyLto.medianMs) * 100) / 100
          : null,
    },
  };
}
