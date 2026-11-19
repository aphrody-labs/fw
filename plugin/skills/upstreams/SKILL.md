---
name: upstreams
description: >-
  Auto-generated Git-pinned upstream reference catalog for Fairwind (llama.cpp, Asterinas,
  Cloudflare OS/Computer/Pingora/Kitesurf/lol-html, rmcp, ZeroClaw, Zed, nix-cachyos-kernel,
  OpenVMM, IronRDP). Keeps zero upstream source bloat in Git; refresh anytime via `just sync-upstreams`.
---

# Fairwind Auto-Generated Upstream Skill Catalog (Synced 2026-11-19)

Refresh live Git HEAD SHAs & doc references via `just sync-upstreams` (`bun src/upstream_skills.ts`).
Machine-readable TSV index: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).

| Upstream | HEAD SHA | Date | Stars | Fairwind Target | Docs |
|---|---|---|---:|---|---|
| `ggml-org/llama.cpp` | `8af68d486a` | 2026-11-19 | 135694 | `crates/fw-ggml` | [docs](https://github.com/ggml-org/llama.cpp/blob/master/CONTRIBUTING.md) |
| `asterinas/asterinas` | `7fecb4ffab` | 2026-11-19 | 5116 | `os/fairwind-os + crates/fw-ostd-wasm` | [docs](https://asterinas.github.io/book/osdk/guide/create-project.html) |
| `cloudflare/cloudflare-os` | `303e0d71b3` | 2026-11-19 | 11828 | `cloudflare/worker.ts` | [docs](https://developers.cloudflare.com/agents/) |
| `cloudflare/computer` | `3636ffb155` | 2026-11-18 | 9936 | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/computer#readme) |
| `cloudflare/pingora` | `3f87e92c98` | 2026-11-09 | 27965 | `crates/fw-cluster` | [docs](https://github.com/cloudflare/pingora#readme) |
| `cloudflare/kitesurf` | `e4e35dbe32` | 2026-11-19 | 2418 | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/kitesurf#readme) |
| `cloudflare/lol-html` | `` | 2026-11-19 | 2103 | `crates/fw-wasm + crates/fw-proto` | [docs](https://github.com/cloudflare/lol-html#readme) |
| `modelcontextprotocol/rust-sdk` | `165f8fd514` | 2026-11-17 | 4125 | `crates/fw-hermes-claw` | [docs](https://modelcontextprotocol.io/specification) |
| `zeroclaw-labs/zeroclaw` | `` | 2026-11-19 | 33494 | `crates/fw-hermes-claw` | [docs](https://github.com/zeroclaw-labs/zeroclaw#readme) |
| `zed-industries/zed` | `df1ba676f2` | 2026-11-19 | 95318 | `crates/fw-ostd-wasm + flake.nix` | [docs](https://zed.dev/docs) |
| `xddxdd/nix-cachyos-kernel` | `10e8df08ad` | 2026-11-18 | 796 | `flake.nix + nix/hosts/` | [docs](https://github.com/xddxdd/nix-cachyos-kernel#readme) |
| `microsoft/openvmm` | `c04c2b735a` | 2026-11-19 | 1976 | `os/fairwind-wsl` | [docs](https://openvmm.dev/guide/) |
| `Devolutions/IronRDP` | `2064ca1c56` | 2026-11-19 | 3316 | `os/fairwind-wsl` | [docs](https://github.com/Devolutions/IronRDP#readme) |
