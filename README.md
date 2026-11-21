# fw

Rust toolchain and agent runtime: one `fw` binary (x86-64-v3, mimalloc) with a coding agent, a Hugging Face Hub client, MCP/ACP/A2A servers, a package manager, local learning and memory stores, and a 40-applet userland that runs in-process.

- Edge Worker: `https://fw.aphrody.com` (`cloudflare/wrangler.jsonc`, `FwOSAgent` SQLite Durable Object, static UI from `crates/fw-proto/ui`).
- Drive CDN: `https://fw.aphrody.com/drive/` serves the shared Aphrody Drive folder through Cloudflare Access (Google sign-in for people, `aphrody-agents` service token for agents and apps), an R2 cache (`aphrody-drive`) and the `drive-cdn@aphrody` service account (`cloudflare/drive.ts`).
- Workspace: the `crates/fw-*` members and `xtask` of `Cargo.toml` (`cargo metadata --no-deps --format-version 1`). Per-directory tiers and gates are in `MAP.tsv` (`cargo xtask map`). Crates without `#![forbid(unsafe_code)]`: `git grep -L "forbid(unsafe_code)" -- "crates/*/src/lib.rs"`.
- Subprocesses: most commands run in-process; `fw ask` (shell tool), `fw convert` (runs `fw-video`) and the `fw-agent`, `fw-gemini`, `fw-llama` and `fw-a2a` crates spawn child processes.

## Install

```bash
# Linux / macOS
curl -fsSL https://fw.aphrody.com/install.sh | sh

# Windows PowerShell
irm https://fw.aphrody.com/install.ps1 | iex

fw self-update                    # SHA-256 verified, keeps the previous binary as .prev
fw self-uninstall --purge-data
```

## Build

```bash
cargo build --release -p fw-bin --bin fw
fw onboard                        # workspace, git, ISA tier, skills, memory
fw onboard export all             # .mcp.json, opencode.json, .zed/settings.json
fw workflow run discover
```

## Commands

`fw --help` lists every command; `fw <command> --help` shows its usage.

- Agent: `fw ask`, `fw tool`, `fw tui`, `fw serve [host:port]` (chat UI plus `/mcp`, `/acp`, `/a2a`, `/v1/chat/completions`, `/webgpu`), `fw acp`, `fw mcp`.
- Hugging Face: `fw hf <search|info|files|pull|datasets|spaces|papers|collections|whoami|sitemap|safetensors|tokenizer|cache>`; `fw vram` plans VRAM, KV cache and `-ngl` layers for llama.cpp.
- ML reference math: `fw abliterate`, `fw heretic`, `fw train`, `fw align` and `fw infer --gguf` run on synthetic data and do not read or modify model weights. `fw abliterate plan` and `fw vram` are planners; `fw infer <prompt>` forwards to an OpenAI-compatible endpoint.
- Memory and learning: `fw learn`, `fw mem <add|search|timeline|get|list|stats|prune>`, `fw graph`, `fw docs`/`fw c7` (resolves crates.io, npm, PyPI, GitHub README and `llms.txt` docs), `fw repomap`, `fw sec`, `fw skills`, `fw watch`.
- Packages: `fw info`, `fetch`, `lock`, `verify`, `tree`, `export` (CycloneDX 1.5), `install`, `audit`, `compile` over the Cargo sparse index, npm and APM.
- Crate intelligence: `fw crates <search|info|compare|rank|awesome|bench|stack|sync>` ranks every crates.io crate from the daily database dump (documented score in `docs/crates/RANKING.md`), writes per-category awesome lists (`docs/crates/awesome`), load-tests Rust web frameworks (`docs/crates/bench`), and generates a Cargo skeleton for a goal; also `GET /v1/crates/*` and the `crates_*` MCP tools on `fw serve` and in the Bun plugin (`crates/fw-crates).
- Userland: `fw sh -c "cmd1 | cmd2"` and 40 applets (`ls`, `grep`, `sort`, `jq`, `tokei`, `du`, `bat`, `base64`, `sha256sum`, `cp`, `mv`, `rm`, and others listed in `crates/fw-uutils/src/lib.rs`).
- Host: `fw tune <hardware|sysctl|scx|ananicy|bench>` (CachyOS profiles and x86-64-v3 and mimalloc microbenchmarks, `cpp/fw_cachy_simd.cpp`).

## Gates

```bash
cargo xtask ci all        # clippy, tests, ruff, bun tests, native ABI, wasm32 build, Cloudflare dry run, purity, MAP.tsv
cargo xtask ci fast       # the same without native ABI, wasm32 and Cloudflare
```

Agents start with [AGENTS.md](AGENTS.md) and [.agents/workflows/fw-agent-journey.md](.agents/workflows/fw-agent-journey.md).
