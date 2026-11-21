import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import {
  canonicalName,
  defaultScanRoot,
  filterItemsFuzzy,
  findDuplicates,
  fuzzyPairSimilarity,
  fuzzySubsequenceScore,
  normalizeStem,
  parseEzaTree,
  parseForksToml,
  parseGitmodules,
  parseGitSlug,
  parseSubmodulesToml,
  posix,
  renderMarkdown,
  renderTerminalSummary,
  runAudit,
  type VendorEntry,
} from "./vendor-dedup.ts";

const tempDirs: string[] = [];
function makeWorkspace(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), "vdedup-"));
  tempDirs.push(root);
  for (const [rel, body] of Object.entries(files)) {
    const abs = join(root, rel);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, body);
  }
  return root;
}

afterEach(() => {
  delete Bun.env.SRC_ROOT;
  for (const d of tempDirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

describe("vendor-dedup parsers and normalizers", () => {
  test("parseEzaTree reconstructs nested directory paths including empty submodule mountpoints and symlink arrows", () => {
    const tree = [
      "/tmp/demo",
      "├── crates",
      "│   └── core",
      "└── third_party",
      "    ├── cloudflare",
      "    │   ├── agents",
      "    │   └── workerd",
      "    └── wgpu -> ../shared/wgpu",
    ].join("\n");
    const parsed = parseEzaTree(tree, "/tmp/demo").map(posix);
    expect(parsed).toEqual([
      "/tmp/demo/crates",
      "/tmp/demo/crates/core",
      "/tmp/demo/third_party",
      "/tmp/demo/third_party/cloudflare",
      "/tmp/demo/third_party/cloudflare/agents",
      "/tmp/demo/third_party/cloudflare/workerd",
      "/tmp/demo/third_party/wgpu",
    ]);
  });

  test("parseGitmodules, parseSubmodulesToml, parseForksToml, and parseGitSlug extract repo slugs cleanly", () => {
    expect(parseGitSlug("../../aphrody-labs/wgpu.git")).toEqual({ slug: "aphrody-labs/wgpu", leaf: "wgpu" });
    expect(parseGitSlug("https://github.com/gfx-rs/wgpu.git")).toEqual({ slug: "gfx-rs/wgpu", leaf: "wgpu" });
    expect(parseGitSlug("git@github.com:eza-community/eza.git")).toEqual({ slug: "eza-community/eza", leaf: "eza" });

    expect(
      parseGitmodules('[submodule "vendor/wgpu"]\n\tpath = vendor/wgpu\n\turl = ../../aphrody-labs/wgpu.git\n\tbranch = trunk\n'),
    ).toEqual([{ path: "vendor/wgpu", url: "../../aphrody-labs/wgpu.git", branch: "trunk" }]);

    expect(
      parseSubmodulesToml('version = 1\n[[submodule]]\npath = "third_party/wgpu"\nrepo = "aphrody-labs/wgpu"\nbranch = "trunk"\nsha = "abcd"\n'),
    ).toEqual([{ path: "third_party/wgpu", repo: "aphrody-labs/wgpu", branch: "trunk", sha: "abcd" }]);

    expect(
      parseForksToml('[meta]\norg = "aphrody-labs"\n[[fork]]\npath = "wasm/wgpu"\nname = "wgpu"\nupstream = "gfx-rs/wgpu"\nbranch = "trunk"\n'),
    ).toEqual({
      org: "aphrody-labs",
      forks: [{ path: "wasm/wgpu", name: "wgpu", upstream: "gfx-rs/wgpu", branch: "trunk" }],
    });
  });

  test("canonicalName, normalizeStem, and fuzzyPairSimilarity recognize variants without short-stem collisions", () => {
    expect(canonicalName("@scope/next.js")).toBe("next-js");
    expect(normalizeStem("aphrody-chromium")).toBe("chromium");
    expect(normalizeStem("longbridge-nsh")).toBe("longbridgensh");
    expect(normalizeStem("ay-cf")).toBe("aycf");

    expect(fuzzyPairSimilarity("wgpu", "wgpu-hal").score).toBeGreaterThanOrEqual(0.84);
    expect(fuzzyPairSimilarity("mullvad", "mullvadvpn-app").score).toBeGreaterThanOrEqual(0.84);
    expect(fuzzyPairSimilarity("next.js", "nextjs").score).toBeGreaterThanOrEqual(0.9);
    expect(fuzzyPairSimilarity("ay-cf", "vercel-ai").score).toBeLessThan(0.5);
  });

  test("defaultScanRoot derives parent directory or SRC_ROOT override", () => {
    const base = defaultScanRoot("/workspace/ay");
    expect(posix(base)).toBe("/workspace");
    Bun.env.SRC_ROOT = "/custom/src";
    expect(posix(defaultScanRoot("/workspace/ay"))).toContain("/custom/src");
  });
});

describe("vendor-dedup duplicate detection and fuzzy filtering", () => {
  test("finds exact and fuzzy duplicates across repos while skipping same-vendorRoot sibling crates", () => {
    const entry = (overrides: Partial<VendorEntry> & Pick<VendorEntry, "id" | "repo" | "relPath" | "vendorRoot" | "kind" | "name">): VendorEntry => ({
      absPath: `/ws/${overrides.repo}/${overrides.relPath}`,
      fileCount: 0,
      isTestFixture: false,
      discoveredBy: ["eza"],
      ...overrides,
    });

    const entries: VendorEntry[] = [
      entry({ id: "ay:third_party/wgpu", repo: "ay", relPath: "third_party/wgpu", vendorRoot: "ay:third_party", kind: "submodule", name: "wgpu", gitSlug: "aphrody-labs/wgpu", repoLeaf: "wgpu" }),
      entry({ id: "aphrody:vendor/wgpu", repo: "aphrody", relPath: "vendor/wgpu", vendorRoot: "aphrody:vendor", kind: "submodule", name: "wgpu", gitSlug: "aphrody-labs/wgpu", repoLeaf: "wgpu" }),
      entry({ id: "aphrody:vendor/craft-anything/vendor/wgpu-hal", repo: "aphrody", relPath: "vendor/craft-anything/vendor/wgpu-hal", vendorRoot: "aphrody:vendor/craft-anything/vendor", kind: "vendor-dir", name: "wgpu-hal" }),
      entry({ id: "aphrody:vendor/craft-libs/vendor/hayro", repo: "aphrody", relPath: "vendor/craft-libs/vendor/hayro", vendorRoot: "aphrody:vendor/craft-libs/vendor", kind: "vendor-dir", name: "hayro", upstreamRepo: "laurenzv/hayro", pkgName: "hayro" }),
      entry({ id: "aphrody:vendor/craft-libs/vendor/hayro-cmap", repo: "aphrody", relPath: "vendor/craft-libs/vendor/hayro-cmap", vendorRoot: "aphrody:vendor/craft-libs/vendor", kind: "vendor-dir", name: "hayro-cmap", upstreamRepo: "laurenzv/hayro", pkgName: "hayro-cmap" }),
    ];

    const { exactGroups, fuzzyGroups } = findDuplicates(entries, 0.78);
    expect(exactGroups.map((g) => g.key)).toEqual(["wgpu"]);
    expect(exactGroups[0]!.entries.map((e) => e.id)).toEqual(["aphrody:vendor/wgpu", "ay:third_party/wgpu"]);

    // Sibling crates in same vendorRoot (`hayro` & `hayro-cmap`) are not flagged; cross-root `wgpu-hal` is caught.
    expect(fuzzyGroups.length).toBe(1);
    expect(fuzzyGroups[0]!.entries.map((e) => e.id)).toEqual([
      "aphrody:vendor/craft-anything/vendor/wgpu-hal",
      "aphrody:vendor/wgpu",
      "ay:third_party/wgpu",
    ]);
  });

  test("filterItemsFuzzy works identically via pure-Bun scorer and fzf binary", () => {
    const items = ["ay:third_party/wgpu", "aphrody:vendor/mullvad", "shenron:third_party/next.js"];
    expect(fuzzySubsequenceScore("wgpu", items[0]!)).toBeGreaterThan(0);
    expect(fuzzySubsequenceScore("zzz", items[0]!)).toBe(0);
    expect(filterItemsFuzzy(items, (x) => x, "mull", false)).toEqual(["aphrody:vendor/mullvad"]);
    expect(filterItemsFuzzy(items, (x) => x, "wgpu", true)).toEqual(["ay:third_party/wgpu"]);
  });

  test("end-to-end runAudit recurses across temp workspace with eza + rg + fuzzy and ignores test fixtures by default", async () => {
    const ws = makeWorkspace({
      "repo-a/submodules.toml": 'version = 1\n[[submodule]]\npath = "third_party/cloudflare/workerd"\nrepo = "aphrody-labs/workerd"\nbranch = "main"\nsha = "0123456789012345678901234567890123456789"\n',
      "repo-a/third_party/cloudflare/workerd/.keep": "",
      "repo-a/third_party/wgpu/Cargo.toml": '[package]\nname = "wgpu"\n',
      "repo-a/tests/fixtures/vendor/sample/Cargo.toml": '[package]\nname = "sample"\n',
      "repo-b/.gitmodules": '[submodule "vendor/wgpu"]\n\tpath = vendor/wgpu\n\turl = ../../aphrody-labs/wgpu.git\n',
      "repo-b/vendor/wgpu/Cargo.toml": '[package]\nname = "wgpu"\n',
      "repo-b/vendor/workerd-cxx/CMakeLists.txt": "project(workerd_cxx)\n",
      "repo-b/external/craft/vendor/wgpu-hal/Cargo.toml": '[package]\nname = "wgpu-hal"\n',
      "repo-c/forks.toml": '[meta]\norg = "aphrody-labs"\n[[fork]]\npath = "runtime/workerd"\nupstream = "cloudflare/workerd"\n',
      "repo-c/tests/fixtures/vendor/sample/Cargo.toml": '[package]\nname = "sample"\n',
    });

    const report = await runAudit({ scanRoot: ws });
    expect(report.scannedRepos).toEqual(["repo-a", "repo-b", "repo-c"]);
    expect(report.exactGroups.map((g) => g.key)).toEqual(["wgpu", "workerd"]);
    expect(report.exactGroups[0]!.entries.map((e) => e.id)).toEqual(["repo-a:third_party/wgpu", "repo-b:vendor/wgpu"]);
    expect(report.exactGroups[1]!.entries.map((e) => e.id)).toEqual(["repo-a:third_party/cloudflare/workerd", "repo-c:runtime/workerd"]);
    expect(report.fuzzyGroups.some((g) => g.entries.some((e) => e.name === "wgpu-hal"))).toBe(true);
    expect(report.fuzzyGroups.some((g) => g.entries.some((e) => e.name === "workerd-cxx"))).toBe(true);
    expect(report.entries.some((e) => e.name === "sample")).toBe(false);

    const withTests = await runAudit({ scanRoot: ws, includeTests: true });
    expect(withTests.exactGroups.some((g) => g.key === "sample")).toBe(true);

    const filtered = await runAudit({ scanRoot: ws, fuzzyQuery: "workerd" });
    expect(filtered.exactGroups.map((g) => g.key)).toEqual(["workerd"]);
    expect(renderMarkdown(report)).toContain("# Vendor & Third-Party Deduplication Audit");
    expect(renderTerminalSummary(report, { ezaTree: true })).toContain("├── repo-a:third_party/wgpu");
  });
});
