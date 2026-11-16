import { describe, expect, test } from "bun:test";
import { liveSource } from "../src/cache.ts";
import { ChromeStatus, STATUS_ENDPOINTS } from "../src/status.ts";
import { FIXTURES, replayFetch } from "./fixtures.ts";

function client() {
  const { fetch, calls } = replayFetch();
  return { status: new ChromeStatus(liveSource({ fetch, retries: 0 })), calls };
}

describe("ChromeStatus", () => {
  test("channels: stable, beta and dev with consecutive milestones", async () => {
    const { status } = client();
    const { value } = await status.channels();
    expect(Object.keys(value)).toEqual(expect.arrayContaining(["stable", "beta", "dev"]));
    expect(value.beta!.mstone).toBe(value.stable!.mstone + 1);
    expect(value.dev!.mstone).toBe(value.beta!.mstone + 1);
    expect(value.stable!.stable_date).toMatch(/^\d{4}-\d\d-\d\dT/);
  });

  test("channels with a range are keyed by milestone", async () => {
    const { status, calls } = client();
    const { value } = await status.channels({ start: 157, end: 158 });
    expect(calls[0]).toBe("https://chromestatus.com/api/v0/channels?start=157&end=158");
    expect(Object.keys(value)).toEqual(["157", "158"]);
  });

  test("feature: the XSSI guard is stripped and enums carry display text", async () => {
    const { status } = client();
    const { value: f } = await status.feature(5191490363064320);
    expect(f.id).toBe(5191490363064320);
    expect(f.name).toContain("WebRTC");
    expect(f.feature_type).toBe("Chromium catches up");
    expect(f.browsers.chrome.status.text).toBe("Proposed");
    expect(f.browsers.ff.view.text).toBe("Shipped/Shipping");
    expect(f.stages.length).toBeGreaterThan(0);
    expect(f.stages.every(s => s.feature_id === f.id)).toBe(true);
    expect(f.standards.maturity.short_text).toBe("Working draft");
  });

  test("search: total count and rows", async () => {
    const { status, calls } = client();
    const { value } = await status.search({ q: "webrtc", num: 1 });
    expect(calls[0]).toBe("https://chromestatus.com/api/v0/features?q=webrtc&num=1");
    expect(value.total_count).toBeGreaterThan(0);
    expect(value.features).toHaveLength(1);
  });

  test("inMilestone: grouped by launch type", async () => {
    const { status } = client();
    const { value } = await status.inMilestone(158);
    expect(Object.keys(value.features_by_type)).toContain("Enabled by default");
    expect(value.features_by_type["Enabled by default"]![0]).toHaveProperty("name");
  });

  test("gates of a feature", async () => {
    const { status } = client();
    const { value } = await status.gates(5191490363064320);
    expect(value.gates.length).toBeGreaterThan(0);
    expect(value.gates[0]).toHaveProperty("team_name");
  });

  test("the endpoint table covers every recorded Chrome Status URL", () => {
    const templates = STATUS_ENDPOINTS.map(e => new RegExp(`^${e.path.replace(/\{[^}]+\}/g, "[^/]+")}$`));
    for (const f of FIXTURES.filter(f => f.url.includes("chromestatus"))) {
      const path = new URL(f.url).pathname.replace("/api/v0", "");
      expect(templates.some(re => re.test(path))).toBe(true);
    }
  });
});
