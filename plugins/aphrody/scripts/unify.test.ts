import { expect, test } from "bun:test";
import { compactDescription, parseSkill, replaceDescription, rewriteLoose, rewriteMarkdown, toolchainCommand } from "./unify-text.ts";

test("legacy commands become the Aphrody toolchain", () => {
  const cases: Array<[string, string]> = [
    ["npm install express", "bun add express"],
    ["npm i -g agent-browser", "bun add -g agent-browser"],
    ["npm install", "bun install"],
    ["npm run build", "bun run build"],
    ["npx -y create-foo@latest x", "bunx create-foo@latest x"],
    ["node build.js --x", "bun build.js --x"],
    ["pip install -r requirements.txt", "uv pip install -r requirements.txt"],
    ["python3 -m venv .venv", "uv venv .venv"],
    ["python scripts/a.py --b", "uv run scripts/a.py --b"],
    ["cd x && pip install foo && python -m pytest", "cd x && uv pip install foo && uv run python -m pytest"],
    ["npx jest --coverage", "bun test --coverage"],
    ["npx eslint . --fix", "bun lint . --fix"],
    ["npx prettier --write .", "bun fmt --write ."],
  ];
  for (const [input, expected] of cases) expect(toolchainCommand(input)).toBe(expected);
  expect(toolchainCommand("bun add x")).toBeUndefined();
});

test("markdown: shell blocks and inline code change, prose, other languages and mapping lines do not", () => {
  const input = "Install `npm i x`.\n```bash\nnpx foo\n```\n```json\n{\"npm install\":1}\n```\nUse `python` instead of `python3` (`python3 a.py`).\n";
  const { text, changes } = rewriteMarkdown(input);
  expect(text).toBe("Install `bun add x`.\n```bash\nbunx foo\n```\n```json\n{\"npm install\":1}\n```\nUse `python` instead of `python3` (`python3 a.py`).\n");
  expect(changes).toBe(2);
  expect(rewriteLoose(" *   npx tsx snapshot.ts --url x\n#!/usr/bin/env npx tsx").text).toBe(" *   bun snapshot.ts --url x\n#!/usr/bin/env bun");
});

test("frontmatter tolerates unquoted colons and list values", () => {
  const skill = parseSkill("---\nname: fw\ndescription: Orientation: the Rust workspace.\nallowed-tools:\n  - Bash(x)\n  - Read\n---\nbody");
  expect(skill.frontmatter).toEqual({ name: "fw", description: "Orientation: the Rust workspace.", "allowed-tools": "Bash(x), Read" });
  expect(replaceDescription("---\nname: a\ndescription: >\n  foo\n  bar\nlicense: MIT\n---\nb", "new: one")).toBe('---\nname: a\ndescription: "new: one"\nlicense: MIT\n---\nb');
});

test("compaction keeps the trigger sentence and returns what it cut", () => {
  const { line, rest } = compactDescription("Use this skill when you need to do a thing. ".repeat(1).trim(), 200);
  expect(line).toBe("Use when you need to do a thing.");
  expect(rest).toBe("");
  const long = `${"Summary words, more words. ".repeat(12)}Use when modifying crates, extensions or syncs.`;
  const cut = compactDescription(long, 120);
  expect(cut.line.length).toBeLessThanOrEqual(130);
  expect(cut.line).toContain("Use when modifying crates");
  expect(cut.rest.length).toBeGreaterThan(0);
});
