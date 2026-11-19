# Fairwind v3.0 — `ggml-org` & `llama.cpp` Compatibility Specification

## Verified `ggml-org` Ecosystem (2026-11-19)

All 26 `github.com/orgs/ggml-org/repositories` were scanned live:
- `ggml-org/llama.cpp` (135,662★): Reference LLM inference engine & `llama-server`
- `ggml-org/whisper.cpp` (55,309★): Speech-to-text ASR on `ggml`
- `ggml-org/ggml` (16,118★): Core tensor library with 18 hardware backends (`ggml-webgpu`, `ggml-cuda`, `ggml-cpu`, `ggml-rpc`, `ggml-vulkan`, `ggml-metal`, `ggml-opencl`, `ggml-sycl`, `ggml-hip`, `ggml-blas`, `ggml-openvino`, `ggml-cann`, `ggml-hexagon`, `ggml-musa`, `ggml-virtgpu`, `ggml-zdnn`, `ggml-zendnn`, `ggml-et`)
- `ggml-org/convert` (70★): Automated HuggingFace/Safetensors -> GGUF v3 converter
- `ggml-org/llama-connect` (6★, Rust): Rust-native client bridge

## `introduction-to-ggml` Architectural Parity (`crates/fw-ggml`)

1. **4D Strided Tensors (`GgmlTensor`)**: Matches `struct ggml_tensor` (`ne[4]` element dimensions, `nb[4]` byte strides, `GgmlType` enum IDs matching `enum ggml_type` in `ggml.h`: `F32=0`, `F16=1`, `Q4_0=2`, `Q8_0=8`, `Q4K=12`, `BF16=30`).
2. **Static DAG Computation Graph (`GgmlGraph`)**: Implements `ggml_context` + `ggml_cgraph` topological evaluation (`Add`, `Mul`, `MulMat`, `RmsNorm`, `SoftMax`, `Silu`, `Rope`) and `ggml_backend_sched` multi-backend graph splitting (`CudaSm89`, `WebGpu`, `Vulkan`, `Metal`, `CpuSimd`, `RpcRemote`, `TpuPjrt`).
3. **GGUF v3 Binary Format**: Exact magic `GGUF`, version 2/3, 32-byte alignment, tensor count, and metadata KV count parsing/encoding.
4. **`ggml-rpc` Binary Wire Protocol**: Exact command opcodes (`AllocBuffer=0`, `GetAlignment=1`, `SetTensor=6`, `GetTensor=7`, `GraphCompute=9`, `GetDeviceMemory=10`) with little-endian framing (`u8 cmd + u64 len + payload`) interoperable with upstream `llama.cpp` `rpc-server`.

## `llama.cpp/CONTRIBUTING.md` Engineering Compliance

- Every kernel, parser, and protocol handler includes deterministic unit and regression tests (`cargo test --workspace`).
- Minimal, clean public API surface; zero bloat or unnecessary external system dependencies.
- Explicit human/automated code review gates (`cargo test`, `bun test`, `uvx ruff check`) run before every commit on `main`.
