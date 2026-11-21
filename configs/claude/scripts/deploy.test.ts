import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deploy, parseRules } from "./deploy.ts";

describe("deploy", () => {
  test("copies changed files once, skips excluded ones and keeps destination-only files", () => {
    const root = mkdtempSync(join(tmpdir(), "deploy-src-"));
    const home = mkdtempSync(join(tmpdir(), "deploy-home-"));
    try {
      mkdirSync(join(root, "scripts", "lib"), { recursive: true });
      writeFileSync(join(root, "CLAUDE.md"), "rules\n");
      writeFileSync(join(root, "scripts", "a.ts"), "a\n");
      writeFileSync(join(root, "scripts", "a.test.ts"), "t\n");
      writeFileSync(join(root, "scripts", "lib", "b.ts"), "b\n");
      mkdirSync(join(home, "scripts"), { recursive: true });
      writeFileSync(join(home, "scripts", "keep.ts"), "keep\n");
      const rules = parseRules(
        '[[copy]]\nsrc = "CLAUDE.md"\ndest = "CLAUDE.md"\n[[copy]]\nsrc = "scripts"\ndest = "scripts"\nexclude = ["**/*.test.ts"]\n',
      );
      expect(deploy(root, home, rules, false)).toEqual(["CLAUDE.md", "scripts/a.ts", "scripts/lib/b.ts"]);
      expect(deploy(root, home, rules, true)).toHaveLength(3);
      expect(deploy(root, home, rules, true)).toEqual([]);
      expect(readFileSync(join(home, "scripts", "lib", "b.ts"), "utf8")).toBe("b\n");
      expect(readFileSync(join(home, "scripts", "keep.ts"), "utf8")).toBe("keep\n");
    } finally {
      rmSync(root, { recursive: true, force: true });
      rmSync(home, { recursive: true, force: true });
    }
  });
});
