# fw - Universal Autonomous Agent Contract (v4.2)

- **First Contact (< 500 tokens)**: Run `fw onboard` (or read `MAP.tsv`, `LIFETIMES_AND_STYLE.md`, `.agents/workflows/fw-agent-journey.md`). Works identically for any autonomous agent (ACP, MCP, Claude, Gemini, Codex, Cursor, Zed, OpenCode, Windsurf, Hermes, OpenClaw).
- **Token Hygiene**: `.rgignore` & `.fdignore` hide generated/vendored trees (`target/`, `dist/`, `uv.lock`) from default searches.
- **Root 27-Crate Pure-Rust Workspace + `xtask` (`Cargo.toml`)**: `crates/fw-{a2a,agent,agent-context,gemini,ggml,llama,llama-sys,ml,model,rag,rag-core,ring3-agent,toolcall,compute,cluster,proto,ui-blitz,wasm,ostd-wasm,uutils,nix,unified-stack,wayland-wgpu,hermes-claw,apm,fx,bin}` + `xtask`, plus `apps/{agents,ai-gateway}`, `packages/ai-os`, `agents/`, `docs/ai/`, and `os/{fw-os,fw-os-boot,fw-ostd-vfs,fw-wsl,initramfs,kernel-pipeline}` — CachyOS `x86-64-v3` (AVX2/FMA) + `mimalloc 0.1.52`, zero Clippy warnings, zero stubs, zero skipped tests.
- **Flat Single Binary (`fw`)**:
  - **Agent, Keyless Learning & Protocols**: `fw` / `fw -c` (REPL), `fw ask`, `fw tool`, `fw config`, `fw mcp`, `fw acp`, `fw serve` (llama.cpp-style Fw Chat UI + `/mcp` + `/acp` + `/a2a` + `/v1/chat/completions` + `/webgpu`), `fw onboard`, `fw workflow`, `fw learn` (agentless XFS dataset loop + `systemd`), `fw mem` (keyless `claude-mem` 3-layer BM25), `fw graph` (keyless `graphify` communities + God Nodes), `fw docs`/`c7` (keyless Context7), `fw skills`, `fw sec`, `fw repomap`, `fw watch`.
  - **Flagship ML & Abliteration**: `fw abliterate` (`plan`, `biprojected`, `ara`, `sweep`, `moe`, `strip-cot`), `fw heretic`, `fw hf` (`search`, `info`, `files`, `pull`, `datasets`, `spaces`, `papers`, `sitemap`, `safetensors`, `tokenizer`), `fw vram`, `fw infer`, `fw train`/`finetune`, `fw align` (`dpo`/`orpo`/`kto`), `fw tune` (`hardware`/`sysctl`/`scx`/`ananicy`/`bench`).
  - **Unified Packages & GNU/Linux Wasm32 Userland**: `fw info`/`fetch`/`lock`/`verify`/`tree`/`export`/`install`/`compile`/`audit` (Cargo sparse + npm + APM) and `40` zero-fork Wasm32-compatible `fw-uutils` applets + `fw sh -c` POSIX pipe shell.
- **Gates (`cargo xtask ci`)**:
  - `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  - `cargo build --release -p fw-bin --bin fw && cargo check --target wasm32-unknown-unknown -p fw-uutils -p fw-fx --lib && cargo build --release --target wasm32-unknown-unknown -p fw-wasm`
  - `bun run cf:dry-run && cargo xtask map && cargo xtask purity`
- **Git Discipline**: Commit explicit file paths on `main`; never `git stash` or `git reset --hard`; zero AI attribution trailers; keep repo private (`aphrody-labs/fw`).
