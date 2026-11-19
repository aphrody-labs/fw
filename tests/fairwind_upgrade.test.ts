import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadTargetsConfig, resolveUpstreamAgyPath } from "../src/targets.ts";
import { buildAgyUpgradePlan, checkUpgrade, promoteBinaryAtomic } from "../src/upgrade.ts";

const ROOT = process.cwd();
const IS_WIN = process.platform === "win32";

describe("fairwind target and self-upgrade pipeline", () => {
  test("fairwind.targets.json registers all in-repo code modules", () => {
    const cfg = loadTargetsConfig(ROOT);
    const ids = cfg.modules.map((m) => m.id);
    for (const expected of [
      "native-pe",
      "native-abi",
      "native-agy",
      "fairwind-cli",
      "fairwind-tests",
      "core-heretic",
      "core-neon",
      "core-craft-libs",
      "core-craft-anything",
      "external-ghidra",
    ]) {
      expect(ids).toContain(expected);
    }
    expect(cfg.agent.targetDir).toBe("target/agy");
    expect(cfg.agent.upstreamStrategy).toBe("read-only-signed-upstream");
  });

  test("promoteBinaryAtomic stages, replaces, and preserves .prev rollback backup", () => {
    const dir = mkdtempSync(join(tmpdir(), "fairwind-promote-"));
    try {
      const dest = join(dir, "agy.exe");
      const v1 = join(dir, "v1.bin");
      const v2 = join(dir, "v2.bin");
      writeFileSync(v1, "version-1");
      writeFileSync(v2, "version-2");

      promoteBinaryAtomic(v1, dest);
      expect(readFileSync(dest, "utf8")).toBe("version-1");

      promoteBinaryAtomic(v2, dest);
      expect(readFileSync(dest, "utf8")).toBe("version-2");
      expect(existsSync(`${dest}.prev`)).toBe(true);
      expect(readFileSync(`${dest}.prev`, "utf8")).toBe("version-1");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("upgrade --check and --with-agy --dry-run emit structured JSON plans", () => {
    const check = checkUpgrade(ROOT);
    expect(check.ok).toBe(true);
    expect(check.mode).toBe("check");
    expect(check.currentRepoDigest.length).toBe(64);
    expect(check.plan.length).toBeGreaterThanOrEqual(4);

    const plan = buildAgyUpgradePlan(ROOT);
    expect(plan.ok).toBe(true);
    expect(plan.prompt).toContain("fairwind.targets.json");
  });

  test("native Fat-LTO agy binary runs --fairwind-info, --fairwind-inspect, and upstream delegation", () => {
    const targetAgy = join(ROOT, "target", "agy", IS_WIN ? "agy.exe" : "agy");
    const fallbackAgy = join(ROOT, "native", "fairwind-agy", "target", "release", IS_WIN ? "agy.exe" : "agy");
    const agyBin = existsSync(targetAgy) ? targetAgy : fallbackAgy;
    expect(existsSync(agyBin)).toBe(true);

    const info = Bun.spawnSync([agyBin, "--fairwind-info"], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
    expect(info.exitCode).toBe(0);
    const infoJson = JSON.parse(info.stdout.toString());
    expect(infoJson.ok).toBe(true);
    expect(infoJson.binary).toBe("fairwind-agy");
    expect(infoJson.abiVersion).toBe(4);
    expect(infoJson.build.lto).toBe("fat");
    expect(infoJson.build.aot).toBe(true);

    if (IS_WIN) {
      const inspect = Bun.spawnSync([agyBin, "--fairwind-inspect", agyBin], {
        cwd: ROOT,
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(inspect.exitCode).toBe(0);
      const pe = JSON.parse(inspect.stdout.toString());
      expect(pe.ok).toBe(true);
      expect(pe.pe32Plus).toBe(true);
      expect(pe.runtimeKind).toBe("native-lto-pe");

      const upstream = resolveUpstreamAgyPath();
      if (upstream) {
        const upInspect = Bun.spawnSync([agyBin, "--fairwind-inspect", upstream], {
          cwd: ROOT,
          stdout: "pipe",
          stderr: "pipe",
        });
        expect(upInspect.exitCode).toBe(0);
        const upPe = JSON.parse(upInspect.stdout.toString());
        expect(upPe.ok).toBe(true);
        expect(upPe.hasAuthenticode).toBe(true);
        expect(upPe.runtimeKind).toBe("google-antigravity-pe");

        const ver = Bun.spawnSync([agyBin, "--version"], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
        const upVer = Bun.spawnSync([upstream, "--version"], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
        expect(ver.exitCode).toBe(0);
        expect(ver.stdout.toString().trim()).toBe(upVer.stdout.toString().trim());
      }
    }
  });
});
