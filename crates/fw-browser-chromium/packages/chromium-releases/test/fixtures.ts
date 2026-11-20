/** The recorded upstream documents the tests replay: name, URL and how `record.ts` trims the live body to a small real sample. */
import { readFileSync } from "node:fs";
import { join } from "node:path";

const DASH = "https://chromiumdash.appspot.com";
const STATUS = "https://chromestatus.com/api/v0";
export const COMMIT = "b8c2be851f62f6a394ec57955d7f31335d7cb6d4";

type Trim = (value: any) => unknown;
const first = (n: number): Trim => v => (Array.isArray(v) ? v.slice(0, n) : v);

export interface Fixture {
  name: string;
  url: string;
  ext?: "json" | "html";
  trim?: Trim;
}

export const FIXTURES: Fixture[] = [
  { name: "releases-stable-windows", url: `${DASH}/fetch_releases?channel=Stable&platform=Windows&num=2` },
  { name: "releases-stable-windows-1", url: `${DASH}/fetch_releases?channel=Stable&platform=Windows&num=1` },
  { name: "releases-extended-windows", url: `${DASH}/fetch_releases?channel=Extended&platform=Windows&num=1` },
  { name: "releases-beta-windows", url: `${DASH}/fetch_releases?channel=Beta&platform=Windows&num=1` },
  { name: "releases-dev-windows", url: `${DASH}/fetch_releases?channel=Dev&platform=Windows&num=1` },
  { name: "releases-canary-windows", url: `${DASH}/fetch_releases?channel=Canary&platform=Windows&num=1` },
  { name: "releases-invalid-channel", url: `${DASH}/fetch_releases?channel=Foo&num=1` },
  { name: "version", url: `${DASH}/fetch_version?version=158.0.8127.37` },
  { name: "commit", url: `${DASH}/fetch_commit?commit=${COMMIT}` },
  { name: "commits", url: `${DASH}/fetch_commits?count=3` },
  { name: "schedule", url: `${DASH}/fetch_milestone_schedule?offset=-3&n=6` },
  { name: "milestones", url: `${DASH}/fetch_milestones?only_active=true`, trim: first(4) },
  { name: "minibranches", url: `${DASH}/fetch_minibranches`, trim: first(3) },
  {
    name: "cros-serving-builds",
    url: `${DASH}/cros/fetch_serving_builds`,
    trim: v => ({ ...v, builds: Object.fromEntries(Object.entries<any>(v.builds).slice(0, 2)) }),
  },
  { name: "status-channels", url: `${STATUS}/channels` },
  { name: "status-channels-range", url: `${STATUS}/channels?start=157&end=158` },
  { name: "status-feature", url: `${STATUS}/features/5191490363064320` },
  { name: "status-search", url: `${STATUS}/features?q=webrtc&num=1` },
  {
    name: "status-milestone",
    url: `${STATUS}/features?milestone=158`,
    trim: v => ({ ...v, features_by_type: Object.fromEntries(Object.entries<any[]>(v.features_by_type).map(([k, rows]) => [k, rows.slice(0, 1)])) }),
  },
  { name: "status-gates", url: `${STATUS}/features/5191490363064320/gates`, trim: v => ({ gates: v.gates.slice(0, 2) }) },
  { name: "channels-page", url: "https://www.chromium.org/getting-involved/chrome-release-channels/", ext: "html" },
  {
    name: "cef-index",
    url: "https://cef-builds.spotifycdn.com/index.json",
    trim: v =>
      Object.fromEntries(
        ["windows64", "linux64"].map(p => [
          p,
          { versions: v[p].versions.slice(0, 14).map((x: any) => ({ ...x, files: x.files.filter((f: any) => f.type === "minimal") })) },
        ]),
      ),
  },
];

export const fixturePath = (f: Fixture) => join(import.meta.dir, "fixtures", `${f.name}.${f.ext ?? "json"}`);

/** A fetch that answers from the recorded fixtures and fails loudly for any other URL. */
export function replayFetch(extra: Record<string, string> = {}) {
  const calls: string[] = [];
  const bodies = new Map<string, string>([...FIXTURES.map(f => [f.url, readFileSync(fixturePath(f), "utf8")] as const), ...Object.entries(extra)]);
  const fetch = async (input: string): Promise<Response> => {
    calls.push(input);
    const body = bodies.get(input);
    if (body === undefined) return new Response(`no fixture for ${input}`, { status: 599 });
    return new Response(body, { status: 200, headers: { "content-type": input.endsWith(".html") || body.startsWith("<") ? "text/html" : "application/json" } });
  };
  return { fetch, calls };
}
