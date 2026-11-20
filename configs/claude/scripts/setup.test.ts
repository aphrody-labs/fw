import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { enabledPlugins, knownMarketplaces } from "./setup.ts";

let tmp: string;
const write = (name: string, data: unknown): string => {
  const p = join(tmp, name);
  writeFileSync(p, typeof data === "string" ? data : JSON.stringify(data));
  return p;
};

beforeAll(() => {
  tmp = mkdtempSync(join(tmpdir(), "claude-setup-test-"));
});
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});

describe("enabledPlugins", () => {
  test("returns only plugins set to true", () => {
    const p = write("a.json", {
      enabledPlugins: { "a@m": true, "b@m": false, "c@n": true },
    });
    expect([...enabledPlugins(p)].sort()).toEqual(["a@m", "c@n"]);
  });

  test("empty set without the key", () => {
    expect(enabledPlugins(write("b.json", {})).size).toBe(0);
  });

  test("throws on invalid JSON or a missing file", () => {
    expect(() => enabledPlugins(write("c.json", "{oops"))).toThrow();
    expect(() => enabledPlugins(join(tmp, "missing.json"))).toThrow();
  });
});

describe("knownMarketplaces", () => {
  test("prefers repo, then path, and drops empty sources", () => {
    const p = write("d.json", {
      extraKnownMarketplaces: {
        one: { source: { source: "github", repo: "org/one" } },
        two: { source: { source: "directory", path: "/x/two" } },
        three: {
          source: { source: "github", repo: "org/three", path: "/ignored" },
        },
        four: { source: { source: "url" } },
      },
    });
    expect(knownMarketplaces(p)).toEqual(["org/one", "/x/two", "org/three"]);
  });

  test("empty list without the key", () => {
    expect(knownMarketplaces(write("e.json", {}))).toEqual([]);
  });
});

describe("module import", () => {
  test("has no side effects: importing in a subprocess prints nothing", () => {
    const r = Bun.spawnSync(
      [
        "bun",
        "--no-install",
        "--no-env-file",
        "-e",
        `const m = await import(${JSON.stringify(join(import.meta.dir, "setup.ts"))}); console.log(typeof m.enabledPlugins, typeof m.knownMarketplaces);`,
      ],
      { cwd: tmp, stdout: "pipe", stderr: "pipe", stdin: "ignore" },
    );
    expect(r.exitCode).toBe(0);
    expect(r.stdout.toString().trim()).toBe("function function");
    expect(r.stderr.toString()).toBe("");
  });
});
