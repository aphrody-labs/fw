# fw agent journey

Any agent (Claude Code, Codex, Cursor, Zed, OpenCode, or a custom ACP or MCP client) can drive `fw` from the shell. `fw --help` lists every command.

## 1. Orient

```bash
fw onboard                  # workspace, git, ISA tier, skills, memory, agent targets
fw doctor                   # PATH, FX_HOME, API keys, Ollama on :11434
fw repomap                  # public symbols in Rust, TS/JS, Python, Go, Kotlin
fw graph --md               # import and symbol graph
fw sec                      # static security and secret scan
```

## 2. Wire an agent

```bash
fw onboard export all       # .mcp.json, opencode.json, .zed/settings.json
fw acp                      # Agent Client Protocol, JSON-RPC 2.0 over stdio
fw serve 127.0.0.1:4380     # chat UI, /mcp, /acp, /a2a, /v1/chat/completions
fw mem add "<decision>"     # cross-session memory (<learn root>/observations/YYYY-MM.jsonl)
fw skills list              # skills from .fx, .agents, .claude, .gemini, .codex, .cursor
```

## 3. Userland applets

```bash
fw sh -c "cat Cargo.toml | grep fw- | sort -u | nl"
fw tokei crates
fw du crates
fw jq ".version" package.json
```

## 4. Hugging Face and local-model planning

```bash
fw hf search qwen --gguf --uncensored
fw hf info ggml-org/Qwen2.5-Coder-7B-Q8_0-GGUF
fw vram --vram-gb 16 --model-gb 18 --layers 60 --ctx 65536 --swa-layers 50
fw abliterate plan Qwen/Qwen3.8-27B --model-gb 16 --vram-gb 12
```

`fw abliterate biprojected|ara|sweep|moe`, `fw heretic`, `fw train` and `fw align` run reference math on synthetic data; they do not change a model.

## 5. Workflows and gates

```bash
fw workflow list
fw workflow run discover
cargo xtask ci fast
```
