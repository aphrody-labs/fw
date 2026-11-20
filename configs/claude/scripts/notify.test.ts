import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SCRIPT = join(import.meta.dir, "notify.ts");

let home: string;
beforeAll(() => {
  home = mkdtempSync(join(tmpdir(), "claude-notify-test-"));
});
afterAll(() => {
  rmSync(home, { recursive: true, force: true });
});

const run = (...args: string[]) =>
  Bun.spawnSync(["bun", "--no-install", "--no-env-file", SCRIPT, ...args], {
    cwd: home,
    env: {
      ...process.env,
      HOME: home,
      USERPROFILE: home,
      RELAY_HOME: join(home, "relay"),
    },
    stdout: "pipe",
    stderr: "pipe",
    stdin: "ignore",
  });

describe("notify.ts", () => {
  test("no message exits 1 with usage on stderr", () => {
    const r = run();
    expect(r.exitCode).toBe(1);
    expect(r.stderr.toString()).toContain("Usage: bun scripts/notify.ts");
    expect(r.stdout.toString()).toBe("");
  });

  test("flags without message text still print usage", () => {
    const r = run("--to", "all", "--urgent", "--hook");
    expect(r.exitCode).toBe(1);
    expect(r.stderr.toString()).toContain("Usage:");
  });

  test("--list prints an empty JSON array without sessions", () => {
    const r = run("--list");
    expect(r.exitCode).toBe(0);
    expect(JSON.parse(r.stdout.toString())).toEqual([]);
  });

  test("--list maps a live session record", () => {
    const dir = join(home, ".claude", "sessions");
    mkdirSync(dir, { recursive: true });
    const pid = process.pid;
    writeFileSync(
      join(dir, `${pid}.json`),
      JSON.stringify({
        pid,
        sessionId: "sess-1",
        name: "tester",
        cwd: "/tmp/x",
        status: "idle",
        messagingSocketPath: "pipe",
      }),
    );
    const r = run("-l");
    expect(r.exitCode).toBe(0);
    const rows = JSON.parse(r.stdout.toString()) as Array<
      Record<string, unknown>
    >;
    expect(Array.isArray(rows)).toBe(true);
    expect(rows).toEqual([
      {
        pid,
        sessionId: "sess-1",
        name: "tester",
        cwd: "/tmp/x",
        status: "idle",
        pipeReady: true,
      },
    ]);
  });
});
