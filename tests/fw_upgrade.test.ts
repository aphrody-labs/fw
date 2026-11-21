import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadTargetsConfig } from "../src/targets.ts";
import { checkUpgrade, promoteBinaryAtomic } from "../src/upgrade.ts";

const ROOT = process.cwd();

describe("fw target and self-upgrade pipeline", () => {
  test("fw.targets.json registers all in-repo code modules", () => {
    const cfg = loadTargetsConfig(ROOT);
    const ids = cfg.modules.map((m) => m.id);
    for (const expected of [
      "native-abi",
      "fw-cli",
      "fw-tests",
      "v3-unified-workspace",
      "external-ghidra",
    ]) {
      expect(ids).toContain(expected);
    }
    expect(cfg.agent.targetDir).toBe("target/fw");
    expect(Object.keys(cfg.artifacts).sort()).toEqual(["fwAbiLto", "fwCliAot"]);
  });

  test("promoteBinaryAtomic stages, replaces, and preserves .prev rollback backup", () => {
    const dir = mkdtempSync(join(tmpdir(), "fw-promote-"));
    try {
      const dest = join(dir, "fw-plugin.exe");
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

  test("upgrade --check emits a structured JSON plan", () => {
    const check = checkUpgrade(ROOT);
    expect(check.ok).toBe(true);
    expect(check.mode).toBe("check");
    expect(check.currentRepoDigest.length).toBe(64);
    expect(check.plan.length).toBeGreaterThanOrEqual(4);
  });
});
