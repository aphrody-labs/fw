# Cloudflare & Hybrid Infrastructure Architecture

`ay-cf` governs one Cloudflare Paid account (`Workers Paid` + `R2 Paid` + `Observability`), four DNS zones (`aphrody.com`, `aphrody.app`, `dragonballfr.com`, `rosegriffon.fr`), three HA `cloudflared` QUIC tunnels (`aphrody`, `rosegriffon`, `shenron`), eleven R2 buckets, and three Linux cluster nodes connected over a private WireGuard mesh (`10.200.77.0/24`).

## Zones & Workloads

| Estate | Zone | Role | Origin Strategy |
| :--- | :--- | :--- | :--- |
| Aphrody OS & Portal | `aphrody.com` | OS downloads, docs proxy, API, MCP gateway | Workers + R2 Custom Domains + `aphrody` tunnel |
| Aphrody WebOS & Apps | `aphrody.app` | WebOS apps, Craft studio, reusable R2 CDNs (`assets.`, `dist.`, `video.`, `bun-lib.`) | R2 Custom Domains + Workers |
| Shenron | `dragonballfr.com` | Portal, Discord/AI bot, MCP, asset CDN | `shenron` HA QUIC tunnel + OVH Pingora shield |
| Rose Griffon | `rosegriffon.fr` | Community site, API, Achillea ranked, bot, MCP, CDN | `rosegriffon` HA QUIC tunnel + `r2.rosegriffon.fr` |

## Submodule Groups (`forks.toml`)

71 `aphrody-labs/*` forks of official `cloudflare/*` repositories are indexed via `.gitmodules` and `160000` gitlinks under `runtime/`, `sdk/`, `agents/`, `rust/`, `python/`, and `net/`.
