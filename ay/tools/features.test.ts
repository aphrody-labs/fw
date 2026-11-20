import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { type Feature, type Owners, load, ownerOf, problems, proofPath, render, splitRepoProof } from "./features.ts";

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
    const repos = Bun.TOML.parse(readFileSync(join(root, "ay.toml"), "utf8")) as {
      products: Record<string, { owner?: string }>;
      repos: Record<string, unknown>;
      matrix?: { platform_owner?: string };
    };
    const owners: Owners = {
      byProduct: {
        ...(repos.matrix?.platform_owner ? { platform: repos.matrix.platform_owner } : {}),
        ...Object.fromEntries(Object.entries(repos.products).flatMap(([k, v]) => (v.owner ? [[k, v.owner]] : []))),
      },
      repos: new Set(Object.keys(repos.repos)),
    };
    expect(problems(features, new Set([...Object.keys(ay.products), "platform"]), () => true, owners)).toEqual([]);
  });

  test("done without proof is refused", () => {
    expect(problems([row({ status: "done" })], products, () => true)).toEqual(["search/x: done without proof"]);
  });

  test("a repo-path proof must exist; URLs and test ids are accepted", () => {
    expect(problems([row({ status: "done", proof: "missing.png" })], products, () => false)).toEqual([
      "search/x: proof missing.png does not exist",
    ]);
    expect(problems([row({ status: "done", proof: "https://ci/run/1" })], products, () => false)).toEqual([]);
    expect(problems([row({ status: "done", proof: "tools/features.test.ts::done" })], products, () => true)).toEqual([]);
  });

  test("a path::test proof is checked on its file part", () => {
    const seen: string[] = [];
    const exists = (p: string) => (seen.push(p), false);
    expect(problems([row({ status: "done", proof: "crates/gone/tests/e2e.rs::a_test" })], products, exists)).toEqual([
      "search/x: proof crates/gone/tests/e2e.rs::a_test does not exist",
    ]);
    expect(seen).toEqual(["crates/gone/tests/e2e.rs"]);
  });

  test("a <repo>:path proof names a [repos] key; unknown prefixes stay plain paths", () => {
    const repos = new Set(["browser"]);
    expect(splitRepoProof("browser:packages/x/test.ts", repos)).toEqual({ repo: "browser", path: "packages/x/test.ts" });
    expect(splitRepoProof("other:packages/x/test.ts", repos)).toEqual({ path: "other:packages/x/test.ts" });
    expect(proofPath("browser:a/b.ts::name with :: inside")).toBe("browser:a/b.ts");
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

  test("a row's owner is its own, else its product's, and must be a key of [repos]", () => {
    const owners: Owners = { byProduct: { search: "aphrody" }, repos: new Set(["aphrody", "ay"]) };
    expect(ownerOf(row(), owners)).toBe("aphrody");
    expect(ownerOf(row({ owner: "ay" }), owners)).toBe("ay");
    expect(problems([row({ owner: "nope" })], products, () => true, owners)).toEqual(["search/x: owner nope is not a key of [repos]"]);
    expect(problems([row({ product: "os" })], products, () => true, owners)).toEqual(["os/x: no owner (set owner on the row or on the product)"]);
  });

  test("render adds an owner column and a rows-by-owner summary", () => {
    const owners: Owners = { byProduct: { search: "aphrody" }, repos: new Set(["aphrody", "ay"]) };
    const md = render([row({ status: "done", proof: "a", owner: "ay" }), row({ id: "z" })], owners);
    expect(md).toContain("| ay | 1 | 1 |");
    expect(md).toContain("| aphrody | 1 | 0 |");
    expect(md).toContain("| Feature | Owner | Layer | Status | Proof | Reference |");
  });
});
