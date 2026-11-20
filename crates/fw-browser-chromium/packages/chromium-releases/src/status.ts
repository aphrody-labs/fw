/** Typed client for the public read API of Chrome Status (https://chromestatus.com/api/v0), the GoogleChrome/chromium-dashboard backend. */
import { query } from "./http.ts";
import type { Source } from "./cache.ts";

export const STATUS_ORIGIN = "https://chromestatus.com";

export interface ChannelSchedule {
  mstone: number;
  version: number;
  stable_date?: string;
  branch_point?: string;
  earliest_beta?: string;
  latest_beta?: string;
  final_beta?: string;
  early_stable?: string;
  late_stable_date?: string;
  [field: string]: unknown;
}

/** `/channels`: what is in Stable, Beta and Dev right now, with the schedule of each milestone. */
export type Channels = Record<string, ChannelSchedule>;

export interface Stage {
  id: number;
  feature_id: number;
  stage_type: number;
  display_name: string | null;
  intent_thread_url: string | null;
  announcement_url: string | null;
  origin_trial_id: string | null;
  desktop_first: number | null;
  desktop_last: number | null;
  android_first: number | null;
  android_last: number | null;
  ios_first: number | null;
  webview_first: number | null;
  rollout_milestone: number | null;
  [field: string]: unknown;
}

export interface BrowserStatus {
  text: string | null;
  val: number | null;
  milestone_str?: string;
}

export interface ViewSignal {
  view: { url: string | null; notes: string | null; text: string | null; val: number | null };
}

/** The feature entity as `/features/<id>` returns it; every enum already carries its display text next to the integer. */
export interface Feature {
  id: number;
  name: string;
  summary: string;
  category: string;
  category_int: number;
  feature_type: string;
  feature_type_int: number;
  intent_stage: string;
  intent_stage_int: number;
  blink_components: string[];
  star_count: number;
  created: { by: string; when: string };
  updated: { by: string; when: string };
  owner_emails: string[];
  editor_emails: string[];
  stages: Stage[];
  bug_url: string | null;
  launch_bug_url: string | null;
  spec_link: string | null;
  explainer_links: string[];
  motivation: string | null;
  wpt: boolean | null;
  wpt_descr: string | null;
  ff_views: number | null;
  safari_views: number | null;
  web_dev_views: number | null;
  browsers: {
    chrome: {
      bug: string | null;
      owners: string[];
      status: BrowserStatus;
      desktop: number | null;
      android: number | null;
      webview: number | null;
      ios: number | null;
      origintrial: boolean;
      [field: string]: unknown;
    };
    ff: ViewSignal;
    safari: ViewSignal;
    webdev: ViewSignal;
    other: ViewSignal;
  };
  standards: { spec: string | null; maturity: { text: string | null; short_text: string | null; val: number | null } };
  is_released: boolean;
  unlisted: boolean;
  deleted: boolean;
  [field: string]: unknown;
}

/** A row of `/features` and `/features?milestone=`: a feature trimmed to what the lists render. */
export interface FeatureSummary {
  id: number;
  name: string;
  summary: string;
  feature_type_int: number;
  blink_components: string[];
  browsers: Feature["browsers"];
  standards: Feature["standards"];
  milestone?: number;
  [field: string]: unknown;
}

export interface FeatureList {
  total_count: number;
  features: FeatureSummary[];
}

/** Roadmap rows: features of one milestone grouped by launch type (`Enabled by default`, `Origin trial`, ...). */
export interface MilestoneFeatures {
  total_count: number;
  features_by_type: Record<string, FeatureSummary[]>;
}

export interface Gate {
  id: number;
  feature_id: number;
  stage_id: number;
  gate_type: number;
  team_name: string;
  state: number;
  [field: string]: unknown;
}

export interface FeatureSearch {
  /** Search syntax of chromestatus.com, e.g. `webrtc`, `browser:chrome`, `milestone:158`. */
  q?: string;
  /** Sort spec, e.g. `-updated`, `name`, `-milestone`. */
  sort?: string;
  num?: number;
  start?: number;
  nameOnly?: boolean;
}

/**
 * The public GET endpoints of `/api/v0`, from `main.py` and the OpenAPI spec of GoogleChrome/chromium-dashboard and probed live on 2026-11-16.
 * Endpoints that answer 403 without a signed-in account are marked `auth`; the rest need no session.
 */
export const STATUS_ENDPOINTS = [
  { path: "/channels", params: "start, end (milestones)", returns: "Channels", auth: false },
  { path: "/features", params: "q, sort, num, start, name_only, milestone, releaseNotesMilestone", returns: "FeatureList | MilestoneFeatures", auth: false },
  { path: "/features/{id}", params: "", returns: "Feature", auth: false },
  { path: "/features/{id}/process", params: "", returns: "process definition and stages", auth: false },
  { path: "/features/{id}/progress", params: "", returns: "field completion map", auth: false },
  { path: "/features/{id}/gates", params: "", returns: "{gates: Gate[]}", auth: false },
  { path: "/features/{id}/votes", params: "", returns: "{votes}", auth: false },
  { path: "/features/stale", params: "", returns: "{stale_features}", auth: false },
  { path: "/blinkcomponents", params: "", returns: "component to owners map", auth: false },
  { path: "/origintrials", params: "", returns: "{origin_trials}", auth: false },
  { path: "/web_feature_ids", params: "", returns: "string[]", auth: false },
  { path: "/web_feature_infos", params: "", returns: "{data}", auth: false },
  { path: "/webdxfeatures", params: "", returns: "web-features usecounter map", auth: false },
  { path: "/releasenotes/{milestone}", params: "", returns: "{milestone, features}", auth: false },
  { path: "/spec_mentors", params: "", returns: "mentor rows", auth: false },
  { path: "/external_reviews/{group}", params: "", returns: "{reviews, link_previews}", auth: false },
  { path: "/currentuser/permissions", params: "", returns: "{user}", auth: false },
  { path: "/gates/pending", params: "", returns: "{gates}", auth: false },
  { path: "/review-latency", params: "", returns: "report", auth: true },
  { path: "/feature-latency", params: "", returns: "report", auth: true },
  { path: "/componentsusers", params: "", returns: "report", auth: true },
  { path: "/feature_links_summary", params: "", returns: "report", auth: true },
] as const;

/** SPA routes of Chrome Status (`main.py`). */
export const STATUS_PAGES = ["/roadmap", "/features", "/features/stale", "/newfeatures", "/feature/<id>", "/metrics", "/enterprise", "/reports/spec_mentors", "/admin/blink"] as const;

export class ChromeStatus {
  constructor(
    private readonly source: Source,
    private readonly origin = STATUS_ORIGIN,
    private readonly ttl = { channels: 15 * 60_000, features: 30 * 60_000, reference: 24 * 3_600_000 },
  ) {}

  url(path: string, params: Record<string, string | number | boolean | undefined> = {}): string {
    return `${this.origin}/api/v0${path}${query(params)}`;
  }

  channels(range?: { start: number; end: number }) {
    return this.source.get<Channels>(this.url("/channels", { ...range }), this.ttl.channels);
  }

  feature(id: number) {
    return this.source.get<Feature>(this.url(`/features/${id}`), this.ttl.features);
  }

  search(q: FeatureSearch = {}) {
    return this.source.get<FeatureList>(this.url("/features", { q: q.q, sort: q.sort, num: q.num, start: q.start, name_only: q.nameOnly }), this.ttl.features);
  }

  inMilestone(milestone: number) {
    return this.source.get<MilestoneFeatures>(this.url("/features", { milestone }), this.ttl.features);
  }

  gates(id: number) {
    return this.source.get<{ gates: Gate[] }>(this.url(`/features/${id}/gates`), this.ttl.features);
  }

  process(id: number) {
    return this.source.get<{ name: string; description: string; applicability: string; stages: unknown[] }>(this.url(`/features/${id}/process`), this.ttl.reference);
  }

  releaseNotes(milestone: number) {
    return this.source.get<{ milestone: number; features: unknown[] }>(this.url(`/releasenotes/${milestone}`), this.ttl.features);
  }

  blinkComponents() {
    return this.source.get<Record<string, unknown>>(this.url("/blinkcomponents"), this.ttl.reference);
  }

  originTrials() {
    return this.source.get<{ origin_trials: unknown[] }>(this.url("/origintrials"), this.ttl.features);
  }

  staleFeatures() {
    return this.source.get<{ stale_features: unknown[] }>(this.url("/features/stale"), this.ttl.features);
  }

  specMentors() {
    return this.source.get<unknown[]>(this.url("/spec_mentors"), this.ttl.reference);
  }
}
