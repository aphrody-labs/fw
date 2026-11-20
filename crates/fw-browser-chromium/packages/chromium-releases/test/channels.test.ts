import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { CHANNELS, channel, fetchChannelsPage, measuredStableWeeks, parseChannelsPage, withReleases } from "../src/channels.ts";
import { liveSource } from "../src/cache.ts";
import { ChromiumDash } from "../src/dash.ts";
import { FIXTURES, fixturePath, replayFetch } from "./fixtures.ts";

const html = readFileSync(fixturePath(FIXTURES.find(f => f.name === "channels-page")!), "utf8");

describe("channels dataset", () => {
  test("is ordered from the tip of tree to the most stable, with unique ids", () => {
    expect(new Set(CHANNELS.map(c => c.id)).size).toBe(CHANNELS.length);
    expect(CHANNELS.map(c => c.id).sort()).toEqual(["beta", "canary", "dev", "extended", "stable"]);
    const byStability = [...CHANNELS].sort((a, b) => a.stability - b.stability);
    expect(byStability.at(-1)!.id).toBe("canary");
    expect(byStability[0]!.stability).toBe(0);
  });

  test("channel() resolves ids, names and Chromium Dash names", () => {
    expect(channel("Stable")?.id).toBe("stable");
    expect(channel("Extended")?.id).toBe("extended");
    expect(channel("Extended Stable")?.id).toBe("extended");
    expect(channel("canary")?.dash).toBe("Canary");
    expect(channel("nope")).toBeUndefined();
  });
});

describe("the live channels page", () => {
  test("parseChannelsPage reads the cadence, install links and guide", () => {
    const page = parseChannelsPage(html);
    expect(page.intro).toContain("twice-daily Canary");
    expect(page.stableMajorWeeks).toBe(4);
    expect(page.downloads.desktop.stable).toBe("https://google.com/chrome");
    expect(page.downloads.desktop.canary).toBe("https://google.com/chrome/canary");
    expect(page.downloads.android.beta).toContain("com.chrome.beta");
    expect(Object.keys(page.downloads.ios).sort()).toEqual(["beta", "stable"]);
    expect(Object.keys(page.guide).sort()).toEqual(["beta", "canary", "dev", "stable"]);
    expect(page.guide.canary).toContain("twice daily");
    expect(page.notes.some(n => n.includes("Other builds"))).toBe(true);
  });

  test("the dataset agrees with the page: availability and cadence", () => {
    const page = parseChannelsPage(html);
    for (const c of CHANNELS) {
      expect(!!page.downloads.desktop[c.id]).toBe(c.id !== "extended" ? c.availability.desktop : false);
      expect(!!page.downloads.android[c.id]).toBe(c.availability.android);
      expect(!!page.downloads.ios[c.id]).toBe(c.availability.ios);
    }
    expect(channel("stable")!.cadence).toContain("every four weeks");
    expect(page.guide.stable).toContain("every four weeks for major");
    expect(page.guide.dev).toContain("once or twice weekly");
    expect(channel("dev")!.cadence).toContain("once or twice weekly");
    expect(channel("canary")!.cadence).toContain("twice daily");
  });

  test("fetchChannelsPage goes through fetch", async () => {
    const { fetch } = replayFetch();
    const page = await fetchChannelsPage(fetch as typeof globalThis.fetch);
    expect(page.stableMajorWeeks).toBe(4);
  });
});

describe("live joins", () => {
  test("withReleases attaches the newest release per channel and the schedule measures the cadence", async () => {
    const { fetch } = replayFetch();
    const dash = new ChromiumDash(liveSource({ fetch, retries: 0 }));
    const rows = (await Promise.all((["Stable", "Extended", "Beta", "Dev", "Canary"] as const).map(channel => dash.releases({ channel, platform: "Windows", num: 1 })))).flatMap(r => r.value);
    const status = withReleases(rows, "Windows");
    expect(status.map(s => [s.id, s.release?.platform])).toEqual([["stable", "Windows"], ["extended", "Windows"], ["beta", "Windows"], ["dev", "Windows"], ["canary", "Windows"]]);
    expect(status.find(s => s.id === "canary")!.release!.milestone).toBeGreaterThan(status.find(s => s.id === "stable")!.release!.milestone);
    const schedule = (await dash.schedule({ offset: -3, n: 6 })).value;
    // The recorded schedule is a two-week cycle while the chromium.org page still says four weeks: the page lags, so the measure is reported next to it.
    expect(measuredStableWeeks(schedule.mstones)).toBe(2);
    expect(parseChannelsPage(html).stableMajorWeeks).toBe(4);
  });
});
