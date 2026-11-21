# Architecture

The Cargo workspace (`Cargo.toml`) builds two artifacts:

1. `target/release/fw` (`crates/fw-bin`), the command-line binary. `.github/workflows/ci.yml` checks it for `x86_64-unknown-linux-musl`.
2. `target/wasm32-unknown-unknown/release/fw_wasm.wasm` (`crates/fw-wasm`), loaded by the Cloudflare Worker and by browser peers.

## Cluster model

`crates/fw-cluster` routes requests over the hosts of `cluster/build.toml` (`HybridClusterRouter::discover`, found through `$FW_ROOT` or the working directory's ancestors) plus the local machine as `fw-compute` measures it; `fw status` reports exactly those nodes. Kubernetes nodes enter through `fw cluster k8s`.

- Proxy choice: consistent hash with health state, written in `fw-cluster` (Pingora is not a dependency).
- Scheduling: KV-cache prefix affinity and prefill/decode disaggregation (`crates/fw-cluster/src/epp.rs`, modeled on `llm-d-router`).
- Browser peers register with `POST /cluster/peers` (`peerId`, `vramGb`, `cpuCores`) on `fw serve`. There is no WebRTC or WebSocket transport.
- Cloudflare: `cloudflare/worker.ts` (`FwOSAgent` Durable Object on SQLite). `src/decisions.ts` serves `/v1beta/decisions` and forwards to the Gemini API `gemini-decision-1` when `GEMINI_API_KEY` is set.

## Crates

| Crate | Responsibility |
| :--- | :--- |
| `fw-ggml` | Tensor and graph IR, GGUF v2/v3 reader and writer, Q8_0 and Q4_0 dot kernels, `ggml-rpc` wire protocol |
| `fw-compute` | Hardware and OS probe, WGSL kernels, compute smoke benchmark |
| `fw-cluster` | Request router described above |
| `fw-proto` | HTTP/1.1 server: control-plane UI (`crates/fw-proto/ui`), GraphQL, OpenAPI 3.1, OpenAI-compatible chat, MCP, ACP, A2A, `/webgpu` |
| `fw-ui-blitz` | Software RGBA rasterizer and PNG encoder; no blitz, vello or GPU |
| `fw-wasm` | `wasm32-unknown-unknown` C-ABI functions |
| `fw-bin` | The `fw` binary |

Other crates are listed by `cargo metadata --no-deps --format-version 1`; per-directory tiers and gates are in `MAP.tsv`.
