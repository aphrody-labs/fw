import { expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  END,
  inject,
  missingProofs,
  parseMatrix,
  proofPath,
  renderMatrix,
  START,
  summarize,
} from "./matrix.ts";
import { REPO_ROOT } from "./lib.ts";

const toml = `
[[capability]]
id = "a"
area = "x"
capability = "Thing"
openclaw = { status = "present", proof = "src/a/ (note)" }
hermes = { status = "missing", proof = "none" }
claude = { status = "partial", was = "missing", proof = "scripts/matrix.ts" }
`;

test("parseMatrix validates statuses, proofs and ids", () => {
  expect(parseMatrix(toml)).toHaveLength(1);
  expect(() => parseMatrix(toml.replace("partial", "weird"))).toThrow(
    "invalid status",
  );
  expect(() => parseMatrix(toml + toml)).toThrow("duplicate");
});

test("proofPath keeps the path before notes and maps none to null", () => {
  expect(proofPath("src/a/ (note)")).toBe("src/a/");
  expect(proofPath("a/b.ts, c")).toBe("a/b.ts");
  expect(proofPath("none")).toBeNull();
});

test("missingProofs only checks non-missing claude cells", () => {
  const rows = parseMatrix(toml);
  expect(missingProofs(rows, REPO_ROOT)).toEqual([]);
  const dir = mkdtempSync(join(tmpdir(), "matrix-"));
  try {
    expect(missingProofs(rows, dir)).toEqual(["a: scripts/matrix.ts"]);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("renderMatrix, summarize and inject", () => {
  const rows = parseMatrix(toml);
  expect(summarize(rows).claude.partial).toBe(1);
  const block = renderMatrix(rows);
  expect(block).toContain("partial (was missing)");
  expect(inject(`a\n${START}\nold\n${END}\nb`, block)).toContain(
    "| x | Thing |",
  );
  expect(() => inject("no markers", block)).toThrow();
});

test("the shipped matrix is fresh and its claude proofs exist", async () => {
  const p = Bun.spawn(
    [
      "bun",
      "--no-install",
      "--no-env-file",
      `${import.meta.dir}/matrix.ts`,
      "--check",
    ],
    { stdout: "ignore", stderr: "pipe" },
  );
  const err = await new Response(p.stderr).text();
  expect({ code: await p.exited, err }).toEqual({ code: 0, err: "" });
});
