---
name: upstreams
description: >-
  Auto-generated Git-pinned upstream reference catalog for fw (llama.cpp, Asterinas,
  Cloudflare OS/Computer/Pingora/Kitesurf/lol-html, rmcp, ZeroClaw, Zed, nix-cachyos-kernel,
  scx, Cloud Hypervisor, uutils, OpenVMM, IronRDP). Refresh anytime via `just sync-upstreams`.
---

# fw Auto-Generated Upstream Skill Catalog

Refresh via `just sync-upstreams` (`cargo xtask sync-upstreams`).
Index: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).

| Upstream | HEAD SHA | Target | Docs |
|---|---|---|---|
| `ggml-org/llama.cpp` | `c7468107ef` | `crates/fw-ggml` | [docs](https://github.com/ggml-org/llama.cpp/blob/master/CONTRIBUTING.md) |
| `asterinas/asterinas` | `7fecb4ffab` | `os/fairwind-os + crates/fw-ostd-wasm` | [docs](https://asterinas.github.io/book/osdk/guide/create-project.html) |
| `cloudflare/cloudflare-os` | `303e0d71b3` | `cloudflare/worker.ts` | [docs](https://developers.cloudflare.com/agents/) |
| `cloudflare/computer` | `3636ffb155` | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/computer#readme) |
| `cloudflare/pingora` | `3f87e92c98` | `crates/fw-cluster` | [docs](https://github.com/cloudflare/pingora#readme) |
| `cloudflare/kitesurf` | `7011aed78c` | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/kitesurf#readme) |
| `cloudflare/lol-html` | `unknown` | `crates/fw-wasm + crates/fw-proto` | [docs](https://github.com/cloudflare/lol-html#readme) |
| `modelcontextprotocol/rust-sdk` | `165f8fd514` | `crates/fw-hermes-claw` | [docs](https://modelcontextprotocol.io/specification) |
| `zeroclaw-labs/zeroclaw` | `unknown` | `crates/fw-hermes-claw` | [docs](https://github.com/zeroclaw-labs/zeroclaw#readme) |
| `zed-industries/zed` | `0cc1ffb6b3` | `crates/fw-ostd-wasm + flake.nix` | [docs](https://zed.dev/docs) |
| `xddxdd/nix-cachyos-kernel` | `10e8df08ad` | `flake.nix + nix/hosts/` | [docs](https://github.com/xddxdd/nix-cachyos-kernel#readme) |
| `microsoft/openvmm` | `c04c2b735a` | `os/fairwind-wsl` | [docs](https://openvmm.dev/guide/) |
| `Devolutions/IronRDP` | `15b4a03ff5` | `os/fairwind-wsl` | [docs](https://github.com/Devolutions/IronRDP#readme) |
| `sched-ext/scx` | `c8f241e960` | `nix/modules/vps-common.nix + os/fairwind-wsl` | [docs](https://github.com/sched-ext/scx#readme) |
| `cloud-hypervisor/cloud-hypervisor` | `a425508457` | `nix/modules/vps-common.nix` | [docs](https://github.com/cloud-hypervisor/cloud-hypervisor#readme) |
| `uutils/coreutils` | `f2d7c59d7d` | `flake.nix + crates/fw-uutils` | [docs](https://uutils.github.io/coreutils/) |
