import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { KEEP, inspect, remoteScript, sshArguments } from "./sync-host.ts";

const sh = (dir: string, ...args: string[]) => Bun.spawnSync(["git", "-C", dir, ...args], { stdout: "pipe", stderr: "pipe" });

test("the host script fetches the commit, keeps build outputs, applies the diff only when there is one, and never reads a remote key", () => {
  const withPatch = remoteScript("/work/sync/r", "a".repeat(40), "https://github.com/o/r", true);
  expect(withPatch).toContain(`git fetch -q --depth 1 origin ${"a".repeat(40)}`);
  expect(withPatch).toContain("git checkout -q --force FETCH_HEAD");
  for (const name of KEEP) expect(withPatch).toContain(`-e ${name}`);
  expect(withPatch).toContain("git apply --binary .sync.patch");
  expect(remoteScript("/w", "b".repeat(40), "u", false)).not.toContain("git apply");
  // Nothing to send: the host must not wait on a tar stream that never comes (a sync hung for six minutes on `tar xf -`).
  expect(remoteScript("/w", "b".repeat(40), "u", false, false)).not.toContain("tar xf -");
  expect(withPatch).toContain("tar xf -");
});

test("the ssh command carries the script as base64 and leaves stdin to the tar stream", () => {
  const [, , host, command] = sshArguments("build-host", "echo 'quoted' \"double\" $HOME");
  expect(host).toBe("build-host");
  expect(command).toContain("base64 -d");
  expect(command).not.toContain("quoted");
  expect(command).not.toContain("<");
});

test("a checkout reports its commit, whether it is pushed, its uncommitted diff and its untracked files", () => {
  const dir = mkdtempSync(join(tmpdir(), "sync-host-"));
  try {
    sh(dir, "init", "-q", "-b", "main");
    sh(dir, "config", "user.email", "t@example.com");
    sh(dir, "config", "user.name", "t");
    writeFileSync(join(dir, "a.txt"), "one\n");
    sh(dir, "add", "a.txt");
    sh(dir, "commit", "-q", "-m", "first");
    sh(dir, "remote", "add", "origin", "https://example.com/r.git");
    writeFileSync(join(dir, "a.txt"), "two\n");
    writeFileSync(join(dir, "new.txt"), "n\n");
    const checkout = inspect(dir);
    expect(checkout.sha).toMatch(/^[0-9a-f]{40}$/);
    expect(checkout.remote).toBe("https://example.com/r.git");
    expect(checkout.pushed).toBe(false);
    expect(new TextDecoder().decode(checkout.patch)).toContain("+two");
    expect(checkout.untracked).toEqual(["new.txt"]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
