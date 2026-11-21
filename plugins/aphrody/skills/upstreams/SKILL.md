---
name: upstreams
description: >-
  Git-pinned catalog of the 81 upstreams fw tracks (llama-cpp, cloudflare-os, cloudflare-computer, pingora, kitesurf, lol-html, rmcp, zeroclaw, zed, nix-cachyos-kernel, scx, cloud-hypervisor, uutils-coreutils, cli-anything, ecc, claude-mem, mesh-llm, openduet, rtk, graphify, agent-skills, ggml, vllm, vllm-router, llm-d, llm-d-router, gateway-api-inference-extension, dynamo, smg, mistral-rs, candle, cubecl, llama-cpp-rs, material-color-utilities, material-theme-builder, material-web, material-design-icons, googlesans-flex, googlesans-code, stitch-skills, gemini-cli, gemini-skills, gemini-cookbook, adk-python, google-apis-rs, yup-oauth2, yazi, android-documentsui, chromium-file-manager, spacedrive, workers-rs, cloudflare-mcp, cloudflare-docs, kiri, cf-quiche, boringtun, cf-foundations, svg-hush, shellflip, wirefilter, saffron, moq-rs, web-bot-auth, cloudflared, artifact-fs, kube-rs, kubernetes-website, crates-io, rustsec-advisory-db, oha, dioxus, blitz, taffy, anyrender, rustd, accessibility-cli, dioxus-components, dioxus-sdk, vello, parley, stylo).
  Live HEAD SHAs refresh every 6 hours (sync-upstreams workflow) or with `just sync-upstreams`.
---

# fw upstream catalog

Generated from `upstreams.toml` by `cargo xtask sync-upstreams` and `bun tools/upstream_watch.ts`; do not edit.
Index: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).

| Upstream | HEAD SHA | Target | Docs |
|---|---|---|---|
| `ggml-org/llama.cpp` | `1fdda89a4d` | `crates/fw-ggml` | [docs](https://github.com/ggml-org/llama.cpp/blob/master/CONTRIBUTING.md) |
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
| `cloud-hypervisor/cloud-hypervisor` | `f0bb015695` | `nix/modules/vps-common.nix` | [docs](https://github.com/cloud-hypervisor/cloud-hypervisor#readme) |
| `uutils/coreutils` | `558c2504de` | `flake.nix + crates/fw-uutils` | [docs](https://uutils.github.io/coreutils/) |
| `HKUDS/CLI-Anything` | `34f519533b` | `plugin/skills + crates/fw-toolcall` | [docs](https://github.com/HKUDS/CLI-Anything/blob/main/cli-anything-plugin/HARNESS.md) |
| `affaan-m/ECC` | `4adeee6732` | `plugin/skills + plugin/rules + configs/claude` | [docs](https://github.com/affaan-m/ECC#readme) |
| `thedotmack/claude-mem` | `f1bb834527` | `crates/fw-bin (fw mem) + crates/fw-agent-context` | [docs](https://github.com/thedotmack/claude-mem#readme) |
| `Mesh-LLM/mesh-llm` | `cd2e30ca85` | `crates/fw-cluster + crates/fw-ggml` | [docs](https://github.com/Mesh-LLM/mesh-llm#readme) |
| `CopilotKit/OpenDuet` | `b32ec102d5` | `plugin/skills + crates/fw-agent` | [docs](https://github.com/CopilotKit/OpenDuet#readme) |
| `rtk-ai/rtk` | `ef0b734b6b` | `crates/fw-bin (fw sh) + crates/fw-toolcall` | [docs](https://github.com/rtk-ai/rtk#readme) |
| `Graphify-Labs/graphify` | `b582ad401a` | `crates/fw-bin (fw graph) + crates/fw-rag` | [docs](https://github.com/Graphify-Labs/graphify#readme) |
| `addyosmani/agent-skills` | `8c99fa36d9` | `plugin/skills` | [docs](https://github.com/addyosmani/agent-skills#readme) |
| `ggml-org/ggml` | `d9fe7bff3d` | `crates/fw-ggml` | [docs](https://github.com/ggml-org/ggml/blob/master/docs/gguf.md) |
| `vllm-project/vllm` | `a5a902d023` | `crates/fw-cluster + crates/fw-compute` | [docs](https://docs.vllm.ai) |
| `vllm-project/router` | `e8ce7eabd2` | `crates/fw-cluster` | [docs](https://github.com/vllm-project/router#readme) |
| `llm-d/llm-d` | `a573b46118` | `crates/fw-cluster` | [docs](https://llm-d.ai) |
| `llm-d/llm-d-router` | `733d0e4ee1` | `crates/fw-cluster` | [docs](https://github.com/llm-d/llm-d-router#readme) |
| `kubernetes-sigs/gateway-api-inference-extension` | `6faa78e599` | `crates/fw-cluster` | [docs](https://gateway-api-inference-extension.sigs.k8s.io) |
| `ai-dynamo/dynamo` | `17d644feb3` | `crates/fw-cluster` | [docs](https://github.com/ai-dynamo/dynamo#readme) |
| `smg-project/smg` | `d80499146a` | `crates/fw-cluster` | [docs](https://github.com/smg-project/smg#readme) |
| `EricLBuehler/mistral.rs` | `21da642fdd` | `crates/fw-llama` | [docs](https://github.com/EricLBuehler/mistral.rs#readme) |
| `huggingface/candle` | `1a49dc27c1` | `crates/fw-ggml` | [docs](https://github.com/huggingface/candle#readme) |
| `tracel-ai/cubecl` | `ed9f1fbb8a` | `crates/fw-compute` | [docs](https://github.com/tracel-ai/cubecl#readme) |
| `utilityai/llama-cpp-rs` | `feebf11b8f` | `crates/fw-llama-sys` | [docs](https://github.com/utilityai/llama-cpp-rs#readme) |
| `material-foundation/material-color-utilities` | `dca2d34221` | `crates/fw-m3` | [docs](https://github.com/material-foundation/material-color-utilities#readme) |
| `material-foundation/material-theme-builder` | `8e4e50ba4a` | `crates/fw-m3` | [docs](https://github.com/material-foundation/material-theme-builder#readme) |
| `material-components/material-web` | `a0bab1d944` | `crates/fw-proto/ui` | [docs](https://material-web.dev/) |
| `google/material-design-icons` | `10ecc14790` | `crates/fw-proto/ui` | [docs](https://fonts.google.com/icons) |
| `googlefonts/googlesans-flex` | `9c3785b85d` | `crates/fw-m3` | [docs](https://fonts.google.com/specimen/Google+Sans+Flex) |
| `googlefonts/googlesans-code` | `b8ec358c92` | `crates/fw-proto/ui` | [docs](https://fonts.google.com/specimen/Google+Sans+Code) |
| `google-labs-code/stitch-skills` | `62fdcd6797` | `plugin/skills` | [docs](https://github.com/google-labs-code/stitch-skills#readme) |
| `google-gemini/gemini-cli` | `eecccfcd5a` | `crates/fw-gemini` | [docs](https://github.com/google-gemini/gemini-cli#readme) |
| `google-gemini/gemini-skills` | `c4d0c62c12` | `plugin/skills` | [docs](https://github.com/google-gemini/gemini-skills#readme) |
| `google-gemini/cookbook` | `61250eb565` | `crates/fw-gemini` | [docs](https://ai.google.dev/gemini-api/docs) |
| `google/adk-python` | `2b90353cd2` | `crates/fw-agent` | [docs](https://google.github.io/adk-docs/) |
| `Byron/google-apis-rs` | `78b8ced0f9` | `crates/fw-google` | [docs](https://github.com/Byron/google-apis-rs#readme) |
| `dermesser/yup-oauth2` | `0bbe68c95b` | `crates/fw-google` | [docs](https://github.com/dermesser/yup-oauth2#readme) |
| `sxyazi/yazi` | `d5e4beb6b5` | `crates/fw-files` | [docs](https://yazi-rs.github.io/docs/quick-start) |
| `aphrody-labs/android-documentsui` | `4568ed0bc0` | `crates/fw-drive` | [docs](https://android.googlesource.com/platform/packages/apps/DocumentsUI/+/refs/tags/android-17.0.0_r1) |
| `aphrody-labs/chromium-file-manager` | `83cee4634e` | `crates/fw-drive` | [docs](https://chromium.googlesource.com/chromium/src/+/refs/tags/159.0.8154.13/ui/file_manager) |
| `spacedriveapp/spacedrive` | `c878ec43c1` | `crates/fw-drive` | [docs](https://github.com/spacedriveapp/spacedrive/blob/main/docs/core/architecture.mdx) |
| `cloudflare/workers-rs` | `a61be7843e` | `crates/fw-edge` | [docs](https://github.com/cloudflare/workers-rs#readme) |
| `cloudflare/mcp` | `b8602d6d5c` | `configs/claude` | [docs](https://github.com/cloudflare/mcp#readme) |
| `cloudflare/cloudflare-docs` | `a1ac58e092` | `fw docs + fw learn` | [docs](https://developers.cloudflare.com/llms.txt) |
| `libracoder/kiri` | `afc7d07814` | `crates/fw-drive (self-hosted runs)` | [docs](https://docs.runkiri.com) |
| `cloudflare/quiche` | `d448b6b239` | `crates/fw-http` | [docs](https://github.com/cloudflare/quiche#readme) |
| `cloudflare/boringtun` | `6dcc889a95` | `crates/fw-cluster` | [docs](https://github.com/cloudflare/boringtun#readme) |
| `cloudflare/foundations` | `c93d6bf9b7` | `crates/fw-cluster + crates/fw-drive` | [docs](https://github.com/cloudflare/foundations#readme) |
| `cloudflare/svg-hush` | `fba34edcbb` | `crates/fw-drive + cloudflare/drive.ts` | [docs](https://github.com/cloudflare/svg-hush#readme) |
| `cloudflare/shellflip` | `216b6eeff1` | `crates/fw-drive + crates/fw-proto` | [docs](https://github.com/cloudflare/shellflip#readme) |
| `cloudflare/wirefilter` | `a101f7c0fc` | `cloudflare/worker.ts (gatekeeper)` | [docs](https://github.com/cloudflare/wirefilter#readme) |
| `cloudflare/saffron` | `90c2af8610` | `crates/fw-bin (fw watch)` | [docs](https://github.com/cloudflare/saffron#readme) |
| `cloudflare/moq-rs` | `f9d1880f62` | `crates/fw-video` | [docs](https://github.com/cloudflare/moq-rs#readme) |
| `cloudflare/web-bot-auth` | `ce5e54fd7e` | `crates/fw-agent` | [docs](https://github.com/cloudflare/web-bot-auth#readme) |
| `cloudflare/cloudflared` | `518d7f7969` | `cloudflare/ + fleet` | [docs](https://github.com/cloudflare/cloudflared#readme) |
| `cloudflare/artifact-fs` | `2b87a48691` | `fleet (fr1 clones)` | [docs](https://github.com/cloudflare/artifact-fs#readme) |
| `kube-rs/kube` | `35136d316b` | `crates/fw-cluster` | [docs](https://kube.rs/getting-started/) |
| `kubernetes/website` | `f2db56e570` | `fw docs + fw learn` | [docs](https://kubernetes.io/docs/) |
| `rust-lang/crates.io` | `b6bbe708ce` | `crates/fw-crates` | [docs](https://crates.io/data-access) |
| `rustsec/advisory-db` | `55e1c3dfbd` | `crates/fw-crates` | [docs](https://github.com/rustsec/advisory-db#readme) |
| `hatoo/oha` | `780c4e6e23` | `crates/fw-crates` | [docs](https://github.com/hatoo/oha#readme) |
| `DioxusLabs/dioxus` | `97adf7915e` | `plugins/aphrody/skills/dioxus + docs/research/dioxus-vs-gpui-2026-11.md` | [docs](https://github.com/DioxusLabs/dioxus/tree/main/notes/architecture) |
| `DioxusLabs/blitz` | `6636442d30` | `crates/fw-ui-blitz` | [docs](https://github.com/DioxusLabs/blitz/blob/main/AGENTS.md) |
| `DioxusLabs/taffy` | `1abd8eeaf6` | `aphrody/crates/gpui` | [docs](https://github.com/DioxusLabs/taffy#readme) |
| `DioxusLabs/anyrender` | `539815c73e` | `crates/fw-ui-blitz` | [docs](https://github.com/DioxusLabs/anyrender#readme) |
| `DioxusLabs/rustd` | `ebcd234a02` | `docs/research/dioxus-vs-gpui-2026-11.md` | [docs](https://github.com/DioxusLabs/rustd#readme) |
| `DioxusLabs/accessibility-cli` | `eb58eb6918` | `plugins/aphrody/skills/agent-desktop` | [docs](https://github.com/DioxusLabs/accessibility-cli/blob/main/AGENTS.md) |
| `DioxusLabs/dioxus-components` | `9a758255ea` | `plugins/aphrody/skills/dioxus` | [docs](https://github.com/DioxusLabs/dioxus-components#readme) |
| `DioxusLabs/sdk` | `551cd80082` | `plugins/aphrody/skills/dioxus` | [docs](https://github.com/DioxusLabs/sdk#readme) |
| `linebender/vello` | `6215871f32` | `crates/fw-ui-blitz` | [docs](https://github.com/linebender/vello#readme) |
| `linebender/parley` | `a62fca2f75` | `crates/fw-ui-blitz` | [docs](https://github.com/linebender/parley#readme) |
| `servo/stylo` | `7129ad1f6b` | `crates/fw-ui-blitz` | [docs](https://github.com/servo/stylo#readme) |
