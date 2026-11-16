import { describe, expect, test } from "bun:test";
import { liveSource } from "../src/cache.ts";
import { ChromiumDash, DASH_ENDPOINTS, DASH_PAGES, latestByChannel, platformsOf, scheduleGapsDays, scheduleTime } from "../src/dash.ts";
import { ApiError } from "../src/http.ts";
import { COMMIT, FIXTURES, replayFetch } from "./fixtures.ts";

function client() {
  const { fetch, calls } = replayFetch();
  return { dash: new ChromiumDash(liveSource({ fetch, retries: 0 })), calls };
}

describe("ChromiumDash", () => {
  test("releases: newest first with hashes, milestone and previous version", async () => {
    const { dash, calls } = client();
    const { value } = await dash.releases({ channel: "Stable", platform: "Windows", num: 2 });
    expect(calls).toEqual(["https://chromiumdash.appspot.com/fetch_releases?channel=Stable&platform=Windows&num=2"]);
    expect(value).toHaveLength(2);
    const [a, b] = value;
    expect(a).toMatchObject({ channel: "Stable", platform: "Windows" });
    expect(a!.version).toMatch(/^\d+\.0\.\d+\.\d+$/);
    expect(a!.milestone).toBe(Number(a!.version.split(".")[0]));
    expect(a!.previous_version).toBe(b!.version);
    expect(a!.time).toBeGreaterThan(b!.time - 1);
    expect(Object.keys(a!.hashes)).toEqual(expect.arrayContaining(["chromium", "v8", "skia", "angle", "dawn", "devtools", "pdfium", "webrtc"]));
    expect(a!.hashes.chromium).toMatch(/^[0-9a-f]{40}$/);
  });

  test("every channel fixture is for its own channel", async () => {
    const { dash } = client();
    for (const channel of ["Extended", "Beta", "Dev", "Canary"] as const) {
      const [r] = (await dash.releases({ channel, platform: "Windows", num: 1 })).value;
      expect(r!.channel).toBe(channel);
    }
  });

  test("an invalid channel is an ApiError naming the field", async () => {
    const { dash } = client();
    await expect(dash.releases({ channel: "Foo" as never, num: 1 })).rejects.toBeInstanceOf(ApiError);
    await expect(dash.releases({ channel: "Foo" as never, num: 1 })).rejects.toThrow("channel");
  });

  test("version: v8 version and main branch position", async () => {
    const { dash } = client();
    const { value } = await dash.version("158.0.8127.37");
    expect(value.milestone).toBe(158);
    expect(value.v8_version).toMatch(/^\d+\.\d+\.\d+\.\d+$/);
    expect(value.hashes.chromium).toBe(COMMIT);
  });

  test("commit: first landed version, review link and per-channel deployment", async () => {
    const { dash } = client();
    const { value } = await dash.commit(COMMIT);
    expect(value.git_hash).toBe(COMMIT);
    expect(value.first_landed).toBe("158.0.8127.37");
    expect(value.review_info?.host).toBe("chromium-review.googlesource.com");
    expect(Object.keys(value.deployment)).toEqual(expect.arrayContaining(["stable", "beta", "dev", "canary", "extended"]));
    expect(value.source_tree).toContain(COMMIT);
  });

  test("commits: summaries of the newest commits", async () => {
    const { dash } = client();
    const { value } = await dash.commits({ count: 3 });
    expect(value.commits.length).toBeGreaterThan(0);
    expect(value.commits[0]).toMatchObject({ repo: "chromium" });
  });

  test("schedule: dated milestone rows with owners", async () => {
    const { dash } = client();
    const { value } = await dash.schedule({ offset: -3, n: 6 });
    expect(value.mstones).toHaveLength(6);
    for (const m of value.mstones) {
      expect(m.stable_date).toMatch(/^\d{4}-\d\d-\d\dT00:00:00$/);
      expect(scheduleTime(m.branch_point)!).toBeLessThan(scheduleTime(m.stable_date)!);
    }
    expect(scheduleGapsDays(value.mstones).every(d => d % 7 === 0 && d > 0)).toBe(true);
  });

  test("milestones, minibranches and ChromeOS builds", async () => {
    const { dash } = client();
    const ms = (await dash.milestones({ onlyActive: true })).value;
    expect(ms[0]).toHaveProperty("schedule_phase");
    expect(ms.every(m => typeof m.milestone === "number")).toBe(true);
    const mb = (await dash.minibranches()).value;
    expect(mb[0]!.chromium_minibranch).toMatch(/^\d+_\d+$/);
    const cros = (await dash.crosServingBuilds()).value;
    const board = Object.values(cros.builds)[0]!;
    expect((board.brandNames ?? []).length).toBeGreaterThan(0);
  });

  test("latestByChannel and platformsOf work on live-shaped data", async () => {
    const { dash } = client();
    const rows = (await Promise.all((["Stable", "Beta", "Dev", "Canary"] as const).map(channel => dash.releases({ channel, platform: "Windows", num: 1 })))).flatMap(r => r.value);
    expect([...latestByChannel(rows).keys()].sort()).toEqual(["Beta/Windows", "Canary/Windows", "Dev/Windows", "Stable/Windows"]);
    expect(platformsOf(rows)).toEqual(["Windows"]);
  });

  test("the endpoint table covers every recorded Chromium Dash URL and the SPA pages", () => {
    const paths = new Set<string>(DASH_ENDPOINTS.map(e => e.path));
    for (const f of FIXTURES.filter(f => f.url.includes("chromiumdash"))) expect(paths.has(new URL(f.url).pathname)).toBe(true);
    expect(DASH_ENDPOINTS.filter(e => e.auth).map(e => e.path)).toContain("/fetch_builds");
    expect(DASH_PAGES).toContain("/schedule");
  });
});
