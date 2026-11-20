import { describe, expect, test } from "bun:test";
import { FORBIDDEN, undeclaredPlugins } from "./hygiene.ts";

const hits = (text: string): string[] =>
  FORBIDDEN.filter((r) => r.re.test(text)).map((r) => r.label);

describe("FORBIDDEN", () => {
  test("has unique labels", () => {
    const labels = FORBIDDEN.map((r) => r.label);
    expect(new Set(labels).size).toBe(labels.length);
  });

  test.each([
    ["C:\\aphrody\\crates", "stale C:\\aphrody path"],
    ["see c:/aphrody/x", "stale C:\\aphrody path"],
    ["the gpui-os tree", "stale gpui-os ref"],
    ["projects/C--aphrody/memory", "stale C--aphrody slug"],
    ["bun-upstream-canary", "stale bun-upstream-canary path"],
    ["C:\\Users\\aphro\\.claude", "hardcoded user home path"],
    ["c:/users/aphro/x", "hardcoded user home path"],
    ["pre-tool-guard.ts", "deprecated pre-tool-guard"],
    ["aphrody-skills@aphrody", "stale aphrody-skills ref"],
    ["A Sovereign OS", "Sovereign slop wording"],
  ])("matches %j as %s", (sample, label) => {
    expect(hits(sample)).toContain(label);
  });

  test.each([
    "C:\\src\\aphrody-dev\\ay",
    "C:/aphrodyx is not it",
    "C--src",
    "aphrody-skillset",
    "C:\\Users\\other\\x",
    "sovereignty",
    "gpui-osx",
    "plain text",
  ])("does not match %j", (sample) => {
    expect(hits(sample)).toEqual([]);
  });
});

describe("undeclaredPlugins", () => {
  test("empty settings yield nothing", () => {
    expect(undeclaredPlugins({})).toEqual([]);
  });

  test("local aphrody marketplace is always known", () => {
    expect(
      undeclaredPlugins({ enabledPlugins: { "x@aphrody": true } }),
    ).toEqual([]);
  });

  test("the built-in Anthropic directory is always known", () => {
    expect(
      undeclaredPlugins({
        enabledPlugins: { "cloudflare@anthropic-plugin-directory": true },
      }),
    ).toEqual([]);
  });

  test("flags marketplaces absent from extraKnownMarketplaces", () => {
    const out = undeclaredPlugins({
      enabledPlugins: {
        "a@declared": true,
        "b@missing": true,
        "c@aphrody": false,
        bare: true,
      },
      extraKnownMarketplaces: { declared: {} },
    });
    expect(out).toEqual(["b@missing", "bare"]);
  });
});
