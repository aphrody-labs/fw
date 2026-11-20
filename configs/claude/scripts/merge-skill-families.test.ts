import { describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  brokenRefs,
  type Family,
  MARKER,
  mergeFamily,
  rewritePaths,
  splitSkill,
} from "./merge-skill-families.ts";

const skill = (name: string, desc: string, body: string) =>
  `---\nname: ${name}\ndescription: >-\n  ${desc}\n---\n${body}`;

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), "merge-skills-"));
  const w = (p: string, t: string) => {
    mkdirSync(join(p, ".."), { recursive: true });
    writeFileSync(p, t);
  };
  w(
    join(root, "fam", "SKILL.md"),
    skill("fam", "Parent skill.", "# Parent\nbody\n"),
  );
  w(
    join(root, "fam-a", "SKILL.md"),
    skill(
      "fam-a",
      "Does A. Use when A.",
      "Run `scripts/a.py` and read references/x.md and ./notes.md; see [t](./templates/t.txt).\n",
    ),
  );
  w(join(root, "fam-a", "scripts", "a.py"), "print(1)\n");
  w(join(root, "fam-a", "references", "x.md"), "[back](../SKILL.md#top)\n");
  w(join(root, "fam-a", "notes.md"), "notes\n");
  w(join(root, "fam-a", "templates", "t.txt"), "t\n");
  w(join(root, "fam-b", "SKILL.md"), skill("fam-b", "Does B.", "B body\n"));
  return root;
}

const family: Family = {
  parent: "fam",
  title: "Fam",
  description: "Fam router.",
  intro: "Intro.",
  children: ["fam-a", "fam-b"],
};

describe("merge-skill-families", () => {
  test("rewritePaths prefixes only entries of the old skill directory", () => {
    const out = rewritePaths(
      "scripts/a.py references/x.md ./notes.md other/scripts/a.py",
      "fam-a",
      [
        { name: "scripts", dir: true },
        { name: "references", dir: true },
        { name: "notes.md", dir: false },
      ],
    );
    expect(out).toBe(
      "references/fam-a/scripts/a.py references/fam-a/references/x.md references/fam-a/notes.md other/scripts/a.py",
    );
  });

  test("merges, keeps every file and body, and is idempotent", () => {
    const root = fixture();
    try {
      const first = mergeFamily(root, family);
      expect(first.merged).toEqual(["fam-a", "fam-b"]);
      expect(existsSync(join(root, "fam-a"))).toBe(false);
      const ref = readFileSync(
        join(root, "fam", "references", "fam-a.md"),
        "utf8",
      );
      expect(ref).toContain("Original frontmatter:");
      expect(ref).toContain("references/fam-a/scripts/a.py");
      expect(
        readFileSync(
          join(root, "fam", "references", "fam-a", "scripts", "a.py"),
          "utf8",
        ),
      ).toBe("print(1)\n");
      expect(
        readFileSync(
          join(root, "fam", "references", "fam-a", "references", "x.md"),
          "utf8",
        ),
      ).toBe("[back](../../fam-a.md#top)\n");
      expect(
        readFileSync(join(root, "fam", "references", "core.md"), "utf8"),
      ).toContain("# Parent");
      const parent = readFileSync(join(root, "fam", "SKILL.md"), "utf8");
      expect(splitSkill(parent).fm).toContain("Fam router.");
      expect(parent).toContain(MARKER);
      expect(parent).toContain("references/fam-b.md");
      expect(brokenRefs(root, "fam")).toEqual([]);
      const second = mergeFamily(root, family);
      expect(second.merged).toEqual([]);
      expect(second.skipped).toEqual(["fam-a", "fam-b"]);
      expect(readFileSync(join(root, "fam", "SKILL.md"), "utf8")).toBe(parent);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("a child whose name collides with a parent reference gets a -skill suffix", () => {
    const root = fixture();
    try {
      mkdirSync(join(root, "fam", "references"), { recursive: true });
      writeFileSync(join(root, "fam", "references", "fam-b.md"), "own file\n");
      mergeFamily(root, family);
      expect(
        readFileSync(join(root, "fam", "references", "fam-b.md"), "utf8"),
      ).toBe("own file\n");
      expect(
        existsSync(join(root, "fam", "references", "fam-b-skill.md")),
      ).toBe(true);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
