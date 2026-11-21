import { describe, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { check, parseConfig, scan } from "./retired.ts";

const config = parseConfig(`
[[term]]
id = "relay-hub"
pattern = '(?<![a-z])relay(?![a-z])'
reason = "removed"

[[term]]
id = "fr2-host"
pattern = '\\bfr2\\b'
reason = "out of service"

[[allow]]
paths = ["docs/history/**"]
reason = "history"

[[allow]]
paths = ["vendor/**"]
terms = ["relay-hub"]
reason = "vendored"
`);

const reader = (files: Record<string, string>) => (file: string) => files[file];

describe("retired surfaces", () => {
  test("matches whole words only", () => {
    const hits = scan(
      ["a.md"],
      reader({
        "a.md": "textureLayer RelayCommand requireLayout\nthe relay hub\nRELAY_HOME=1\nrelayed",
      }),
      config,
    );
    expect(hits.map((hit) => [hit.line, hit.term])).toEqual([
      [2, "relay-hub"],
      [3, "relay-hub"],
    ]);
  });

  test("allowed paths are skipped, per term when limited", () => {
    const files = {
      "docs/history/note.md": "relay and fr2",
      "vendor/lib.md": "relay and fr2",
    };
    const hits = scan(Object.keys(files), reader(files), config);
    expect(hits.map((hit) => `${hit.file}:${hit.term}`)).toEqual(["vendor/lib.md:fr2-host"]);
  });

  test("unreadable files are ignored", () => {
    expect(scan(["bin.dat"], () => undefined, config)).toEqual([]);
  });

  test("a git checkout fails on a new reference and passes without it", () => {
    const root = mkdtempSync(join(tmpdir(), "retired-"));
    try {
      const git = (...args: string[]) => Bun.spawnSync(["git", "-C", root, ...args]);
      git("init", "-q");
      mkdirSync(join(root, "src"));
      writeFileSync(join(root, "src", "ok.ts"), "export const fleet = ['fr1', 'prd'];\n");
      const configPath = join(root, "retired.toml");
      writeFileSync(
        configPath,
        '[[term]]\nid = "fr2-host"\npattern = \'\\bfr2\\b\'\nreason = "x"\n',
      );
      git("add", "src/ok.ts");
      expect(check(root, configPath)).toEqual([]);
      writeFileSync(join(root, "src", "bad.ts"), "const host = 'fr2';\n");
      git("add", "src/bad.ts");
      expect(check(root, configPath).map((hit) => `${hit.file}:${hit.line}`)).toEqual(["src/bad.ts:1"]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
