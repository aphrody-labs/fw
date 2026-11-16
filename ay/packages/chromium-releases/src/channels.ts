/**
 * The Chrome release channels as a typed dataset, from https://www.chromium.org/getting-involved/chrome-release-channels/
 * (and the Chromium Dash `Extended` channel, which that page does not list). `parseChannelsPage` reads the same facts live,
 * and the tests compare the dataset against a recorded copy of the page so drift is caught.
 */
import type { DashChannel, Release, ScheduleMilestone } from "./dash.ts";
import { scheduleGapsDays } from "./dash.ts";

export const CHANNELS_PAGE = "https://www.chromium.org/getting-involved/chrome-release-channels/";
export const CHANNELS_DOC_SOURCE = "https://chromium.googlesource.com/playground/chromium-org-site/+/b3964e3363a057a523a16d0add46a646a4263a20/chrome-release-channels.md";

export type ChannelId = "canary" | "dev" | "beta" | "stable" | "extended";
export type PageSection = "desktop" | "android" | "ios";

export interface ChannelInfo {
  id: ChannelId;
  name: string;
  /** The `channel` value of Chromium Dash `/fetch_releases`. */
  dash: DashChannel;
  /** The key of the channel in Chrome Status `/channels`, when it has one. */
  status?: "stable" | "beta" | "dev";
  /** 0 is the most stable channel; larger is closer to the tip of tree. */
  stability: number;
  summary: string;
  /** How often a new build ships, in the page's words. */
  cadence: string;
  /** Typical lag before a change from the tip reaches the channel. */
  lag: string;
  /** Installs next to other channels with its own profile on desktop. */
  parallelInstall: boolean;
  /** Where the channel exists, by the page's sections. */
  availability: Record<PageSection, boolean>;
  /** Page heading the channel's own paragraph carries in "How do I choose which channel to use?". */
  pageLabel?: string;
}

export const CHANNELS: readonly ChannelInfo[] = [
  {
    id: "stable",
    name: "Stable",
    dash: "Stable",
    status: "stable",
    stability: 0,
    summary: "Fully tested by the Chrome test team; the best bet to avoid crashes and other issues.",
    cadence: "A minor release every week, a major release every four weeks",
    lag: "Features spend about a month in Beta before Stable",
    parallelInstall: true,
    availability: { desktop: true, android: true, ios: true },
    pageLabel: "Stable channel",
  },
  {
    id: "extended",
    name: "Extended Stable",
    dash: "Extended",
    stability: 0,
    summary: "Stable for enterprises that need a slower cycle: a major release every second Stable milestone with security fixes in between.",
    cadence: "A major release every eight weeks (every second Stable milestone), weekly security updates",
    lag: "One Stable milestone behind Stable between majors",
    parallelInstall: false,
    availability: { desktop: true, android: false, ios: false },
  },
  {
    id: "beta",
    name: "Beta",
    dash: "Beta",
    status: "beta",
    stability: 1,
    summary: "What is next with minimal risk; new features spend about a month in Beta before they reach Stable.",
    cadence: "Updated about weekly, a new major every four weeks",
    lag: "About a month before Stable",
    parallelInstall: true,
    availability: { desktop: true, android: true, ios: true },
    pageLabel: "Beta channel",
  },
  {
    id: "dev",
    name: "Dev",
    dash: "Dev",
    status: "dev",
    stability: 2,
    summary: "What the team is working on right now; tested, but still subject to bugs.",
    cadence: "Updated once or twice weekly",
    lag: "No lag between major versions",
    parallelInstall: true,
    availability: { desktop: true, android: true, ios: false },
    pageLabel: "Dev channel",
  },
  {
    id: "canary",
    name: "Canary",
    dash: "Canary",
    stability: 3,
    summary: "Changes as soon as they are made, with only automated testing; runs next to any other channel with its own profile.",
    cadence: "Released twice daily",
    lag: "None: built from the tip of tree",
    parallelInstall: true,
    availability: { desktop: true, android: true, ios: false },
    pageLabel: "Canary build",
  },
];

export function channel(id: string): ChannelInfo | undefined {
  const key = id.toLowerCase().replace(/[\s_-]+stable$/, "").replace("extended stable", "extended");
  return CHANNELS.find(c => c.id === key || c.dash.toLowerCase() === key || c.name.toLowerCase() === key);
}

export interface ChannelsPage {
  /** The first paragraph: how channels roll out, with the Stable major cadence. */
  intro: string;
  /** Major release cadence of Stable in weeks, read from "every N weeks" in the intro. */
  stableMajorWeeks?: number;
  /** Install links by section (`desktop`, `android`, `ios`) and channel id. */
  downloads: Record<PageSection, Partial<Record<ChannelId, string>>>;
  /** Paragraph per channel from "How do I choose which channel to use?". */
  guide: Partial<Record<ChannelId, string>>;
  /** The notes after the guide: other builds, translations. */
  notes: string[];
}

const WORDS: Record<string, number> = { one: 1, two: 2, three: 3, four: 4, five: 5, six: 6, eight: 8 };
const decode = (s: string) =>
  s
    .replace(/<[^>]*>/g, "")
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&gt;/g, ">")
    .replace(/&lt;/g, "<")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();

/** Parse the live chrome-release-channels page (HTML rendered from the chromium/website repository). */
export function parseChannelsPage(html: string): ChannelsPage {
  const main = html.slice(html.indexOf("<main"));
  const intro = decode(/<\/nav>\s*<p>([\s\S]*?)<\/p>/.exec(main)?.[1] ?? "");
  const weeks = /every\s+(\w+)\s+weeks/.exec(intro)?.[1];
  const stableMajorWeeks = weeks ? (WORDS[weeks.toLowerCase()] ?? (Number(weeks) || undefined)) : undefined;

  const downloads: ChannelsPage["downloads"] = { desktop: {}, android: {}, ios: {} };
  const sections: [PageSection, RegExp][] = [
    ["desktop", /Windows, macOS, and Linux/],
    ["android", /^Android$/],
    ["ios", /^iOS$/],
  ];
  for (const m of main.matchAll(/<h4[^>]*>([\s\S]*?)<\/h4>([\s\S]*?)(?=<h[1-4]|$)/g)) {
    const title = decode(m[1]!);
    const section = sections.find(([, re]) => re.test(title))?.[0];
    if (!section) continue;
    for (const li of m[2]!.matchAll(/<li><a href="([^"]+)">([^<]+)<\/a><\/li>/g)) {
      const id = channel(decode(li[2]!))?.id;
      if (id) downloads[section][id] = decode(li[1]!);
    }
  }

  const guide: ChannelsPage["guide"] = {};
  const choose = /<h3[^>]*id="how-do-i-choose[^"]*"[\s\S]*?(?=<h3)/.exec(main)?.[0] ?? "";
  const notes: string[] = [];
  for (const li of choose.matchAll(/<li><strong>([^<]+?):?<\/strong>([\s\S]*?)<\/li>/g)) {
    const label = decode(li[1]!).replace(/:$/, "");
    const info = CHANNELS.find(c => c.pageLabel === label);
    const text = decode(li[2]!);
    if (info) guide[info.id] = text;
    else notes.push(`${label}: ${text}`);
  }
  const note = /<p><strong>Note<\/strong>\s*:?([\s\S]*?)<\/p>/.exec(choose)?.[1];
  if (note) notes.push(decode(note).replace(/^:\s*/, ""));
  return { intro, stableMajorWeeks, downloads, guide, notes };
}

export async function fetchChannelsPage(doFetch: typeof fetch = fetch): Promise<ChannelsPage> {
  const res = await doFetch(CHANNELS_PAGE, { signal: AbortSignal.timeout(30_000) });
  if (!res.ok) throw new Error(`${CHANNELS_PAGE}: ${res.status}`);
  return parseChannelsPage(await res.text());
}

export interface ChannelStatus extends ChannelInfo {
  /** Newest release of the channel on the platform, from Chromium Dash. */
  release?: Pick<Release, "version" | "milestone" | "platform" | "time" | "previous_version">;
}

/** The dataset joined with the newest live release of each channel on one platform. */
export function withReleases(releases: Release[], platform: string): ChannelStatus[] {
  return CHANNELS.map(c => {
    const r = releases.filter(x => x.channel === c.dash && x.platform === platform).sort((a, b) => b.time - a.time)[0];
    return { ...c, release: r && { version: r.version, milestone: r.milestone, platform: r.platform, time: r.time, previous_version: r.previous_version } };
  });
}

/** Stable major cadence in weeks measured on the live schedule: the median gap between consecutive `stable_date`s. */
export function measuredStableWeeks(mstones: ScheduleMilestone[]): number | undefined {
  const gaps = scheduleGapsDays(mstones).sort((a, b) => a - b);
  if (!gaps.length) return undefined;
  return Math.round(gaps[Math.floor(gaps.length / 2)]! / 7);
}
