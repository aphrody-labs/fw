import { describe, expect, test } from "bun:test";
import {
  limiter,
  mapCft,
  mapDashRelease,
  mapFirefox,
  mapRepo,
  mapRest,
  mapWebkitFeed,
  parseChromiumVersion,
  parseSources,
  parseV8Version,
  repoOf,
  repoQuery,
  table,
  track,
  type Tracked,
} from "./track.ts";

const SHA = "695124a56869e2896eb6af48b308363010007114";

describe("manifests", () => {
  const upstreams = `
[[upstream]]
name = "servo"
url = "https://github.com/servo/servo"
branch = "main"

[[upstream]]
name = "bun"
url = "https://github.com/aphrody-labs/bun.git"
branch = "bun"
mode = "bun"
`;
  const lock = { servo: { branch: "main", sha: SHA, url: "https://github.com/servo/servo" } };
  const trackTable = {
    chromium: {
      kind: "engine",
      repo: "chromium/chromium",
      provider: "chromium",
      version_channel: "canary",
      checkout: { env: "CHROMIUM_SRC", default: "/srv/chromium/src", ssh: "fr1" },
      release: false,
    },
    ladybird: { kind: "engine", repo: "LadybirdBrowser/ladybird", branch: "master" },
  };

  test("repoOf", () => {
    expect(repoOf("https://github.com/aphrody-labs/bun.git")).toEqual({ owner: "aphrody-labs", repo: "bun" });
    expect(repoOf("https://gitlab.com/a/b")).toBeNull();
    expect(repoOf("nonsense")).toBeNull();
  });

  test("parseSources merges upstreams, lock pins and track entries", () => {
    const s = parseSources(upstreams, lock, trackTable);
    expect(s.map((x) => x.name)).toEqual(["servo", "bun", "chromium", "ladybird"]);
    expect(s[0]).toMatchObject({ kind: "crate", owner: "servo", repo: "servo", branch: "main", pin: SHA });
    expect(s[0].release).toBe(true);
    expect(s[1]).toMatchObject({ kind: "bun", repo: "bun", branch: "bun", pin: null });
    expect(s[2]).toMatchObject({ owner: "chromium", repo: "chromium", provider: "chromium" });
    expect(s[2]).toMatchObject({ release: false, channel: "canary" });
    expect(s[2].checkout).toEqual({ env: "CHROMIUM_SRC", default: "/srv/chromium/src", ssh: "fr1" });
    expect(s[3]).toMatchObject({ branch: "master", source: "https://github.com/LadybirdBrowser/ladybird" });
  });

  test("a track entry replaces an upstream of the same name", () => {
    const s = parseSources(upstreams, lock, { servo: { kind: "engine", repo: "servo/servo", release: false } });
    expect(s.filter((x) => x.name === "servo")).toHaveLength(1);
    expect(s[0]).toMatchObject({ kind: "engine", release: false, pin: null });
  });
});

describe("github mapping", () => {
  const src = parseSources("", {}, { x: { repo: "o/r", branch: "main" } })[0];

  test("repoQuery", () => {
    const q = repoQuery("r0", { ...src, pin: SHA });
    expect(q).toContain('r0:repository(owner:"o",name:"r")');
    expect(q).toContain('qualifiedName:"refs/heads/main"');
    expect(q).toContain(`compare(headRef:"${SHA}"){behindBy}`);
    expect(q).toContain("latestRelease");
    const bare = repoQuery("r1", { ...src, pin: "160.0.1.0", release: false });
    expect(bare).not.toContain("compare");
    expect(bare).not.toContain("latestRelease");
  });

  test("mapRepo prefers the release, else the newest version-like tag", () => {
    const ref = { target: { oid: "a".repeat(40), committedDate: "2026-11-16T22:22:00Z" }, compare: { behindBy: 18 } };
    const latestRelease = { tagName: "v0.8.0", publishedAt: "2026-11-09T12:29:13Z" };
    expect(mapRepo({ ref, latestRelease }, "main")).toEqual({
      head: "a".repeat(40),
      head_date: "2026-11-16T22:22:00.000Z",
      version: "v0.8.0",
      version_date: "2026-11-09T12:29:13.000Z",
      behind: 18,
    });
    const tags = {
      nodes: [
        { name: "rustc-perf", target: { committedDate: "2022-03-25T16:10:45Z" } },
        { name: "v0.6.0", target: { tagger: { date: "2016-10-07T10:51:56Z" } } },
      ],
    };
    expect(mapRepo({ ref: { target: ref.target }, latestRelease: null, refs: tags }, "main")).toMatchObject({
      version: "v0.6.0",
      version_date: "2016-10-07T10:51:56.000Z",
      behind: null,
    });
  });

  test("mapRepo reports a missing repository or branch instead of throwing", () => {
    expect((mapRepo(null, "main") as Error).message).toContain("not found");
    expect((mapRepo({ ref: null }, "dev") as Error).message).toBe("branch dev not found");
  });

  test("mapRest", () => {
    const commit = { sha: SHA, commit: { committer: { date: "2026-11-16T22:22:00Z" } } };
    expect(mapRest(commit, { ahead_by: 7 }, { tag_name: "v1", published_at: "2026-01-01T00:00:00Z" }, null)).toEqual({
      head: SHA,
      head_date: "2026-11-16T22:22:00.000Z",
      version: "v1",
      version_date: "2026-01-01T00:00:00.000Z",
      behind: 7,
    });
    expect(mapRest(commit, null, null, { name: "v0.3" })).toMatchObject({ version: "v0.3", behind: null });
  });
});

describe("release data", () => {
  test("chromiumdash release", () => {
    const j = [{ channel: "Canary", milestone: 160, platform: "Linux", time: 1794856777227, version: "160.0.8166.0" }];
    const date = new Date(1794856777227).toISOString();
    expect(mapDashRelease(j)).toEqual({ version: "160.0.8166.0", date, milestone: 160 });
    expect(mapDashRelease([])).toBeNull();
    expect(mapDashRelease({ error: 1 })).toBeNull();
  });

  test("Chrome for Testing fallback", () => {
    const j = { channels: { Stable: { version: "157.0.8103.78" }, Canary: { version: "160.0.8166.0" } } };
    expect(mapCft(j)).toEqual({
      stable: { version: "157.0.8103.78", date: null },
      canary: { version: "160.0.8166.0", date: null },
    });
  });

  test("firefox_versions", () => {
    const j = {
      FIREFOX_AURORA: "",
      FIREFOX_DEVEDITION: "161.0b4",
      FIREFOX_ESR: "153.7.0esr",
      FIREFOX_NIGHTLY: "162.0a1",
      LAST_MERGE_DATE: "2026-11-05",
      LAST_RELEASE_DATE: "2026-11-10",
      LATEST_FIREFOX_VERSION: "160.0",
    };
    expect(mapFirefox(j, "nightly")).toEqual({
      version: "162.0a1",
      version_date: "2026-11-05T00:00:00.000Z",
      channels: { nightly: "162.0a1", beta: "161.0b4", stable: "160.0", esr: "153.7.0esr" },
    });
    expect(mapFirefox(j, "stable")).toMatchObject({ version: "160.0", version_date: "2026-11-10T00:00:00.000Z" });
    expect(mapFirefox({}, "nightly")).toEqual({ version: null, version_date: null, channels: {} });
  });

  test("webkit.org feed, newest post per channel, non-breaking spaces included", () => {
    const item = (t: string, d: string) => `<item><title>${t}</title><pubDate>${d}</pubDate></item>`;
    const xml = `<rss><channel><title>Blog</title>
      ${item("Other", "Mon, 16 Nov 2026 23:45:44 +0000")}
      ${item("Release Notes for Safari Technology Preview 256", "Thu, 05 Nov 2026 00:09:57 +0000")}
      ${item("WebKit Features for Safari 27.2", "Tue, 03 Nov 2026 18:00:35 +0000")}
      ${item("Release Notes for Safari Technology Preview 255", "Fri, 23 Oct 2026 00:00:00 +0000")}</channel></rss>`;
    expect(mapWebkitFeed(xml, "stp")).toEqual({
      version: "STP 256",
      version_date: "2026-11-05T00:09:57.000Z",
      channels: { stp: "STP 256", safari: "27.2" },
    });
    expect(mapWebkitFeed("<rss/>", "stp")).toEqual({ version: null, version_date: null, channels: {} });
  });
});

describe("checkout files", () => {
  test("chrome/VERSION", () => {
    expect(parseChromiumVersion("MAJOR=157\nMINOR=0\nBUILD=8103\nPATCH=78\n")).toBe("157.0.8103.78");
    expect(parseChromiumVersion("MAJOR=157\n")).toBeNull();
  });

  test("v8-version.h", () => {
    const h = (patch: number) => `#define V8_MAJOR_VERSION 15
#define V8_MINOR_VERSION 7
#define V8_BUILD_NUMBER 112
#define V8_PATCH_LEVEL ${patch}`;
    expect(parseV8Version(h(43))).toBe("15.7.112.43");
    expect(parseV8Version(h(0))).toBe("15.7.112");
    expect(parseV8Version("")).toBeNull();
  });
});

describe("plumbing", () => {
  test("limiter never runs more than max at once and keeps every result", async () => {
    const gate = limiter(3);
    let active = 0;
    let peak = 0;
    const out = await Promise.all(
      Array.from({ length: 12 }, (_, i) =>
        gate(async () => {
          peak = Math.max(peak, ++active);
          await Bun.sleep(5);
          active--;
          return i;
        }),
      ),
    );
    expect(out).toEqual([...Array(12).keys()]);
    expect(peak).toBe(3);
  });

  test("table", () => {
    const rows: Tracked[] = [
      {
        name: "servo",
        kind: "crate",
        source: "",
        head: SHA,
        head_date: "2026-11-16T22:22:00.000Z",
        version: "v0.8.0",
        version_date: null,
        pinned: SHA,
        behind: 18,
      },
      {
        name: "dead",
        kind: "tool",
        source: "",
        head: null,
        head_date: null,
        version: null,
        version_date: null,
        pinned: null,
        behind: null,
        channels: { a: "1" },
        error: "404 Not Found api.github.com",
      },
    ];
    const t = table(rows).split("\n");
    expect(t[0]).toStartWith("name");
    expect(t[2]).toContain("695124a56");
    expect(t[2]).toContain("2026-11-16");
    expect(t[2]).toEndWith("18");
    expect(t[4]).toContain("channels: a=1");
    expect(t[5]).toContain("error: 404 Not Found");
  });
});

test.skipIf(!Bun.env.TRACK_LIVE)(
  "live: track() answers for every manifest entry, with real data",
  async () => {
    const rows = await track(`${import.meta.dir}/../..`);
    const toml = Bun.TOML.parse(await Bun.file(new URL("../upstreams.toml", import.meta.url)).text()) as any;
    const names = new Set(rows.map((r) => r.name));
    for (const u of toml.upstream) expect(names.has(u.name)).toBe(true);
    const chromium = rows.find((r) => r.name === "chromium")!;
    expect(chromium.version).toMatch(/^\d+\.\d+\.\d+\.\d+$/);
    expect(Object.keys(chromium.channels ?? {}).sort()).toEqual(["beta", "canary", "dev", "stable"]);
    expect(rows.find((r) => r.name === "servo")?.head).toMatch(/^[0-9a-f]{40}$/);
    expect(rows.filter((r) => !r.error).length).toBeGreaterThan(rows.length / 2);
  },
  60_000,
);
