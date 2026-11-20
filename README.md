# fw

Single pure-Rust v4.2 binary (`#![forbid(unsafe_code)]`, `x86-64-v3`, `mimalloc 0.1.52`, zero subprocesses) for local LLM abliteration, Hugging Face Hub operations, autonomous agent protocols, keyless self-learning, and a 40-applet GNU/Linux Wasm32 userland.

- Live Edge Worker: `https://fw-edge.aphrody-labs.workers.dev` (`cloudflare/wrangler.jsonc`, `FwOSAgent` SQLite Durable Object + `web/` UI)
- Workspace: 15 pure-Rust crates (`crates/fw-{ggml,compute,cluster,proto,ui-blitz,wasm,ostd-wasm,uutils,nix,unified-stack,wayland-wgpu,hermes-claw,apm,fx,bin}`) + `xtask`, 7 external crates (`serde`, `serde_json`, `sha2`, `tokio`, `flate2`, `mimalloc`, `ureq`)

## Quick Start

```bash
# Build native release binary (Fat-LTO, x86-64-v3, mimalloc)
cargo build --release -p fw-bin --bin fw

# Orient any autonomous agent in one <500-token call
fw onboard
fw onboard export all          # exports .mcp.json, opencode.json, .zed/settings.json
fw workflow run discover       # doctor + repomap + graph + sec
```

## Core Capabilities

### 1. Flagship Abliteration, Inference, Training & Hugging Face (Zero Subprocess)
- `fw abliterate <plan|biprojected|ara|sweep|moe|strip-cot|presets>` — 7-step local studio planner, biprojected row-norm orthogonalization (Lai 2025), Adaptive Refusal Abliteration (Weidmann 2026), Pareto KL sweep, MoE expert-granular targeting, and CoT refusal scorer.
- `fw heretic <presets|inspect|abliterate>` — Heretic TOML presets and directional orthogonalization in pure Rust.
- `fw vram` — Exact `llama.cpp` VRAM, SWA KV-cache, and `-ngl` layer planner.
- `fw hf <search|info|files|pull|datasets|spaces|papers|collections|whoami|sitemap|safetensors|tokenizer|cache>` — Pure-Rust TLS 1.3 (`ureq 3.5.2`) Hugging Face Hub client, binary Safetensors header parser, and `tokenizer.json` encoder.
- `fw infer` / `fw train` / `fw align <dpo|orpo|kto>` — GGUF v3 Q8_0 inference, LoRA fine-tuning, and preference alignment.

### 2. Pure-Rust Keyless Self-Learning & Infinite XFS Dataset
- `fw learn <status|harvest|distill|search|loop|systemd>` — Agentless learning loop (reads `.git/logs/HEAD`, rebuilds knowledge graphs, ingests docs, writes deduplicated `corpus/YYYY-MM.jsonl` pairs) with hardened `systemd` unit for 24/7 VPS XFS storage (`/srv/xbuild/fw-learn`).
- `fw mem <add|search|timeline|get|list|stats|prune>` — Keyless `claude-mem`: structured observations, 3-layer progressive disclosure (`search` BM25 -> `timeline` -> `get`), SHA-256 deduplication, monthly JSONL shards.
- `fw graph [query|god-nodes|communities] [dir] [--md] [--save]` — Keyless `graphify`: multi-language AST/symbol/import graph, Label Propagation communities, God Nodes, and cross-community bridges.
- `fw docs` / `fw c7 <resolve|query|ingest|list>` — Keyless Context7: live TLS 1.3 resolver for crates.io, npm, PyPI, GitHub READMEs, and `llms.txt` with BM25 token-budgeted retrieval and permanent XFS cache.

### 3. GNU/Linux Wasm32 Userland & Unified Package Manager
- `fw sh -c "cmd1 | cmd2 | cmd3"` — Zero-fork in-process POSIX pipeline shell over 40 Wasm32-compatible applets (`ls`, `find`, `grep`, `cat`, `head`, `tail`, `wc`, `sort`, `uniq`, `cut`, `tr`, `nl`, `rev`, `seq`, `jq`, `tokei`, `du`, `bat`, `base64`, `sha256sum`, `stat`, `which`, `cp`, `mv`, `rm`, `mkdir`, `touch`, etc.).
- `fw info` / `fw fetch` / `fw lock` / `fw verify` / `fw tree` / `fw export` — Unified Cargo RFC 2789 sparse index + npm + APM resolver, SHA-256 verifier, and CycloneDX 1.5 SBOM exporter.

### 4. Agent Protocols, Security & CachyOS Tuning
- `fw serve [host:port]` — Local Fw Chat UI (`web/`, `100/100` Baseline web audit) + `/mcp`, `/acp`, `/a2a`, `/v1/chat/completions`, `/webgpu`.
- `fw ask` / `fw acp` / `fw mcp` / `fw sec` / `fw skills` / `fw repomap` / `fw watch`.
- `fw tune <hardware|sysctl|scx|ananicy|bench>` — CachyOS `chwd`/`sysctl`/`scx_lavd`/`ananicy-cpp` profiles and local `x86-64-v3` + `mimalloc` microbenchmarks (`core/cpp/fw_cachy_simd.cpp`).

## Verification & CI Gates

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --target wasm32-unknown-unknown -p fw-uutils -p fw-fx --lib
cargo build --release -p fw-bin --bin fw
cargo xtask map && cargo xtask purity
bun run cf:dry-run
```

For autonomous agent onboarding, start with [AGENTS.md](AGENTS.md) and [.agents/workflows/fw-agent-journey.md](.agents/workflows/fw-agent-journey.md).
