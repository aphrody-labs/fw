# Fairwind v3.0 — Cross-Hardware & Cross-Cloud Deployment Matrix

| Node / Target | Hardware / OS | Rust Target Triple | System Dependencies | Compute Backends | Role in Cluster |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Base CUDA PC** | Windows 11, RTX 4070 (`sm_89`, 12 GB VRAM) | `x86_64-pc-windows-msvc` (`+crt-static`) | **0** (optional `nvcuda.dll` / `d3d12.dll` probe) | `ggml-cuda`, `ggml-webgpu` (Dawn/D3D12/Vulkan), `ggml-rpc`, Blitz `vello_gpu` | Primary CUDA inference + dev + Dawn WebGPU proof |
| **Linux 7.3 Server (VPS 1)** | Linux 7.3, 12 cores, 45 GB RAM, 1 TB disk, **no GPU** | `x86_64-unknown-linux-musl` (`+crt-static`) | **0** (`ldd`: not a dynamic executable) | `ggml-cpu` (AVX2/AVX-512), `ggml-rpc`, Pingora LB, Blitz `vello_cpu` | Proxy, KV-aware LB, GGUF CAS storage, 32B–70B CPU inference |
| **VPS 2 & VPS 3** | Linux 7.3, 8 cores, 24–32 GB RAM, no GPU | `x86_64-unknown-linux-musl` (`+crt-static`) | **0** | `ggml-cpu`, `ggml-rpc`, Pingora LB | HA proxy, KV-cache LB, shard distribution, CPU workers |
| **Remote GPU Hosts (3x)** | Linux + NVIDIA L4/T4 / GCloud GPU/TPU | `x86_64-unknown-linux-musl` | **0** (dynamic CUDA/PJRT probe) | `ggml-cuda`, `tpu-pjrt`, `ggml-rpc` | Disaggregated Prefill/Decode GPU pool (4 GPU total with PC) |
| **End-User Browser / Device** | Any WebGPU / Wasm32 device (Desktop/Mobile) | `wasm32-unknown-unknown` | **0** | WebGPU WGSL (Dawn/WGPU) + Wasm SIMD128 FPU | Federated speculative drafting & edge compute via WebRTC/WS |
| **Cloudflare Workers** | Cloudflare Edge (300+ PoPs) | `wasm32-unknown-unknown` + TS | **0** | `fw-wasm` + `FairwindOSAgent` DO SQLite + R2 | Global ingress, VFS, R2 GGUF CDN, MCP/ACP/A2A edge gateway |
| **Darwin / Mobile** | macOS Apple Silicon / Android / iOS | `aarch64-apple-darwin`, `aarch64-linux-android` | **0** | Metal / WebGPU / NEON / Blitz | Native mobile & desktop clients |
