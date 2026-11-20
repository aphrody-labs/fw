import { afterEach, beforeEach, expect, test } from "bun:test";
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  briefOutput,
  briefs,
  refresh,
  repoStates,
  save,
  stateDir,
} from "./restart-pack.ts";

let root = "";
const git = (cwd: string, ...args: string[]) =>
  Bun.spawnSync(
    ["git", "-c", "user.name=t", "-c", "user.email=t@example.test", ...args],
    { cwd, stdout: "ignore", stderr: "ignore" },
  );

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "restart-"));
  Bun.env.WORKSPACE_ROOT = root;
  Bun.env.CLAUDE_RESTART_DIR = join(root, "state");
  Bun.env.AY_PRIVATE_DIR = join(root, "private");
  for (const name of ["alpha", "beta"]) {
    const dir = join(root, name);
    mkdirSync(dir);
    git(dir, "init", "-q", "-b", "main");
    writeFileSync(join(dir, "f.txt"), name);
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", `first commit of ${name}`);
  }
  writeFileSync(join(root, "beta", "dirty.txt"), "x");
});
afterEach(() => {
  delete Bun.env.WORKSPACE_ROOT;
  delete Bun.env.CLAUDE_RESTART_DIR;
  delete Bun.env.AY_PRIVATE_DIR;
  rmSync(root, { recursive: true, force: true });
});

test("repository states: branch, dirty count, last commit; non-repositories are ignored", () => {
  mkdirSync(join(root, "plain"));
  const states = repoStates(root);
  expect(states.map((s) => s.name)).toEqual(["alpha", "beta"]);
  expect(states[1]).toMatchObject({
    branch: "main",
    dirty: 1,
    ahead: 0,
    behind: 0,
  });
  expect(states[0]!.last).toContain("first commit of alpha");
});

test("the pack lists the open ledger items, the saved briefs and the repositories", async () => {
  mkdirSync(join(root, "private", "plan"), { recursive: true });
  writeFileSync(
    join(root, "private", "plan", "work.toml"),
    '[[item]]\nid = "a"\ntitle = "Open thing"\nstatus = "running"\nowner = "agent x"\nnext = "do it"\n[[item]]\nid = "b"\ntitle = "Finished thing"\nstatus = "done"\n',
  );
  writeFileSync(
    join(root, "brief-src.md"),
    "Goal: finish the thing\nmore text",
  );
  await save("agent-x", join(root, "brief-src.md"));
  expect(briefs()).toEqual([
    { name: "agent-x", first: "Goal: finish the thing" },
  ]);
  const text = refresh();
  expect(text).toContain("| Open thing | running | agent x | do it |");
  expect(text).not.toContain("Finished thing");
  expect(text).toContain("`agent-x`: Goal: finish the thing");
  expect(text).toContain("| beta | main | 0 | 0 | 1 |");
});

test("the SessionStart output carries the pack when fresh and nothing when absent or stale", () => {
  expect(briefOutput()).toBe("");
  refresh();
  const out = JSON.parse(briefOutput());
  expect(out.hookSpecificOutput.hookEventName).toBe("SessionStart");
  expect(out.hookSpecificOutput.additionalContext).toContain("RESTART BRIEF");
  const old = new Date(Date.now() - 80 * 3600_000);
  utimesSync(join(stateDir(), "restart.md"), old, old);
  expect(briefOutput()).toBe("");
});

test("brief names are restricted", async () => {
  await expect(save("../escape", join(root, "alpha", "f.txt"))).rejects.toThrow(
    "brief name",
  );
});

test("the injected brief drops the instructions block and stays bounded", () => {
  refresh();
  const ctx = JSON.parse(briefOutput()).hookSpecificOutput
    .additionalContext as string;
  expect(ctx).not.toContain("What to do first");
  expect(ctx.length).toBeLessThan(4400);
});
