# anomalyco (formerly sst): opencode and the org replica

Read live on 2026-11-16 and 2026-11-17 with `packages/org-mirror` (GitHub GraphQL and REST, `git clone --mirror`, npm, crates.io). Counts are in `anomalyco.toml`; the data itself lives in the sqlite replica on the test host and on the PC under `$AY_DATA_DIR/mirror/anomalyco/`, never in the repository.

## The org
- 86 repos, 11 forks, 29 archived, 41 pushed in the last 90 days. All 45 active non-fork repos are forked or synced in aphrody-labs.
- By last push: opencode (227k stars), opentui (13.8k), models.dev (7.7k), rune (7.3k), hex (7.1k), browse, nightmail, deadbolt, opentunnel, strobe, opencode-console-cli, textjs, torpedo.
- Languages by bytes over all repos: TypeScript 120.6 MB (61 repos), Rust 75.8 MB (24), C++ 19.1 MB, Python 18.4 MB (22), MDX, C, Zig 8.0 MB (4), Go 5.6 MB (12).
- Tags (language share of 10% or more, or a manifest, `build.zig`, `bun.lock`, `bunfig.toml`, a `bun` script): bun on 11 of the 12 deep-synced repos, rust 8, typescript 9, zig 1 (opentui), go (torpedo and 10 more repos by language). Rust, Zig, Python, Bun and Go repos are listed per tag in `[tags]`.

## What was replicated (12 most recently pushed non-fork repos, full)
Replica rows against GitHub's own counts at the end of the run:

| Table | Replica | GitHub |
|---|---|---|
| opencode issues | 34,143 | 34,143 |
| opencode pull requests | 29,332 | 29,331 |
| opencode releases | 898 | 898 |
| opencode labels | 77 | 77 |
| all 13 repos, issues / pulls / releases | 37,274 / 46,315 / 1,100 | 37,274 / 46,314 / 1,100 |
| opencode default-branch commits (git rev-list against GitHub, at fetch time) | 22,762 | 22,762 |

At the first full pass opencode was 34,130 / 34,133 issues and 29,294 / 29,313 pull requests: GitHub counts a few items its list query then returns as null; the sync isolates such an item by shrinking the page, skips it, and the next incremental pass picked up the ones that were only late. Whole replica: 66,745 commits, 6,752 refs, 180,291 comments, 16,330 reviews, 461,359 timeline events, 16,194 release assets, 1,142 changelog snapshots (104 parsed versions: only a few repos keep a CHANGELOG file; opencode publishes through releases and `opencode.ai/changelog`), 208 manifests.

## Packages
- npm: 169 published packages (43 from manifests, 125 by `@scope` search, `opencode-ai` named by hand with `--package`); 6 manifest names are not on npm. `opencode-ai` has 12,265 versions and 16.1 M downloads in 30 days; `@opencode-ai/plugin` 139 M; `@opentui/core` 6.3 M.
- crates.io: 2 published crates (opentunnel, opentunnel-cli), 32 manifest names that are not on crates.io, 15 names that belong to someone else (`hex`, `rune`, `browse`, ... recorded as `foreign` and not attributed: the registry entry predates the repo or points at another owner).
- PyPI: none of the 12 deep-synced repos publishes there; the client is covered by a recorded real response.
- Zig: no registry exists; opentui and one more `build.zig.zon` package are recorded with their git tags as versions.
- Download windows differ and are stored with the number: 30 days for npm and PyPI, 90 days for crates.io.

## Sites and APIs
- opencode.ai: 2,101 sitemap pages mirrored (`/data` 1,617, `/changelog` 204, `/docs` 69, `/console` 27, plus language roots). sst.dev 336 pages, openauth.js.org 32; models.dev has no sitemap pages (its data is the models.dev repo).
- opencode server API: `opencode serve` serves its OpenAPI 3.1.0 document at `/openapi.json` (`packages/server/src/routes.ts`); the same spec is committed at `packages/protocol/openapi.json` (title "opencode HttpApi 0.0.1", 139 paths).

## How it runs
`bun packages/org-mirror/src/cli.ts sync <org> [--repo x] [--top N] [--since ISO] [--package npm:NAME] [--steps ...] [--db path]`, then `stats`, `export --toml`, `serve`. Issues and pull requests crawl newest-update-first, store the cursor after every page and stop at the watermark of the previous complete crawl; commits come from `git log` of the mirror and only the ones past the previous tips are read. The full run took about 2.5 hours on <test-host>, dominated by opencode's 63,000 issues and pull requests.

## Not replicated
- The visual Craft replica of opencode.ai (feature row `craft/replica-opencode-ai`, a separate step); the site mirror is only the input.
- Inline review-thread comments, reaction details, draft releases, Discussions, Projects, wiki, Actions runs and checks, repository settings, and private repos.
- Git history of the 74 repos that were not deep-synced (their metadata, languages, topics and language tags are stored), forks, and Go module versions.
- Per-version download counts for npm and PyPI (the registries only expose package totals); PyPI had no candidate.
