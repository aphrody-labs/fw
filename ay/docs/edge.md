# The unified edge Worker

`apps/edge` is one Cloudflare Worker (`ay-edge`) that owns the hosts and paths of the ecosystem and routes each request to the function that
serves it. One file, `apps/edge/edge.toml`, declares the Worker, the buckets, the services and every route; `tools/edge-config.ts` renders
`apps/edge/wrangler.jsonc` from it (checked in CI), and the Worker matches the same table at runtime (`vars.EDGE_ROUTES`), so the zone and the
code cannot disagree. Hosts come from `ay.toml` `[domains]` (`{root}`, `{app}`) and `edge.toml` `[domains]`; the account and zone ids from the
git-ignored `cf.local.toml`. Decision record: ADR-0014.

```text
request -> route table (host, longest path) -> module service     code of this repository, bundled
                                            -> service binding     script that still owns code or state in another repository
```

## Services

| Service | Mode | Serves | Code |
|---|---|---|---|
| `releases` | module | downloads (latest and versioned, per os and arch), `/manifest.json`, portal, legacy aphrody channel API and installers, `/brand/*`, `/ay-tools/*` | `apps/edge/src/services/releases.ts`, `apps/tools-release` reused |
| `assets` | module | content-addressed `v1/<kind>/<sha256>`, agent-skills index, plain keys under `/r2-assets` | `apps/edge/src/services/assets.ts`, `apps/cf-gateway` reused |
| `wasm` | module | wasm32 packages: index, raw module, invoke | `apps/edge/src/services/wasm.ts` |
| `aphrody` | service `aphrody-worker` | docs proxy, GitHub webhook intake into the `github-events` queue | `aphrody` repository |
| `learn` | service `aphrody-learn` | learn sync | `aphrody` repository |
| `auto` | service `auto` | the auto app: D1, `CompanyDO`, the automation Workflow | `apps/auto-server` |
| `shenron` | service `shenron-cdn` | R2 `shenron-cdn`, manifests, import state | `shenron` repository |
| `fw` | service `fw-edge`, disabled | static assets and the `FwOSAgent` Durable Object | `fw` repository |

A module is bundled into the Worker. A service-mode script keeps its code, cron triggers, queue producers and Durable Object classes: it
receives no route of its own and is reached through a service binding. That is how a Durable Object holding data is never re-declared in another
script (below), and how code owned by another repository is used without copying it (docs/architecture.md, Independence). `fw-edge` is not deployed
on the account (read 2026-11-19), so its row stays `enabled = false` until it is, or until `fw` exports its handler as a module.

## Route table (read live 2026-11-19, `bun tools/edge-cutover.ts plan`)

| Route or domain | Owner today | Owner after the cutover |
|---|---|---|
| `releases.<root>/*`, `downloads.<root>/*`, `<root>/download*` | `aphrody-worker` | `ay-edge` / `releases` |
| `<root>/r2-assets/*` | `aphrody-worker` | `ay-edge` / `assets` |
| `assets.<root>/*` | none (and no DNS record) | `ay-edge` / `assets` (gateway) |
| `wasm.<root>/*` | none (and no DNS record) | `ay-edge` / `wasm` |
| `docs.<root>/*`, `<root>/github` | `aphrody-worker` | `ay-edge` -> `aphrody-worker` |
| `learn.<root>/*` | `aphrody-learn` | `ay-edge` -> `aphrody-learn` |
| `auto.<root>` (custom domain) | `auto` | `ay-edge` -> `auto` |
| `cdn.<shenron>/*` | `shenron-cdn` | `ay-edge` -> `shenron-cdn` |
| `ay-cf-gateway`, `ay-tools-release` (no route) | workers.dev or nothing | `assets.<root>/*`, `releases.<root>/ay-tools/*` |
| `assets.<app>` | R2 custom domain of the assets bucket | unchanged: the bucket serves immutable assets directly |

`<root>/*` stays with the `aphrody` site Worker (a longer route wins in Cloudflare, so the routes above take precedence). `<root>`, `<app>`
and `<shenron>` are the values of `[domains]`. Workers that belong to the aphrody monorepo and are not in this scope are untouched
(`aphrody`, `aphrody-app`, `aphrody-deploy`, `aphrody-webos`, `aphrody-sites`, `aphrody-mcp-gateway`, `api-aphrody-com`, `bun-video-showcase`, `docs-proxy`).

## Bindings

| Binding | Kind | Target (edge.toml) | Replaces |
|---|---|---|---|
| `RELEASES` | R2 | `aphrody-releases` | `RELEASES_BUCKET` of aphrody-worker, `RELEASES` of auto |
| `ASSETS` | R2 | `aphrody-assets` | `ASSETS` of ay-cf-gateway, `ASSETS_BUCKET` of aphrody-worker, `FILES` of auto |
| `VIDEO` | R2 | `aphrody-video` | `VIDEO` of ay-cf-gateway |
| `CHROMIUM` | R2 | `aphrody-chromium` | `RELEASES` of ay-tools-release |
| `SVC_APHRODY`, `SVC_LEARN`, `SVC_AUTO`, `SVC_SHENRON_CDN` | service | `aphrody-worker`, `aphrody-learn`, `auto`, `shenron-cdn` | their routes |
| `EDGE_ROUTES`, `EDGE_SERVICES` | vars | rendered table | |

Stay on their script (service mode): D1 `auto` (`DB`), Durable Object `CompanyDO` (`COMPANY_DO`), Workflow `AutomationWorkflow`, `AI`, the `auto`
static assets, R2 `aphrody-backups` and `shenron-cdn`, the `github-events` queue producer, every cron trigger and secret. Each module sees its historic
binding name through a derived env, so two services never fight over `ASSETS` or `RELEASES`.

## Durable Objects, D1 and the Workflow

Read live: the account has one Durable Object namespace that is in scope, `auto_CompanyDO` (SQLite storage, script `auto`), one D1 database `auto`,
and the Workflow `auto-automation-workflow`. `fw-edge` and its `FwOSAgent` class are not deployed, so there is no state to move for fw.

1. Now (committed, zero data movement): `auto` stays a service-mode script behind `SVC_AUTO`. Its `CompanyDO` namespace, D1 binding and
   Workflow are untouched; the Worker forwards the request, so the instances and their storage keep running where they are. Dual running is the default.
2. Fusing `auto` into the Worker is a separate, one-way step that needs the auto module in `apps/edge` first (its static assets directory is a second
   Workers Static Assets binding, and a Worker has one; its `ASSETS`, `FILES` and `RELEASES` bindings map onto `ASSETS` and `RELEASES` above). Then, in one deploy:

   ```jsonc
   "durable_objects": { "bindings": [{ "name": "COMPANY_DO", "class_name": "CompanyDO" }] },
   "migrations": [{ "tag": "v1-transfer-auto", "transferred_classes": [{ "from": "CompanyDO", "from_script": "auto", "to": "CompanyDO" }] }]
   ```

   `transferred_classes` moves the namespace with its data to `ay-edge`; after it `auto` no longer owns the class and must be redeployed without it.
   Both this fragment and the cross-script form below pass `wrangler deploy --dry-run` (4.162.0). Before it: run auto's own backup to
   `aphrody-backups` and `wrangler d1 export auto`; a Durable Object's storage has no export through configuration. Rollback is the reverse
   migration (`from_script: "ay-edge"`).
3. Not possible from configuration alone, so kept where it is: the Workflow instances in flight (a Workflow belongs to the script that created it)
   stay on `auto`; the unified Worker would reach them with `"workflows": [{ "name": "auto-automation-workflow", "binding": "AUTOMATION_WORKFLOW", "class_name": "AutomationWorkflow", "script_name": "auto" }]`.
   A middle path with no migration at all is the cross-script binding `{ "name": "COMPANY_DO", "class_name": "CompanyDO", "script_name": "auto" }`:
   the module code runs in `ay-edge`, the objects keep running in `auto`.

## Assets, one layout and one tool

`tools/r2-sync.ts` (successor of `r2-cdn-sync.ts`) is the only writer of the non-release buckets; bucket names come from `edge.toml` `[buckets]`.

| Bucket (binding) | Layout | Written by |
|---|---|---|
| `ASSETS` | `brand/<file>` identity files of `brand/assets` (icon.svg, favicon.ico, apple-touch-icon.png, manifest); `v1/<kind>/<sha256>.<ext>` content-addressed; mirrors under their own prefix | `r2-sync brand`, `r2-sync assets`, `r2-sync mirror` |
| `VIDEO` | `v1/loop`, `v1/stream`, `v1/segment` | `r2-sync assets --kind loop` |
| `RELEASES` | `<product>/<version>/...`, `<product>/channels/<channel>.json`, `manifest.json`; legacy `channels/`, `manifests/`, `scripts/` stay readable | `tools/release-publish.ts` (docs/release.md) |
| `CHROMIUM` | `ay-tools/...` | the ay-tools pipeline |

The portal reads its icon and touch icon from `brand/` of the assets bucket, which `r2-sync brand` fills from `brand/assets`, the output of the design
pipeline. No repository carries another copy of those files (a hash scan of the sibling repositories found none outside an archive).

## Releases and downloads

Layout, matrix, workflow and the way `fw`, `aphrody` and `nie` call it: docs/release.md. The Worker serves
`/<product>/latest/<os>-<arch>` (302 to the asset; `?channel=`, `?kind=`, `?abi=`), `/<product>/<version>/<file>` (ranges, ETag, immutable),
`/api/releases/<product>/<channel>/latest`, `/manifest.json` and the portal.

## WebAssembly

`tools/edge-wasm.ts` builds the corelib wasm32 variants (`base`, `simd`, `atomics`, `component`), validates them and writes `target/edge-wasm/`
(`registry.ts`, `wasm.json`, the modules). `services/wasm.ts` imports them as `CompiledWasm` (the `rules` entry of the rendered config) and
instantiates them once per isolate. A Worker cannot compile bytes at runtime, so only a module bundled at build time can run; a larger or foreign
module (fw-wasm, nie-plugin built on the build host and passed with `--from`) is served from R2 without being instantiated. `bun test
apps/edge/test/wasm.test.ts` starts `wrangler dev --local` and calls an exported corelib function over HTTP.

## Deploy

`.github/workflows/edge-deploy.yml`, on push to main under `apps/edge`: verify (rendered config current, wasm build, tests, `wrangler deploy --dry-run`),
then deploy only when the repository variable `EDGE_DEPLOY` is `true`. Until `EDGE_CUTOVER_DONE` is `true` it deploys the config without routes, so the
live routes stay on the old scripts and the Worker answers on workers.dev under `/_/<service>/...`; afterwards it deploys the full config and smoke
tests the routed hosts. Secrets: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`.

## Verify locally

```sh
bun tools/edge-config.ts --check                       # wrangler.jsonc matches edge.toml
bun test apps/edge tools/edge-cutover.test.ts tools/r2-sync.test.ts
cd apps/edge && wrangler deploy --dry-run              # bundles and validates, deploys nothing
bun tools/edge-cutover.ts plan                         # read-only: owner of every route today
```

`bun test apps/edge/test/dev.test.ts` runs the Worker in workerd with stubs for the four service-mode scripts, R2 filled by `tools/release-publish.ts`,
and sends real requests with the Host of each route (`wrangler dev` rewrites the host, so the test names it in `x-edge-host`, a header honoured only
when the dev config sets `EDGE_HOST_HEADER`; the rendered config never does).

## Cutover checklist (the owner confirms the first deploy; nothing here runs on its own)

The routes of `aphrody.com`, `aphrody.app` and the custom domain move zones that `ay.toml` `[domains]` marks read-only for this repository; the owner's directive
of 2026-11-19 authorizes the move, and `edge-cutover` changes nothing without `--apply`.

1. Secrets and variables: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` as repository secrets; the token needs Workers Scripts, Workers Routes and DNS read.
   Create the environment `edge-production`.
2. DNS: add proxied records for `assets.<root>` and `wasm.<root>` (`edge-cutover plan` lists the hosts without one).
3. Fill the buckets: `bun tools/r2-sync.ts brand`; the first release through docs/release.md (`manifest.json` and `<product>/...`). The legacy aphrody objects stay readable.
4. Set the repository variable `EDGE_DEPLOY=true` and push (or run the workflow): the Worker is uploaded without routes. Check
   `https://ay-edge.<account-subdomain>.workers.dev/_/` and `/_/releases/manifest.json`, `/_/aphrody/...`, `/_/auto/...`, `/_/shenron/...`.
5. `bun tools/edge-cutover.ts plan`, then `bun tools/edge-cutover.ts apply --apply`. Previous owners are saved in `.private/edge-cutover.json`.
   Routes move one by one; the custom domain `auto.<root>` is re-attached to `ay-edge`.
6. Set `EDGE_CUTOVER_DONE=true`. From now on every push deploys the full config.
7. Smoke: `releases.<root>/health`, `/manifest.json`, `docs.<root>`, `learn.<root>`, `auto.<root>`, `cdn.<shenron>`.
8. Rollback at any step: `bun tools/edge-cutover.ts rollback --apply` restores the previous owners; the old scripts were never deleted.
9. Later, one at a time and only after a week of traffic: retire `ay-cf-gateway` and `ay-tools-release` (their code is reused by `ay-edge`), then the
   routes-less parts of `aphrody-worker`; fuse `auto` and `fw-edge` as described above.
