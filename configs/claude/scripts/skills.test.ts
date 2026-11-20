import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  audit,
  jaccard,
  loadSkills,
  parseFrontmatter,
  tokens,
} from "./skills.ts";

test("parseFrontmatter reads valid YAML and returns {} for invalid YAML", () => {
  expect(parseFrontmatter("---\nname: a\ndescription: b\n---\nbody")).toEqual({
    name: "a",
    description: "b",
  });
  expect(parseFrontmatter("---\nname: a\ndescription: x: y\n---\n")).toEqual(
    {},
  );
  expect(parseFrontmatter("no frontmatter")).toEqual({});
});

test("tokens and jaccard measure description overlap", () => {
  expect(
    jaccard(tokens("Build Rust crates fast"), tokens("build rust crates")),
  ).toBeGreaterThan(0.7);
  expect(jaccard(tokens("alpha beta"), tokens("gamma delta"))).toBe(0);
});

function skillDir(root: string, name: string, front: string): void {
  mkdirSync(join(root, name), { recursive: true });
  writeFileSync(join(root, name, "SKILL.md"), `---\n${front}\n---\nbody`);
}

test("audit flags missing fields, name mismatch, duplicate names and near-duplicates", () => {
  const a = mkdtempSync(join(tmpdir(), "skills-a-"));
  const b = mkdtempSync(join(tmpdir(), "skills-b-"));
  try {
    skillDir(
      a,
      "good",
      "name: good\ndescription: Deploy workers to the edge network",
    );
    skillDir(a, "nodesc", "name: nodesc");
    skillDir(
      a,
      "mismatch",
      "name: other\ndescription: something unrelated entirely",
    );
    skillDir(
      b,
      "good",
      "name: good\ndescription: Deploy workers to the edge network quickly",
    );
    skillDir(
      b,
      "twin",
      "name: twin\ndescription: Deploy workers to the edge network",
    );
    const skills = [...loadSkills(a, "a"), ...loadSkills(b, "b")];
    const kinds = audit(skills).map((i) => i.kind);
    expect(kinds).toContain("missing-description");
    expect(kinds).toContain("name-mismatch");
    expect(kinds).toContain("duplicate-name");
    expect(kinds).toContain("near-duplicate");
  } finally {
    rmSync(a, { recursive: true, force: true });
    rmSync(b, { recursive: true, force: true });
  }
});

test("the repo's own skills have no errors", async () => {
  const p = Bun.spawn(
    ["bun", "--no-install", "--no-env-file", `${import.meta.dir}/skills.ts`],
    { stdout: "pipe", stderr: "pipe" },
  );
  const out = await new Response(p.stdout).text();
  expect({
    code: await p.exited,
    out: out.split("\n").filter((l) => /^(missing|name|duplicate)/.test(l)),
  }).toEqual({ code: 0, out: [] });
});
