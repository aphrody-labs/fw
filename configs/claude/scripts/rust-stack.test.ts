import { expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { contradictions, directDependencies, END, inject, parseStack, renderStack, START, stringifyStack, summarize } from "./rust-stack.ts";

const toml = `
[[entry]]
name = "serde"
kind = "crate"
domain = "serde"
fw = "used"
purpose = "serialization"
reason = "declared"
source = "https://crates.io/crates/serde"

[[entry]]
name = "smol"
kind = "crate"
domain = "async"
fw = "reject"
purpose = "runtime"
reason = "tokio only"
source = "https://crates.io/crates/smol"
`;

test("parseStack validates kind, status, required fields and duplicates", () => {
  expect(parseStack(toml).entries).toHaveLength(2);
  expect(() => parseStack(toml.replace('"reject"', '"maybe"'))).toThrow("invalid status");
  expect(() => parseStack(toml.replace('reason = "tokio only"\n', ""))).toThrow("reason required");
  expect(() => parseStack(toml.replace("https://crates.io/crates/smol", "http://x"))).toThrow("https");
  expect(() => parseStack(toml + toml)).toThrow("duplicate");
});

test("stringify then parse is lossless and summarize counts statuses", () => {
  const stack = parseStack(toml);
  expect(parseStack(stringifyStack(stack))).toEqual(stack);
  expect(summarize(stack.entries)).toEqual({ used: 1, adopt: 0, reject: 1, total: 2 });
});

test("render and inject replace only the marked block", () => {
  const block = renderStack(parseStack(toml));
  expect(block).toContain("| [serde](https://crates.io/crates/serde) | crate |");
  const doc = `head\n${START}\nold\n${END}\ntail`;
  const next = inject(doc, block);
  expect(next.startsWith("head\n")).toBe(true);
  expect(next.endsWith("\ntail")).toBe(true);
  expect(next).not.toContain("old");
  expect(() => inject("none", block)).toThrow("markers");
});

test("contradictions compare statuses with the manifests and proof paths", () => {
  const root = mkdtempSync(join(tmpdir(), "rust-stack-"));
  try {
    mkdirSync(join(root, "crates", "a"), { recursive: true });
    writeFileSync(join(root, "crates", "a", "Cargo.toml"), `[package]\nname = "a"\n[dependencies]\nserde = "1"\nsmol = "2"\nlocal = { path = "../b" }\n`);
    expect([...directDependencies(root)].sort()).toEqual(["serde", "smol"]);
    const entries = parseStack(toml).entries;
    expect(contradictions(entries, root)).toEqual({ errors: [], warnings: ["smol: status reject but a Cargo.toml declares it"] });
    expect(contradictions([{ ...entries[0]!, name: "tokio" }], root).warnings).toEqual(["tokio: status used but no Cargo.toml declares it"]);
    const tool = { ...entries[0]!, kind: "tool" as const, name: "cargo-x", proof: "ci.yml" };
    expect(contradictions([tool], root).errors).toEqual(["cargo-x: proof path ci.yml missing"]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
