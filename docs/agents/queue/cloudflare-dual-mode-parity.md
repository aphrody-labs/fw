# Lot: Cloudflare dual mode, self-hosted and cloud, with proven parity

Order of the owner, 2026-11-21: every Cloudflare service of the stack runs both on Cloudflare and self-hosted on our own compute. The two modes are proven equivalent. fw drives our custom compute first: fr1 and prd as described in `cluster/build.toml` and `cluster/topology.toml`, then Cloudflare.

## Services

| Service | Repo | Notes |
|---|---|---|
| fw-edge | fw `crates/fw-edge` | workers-rs |
| material-cdn | fw `cloudflare/material-cdn` | |
| ay-edge | ay `apps/edge` | with every routed service |
| auto-server | ay | |
| the site | | |
| web.aphrody.com | | |
| releases.aphrody.com | | |

Bindings each service uses: R2, D1, KV, Queues, Durable Objects, cron.

## Self-hosted mode

- **Runtime:** workerd, the open-source Workers runtime, on fr1 and prd under systemd.
- **Storage:**
  - R2 is backed by an S3-compatible store on our disks, Garage or MinIO; pick the best maintained;
  - D1 by SQLite files;
  - KV and Queues by the workerd local implementations or a self-hosted equivalent.
- **Config:** the same wrangler config, compiled to a workerd config by a fw command.
- **TLS and routing:** through the existing fleet ingress.

## Parity

- **Matrix:** one row per binding and per route, cloud against self-hosted. It lives in `docs/research/cloudflare-dual-mode.md`.
- **Test:** a test suite sends the same requests to both and diffs bodies, headers and side effects. It runs in CI against both.
- **Measures:** latency and cost per request in both modes.
- **Failover:** DNS or Worker routing can send traffic to either mode.
