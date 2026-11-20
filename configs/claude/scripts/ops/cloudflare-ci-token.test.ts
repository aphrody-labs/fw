import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SCRIPT = join(import.meta.dir, "cloudflare-ci-token.ts");

const run = (args: string[], token?: string) => {
  const cwd = mkdtempSync(join(tmpdir(), "claude-cf-test-"));
  try {
    const env: Record<string, string | undefined> = { ...process.env };
    if (token === undefined) delete env.CLOUDFLARE_API_TOKEN;
    else env.CLOUDFLARE_API_TOKEN = token;
    return Bun.spawnSync(
      ["bun", "--no-install", "--no-env-file", SCRIPT, ...args],
      { cwd, env, stdout: "pipe", stderr: "pipe", stdin: "ignore" },
    );
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
};

describe("cloudflare-ci-token.ts", () => {
  test("no args and no token exits 2 with usage", () => {
    const r = run([]);
    expect(r.exitCode).toBe(2);
    expect(r.stderr.toString()).toContain("usage: CLOUDFLARE_API_TOKEN=");
    expect(r.stdout.toString()).toBe("");
  });

  test("token without repositories exits 2", () => {
    const r = run([], "dummy");
    expect(r.exitCode).toBe(2);
    expect(r.stderr.toString()).toContain("<owner/repo>");
  });

  test("repositories without token exits 2", () => {
    const r = run(["owner/repo"]);
    expect(r.exitCode).toBe(2);
    expect(r.stderr.toString()).toContain("usage:");
  });
});
