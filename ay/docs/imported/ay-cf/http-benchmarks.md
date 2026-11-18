# HTTP Client Benchmarks (`xh` vs `curl` vs `wget2` vs `bun fetch`)

Reproducible latency benchmark against `https://1.1.1.1/cdn-cgi/trace` (5 runs per client) via `just bench-http` (`tools/bench-http.ts`).

## Linux (`fr1`, x86_64)

| Tool | Available | OK | Min (ms) | p50 (ms) | p95 (ms) | Mean (ms) | Notes |
| :--- | :---: | :---: | ---: | ---: | ---: | ---: | :--- |
| `bun-fetch` | yes | 5/5 | 11.44 | 22.81 | 35.58 | 21.22 | Bun 1.4.5 native `fetch` (in-process keep-alive/JIT) |
| `wget2` | yes | 5/5 | 47.00 | 47.54 | 57.19 | 49.35 | C `wget2` HTTP/2 |
| `curl` | yes | 5/5 | 63.97 | 80.40 | 88.26 | 76.82 | C `libcurl` HTTP/2 |
| `xh` | yes | 5/5 | 73.87 | 80.09 | 132.71 | 88.59 | Rust `reqwest`/`hyper` (`rust/xh`) |
| `curl-http3` | no | 0/0 | 0 | 0 | 0 | 0 | `curl` binary lacks HTTP/3 |

## Windows (workstation, x86_64)

| Tool | Available | OK | Min (ms) | p50 (ms) | p95 (ms) | Mean (ms) | Notes |
| :--- | :---: | :---: | ---: | ---: | ---: | ---: | :--- |
| `curl` | yes | 5/5 | 57.15 | 58.52 | 62.14 | 59.16 | C `libcurl` HTTP/2 |
| `bun-fetch` | yes | 5/5 | 75.14 | 77.39 | 221.08 | 106.33 | Bun 1.4.5 native `fetch` |
| `xh` | yes | 5/5 | 135.01 | 152.27 | 414.30 | 240.10 | Rust `reqwest`/`hyper` (`rust/xh`) |
| `curl-http3` | no | 0/0 | 0 | 0 | 0 | 0 | `curl` binary lacks HTTP/3 |
| `wget2` | no | 0/0 | 0 | 0 | 0 | 0 | Not installed on host |

## Takeaways

- **In-process scripts / Workers**: `bun-fetch` eliminates process-spawn overhead (11.44 ms min on Linux).
- **CLI / ergonomic HTTP**: `xh` (`rust/xh`) provides Rust memory safety and HTTPie-compatible UX, within ~10 ms of `curl` on Linux p50 (80.09 ms vs 80.40 ms).
- **HTTP/3**: Stock `curl` on both hosts lacks `--http3-only`; `quiche` (`rust/quiche`) remains the primary HTTP/3 engine in `ay-cf`.
