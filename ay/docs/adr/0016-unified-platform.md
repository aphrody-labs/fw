# ADR-16: Unified platform: one edge Worker, consolidated R2, kiri on the fleet, one GPUI dashboard

Status: proposed 2026-11-20. Extends 0012, 0014 and 0015; amends 0014 (service list, bucket layout) and 0015 (kiri rollout order: test host, build host,
prod host). Applies the owner's directive of 2026-11-20 (deduplicate the Cloudflare configuration: one Worker, fewer R2 buckets; Docker and Kubernetes
through an optimized fork of kiri; autoscaling, self-hosting and deployment of every repository; one native GPUI dashboard with a Rust and Go backend).
Every figure below comes from `docs/cloud/platform-inventory-2026-11-20.md` (live read, 2026-11-20) unless a source is named. Nothing was changed on the
account or the hosts to write this record. The rose-griffon Workers and bucket are external and untouched.

## Context
- The account runs 25 Worker scripts for 954,029 requests in 7 days (136,290 a day; 4.09 million projected over 30 days against the 10 million
  included requests of `cf.toml` `[finops]`). `ay-edge` carries 44,300 (4.6 %); `shenron-cdn` carries 503,962 (52.8 %); 16 scripts are merge or delete
  candidates. Only 8 scripts own zone routes (59 routes); 26 custom domains point at 8 scripts (15 of them on `aphrody-webos`).
- 12 R2 buckets hold 265,805 objects and 70.71 GB. No bucket has a custom domain and `r2.dev` is disabled on all 12: every public byte already goes
  through a Worker, so "public" and "private" are properties of what a Worker serves, not of the bucket.
- ADR-14 declared one Worker; the live state is `ay-edge` plus 24 scripts, 8 of them deployed on 2026-11-20 (`ay-edge`, `aphrody-worker`, `aphrody-web`, `fw-edge`,
  `nie-sandbox`, the 3 external), `shenron-cdn` on 2026-11-19 and the other 16 on 2026-11-18. The sources of 8 live scripts exist only in `aphrody-labs/legacy`
  (5 by identical directory name, 3 by inference from the hostname or role); 6 scripts have no configuration in any checkout (`aphrody`, `aphrody-learn`,
  `docs-proxy`, `open-source-website-assets`, `aphrody-releases`, `nie-sandbox`).
- Defects found by the inventory: `aphrody-github` throws on 104,472 of 259,331 invocations (40.3 %) and is the live GitHub App webhook target;
  `aphrody-app` binds a D1 database and a KV namespace that return 404; the `assets` and `bucket` modules of `ay-edge` serve every key of the bucket they
  bind (`apps/edge/src/services/assets.ts`), while the aphrody topology file lists `learn/` and `auto/assets/` (company files) as prefixes of
  `aphrody-assets`; `aphrody-worker` and `aphrody-learn` are two copies of one program (same cron `*/15`, same bindings).
- Fleet: build host (12 cores, 45 GB, podman only, root disk 86 %), test host (6 cores, 11 GB, no runtime, root disk 71 %), prod host (8 cores, 31 GB, Docker 29.10.0,
  4 Compose projects, 9 running containers, 8 of them on host networking, all `restart: unless-stopped`, none privileged, at most 18 image layers). No cluster
  exists on any host: `kubectl` is installed on two of them and refuses the connection.
- kiri upstream (Apache-2.0, 7 releases from 2026-11-02 to 2026-11-19, 186 open issues) has a Linux host mode without a VM, the Docker Engine API, Compose and
  buildx, and `kiri k8s start` (single-node k3s in one container). The fork is 0 ahead and 2 behind.

## Decision

### 1. Workers: one routable Worker, state scripts behind it
`ay-edge` is the only script that owns routes, custom domains and workers.dev. A script that owns state (Durable Object, Workflow, D1, Vectorize) stays a
service-mode service reached through a service binding, with no route, no domain and workers.dev off, until its own migration gate passes (ADR-14, item 2).
Of 25 scripts: 1 stays as the Worker, 16 leave (11 merge as modules, 5 are deleted), 5 stay as internal state scripts, 3 are external. A Worker's `cpu_ms` is
a ceiling per script, so merged modules keep their budget by measurement: a module whose p99 exceeds the ceiling in preview stays a service.

| Script | Decision | Target | Evidence |
|---|---|---|---|
| ay-edge | keep | the Worker | 14 routes, 1 custom domain, 6 R2 bindings, deployed 2026-11-20 |
| aphrody-github | merge | module `github` at `aphrody.com/github`; the GitHub App hook URL moves there; fix the 40.3 % exceptions in the module first | 259,331 req, 104,472 `scriptThrewException`; `aphrody.com/github` already reaches `aphrody-worker`, which also produces to `github-events` |
| aphrody-worker | merge | modules `docs`, `learn`, `fleet` of `ay-edge` (cron `*/15`, `github-events` producer move with it) | 797 req; bound by `ay-edge` today; source in `aphrody` `.cloudflare` |
| aphrody-learn | merge into the same `learn` module, then delete | one cron instead of two | 1,396 req; same bindings and cron as `aphrody-worker` |
| aphrody-web | merge (same code, `env.web`), routes `web.aphrody.com/*` and `aphrody.com/web*` move | module `releases` portal | 15 req in 7 days |
| aphrody-deploy | merge, last | module `deploy`; the `Insights` Durable Object class moves by `transferred_classes` | 41,994 req, 6 routes, 2 domains; owns the one `Insights` namespace |
| aphrody | merge with aphrody-deploy | module `deploy` (same runtime, other project config) | 15,097 req; binds the same `Insights` namespace as aphrody-deploy |
| aphrody-webos | merge | module `webos`, 29 routes and 15 domains; keeps the service binding to `aphrody-app` | 26,319 req; source in `legacy` |
| aphrody-sites | merge | module `sites` (`creator.`, `cdn.`, `shell.` hosts, `_headers` from the bucket) | 4,018 req; 3 domains |
| api-aphrody-com | merge | module `api` (`api.aphrody.com`) | 3,255 req, 0 errors; stateless, 3,426 bytes uploaded |
| bun-video-showcase | merge | module `showcase` (`video.aphrody.com/*`) | 2,113 req; 1 route |
| aphrody-mcp-gateway | merge | module `mcp` (`mcp.aphrody.com`); 7 secrets move with it; no DO, KV or D1 | 2,086 req, 0 errors |
| ay-cf-gateway | delete | already module `assets` of `ay-edge` | 0 req, no route, no domain, no workers.dev |
| ay-tools-release | delete | `releases` module serves the ay-tools builds | 7 req, workers.dev only |
| docs-proxy | delete | `docs.aphrody.com` is served by `ay-edge` | 31 req, workers.dev only; the aphrody deploy check lists its directory as legacy |
| open-source-website-assets | delete | none; legacy of a former upstream site | 532 req (35 non-OK), workers.dev only; same legacy list |
| aphrody-releases | delete after the caller is identified | module `releases` | 329 req with no route, domain, workers.dev or service binding pointing at it; same legacy list |
| auto | keep, internal | fuse later by `transferred_classes` (ADR-14); the Workflow is rebuilt in `ay-edge` at that time | D1, `CompanyDO`, Workflow, cron `0 6 * * *`; 37,383 req |
| aphrody-app | keep, internal; routes of `bun.aphrody.app` and `bun.aphrody.com` move to `ay-edge` | state script until the Workflow `Ingest`, Vectorize and queue consumer have a module | 140 req; DO `Room`; D1 and KV bindings are dangling and are fixed or removed first |
| shenron-cdn | keep, internal (owned by shenron) | no merge before the shenron Garage cutover | 503,962 req; manifest shards in its configuration |
| fw-edge | keep, internal (owned by fw) | joins `ay-edge` when `fw` exports a module (ADR-14) | 38 req, workers.dev, DO `FwOSAgent` |
| nie-sandbox | keep, internal (owned by nie) | revisit when its source is in a checkout | 0 req, created 2026-11-20, DO `NieJob`, bucket `nie-private`, no configuration in any checkout |
| rosegriffon-discord, rosegriffon-edge, rosegriffon-maintenance | external, untouched | none | other organization, project stopped |

End state: 9 scripts instead of 25 after the 16 exits: `ay-edge`, 5 internal state scripts, 3 external. With every state
migration done (auto, aphrody-app, fw-edge, nie-sandbox) the account holds `ay-edge`, `shenron-cdn` and the 3 external scripts.

### 2. R2: three buckets in scope instead of eleven
Target buckets: `aphrody-public` (every object an `ay-edge` module serves to the public), `aphrody-private` (everything else of the account), and the existing
`shenron-cdn` (181,584 objects whose manifest hashes live in the shenron repository; moved with the Garage cutover, not before). `rosegriffon-cdn` is external.
Old names become key prefixes, so a module changes a prefix in `edge.toml` and a URL does not change. No bucket gets a custom domain or `r2.dev`; each keeps
the lifecycle rule that aborts incomplete multipart uploads after 86,400 s. Bucket CORS is dropped in favor of the `[cors.*]` tables of `edge.toml`
(no browser reaches the S3 endpoint).

| Source bucket | Objects | GB | Target | Notes |
|---|---:|---:|---|---|
| aphrody-assets | 800 | 0.91 | `aphrody-public/assets/` except `learn/` and `auto/assets/` | those two prefixes go to `aphrody-private/learn/` and `aphrody-private/auto/assets/` after the S3 listing of the copy step confirms they exist |
| aphrody-video | 78 | 0.02 | `aphrody-public/video/` | |
| aphrody-releases | 11 | 0.08 | `aphrody-public/releases/` | layout owned by `tools/release-publish.ts` |
| aphrody-dist | 40,986 | 18.33 | `aphrody-public/dist/` | largest copy, 5,572,379 HeadObject a week: identify the S3 clients first |
| bun-lib | 8 | 0.28 | `aphrody-public/bun-lib/` | |
| aphrody-chromium | 921 | 2.20 | `aphrody-public/chromium/` | 8 reads a week; served by the `releases` module |
| aphrody-backups | 134 | 7.93 | `aphrody-private/backups/` | holds the statutory Factur-X archives (`auto/archives/`); moved last, old bucket kept 30 days |
| aphrody-gnu | 708 | 2.76 | `aphrody-private/gnu/` | cold; lifecycle transition to Infrequent Access (the SDK 7.3.0 lifecycle type carries `storageClassTransitions`) |
| aphrody-lore | 39,461 | 0.23 | `aphrody-private/lore/` | no Worker binding; 3,077,997 HeadObject and 3,204 ListObjects a week from an unidentified client, found before the move |
| nie-private | 555 | 0.07 | `aphrody-private/nie/` | after `nie-sandbox` is stable; bucket created today and written 555 times this week |
| shenron-cdn | 181,584 | 37.79 | keep | shenron owns it; see above |
| rosegriffon-cdn | 559 | 0.11 | external | untouched |

Public: 42,804 objects, 21.82 GB. Private: 40,858 objects, 10.99 GB. `aphrody-private` is never served by a prefix-wide module: each reader (`mcp` for
`backups/`, the `learn` module for `learn/`, `auto` for `auto/`) names its prefix and checks its caller. The `assets` and `bucket` modules gain a prefix
allow-list in the same change, so `aphrody-public` cannot expose a private key by construction.

### 3. Migration order, verification, rollback
Each step ends with a measured check against the baseline of the inventory and has a reversal that does not need the next step. Deletions are the only
irreversible actions: each is preceded by an export of the script and its settings to `.private/` (done on 2026-11-20 for 16 scripts) and by a quiet window
with zero requests, read from the GraphQL `workersInvocationsAdaptive` and `r2OperationsAdaptiveGroups` datasets.

| # | Step | Verification | Rollback |
|---|---|---|---|
| 0 | Fix the defects: `github` exceptions, dangling D1 and KV of `aphrody-app`, prefix allow-list of the `assets` and `bucket` modules | module tests in `apps/edge/test`; 24 hours with a non-OK ratio under 1 % | redeploy the previous script version |
| 1 | Delete `ay-cf-gateway`, `ay-tools-release`, `docs-proxy`, `open-source-website-assets` | a second 7-day window with 0 requests outside health probes, no route or domain | re-upload the exported script |
| 2 | Create `aphrody-public` and `aphrody-private` with the multipart lifecycle rule | bucket listing and lifecycle read-back | delete the empty buckets |
| 3 | Server-side copy, bucket by bucket (S3 CopyObject), public tier first, `backups` last | object count and payload bytes equal the GraphQL storage figures (for example 40,986 and 18,328,252,275 for dist); size and ETag compared on every key | delete the new prefix; the old bucket is untouched |
| 4 | `edge.toml` gains `bucket + prefix` per binding; upload `ay-edge` without route changes | `/_/<service>/` preview on workers.dev; for 200 sampled keys per module bytes, ETag, Range, HEAD and CORS preflight equal old and new paths | redeploy the previous `ay-edge` version (old bindings stay until step 9) |
| 5 | Writers switch to the new layout (`tools/r2-sync.ts`, `tools/release-publish.ts`, fw gnu sync, auto backups), then a final delta copy | a write lands in the new prefix and shows in the preview; the old bucket receives 0 PutObject for 24 hours | point the writer back by environment |
| 6 | Merge modules in this order: `github`, `api`, `sites`, `showcase`, `mcp`, `learn` (with the cron and the queue producer), `webos`, `deploy` | per module: tests, preview, then one route or domain moved with `tools/edge-cutover.ts` (preview first proves that a Workers Domains `PUT` re-points an existing hostname in place; if not, detach and attach run in one script); body hash and status equal the old script; status counts of the next 48 hours compared with the 7-day baseline; for `mcp`, a real MCP client completes login, `tools/list` and one call | `edge-cutover rollback` for the route; `PUT` the old service on the domain; the GitHub App hook URL reverts |
| 7 | Move `Insights` to `ay-edge` with `transferred_classes` and the `deploy` module | `Insights` namespace answers from the new script; row counts equal before and after | take the export first and keep `aphrody-deploy` and its class deployed until the new namespace is verified |
| 8 | Delete merged scripts one at a time after a 7-day soak | 0 invocations for 7 days; domains and routes list no longer names the script | re-upload the export |
| 9 | Delete the old buckets after 14 days with 0 Get, Head and Put in `r2OperationsAdaptiveGroups` (`aphrody-backups` after 30) | final count and bytes of the target prefix equal the figures recorded at step 3 | none after deletion: the target holds the verified copy |

### 4. kiri: fork, patch queue, per-host rollout

**Fork model.** `aphrody-labs/kiri` stays a mirror: its `main` receives upstream `main` by fast-forward and never carries a local commit, so the existing
`gh repo sync` path of `fw-nt/tools/sync.ts` keeps working. Two further branches hold the work:
- `patches`: only `patches/*.patch` (`git format-patch` series) and `patches/series`. Each patch header names its purpose and carries
  `Upstream-Status: carry | submitted <url> | merged <tag>`. Upstream contributions need the maintainer's CLA; until then every patch is a carry.
- `aphrody`: generated, `main` plus the series applied with `git am --3way`, force-pushed by the workflow only. The gitlink of `third_party/kiri` pins its head.

**Automation, reusing `fw-nt/tools/sync.ts` and `.github/workflows/sync.yml` (every 6 hours).** `forks.toml` gains two optional keys on a fork,
`patched = "aphrody"` (generated branch) and `patches = "patches"` (series branch); the drift check against `.gitmodules` then compares the pinned branch
(`patched` when set). After the mirror step, `sync.ts`, for a fork with `patched`:
1. clones `main` and the series branch shallowly into a temporary directory and applies the series;
2. a patch that fails to apply but applies in reverse is already upstream: it is reported as `merged` and a removal is proposed, not applied silently;
3. any other failure aborts, leaves `aphrody` and the gitlink at the last good commit, creates or updates one issue titled `kiri patch <name> conflicts
   with upstream <sha>` and exits non-zero so the workflow run fails and the dashboard shows it red;
4. on success it force-pushes `aphrody` with `--force-with-lease` and bumps the gitlink as it does for every fork today.
A separate job of the fork (`aphrody-release.yml`), on a runner of the build role, builds `cargo xtask linux` from the tag plus series, runs
`tests/linux-host.sh`, and publishes `kiri-<upstream tag>-aphrody.<n>` static binaries with SHA-256 files to the fork's releases. The fleet installs only
those tags, never the branch head. Upstream releases arrive every 1 to 5 days (7 between 2026-11-02 and 2026-11-19), so the lag is bounded by one sync
period plus one build.

**Patches to qualify first (each must be proven necessary on the test host, not assumed).** (a) servers must not self-update or call a share relay: read
`update.rs` and `share.rs` behavior on the Linux daemon and add switches only if absent; (b) a multi-node join for the single-node k3s (`kiri k8s join`),
required for one cluster across hosts; (c) an Arch Linux package next to the `.deb` and `.rpm`; (d) rebranding, since the name and logo are excluded from
the license: the fleet runs the binary under an Aphrody name before any redistribution.

**Per-host rollout (order amended from 0015).** A host moves only after the previous one passed its proof.
1. Test host first: install the package, `docker context create kiri`, run `kiri k8s start`, run the stateless part of the shenron stack from its Compose
   files with test data, restart the daemon while containers run (prod uses `live-restore`, so survival across a daemon restart must be proven), and
   confirm with `ss` that the daemon opens no outbound connection beyond image registries. `kiri.service` sets the daemon's `HOME` to
   `/var/lib/kiri-daemon`, on a root disk at 71 % of 96 GB: where its state really lands is confirmed, and a size budget is set and checked by `ay hygiene`.
2. Build host second: kiri replaces podman for image builds (buildx), with its data directory bind-mounted on the work volume because the root disk is at
   86 % of 290 GB. Proof: the 9 production images rebuild with identical layer counts (3 to 18) and the Compose stack starts healthy.
3. Prod host last, no package (Arch Linux): install the static binaries built on the build host and `contrib/linux/kiri.service`, create the `kiri`
   context beside the Docker socket. Docker and Compose stay installed until the last project has soaked 7 days.
   Eight of nine containers use host networking, so a container cannot run on both engines at once. Migration is therefore stop-then-start per Compose project,
   on the same bind mounts, which makes rollback exact (`compose down` on kiri, `compose up` on Docker, data untouched). Order: `shenron-tv`,
   `shenron-admin` (stateless, 3 and 5 layers); `shenron-cloudflared` (the shenron tunnel shows 8 connections, so one connector may drop); `shenron-gateway`,
   `shenron-site` (45 % CPU, compared before and after on request error ratio); `shenron-bot`; `shenron-redis`; `shenron-postgres` (pg_dump and backup timer run
   first, memory and ownership of the bind mounts checked); `nie-pg` (published on loopback and the mesh) last. Gate per project: all containers `healthy`,
   public health endpoints 200 through the tunnel, 24 hours with the Worker non-OK ratio of `shenron-cdn` at or below its 0.47 % baseline (2,349 of 503,962).
4. Kubernetes: `kiri k8s start` on the test host only, for stateless services and for the autoscaler tests. Production workloads stay on Compose over kiri until
   a multi-node join patch (b) exists and a service has run 7 days on the test cluster. This keeps the owner's one-command Kubernetes without moving
   production onto a single-node k3s that cannot fail over.

### 5. Autoscaling: who decides
The fleet is fixed at three hosts and the Cloudflare edge scales itself, so autoscaling here means replica counts inside host capacity and burst placement
onto the build and test hosts. Node creation is out of scope.

| Decision | Owner | Timescale | Why |
|---|---|---|---|
| Desired state, min and max replicas, memory reserve, cost and quota ceilings | Bun tools (`ay.toml` and `cf.toml` rendered by `tools/*.ts`, committed) | per commit | declarative, reviewed, already how `cf-quota`, `cf-finops`, `deploy` work |
| Replica count of a service inside its bounds, burst placement on hosts labeled `burst` (never prod data) | Go controller `ay-ctl`, one agent per host | 15 s level-triggered loop | needs `client-go` v0.37.2 and the moby client v0.6.3 (module `github.com/moby/moby/client`), watch semantics and idempotent reconcile |
| Pod autoscaling inside a k3s | the native HorizontalPodAutoscaler; `ay-ctl` only creates and bounds the object | native | no second loop |
| Edge capacity and quotas | none (Workers scale); the Rust hub reads request, CPU and R2 operation counts and `cf-quota.ts` alerts | hourly | 4.09 million projected against 10 million included |
| Deploy, rollout, rollback | Bun (`tools/auto-deploy-*.ts`, `deploy.ts`) calling the `ay-ctl` rollout API | per event | one pipeline, one audit trail |

Policy: target 60 % CPU over a 5-minute window, minimum 1 replica, maximum bounded by 75 % of the node's RAM minus the measured resident set of its
stateful services, 10-minute scale-in cooldown, no scaling of a service that holds a bind-mounted data directory. Prerequisite for any replica above 1:
the service leaves `network_mode: host` and is fronted by a Pingora load balancer rendered from the service registry (`ay-lb`, `pingora` crate 0.9.1,
the aphrody-labs fork exists), which also replaces nginx on the prod host as ADR-15 item 4 requires. Until then the controller runs in observe mode and
only writes decisions to its log.

### 6. One native dashboard
**Client.** `aphrody` gets `crates/aphrody_dashboard` (GPUI binary and an `AppModule` of `gpui_app_shell`, so it also runs inside Aphrody Studio),
`crates/aphrody_dash_client` (tonic client with reconnect and streaming) and `crates/aphrody_dash_api` (prost types). Views: Overview (three nodes,
capacity, alerts), Workloads (containers and pods, logs, exec, scale, rollout), Edge (Workers, routes, domains, R2 buckets, 7-day requests), Deploys
(pipeline runs, fork sync state, patch queue), Scale (policies, decisions), Settings (session). Every row is exposed through AccessKit and every action is
reachable from the keyboard; high-contrast and large-text settings follow the OS. It depends on `fw`, `gpui` and `nsh-*` only.

**Protocol.** Protobuf and gRPC (server streaming for watches and logs), one schema `aphrody/proto/dash/v1` with services `Fleet`, `Workload`, `Edge`,
`Deploy`, `Auth`. The schema lives in `aphrody` because the client is there and `ay` sits above it in the stack: `ay` depends on the generated crate by path
and its Go module generates stubs from the sibling checkout with `buf generate`; CI runs `buf lint`, `buf breaking` and a generated-code drift check. Transport
is HTTP/2 with mTLS on the mesh between hosts and, from the desktop, the tunnel behind Cloudflare Access with a service token and the owner's login.

**Backend, both in `ay` (`services/`).**
- `ay-ctl` (Go, one agent per host): talks to kiri's Docker socket (moby client) and to the local k3s (client-go); exposes `Workload` and the rollout API;
  runs the autoscaler of section 5.
- `ay-hub` (Rust, the single endpoint the dashboard dials; primary on the prod host, warm standby on the test host through the existing standby timers):
  `Fleet` (fans in the three `ay-ctl` streams and host probes), `Edge` (read model of the Cloudflare REST and GraphQL endpoints used by the inventory, about
  twenty; `cloudflare-rs` is not used: its crate release is 0.14.0 from 2025-03-13), `Auth` (Access JWT audience check, short-lived session, step-up for writes),
  an append-only audit log in SQLite mirrored to `aphrody-private/audit/`. Writes to Cloudflare are not reimplemented: the hub starts
  `tools/cf-reconcile.ts --apply` as an audited job, so one implementation owns mutations.
- `ay-edge` modules that are compute-bound and already exist as Rust crates (`ay-cf` content addressing, range parsing) compile with the `workers-rs` fork
  (`worker` crate 0.9.0) through `tools/edge-wasm.ts`; no working TypeScript module is rewritten for its own sake.

Dependency rule check: `aphrody_dashboard` -> `fw`; `ay-hub` -> `aphrody_dash_api` and `ay-cf`; `ay-ctl` -> generated stubs of the `aphrody` schema; kiri is
installed from release artifacts of the fork, not linked, so `fw-nt` stays a satellite.

### 7. Work packages and gates
| WP | Repo | Deliverable | Gate | After |
|---|---|---|---|---|
| 1 | ay | `tools/cf-inventory.ts` (typed, SDK, writes the private raw file and the public summary) | reproduces 25 Workers, 12 buckets and the figures of the inventory | none |
| 2 | ay, legacy sources | Defect fixes (section 1 and step 0) | module tests; 24 hours under 1 % non-OK | 1 |
| 3 | ay | `edge.toml` prefix support, prefix allow-list, `tools/r2-migrate.ts` (copy, verify, report), `cf.toml` `[buckets]` reduced | counts and bytes equal GraphQL; 200-key parity per module | 2 |
| 4 | ay | Module merges, steps 6 and 8, deleting the 11 merged scripts after soak | per-module gate of section 3 | 3 |
| 5 | ay, aphrody, fw, nie | State fusion: `Insights` transfer, `auto` fuse (ADR-14), decisions for `aphrody-app`, `fw-edge`, `nie-sandbox` | export verified; 7-day soak | 4 |
| 6 | fw-nt, kiri fork | Patch queue in `tools/sync.ts` and workflow, `patches` and `aphrody` branches, fork release workflow, a test for the sync step | an injected conflicting patch fails the run red and opens one issue; a clean run produces the `aphrody` head and the gitlink bump | none |
| 7 | ay | `tools/kiri-fleet.ts` (install, upgrade, rollback by symlink, per role) | test host: `tests/linux-host.sh`, daemon restart keeps containers, no outbound beyond registries, `kiri k8s start` ready | 6 |
| 8 | ay | Build host on kiri (buildx), data directory on the work volume | 9 production images rebuild with equal layer counts; Compose stack healthy | 7 |
| 9 | shenron, ay | Prod migration per Compose project (section 4) | per-project gate; 7-day soak before Docker is removed | 8 |
| 10 | aphrody | `proto/dash/v1`, `aphrody_dash_api`, `aphrody_dash_client` | `buf lint`, `buf breaking`, Rust and Go stubs generated in CI | none |
| 11 | ay | `ay-hub` (Rust) | numbers equal WP 1; Access audience check rejects a forged token; gRPC stream survives 1 hour through the tunnel | 1, 10 |
| 12 | ay | `ay-ctl` (Go) and Pingora `ay-lb` | scale-up and scale-down integration test on the test host; 24 hours without flapping; observe-mode log reviewed before it acts | 7, 10 |
| 13 | aphrody | `aphrody_dashboard` | screenshots of every view inspected at three window sizes; keyboard-only traversal; live data from `ay-hub` | 10, 11 |
| 14 | ay | Autoscale enabled for services that left host networking | 7 days without a manual correction | 9, 12 |

## Consequences
- Every public request of the ecosystem passes through one script: a failure of `ay-edge` is a failure of all hosts (ADR-14); the 16 exits are
  reversible until step 8 and the exports exist.
- Route and domain ownership of `aphrody.com` and `aphrody.app` moves from 9 scripts to `ay-edge`; DNS records and zones are not touched. `ay.toml` `[domains]`
  still marks these zones read-only for this repository, and the move is applied under the owner's directive as in ADR-14.
- Prod keeps Docker until step 9 has soaked; no change reaches prod that has not run on the test and build hosts.
- A single-node k3s cannot fail over, so Kubernetes is a test-host capability until the join patch exists.
