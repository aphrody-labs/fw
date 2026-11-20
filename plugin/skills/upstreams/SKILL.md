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
| `ggml-org/llama.cpp` | `184862801a` | `crates/fw-ggml` | [docs](https://github.com/ggml-org/llama.cpp/blob/master/CONTRIBUTING.md) |
| `asterinas/asterinas` | `12374d1337` | `os/fw-os + crates/fw-ostd-wasm` | [docs](https://asterinas.github.io/book/osdk/guide/create-project.html) |
| `cloudflare/cloudflare-os` | `dc2594d58a` | `cloudflare/worker.ts` | [docs](https://developers.cloudflare.com/agents/) |
| `cloudflare/computer` | `cd0146b287` | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/computer#readme) |
| `cloudflare/pingora` | `3f87e92c98` | `crates/fw-cluster` | [docs](https://github.com/cloudflare/pingora#readme) |
| `cloudflare/kitesurf` | `c19d8354d6` | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/kitesurf#readme) |
| `cloudflare/lol-html` | `unknown` | `crates/fw-wasm + crates/fw-proto` | [docs](https://github.com/cloudflare/lol-html#readme) |
| `modelcontextprotocol/rust-sdk` | `459cb4f841` | `crates/fw-hermes-claw` | [docs](https://modelcontextprotocol.io/specification) |
| `zeroclaw-labs/zeroclaw` | `unknown` | `crates/fw-hermes-claw` | [docs](https://github.com/zeroclaw-labs/zeroclaw#readme) |
| `zed-industries/zed` | `10d1b2a564` | `crates/fw-ostd-wasm + flake.nix` | [docs](https://zed.dev/docs) |
| `xddxdd/nix-cachyos-kernel` | `ffdb35a774` | `flake.nix + nix/hosts/` | [docs](https://github.com/xddxdd/nix-cachyos-kernel#readme) |
| `microsoft/openvmm` | `5cf28492b3` | `os/fw-wsl` | [docs](https://openvmm.dev/guide/) |
| `Devolutions/IronRDP` | `63d442e8dd` | `os/fw-wsl` | [docs](https://github.com/Devolutions/IronRDP#readme) |
| `sched-ext/scx` | `c8f241e960` | `nix/modules/vps-common.nix + os/fw-wsl` | [docs](https://github.com/sched-ext/scx#readme) |
| `cloud-hypervisor/cloud-hypervisor` | `1ff2b3decc` | `nix/modules/vps-common.nix` | [docs](https://github.com/cloud-hypervisor/cloud-hypervisor#readme) |
| `uutils/coreutils` | `276ffe9cc0` | `flake.nix + crates/fw-uutils` | [docs](https://uutils.github.io/coreutils/) |
| `HKUDS/CLI-Anything` | `34f519533b` | `plugin/skills + crates/fw-toolcall` | [docs](https://github.com/HKUDS/CLI-Anything/blob/main/cli-anything-plugin/HARNESS.md) |
| `affaan-m/ECC` | `4adeee6732` | `plugin/skills + plugin/rules + configs/claude` | [docs](https://github.com/affaan-m/ECC#readme) |
| `thedotmack/claude-mem` | `c0f37d60b2` | `crates/fw-bin (fw mem) + crates/fw-agent-context` | [docs](https://github.com/thedotmack/claude-mem#readme) |
| `Mesh-LLM/mesh-llm` | `b54e992781` | `crates/fw-cluster + crates/fw-ggml` | [docs](https://github.com/Mesh-LLM/mesh-llm#readme) |
| `CopilotKit/OpenDuet` | `b32ec102d5` | `plugin/skills + crates/fw-agent` | [docs](https://github.com/CopilotKit/OpenDuet#readme) |
| `rtk-ai/rtk` | `5315347b56` | `crates/fw-bin (fw sh) + crates/fw-toolcall` | [docs](https://github.com/rtk-ai/rtk#readme) |
| `Graphify-Labs/graphify` | `f51d46c63d` | `crates/fw-bin (fw graph) + crates/fw-rag` | [docs](https://github.com/Graphify-Labs/graphify#readme) |
| `addyosmani/agent-skills` | `8c99fa36d9` | `plugin/skills` | [docs](https://github.com/addyosmani/agent-skills#readme) |
