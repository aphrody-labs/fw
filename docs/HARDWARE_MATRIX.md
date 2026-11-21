# Deployment matrix

Measured 2026-11-21 with `nvidia-smi`, `nproc`, `/proc/cpuinfo`, `free -b`, `df -B1` and `uname -r` on each node. The hosts and their addresses are declared in `cluster/build.toml`; `fw status` and `fw cluster` read that file.

| Node | Measured hardware | Role |
| :--- | :--- | :--- |
| PC | Windows 11, NVIDIA GeForce RTX 4070 (12282 MiB, compute capability 8.9) | CUDA inference, development, Dawn WebGPU proof |
| `fr1` | Linux 7.3.0-10-generic, 12 vCPU (Intel "Core Processor (Haswell, no TSX)"), 49.3 GB RAM, `/srv/xbuild` 864 GB, no GPU | build factory: sccache-dist and NativeLink scheduler, CI runners, agents |
| `prd` | Linux 7.4.0-rc3 CachyOS, 8 vCPU (AMD EPYC 9354P), 33.6 GB RAM, no GPU | production plus a capped build slice (`fw-build.slice`) |
| Browser / device | any WebGPU or `wasm32-unknown-unknown` client | registers as a peer through `POST /cluster/peers` |
| Cloudflare | Workers, Durable Objects, R2 | edge ingress; declared in `cluster/estate.toml` and `cloudflare/wrangler.jsonc` |

There is no remote GPU host. A GPU or TPU node enters the router only when it is declared in `cluster/build.toml` or joins through Kubernetes (`fw cluster k8s`).
