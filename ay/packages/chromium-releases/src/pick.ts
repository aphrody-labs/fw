/** Pick the Chromium and CEF version to embed for a release channel, from live Chromium Dash and CEF build data. */
import type { Cache } from "./cache.ts";
import type { ChromiumDash, DashChannel, DashPlatform, Release } from "./dash.ts";
import { channel as findChannel } from "./channels.ts";
import { getJson, type FetchLike } from "./http.ts";

export const CEF_INDEX = "https://cef-builds.spotifycdn.com/index.json";
export const CEF_DOWNLOAD = "https://cef-builds.spotifycdn.com";

export type CefPlatform = "windows64" | "windows32" | "windowsarm64" | "macosx64" | "macosarm64" | "linux64" | "linux32" | "linuxarm" | "linuxarm64";

export interface CefBuild {
  cef_version: string;
  /** CEF's own channel: `stable` or `beta`. */
  channel: string;
  chromium_version: string;
  /** The `minimal` distribution (the one `ay-chromium` downloads), when the build has one. */
  minimal?: { name: string; sha1: string; size: number };
}

/** The slice of the CEF index the picker needs: the newest builds per platform. */
export type CefIndex = Partial<Record<CefPlatform, CefBuild[]>>;

const KEEP_PER_PLATFORM = 40;

interface RawCefIndex {
  [platform: string]: { versions: { cef_version: string; channel: string; chromium_version: string; files: { name: string; sha1: string; size: number; type: string }[] }[] };
}

const milestoneOf = (v: string) => Number(v.split(".")[0]);

/** Reduce the 10 MB upstream index to the newest builds of each platform. */
export function trimCefIndex(raw: RawCefIndex): CefIndex {
  const out: CefIndex = {};
  for (const [platform, { versions }] of Object.entries(raw)) {
    out[platform as CefPlatform] = versions
      .map(v => {
        const f = v.files.find(x => x.type === "minimal");
        return { cef_version: v.cef_version, channel: v.channel, chromium_version: v.chromium_version, minimal: f && { name: f.name, sha1: f.sha1, size: f.size } };
      })
      .sort((a, b) => milestoneOf(b.chromium_version) - milestoneOf(a.chromium_version) || b.cef_version.localeCompare(a.cef_version, undefined, { numeric: true }))
      .slice(0, KEEP_PER_PLATFORM);
  }
  return out;
}

/** Load the trimmed CEF index, through the sqlite cache when given (refreshed after `maxAgeMs`, default 6 hours). */
export async function loadCefIndex(options: { cache?: Cache; fetch?: FetchLike; maxAgeMs?: number } = {}): Promise<CefIndex> {
  const key = `${CEF_INDEX}#trimmed`;
  const hit = options.cache?.read<CefIndex>(key);
  if (hit && Date.now() - hit.fetchedAt < (options.maxAgeMs ?? 6 * 3_600_000)) return hit.value;
  try {
    const index = trimCefIndex(await getJson<RawCefIndex>(CEF_INDEX, { fetch: options.fetch, timeoutMs: 120_000 }));
    options.cache?.write(key, index);
    return index;
  } catch (e) {
    if (hit) return hit.value;
    throw e;
  }
}

/** The CEF platform id and Chromium Dash platform of the machine, or of `os`/`arch` as Bun reports them. */
export function hostPlatform(os: string = process.platform, arch: string = process.arch): { cef: CefPlatform; dash: DashPlatform } {
  const x64 = arch === "x64";
  if (os === "win32") return { cef: x64 ? "windows64" : arch === "arm64" ? "windowsarm64" : "windows32", dash: "Windows" };
  if (os === "darwin") return { cef: arch === "arm64" ? "macosarm64" : "macosx64", dash: "Mac" };
  return { cef: x64 ? "linux64" : arch === "arm64" ? "linuxarm64" : arch === "arm" ? "linuxarm" : "linux32", dash: "Linux" };
}

export interface Selection {
  channel: string;
  platform: { dash: DashPlatform; cef: CefPlatform };
  chromium: { version: string; milestone: number; time: number; hashes: Record<string, string> };
  /** The CEF build to embed, absent when the index has no build at or below the channel's milestone. */
  cef?: CefBuild & { downloadUrl?: string };
  /** `exact` when CEF ships the same Chromium version, `milestone` for the same major, `behind` when CEF trails the channel. */
  match: "exact" | "milestone" | "behind" | "none";
  /** Milestones CEF is behind the channel's Chromium (0 when level). */
  lag: number;
}

/** Choose the CEF build closest to a Chromium release: same version, else same milestone, else the newest older milestone. */
export function chooseCef(builds: CefBuild[], release: { version: string; milestone: number }): { build?: CefBuild; match: Selection["match"]; lag: number } {
  const exact = builds.find(b => b.chromium_version === release.version);
  if (exact) return { build: exact, match: "exact", lag: 0 };
  const byVersion = (a: CefBuild, b: CefBuild) => b.chromium_version.localeCompare(a.chromium_version, undefined, { numeric: true }) || b.cef_version.localeCompare(a.cef_version, undefined, { numeric: true });
  const newest = (list: CefBuild[]) => [...list].sort(byVersion)[0];
  const same = newest(builds.filter(b => milestoneOf(b.chromium_version) === release.milestone));
  if (same) return { build: same, match: "milestone", lag: 0 };
  const older = newest(builds.filter(b => milestoneOf(b.chromium_version) < release.milestone));
  if (!older) return { match: "none", lag: 0 };
  return { build: older, match: "behind", lag: release.milestone - milestoneOf(older.chromium_version) };
}

export interface PickOptions {
  /** `stable`, `extended`, `beta`, `dev` or `canary` (case-insensitive, Chromium Dash names accepted). Default `stable`. */
  channel?: string;
  /** CEF platform id; default the host's. */
  cefPlatform?: CefPlatform;
  /** Chromium Dash platform; default derived from `cefPlatform`. */
  platform?: DashPlatform;
}

const dashOfCef = (p: CefPlatform): DashPlatform => (p.startsWith("windows") ? "Windows" : p.startsWith("macos") ? "Mac" : "Linux");

/** The version of Chromium a channel ships on a platform, and the CEF build that embeds the closest Chromium. */
export async function pickChromium(dash: ChromiumDash, loadIndex: () => Promise<CefIndex>, options: PickOptions = {}): Promise<Selection> {
  const info = findChannel(options.channel ?? "stable");
  if (!info) throw new Error(`unknown channel ${options.channel}`);
  const cefPlatform = options.cefPlatform ?? hostPlatform().cef;
  const platform = options.platform ?? dashOfCef(cefPlatform);
  const [{ value: releases }, index] = await Promise.all([dash.releases({ channel: info.dash as DashChannel, platform, num: 1 }), loadIndex()]);
  const release = releases[0];
  if (!release) throw new Error(`no ${info.dash} release on ${platform}`);
  const { build, match, lag } = chooseCef(index[cefPlatform] ?? [], release);
  return {
    channel: info.id,
    platform: { dash: platform, cef: cefPlatform },
    chromium: { version: release.version, milestone: release.milestone, time: release.time, hashes: release.hashes },
    cef: build && { ...build, downloadUrl: build.minimal && `${CEF_DOWNLOAD}/${encodeURIComponent(build.minimal.name)}` },
    match,
    lag,
  };
}
