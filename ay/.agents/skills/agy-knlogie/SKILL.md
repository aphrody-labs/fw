---
name: agy-knlogie
description: Verified operational knowledge base for the complete Google Antigravity stack (Desktop App, agy CLI, IDE extensions, ACP JSON-RPC, localharness Python SDK, community Go/TS ports, Managed Agents Cloud API, MCP, rules, skills, hooks, plugins, sidecars, and dynamic workflows). Load whenever any LLM needs to operate, automate, debug, or extend Google Antigravity without schema errors or version traps.
---

# agy-knlogie — Verified Antigravity Stack Operating Spec (2026-11-19)

## 1. Runtime & Surface Selection
- **Antigravity CLI (`agy`)**: Zero-dependency native binary (`google-antigravity/antigravity-cli`). Shares agent engine and bidirectional settings/permissions with Antigravity App; exports sessions to GUI. Default mode since **v1.5.7** is **`auto`** (including `-p` headless runs); pin `"agentMode": ""` in `~/.gemini/antigravity-cli/settings.json` to restore `default`.
- **Antigravity App**: Standalone GUI supporting multi-agent orchestration, built-in Chromium, Marketplace plugins, and **Sidecars** (App-only; NOT supported in CLI or standalone IDE).
- **Antigravity IDE / Extensions**: VS Code, Visual Studio, JetBrains, Android Studio, Zed, Xcode (GA), Eclipse (Preview). IDE local Language Server runs on `127.0.0.1` over **ConnectRPC** with ephemeral per-session **CSRF token**; local state sits in SQLite `state.vscdb` (Electron ABI v140 / Node v22.21.1 breaks native `better-sqlite3` — use WASM `sql.js`).
- **Antigravity Python SDK (`google-antigravity`)**: **Only official Google SDK** (Apache-2.0; Go and TypeScript remain on Google's roadmap per Issue #125). Must install via `pip install google-antigravity` because the compiled **`localharness`** binary is shipped only in PyPI platform wheels. Default model: `gemini-3.8-flash`.
- **Managed Cloud Agents API (`google-genai` `v1beta`)**: Hosted isolated Linux sandbox via `agent="antigravity-preview-09-2026"` (`1,048,576` input window auto-compacted at `~135k`; `65,536` output limit; text+image input, text output).

## 2. CLI (`agy`), Headless NDJSON & ACP Contracts
- **Install**:
  - Unix: `curl -fsSL https://antigravity.google/cli/install.sh | bash`
  - Windows PS: `irm https://antigravity.google/cli/install.ps1 | iex`
  - Windows CMD: `curl -fsSL https://antigravity.google/cli/install.cmd -o install.cmd && install.cmd && del install.cmd`
- **Single-shot headless**: `agy -p "<prompt>" --output-format text|json|stream-json [--json-schema '<schema>']`
- **Bidirectional streaming**: `agy --input-format stream-json --output-format stream-json`
  - NEVER pair `--input-format stream-json` with `-p` (dropped) or `--output-format text|json` (drops intermediate turns).
  - Send NDJSON line: `{"event":"user","message":{"content":"..."}}`
  - Read NDJSON lifecycle: `init` -> `step_update`* -> `result`. Wait for `result` before sending the next prompt.
  - Extract deltas cleanly: `jq -j` on `text_delta`; unknown `--model` exits `1` with JSON error envelope.
- **ACP (Editor integration)**: `agy acp` (JSON-RPC over stdin/stdout; login via `agy acp --login`).
  - Zed config: `{"agent_servers":{"Antigravity":{"type":"custom","command":"agy","args":["acp"]}}}`.
- **Transcripts & Observability**:
  - CLI logs: `~/.gemini/antigravity-cli/brain/<id>/.system_generated/logs/transcript_full.jsonl`
  - App/IDE logs: `~/.gemini/antigravity/brain/<id>/.system_generated/logs/transcript_full.jsonl`
  - GCP export env vars: `GOOGLE_CLOUD_PROJECT`, `AGY_LOG_EXPORTER=gcp`, `AGY_OTEL_EXPORTER=gcp`.

## 3. Extensibility Invariants & Known Failure Modes
### Rules
- `AGENTS.md` / `GEMINI.md` (workspace up to repo root, `~/.gemini/`, `~/.gemini/config/`): plain Markdown, **NO YAML frontmatter**, always active.
- `.agents/rules/*.md` and `~/.gemini/config/rules/*.md`: flat scan only; **MUST** start with YAML frontmatter (`trigger: always_on | model_decision | glob | manual`, `description`, `globs`). Invalid or camelCase `trigger` causes silent discard.

### MCP Servers
- Paths: `.agents/mcp_config.json` (workspace), `~/.gemini/config/mcp_config.json` (global).
- Entry requires `command` (stdio) OR `serverUrl` (Streamable HTTP + optional `headers`/OAuth).
- `timeoutSeconds`: positive int overrides limits; `-1` disables timeout; `0`/unset keeps defaults.
- Lazy loading reads `/mcp/<server>/<tool>.json` and calls built-in `call_mcp_tool(ServerName, ToolName, Arguments)` (auto-fallback to eager if disk write fails).
- Permission policy syntax: `mcp(server/tool)`, `mcp(server/*)`, `mcp(*)`.

### Lifecycle Hooks (`.agents/hooks.json`, `~/.gemini/config/hooks.json`)
- **Schema split (CRITICAL)**:
  - `PreToolUse` and `PostToolUse` require `matcher` and nested `"hooks": [...]`.
  - `SessionStart`, `PreInvocation`, `PostInvocation`, and `Stop` have **NO `matcher`** and take a **direct array of handlers** — nesting an extra `"hooks": [...]` crashes Language Server JSON parsing and silently drops the entire file.
- Verify hook execution on target surface: CLI executes reliably; IDE v1.107.0–v2.1.1 on Windows had silent non-dispatch bugs (require IDE >= v2.5.5).

### Plugins & Security Guardrails (v1.5.7+)
- Paths: `.agents/plugins/<name>/`, `~/.gemini/config/plugins/<name>/`. Manifest: `plugin.json` (Antigravity extension key: `extensions["com.google.antigravity"]`).
- Commands: `agy plugin validate ./`, `agy plugin install <name>@antigravity-plugins-official`, `agy agents`, `agy --agent <name>`.
- Untrusted repo guard: `.agents/plugins.json` and `.agents/skills.json` silently skip entries escaping workspace/repo root via absolute paths, `../`, or symlinks.

### Sidecars (Antigravity App Only)
- Defined globally only: `~/.gemini/config/sidecars/<name>/sidecar.json` + enabled in `~/.gemini/config/config.json` (`sidecars.<name>.enabled: true`). Logs: `~/.gemini/antigravity/sidecar_data/<name>/logs`.
- Process keys: `command`, `args`, `restart_policy` (`"on-failure"`), `description`, `display_name`.
- Cron keys: `"builtin": "schedule"`, `args: ["0 * * * *", ...]` dispatching `agentapi new-conversation` or `agentapi send-message`.
- TOS guardrail: Do NOT wrap/relay consumer-plan Antigravity (`agy -p` or Sidecars) inside third-party coding agents.

### Dynamic Workflows (`/run-workflow`) & Subagents
- `/run-workflow` generates an auditable Python orchestration script with per-agent `model` (`inherit|flash_lite|flash|pro`), `type_name` (`self|research|<custom>`), and `workspace` (`inherit|branch|share`). CLI tree inspection: `/tasks` or `Ctrl+T` / `Ctrl+O`.
- Worktrees since CLI v1.5.7 preserve staged, unstaged, and untracked changes.

## 4. Python SDK (`google-antigravity`) Verified Patterns
- **Cloud / Enterprise**: `LocalAgentConfig(vertex=True, api_key="...")` (Express) or `LocalAgentConfig(vertex=True, project="...", location="us-central1")` (Standard ADC).
- **On-device LiteRT (offline)**: `LiteRTAgentConfig(model_path="/abs/path/gemma-4-26B-A4B-it-gpu.litertlm", backend="gpu", enable_speculative_decoding=True)` (never expand `~` implicitly; never use `LocalOpenAIAgentConfig` against `litert-lm serve`).
- **Ollama / vLLM / LM Studio**: `LocalOpenAIAgentConfig(model="gemma4:26b").lightweight()`.
- **Structured output**: Pass a `pydantic.BaseModel` class to `response_schema`.
- **Nested subagents**: Configure `max_subagent_depth`, `allowed_subagents`, scoped tool allowlists, and `@hooks.pre_tool_call_decide` / `@hooks.post_tool_call`.
- **Go binary override (community SDKs)**: Export `ANTIGRAVITY_HARNESS_PATH=/path/to/localharness`.
