# Chrome release channels, Chromium Dash and Chrome Status in Bun

Three parts, all pure Bun (`Bun.serve`, `bun:sqlite`, `bun:test`, no npm dependencies):

| Part | Path | What it is |
|---|---|---|
| `@aphrody/chromium-releases` | `packages/chromium-releases` | Typed client for Chromium Dash and the public read API of Chrome Status, the channel dataset, a sqlite read-through cache, the `chromium-releases` CLI and the CEF/Chromium version picker |
| `chromium-dash` | `apps/chromium-dash` | `Bun.serve` app: the pages of both dashboards, their JSON endpoints at the original paths, brand M3 tokens and Google Sans Flex |
| `pickVersion` | `packages/chromium/src/version.ts` | The picker exposed to the browser package: which Chromium and which CEF build to embed for a channel |

Sources, read on 2026-11-16: [chrome-release-channels](https://www.chromium.org/getting-involved/chrome-release-channels/) (and its [markdown source](https://chromium.googlesource.com/playground/chromium-org-site/+/b3964e3363a057a523a16d0add46a646a4263a20/chrome-release-channels.md)), [chromiumdash.appspot.com](https://chromiumdash.appspot.com/), [GoogleChrome/chromium-dashboard](https://github.com/GoogleChrome/chromium-dashboard) (fork `aphrody-labs/chromium-dashboard`, synced) and its live deployment chromestatus.com.

## Channels

`CHANNELS` is the typed dataset: Stable, Extended Stable, Beta, Dev, Canary, each with summary, cadence, lag, side-by-side install and where it exists. `parseChannelsPage` reads the live chromium.org page (cadence sentence, install links per desktop, Android and iOS, the guide text) and the tests compare the dataset with a recorded copy of it. The current page and the original 2013-era markdown differ (six weeks in the markdown, four on the page), and the live schedule differs again: `fetch_milestone_schedule` shows a **two-week** Stable cycle in 2026, so every view reports the measured cadence next to the page's claim (`measuredStableWeeks`). The Extended channel is not on the page; it comes from Chromium Dash (`channel=Extended`), where majors are every second milestone.

## Chromium Dash endpoints

Found in the page bundle (`/build/cd-app.min.js` and its 30 lazy chunks) and probed live. `GET` endpoints below need no session; the others answer `401 {"error":"User not authenticated"}` and are listed but not served.

| Endpoint | Parameters | Returns |
|---|---|---|
| `GET /fetch_releases` | `channel` (Stable, Extended, Beta, Dev, Canary, Canary_asan), `platform` (Windows, Win32, Mac, Linux, Android, iOS, Webview, FuchsiaWebEngine), `milestone`, `num` (100 works, 200 fails), `offset` | releases newest first: version, milestone, time, previous version, branch position, hashes of chromium/v8/skia/angle/dawn/devtools/pdfium/webrtc |
| `GET /fetch_extra_releases` | `platform` | extra Webview rows |
| `GET /fetch_version` | `version` (`N.0.N.N`) | hashes, milestone, V8 version |
| `GET /fetch_milestones` | `only_active`, `only_branched` | milestones with branches of each repository and owners |
| `GET /fetch_milestone_schedule` | `mstone` (number, `next`, `current`, `previous`), `offset`, `n` | dated schedule rows and owners |
| `GET /fetch_minibranches` | | minibranch per repository and milestone |
| `GET /fetch_commit` | `commit` (1-40 hex), `platform` | commit, first landed version, per-channel deployment, review link |
| `GET /fetch_commits` | `count`, `repo`, `branch` | latest commits |
| `GET /cros/fetch_serving_builds`, `/cros/download_serving_builds_csv` | `deviceCategory` | ChromeOS boards and recovery images |
| `POST /fetch_builds`, `/fetch_security_fixes`, `/fetch_stability_urls`, `/fetch_triggered_history`, `/fetch_issue_query_count`, `/fetch_issue_query_timestamp`, `/fetch_user_status`, `/_admin/*` | | sign-in required |

Pages of the original single-page app: `/home /releases /schedule /commits /branches /builds /cves /stability /trigger-builds`. Validation errors come back as a JSON string holding a pydantic error list (HTTP 400); the client turns them into `ApiError`.

## Chrome Status endpoints (`/api/v0`)

From `main.py`, `openapi/api.yaml` (which lacks the read endpoints) and live probes. Responses start with the XSSI guard `)]}'`, which the client strips and the app keeps.

Served without a session: `/channels` (`start`, `end`), `/features` (`q`, `sort`, `num`, `start`, `name_only`, `milestone`, `releaseNotesMilestone`), `/features/{id}`, `/features/{id}/process|progress|gates|votes`, `/features/stale`, `/blinkcomponents`, `/origintrials`, `/web_feature_ids`, `/web_feature_infos`, `/webdxfeatures`, `/releasenotes/{milestone}`, `/spec_mentors`, `/external_reviews/{group}`. Sign-in required (403): `/review-latency`, `/feature-latency`, `/componentsusers`, `/feature_links_summary`, and all writes. SPA routes: `/roadmap /features /feature/<id> /metrics /enterprise ...`.

## Use

```sh
bun packages/chromium-releases/src/cli.ts channels --platform Windows
bun packages/chromium-releases/src/cli.ts releases --channel Stable --platform Linux --num 5
bun packages/chromium-releases/src/cli.ts schedule --milestone next
bun packages/chromium-releases/src/cli.ts feature 5191490363064320
bun packages/chromium-releases/src/cli.ts pick --channel beta --platform linux64
bun packages/chromium-releases/src/cli.ts endpoints
PORT=8787 bun apps/chromium-dash/src/main.ts     # pages and JSON at http://127.0.0.1:8787
```

The cache is `AY_CHROMIUM_RELEASES_DB`, else `<user cache>/<cli from ay.toml>/chromium-releases.sqlite`. Lifetimes: releases and feature lists 15-30 minutes, schedules and branches 6 hours, commits 30 days; an unreachable upstream serves the stored copy, flagged `stale` (`x-cache-stale` header, a banner on pages).

```ts
import { pickVersion } from "@aphrody/chromium";
const sel = await pickVersion({ channel: "stable", cefPlatform: "linux64" });
// sel.chromium.version, sel.cef.cef_version, sel.cef.downloadUrl, sel.match ("exact" | "milestone" | "behind"), sel.lag
```

On 2026-11-16 Chrome Stable was 158 while the newest CEF Stable build was 156 and CEF Beta carried 158: the picker returns the closest CEF build and states the lag instead of hiding it.

## Tests and proof

- `bun test ./packages/chromium-releases ./apps/chromium-dash ./packages/chromium/test/version.test.ts`: 80 tests against fixtures recorded from the live services (`packages/chromium-releases/test/fixtures`, re-recorded with `bun packages/chromium-releases/test/record.ts`). `bun run check` runs them in CI.
- Pages, side by side with the originals, rendered in Chrome by agent-browser: `C:\data\shots\chromium-dash\orig-*.png` (chromiumdash.appspot.com and chromestatus.com) next to `ay-*.png` (this app): home, releases, schedule, branches, commits, channels, roadmap, features, feature, api, serving-builds.
- JSON: `/fetch_releases`, `/fetch_version`, `/fetch_milestone_schedule` of the app hash equal to the upstream bodies; `/api/v0/channels` keeps the guard.

## Not replicated

- The data pipelines: the app is a cache in front of the live services, it does not crawl git, Omaha or Buganizer itself, and has no datastore of its own for features.
- Pages needing sign-in or POST: builds, CVEs, stability, trigger-builds, the admin pages; Chrome Status editing (guide, stages, intents, gates voting, origin trial requests), stars, notifications.
- Chrome Status stats pages (`/metrics/*` timelines), enterprise release notes, Gantt view, spec mentors and external review report pages, Blink component admin. Their endpoints (`/webdxfeatures`, `/releasenotes`, `/spec_mentors`, `/external_reviews`) are in the client.
- The original look (Polymer app bar, Lit components): the pages use the brand M3 tokens and Google Sans Flex instead; "Show changes" between releases, per-platform "Load more" by script (links instead) and the commit range tools are not built.
