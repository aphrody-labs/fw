# Fictional, placeholder and hallucinated content purge (2026-11-21)

One row per finding. "Fix" names the change (one commit); every number below was read from a command on 2026-11-21.

## Findings fixed

| Where | It claimed | What is true (source) | Fix |
| :--- | :--- | :--- | :--- |
| `cluster/topology.toml` | 4 GPU hosts (PC RTX 4070, gcloud L4, T4, TPU burst), 3 VPS, `10.66.0.x` RPC addresses, "documentation only" | the fleet is `fr1` (12 vCPU, 49.3 GB, no GPU) and `prd` (8 vCPU, 33.6 GB, no GPU), mesh `10.200.77.0/24` (`nproc`, `free -b`, `cluster/build.toml`); `ls /dev/nvidia*` fails on both | file deleted |
| `crates/fw-cluster/src/lib.rs` `default_hybrid_cluster` | the same hardcoded 7 nodes | nodes are read from `cluster/build.toml` plus the measured local machine | replaced by `HybridClusterRouter::discover`, `from_build_cluster`, `local_node`; tests read the real file |
| `fw status` `gpuHosts` | 4 | derived: 1 only when `fw-compute` finds CUDA on the local machine | `crates/fw-bin/src/main.rs`, `tests/fw_v3_cluster_webgpu.test.ts` |
| `fw status` / `summary()` router string, `strategy` | "pingora + ai-dynamo + vllm-router" | none is a dependency; the router is consistent hash plus an llm-d style filter/score/pick | strings corrected |
| `cluster/cloudflare-gcloud.toml` | bucket `gs://fw-models-gguf`, R2 `fw-gguf-cas`, TPU enabled, GPU burst pool of three fake nodes | `gcloud storage ls gs://fw-models-gguf` answers 404; `wrangler r2 bucket list` has no `fw-gguf-cas`; nothing read the file; the Worker and DO live in `cloudflare/wrangler.jsonc` and `cluster/estate.toml` | file deleted |
| prd systemd `fairwind-build-worker.slice`, `fairwind-build.slice`, `fairwind.slice` | the name Fairwind (an invented project name of the agy period, 2026-11-19; the units were `/etc/systemd/system/fairwind-build-worker.slice`) | live on prd: `nativelink-worker` and `sccache-dist-server` run in `fw-build.slice` (the `[policy]` slice of `cluster/build.toml`, written by `build-cluster.ts apply`); the Fairwind slices are stopped and removed; the Actions runner unit file names `fw-build.slice` and joins it at its next restart (a job was running) | live change on prd; `cluster/build.toml` and `build-cluster.ts` already carried the new slice upstream; `FAIRWIND_ROOT` fallback removed from `src/fw_cli.ts`; skills corrected |
| `m3_agent_tui.rs`, `fw-ostd-wasm` | a fake "superblock" block and two fake XArray pages seeded into empty stores, `webgpu_virtio_ready: true`, `wasm_target_tuned: true` | the stores were empty; the flags were constants | seeds and constant fields removed; `wasm_target_tuned` is `cfg!(target_arch = "wasm32")` |
| `fw-ggml` `RpcCmd`, `docs/GGML_COMPATIBILITY.md` | opcodes "matching" `ggml-rpc.cpp`, "interoperable with upstream `rpc-server`" | upstream `enum rpc_cmd` (ggml-org/llama.cpp master, protocol 10.0.1 from `include/ggml-rpc.h`) has `SET_TENSOR_HASH` at 7, `GET_DEVICE_MEMORY` at 11 and a mandatory `HELLO` at 14; the loopback pair never says hello | enum rewritten from upstream, docs state the loopback has no interop |
| `core/py/fw_py_ai/migrate_vllm_llmd_to_fw.py` | nodes with 24 GB VRAM, 64 GB RAM, 16 cores, `10.66.0.21`, rpc `50051+idx`, a "Pingora ingress" | the source configs carry none of it | unknown hardware stays 0, no invented rpc address, llm-d endpoints derive from the InferencePool name |
| `plugins/vllm-fw` default node id, `tests/test_fw_vllm_llmd.py` fixtures | `gpu-1-pc-rtx4070`, `gpu-2-gcloud-l4`, `10.66.0.x` | router's own node id is `local`; fixtures use RFC 5737 `192.0.2.x` and neutral ids | edited |
| `docs/HARDWARE_MATRIX.md`, `docs/ARCHITECTURE.md` | VPS 1 "1 TB disk", "3x remote GPU hosts", sample topology | measured table (`nvidia-smi`, `/proc/cpuinfo`, `df -B1`, `uname -r`) | rewritten |
| `src/bun_native_ai.ts` | `fr1` is "EPYC 12c" | fr1 is Intel Haswell 12 vCPU; prd is EPYC 9354P | strings corrected; latency numbers labelled as simulation parameters |
| `plugins/aphrody/skills/shenron-bun-mastery` | assets mirrored through "Pingora/Garage Edge Simulators" | Garage is real (shenron AGENTS.md); no Pingora simulator exists | line corrected |
| `tools/competitor_audit.ts` and its output | `fw-cluster` is Pingora/Dynamo/vLLM-Router | none is a dependency | wording corrected |

## Findings checked and kept (real)

| Claim | Evidence |
| :--- | :--- |
| `gemini-decision-1`, `POST /v1beta/decisions`, `noul`/`choice`/`score`, 262,144 input tokens, 0 output tokens | `https://aistudio.google.com/docs/models/gemini-decision-1.md` and `/docs/decisions-api.md` answer 200 and state them |
| `gemini-4-neon` | `https://aistudio.google.com/docs/whats-new-gemini-4-neon.md` answers 200 |
| star counts of `modern_cli_ranking` (dated `stars_nov_2026`) | `gh api repos/<repo> --jq .stargazers_count` returns values within 0.2 percent for all 25 repositories: snapshots, not inventions |
| `Embedder` / `load_catalog_role` fallbacks in `fw-model` | return typed errors, no fake data |

## Guard

`configs/claude/retired.toml` (CI job `retired-surfaces`, `bun configs/claude/scripts/check.ts`) gains the terms `fairwind-name`, `invented-cluster`, `invented-seed`, `unfinished-macro`. A tracked file outside an `allow` entry that matches fails the job. Scan of the tracked files outside `aphrody/` and `ay/` at base `cd2330a612`: 0 hits after the fixes; `git grep` of the same identifiers in `aphrody/` and `ay/` found none.

## Open

- prd: `fleet-build-worker.slice` (an intermediate name I created and then replaced by `fw-build.slice`) stays loaded until the Actions runner restarts; remove `/etc/systemd/system/fleet-build-worker.slice` afterwards.
- The `stars_nov_2026` constants of `fw-hermes-claw` and `fw-edge` are dated snapshots (live counts drift by up to a few hundred); read them live or drop them.
- `crates/fw-ui-blitz` `render_cluster_hud_png` paints fixed coloured bands labelled as "4 GPU node cards, 3 VPS": no data behind them.
- `fw status` `zeroSystemDeps` is a literal `true`.
- `src/bun_native_ai.ts` per-node latency parameters are not measurements.
- `fw-ggml` has no `ggml-rpc` client that performs `HELLO`.
- `m3_agent_tui.rs` commits store frame ids computed as `0x1000 * (index + 1)`.
