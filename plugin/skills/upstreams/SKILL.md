---
name: upstreams
description: >-
  Git-pinned catalog of the 47 upstreams fw tracks (llama-cpp, cloudflare-os, cloudflare-computer, pingora, kitesurf, lol-html, rmcp, zeroclaw, zed, nix-cachyos-kernel, scx, cloud-hypervisor, uutils-coreutils, cli-anything, ecc, claude-mem, mesh-llm, openduet, rtk, graphify, agent-skills, ggml, vllm, vllm-router, llm-d, llm-d-router, gateway-api-inference-extension, dynamo, smg, mistral-rs, candle, cubecl, llama-cpp-rs, material-color-utilities, material-theme-builder, material-web, material-design-icons, googlesans-flex, stitch-skills, gemini-cli, gemini-skills, gemini-cookbook, adk-python, antigravity-cli, antigravity-sdk-python, google-apis-rs, yup-oauth2).
  Live HEAD SHAs refresh every 6 hours (sync-upstreams workflow) or with `just sync-upstreams`.
---

# fw upstream catalog

Generated from `upstreams.toml` by `cargo xtask sync-upstreams` and `bun tools/upstream_watch.ts`; do not edit.
Index: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).

| Upstream | HEAD SHA | Target | Docs |
|---|---|---|---|
| `ggml-org/llama.cpp` | `29b52d398b` | `crates/fw-ggml` | [docs](https://github.com/ggml-org/llama.cpp/blob/master/CONTRIBUTING.md) |
| `cloudflare/cloudflare-os` | `b84d584292` | `cloudflare/worker.ts` | [docs](https://developers.cloudflare.com/agents/) |
| `cloudflare/computer` | `cd0146b287` | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/computer#readme) |
| `cloudflare/pingora` | `3f87e92c98` | `crates/fw-cluster` | [docs](https://github.com/cloudflare/pingora#readme) |
| `cloudflare/kitesurf` | `9295cc263b` | `cloudflare/worker.ts` | [docs](https://github.com/cloudflare/kitesurf#readme) |
| `cloudflare/lol-html` | `unknown` | `crates/fw-wasm + crates/fw-proto` | [docs](https://github.com/cloudflare/lol-html#readme) |
| `modelcontextprotocol/rust-sdk` | `86c65108c9` | `crates/fw-hermes-claw` | [docs](https://modelcontextprotocol.io/specification) |
| `zeroclaw-labs/zeroclaw` | `unknown` | `crates/fw-hermes-claw` | [docs](https://github.com/zeroclaw-labs/zeroclaw#readme) |
| `zed-industries/zed` | `2f808b3c2d` | `crates/fw-ostd-wasm + flake.nix` | [docs](https://zed.dev/docs) |
| `xddxdd/nix-cachyos-kernel` | `9809224771` | `flake.nix + nix/hosts/` | [docs](https://github.com/xddxdd/nix-cachyos-kernel#readme) |
| `sched-ext/scx` | `0d149b2aa8` | `nix/modules/vps-common.nix + os/fw-wsl` | [docs](https://github.com/sched-ext/scx#readme) |
| `cloud-hypervisor/cloud-hypervisor` | `8d5dd53b7c` | `nix/modules/vps-common.nix` | [docs](https://github.com/cloud-hypervisor/cloud-hypervisor#readme) |
| `uutils/coreutils` | `27752927d5` | `flake.nix + crates/fw-uutils` | [docs](https://uutils.github.io/coreutils/) |
| `HKUDS/CLI-Anything` | `34f519533b` | `plugin/skills + crates/fw-toolcall` | [docs](https://github.com/HKUDS/CLI-Anything/blob/main/cli-anything-plugin/HARNESS.md) |
| `affaan-m/ECC` | `4adeee6732` | `plugin/skills + plugin/rules + configs/claude` | [docs](https://github.com/affaan-m/ECC#readme) |
| `thedotmack/claude-mem` | `f1bb834527` | `crates/fw-bin (fw mem) + crates/fw-agent-context` | [docs](https://github.com/thedotmack/claude-mem#readme) |
| `Mesh-LLM/mesh-llm` | `ea3d3e5c68` | `crates/fw-cluster + crates/fw-ggml` | [docs](https://github.com/Mesh-LLM/mesh-llm#readme) |
| `CopilotKit/OpenDuet` | `b32ec102d5` | `plugin/skills + crates/fw-agent` | [docs](https://github.com/CopilotKit/OpenDuet#readme) |
| `rtk-ai/rtk` | `ef0b734b6b` | `crates/fw-bin (fw sh) + crates/fw-toolcall` | [docs](https://github.com/rtk-ai/rtk#readme) |
| `Graphify-Labs/graphify` | `b582ad401a` | `crates/fw-bin (fw graph) + crates/fw-rag` | [docs](https://github.com/Graphify-Labs/graphify#readme) |
| `addyosmani/agent-skills` | `8c99fa36d9` | `plugin/skills` | [docs](https://github.com/addyosmani/agent-skills#readme) |
| `ggml-org/ggml` | `d9fe7bff3d` | `crates/fw-ggml` | [docs](https://github.com/ggml-org/ggml/blob/master/docs/gguf.md) |
| `vllm-project/vllm` | `2f170a0fed` | `crates/fw-cluster + crates/fw-compute` | [docs](https://docs.vllm.ai) |
| `vllm-project/router` | `e8ce7eabd2` | `crates/fw-cluster` | [docs](https://github.com/vllm-project/router#readme) |
| `llm-d/llm-d` | `d83e85c37c` | `crates/fw-cluster` | [docs](https://llm-d.ai) |
| `llm-d/llm-d-router` | `733d0e4ee1` | `crates/fw-cluster` | [docs](https://github.com/llm-d/llm-d-router#readme) |
| `kubernetes-sigs/gateway-api-inference-extension` | `6faa78e599` | `crates/fw-cluster` | [docs](https://gateway-api-inference-extension.sigs.k8s.io) |
| `ai-dynamo/dynamo` | `ac08d8d7a3` | `crates/fw-cluster` | [docs](https://github.com/ai-dynamo/dynamo#readme) |
| `smg-project/smg` | `eab5b5be78` | `crates/fw-cluster` | [docs](https://github.com/smg-project/smg#readme) |
| `EricLBuehler/mistral.rs` | `21da642fdd` | `crates/fw-llama` | [docs](https://github.com/EricLBuehler/mistral.rs#readme) |
| `huggingface/candle` | `1a49dc27c1` | `crates/fw-ggml` | [docs](https://github.com/huggingface/candle#readme) |
| `tracel-ai/cubecl` | `ed9f1fbb8a` | `crates/fw-compute` | [docs](https://github.com/tracel-ai/cubecl#readme) |
| `utilityai/llama-cpp-rs` | `feebf11b8f` | `crates/fw-llama-sys` | [docs](https://github.com/utilityai/llama-cpp-rs#readme) |
| `material-foundation/material-color-utilities` | `dca2d34221` | `crates/fw-m3` | [docs](https://github.com/material-foundation/material-color-utilities#readme) |
| `material-foundation/material-theme-builder` | `8e4e50ba4a` | `crates/fw-m3` | [docs](https://github.com/material-foundation/material-theme-builder#readme) |
| `material-components/material-web` | `a0bab1d944` | `crates/fw-proto/ui` | [docs](https://material-web.dev/) |
| `google/material-design-icons` | `10ecc14790` | `crates/fw-proto/ui` | [docs](https://fonts.google.com/icons) |
| `googlefonts/googlesans-flex` | `9c3785b85d` | `crates/fw-m3` | [docs](https://fonts.google.com/specimen/Google+Sans+Flex) |
| `google-labs-code/stitch-skills` | `62fdcd6797` | `plugin/skills` | [docs](https://github.com/google-labs-code/stitch-skills#readme) |
| `google-gemini/gemini-cli` | `eecccfcd5a` | `crates/fw-gemini` | [docs](https://github.com/google-gemini/gemini-cli#readme) |
| `google-gemini/gemini-skills` | `c4d0c62c12` | `plugin/skills` | [docs](https://github.com/google-gemini/gemini-skills#readme) |
| `google-gemini/cookbook` | `61250eb565` | `crates/fw-gemini` | [docs](https://ai.google.dev/gemini-api/docs) |
| `google/adk-python` | `f578fcd2db` | `crates/fw-agent` | [docs](https://google.github.io/adk-docs/) |
| `google-antigravity/antigravity-cli` | `74b5ed6a97` | `native/fw-agy` | [docs](https://github.com/google-antigravity/antigravity-cli#readme) |
| `google-antigravity/antigravity-sdk-python` | `97c5c53d07` | `native/fw-agy` | [docs](https://github.com/google-antigravity/antigravity-sdk-python#readme) |
| `Byron/google-apis-rs` | `78b8ced0f9` | `crates/fw-google` | [docs](https://github.com/Byron/google-apis-rs#readme) |
| `dermesser/yup-oauth2` | `0bbe68c95b` | `crates/fw-google` | [docs](https://github.com/dermesser/yup-oauth2#readme) |
