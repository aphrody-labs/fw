import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SCRIPT = join(import.meta.dir, "title.ts");

let tmp: string;
let project: string;
beforeAll(() => {
  tmp = mkdtempSync(join(tmpdir(), "claude-title-test-"));
  project = join(tmp, "demo-project");
  mkdirSync(join(project, ".git"), { recursive: true });
  writeFileSync(
    join(project, ".git", "HEAD"),
    "0123456789abcdef0123456789abcdef01234567\n",
  );
});
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});

const run = (stdin: string, cwd = tmp) =>
  Bun.spawnSync(["bun", "--no-install", "--no-env-file", SCRIPT], {
    cwd,
    stdin: new TextEncoder().encode(stdin),
    stdout: "pipe",
    stderr: "pipe",
  });

describe("title.ts", () => {
  test("renders state, folder, short detached sha and conversation title", () => {
    const r = run(
      JSON.stringify({
        workspace: { project_dir: project },
        agent_state: "running",
        conversation_title: "Fix things",
      }),
    );
    expect(r.exitCode).toBe(0);
    expect(r.stdout.toString()).toBe(
      "[running] agy demo-project (0123456) - Fix things",
    );
  });

  test("uses cwd and the branch name for ref heads", () => {
    writeFileSync(join(project, ".git", "HEAD"), "ref: refs/heads/main\n");
    const r = run(JSON.stringify({ cwd: project }));
    expect(r.stdout.toString()).toBe("agy demo-project (main)");
  });

  test("truncates to 100 characters", () => {
    const r = run(
      JSON.stringify({ cwd: project, conversation_title: "x".repeat(300) }),
    );
    expect(r.stdout.toString().length).toBe(100);
  });

  test("invalid stdin falls back to the process cwd", () => {
    const r = run("{nope", project);
    expect(r.exitCode).toBe(0);
    expect(r.stdout.toString()).toBe("agy demo-project (main)");
  });
});
