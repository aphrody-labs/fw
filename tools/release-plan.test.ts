import { describe, expect, test } from "bun:test";
import { CHANNEL_ALIASES, CHANNELS as EDGE_CHANNELS } from "../apps/edge/src/manifest.ts";
import { CHANNELS, channelName, config, milestone, parseTag, plan, version } from "./release-plan.ts";

const schedule = { cadence_weeks: 4, anchor_milestone: 5, anchor_stable: "2026-11-21", beta_weeks: 3, extended_every: 2, extended_weeks: 8, lts_weeks: 104 };

describe("release train", () => {
  test("the edge Worker serves exactly the channels and aliases of release.toml", () => {
    expect([...EDGE_CHANNELS]).toEqual(CHANNELS);
    expect(CHANNEL_ALIASES).toEqual(config.channels.aliases);
  });

  test("milestones follow the cadence, cut three weeks before stable", () => {
    expect(milestone(5, schedule)).toMatchObject({ branch: "2026-10-31", stable: "2026-11-21", next: "2026-12-19", extendedUntil: null, ltsUntil: null });
    expect(milestone(6, schedule)).toMatchObject({ branch: "2026-11-28", stable: "2026-12-19", extendedUntil: "2027-02-13" });
  });

  test("the last milestone of a calendar year is longterm for two years (kernel.org)", () => {
    expect(milestone(6, schedule).ltsUntil).toBe("2028-12-16");
    expect(milestone(7, schedule).ltsUntil).toBeNull();
  });

  test("main carries the milestone after the newest branch point", () => {
    expect(plan("2026-11-21", schedule).channels).toEqual({ canary: 6, dev: 6, beta: null, stable: 5, extended: [], lts: [] });
    expect(plan("2026-11-30", schedule).channels).toEqual({ canary: 7, dev: 7, beta: 6, stable: 5, extended: [], lts: [] });
    expect(plan("2027-01-20", schedule).channels).toEqual({ canary: 8, dev: 8, beta: null, stable: 7, extended: [6], lts: [6] });
  });

  test("versions order canary < beta < stable within a milestone, as semver", () => {
    const ordered = [version("canary", 6, 412), version("beta", 6, 380, 4), version("stable", 6, 380, 0), version("stable", 6, 380, 2)];
    expect(ordered).toEqual(["6.0.0-canary.412", "6.380.0-beta.4", "6.380.0", "6.380.2"]);
    const sorted = [...ordered].sort((a, b) => Bun.semver.order(a, b));
    expect(sorted).toEqual(ordered);
  });

  test("tags parse back to their channel", () => {
    expect(parseTag("v6.0.0-canary.412", "2026-11-25")).toMatchObject({ channel: "canary", milestone: 6 });
    expect(parseTag("v6.380.0-beta.4", "2026-12-01")).toMatchObject({ channel: "beta", n: 4 });
    expect(parseTag("v5.359.0", "2026-11-21")).toMatchObject({ channel: "stable", milestone: 5, build: 359 });
    expect(() => parseTag("v2026.11.0-foo", "2026-11-21")).toThrow();
  });

  test("old channel names resolve to the train's", () => {
    expect(channelName("preview")).toBe("beta");
    expect(channelName("nightly")).toBe("canary");
    expect(() => channelName("weekly")).toThrow();
  });
});
