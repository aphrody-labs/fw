import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { type Feature, load, problems, render } from "./features.ts";

const root = resolve(import.meta.dir, "..");
const products = new Set(["search", "os", "platform"]);
const row = (over: Partial<Feature> = {}): Feature => ({
  product: "search",
  id: "x",
  title: "X",
  reference: "https://example.org/",
  layer: "ui",
  status: "todo",
  ...over,
});

describe("feature matrix", () => {
  test("the committed matrix parses and has no problems", () => {
    const features = load(readFileSync(join(root, "docs/features.toml"), "utf8"));
    const ay = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as { products: Record<string, unknown> };
    expect(features.length).toBeGreaterThan(40);
    expect(problems(features, new Set([...Object.keys(ay.products), "platform"]), () => true)).toEqual([]);
  });

  test("done without proof is refused", () => {
    expect(problems([row({ status: "done" })], products, () => true)).toEqual(["search/x: done without proof"]);
  });

  test("a repo-path proof must exist; URLs and test ids are accepted", () => {
    expect(problems([row({ status: "done", proof: "missing.png" })], products, () => false)).toEqual([
      "search/x: proof missing.png does not exist",
    ]);
    expect(problems([row({ status: "done", proof: "https://ci/run/1" })], products, () => false)).toEqual([]);
    expect(problems([row({ status: "done", proof: "tools/features.test.ts::done" })], products, () => false)).toEqual([]);
  });

  test("duplicates, unknown products, layers and non-https references are refused", () => {
    const issues = problems(
      [row(), row(), row({ id: "y", product: "nope", layer: "x" as Feature["layer"], reference: "http://a" })],
      products,
      () => true,
    );
    expect(issues).toEqual([
      "search/x: duplicate id",
      "nope/y: unknown product",
      "nope/y: unknown layer x",
      "nope/y: reference is not an https URL",
    ]);
  });

  test("render counts done rows per product and marks hardware proofs", () => {
    const md = render([row({ status: "done", proof: "a" }), row({ id: "h", proof_kind: "hardware" })]);
    expect(md).toContain("## search (1/2 done)");
    expect(md).toContain("needs hardware");
  });
});
