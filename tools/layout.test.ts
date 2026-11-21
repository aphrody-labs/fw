import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const tool = join(import.meta.dir, "layout.ts");

function repo(files: Record<string, string>) {
  const root = mkdtempSync(join(tmpdir(), "layout-test-"));
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(root, path, ".."), { recursive: true });
    writeFileSync(join(root, path), content);
  }
  Bun.spawnSync(["git", "init", "-q"], { cwd: root });
  Bun.spawnSync(["git", "add", "-A"], { cwd: root });
  return root;
}
const run = (root: string, command: string) => {
  const result = Bun.spawnSync(["bun", tool, command, "--root", root]);
  return { code: result.exitCode, out: result.stdout.toString(), err: result.stderr.toString() };
};

const manifest = (name: string, dependencies = "") => `[package]\nname = "${name}"\nversion = "0.1.0"\nedition = "2024"\n\n[dependencies]\n${dependencies}\n`;
const layout = `repo = "demo"\n\n[repos.demo]\nlayer = 1\npath = "."\nurl = "https://example.com/demo"\n\n[repos.low]\nlayer = 0\npath = "../low"\nurl = "https://example.com/low"\n\n[repos.high]\nlayer = 2\npath = "../high"\nurl = "https://example.com/high"\n\n[cargo]\nmembers = ["crates/*"]\n`;
const workspace = `[workspace]\nresolver = "3"\nmembers = ["crates/a"]\n\n[workspace.dependencies]\nserde = "1"\n\n[profile.release]\nlto = true\n`;

test("sync writes every local crate into workspace.dependencies and is idempotent", () => {
  const root = repo({
    "Cargo.toml": workspace,
    "layout.toml": layout,
    "crates/a/Cargo.toml": manifest("a"),
    "crates/b/Cargo.toml": manifest("b", "a = { workspace = true }"),
  });
  expect(run(root, "sync").code).toBe(0);
  const text = readFileSync(join(root, "Cargo.toml"), "utf8");
  expect(text).toContain('a = { path = "crates/a" }');
  expect(text).not.toContain('b = { path');
  expect(text).toContain('"crates/*"');
  expect(text).toContain("[profile.release]");
  expect(run(root, "check").code).toBe(0);
  expect(run(root, "sync").out).toContain('"wrote":[]');
});

test("check rejects a member that names a mapped crate by path", () => {
  const root = repo({
    "Cargo.toml": workspace,
    "layout.toml": layout,
    "crates/a/Cargo.toml": manifest("a"),
    "crates/b/Cargo.toml": manifest("b", 'a = { path = "../a" }'),
  });
  run(root, "sync");
  const result = run(root, "check");
  expect(result.code).toBe(1);
  expect(result.err).toContain("a.workspace = true");
});

test("a home in a repository above is a direction error", () => {
  const root = repo({
    "Cargo.toml": workspace,
    "layout.toml": `${layout}\n[cargo.crate.up]\nrepo = "high"\npath = "crates/up"\n`,
    "crates/a/Cargo.toml": manifest("a"),
  });
  run(root, "sync");
  expect(run(root, "check").err).toContain("direction is fw <- aphrody <- ay");
});

test("an unpinned git dependency fails when pinned_git is on", () => {
  const root = repo({
    "Cargo.toml": workspace,
    "layout.toml": `${layout.replace("[cargo]", '[policy]\npinned_git = true\n\n[cargo]')}\n[cargo.crate.fork]\ngit = "https://example.com/fork"\nbranch = "main"\n`,
    "crates/a/Cargo.toml": manifest("a"),
  });
  run(root, "sync");
  expect(run(root, "check").err).toContain("not a pin");
});

test("an area map rejects uncovered tracked paths", () => {
  const root = repo({
    "Cargo.toml": workspace,
    "layout.toml": `${layout}\n[[area]]\npath = "crates"\ndecision = "keep"\nwhy = "members"\n`,
    "crates/a/Cargo.toml": manifest("a"),
    "stray/file.txt": "x",
  });
  run(root, "sync");
  expect(run(root, "check").err).toContain("not covered by an [[area]]");
});

test("a new non-Rust source file fails until it is in the baseline or deliberately allowed", () => {
  const root = repo({
    "Cargo.toml": workspace,
    "layout.toml": `${layout}\n[rustonly]\nbaseline = "baseline.txt"\nallow = ["vendored/**"]\n`,
    "crates/a/Cargo.toml": manifest("a"),
    "scripts/old.ts": "export {};\n",
    "vendored/tool.py": "x = 1\n",
  });
  run(root, "sync");
  expect(run(root, "check").err).toContain("scripts/old.ts");
  expect(run(root, "rustonly").code).toBe(0);
  expect(readFileSync(join(root, "baseline.txt"), "utf8")).toBe("scripts/old.ts\n");
  writeFileSync(join(root, "scripts/new.ts"), "export {};\n");
  Bun.spawnSync(["git", "add", "-A"], { cwd: root });
  expect(run(root, "check").err).toContain("scripts/new.ts");
});
