# Release train: milestones, branches and channels

One train for the whole stack (fw, aphrody, ay, nie, shenron). It merges two upstream models, read live on 2026-11-21:

- **Chromium** ([chromiumdash](https://chromiumdash.appspot.com/schedule)): one milestone number for every product, a new milestone every 4 weeks, a branch point a few weeks before stable, a Beta period on the branch, Extended Stable for every other milestone, and LTS milestones. On that day: Canary 160.0.8174.2, Dev 160.0.8171.0, Beta and Stable 159.0.8154.13, Extended 156.0.8078.108; M160 branches on 2026-11-30 and ships on 2027-01-05.
- **kernel.org** ([releases.json](https://www.kernel.org/releases.json)): mainline with its `-rcN` phase, one stable series, longterm series picked as the last release of each calendar year and kept at least two years, and linux-next as the daily integration tree. On that day: mainline 7.4-rc3, stable 7.3.5, longterm 6.18.64, 6.12.118, 6.6.162, 6.1.192, 5.15.225, 5.10.274, linux-next next-20261120.

The rules live in `release.toml` `[schedule]` and `[channels]`; `tools/release-plan.ts` computes everything from them.

## Milestones

| Rule | Value | From |
|---|---|---|
| Cadence | a milestone reaches stable every `cadence_weeks` (4) | Chromium |
| Branch point | `release/<M>` is cut from main `beta_weeks` (3) before stable | Chromium branch point, kernel merge window close |
| Beta | builds of `release/<M>` before its stable date | Chromium Beta, kernel `-rcN` |
| Extended | even milestones stay supported `extended_weeks` (8) after their stable | Chromium Extended Stable |
| Longterm | the last milestone to reach stable in a calendar year, supported `lts_weeks` (104) | kernel.org longterm |
| Numbering | one milestone number for every product; M5 (2026-11-21) is the first, after fw 4.x | Chromium |

`bun tools/release-plan.ts plan --date <day>` prints, for a day, the milestone on each channel and the dates of the live milestones.

## Versions

Semver, so crates.io, npm, PyPI and Bun's `Bun.semver.order` sort them correctly:

| Channel | Built from | Version | Example |
|---|---|---|---|
| canary | main, daily | `M.0.0-canary.<build>` | `6.0.0-canary.412` |
| dev | main, weekly | `M.0.0-dev.<build>` | `6.0.0-dev.420` |
| beta | `release/<M>` before stable | `M.B.0-beta.<n>` | `6.380.0-beta.4` |
| stable, extended, lts | `release/<M>` | `M.B.P` | `6.380.2` |

- `M` is the milestone.
- `B` is the commit count of main at the branch point; Chromium's BUILD.
- `n` counts the commits on the branch since the cut; the kernel's rcN.
- `P` counts the patch releases on the branch.

Main builds carry the next milestone, so every canary of M sorts below every release of M. Tags are `v<version>`. A tag without a prerelease suffix belongs to the newest channel that still supports its milestone on that day.

## Channels and their upstreams

| Channel | Kernel | Chromium | Zed (GPUI) |
|---|---|---|---|
| canary | linux-next | Canary | nightly |
| dev | mainline | Dev | nightly |
| beta | mainline -rcN | Beta | preview |
| stable | stable | Stable | stable |
| extended | previous stable series until EOL | Extended | stable |
| lts | longterm | LTS | stable |

Old names keep resolving through `[channels].aliases`: `nightly` is canary, `preview` is beta, `longterm` is lts, `mainline` is dev.

The edge Worker (`apps/edge/src/manifest.ts`) and the publisher accept them, and `tools/release-plan.test.ts` keeps their list equal to release.toml.

`bun tools/release-plan.ts upstream` reads all of these live:

- kernel.org releases.json;
- the Chromium channels and schedule;
- the Zed stable and preview releases;
- the `gpui` crate on crates.io.

aphrody's upstream sync follows the Zed (GPUI) row: its beta builds track Zed preview, its stable builds track Zed stable.

## In CI

Every release workflow resolves version, channel and tag with `bun tools/release-plan.ts meta`, from the `GITHUB_*` environment of the run:

- a `v*` tag is parsed back;
- a run on `release/<M>` is beta, with `B` and `n` counted from the merge base with main;
- anything else is canary, or the channel given to the dispatch.

The build jobs write that version into every manifest before compiling, with `bun tools/version.ts set` in fw, so a binary always reports its release version.

A stable, extended or lts tag must match the committed product version: `bun tools/version.ts --expect`.
