# Servers and infrastructure: feature and benchmark matrix

Retrieved 2026-11-16 from live sources. Dataset with per-fact source URLs: `servers-infra.toml` (same folder). "unknown" means no live source gave the value.

## Components

| Component | Latest release (date) | Licence | HTTP/1.1 | HTTP/2 | HTTP/3 | WebSockets | TLS 1.3 |
|---|---|---|---|---|---|---|---|
| hyper | 1.12.0 (2026-10-06) | MIT | yes | yes | no (separate hyperium/h3) | unknown | unknown |
| axum | 0.8.9 (2026-04-14) | MIT | yes | yes (feature `http2`) | no | yes (feature `ws`) | via rustls examples; level unknown |
| Apache httpd | 2.4.71 (2026-11-11) | Apache-2.0 | yes | yes (mod_http2) | no module in 2.4 index | yes (mod_proxy_wstunnel) | yes (OpenSSL 1.1.1+) |
| nginx | mainline 1.31.8 (2026-10-21), stable 1.30.6 (2026-10-14) | BSD-2-Clause | yes | yes | yes (since 1.25.0) | yes | yes |
| Bun.serve | 1.4.4 (2026-10-22) | MIT (JSC LGPL-2) | yes | experimental | experimental | yes | TLS yes; 1.3 level unknown |
| Caddy | 2.11.9 (2026-11-05) | Apache-2.0 | yes | yes | yes | yes | yes (default max tls1.3) |
| Docker Engine | 29.10.0 (2026-11-16) | Apache-2.0 | n/a | n/a | n/a | n/a | n/a |
| Docker Compose | 5.7.1 (2026-11-11) | Apache-2.0 | n/a | n/a | n/a | n/a | n/a |
| Podman | 6.2.0 (2026-11-10) | Apache-2.0 | n/a | n/a | n/a | n/a | n/a |
| systemd | 262.2 (2026-11-05) | LGPL-2.1-or-later | n/a | n/a | n/a | n/a | n/a |
| PostgreSQL | 18.7 (2026-11-12); 19 RC 2 not GA | PostgreSQL License | n/a | n/a | n/a | n/a | yes (via OpenSSL) |
| Redis | 8.10.3 (2026-10-26) | RSALv2 / SSPLv1 / AGPLv3 (8.0+) | n/a | n/a | n/a | n/a | TLS yes; level unknown |
| Valkey | 9.1.3 (2026-10-26) | BSD-3-Clause | n/a | n/a | n/a | n/a | TLS yes; level unknown |
| SQLite | 3.54.1 (2026-10-16) | Public domain | n/a | n/a | n/a | n/a | n/a |

Sources: GitHub releases API (hyperium/hyper, tokio-rs/axum, oven-sh/bun, caddyserver/caddy, docker/compose, podman-container-tools/podman, systemd/systemd, redis/redis, valkey-io/valkey, nginx/nginx), https://httpd.apache.org/download.cgi, https://nginx.org/en/download.html, https://docs.docker.com/engine/release-notes/, https://www.postgresql.org/, https://sqlite.org/index.html; protocols from https://github.com/hyperium/hyper/blob/master/README.md, https://github.com/tokio-rs/axum/blob/main/axum/Cargo.toml, https://httpd.apache.org/docs/2.4/mod/, https://nginx.org/en/docs/quic.html, https://bun.sh/docs/api/http, https://github.com/caddyserver/caddy/blob/master/README.md, https://caddyserver.com/docs/caddyfile/directives/tls. All retrieved 2026-11-16.

## Key features

- hyper: async HTTP/1 and HTTP/2 client and server library; low-level base of axum and warp.
- axum: macro-free routing, extractors, tower and tower-http middleware; tokio-based.
- Apache httpd: MPM event, modular (mod_http2, mod_ssl, mod_proxy_wstunnel).
- nginx: reverse proxy, QUIC and HTTP/3, njs scripting.
- Bun.serve: `routes` object, unix and abstract sockets, HTTP/3 over UDP with Alt-Svc (experimental).
- Caddy: automatic HTTPS (ACME), HTTP/1.1, 2 and 3 by default, Go module plugins.
- Docker Engine 29 and Compose 5: OCI containers and compose-spec multi-container apps.
- Podman: daemonless, rootless, pods, Netavark and pasta networking, Kubernetes ready.
- systemd: socket and D-Bus activation, cgroup tracking, dependency-based transactions, systemd-boot.
- PostgreSQL 18: asynchronous I/O subsystem, skip scan, OAuth authentication (https://www.postgresql.org/docs/18/release-18.html, 18.0 released 2025-09-25).
- Redis 8: io-threads, Query Engine, probabilistic structures. Valkey 9: 2,000-node cluster, pipeline prefetch, zero-copy replies, Multipath TCP.
- SQLite: ACID, zero-config, single file, under 900 KiB, 100% branch coverage.

## Benchmarks: TechEmpower Framework Benchmarks, latest round R23

Source: https://www.techempower.com/benchmarks/results/round23/ph.json, retrieved 2026-11-16. R23 was published 2025-03-03 (https://api.github.com/repos/TechEmpower/FrameworkBenchmarks/releases/tags/R23); the run is "Continuous Benchmarking Run 2025-01-30 18:47:41", window 2025-01-30 to 2025-02-06, environment "Citrine" (new hardware). No later round exists (round24 JSON returns 404) and the repository is archived. Values are peak req/s over the concurrency levels, computed as totalRequests / (endTime - startTime); rank is among all entries that reported the test.

| Test | Entry | req/s | Rank |
|---|---|---|---|
| JSON | hyper | 2,885,610 | 14 / 540 |
| JSON | axum | 2,709,795 | 34 / 540 |
| JSON | bun | 2,097,129 | 96 / 540 |
| JSON | nginx-njs | 2,096,981 | 97 / 540 |
| JSON | http4k-apache4 (Apache HttpCore, not httpd) | 916,241 | 217 / 540 |
| JSON | php-caddy (PHP-FPM behind Caddy) | 94,254 | 442 / 540 |
| Plaintext | hyper | 17,468,474 | 34 / 533 |
| Plaintext | axum | 12,048,256 | 64 / 533 |
| Plaintext | bun | 7,046,990 | 99 / 533 |
| Plaintext | nginx | 4,549,001 | 127 / 533 |
| Plaintext | nginx-njs | 2,920,487 | 156 / 533 |
| Plaintext | http4k-apache4 | 1,059,846 | 266 / 533 |
| Plaintext | php-caddy | 123,780 | 428 / 533 |
| Fortunes | axum-pg | 1,114,266 | 7 / 518 |
| Fortunes | hyper-db | 1,066,645 | 11 / 518 |
| Fortunes | php-caddy | 69,083 | 351 / 518 |
| Fortunes | axum-sqlx | 38,712 | 412 / 518 |
| Single query | axum-pg | 1,190,767 | 15 / 554 |
| Single query | axum-sqlx | 41,106 | 454 / 554 |
| Multiple queries | axum-pg | 1,243,171 | 10 / 535 |
| Updates | axum-pg | 421,458 | 27 / 484 |

Gaps: no stock Apache httpd entry, no standalone Caddy entry, no Bun DB-test entry: unknown. The axum variants differ by Postgres client (axum-pg vs axum-sqlx is a 29x gap in Fortunes), so the client matters more than the framework.

## Benchmarks: official database pages

- Redis (https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/benchmarks/): example SET 180,180.17 req/s p50 0.143 ms, LPUSH 188,323.91 req/s; pipelined -P 16 on a MacBook Air 11in SET 1,536,098.25 and GET 1,811,594.25 req/s. Redis 8 (https://redis.io/blog/redis-8-ga/, 2025-05-01): p50 latency down 5.4% to 87.4% over 7.2.5 in 149 tests; io-threads=8 gave up to 112% more throughput on multi-core Intel. Vendor-stated.
- Valkey (https://valkey.io/topics/benchmark/): example SET 74,239.05 and LPUSH 79,239.30 req/s. https://valkey.io/blog/1-billion-rps/ (2025-10-20): over 1 billion SET req/s on 2,000 r7g.2xlarge nodes with 750 c7g.16xlarge load generators. https://valkey.io/blog/introducing-valkey-9/ (2025-10-21): up to 40% from pipeline prefetch, up to 20% from zero-copy replies.
- SQLite (https://sqlite.org/fasterthanfs.html): about 35% faster than individual files for 10 KB blobs, about 20% less disk. https://sqlite.org/speed.html is a placeholder; the 2007 comparison was removed.
- PostgreSQL: no official published benchmark results found; pgbench is the bundled tool (https://www.postgresql.org/docs/current/pgbench.html). Value: unknown.

## Recommendation for Aphrody cloud and search

1. Edge: Caddy (HTTP/1.1, 2, 3 and ACME by default, Apache-2.0); nginx 1.31 mainline where QUIC tuning or existing configs matter. Apache httpd only for legacy modules.
2. Services: Rust axum on hyper (hyper 2.89M and axum 2.71M req/s JSON in R23, ahead of Bun 2.10M). Use Bun.serve for TypeScript surfaces only; its HTTP/2 and HTTP/3 are experimental.
3. Data: PostgreSQL 18 as system of record with a pipelined client (axum-pg style, not sqlx defaults, per the Fortunes gap); SQLite for embedded and per-node state; Valkey 9.1 for cache and queues, since Redis 8 is RSALv2/SSPLv1/AGPLv3 while Valkey is BSD-3-Clause.
4. Runtime: Podman rootless under systemd units for production; Docker Engine 29 with Compose 5 for developer machines and CI.
5. Caveat: R23 data is from Jan-Feb 2025 and the project is archived; use it for ordering and re-measure on <test-host> before fixing capacity numbers.
