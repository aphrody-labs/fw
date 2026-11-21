#!/usr/bin/env bun
// The release train of the stack: one milestone number for every product (Chromium), a new milestone every
// [schedule].cadence_weeks, a release branch cut from main at its branch point, a beta period on that branch (the kernel's
// -rcN phase), then stable patch releases; every [schedule].extended_every-th milestone stays supported as extended
// stable (Chromium), and the last milestone to reach stable in a calendar year is longterm (kernel.org). The rules and the
// channel table are release.toml [schedule] and [channels]; docs/release/model.md explains them.
//   bun tools/release-plan.ts plan [--date YYYY-MM-DD]       milestones and what each channel ships on that date
//   bun tools/release-plan.ts meta                           version, channel and tag of the current CI run (GITHUB_* env)
//   bun tools/release-plan.ts version <channel> <milestone> <build> [<patch-or-n>]
//   bun tools/release-plan.ts upstream                       live kernel.org, Chromium, Zed and gpui channels
import { join } from "node:path";

type Schedule = { cadence_weeks: number; anchor_milestone: number; anchor_stable: string; beta_weeks: number; extended_every: number; extended_weeks: number; lts_weeks: number };
type ChannelRow = { from: string; kernel: string; chromium: string; zed: string; prerelease?: string };
type Config = { schedule: Schedule; channels: { order: string[]; aliases: Record<string, string> } & Record<string, ChannelRow | unknown> };

const configPath = join(import.meta.dir, "..", "release.toml");
export const config = Bun.TOML.parse(await Bun.file(configPath).text()) as unknown as Config;
export const CHANNELS = config.channels.order;
const DAY = 86_400_000;
const WEEK = 7 * DAY;
const utc = (date: string) => Date.parse(`${date}T00:00:00Z`);
const iso = (ms: number) => new Date(ms).toISOString().slice(0, 10);

export function channelName(name: string): string {
  const resolved = config.channels.aliases[name] ?? name;
  if (!CHANNELS.includes(resolved)) throw new Error(`unknown channel ${name}; release.toml [channels].order is ${CHANNELS.join(", ")}`);
  return resolved;
}

export type Milestone = { milestone: number; branch: string; stable: string; extendedUntil: string | null; ltsUntil: string | null; next: string };

export function milestone(m: number, s: Schedule = config.schedule): Milestone {
  const stable = utc(s.anchor_stable) + (m - s.anchor_milestone) * s.cadence_weeks * WEEK;
  const next = stable + s.cadence_weeks * WEEK;
  const extended = m % s.extended_every === 0 ? stable + s.extended_weeks * WEEK : null;
  // Longterm: no later milestone reaches stable in the same calendar year.
  const lts = new Date(next).getUTCFullYear() > new Date(stable).getUTCFullYear() ? stable + s.lts_weeks * WEEK : null;
  return { milestone: m, branch: iso(stable - s.beta_weeks * WEEK), stable: iso(stable), extendedUntil: extended === null ? null : iso(extended), ltsUntil: lts === null ? null : iso(lts), next: iso(next) };
}

/** The milestone whose stable window contains the date (stable <= date < next stable). */
export function stableMilestoneOn(date: string, s: Schedule = config.schedule): number {
  return s.anchor_milestone + Math.floor((utc(date) - utc(s.anchor_stable)) / (s.cadence_weeks * WEEK));
}

/** What each channel ships on a date: main carries the milestone after the newest branch point. */
export function plan(date: string, s: Schedule = config.schedule) {
  const stable = stableMilestoneOn(date, s);
  const beta = utc(milestone(stable + 1, s).branch) <= utc(date) ? stable + 1 : null;
  const main = (beta ?? stable) + 1;
  const extended: number[] = [];
  const lts: number[] = [];
  for (let m = stable; m >= s.anchor_milestone; m--) {
    const row = milestone(m, s);
    if (m < stable && row.extendedUntil && utc(row.extendedUntil) > utc(date)) extended.push(m);
    if (row.ltsUntil && utc(row.ltsUntil) > utc(date)) lts.push(m);
  }
  return { date, channels: { canary: main, dev: main, beta, stable, extended, lts }, milestones: [main, ...(beta ? [beta] : []), stable].map((m) => milestone(m, s)) };
}

/**
 * Semver of a build. main (canary, dev): M.0.0-<channel>.<build>, below every release of M. Release branch: beta is
 * M.B.0-beta.<n> (n commits since the cut, the kernel's rcN), stable/extended/lts are M.B.P with P the patch count on the branch.
 */
export function version(channel: string, m: number, build: number, patchOrN = 0): string {
  const resolved = channelName(channel);
  if (resolved === "canary" || resolved === "dev") return `${m}.0.0-${resolved}.${build}`;
  if (resolved === "beta") return `${m}.${build}.0-beta.${patchOrN}`;
  return `${m}.${build}.${patchOrN}`;
}

/** Parse a tag back: vM.B.P (stable, extended or lts by the plan), vM.B.0-beta.N, vM.0.0-canary.B, vM.0.0-dev.B. */
export function parseTag(tag: string, date: string) {
  const match = tag.match(/^v(\d+)\.(\d+)\.(\d+)(?:-([a-z]+)\.(\d+))?$/);
  if (!match) throw new Error(`tag ${tag} is not vM.B.P or vM.B.P-<channel>.<n> (docs/release/model.md)`);
  const [, m, b, p, pre, n] = match;
  const versionText = tag.slice(1);
  if (pre) return { version: versionText, channel: channelName(pre), milestone: Number(m), build: Number(b), patch: Number(p), n: Number(n) };
  // A release without a prerelease suffix belongs to the newest channel that still supports its milestone on that date.
  const { channels } = plan(date);
  const mNumber = Number(m);
  const channel = channels.lts.includes(mNumber) && mNumber !== channels.stable ? "lts" : channels.extended.includes(mNumber) ? "extended" : "stable";
  return { version: versionText, channel, milestone: mNumber, build: Number(b), patch: Number(p), n: 0 };
}

async function upstream() {
  const json = async (url: string, headers: Record<string, string> = {}) => {
    const response = await fetch(url, { headers: { "user-agent": "ay-release-plan (github.com/aphrody-labs/ay)", ...headers } });
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    return response.json() as Promise<any>;
  };
  const kernel = (await json("https://www.kernel.org/releases.json")).releases.map((r: any) => ({ moniker: r.moniker, version: r.version, eol: r.iseol, released: r.released.isodate }));
  const chromium: Record<string, { version: string; milestone: number }> = {};
  for (const channel of ["Canary", "Dev", "Beta", "Stable", "Extended"]) {
    const [row] = await json(`https://chromiumdash.appspot.com/fetch_releases?platform=Windows&num=1&channel=${channel}`);
    chromium[channel] = { version: row.version, milestone: row.milestone };
  }
  const schedule = (await json("https://chromiumdash.appspot.com/fetch_milestone_schedule?offset=0&n=3")).mstones.map((m: any) => ({ milestone: m.mstone, branch: m.branch_point?.slice(0, 10), stable: m.stable_date?.slice(0, 10), ltc: m.ltc_date?.slice(0, 10) ?? null }));
  const token = Bun.env.GH_TOKEN ?? Bun.env.GITHUB_TOKEN;
  const zedReleases = (await json("https://api.github.com/repos/zed-industries/zed/releases?per_page=20", token ? { authorization: `Bearer ${token}` } : {})) as any[];
  const zed = { stable: zedReleases.find((r) => !r.prerelease)?.tag_name, preview: zedReleases.find((r) => r.prerelease)?.tag_name };
  const gpui = (await json("https://crates.io/api/v1/crates/gpui")).crate.max_version;
  return { read: new Date().toISOString(), kernel, chromium, chromiumSchedule: schedule, zed, gpui };
}

if (import.meta.main) {
  const [command, ...rest] = Bun.argv.slice(2);
  const flag = (name: string) => { const i = rest.indexOf(name); return i >= 0 ? rest[i + 1] : undefined; };
  const today = flag("--date") ?? iso(Date.now());
  if (command === "plan") console.log(JSON.stringify(plan(today), null, 1));
  else if (command === "version") console.log(version(rest[0]!, Number(rest[1]), Number(rest[2]), Number(rest[3] ?? 0)));
  else if (command === "upstream") console.log(JSON.stringify(await upstream(), null, 1));
  else if (command === "meta") {
    // Tags carry their version; a run on a branch or a dispatch builds main (canary, dev) or a release branch (beta).
    const env = Bun.env;
    const sh = (args: string[]) => Bun.spawnSync(["git", ...args]).stdout.toString().trim();
    let out: { version: string; channel: string; tag: string };
    if (env.GITHUB_REF_TYPE === "tag") {
      const tag = env.GITHUB_REF_NAME!;
      const parsed = parseTag(tag, today);
      out = { version: parsed.version, channel: parsed.channel, tag };
    } else {
      const ref = env.GITHUB_REF_NAME ?? sh(["rev-parse", "--abbrev-ref", "HEAD"]);
      const branch = ref.match(/^release\/(\d+)$/);
      const channel = channelName(env.INPUT_CHANNEL || (branch ? "beta" : "canary"));
      const build = Number(sh(["rev-list", "--count", "HEAD"]));
      if (branch) {
        const m = Number(branch[1]);
        // B is main's commit count at the cut (the merge base), n the commits on the branch since; needs fetch-depth 0.
        const base = Number(sh(["rev-list", "--count", sh(["merge-base", "HEAD", "origin/main"])]));
        const v = version(channel, m, base, build - base);
        out = { version: v, channel, tag: `v${v}` };
      } else {
        const v = version(channel, plan(today).channels.canary, build);
        out = { version: v, channel, tag: `v${v}` };
      }
    }
    const lines = Object.entries(out).map(([k, v]) => `${k}=${v}`).join("\n");
    if (env.GITHUB_OUTPUT) await Bun.write(env.GITHUB_OUTPUT, `${await Bun.file(env.GITHUB_OUTPUT).text().catch(() => "")}${lines}\n`);
    console.log(JSON.stringify(out));
  } else {
    console.error("usage: bun tools/release-plan.ts plan|meta|version|upstream (see the header)");
    process.exit(2);
  }
}
