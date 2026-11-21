import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { checkoutsOnDisk, compare, registry, slugOf } from "./workspace.ts";

function root(...gitDirs: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), "ws-"));
  for (const d of gitDirs) mkdirSync(join(dir, d, ".git"), { recursive: true });
  mkdirSync(join(dir, "plain"));
  return dir;
}

describe("workspace", () => {
  test("registry keeps declaration order and resolves sibling checkouts", () => {
    const base = root();
    const cfg = { product: { repo: "ay" }, repos: { fw: { slug: "o/fw", kind: "toolchain" }, ay: { slug: "o/ay", kind: "sdk" }, x: { slug: "o/x", checkout: join(base, "elsewhere") } } };
    const repos = registry(cfg);
    expect(repos.map((r) => r.key)).toEqual(["fw", "ay", "x"]);
    expect(repos[2]!.dir).toBe(join(base, "elsewhere"));
  });

  test("compare separates missing, unregistered and retired checkouts", () => {
    const base = root("fw", "old", "stray");
    const repos = [{ key: "fw", slug: "o/fw", kind: "", dir: join(base, "fw") }, { key: "ay", slug: "o/ay", kind: "", dir: join(base, "ay") }];
    const onDisk = checkoutsOnDisk(base);
    expect(onDisk.length).toBe(3);
    expect(compare(repos, onDisk, { old: { slug: "o/old" } })).toEqual({ missing: ["ay"], unregistered: ["stray"], retiredPresent: ["old"] });
  });

  test("slugOf reads https and ssh GitHub remotes only", () => {
    expect(slugOf("https://github.com/Aphrody-Labs/fw.git")).toBe("aphrody-labs/fw");
    expect(slugOf("git@github.com:aphrody-labs/fw-nt.git\n")).toBe("aphrody-labs/fw-nt");
    expect(slugOf("https://gitlab.com/a/b.git")).toBeUndefined();
  });
});
