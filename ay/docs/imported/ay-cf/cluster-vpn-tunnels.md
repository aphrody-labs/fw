# Hybrid Cluster, Proxy VPS & Mullvad Split-Tunneling

## Node Roles

| Role | Provider | Mesh Address | Stack |
| :--- | :--- | :--- | :--- |
| `build` | OVH #1 | `10.200.77.1` | 12-core Rust/C++/Bazel/Bun CI runner, `sccache`, NativeLink CAS/Scheduler |
| `edge_proxy` | OVH #2 | `10.200.77.3` | OVH Anti-DDoS front running Pingora/Nginx L4/L7 proxy, Garage cold S3, Valkey |
| `origin` | Hostinger | `10.200.77.5` | Production services locked behind `nftables` (WireGuard + `cloudflared` QUIC only) |

## Mullvad VPN UID Policy Routing (`[vpn]`)

Generated via `just cluster-proxy` into `.private/generated/mullvad-uid-routes.sh`:

- Each Mullvad exit binds exclusively to a dedicated system user (`vpn-us` `uid=2001` table `51821`, `vpn-jp` `uid=2002` table `51822`, `vpn-eu` `uid=2003` table `51823`).
- `ip rule add to 10.200.77.0/24 lookup main priority 50` runs at higher priority than `uidrange` rules so cluster WireGuard traffic, `cloudflared` QUIC tunnels, and SSH never enter Mullvad tunnels.

## Generating Live Configs

```sh
just cluster-proxy
just tunnels
```
