# Fairwind v3.0 - Unified Hybrid Cluster & Zero-Dependency Architecture

## Overview

Fairwind v3.0 consolidates all low-level OS, inference, protocol, UI, and cluster orchestration capabilities into:
1. **One zero-system-dependency static binary** (`target/release/fairwind-native` / `fairwind-native.exe`, compiled with Fat-LTO and static CRT on Windows MSVC and Linux `musl`).
2. **One zero-dependency WebAssembly module** (`target/wasm32-unknown-unknown/release/fw_wasm.wasm`) shared by **Cloudflare Workers** and **End-User Browser WebGPU/FPU compute peers**.

## Hybrid Cluster Topology

- **Base CUDA Node (PC)**: Windows 11 + NVIDIA GeForce RTX 4070 (`sm_89`, CUDA 13.5, `ggml-cuda` + Dawn/WGPU WebGPU + `ggml-rpc`).
- **3 VPS Control/Proxy/LB/Storage/CPU Nodes**: Zero-dep `x86_64-unknown-linux-musl` static binary running:
  - **Cloudflare Pingora** consistent-hash reverse proxy & health-checked load balancer pattern
  - **`ai-dynamo/dynamo`** + **`vllm-project/router`** KV-cache radix prefix routing & disaggregated Prefill/Decode (P/D) scheduling
  - **Distributed GGUF v3 CAS storage & distribution**
  - **3 CPU Inference Workers** (including Linux 7.3, 12 cores, 45 GB RAM, 1 TB disk, no GPU, `ggml-cpu` AVX2/AVX-512)
- **4 GPU Inference Hosts**: Base CUDA PC (`gpu-1-pc-rtx4070`) + 3 remote/GCloud GPU hosts (`gpu-2..4`) pooled via `ggml-rpc` + WireGuard/QUIC.
- **End-User Federated WebGPU + FPU Pool**: Browser and desktop/mobile devices join via **WebRTC DataChannels** (`/rtc/offer`) or **WebSockets** to contribute local WebGPU WGSL & Wasm SIMD128 compute.
- **Cloud Acceleration**:
  - **Cloudflare**: `cloudflare/worker.ts` (`FairwindOSAgent` DO SQLite, R2 GGUF CDN, Tunnels, K2/Basin)
  - **Google Cloud (`gcloud`)**: Gemini Decision 1 (`/v1beta/decisions`), GCS model mirror, GPU/TPU PJRT burst nodes.

## Crate Layout (`Cargo.toml` Root Workspace)

| Crate | Responsibility | Targets |
| :--- | :--- | :--- |
| `crates/fw-ggml` | 100% `introduction-to-ggml` 4D tensor/cgraph DAG, GGUF v3, Q8_0/Q4_0 kernels, `ggml-rpc` wire protocol | Native + Wasm |
| `crates/fw-compute` | Cross-OS (Win32/UAPI/POSIX/Darwin) & hardware (CUDA/WebGPU/Vulkan/Metal/CPU/TPU PJRT) router + WGSL kernels | Native + Wasm |
| `crates/fw-cluster` | Pingora consistent-hash LB + `ai-dynamo` / `vllm-router` KV-cache radix & disaggregated P/D scheduler | Native + Wasm |
| `crates/fw-proto` | Zero-dep HTTP, WebSockets (RFC 6455), WebRTC SDP/DataChannel, GraphQL, OpenAPI 3.1, OpenAI, MCP, ACP, A2A, `/webgpu` harness | Native |
| `crates/fw-ui-blitz` | DioxusLabs/blitz (`v0.3.0-beta.4`) aligned zero-dep RGBA PNG renderer (`vello_gpu` / `vello_cpu` parity) | Native |
| `crates/fw-wasm` | `wasm32-unknown-unknown` & `wasm32-wasip2` compute kernel for Cloudflare Workers & End-User Browser WebGPU/FPU | Wasm32 |
| `crates/fw-bin` | Single unified zero-system-dependency Fat-LTO binary (`fairwind-native`) | Win11 / Linux musl / Darwin |
