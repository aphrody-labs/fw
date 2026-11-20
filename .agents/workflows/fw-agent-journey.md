# Fw Universal Autonomous Agent Journey & Reproducible Workflows

Any autonomous agent (Claude Code, Codex, Antigravity, Cursor, Zed, OpenCode, Windsurf, Hermes, OpenClaw, or custom ACP/MCP clients) discovers and operates `fw` through one flat, zero-subprocess binary (`fw`).

## 1. First-Contact Discovery (< 500 tokens)

```bash
fw onboard                  # Workspace, git, ISA tier, skills, memory, universal targets
fw doctor                   # Zero-spawn PATH + FX_HOME + API key + Ollama :11434 probe
fw repomap                  # Public symbols across Rust/TS/JS/Python/Go/Kotlin
fw graph --md               # Deterministic import & symbol graph (zero vector store)
fw sec                      # Static security & secret scan
```

## 2. Universal Agent Bridge (Zero Vendor Lock-In)

```bash
fw onboard export all       # Writes .mcp.json, opencode.json, .zed/settings.json
fw acp                      # Agent Client Protocol JSON-RPC 2.0 over stdio
fw serve 127.0.0.1:4380     # Native Fw Chat UI + /mcp + /acp + /a2a + /v1/chat/completions
fw mem add "<decision>"     # Persistent cross-session memory (~/.fx/memory.jsonl)
fw skills list              # Unified skills across .fx/.agents/.claude/.gemini/.codex/.cursor
```

## 3. GNU/Linux + WebAssembly Userland (40 Zero-Fork Applets)

```bash
fw sh -c "cat Cargo.toml | grep fw- | sort -u | nl"
fw tokei crates
fw du crates
fw jq ".version" package.json
```

## 4. Flagship Abliteration, Hugging Face, VRAM & Alignment

```bash
fw hf search qwen --gguf --uncensored
fw hf info ggml-org/Qwen2.5-Coder-7B-Q8_0-GGUF
fw vram --vram-gb 16 --model-gb 18 --layers 60 --ctx 65536 --swa-layers 50
fw abliterate plan Qwen/Qwen3.8-27B --model-gb 16 --vram-gb 12
fw abliterate biprojected --row-norm full --strength 1.0
fw abliterate ara --rank 16 --knn 5 --steps 30
fw abliterate sweep --trials 18
fw align dpo --beta 0.1 --steps 60
```

## 5. Reproducible Built-In Workflows & CI Gates

```bash
fw workflow run discover
fw workflow run abliterate-e2e
fw workflow run pkg-audit
fw workflow run perf-check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo xtask map && cargo xtask purity
```
