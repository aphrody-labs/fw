import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { findAbiLibrary } from "../src/targets.ts";

const ROOT = process.cwd();
const IS_WIN = process.platform === "win32";

describe("fw unified product", () => {
  test("core layout contains first-party unified modules and native LTO/AOT crates (zero dead upstream copies)", () => {
    expect(existsSync(join(ROOT, "LICENSE"))).toBe(true);
    expect(existsSync(join(ROOT, "THIRD_PARTY_NOTICES.md"))).toBe(true);
    expect(existsSync(join(ROOT, "core", "py", "heretic"))).toBe(false);
    expect(existsSync(join(ROOT, "core", "ts", "neon"))).toBe(false);
    expect(existsSync(join(ROOT, "core", "rs", "craft-libs"))).toBe(false);
    expect(existsSync(join(ROOT, "core", "rs", "craft-anything"))).toBe(false);
    expect(existsSync(join(ROOT, "native", "fw-pe"))).toBe(false);
    expect(existsSync(join(ROOT, "native", "fw-sniff"))).toBe(false);
    expect(existsSync(join(ROOT, "native", "fw-abi", "Cargo.toml"))).toBe(true);
    expect(existsSync(join(ROOT, "fw.targets.json"))).toBe(true);
    expect(existsSync(join(ROOT, ".agents", "rules", "fw-auto-upgrade.md"))).toBe(true);
  });

  test("fw_cli.ts status returns valid JSON with the C ABI loaded", () => {
    const proc = Bun.spawnSync(["bun", "src/fw_cli.ts", "status"], {
      cwd: ROOT,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(proc.exitCode).toBe(0);
    const data = JSON.parse(proc.stdout.toString());
    expect(data.ok).toBe(true);
    expect(data.product).toBe("fw");
    expect(data.abiLoaded).toBe(true);
    expect(data.abi.version).toBe(4);
    expect(data.abi.health.capabilities).toContain("craft-pe");
    expect(data.abi.health.capabilities).toContain("craft-sniff");
    expect(data.abi.health.capabilities).toContain("iced-x86-disasm");
    expect(data.abi.health.capabilities).toContain("gguf-ggml");
    expect(data.subcommands).toContain("inspect");
    expect(data.subcommands).toContain("target");
    expect(data.subcommands).toContain("upgrade");
    expect(data.subcommands).toContain("bench");
  });

  test("fw_cli.ts ast parses Rust and TypeScript via Tree-sitter C ABI", () => {
    for (const [lang, file] of [
      ["rust", "native/fw-abi/src/lib.rs"],
      ["typescript", "src/fw_cli.ts"],
    ]) {
      const proc = Bun.spawnSync(["bun", "src/fw_cli.ts", "ast", lang, file], {
        cwd: ROOT,
        stdout: "pipe",
        stderr: "pipe",
      });
      expect(proc.exitCode).toBe(0);
      const data = JSON.parse(proc.stdout.toString());
      expect(data.ok).toBe(true);
      expect(data.hasError).toBe(false);
      expect(data.nodeCount).toBeGreaterThan(10);
    }
  });

  test("fw_cli.ts inspect parses PE binaries via craft-pe C ABI on Windows", () => {
    if (!IS_WIN) return;
    const abiBin = findAbiLibrary(ROOT);
    expect(abiBin).not.toBeNull();
    const proc = Bun.spawnSync(["bun", "src/fw_cli.ts", "inspect", abiBin!], {
      cwd: ROOT,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(proc.exitCode).toBe(0);
    const data = JSON.parse(proc.stdout.toString());
    expect(data.ok).toBe(true);
    expect(data.pe32Plus).toBe(true);
    expect(data.machine).toBe("x86_64");
    expect(data.runtimeKind).toBe("native-lto-pe");
    expect(data.sha256.length).toBe(64);
    expect(data.sectionCount).toBeGreaterThan(0);
  });
});
