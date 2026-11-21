import { expect, test } from "bun:test";
import { formatLedger, parseInventory, parseLedger, referencedNames } from "./repo-archive.ts";

test("referencedNames reads org/name slugs and relative submodule urls, case-insensitively and without .git", () => {
  const text = 'url = ../../aphrody-labs/Foo.git\nrepo = "aphrody-dev/bar"\nurl = ../baz.git\npath = ../../crates/x\nrose-griffon/rg';
  expect([...referencedNames(text)].sort()).toEqual(["bar", "baz", "foo", "rg"]);
});

test("parseInventory keeps the repo column, drops duplicates and malformed rows", () => {
  expect(parseInventory("repo\tcategory\no/a\tx\no/a\ty\nnot a repo\tz\no/b\tw\n")).toEqual(["o/a", "o/b"]);
  expect(() => parseInventory("name\tx\n")).toThrow();
});

test("ledger round-trips and a restore reads only the archived rows", () => {
  const rows = [
    { repo: "o/a", action: "archived", reason: "unreferenced, last push 2026-01-01" },
    { repo: "o/b", action: "kept", reason: "referenced" },
  ] as const;
  const parsed = parseLedger(formatLedger([...rows]));
  expect(parsed).toEqual([...rows]);
  expect(parsed.filter((r) => r.action === "archived").map((r) => r.repo)).toEqual(["o/a"]);
  expect(() => parseLedger("repo\tnope\n")).toThrow(/header/);
});
