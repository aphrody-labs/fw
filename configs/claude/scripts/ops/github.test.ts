import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SCRIPT = join(import.meta.dir, "github.ts");

describe("github.ts", () => {
  test("--help exits 0 with usage and does not touch gh", () => {
    const cwd = mkdtempSync(join(tmpdir(), "claude-gh-test-"));
    try {
      const r = Bun.spawnSync(
        [process.execPath, "--no-install", "--no-env-file", SCRIPT, "--help"],
        {
          cwd,
          env: { ...process.env, PATH: "" },
          stdout: "pipe",
          stderr: "pipe",
          stdin: "ignore",
        },
      );
      expect(r.exitCode).toBe(0);
      const out = r.stdout.toString();
      expect(out).toContain("usage: bun scripts/ops/github.ts [audit|fix]");
      expect(out).toContain("read-only");
    } finally {
      rmSync(cwd, { recursive: true, force: true });
    }
  });
});
