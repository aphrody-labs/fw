# Cloudflare dual mode: every service on Cloudflare and on our nodes

Lot `docs/agents/queue/cloudflare-dual-mode-parity.md`. State of 2026-11-21. `cluster/build.toml` now names the nodes `n1` (alias `fr1`) and `n2` (alias `prd`); `fw-workerd` accepts either, `cluster/dual-mode.toml` lists `n1` and `n2`, and the measurements below keep the names they were taken under (`fr1`, `prd`). `cluster/estate.toml` lists `selfhosted_hosts = ["n1", "n2"]` for fw-edge, material-cdn, ay-edge, auto and aphrody-web; `media-cdn` (bucket `aphrody-media`, bindings `MEDIA` and `IMAGES`) is cloud-only for now, and the Images binding has no self-hosted implementation. Every figure below comes from a command run on that day on `fr1` or `prd` (hosts from `cluster/build.toml`); `fw-workerd` is `crates/fw-workerd`, the declaration is `cluster/dual-mode.toml`.

## Decisions

| Question | Decision | Evidence read today |
| --- | --- | --- |
| Runtime | workerd 1.20261121.1, the npm prebuilt `@cloudflare/workerd-linux-64`, under systemd on `fr1` (x86-64-v3 Haswell) and `prd` (Zen 4, AVX-512). The same binary (sha256 `80cb4197c2cca0e7...`) runs on both: it is the vendor build, not compiled on either host. | `npm view workerd version` gave 1.20261121.1; `gh api repos/cloudflare/workerd/releases` shows `v1.20261121.1` published 2026-11-21T00:46:52Z; `workerd --version` on both hosts prints `workerd 2026-11-21`. |
| Object store behind R2 | Garage 2.5.0 (already installed on both nodes), a second instance dedicated to this mode. MinIO is out. | `gh api repos/minio/minio`: `archived: true`, last push 2026-04-24, last release `RELEASE.2025-10-15T17-29-55Z`. `gh api repos/deuxfleurs-org/garage`: not archived, pushed 2026-11-17, tag `v2.5.0`, which is the installed `garage --version`. |
| How R2 reaches Garage | A bridge Worker (`fw-s3-bridge.js`, in `crates/fw-workerd/src/bridge.rs`) that speaks workerd's `bucket` service protocol over plain S3 calls signed with SigV4. workerd's bundled R2 Worker (`/workerd/r2.js`) stays in front, so the application sees the real `R2Bucket` class. | workerd's own cluster bucket refuses Garage: `joining the cluster: bucket config: the bucket ignores conditional writes (If-None-Match and If-Match on S3 ...)`, and Garage's known-issues page states conditional writes are structurally impossible there. Without a cluster, workerd's `bucket` service writes only to a local directory, which is not S3. |
| D1, KV, Queues, cron, Durable Objects, assets, Cache API | The implementations bundled in workerd (`/workerd/d1.js`, `kv.js`, `queues.js`, `cron.js`, the `assets` and `cache` services), the same sources `wrangler dev` uses; state is SQLite files and cache files under `<work>/edge/state/<service>`. | `workerd.capnp` (`Module.bundled`) and `src/workerd/server/cli/wrangler.rs` read from `cloudflare/workerd` main today; the probe below runs each of them. |
| Second Garage, not the shenron-cdn one | Own unit `fw-edge-garage.service`, replication factor 1, loopback only (S3 3910, RPC 3912, admin 3913), own directories under `<work>/edge/garage`. Port 3911 is taken by another process on `fr1` (it made the first start panic with `Address in use`), so RPC uses 3912. | `ss -ltnp` on `fr1` showed 3911 held by `mcp_fixture_server`; `systemctl cat garage.service` shows the existing Garage serves shenron-cdn on the mesh address with replication factor 2. |

## What was built

`fw-workerd` (Rust, `crates/fw-workerd`, binary `fw-workerd`):

| Command | Does |
| --- | --- |
| `compile --service <name> --host <host> --bundle <dir> --out <dir>` | wrangler config (`.jsonc`, `.json` or `.toml`, optional `env`) plus the bundle of `wrangler deploy --dry-run --outdir <dir>` to `<name>.capnp`, `<name>.report.json` and the copied modules |
| `units --host <host> --out <dir>` | `garage.toml`, `fw-edge-garage.service`, one `fw-workerd-<service>.service` per service on that host, and an idempotent `bootstrap.sh` (directories, secrets, Garage layout, bucket, key, units) |
| `bindings --wrangler <file> [--env <name>]` | which bindings the compiler wires and which it cannot, without a bundle |
| `probe --out <dir>` | a Worker that exercises R2, D1, KV, Queues with a consumer, a SQLite Durable Object, the Cache API and a cron trigger, answering deterministic JSON |
| `parity --spec <file> --cloud <url> --self <url> [--repeat n]` | the same requests to both URLs; status, 13 compared headers, body length and sha256; per-request latency; exit 1 on any difference |

Hosts, users and work directories come from `cluster/build.toml`; ports, the workerd version, the R2 bucket name and the services come from `cluster/dual-mode.toml`. Secrets are never in a generated config: Worker secrets are `fromEnvironment` bindings read from `<work>/edge/secrets/<service>.env`, the S3 key from `aws.env` (mode 600, written by the bootstrap).

Allowed differences between the two modes (`parity::ALLOWED_DIFFERENCES`, never compared): `server`, `cf-ray`, `cf-cache-status`, `date`, `alt-svc`, `nel`, `report-to`, `age`, `server-timing`, `cf-placement`, `set-cookie`, `strict-transport-security`, `expect-ct`, `connection`, `last-modified` (a seeded copy has its copy time). Per request, a spec may list `ignore_headers` and `allow_values` with the reason in a comment: `accept-ranges` on a 206 (Cloudflare's edge drops it) and `x-material-source` (`built`, `r2`, `edge-cache`, which layer answered). `accept-encoding: identity` is pinned because Cloudflare compresses at its edge and workerd does not.

## Parity matrix

Status: **works** = run in this mode and compared or probed today; **wired** = `fw-workerd bindings` reports it supported and the same mechanism works elsewhere in this table, but the service itself was not run; **missing** = no implementation in this mode.

### Bindings

| Service (wrangler) | Binding | Cloud | Self-hosted | Status | Evidence |
| --- | --- | --- | --- | --- | --- |
| material-cdn | R2 `MATERIAL` (`aphrody-material`, 175,582 objects, 238,773,274 bytes) | R2 | workerd `r2.js` over the S3 bridge to Garage | works | `rclone size` of the Garage prefix: count 175582, bytes 238773274, equal to R2's usage API; parity 19 of 19 requests identical on `fr1` and on `prd` |
| material-cdn | wasm modules (hb-subset, WOFF2 encoder) | Workers | workerd wasm modules | works | `generated-icon-font`, `generated-text-font`, `generated-sprite` bodies have the same sha256 on both sides |
| material-cdn | Cache API | edge cache | workerd cache service on disk | works | `generated-sprite-again` identical; probe `/cache` returns `cached-body` |
| material-cdn | side effect: object written by a build | R2 `v1/gen/<hash>` | Garage `fw-edge-r2/aphrody-material/v1/gen/<hash>` | works | one fresh `?text=DualMode...` request to both: same `Content-Location` `/v1/g/7391ee10914321146386.woff2`, same body sha256 `81b9397e1667`, object 2712 bytes with MD5 `ffdece1ac28f` in R2 and in Garage |
| fw-edge | R2 `DRIVE_CACHE`, `FW_DRIVE_BLOBS` | R2 | S3 bridge | wired | `fw-workerd bindings --wrangler crates/fw-edge/wrangler.jsonc` |
| fw-edge | D1 `FW_DRIVE_DB` | D1 | workerd `d1.js`, SQLite file | wired | probe `/d1`: `changes 1`, `lastRowId 1`, batch count 2, `first "x"`, `raw` rows |
| fw-edge | Durable Object `FW_OS` (`FwOSAgent`, SQLite) | DO | workerd DO, SQLite file | wired | probe `/do` counter survives requests (`count 1` then `2`) |
| fw-edge | assets `ASSETS` (`run_worker_first`) | Workers assets | workerd `assets` service | wired | compiled into `<name>-assets` with `userWorker = main` |
| fw-edge | `ai` `AI`, `version_metadata` | Workers AI, metadata | none | missing | reported under `unsupported` |
| ay-edge | R2 `RELEASES`, `ASSETS`, `VIDEO`, `CHROMIUM`, `DIST`, `BUN_LIB` | R2 | S3 bridge | wired | buckets total 912,904,604 + 379,996,937 + 19,875,545 + 2,202,019,058 + 18,328,252,275 + 282,593,457 bytes in R2 (usage API); `fr1` had 508 GB free under `/srv/xbuild` when `df` ran |
| ay-edge | services `SVC_APHRODY`, `SVC_AUTO`, `SVC_SHENRON_CDN` | service bindings | external service to the peer's loopback port | wired | resolved to the peer's loopback port once the peer is declared in `dual-mode.toml`; none of the three is declared yet, so the compiler reports them under `unsupported` until then |
| auto (`ay/apps/auto-server`) | R2 `FILES`, `BACKUPS`, `RELEASES`; D1 `DB`; DO `COMPANY_DO`; assets; cron `0 6 * * *` | yes | S3 bridge; `d1.js`; DO; assets; `cron.js` | wired | probe: D1, DO, cron (`/cron/seen` returned `{"ran":true}` within 60 s) |
| auto | `ai` `AI`; workflow `AUTOMATION_WORKFLOW` | Workers AI, Workflows | none (workerd bundles `workflows.js`, not wired here) | missing | `unsupported` |
| aphrody-worker (the site backend) | R2 x2; Queue producer `EVENTS` (`github-events`); cron `*/15 * * * *` | yes | S3 bridge; `queues.js`; `cron.js` | wired | probe `/queue` sent 3 messages, the consumer saw `[1,2,3]` |
| aphrody-web (`env web`) | R2 `RELEASES_BUCKET`, `ASSETS_BUCKET` | R2 | S3 bridge | wired | `bindings --env web` |
| any | vars, secrets | vars, secrets | text and `fromEnvironment` bindings | works | probe `/vars` returns `hello` |
| any | KV | KV | `kv.js`, SQLite file and blob disk | works | probe `/kv`: value, metadata, JSON read, list, delete |
| any | R2 API surface | R2 | `r2.js` | works | probe `/r2`: put with `httpMetadata` and `customMetadata`, head, range read (`wo-`), list with prefix and delimiter, delete of an array |
| any | R2 multipart upload | R2 | bridge maps to S3 multipart | wired | not exercised: no object above the 5 MiB part size was put |
| any | R2 conditional put (`onlyIf`) | atomic | HEAD then PUT, not atomic | differs | Garage has no conditional writes |

### Routes

| Hostname and pattern | Worker | Cloud | Self-hosted | Status |
| --- | --- | --- | --- | --- |
| `material.aphrody.com` (custom domain) | material-cdn | live | `127.0.0.1:8811` on `fr1`, unit `fw-workerd-material-cdn.service` active | works on the node, not exposed |
| `fw.aphrody.com` | fw-edge | live | not declared in `dual-mode.toml` | missing (needs `worker-build` output and the `ai` binding) |
| `releases.aphrody.com/*`, `downloads.aphrody.com/*`, `aphrody.com/download*`, `assets.aphrody.com/*`, `aphrody.com/r2-assets/*`, `wasm.aphrody.com/*`, `docs.aphrody.com/*`, `aphrody.com/github`, `auto.aphrody.com`, `cdn.dragonballfr.com/*`, `assets.aphrody.app/*`, `video.aphrody.app/*`, `dist.aphrody.app/*`, `bun-lib.aphrody.app/*` | ay-edge | live | not declared | wired (bindings), no data seeded |
| `auto.aphrody.com` | auto | live | not declared | wired |
| `web.aphrody.com/*`, `aphrody.com/web*` | aphrody-web | live | not declared | wired |

No self-hosted route faces the network: a hostname reaches a node only through the fleet ingress (the `aphrody` tunnel or nginx on `prd`), which belongs to the estate lot (DNS and tunnels).

## Measured parity of material-cdn

`fw-workerd parity --spec crates/fw-workerd/parity/material-cdn.toml --cloud https://material.aphrody.com --self http://127.0.0.1:8811` on `fr1`: 19 requests, 19 identical. The same spec against `prd` (its own workerd under systemd and its own Garage, seeded from `fr1`'s copy with rclone: 227.717 MiB, the same size as the source) through an ssh tunnel from `fr1`: 19 of 19 identical. They cover the index and catalogue JSON (1,948,215 bytes), CSS, tokens, an icon, a sprite (1,893,739 bytes), a 4,001,808-byte WOFF2, a `Range: bytes=0-1023` (206, 1024 bytes), `HEAD`, a matching and a stale `?v=` pin (200 and 404), a 404, a CORS preflight (204), and five generated outputs built by the wasm subsetter (sprite twice, icon font, text font, a POST). The first run showed one real difference, `accept-ranges` missing from Cloudflare's 206; it is now an explicit `ignore_headers` entry.

On `prd` the latency of the same five requests is 16.4 to 89.8 ms p50 through the tunnel (variable font 89.8 ms against 38.3 ms cloud) and is not comparable with the loopback figures above: it includes the ssh hop and a host at a load average above 30.

Probe Worker on `fr1` (`fw-workerd probe`, compiled and served by workerd): `/vars`, `/r2`, `/d1`, `/kv`, `/do` (twice), `/cache`, `/queue` then `/queue/seen` (`[1,2,3]` after the consumer ran) and `/cron/seen` (`{"ran":true}`) all answered as the Cloudflare API specifies. The probe has no cloud twin (deploying it would add a Worker to the production account), so those rows are probe-proved against the documented API, not diffed against Cloudflare.

## Latency and cost

Latency (parity run, 15 samples per request and side, `fr1`; the cloud figure includes the internet round trip from the node, the self-hosted figure is loopback and excludes the fleet ingress hop):

| Request | Cloud p50 / p95 | Self-hosted p50 / p95 |
| --- | --- | --- |
| `/v1/meta.json` (9,235 bytes) | 28.8 / 61.9 ms | 5.7 / 8.7 ms |
| `/v1/fonts/material-symbols.css` | 25.7 / 36.5 ms | 6.3 / 10.0 ms |
| `/v1/icons/outlined/home.svg` | 22.8 / 29.3 ms | 5.1 / 7.0 ms |
| `/v1/fonts/material-symbols-outlined/variable.3ae08513.woff2` (4,001,808 bytes) | 43.2 / 50.9 ms | 13.1 / 16.7 ms |
| `/v1/fonts/google-sans-flex.woff2?text=Aphrody&wght=400..700` (cached build) | 23.2 / 29.2 ms | 4.2 / 8.3 ms |

CPU per request of the self-hosted Worker, from the unit's `CPUUsageNSec` before and after N requests (`fr1` was at a load average above 80 from other builds, so wall time is not reported): static SVG 1.39 ms (n=300), CSS 1.63 ms (n=300), a text-font build 119.0 ms (n=15), a sprite build 6.79 ms (n=30). Resident memory of the service after the runs: 186,023,936 bytes. Cloud side, Workers GraphQL for `material-cdn`, last 24 hours: 733 successful requests, CPU time p50 0.561 ms, p99 207.2 ms, mean 13.1 ms (9,636,920 us in total).

Cost per request. Cloud, with the rates of `ay/packages/cf-control-plane/cf.toml` and applying only beyond the included quotas: 0.30 USD per million requests, 0.02 USD per million CPU-ms, 0.36 USD per million R2 class B reads, so about 6.7e-7 USD for a request that reads R2 at 0.56 ms CPU; inside the included quotas the marginal cost is zero. Self-hosted: the node is paid either way. Priced by CPU time, `fr1` (OVH catalogue price for VPS-4 2026, renew, 36.99 EUR per month before tax, 12 vCPU: 1.19e-6 EUR per vCPU-second) costs 1.7e-9 EUR for a 1.39 ms static request and 1.4e-7 EUR for a 119 ms build; `prd` (Hostinger KVM 8, 64.99 EUR per month from the billing API, 8 vCPU: 3.13e-6 EUR per vCPU-second) is 2.6 times dearer per CPU-second and carries production. The self-hosted price is a fully loaded upper bound only at saturation; at today's 733 requests per day it is a rounding error against the fixed monthly price.

## Failover

Design, nothing public is switched:

1. DNS: the hostname is a Worker custom domain in cloud mode and a proxied CNAME to a tunnel hostname in self-hosted mode; the estate file of the edge-estate lot owns the record, so a flip is one reviewed change that `fw estate plan` shows. A tunnel ingress rule points the hostname at `127.0.0.1:<port>` of a node.
2. Gate before any flip: `fw-workerd parity` against both URLs must exit 0 for the service's spec.
3. Worker routing (no DNS change): the fronting Worker (`ay-edge`) can try the self-hosted origin with `fetch` and fall back to its own R2 binding, or the reverse; this needs the node reachable through the tunnel and is not implemented.

Not proven: no record, route or tunnel rule was changed, because DNS and tunnels are the estate lot's. Cloudflare Load Balancing would automate health-based failover but is a paid product and is not used.

## CI

Gap: no self-hosted GitHub runner is registered for `aphrody-labs/fw` (`gh api repos/aphrody-labs/fw/actions/runners` is empty), and a hosted runner cannot reach a node's loopback, so a workflow cannot diff both modes without opening the nodes to the internet. Today the suite runs on demand on the node: `bun tools/fr1.ts run -- cargo run -p fw-workerd -- parity --spec crates/fw-workerd/parity/material-cdn.toml --cloud https://material.aphrody.com --self http://127.0.0.1:8811`. `cargo test -p fw-workerd` (15 tests: wrangler parsing, config rendering, units, parity comparison) runs in the workspace gate; workspace `cargo clippy -p fw-workerd --locked --all-targets -- -D warnings` passed on `fr1`.

## Open

- prd runs the unit `fw-workerd-material-cdn.service` and its own Garage (`fw-edge-garage.service`), both enabled; the `prd` Garage has no data other than `aphrody-material`.
- Seed and declare the other services: `fw-edge` (needs `worker-build` and an answer for `ai`), `ay-edge`, `auto`, `aphrody-worker`, `aphrody-web`, `shenron-cdn`; each needs its bundle (`wrangler deploy --dry-run --outdir`) and its buckets copied with rclone (`aphrody-dist` is 18.3 GB, `aphrody-chromium` 2.2 GB).
- `workflows` (workerd bundles an engine), `ai`, `version_metadata`: not wired.
- Expose a hostname through the fleet ingress and run the parity suite through it (estate lot).
- Atomic R2 conditional puts need a store with conditional writes; the bridge checks with a HEAD first.
- The `fw edge host` command and the `fw-bin` dispatch are not wired.
