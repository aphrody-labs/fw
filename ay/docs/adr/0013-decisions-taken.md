# ADR-13: Decisions taken by the assistant under the owner's delegation

Status: accepted 2026-11-17. The owner delegated every open choice ("choose everything yourself, no human in the loop"); this record fixes them so no one asks again. Nothing here touches the frozen DNS zones of aphrody.com and aphrody.app.

## Roles
- **Repositories** (`aphrody-labs/.github` `naming/naming.toml`, enforced by `naming/repos.ts`): every active repository has one kind: product (`os`, `sdk`, `unix`, `android`, `nt`, `ai`, `runtime`, `browser`, `app`, `game`), `tool` (rust-mono, ay-tools, ayvpn, aysh), `config` (claude, gemini and their private `-local` twins), `mirror` (GNU, zig, depot_tools, Wayland, Steam runtime: upstream names, no patches), `fork`, `family` (yolo, frozen) or `archive` (merged repositories, kept read-only).
- **Domains** (as served today, read from the account): `aphrody.com` is the brand, the main page, `api`, `mcp`, `creator`, `cdn`, `downloads` and releases; `aphrody.app` is the web OS and its tools (`deploy`, `webos`, the shell, `bun`, `console`, `control`, `admin`); `dragonballfr.com` is shenron; `rosegriffon.fr` is rosegriffon. A new service takes its host from the role of its product, never a new zone.
- **Engines of the browser**: Chromium 160 is the reference engine and the one that ships; kitesurf is the light engine for the edge (Dynamic Workers) and the proof of the Blitz/Stylo/Taffy/Vello/Boa stack; Servo and Obscura stay as comparison engines of the acceptance tool until kitesurf reaches parity on the shared WPT corpus, then leave the workspace.
- **One stylo, one taffy, one boa**: the engine crates live once, in `kitesurf/crates` as `browser-*` (published on crates.io); the browser workspace depends on those, and its imported Servo copy of stylo is kept only for the Servo build.

## Cloudflare
- One service per area (ADR-0012). The cause of the 1027 refusals is the GitHub App delivering every event (check_run, workflow_job, check_suite, status...) to the events Worker, which keeps five kinds: the App subscribes only to `workflow_run`, `push`, `release` and the four security alerts (installations arrive regardless). That is an App setting that GitHub only lets a signed-in owner change (sudo confirmation), so it is the one step left to the owner's session; Workers Paid is chosen for Containers and Dynamic Workers and is bought in the dashboard by the owner (a purchase is never made by an agent).
- Purge candidates (inventory, private) are removed one at a time when nothing deploys them and they had no traffic for 30 days; the `auto` tunnel is relaunched with its app or removed.

## Publishing
- The 26 `browser-*` crates of kitesurf are published from the `publish-crates` workflow (org secret, never read by an agent), in dependency order; later releases move to trusted publishing.
- The production host stays as it is: shenron and rosegriffon run on it, so a kernel or userland change there waits for a staged rollout proven on the test host first.
