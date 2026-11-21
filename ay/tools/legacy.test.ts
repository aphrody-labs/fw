import { expect, test } from "bun:test";
import { classify, countFiles, norm, stateOf, summary, toToml, words, type Unit } from "./legacy.ts";

const keywords = { browser: ["browser", "chromium", "cef"], brand: ["m3", "tokens", "design"], cloud: ["edge", "deploy"] };

test("words split names and paths", () => {
  expect(words("frameworks/m3-tokens")).toEqual(["frameworks", "m3", "tokens"]);
});

test("classify picks the product with the most keyword hits, else platform", () => {
  expect(classify("@aphrody/m3-tokens", "frameworks/m3-tokens", keywords)).toBe("brand");
  expect(classify("chromium-ts", "frameworks/chromium-ts", keywords)).toBe("browser");
  expect(classify("env-kit", "system/env-kit", keywords)).toBe("platform");
});

test("norm drops scopes, product prefixes and separators", () => {
  expect(norm("@aphrody/m3-tokens")).toBe("m3tokens");
  expect(norm("ay_brand")).toBe("brand");
  expect(norm("aphrody-chromium")).toBe("chromium");
});

test("stateOf: ported when present here, tracked when named in docs, else review", () => {
  const here = new Set(["brand", "chromium"]);
  expect(stateOf("ay_brand", here, "")).toBe("ported");
  expect(stateOf("lightpanda", here, "we measure lightpanda next to obscura")).toBe("tracked");
  expect(stateOf("flowcat-core", here, "nothing")).toBe("review");
  expect(stateOf("ui", here, "ui everywhere")).toBe("review");
});

test("countFiles attributes each file to its longest prefix", () => {
  const c = countFiles(["a/b/x", "a/b/c/y", "a/z", "q"], ["a", "a/b", "a/b/c"]);
  expect([c.get("a"), c.get("a/b"), c.get("a/b/c")]).toEqual([1, 1, 1]);
});

test("toToml round-trips through Bun.TOML", () => {
  const u: Unit = { kind: "crate", name: "x", path: "system/x", description: 'say "hi"', files: 3, product: "os", state: "review" };
  const t = Bun.TOML.parse(toToml([u], [{ ecosystem: "cargo", name: "serde", version: "1", users: 4 }], { repo: "o/r" })) as any;
  expect(t.meta.repo).toBe("o/r");
  expect(t.unit[0]).toEqual(u);
  expect(t.dep[0].users).toBe(4);
  expect(summary([u], [])).toContain("| crate | 0 | 0 | 1 |");
});
