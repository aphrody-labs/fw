import { describe, expect, test } from "bun:test";
import { Cache } from "@aphrody/chromium-releases";
import { replayFetch } from "../../chromium-releases/test/fixtures.ts";
import { pickVersion } from "../src/version.ts";

describe("pickVersion", () => {
  test("stable on Windows: the channel's Chromium and a CEF minimal build for it", async () => {
    const { fetch } = replayFetch();
    const sel = await pickVersion({ channel: "stable", cefPlatform: "windows64", cache: new Cache(":memory:"), fetch });
    expect(sel.channel).toBe("stable");
    expect(sel.chromium.version).toMatch(/^\d+\.0\.\d+\.\d+$/);
    expect(sel.cef?.minimal?.name).toMatch(/^cef_binary_.*windows64.*minimal/);
    expect(sel.cef?.downloadUrl).toStartWith("https://cef-builds.spotifycdn.com/");
  });

  test("every channel resolves, newer channels never pick an older Chromium than Stable", async () => {
    const { fetch } = replayFetch();
    const cache = new Cache(":memory:");
    const milestones: Record<string, number> = {};
    for (const channel of ["stable", "beta", "dev", "canary"]) milestones[channel] = (await pickVersion({ channel, cefPlatform: "windows64", cache, fetch })).chromium.milestone;
    expect(milestones.beta!).toBeGreaterThanOrEqual(milestones.stable!);
    expect(milestones.dev!).toBeGreaterThanOrEqual(milestones.beta!);
    expect(milestones.canary!).toBeGreaterThanOrEqual(milestones.dev!);
  });
});
