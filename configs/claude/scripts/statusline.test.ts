import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SCRIPT = join(import.meta.dir, "statusline.ts");

let tmp: string;
let project: string;
beforeAll(() => {
  tmp = mkdtempSync(join(tmpdir(), "claude-status-test-"));
  project = join(tmp, "demo-project");
  mkdirSync(join(project, ".git"), { recursive: true });
  writeFileSync(join(project, ".git", "HEAD"), "ref: refs/heads/feature/x\n");
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

describe("statusline.ts", () => {
  test("renders model, folder, branch, context and cost", () => {
    const r = run(
      JSON.stringify({
        model: { display_name: "Opus 9" },
        workspace: { project_dir: project },
        context_window: { used_percentage: 42.4 },
        cost: { total_cost_usd: 1.234 },
      }),
    );
    expect(r.exitCode).toBe(0);
    const line = r.stdout.toString();
    expect(line).toContain("Opus 9");
    expect(line).toContain("demo-project (feature/x)");
    expect(line).toContain("ctx 42%");
    expect(line).toContain("$1.23");
    expect(line).not.toContain("\n");
  });

  test("renders agy-style fields", () => {
    const r = run(
      JSON.stringify({
        model: { id: "m-1" },
        cwd: project,
        cycle_mode: "plan",
        sandbox: { enabled: true },
        agent: { name: "bot" },
        agent_state: "running",
        task_count: 2,
        pending_input_count: 1,
        cost: { total_usd: 0.5 },
      }),
    );
    expect(r.stdout.toString()).toBe(
      "m-1 | demo-project (feature/x) | [plan+sbx] | @bot | running | 2t | 1q | $0.50",
    );
  });

  test("omits ctx and cost when absent or null", () => {
    const r = run(
      JSON.stringify({
        model: { display_name: "M" },
        workspace: { current_dir: project },
        context_window: { used_percentage: null },
      }),
    );
    expect(r.stdout.toString()).toBe("M | demo-project (feature/x)");
  });

  test("invalid stdin still prints a minimal line", () => {
    const r = run("not json", project);
    expect(r.exitCode).toBe(0);
    expect(r.stdout.toString()).toBe("agy | demo-project (feature/x)");
  });

  test("empty stdin prints the cwd folder", () => {
    const r = run("", tmp);
    expect(r.exitCode).toBe(0);
    expect(r.stdout.toString().startsWith("agy | ")).toBe(true);
  });
});
