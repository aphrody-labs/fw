import { describe, expect, test } from "bun:test";
import { checkAll, loadManifest, relativeForkUrl, renderGitmodules, validateManifest } from "./submodules.ts";

describe("Pop!_OS / COSMIC-style fork submodules manifest", () => {
  test("every fork has a valid 40-hex SHA, relative URL, and zero duplicate repos", () => {
    const m = loadManifest();
    expect(m.submodule.length).toBeGreaterThanOrEqual(35);
    expect(validateManifest(m)).toEqual([]);
    expect(relativeForkUrl("aphrody-labs/dioxus")).toBe("../../aphrody-labs/dioxus.git");
    const gm = renderGitmodules(m);
    expect(gm).toContain('[submodule "third_party/dioxus"]');
    expect(gm).toContain('[submodule "third_party/cloudflare/cloudflare-os"]');
    expect(gm).toContain('[submodule "third_party/vercel/workflow"]');
    expect(gm).toContain('[submodule "third_party/browser"]');
  });

  test("rejects duplicate fork checkouts and non-third_party paths", () => {
    const errs = validateManifest({
      version: 1,
      submodule: [
        { path: "vendor/foo", repo: "aphrody-labs/dioxus", branch: "main", sha: "a".repeat(40), domain: "d", role: "r" },
        { path: "third_party/bar", repo: "aphrody-labs/dioxus", branch: "main", sha: "123", domain: "d", role: "r" },
      ],
    });
    expect(errs.some((e) => e.includes("third_party/"))).toBe(true);
    expect(errs.some((e) => e.includes("duplicate fork submodule"))).toBe(true);
    expect(errs.some((e) => e.includes("invalid 40-hex sha"))).toBe(true);
  });

  test("committed .gitmodules and git index gitlinks match submodules.toml", () => {
    expect(checkAll()).toEqual([]);
  });
});
