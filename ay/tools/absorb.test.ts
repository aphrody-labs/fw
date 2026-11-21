import { beforeAll, describe, expect, test } from "bun:test";
import { hostWork } from "./hosts.ts";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  appendLedger, buildScript, ciState, classify, ledgerRow, mergeMessage, parseTarget, rank, score, shq, tail, tomlString,
  type Candidate, type Row,
} from "./absorb.ts";

const base: Omit<Candidate, "score"> = {
  number: 1, title: "x", url: "u", author: "a", updatedAt: "2026-05-01T00:00:00Z", draft: false, mergeable: "MERGEABLE",
  ci: "success", review: "none", approvals: 0, reactions: 0, labels: [], additions: 10, deletions: 5, files: 1, rust: true, kind: "other",
};

describe("absorb targets", () => {
  test("PR numbers, hashes and URLs", () => {
    expect(parseTarget("42")).toEqual({ kind: "pr", number: 42 });
    expect(parseTarget("#7")).toEqual({ kind: "pr", number: 7 });
    expect(parseTarget("https://github.com/a/b/pull/9")).toEqual({ kind: "pr", number: 9 });
  });
  test("fork branches, with and without the repository", () => {
    expect(parseTarget("someone:fix/x")).toEqual({ kind: "branch", owner: "someone", repo: null, branch: "fix/x" });
    expect(parseTarget("someone/cef-rs-fork:main")).toEqual({ kind: "branch", owner: "someone", repo: "cef-rs-fork", branch: "main" });
    expect(() => parseTarget("nonsense")).toThrow();
  });
});

describe("absorb ranking", () => {
  test("classification", () => {
    expect(classify("Fix crash on shutdown", [])).toBe("fix");
    expect(classify("Add Wayland support", [])).toBe("feature");
    expect(classify("Bump deps", ["bug"])).toBe("fix");
    expect(classify("Bump deps", [])).toBe("other");
    expect(classify("docs: fix typo in README", [])).toBe("other");
  });
  test("CI rollup states", () => {
    expect(ciState("SUCCESS")).toBe("success");
    expect(ciState("ERROR")).toBe("failure");
    expect(ciState("EXPECTED")).toBe("pending");
    expect(ciState(null)).toBe("none");
  });
  test("green approved small bug fixes beat big failing features", () => {
    const fix = { ...base, number: 1, kind: "fix" as const, review: "APPROVED" as const, approvals: 1, reactions: 3 };
    const feat = { ...base, number: 2, kind: "feature" as const, ci: "failure" as const, additions: 4000 };
    const conflicting = { ...base, number: 3, kind: "fix" as const, mergeable: "CONFLICTING" as const };
    expect(rank([feat, conflicting, fix]).map(c => c.number)).toEqual([1, 3, 2]);
  });
  test("a smaller diff wins a tie and drafts sink", () => {
    expect(score({ ...base, additions: 5, deletions: 0 })).toBeGreaterThan(score({ ...base, additions: 900, deletions: 100 }));
    expect(score({ ...base, draft: true })).toBeLessThan(score(base) - 5);
  });
});

describe("absorb ledger", () => {
  const row: Row = {
    repo: "a/b", fork: "aphrody-labs/b", source: "https://github.com/a/b/pull/1", title: 'say "hi"\\', result: "merged",
    test: "cargo test", date: "2026-11-16",
  };
  test("TOML escaping", () => {
    expect(tomlString('a"b\\c\nd')).toBe('"a\\"b\\\\c\\nd"');
  });
  test("rows parse back as TOML", () => {
    const parsed = Bun.TOML.parse(`version = 1\n${ledgerRow({ ...row, detail: "line1\nline2" })}${ledgerRow(row)}`) as { merge: Record<string, unknown>[] };
    expect(parsed.merge).toHaveLength(2);
    expect(parsed.merge[0]!.title).toBe('say "hi"\\');
    expect(parsed.merge[0]!.detail).toBe("line1\nline2");
    expect(String(parsed.merge[0]!.date)).toContain("2026-11-16");
  });
  test("the file starts with version = 1 and is only appended to", () => {
    const f = join(mkdtempSync(join(tmpdir(), "absorb-")), "absorbed.toml");
    appendLedger(row, f);
    appendLedger({ ...row, result: "conflict" }, f);
    const parsed = Bun.TOML.parse(readFileSync(f, "utf8")) as { version: number; merge: { result: string }[] };
    expect(parsed.version).toBe(1);
    expect(parsed.merge.map(m => m.result)).toEqual(["merged", "conflict"]);
  });
});

describe.skipIf(!existsSync(join(import.meta.dir, "..", "ay.local.toml")))("absorb script", () => {
  const plan = () => ({
    dir: `${hostWork("build")}/absorb/cef-rs`, forkUrl: "https://github.com/aphrody-labs/cef-rs.git", base: "dev",
    srcUrl: "https://github.com/tauri-apps/cef-rs.git", srcRef: "pull/5/head", test: "cargo test --locked",
    message: mergeMessage("tauri-apps/cef-rs", { number: 5, title: "Fix it", url: "https://github.com/tauri-apps/cef-rs/pull/5" }), timeout: 60,
  });
  let s = "";
  beforeAll(() => {
    s = buildScript(plan());
  });
  test("merges on the default branch through FETCH_HEAD and never forces, branches, stashes or resets", () => {
    expect(s).toContain("git checkout 'dev'");
    expect(s).toContain("fetch 'https://github.com/tauri-apps/cef-rs.git' 'pull/5/head'");
    expect(s).toContain("push origin HEAD:'dev'");
    expect(s).not.toMatch(/--force|push -f|stash|reset|checkout -b|branch |worktree/);
    expect(s).toContain("CARGO_INCREMENTAL=0");
    expect(s).toContain(`export ABSORB='${hostWork("build")}/absorb'`);
  });
  test("the PR URL goes into the merge message", () => {
    expect(s).toContain("Merge tauri-apps/cef-rs#5: Fix it");
    expect(s).toContain("https://github.com/tauri-apps/cef-rs/pull/5");
  });
  test("the script is valid bash", () => {
    const r = Bun.spawnSync(["bash", "-n"], { stdin: new TextEncoder().encode(s) });
    expect(r.exitCode).toBe(0);
  });
  test("quoting and tails", () => {
    expect(shq("it's")).toBe(`'it'\\''s'`);
    expect(tail("a\n\nb\nc\nd", 2)).toBe("c\nd");
  });
});
