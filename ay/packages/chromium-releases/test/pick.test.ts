import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { Cache, liveSource } from "../src/cache.ts";
import { ChromiumDash } from "../src/dash.ts";
import { chooseCef, hostPlatform, loadCefIndex, pickChromium, trimCefIndex, type CefBuild } from "../src/pick.ts";
import { FIXTURES, fixturePath, replayFetch } from "./fixtures.ts";

const raw = JSON.parse(readFileSync(fixturePath(FIXTURES.find(f => f.name === "cef-index")!), "utf8"));
const build = (chromium_version: string, cef = `${chromium_version}.cef`): CefBuild => ({ cef_version: cef, channel: "stable", chromium_version });

describe("CEF index", () => {
  test("trimCefIndex keeps the minimal distribution, newest milestone first", () => {
    const index = trimCefIndex(raw);
    const list = index.windows64!;
    expect(list.length).toBeGreaterThan(5);
    expect(list.every(b => b.minimal?.name.includes("windows64") && b.minimal.name.endsWith("minimal.tar.bz2"))).toBe(true);
    const majors = list.map(b => Number(b.chromium_version.split(".")[0]));
    expect(majors).toEqual([...majors].sort((a, b) => b - a));
  });

  test("loadCefIndex fetches once and then serves the cache", async () => {
    const { fetch, calls } = replayFetch();
    const cache = new Cache(":memory:");
    const a = await loadCefIndex({ cache, fetch });
    const b = await loadCefIndex({ cache, fetch });
    expect(b).toEqual(a);
    expect(calls).toHaveLength(1);
  });

  test("loadCefIndex serves a stale copy when the index is unreachable", async () => {
    const cache = new Cache(":memory:");
    cache.write("https://cef-builds.spotifycdn.com/index.json#trimmed", { linux64: [build("150.0.1.1")] }, 1);
    const down = async () => new Response("down", { status: 503 });
    expect((await loadCefIndex({ cache, fetch: down, maxAgeMs: 10 })).linux64).toHaveLength(1);
  });
});

describe("chooseCef", () => {
  const builds = [build("156.0.8078.96"), build("156.0.8078.10"), build("154.0.1.1")];
  test("exact version wins", () => expect(chooseCef(builds, { version: "156.0.8078.10", milestone: 156 })).toMatchObject({ match: "exact", lag: 0, build: { chromium_version: "156.0.8078.10" } }));
  test("same milestone picks the newest", () => expect(chooseCef(builds, { version: "156.0.8078.99", milestone: 156 })).toMatchObject({ match: "milestone", build: { chromium_version: "156.0.8078.96" } }));
  test("an older CEF is reported with its lag in milestones", () => expect(chooseCef(builds, { version: "158.0.1.1", milestone: 158 })).toMatchObject({ match: "behind", lag: 2, build: { chromium_version: "156.0.8078.96" } }));
  test("nothing at or below the milestone", () => expect(chooseCef(builds, { version: "150.0.1.1", milestone: 150 })).toEqual({ match: "none", lag: 0 }));
  test("a newer CEF build of the same Chromium wins the tie", () => {
    const tie = [build("158.0.8127.18", "158.0.1+gaaa+chromium-158.0.8127.18"), build("158.0.8127.18", "158.0.3+gbbb+chromium-158.0.8127.18")];
    expect(chooseCef(tie, { version: "158.0.8127.20", milestone: 158 }).build!.cef_version).toStartWith("158.0.3");
  });
});

describe("pickChromium", () => {
  test("matches the channel's live Chromium to a CEF build from the same index", async () => {
    const { fetch } = replayFetch();
    const dash = new ChromiumDash(liveSource({ fetch, retries: 0 }));
    const sel = await pickChromium(dash, () => loadCefIndex({ fetch }), { channel: "Stable", cefPlatform: "windows64" });
    const stable = (await dash.releases({ channel: "Stable", platform: "Windows", num: 1 })).value[0]!;
    expect(sel).toMatchObject({ channel: "stable", platform: { dash: "Windows", cef: "windows64" }, chromium: { version: stable.version, milestone: stable.milestone } });
    expect(sel.cef?.minimal?.name).toContain("windows64");
    expect(sel.cef?.downloadUrl).toStartWith("https://cef-builds.spotifycdn.com/cef_binary_");
    expect(sel.cef?.downloadUrl).not.toContain("+");
    expect(["exact", "milestone", "behind"]).toContain(sel.match);
    expect(sel.lag).toBe(sel.match === "behind" ? stable.milestone - Number(sel.cef!.chromium_version.split(".")[0]) : 0);
  });

  test("rejects an unknown channel", async () => {
    const { fetch } = replayFetch();
    const dash = new ChromiumDash(liveSource({ fetch, retries: 0 }));
    await expect(pickChromium(dash, async () => ({}), { channel: "nightly" })).rejects.toThrow("unknown channel");
  });

  test("hostPlatform maps Bun's platform and arch", () => {
    expect(hostPlatform("win32", "x64")).toEqual({ cef: "windows64", dash: "Windows" });
    expect(hostPlatform("darwin", "arm64")).toEqual({ cef: "macosarm64", dash: "Mac" });
    expect(hostPlatform("linux", "x64")).toEqual({ cef: "linux64", dash: "Linux" });
    expect(hostPlatform("linux", "arm64")).toEqual({ cef: "linuxarm64", dash: "Linux" });
  });
});
