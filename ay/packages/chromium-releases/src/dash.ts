/** Typed client for Chromium Dash (https://chromiumdash.appspot.com): releases, milestones, schedule, commits and ChromeOS builds. */
import { query } from "./http.ts";
import type { Source } from "./cache.ts";

export const DASH_ORIGIN = "https://chromiumdash.appspot.com";

/** Values the `/fetch_releases` validator accepts (read from its error messages and the data on 2026-11-16). */
export type DashChannel = "Stable" | "Beta" | "Dev" | "Canary" | "Canary_asan" | "Extended";
export type DashPlatform = "Windows" | "Win32" | "Mac" | "Linux" | "Android" | "iOS" | "Webview" | "FuchsiaWebEngine";

export const DASH_CHANNELS: readonly DashChannel[] = ["Stable", "Extended", "Beta", "Dev", "Canary", "Canary_asan"];
export const DASH_PLATFORMS: readonly DashPlatform[] = ["Windows", "Win32", "Mac", "Linux", "Android", "iOS", "Webview", "FuchsiaWebEngine"];

export interface Release {
  channel: DashChannel;
  chromium_main_branch_position: number | null;
  /** Git hashes of chromium, v8, skia, angle, dawn, devtools, pdfium and webrtc at this version. */
  hashes: Record<string, string>;
  milestone: number;
  platform: DashPlatform;
  previous_version: string | null;
  /** Epoch milliseconds of the release. */
  time: number;
  version: string;
}

export interface VersionInfo {
  chromium_main_branch_position: number | null;
  hashes: Record<string, string>;
  milestone: number;
  v8_main_branch_position: number | null;
  v8_version: string;
}

export interface Milestone {
  milestone: number;
  schedule_active: boolean;
  schedule_phase: string | null;
  merge_phase: string | null;
  chromium_branch: string | null;
  chromium_main_branch_hash: string | null;
  chromium_main_branch_position: number | null;
  angle_branch: string | null;
  dawn_branch: string | null;
  devtools_branch: string | null;
  pdfium_branch: string | null;
  skia_branch: string | null;
  skia_commit: string | null;
  v8_branch: string | null;
  webrtc_branch: string | null;
  cros_owner: string | null;
  cros_ldap: string | null;
  desktop_owner_emea: string | null;
  desktop_owner_us: string | null;
  mobile_owner_emea: string | null;
  mobile_owner_us: string | null;
  [field: string]: unknown;
}

/** One row of the milestone schedule; every date is `YYYY-MM-DDT00:00:00` and some are absent on older milestones. */
export interface ScheduleMilestone {
  mstone: number;
  feature_freeze?: string;
  branch_point?: string;
  earliest_beta?: string;
  earliest_beta_chromeos?: string;
  earliest_beta_ios?: string;
  latest_beta?: string;
  final_beta?: string;
  final_beta_cut?: string;
  early_stable?: string;
  early_stable_cut?: string;
  early_stable_ios?: string;
  early_stable_cut_ios?: string;
  stable_date?: string;
  stable_cut?: string;
  stable_cut_ios?: string;
  late_stable_date?: string;
  stable_refresh_first?: string;
  ldaps: Record<string, string | null>;
  owners: Record<string, string | null>;
  [field: string]: unknown;
}

export interface Schedule {
  mstones: ScheduleMilestone[];
}

export interface Minibranch {
  milestone: number;
  chromium_minibranch: string;
  expire_time: string | null;
  [repo: string]: string | number | null;
}

export interface CommitSummary {
  git_hash: string;
  repo: string;
  branch: string;
  commit_type: string;
  time: number;
  title: string;
  earliest: string | null;
  deployment: Record<string, string | null>;
}

export interface Commit extends CommitSummary {
  author: string;
  message: string;
  position: number | null;
  first_landed: string | null;
  bug_info: unknown[];
  relations: unknown[];
  review_info: { change_id: string; code_review_url: string; host: string } | null;
  source_tree: string;
}

export interface CrosBuilds {
  builds: Record<string, { brandNames?: string[]; fsiMilestoneNumber?: string; isAue?: boolean; isExperimental?: boolean; pushRecoveries?: Record<string, string>; [field: string]: unknown }>;
  [field: string]: unknown;
}

export interface ReleasesQuery {
  channel?: DashChannel;
  platform?: DashPlatform;
  milestone?: number;
  /** Rows to return; the service rejects very large values (200 and 500 failed on 2026-11-16, 100 works). */
  num?: number;
  offset?: number;
}

/**
 * Every endpoint Chromium Dash serves, discovered from its page bundle (`/build/cd-app.min.js` and the lazy chunks)
 * and probed live on 2026-11-16. `auth` endpoints answer `401 {"error":"User not authenticated"}` without a Google session.
 */
export const DASH_ENDPOINTS = [
  { path: "/fetch_releases", method: "GET", params: "channel, platform, milestone, num, offset", used: "releases page, home", returns: "Release[]", auth: false },
  { path: "/fetch_extra_releases", method: "GET", params: "platform", used: "releases page (Webview extra rows)", returns: "Release[]", auth: false },
  { path: "/fetch_version", method: "GET", params: "version (N.0.N.N)", used: "version lookup", returns: "VersionInfo", auth: false },
  { path: "/fetch_milestones", method: "GET", params: "only_active, only_branched", used: "branches page, home", returns: "Milestone[]", auth: false },
  { path: "/fetch_milestone_schedule", method: "GET", params: "mstone (number|next|current|previous), offset, n", used: "schedule page", returns: "{mstones: ScheduleMilestone[]}", auth: false },
  { path: "/fetch_minibranches", method: "GET", params: "", used: "branches page", returns: "Minibranch[]", auth: false },
  { path: "/fetch_commit", method: "GET", params: "commit (hex, 1-40), platform", used: "commit lookup", returns: "Commit", auth: false },
  { path: "/fetch_commits", method: "GET", params: "count, repo, branch, platform, ...", used: "commits page", returns: "{commits: CommitSummary[]}", auth: false },
  { path: "/cros/fetch_serving_builds", method: "GET", params: "", used: "ChromeOS serving builds", returns: "CrosBuilds", auth: false },
  { path: "/cros/download_serving_builds_csv", method: "GET", params: "deviceCategory", used: "ChromeOS CSV export", returns: "text/csv", auth: false },
  { path: "/fetch_builds", method: "POST", params: "platform_group, milestone", used: "builds page", returns: "Build[]", auth: true },
  { path: "/fetch_security_fixes", method: "POST", params: "", used: "cves page", returns: "SecurityFix[]", auth: true },
  { path: "/fetch_stability_urls", method: "POST", params: "", used: "stability page", returns: "StabilityUrl[]", auth: true },
  { path: "/fetch_triggered_history", method: "POST", params: "", used: "trigger-builds page", returns: "TriggeredBuild[]", auth: true },
  { path: "/fetch_issue_query_count", method: "POST", params: "", used: "issue counters", returns: "count", auth: true },
  { path: "/fetch_issue_query_timestamp", method: "POST", params: "", used: "issue counters", returns: "timestamp", auth: true },
  { path: "/fetch_user_status", method: "POST", params: "", used: "sign-in state", returns: "{identity}", auth: true },
  { path: "/_admin/fetch_milestone_for_update", method: "POST", params: "", used: "admin", returns: "", auth: true },
  { path: "/_admin/fetch_minibranches_for_update", method: "POST", params: "", used: "admin", returns: "", auth: true },
  { path: "/_admin/fetch_security_fixes_for_update", method: "POST", params: "", used: "admin", returns: "", auth: true },
  { path: "/_admin/fetch_reporters", method: "POST", params: "", used: "admin", returns: "", auth: true },
] as const;

/** Pages of the Chromium Dash single-page app (lazy webpack chunks). */
export const DASH_PAGES = ["/home", "/releases", "/schedule", "/commits", "/branches", "/builds", "/cves", "/stability", "/trigger-builds"] as const;

export class ChromiumDash {
  constructor(
    private readonly source: Source,
    private readonly origin = DASH_ORIGIN,
    /** Cache lifetimes in milliseconds. Releases move hourly, schedules daily. */
    private readonly ttl = { releases: 15 * 60_000, schedule: 6 * 3_600_000, commit: 30 * 24 * 3_600_000 },
  ) {}

  url(path: string, params: Record<string, string | number | boolean | undefined> = {}): string {
    return `${this.origin}${path}${query(params)}`;
  }

  releases(q: ReleasesQuery = {}, maxAgeMs = this.ttl.releases) {
    return this.source.get<Release[]>(this.url("/fetch_releases", { ...q }), maxAgeMs);
  }

  extraReleases(platform: DashPlatform, maxAgeMs = this.ttl.releases) {
    return this.source.get<Release[]>(this.url("/fetch_extra_releases", { platform }), maxAgeMs);
  }

  version(version: string) {
    return this.source.get<VersionInfo>(this.url("/fetch_version", { version }), this.ttl.commit);
  }

  milestones(q: { onlyActive?: boolean; onlyBranched?: boolean } = {}) {
    return this.source.get<Milestone[]>(this.url("/fetch_milestones", { only_active: q.onlyActive, only_branched: q.onlyBranched }), this.ttl.schedule);
  }

  schedule(q: { mstone?: number | "next" | "current" | "previous"; offset?: number; n?: number } = {}) {
    return this.source.get<Schedule>(this.url("/fetch_milestone_schedule", { ...q }), this.ttl.schedule);
  }

  minibranches() {
    return this.source.get<Minibranch[]>(this.url("/fetch_minibranches"), this.ttl.schedule);
  }

  commit(hash: string, platform?: DashPlatform) {
    return this.source.get<Commit>(this.url("/fetch_commit", { commit: hash, platform }), this.ttl.commit);
  }

  commits(q: { count?: number; repo?: string; branch?: string } = {}) {
    return this.source.get<{ commits: CommitSummary[] }>(this.url("/fetch_commits", { ...q }), this.ttl.releases);
  }

  crosServingBuilds() {
    return this.source.get<CrosBuilds>(this.url("/cros/fetch_serving_builds"), this.ttl.schedule);
  }
}

/** The newest release of each (channel, platform) among `releases`, which the API returns newest first. */
export function latestByChannel(releases: Release[]): Map<string, Release> {
  const out = new Map<string, Release>();
  for (const r of releases) {
    const key = `${r.channel}/${r.platform}`;
    const cur = out.get(key);
    if (!cur || r.time > cur.time) out.set(key, r);
  }
  return out;
}

/** The platforms present in live release data, so callers never rely on a hardcoded list. */
export function platformsOf(releases: Release[]): string[] {
  return [...new Set(releases.map(r => r.platform))].sort();
}

/** `YYYY-MM-DDT00:00:00` (UTC) to epoch milliseconds. */
export function scheduleTime(date: string | undefined): number | undefined {
  return date ? Date.parse(`${date}Z`) : undefined;
}

/** Whole days between consecutive `field` dates of the schedule, ascending by milestone. */
export function scheduleGapsDays(mstones: ScheduleMilestone[], field: keyof ScheduleMilestone = "stable_date"): number[] {
  const times = [...mstones].sort((a, b) => a.mstone - b.mstone).map(m => scheduleTime(m[field] as string | undefined));
  const gaps: number[] = [];
  for (let i = 1; i < times.length; i++) {
    const a = times[i - 1];
    const b = times[i];
    if (a !== undefined && b !== undefined) gaps.push(Math.round((b - a) / 86_400_000));
  }
  return gaps;
}
