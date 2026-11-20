# Fw Verified Cross-Cloud, ML, Low-Level & Agent Ecosystem Catalog (2026-11-19)

Primary-source verified repositories, crate/package versions, and community fixes integrated or tracked by Fw.

## 1. Hugging Face (`github.com/orgs/huggingface`)
- `huggingface/candle` (`0.11.1`, 21.3k★) - pure-Rust zero-Python ML framework (CPU/SIMD, CUDA, Metal, `wasm32`). Note: `Device::Vulkan` (`#4096`) unmerged; use CPU/CUDA/Metal/WASM/WGSL.
- `safetensors/safetensors` (`0.8.0`, 4.0k★) - zero-copy `mmap` tensor format; header inspection built natively into Fw ABI alongside GGUF.
- `huggingface/tokenizers` (`0.23.2` stable, `1.0.0-rc.4`, 11.3k★) - native & `wasm32` Rust tokenizers.
- `huggingface/xet-core` (`1.8.1`, 635★) - content-defined chunk dedup for multi-GB weights & VFS sync.
- `huggingface/hf-hub` (`1.0.0`, 347★) - pure-Rust Hub client with Xet support.
- `huggingface/transformers.js` (`@huggingface/transformers@4.3.2`, 16.4k★) - WebGPU/ONNX inference across Browser/Bun/Node.
- `huggingface/lerobot` (`v0.7.0`, 29.0k★) - robotics / System-1 control & world-model policy training.
- `huggingface/smolagents` (`v1.26.0`, 30.4k★) - CodeAgent loop architecture.
- `huggingface/peft` (`v0.21.3`, 21.9k★) + `transformers` (`v5.23.0`) + `trl` (`v1.18.1`) - LoRA/QLoRA fine-tuning integrated with `configs/heretic` & `core/py/fw_ml`.
- `huggingface/text-embeddings-inference` (`v1.9.4`, 5.1k★) - Rust embedding/reranker architecture. *(Note: `text-generation-inference` is archived; do not adopt.)*

## 2. Anomalyco (`github.com/orgs/anomalyco`)
- `anomalyco/opencode` (`v1.19.6`, 227.9k★) - CLI/TUI/Desktop coding agent reference; fixes applied in Fw: `#14973` (multi-turn `tool_calls` continuation), `#64800` (ACP embedded blobs with optional `mimeType`), `#64774`/`#64795` (VFS path-scoped tool resolution).
- `anomalyco/opentui` (`v0.9.3`, 13.9k★) - Flexbox TUI engine with Rust bindings (`#2157` private Windows CRT linking for MSVC Fat-LTO).
- `anomalyco/models.dev` (7.7k★) - live open JSON model/pricing/capability catalog (`https://models.dev/api.json`).
- `anomalyco/rune` (7.5k★) - cross-OS + Web GPU-rendered Rust UI with headless PNG verification.
- `anomalyco/rift` (`v0.0.13`, 1.4k★) - CoW workspaces for parallel agents; `anomalyco/terminal-control` (`v1.2.1`) - PTY/TUI test automation; `anomalyco/opentunnel` (`v0.6.2`) - blind local TLS tunnel.

## 3. Cloudflare Edge OS (`cloudflare/*`)
- `cloudflare/cloudflare-os` (11.8k★) - Router + Workshop + Gatekeepers + sandboxed Gadgets.
- `cloudflare/computer` (`@cloudflare/computer@0.11.3` + `@cloudflare/dofs`, 9.9k★) - POSIX VFS in DO SQLite + 3-tier runtime (`just-bash@3.6.1`, `@cloudflare/codemode@0.5.6` + `worker_loaders`, `@cloudflare/sandbox@1.0.3`).
- `cloudflare/agents` (`agents@0.33.0`, `@cloudflare/ai-chat@0.12.2`, `agents/models/ai-sdk` `createAI()`).
- `cloudflare/workers-rs` (`worker@0.9.0`) - Rust->WASM Workers SDK; Browser Run (`@cloudflare/playwright@1.3.6`) + Kitesurf; Basin (GA) & K2 (Beta); `hono@4.13.x`; `@modelcontextprotocol/server@2.x`.

## 4. Vercel Fluid Compute (`vercel/*`)
- `vercel@66.2.0`, `@vercel/functions@3.11.3`, `ai@7.0.177`, `@vercel/sandbox@4.0.0`, `workflow@5.6.1`, `next@16.4.6`.
- Fw `vercel/` targets **Bun 1.4.x Fluid Compute** (`bunVersion: "1.4.x"`, zero legacy `runtime: "edge"`, zero TS decorators per `vercel#17712`, `globalStore = false` per `vercel#17711`, REST/Node24 isolation for Sandbox until `vercel/sandbox#479` lands).

## 5. Google Gemini / Gemma / Antigravity
- `google-antigravity` Python SDK is consumed from PyPI (Apache-2.0), not vendored.
- `gemini-decision-1` (`POST /v1beta/decisions`) dual-mode implementation (`src/decisions.ts`): official remote API when `GEMINI_API_KEY` is set + zero-output-token calibrated local System-1 fallback (`noul`, `choice`, `score`).
- Cross-OS kernel foundations: `packages/windows-rs` (`microsoft/windows-rs`), `packages/rust-for-linux` (`Rust-for-Linux/linux` 7.4-rc3), `native/fw-kernel` (`177 KiB` Fat-LTO), and `native/fw-world` (`15 KiB` pure `no_std` bare-metal/WASM System-1 + World Model).
