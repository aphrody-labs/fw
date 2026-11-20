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

## Latency: HTTP/3, cache, early hints, placement

**HTTP/3 at the edge.** `packages/cf-control-plane/cf.toml` `[zone_settings]` declares `http3 = "on"`, `zero_rtt = "on"`, `tls_1_3 = "zrt"`, `early_hints = "on"` and
`min_tls_version = "1.2"` for every zone (a `[zones."<zone>".settings]` table overrides one key). `bun tools/cf-reconcile.ts` diffs them with the zone routes
and DNS; `--apply-settings` writes only the settings (never DNS or routes); `--apply-zones` and `--apply-all` include them. Read live 2026-11-19, the four zones
(`aphrody.com`, `aphrody.app`, `dragonballfr.com`, `rosegriffon.fr`) already had exactly these values, so the apply changed nothing (0 settings applied).

`bun tools/cf-h3-probe.ts` (`ay cf-h3-probe`) proves it: for every host of cf.toml and edge.toml it opens a real QUIC connection (`fetch(..., { protocol: "http3" })`
rejects instead of falling back to TCP) and records the Alt-Svc of the HTTP/2 response in `.private/h3-evidence.json`. Verdicts: `h3`, `no-dns` (declared, absent from live
DNS), `r2-custom-domain`, `fail` (the only one that exits 1). Read 2026-11-19: 106 proxied hosts answer h3 with `alt-svc: h3=":443"; ma=86400`; an independent client
(xh 0.26.3 built with `http3`, run on the test host) agreed on 106 and failed on the same 5. The 5 R2 custom domains (`assets.<app>`, `dist.<app>`, `bun-lib.<app>`,
`video.<app>`, `video.<root>`) answer HTTP/2 with no Alt-Svc whatever the zone setting is: R2 serves them directly. Fronting one with a route of this Worker gives it h3
(`assets.<root>` does once it is routed); the bucket domains stay as they are until the owner moves them. 14 hosts (the `rosegriffon.fr` tunnel hosts, `assets.<root>`, `wasm.<root>`)
have no DNS record yet: `cf-reconcile` lists the records to create.

**HTTP/3 in Bun servers.** `Bun.serve` implements HTTP/3 itself (oven-sh/bun `docs/runtime/http/server.mdx`, experimental): `http3: true` with `tls` listens on the same port over TCP
(HTTP/1.1, with `Alt-Svc`) and UDP (QUIC); `http1: false` makes it UDP only. `packages/edge-stack` `listenOptions()` reads `AY_TLS_CERT`, `AY_TLS_KEY` (PEM files, optional `AY_TLS_CA`) and
`AY_HTTP3` (`1` default once TLS is set, `only`, `0`), and `apps/auto-server` (self-hosted), `apps/chromium-dash` and `packages/org-mirror` spread it into `Bun.serve`;
`packages/edge-stack/test/serve.test.ts` proves an h3 round trip, the Alt-Svc of the TCP response, and h3-only with no TCP listener. The other Bun servers (the `ai-os` gateway, terminal and RPC servers, the ai-triad gateway, the pi-opencode UI, the env-kit OAuth callbacks) do not read these options: they are local endpoints, or reached through
the Cloudflare edge or the tunnels, which terminate HTTP/3 for them. One that must become a public listener gets `...listenOptions()` in its `Bun.serve` call. Never set
`BUN_FEATURE_FLAG_EXPERIMENTAL_HTTP2_CLIENT` or `BUN_FEATURE_FLAG_EXPERIMENTAL_HTTP3_CLIENT` in the Windows environment; the tests need neither.

**Per-route hints.** `edge.toml` `[services.<name>.hints."<route as written>"]` carries `cache = { ttl, swr }` and `early_hints = ["<Link value>"]`. They are rendered into
`vars.EDGE_ROUTES` and applied by `apps/edge/src/hints.ts`: a cacheable GET (200, no cookie, no authorization, no range) is stored in the Cache API for `ttl` seconds and gets
`s-maxage` and `stale-while-revalidate` when the service set no `cache-control` of its own; `early_hints` are added as `Link` headers, which Cloudflare turns into a 103
once the zone setting `early_hints` is on. Today: `docs.<root>/*` 300 s plus 3600 s stale, `cdn.<shenron>/*` 3600 s plus 86400 s stale, the portal (`<root>/download*`) preloads `/brand/icon.svg`.
Mutating routes (the webhook, `auto`) have no hint and are never cached.

**Smart Placement.** `[worker] placement = "smart"` renders `placement.mode`. It is not set: the modules read R2 (global) and the other services are forwarded over service bindings, so
pulling the Worker toward one region would slow every host. It pays for a backend with single-region state, which is the D1 database and `CompanyDO` of `auto`; it goes on that script today
and on this Worker only once `auto` is fused into it.

## One route table, checked

`bun tools/edge-config.ts --check` (part of `bun run check`) fails when `wrangler.jsonc` is not the render of `edge.toml`, and on any drift between the three files:

- every Worker route and custom domain of cf.toml is the edge Worker's (`script = "ay-edge"`, and `edge.toml` has that exact route) or intentionally external (the script is in
  cf.toml `[edge] external_scripts`, the scripts of other repositories, and `edge.toml` has no such route): exactly one owner;
- every `edge.toml` route is declared in cf.toml, in a zone cf.toml knows, and its host has a proxied DNS record (a route on a host without one never receives traffic);
- no pattern is declared by two services, a hint names a route of its service, `[workers.ay-edge]` of cf.toml agrees with `[worker]` (`cpu_ms`, `workers_dev = false`);
- the committed `wrangler.jsonc` carries the rendered `routes`, `vars`, `limits`, `placement` and `workers_dev`.

cf.toml declares the post-cutover owner of the edge routes. `cf-reconcile` therefore shows them as route updates, which equal the `edge-cutover plan` below, and it
never creates or moves a route or custom domain whose script is not deployed (`ay-edge` is not until its first upload): they are listed as skipped.

## Cutover checklist (the owner confirms the first deploy; nothing here runs on its own)

Status 2026-11-20: done with the owner's approval. `ay-edge` was uploaded without routes and checked on workers.dev, the proxied AAAA records of `assets.<root>` and `wasm.<root>` were created (declared in cf.toml `worker_aaaa`), `edge-cutover apply --apply` moved the 10 routes and the `auto` custom domain, and `EDGE_DEPLOY` and `EDGE_CUTOVER_DONE` are `true`. Every routed host answers with `x-served-by: ay-edge`. The 12 `rosegriffon.fr` hosts that cf.toml declares as tunnel CNAMEs have no DNS record because that tunnel was deleted on 2026-11-19: they are not created until the rosegriffon owner declares a live target. The R2 custom domains (`assets.<app>`, `video.*`, `bun-lib`, `dist`) are served by R2 directly over HTTP/2 and are not fronted by the Worker.

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
