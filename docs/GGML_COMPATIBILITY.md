# ggml and llama.cpp compatibility

## Verified `ggml-org` Ecosystem (2026-11-19)

All 26 `github.com/orgs/ggml-org/repositories` (`gh api orgs/ggml-org --jq .public_repos`) were scanned live; stargazer counts are not recorded here, read them with `gh api repos/<repo> --jq .stargazers_count`:
- `ggml-org/llama.cpp`: Reference LLM inference engine & `llama-server`
- `ggml-org/whisper.cpp`: Speech-to-text ASR on `ggml`
- `ggml-org/ggml`: Core tensor library with 18 hardware backends (`ggml-webgpu`, `ggml-cuda`, `ggml-cpu`, `ggml-rpc`, `ggml-vulkan`, `ggml-metal`, `ggml-opencl`, `ggml-sycl`, `ggml-hip`, `ggml-blas`, `ggml-openvino`, `ggml-cann`, `ggml-hexagon`, `ggml-musa`, `ggml-virtgpu`, `ggml-zdnn`, `ggml-zendnn`, `ggml-et`)
- `ggml-org/convert`: Automated HuggingFace/Safetensors -> GGUF v3 converter
- `ggml-org/llama-connect` (Rust): Rust-native client bridge

## What `crates/fw-ggml` implements

1. **4D Strided Tensors (`GgmlTensor`)**: Matches `struct ggml_tensor` (`ne[4]` element dimensions, `nb[4]` byte strides). `GgmlType` covers every live `enum ggml_type` of `ggml-org/ggml` master (`GGML_TYPE_COUNT = 44`, 36 live ids): `F32=0`, `F16=1`, `Q4_0=2`, `Q4_1=3`, `Q5_0=6`, `Q5_1=7`, `Q8_0=8`, `Q8_1=9`, K-quants `Q2K..Q8K=10..15`, I-quants `IQ2_XXS=16`, `IQ2_XS=17`, `IQ3_XXS=18`, `IQ1_S=19`, `IQ4_NL=20`, `IQ3_S=21`, `IQ2_S=22`, `IQ4_XS=23`, scalars `I8..I64=24..27`, `F64=28`, `IQ1_M=29`, `BF16=30`, ternary `TQ1_0=34`, `TQ2_0=35`, `MXFP4=39`, `NVFP4=40`, `Q1_0=41`, `Q2_0=42`, `F8_E4M3=43`. Removed ids (4, 5, 31-33, 36-38) are rejected. Block sizes and bytes per block (`block_size()`, `type_size()`, `row_size()`) are asserted in unit tests against `sizeof(block_*)` from `ggml-common.h`.
2. **Static DAG Computation Graph (`GgmlGraph`)**: Implements `ggml_context` + `ggml_cgraph` topological evaluation (`Add`, `Mul`, `MulMat`, `RmsNorm`, `SoftMax`, `Silu`, `Rope`) and `ggml_backend_sched` multi-backend graph splitting (`CudaSm89`, `WebGpu`, `Vulkan`, `Metal`, `CpuSimd`, `RpcRemote`, `TpuPjrt`).
3. **GGUF v2/v3 Binary Format**: Exact magic `GGUF`, version 2/3, `general.alignment` (default 32), full metadata KV (all 13 value types, arrays), tensor-info table with every `GgmlType`, tensor data slicing and an encoder (`parse_gguf`, `encode_gguf`). Sidecar files resolved next to a model follow `llama.cpp` `common/download.cpp` (`GgufSidecar`: `mmproj`, `mtp-`, `dflash-`, `eagle3-`, `dspark-` path keywords). Big-endian files are not read.
4. **`ggml-rpc` opcodes and a loopback pair**: `RpcCmd` lists every opcode of `enum rpc_cmd` in `ggml/src/ggml-rpc/ggml-rpc.cpp` at protocol 10.0.1 (`RPC_PROTO_VERSION`; read from `ggml-org/llama.cpp` master with `gh api` on 2026-11-21, `RPC_CMD_HELLO` is 14). Framing is `u8 cmd + u64 len + payload`. The loopback server answers `GetDeviceMemory` and `GetAlignment` only and never performs the `Hello` handshake, so it does not interoperate with `rpc-server`: the interop client is open work.
5. **Cluster routing (`crates/fw-cluster`, `epp`)**: llm-d EPP model: composable `Filter` -> weighted `Scorer` -> `Picker` profiles (prefix-cache affinity, token load, queue depth, capability), a P/D handler running a prefill profile then a decode profile, and a block-hash KV index fed by `observe` or `KvEvent`. Source semantics: `llm-d/llm-d-router` `pkg/epp/framework/plugins/scheduling`.
6. **`llama.cpp` binding (`crates/fw-llama-sys`)**: vendored headers pinned to the release tag in `include/LLAMA_CPP_TAG` (`LLAMA_CPP_TAG`), refreshed by `bun sync-headers.ts [tag]`.
