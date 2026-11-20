import { expect, test } from "bun:test";
import { parseInventory, referencedNames } from "./repo-archive.ts";

test("referencedNames reads org/name slugs and relative submodule urls, case-insensitively and without .git", () => {
  const text = 'url = ../../aphrody-labs/Foo.git\nrepo = "aphrody-dev/bar"\nurl = ../baz.git\npath = ../../crates/x\nrose-griffon/rg';
  expect([...referencedNames(text)].sort()).toEqual(["bar", "baz", "foo", "rg"]);
});

test("parseInventory keeps the repo column, drops duplicates and malformed rows", () => {
  expect(parseInventory("repo\tcategory\no/a\tx\no/a\ty\nnot a repo\tz\no/b\tw\n")).toEqual(["o/a", "o/b"]);
  expect(() => parseInventory("name\tx\n")).toThrow();
});
