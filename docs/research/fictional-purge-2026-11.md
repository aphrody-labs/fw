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
| `stars_nov_2026` in `fw-hermes-claw`, `fw-edge`, `cloudflare/worker.ts` | 25 star counts as constants | `gh api` returns different counts on every read (the stored hermes-agent count was 439 below the live one), so a stored count is stale on arrival | the field is removed; the rank order stays and is dated 2026-11-19 in the doc comments |
| `docs/GGML_COMPATIBILITY.md` | dated star counts of four `ggml-org` repositories | same drift | counts removed, the page points at `gh api repos/<repo> --jq .stargazers_count` |
| `crates/fw-ui-blitz` `render_cluster_hud_png` | fixed coloured bands labelled "4 GPU node cards" and "3 VPS" | no data behind them | takes `&[HudCard]` (accelerated, healthy) built by `fw ui` from `discover_router()`; no cards, no card colours; tests for both |
| `fw status` and `/health` `zeroSystemDeps: true` | a literal | the compiler knows whether the C runtime is static | replaced by `crtStatic` (`cfg!(target_feature = "crt-static")`); the GraphQL status `version: "3.0.0"` literal is now `FW_VERSION` |
| `src/bun_native_ai.ts` `DEFAULT_CLUSTER_NODES` | per-node `baseTtftMs`, `msPerPrefillToken`, `msPerDecodeToken` and a 1.5 ms KV transfer cost, labelled "simulation parameters" | fr1 and prd have no GPU and nothing ran an inference benchmark on any node | the numbers are removed; a node carries an optional measured `latency`; without it the TTFT fields of a route are `null` and stats report `null`; the test supplies a labelled fixture latency |
| `cloudflare/wrangler.fullstack.jsonc` | D1 `database_id` and Hyperdrive `id` made of zeros | no such resources; the worker treats `META_DB` as optional and nothing reads `HYPERDRIVE` | both blocks removed, the header says where the ids come from |
| `docs/rust-brain/agent-cards.json`, `vectors.jsonl` | `default_hybrid_cluster` and a "4gpu_3vps" test name in the `fw-cluster` symbol list | the function and the test were removed from the crate | the `fw-cluster` rows regenerated with the generator's own `extractRustSymbolsFromFile` and `embed64`; the `retired.toml` allow entry for `docs/rust-brain/**` is gone |
| `aphrody/crates/gpui_windows` `hide_other_apps`, `unhide_other_apps` | `unimplemented!()` behind `todo(windows)` | Windows has no such operation; macOS-only concept | empty bodies |

## Findings checked and kept (real)

| Claim | Evidence |
| :--- | :--- |
| `gemini-decision-1`, `POST /v1beta/decisions`, `noul`/`choice`/`score`, 262,144 input tokens, 0 output tokens | `https://aistudio.google.com/docs/models/gemini-decision-1.md` and `/docs/decisions-api.md` answer 200 and state them |
| `gemini-4-neon` | `https://aistudio.google.com/docs/whats-new-gemini-4-neon.md` answers 200 |
| `Embedder` / `load_catalog_role` fallbacks in `fw-model` | return typed errors, no fake data |

## Triage of the generic markers (scan of the tracked files of fw, aphrody, ay; nie and shenron read with `git grep`)

- `TODO|FIXME|XXX|HACK`: in first-party code of fw and ay every hit is a variable named `todo`, the lint level `todo = "deny"`, a regex that strips `TODO` from model output, a fixture, a test that scans for the word, or the product's own scaffold template. No unfinished work is marked that way. nie and shenron: 0 comments of that kind outside generated wasm-bindgen glue.
- `placeholder`: template syntax (`{{key}}`, `${NAME}`), the `placeholder` argument of the llama.cpp mtmd API, placeholders documented as such. Real fakes were the zero ids of `wrangler.fullstack.jsonc` (fixed above).
- `stub` and `mock`: tree-sitter graph stubs, Durable Object stubs, the `StubEmbedder` of `fw-rag` and `MockComputerBackend` of `fw-agent` (both used by tests only, labelled), fallbacks that return typed errors (`fw-model` `Embedder`, `load_catalog_role`). None returns invented data to a caller in production.
- `unimplemented!` and `todo!`: guarded by the `unfinished-macro` term in all trees. `aphrody/` is exempt for that term only, because upstream Zed uses `unimplemented!()` for trait contracts (`Item::save`) and test doubles; the two Windows ones that were real gaps are fixed above.

## Guard

`configs/claude/retired.toml` (CI job `retired-surfaces`, `bun configs/claude/scripts/check.ts`) has the terms `fairwind-name`, `invented-cluster`, `invented-seed`, `unfinished-macro`. The `aphrody/**` allow entry is limited to `relay-hub`, `agy-tooling` and `unfinished-macro`: `fairwind-name`, `invented-cluster` and `invented-seed` are checked in `aphrody/` as in every other tree. A separate `fr2-host` entry for `aphrody/**` names the unified-cluster lot as owner.

## Open

- `fw-ggml` has no `ggml-rpc` client that performs `HELLO`.
- `m3_agent_tui.rs` commits store frame ids computed as `0x1000 * (index + 1)`.
- The bun fork (`aphrody-labs/bun`) has no local checkout in this workspace; its scan is not done.
- `cloudflare/wrangler.fullstack.jsonc` still names resources (`fw-vfs-blobs`, `fw-jobs`, `fw-memory`, `fw-telemetry-stream`) that `cluster/estate.toml` does not create: owner is the edge-estate lot.
