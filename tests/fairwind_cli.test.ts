import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();

describe("fairwind unified product", () => {
  test("core layout contains all four absorbed components", () => {
    expect(existsSync(join(ROOT, "core", "py", "heretic"))).toBe(true);
    expect(existsSync(join(ROOT, "core", "ts", "neon"))).toBe(true);
    expect(existsSync(join(ROOT, "core", "rs", "craft-libs"))).toBe(true);
    expect(existsSync(join(ROOT, "core", "rs", "craft-anything"))).toBe(true);
  });

  test("fairwind_cli.ts status returns valid JSON with abiLoaded true", () => {
    const proc = Bun.spawnSync(["bun", "src/fairwind_cli.ts", "status"], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
    expect(proc.exitCode).toBe(0);
    const data = JSON.parse(proc.stdout.toString());
    expect(data.ok).toBe(true);
    expect(data.product).toBe("fairwind");
    expect(data.abiLoaded).toBe(true);
    expect(data.absorbed.heretic.files).toBeGreaterThan(0);
    expect(data.absorbed.neon.files).toBeGreaterThan(0);
    expect(data.absorbed.craftLibs.files).toBeGreaterThan(0);
    expect(data.absorbed.craftAnything.files).toBeGreaterThan(0);
  });
});
