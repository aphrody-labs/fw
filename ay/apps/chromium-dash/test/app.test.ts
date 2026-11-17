import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { Cache, createClients } from "@aphrody/ay-chromium-releases";
import { FIXTURES, COMMIT, fixturePath } from "../../../packages/chromium-releases/test/fixtures.ts";
import { createApp } from "../src/app.ts";
import { escape, html } from "../src/html.ts";
import { ago, dateOf, scheduleDay, statusDate } from "../src/format.ts";

const body = (name: string) => readFileSync(fixturePath(FIXTURES.find(f => f.name === name)!), "utf8");

/** Answers every upstream URL from the recorded fixtures by path, so the pages render real data offline. */
function upstream() {
  const calls: string[] = [];
  const fetch = async (input: string): Promise<Response> => {
    calls.push(input);
    const u = new URL(input);
    const p = u.pathname;
    const reply = (name: string, status = 200) => new Response(body(name), { status });
    if (p === "/fetch_releases") {
      const ch = u.searchParams.get("channel")?.toLowerCase();
      return ch && ["stable", "beta", "dev", "canary", "extended"].includes(ch) ? reply(`releases-${ch}-windows`) : reply("releases-invalid-channel", 400);
    }
    if (p === "/fetch_milestones") return reply("milestones");
    if (p === "/fetch_milestone_schedule") return reply("schedule");
    if (p === "/fetch_minibranches") return reply("minibranches");
    if (p === "/fetch_version") return reply("version");
    if (p === "/fetch_commit") return /^[0-9a-f]+$/i.test(u.searchParams.get("commit") ?? "") ? reply("commit") : reply("releases-invalid-channel", 400);
    if (p === "/fetch_commits") return reply("commits");
    if (p === "/cros/fetch_serving_builds") return reply("cros-serving-builds");
    if (p === "/api/v0/channels") return reply("status-channels");
    if (p === "/api/v0/features" && u.searchParams.has("milestone")) return reply("status-milestone");
    if (p === "/api/v0/features") return reply("status-search");
    if (/^\/api\/v0\/features\/\d+\/gates$/.test(p)) return reply("status-gates");
    if (/^\/api\/v0\/features\/5191490363064320$/.test(p)) return reply("status-feature");
    if (/^\/api\/v0\/features\/\d+$/.test(p)) return new Response("<h1>Not Found</h1>", { status: 404 });
    if (u.hostname === "www.chromium.org") return reply("channels-page");
    if (u.hostname === "cef-builds.spotifycdn.com") return reply("cef-index");
    return new Response(`no fixture for ${input}`, { status: 599 });
  };
  return { fetch, calls };
}

function app() {
  const { fetch, calls } = upstream();
  const clients = createClients({ cache: new Cache(":memory:"), fetch, retries: 0 });
  return { app: createApp({ clients, fetch: fetch as typeof globalThis.fetch }), calls };
}

async function get(a: ReturnType<typeof app>["app"], path: string) {
  const res = await a.fetch(new Request(`http://localhost${path}`));
  return { res, text: await res.text() };
}

describe("pages", () => {
  test("home: newest release per channel, schedule cards and milestone table", async () => {
    const { app: a } = app();
    const { res, text } = await get(a, "/");
    expect(res.status).toBe(200);
    expect(text).toContain("Chromium data, all in one place.");
    for (const ch of ["Stable", "Beta", "Dev", "Canary"]) expect(text).toContain(`data-channel="${ch}"`);
    expect(text).toContain("data-milestone=");
    expect(text).toContain("/assets/m3-tokens.css");
    expect(text).toContain("Google+Sans+Flex");
  });

  test("releases: one card per channel with versions and commit links", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/releases?platform=Windows");
    expect(text).toContain('class="card stable"');
    expect(text).toContain('class="card extended"');
    expect(text).toMatch(/\d+\.0\.\d+\.\d+/);
    expect(text).toContain("/commit/");
    expect(text).toContain('aria-current="page"');
  });

  test("releases for a single channel offer Load more only when the page is full", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/releases?channel=Stable&num=2");
    expect(text).toContain("Load more");
    expect(text).toContain("num=12");
    expect((await get(a, "/releases?channel=Stable&num=50")).text).not.toContain("Load more");
  });

  test("schedule: dated rows and paging links", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/schedule");
    expect(text).toContain("Stable Release");
    expect(text).toContain("Feature Freeze");
    expect(text).toMatch(/(Mon|Tue|Wed|Thu|Fri), [A-Z][a-z]{2} \d+, 20\d\d/);
    expect(text).toContain("PREV");
  });

  test("branches: milestone branches and minibranches", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/branches");
    expect(text).toContain("Minibranches");
    expect(text).toContain("ANGLE");
  });

  test("commit lookup shows where the commit first landed", async () => {
    const { app: a } = app();
    const { res, text } = await get(a, `/commit/${COMMIT}`);
    expect(res.status).toBe(200);
    expect(text).toContain("158.0.8127.37");
    expect(text).toContain("First landed in");
    expect((await get(a, `/commits?q=${COMMIT}`)).text).toContain("First landed in");
  });

  test("an invalid commit hash is explained and an unknown one is reported, both on the page", async () => {
    const { app: a } = app();
    expect((await get(a, "/commits?q=zzz")).text).toContain("1 to 40 hexadecimal digits");
    const none = await get(a, "/commit/00000");
    expect(none.res.status).toBe(200);
    expect(none.text).toContain("Recent commits");
  });

  test("serving builds lists ChromeOS boards", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/serving-builds");
    expect(text).toContain("ChromeOS boards");
  });

  test("channels page: the five channels, measured cadence against the page's claim, install links", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/channels");
    for (const id of ["stable", "extended", "beta", "dev", "canary"]) expect(text).toContain(`data-channel="${id}"`);
    expect(text).toContain("every <b>2 weeks</b>");
    expect(text).toContain("page still says 4");
    expect(text).toContain("https://google.com/chrome/canary");
    expect(text).toContain("How to switch channels");
  });

  test("roadmap: stable, next up and dev columns with features grouped by launch type", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/roadmap");
    expect(text).toContain("Stable");
    expect(text).toContain("Next up");
    expect(text).toContain("Enabled by default");
    expect(text).toContain('href="/feature/');
  });

  test("features search renders rows and keeps the query", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/features?q=webrtc");
    expect(text).toContain('value="webrtc"');
    expect(text).toContain("features, showing 1 to");
  });

  test("feature page: overview, specification, status and stages", async () => {
    const { app: a } = app();
    const { res, text } = await get(a, "/feature/5191490363064320");
    expect(res.status).toBe(200);
    for (const s of ["WebRTC Encoded Transform: generateKeyFrame", "Overview", "Motivation", "Specification", "Status in Chromium", "Proposed", "Shipped/Shipping", "Review gates"]) expect(text).toContain(s);
  });

  test("an unknown feature is a 404 page", async () => {
    const { app: a } = app();
    const { res, text } = await get(a, "/feature/1");
    expect(res.status).toBe(404);
    expect(text).toContain("Nothing at");
  });

  test("api page lists both services and marks sign-in endpoints", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/api");
    expect(text).toContain("/fetch_releases");
    expect(text).toContain("/fetch_builds");
    expect(text).toContain("needs sign-in");
  });

  test("assets: the brand tokens and icon are served from the repository", async () => {
    const { app: a } = app();
    const css = await get(a, "/assets/m3-tokens.css");
    expect(css.res.headers.get("content-type")).toContain("text/css");
    expect(css.text).toContain("--md-sys-color-primary");
    const icon = await get(a, "/assets/icon.svg");
    expect(icon.text).toContain("<svg");
  });

  test("an upstream outage with a warm cache serves the stale copy and says so", async () => {
    const { fetch } = upstream();
    let down = false;
    const flaky = async (input: string) => (down ? new Response("down", { status: 503 }) : fetch(input));
    const cache = new Cache(":memory:");
    const clients = createClients({ cache, fetch: flaky, retries: 0 });
    const a = createApp({ clients, fetch: flaky as typeof globalThis.fetch });
    expect((await get(a, "/releases")).text).not.toContain("unreachable");
    cache.db.run("UPDATE entries SET fetched_at = 1");
    down = true;
    const again = await get(a, "/releases");
    expect(again.res.status).toBe(200);
    expect(again.text).toContain("unreachable");
  });
});

describe("JSON APIs", () => {
  test("Chromium Dash endpoints are served at their original paths with the upstream body", async () => {
    const { app: a } = app();
    const { res, text } = await get(a, "/fetch_releases?channel=Stable&platform=Windows&num=1");
    expect(res.headers.get("content-type")).toContain("application/json");
    expect(JSON.parse(text)[0]).toMatchObject({ channel: "Stable", platform: "Windows" });
    expect(JSON.parse((await get(a, "/fetch_milestone_schedule?mstone=158")).text).mstones.length).toBeGreaterThan(0);
    expect(JSON.parse((await get(a, "/fetch_version?version=158.0.8127.37")).text).milestone).toBe(158);
  });

  test("a rejected parameter is a 400 with the upstream reason", async () => {
    const { app: a } = app();
    const { res, text } = await get(a, "/fetch_releases?channel=Foo");
    expect(res.status).toBe(400);
    expect(JSON.parse(text).error).toContain("Invalid channel");
  });

  test("sign-in endpoints are not served", async () => {
    const { app: a } = app();
    expect((await get(a, "/fetch_builds")).res.status).toBe(404);
  });

  test("Chrome Status keeps the XSSI guard on /api/v0 and refuses endpoints that need sign-in", async () => {
    const { app: a } = app();
    const { text } = await get(a, "/api/v0/channels");
    expect(text.startsWith(")]}'\n")).toBe(true);
    expect(JSON.parse(text.slice(5)).stable.mstone).toBeGreaterThan(0);
    const denied = await get(a, "/api/v0/review-latency");
    expect(denied.res.status).toBe(404);
  });

  test("/api/channels joins the dataset with live releases", async () => {
    const { app: a } = app();
    const j = JSON.parse((await get(a, "/api/channels?platform=Windows")).text);
    expect(j.channels.map((c: { id: string }) => c.id)).toEqual(["stable", "extended", "beta", "dev", "canary"]);
    expect(j.channels[0].release.version).toMatch(/^\d+\.0\./);
    expect(j.stableMajorWeeksMeasured).toBe(2);
  });

  test("/api/pick returns the Chromium version and CEF build for a channel", async () => {
    const { app: a } = app();
    const j = JSON.parse((await get(a, "/api/pick?channel=stable&platform=windows64")).text);
    expect(j.platform).toEqual({ dash: "Windows", cef: "windows64" });
    expect(j.cef.downloadUrl).toStartWith("https://cef-builds.spotifycdn.com/");
    expect(["exact", "milestone", "behind"]).toContain(j.match);
  });

  test("only GET and HEAD are accepted", async () => {
    const { app: a } = app();
    expect((await a.fetch(new Request("http://localhost/", { method: "POST" }))).status).toBe(405);
  });

  test("repeated requests are answered from sqlite, not upstream", async () => {
    const { app: a, calls } = app();
    await get(a, "/fetch_milestones");
    await get(a, "/fetch_milestones");
    expect(calls.filter(c => c.includes("/fetch_milestones"))).toHaveLength(1);
  });
});

describe("helpers", () => {
  test("html escapes values but not nested templates", () => {
    expect(html`<b>${"<i>&"}</b>${html`<u>x</u>`}${["a", html`<s/>`]}`.toString()).toBe("<b>&lt;i&gt;&amp;</b><u>x</u>a<s/>");
    expect(escape("\"'")).toBe("&quot;&#39;");
  });

  test("dates are UTC and relative days are day-granular", () => {
    expect(dateOf(Date.UTC(2026, 10, 12))).toBe("Nov 12, 2026");
    expect(scheduleDay("2026-11-17T00:00:00")).toBe("Tue, Nov 17, 2026");
    expect(statusDate("2026-11-16 14:23:24.535593")).toBe("Nov 16, 2026");
    const now = Date.UTC(2026, 10, 16, 12);
    expect(ago(Date.UTC(2026, 10, 4, 12), now)).toBe("12 days ago");
    expect(ago(Date.UTC(2026, 10, 15, 12), now)).toBe("yesterday");
    expect(ago(Date.UTC(2026, 10, 17, 12), now)).toBe("tomorrow");
    expect(ago(Date.UTC(2026, 10, 20, 12), now)).toBe("in 4 days");
  });
});
