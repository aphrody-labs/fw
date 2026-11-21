# Platform inventory, 2026-11-20

Read-only snapshot taken on 2026-11-20 for ADR-16. Sources: the Cloudflare REST API and GraphQL Analytics through the `cloudflare` SDK of
`packages/cf-control-plane` (account scope, 7 days ending 2026-11-20 19:00 UTC), `ssh` on the three fleet hosts (named by role), `gh api`, and
`git grep` over the workspace checkouts. Raw output, with ids, is in `.private/cf/inventory-2026-11-20.json` (git-ignored). Nothing was changed
on the account or the hosts.

## Cloudflare Workers (25 scripts, 954,029 requests in 7 days)

`non-OK` counts invocations whose status is not `success` (client disconnects included). Deployer is the repository and file that holds the
configuration; `legacy` is `aphrody-labs/legacy`; `none` means no checkout of the workspace holds a configuration for the script.

| Worker | Req 7d | non-OK | Routes | Custom domains | workers.dev | Cron | State and bindings | Last deploy | Deployer |
|---|---:|---:|---:|---|---|---|---|---|---|
| shenron-cdn | 503,962 | 2,349 | 0 | none (route `cdn.dragonballfr.com/*` is owned by ay-edge, which binds it as a service) | no | none | R2 `shenron-cdn`; 4 manifest shard vars | 2026-11-19 | shenron `deploy/cloudflare/wrangler.jsonc` |
| aphrody-github | 259,331 | 104,472 (all `scriptThrewException`, 40.3 %) | 0 | none | yes | none | queue producer `github-events`; webhook secret | 2026-11-18 | probable: legacy `infra/cloudflare/workers/github-app` |
| ay-edge | 44,300 | 736 | 14 | `auto.aphrody.com` | no | none | R2 x6 (assets, bun-lib, chromium, dist, releases, video); services x4 (aphrody-worker, auto, aphrody-learn, shenron-cdn) | 2026-11-20 | ay `apps/edge/edge.toml` |
| aphrody-deploy | 41,994 | 133 | 6 | `aphrody.app`, `www.aphrody.app` | no | none | R2 `aphrody-dist`; DO `Insights` (owner); Images; Access vars | 2026-11-18 | legacy `packages/deploy/wrangler.toml` |
| auto | 37,383 | 11 | 0 | bound by ay-edge (`auto.aphrody.com`) | no | `0 6 * * *` | D1 `auto`; DO `CompanyDO` (SQLite); Workflow `AutomationWorkflow`; R2 backups, assets, releases; AI | 2026-11-18 | ay `apps/auto-server/wrangler.jsonc` |
| aphrody-webos | 26,319 | 9 | 29 | 15 `*.aphrody.app` (13 craft apps, `apt`) | no | none | R2 `aphrody-assets`; service `aphrody-app` | 2026-11-18 | legacy `infra/cloudflare/workers/aphrody-webos` |
| aphrody | 15,097 | 76 | 1 | `www.aphrody.com` | no | none | DO `Insights` (class owned by aphrody-deploy, same namespace); R2 `aphrody-dist`; Images | 2026-11-18 | none (same runtime as aphrody-deploy) |
| aphrody-sites | 4,018 | 13 | 0 | `creator.aphrody.com`, `cdn.aphrody.com`, `shell.aphrody.app` | no | none | R2 assets, dist | 2026-11-18 | probable: legacy `infra/cloudflare/workers/creator-aphrody-com` |
| api-aphrody-com | 3,255 | 0 | 0 | `api.aphrody.com` | no | none | R2 `aphrody-dist`; catalogue vars | 2026-11-18 | probable: legacy `infra/cloudflare/workers/api-gateway` |
| rosegriffon-maintenance | 2,225 | 0 | 0 | none | no | none | static assets | 2026-11-20 | external (rose-griffon) |
| bun-video-showcase | 2,113 | 3 | 1 (`video.aphrody.com/*`) | none | no | none | R2 `aphrody-video` | 2026-11-18 | legacy `infra/cloudflare/workers/bun-video-showcase` |
| aphrody-mcp-gateway | 2,086 | 0 | 0 | `mcp.aphrody.com` | no | none | R2 `aphrody-backups`; 7 secrets (OAuth, Google, Access) | 2026-11-18 | legacy `infra/cloudflare/workers/aphrody-mcp-gateway` |
| aphrody-learn | 1,396 | 0 | 0 | none (bound by ay-edge) | no | `*/15 * * * *` | R2 assets, releases; queue producer `github-events` | 2026-11-18 | none |
| aphrody-worker | 797 | 0 | 0 | none (bound by ay-edge) | no | `*/15 * * * *` | R2 assets, releases; queue producer `github-events` | 2026-11-20 | aphrody `.cloudflare/aphrody-worker/wrangler.toml` |
| open-source-website-assets | 532 | 35 | 0 | none | yes | none | R2 `aphrody-assets` | 2026-11-18 | none (aphrody `tooling/bun/cloudflare-deploy.ts` lists its directory as legacy that must not exist) |
| aphrody-releases | 329 | 1 | 0 | none | no | none | R2 `aphrody-releases` | 2026-11-18 | none (same legacy list) |
| rosegriffon-edge | 153 | 0 | 5 (`cdn.rosegriffon.fr`) | none | no | none | R2 `rosegriffon-cdn` | 2026-11-20 | external (rose-griffon) |
| aphrody-app | 140 | 5 | 0 | `bun.aphrody.app`, `bun.aphrody.com` | no | `17 3 * * *` | D1 `DB` and KV `CACHE` that no longer exist (both 404); DO `Room`; Workflow `Ingest`; Vectorize `bun-native`; queue consumer `aphrody-app-jobs`; R2 assets; AI; 3 rate limiters; Smart Placement | 2026-11-18 | legacy `apps/aphrody-app/wrangler.toml` |
| fw-edge | 38 | 0 | 0 | none | yes | none | DO `FwOSAgent`; static assets | 2026-11-20 | fw `cloudflare/wrangler.jsonc` |
| docs-proxy | 31 | 0 | 0 | none | yes | none | none | 2026-11-18 | none (legacy list) |
| aphrody-web | 15 | 0 | 2 (`web.aphrody.com/*`, `aphrody.com/web*`) | none | no | none | R2 assets, releases | 2026-11-20 | aphrody `.cloudflare/aphrody-worker/wrangler.toml` (`env.web`) |
| ay-tools-release | 7 | 0 | 0 | none | yes | none | R2 `aphrody-chromium` | 2026-11-18 | ay `apps/tools-release/wrangler.jsonc` |
| ay-cf-gateway | 0 | 0 | 0 | none | no | none | R2 assets, video | 2026-11-18 | ay `apps/cf-gateway/wrangler.jsonc` |
| nie-sandbox | 0 | 0 | 0 | none | no | none | DO `NieJob`; R2 `nie-private`; Access vars | 2026-11-20 | none |
| rosegriffon-discord | 0 | 0 | 1 (`api.rosegriffon.fr/v1/discord/interactions`) | none | no | none | secrets only | 2026-11-20 | external (rose-griffon) |

Zone routes: 59 in total (aphrody.app 25, aphrody.com 27, dragonballfr.com 1, rosegriffon.fr 6). Owners: aphrody-webos 29, ay-edge 14, aphrody-deploy 6,
rosegriffon-edge 5 (+1 rosegriffon-discord), aphrody-web 2, aphrody 1, bun-video-showcase 1. Custom domains (Workers Domains API): 11 non-webos
hostnames plus the 15 of aphrody-webos.

Account-level objects: D1 `auto` (307,200 bytes); queues `aphrody-app-jobs` (consumer: aphrody-app) and `github-events` (1 HTTP-pull consumer, batch 50,
10 retries; producers: aphrody-github, aphrody-learn, aphrody-worker); KV namespaces listed: 0; Durable Object namespaces: 5 (`aphrody-app:Room`,
`aphrody-deploy:Insights`, `auto:CompanyDO`, `nie-sandbox:NieJob`, `fw-edge:FwOSAgent`); Workflows: 2 (`auto-automation-workflow`, `aphrody-app-ingest`);
Vectorize indexes: `bun-native`, `shenron-rag`, `shenron-rag-m3`; tunnels: 4, all healthy (aphrody 4 connections, nie 4, rosegriffon 4, shenron 8).

Observed defects (each is a work package in ADR-16):
- `aphrody-github` throws on 104,472 of 259,331 invocations (40.3 %); it is the live GitHub App webhook target through its workers.dev URL.
- `aphrody-app` binds a D1 database and a KV namespace that return 404 (the only D1 database of the account is `auto`).
- `apps/edge/src/services/assets.ts` and the `bucket` module serve every key of the bucket they bind; the topology file of the aphrody repository
  lists `learn/` and `auto/assets/` (company files) as prefixes of `aphrody-assets`.
- Two scripts, `aphrody-worker` and `aphrody-learn`, carry the same cron, bindings and near-identical code (247,181 and 230,200 bytes uploaded).

## R2 buckets (12; 265,805 objects; 70.71 GB payload)

No bucket has a custom domain (12 of 12 empty) and the `r2.dev` managed domain is disabled on all 12: every public byte goes through a Worker.
Every bucket carries one lifecycle rule (abort incomplete multipart after 86,400 s). CORS rules exist on 5 (aphrody-assets, aphrody-dist, aphrody-video,
shenron-cdn, rosegriffon-cdn). Storage class is Standard everywhere. Operations are the S3 and Workers operations of the 7 days.

| Bucket | Objects | GB | Created | GetObject 7d | HeadObject 7d | PutObject 7d | Binding Workers | Writers outside Workers |
|---|---:|---:|---|---:|---:|---:|---|---|
| shenron-cdn | 181,584 | 37.79 | 2026-10-11 | 1,123,255 | 296,243 | 0 | shenron-cdn | shenron runbooks (rclone) |
| aphrody-dist | 40,986 | 18.33 | 2026-10-09 | 1,935,229 | 5,572,379 | 4,639 | aphrody, aphrody-deploy, aphrody-sites, api-aphrody-com, ay-edge | not identified in the checkouts (5 Workers bind it; 5,572,379 HeadObject suggest S3 clients) |
| aphrody-lore | 39,461 | 0.23 | 2026-11-12 | 0 | 3,077,997 | 110 | none | unidentified S3 client (3,204 ListObjects) |
| aphrody-backups | 134 | 7.93 | 2026-11-16 | 0 | 190 | 0 (multipart: 662 UploadPart) | auto, aphrody-mcp-gateway | fw `packages/ai-os/src/brain/skills-archive.ts` |
| aphrody-gnu | 708 | 2.76 | 2026-11-14 | 0 | 1,456 | 717 | none | fw `packages/gnu` sync |
| aphrody-chromium | 921 | 2.20 | 2026-11-12 | 8 | 24 | 911 | ay-edge, ay-tools-release | not identified in the checkouts |
| aphrody-assets | 800 | 0.91 | 2026-10-09 | 1,683,537 | 3,344,659 | 823 | aphrody-app, aphrody-learn, aphrody-sites, aphrody-web, aphrody-webos, aphrody-worker, auto, ay-cf-gateway, ay-edge, open-source-website-assets | `tools/r2-sync.ts` (brand, assets, mirror) |
| bun-lib | 8 | 0.28 | 2026-10-13 | 237 | 11 | 0 | ay-edge | not identified in the checkouts |
| rosegriffon-cdn | 559 | 0.11 | 2026-11-12 | 3,230 | 405,318 | 3,504 | rosegriffon-edge | external |
| aphrody-releases | 11 | 0.08 | 2026-11-17 | 141 | 10 | 38 | aphrody-learn, aphrody-releases, aphrody-web, aphrody-worker, auto, ay-edge | `tools/release-publish.ts` |
| nie-private | 555 | 0.07 | 2026-11-20 | 31 | 0 | 555 | nie-sandbox | none |
| aphrody-video | 78 | 0.02 | 2026-10-12 | 1,803 | 28 | 0 | ay-edge, ay-cf-gateway, bun-video-showcase | `tools/r2-sync.ts` (assets, `--bucket VIDEO`) |

Public tier (served through ay-edge routes): assets, video, releases, dist, bun-lib, chromium = 42,804 objects, 21.82 GB. Private or cold:
backups, lore, gnu, nie-private = 40,858 objects, 10.99 GB. Shenron: 181,584 objects, 37.79 GB. External: rosegriffon-cdn.

## Fleet (read-only over ssh)

| | build | test | prod |
|---|---|---|---|
| OS, kernel | Ubuntu 26.10, 7.3.0-10-generic | Ubuntu 26.10, 7.3.0-10-generic | Arch Linux, 7.4.0-rc3 cachyos |
| CPU, RAM used | 12 cores, 45 GB (8 GB used) | 6 cores, 11 GB (2 GB used) | 8 cores, 31 GB (12 GB used) |
| Disks | `/` 290 GB, 86 % used; work volume 805 GB, 4 % | `/` 96 GB, 71 %; local work volume 4 GB; NFS of the build work volume 805 GB | `/` 300 GB, 26 %; game volume 100 GB, 62 % |
| Container runtime | podman 5.8.6, runc 1.5.1; no containers; `/dev/kvm` present | none; `/dev/kvm` present | Docker 29.10.0 (API 1.57), Compose 5.8.0, buildx 0.39.0, containerd 2.4.2, runc 1.5.3; `live-restore` on |
| Kubernetes | `kubectl` binary, no cluster (connection refused) | `kubectl` binary, no cluster | no `kubectl` |
| Our services | CI runner of shenron, Garage S3, nativelink server and worker, link cache, NFS server, fleet and hygiene timers | auto-server, auto-lb, Garage S3, shenron CDN Garage, nativelink worker, edge firewall and fleet standby timers | auto-server, nginx, host PostgreSQL, host Valkey, site and link services, cloudflared units, backup timers; rose-griffon services (external) |

Production containers on the prod host (9 running, 11 in `docker ps -a`; 1 named volume; 15 images, 10.19 GB):

| Compose project | Containers | Network | Notes |
|---|---|---|---|
| shenron | site, gateway, postgres (pgvector pg18), redis, bot, cloudflared | all `host` | site at 45 % CPU and 1.1 GiB; postgres 1.5 GiB; redis limited to 512 MiB |
| shenron-admin | admin | host | |
| shenron-tv | tv | host | memory limit 3 GiB |
| nie-pg | postgres 18.6 | own bridge, published on loopback and the mesh | |

All 9: `restart: unless-stopped`, none privileged, none `pid: host`, 3 to 18 image layers, bind mounts (1 to 12 per container). Eight of nine use host networking.

## kiri (libracoder/kiri, read 2026-11-20)

- License Apache-2.0; name and logo excluded from the license ("forks are welcome, under a different name and logo"); contributions go through a CLA
  held by the maintainer company. Guest kernel is Linux (GPL-2.0) and exists only in the VM modes.
- Created 2026-11-02, 11,617 stars, 186 open issues, last push 2026-11-20. 7 releases between 2026-11-02 and 2026-11-19 (v0.1.14 to v0.2.2), 16 assets each.
  The roadmap says a release most weeks (0.2.3 next, 0.3.0 sharing through Tailscale and Cloudflare Tunnel, 0.4.0 Coolify).
- Linux host mode: no VM, the agent runs on the host kernel; `contrib/linux/kiri.service` (socket `/run/kiri/docker.sock`, group `kiri`, cgroup delegation);
  `.deb` and `.rpm` packages (`kiri` with docker CLI, Compose, buildx; `kiri-lite` engine only); `cargo xtask linux` builds static binaries.
- Docker Engine API with `--network host`, user networks, `--memory`, `--cpus`, `--restart`, seccomp, capabilities; Compose and buildx work through the `kiri` context.
- Kubernetes: `kiri k8s start`, a single-node k3s in one container (about 20 s); no multi-node mode.
- Known blockers listed upstream for other workloads: images with 60 or more layers (ours: at most 18), `--pid host` (ours: none).
- Fork `aphrody-labs/kiri`: 0 commits ahead, 2 behind upstream `main`; gitlink at `fw-nt/third_party/kiri`.
