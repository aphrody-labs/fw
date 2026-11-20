#!/usr/bin/env bun
/** `chromium-releases`: the release channels, Chromium Dash and Chrome Status from the terminal. Every command prints JSON. */
import { parseArgs } from "node:util";
import { Cache, defaultDbPath } from "./cache.ts";
import { CHANNELS, fetchChannelsPage, measuredStableWeeks, withReleases } from "./channels.ts";
import { DASH_CHANNELS, DASH_ENDPOINTS, DASH_PLATFORMS, platformsOf, type DashChannel, type DashPlatform } from "./dash.ts";
import { createClients } from "./index.ts";
import { hostPlatform, loadCefIndex, pickChromium, type CefPlatform } from "./pick.ts";
import { STATUS_ENDPOINTS } from "./status.ts";

const USAGE = `usage: chromium-releases <command> [options]
  channels [--platform P]            the channels with their newest live release and the measured cadence
  releases [--channel C] [--platform P] [--milestone N] [--num N] [--offset N]
  schedule [--milestone N|next|current|previous] [--offset N] [--n N]
  milestones [--active] [--branched]
  version <N.0.N.N>                  hashes and V8 version of a Chrome version
  commit <hash>                      where a commit landed and which channels ship it
  commits [--repo R] [--count N]
  feature <id>                       one Chrome Status feature
  features [--q QUERY] [--sort S] [--num N] [--start N] [--milestone N]
  roadmap                            Chrome Status channels (stable, beta, dev) and their dates
  pick [--channel C] [--platform CEF_PLATFORM]   the Chromium and CEF build to embed
  refresh                            re-read the core documents into the cache
  endpoints                          every endpoint of both services
global: --no-cache (skip sqlite)  --db PATH (default ${defaultDbPath()})`;

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    channel: { type: "string" },
    platform: { type: "string" },
    milestone: { type: "string" },
    num: { type: "string" },
    offset: { type: "string" },
    n: { type: "string" },
    count: { type: "string" },
    repo: { type: "string" },
    q: { type: "string" },
    sort: { type: "string" },
    start: { type: "string" },
    active: { type: "boolean" },
    branched: { type: "boolean" },
    "no-cache": { type: "boolean" },
    db: { type: "string" },
    help: { type: "boolean", short: "h" },
  },
});

const int = (v: string | undefined) => (v === undefined ? undefined : Number(v));
const [command, arg] = positionals;
const print = (value: unknown) => console.log(JSON.stringify(value, null, 2));

async function main(): Promise<void> {
  if (!command || values.help) return void console.log(USAGE);
  if (command === "endpoints") return print({ chromiumdash: DASH_ENDPOINTS, chromestatus: STATUS_ENDPOINTS, channels: DASH_CHANNELS, platforms: DASH_PLATFORMS });
  const cache = values["no-cache"] ? false : new Cache(values.db);
  const { dash, status, cache: opened } = createClients({ cache });
  try {
    switch (command) {
      case "channels": {
        const platform = (values.platform ?? hostPlatform().dash) as DashPlatform;
        const [releases, schedule, page] = await Promise.all([
          Promise.all(CHANNELS.map(c => dash.releases({ channel: c.dash, platform, num: 1 }).then(r => r.value))).then(r => r.flat()),
          dash.schedule({ offset: -4, n: 12 }),
          fetchChannelsPage().catch(() => undefined),
        ]);
        return print({ platform, channels: withReleases(releases, platform), stableMajorWeeksMeasured: measuredStableWeeks(schedule.value.mstones), page });
      }
      case "releases":
        return print((await dash.releases({ channel: values.channel as DashChannel, platform: values.platform as DashPlatform, milestone: int(values.milestone), num: int(values.num), offset: int(values.offset) })).value);
      case "schedule": {
        const m = values.milestone;
        return print((await dash.schedule({ mstone: m && !/^\d+$/.test(m) ? (m as "next") : int(m), offset: int(values.offset), n: int(values.n) })).value);
      }
      case "milestones":
        return print((await dash.milestones({ onlyActive: values.active, onlyBranched: values.branched })).value);
      case "version":
        return print((await dash.version(need(arg, "version"))).value);
      case "commit":
        return print((await dash.commit(need(arg, "hash"))).value);
      case "commits":
        return print((await dash.commits({ repo: values.repo, count: int(values.count) })).value);
      case "feature":
        return print((await status.feature(Number(need(arg, "id")))).value);
      case "features": {
        const m = int(values.milestone);
        if (m) return print((await status.inMilestone(m)).value);
        return print((await status.search({ q: values.q, sort: values.sort, num: int(values.num), start: int(values.start) })).value);
      }
      case "roadmap":
        return print((await status.channels()).value);
      case "pick": {
        const cefPlatform = values.platform as CefPlatform | undefined;
        return print(await pickChromium(dash, () => loadCefIndex({ cache: opened }), { channel: values.channel, cefPlatform }));
      }
      case "refresh": {
        const platform = (values.platform ?? hostPlatform().dash) as DashPlatform;
        const fresh = 0;
        const jobs = [
          ...DASH_CHANNELS.map(channel => dash.releases({ channel, platform, num: 1 }, fresh)),
          dash.milestones(),
          dash.schedule({ offset: -4, n: 12 }),
          dash.minibranches(),
          status.channels(),
          loadCefIndex({ cache: opened, maxAgeMs: fresh }),
        ];
        await Promise.all(jobs);
        return print({ db: values.db ?? defaultDbPath(), refreshed: jobs.length, keys: opened?.keys().length ?? 0, platforms: platformsOf((await dash.releases({ milestone: (await dash.schedule({ mstone: "current" })).value.mstones[0]!.mstone, num: 100 })).value) });
      }
      default:
        console.error(USAGE);
        process.exitCode = 2;
    }
  } finally {
    opened?.close();
  }
}

function need(value: string | undefined, name: string): string {
  if (!value) throw new Error(`missing <${name}>`);
  return value;
}

main().catch(e => {
  console.error(e instanceof Error ? e.message : e);
  process.exit(1);
});
