---
name: zed-ai
description: >-
  Authoritative Zed AI documentation reference distilled from zed-industries/zed/docs/src/ai
  (28 files): Zed Agent, External Agents (ACP & ACP Registry), Terminal Threads, Parallel
  Agents (Git worktrees), Skills & Instructions (replacing Rules), Built-in Tools,
  Tool Permissions (v0.224+), OS Sandboxing (Seatbelt/Bubblewrap), MCP context servers,
  Edit Prediction (Zeta/Copilot/Codestral/Mercury/local), LLM Providers, Gateways,
  Local Models (llama.cpp/Ollama/LM Studio), Subscriptions, and AI Privacy.
---

# Zed AI Architecture & Configuration (`zed-industries/zed/docs/src/ai`)

Synced from upstream `zed-industries/zed` (`docs/src/ai/`, 28 files mirrored in `../../docs/zed/ai/`).

## 1. Three Agent Paths

1. **Zed Agent** (`zed-agent.md`): Native agent in the Agent Panel and Threads Sidebar. Uses Zed-configured LLM providers, built-in tools, profiles, skills, instructions, MCP servers, automatic compaction (`90%` default threshold, `/compact`), subagents (`spawn_agent`), and OS sandboxing.
2. **External Agents via ACP** (`external-agents.md`): Integrates external agents (Claude, Codex, OpenCode, Copilot, Cursor, Pi Coding Agent, Poolside) through the **Agent Client Protocol (ACP)** (`agentclientprotocol.com`). Install via **ACP Registry** (`Add Agent -> Install from Registry`, replacing deprecated pre-v1.5.0 ACP extensions) or configure custom agents under `agent_servers.<agent-id>` in `settings.json`.
3. **Terminal Threads** (`terminal-threads.md`): Terminal-backed threads in the Threads Sidebar for running agent CLIs/TUIs directly with native notifications (OpenCode, Pi) and terminal titles (Codex).
4. **Parallel Agents** (`parallel-agents.md`): Run multiple Zed Agent, External Agent, and Terminal Threads concurrently in the Threads Sidebar across projects, isolated via **Git worktrees**.

## 2. Instructions & Skills (Rules Replaced)

- **Rules deprecated** (`rules.md`): Legacy Rules Library has been replaced by **Instructions** (always-on) and **Skills** (on-demand).
- **Instructions** (`instructions.md`):
  - Personal always-on: `~/.config/zed/AGENTS.md` (Linux/macOS) or `%APPDATA%\Zed\AGENTS.md` (Windows).
  - Project always-on (auto-discovered): `AGENTS.md`, `AGENT.md`, `CLAUDE.md`, `GEMINI.md`, `.rules`, `.cursorrules`, `.windsurfrules`, `.clinerules`, `.github/copilot-instructions.md`.
- **Skills** (`skills.md`):
  - Folder containing `SKILL.md` (YAML frontmatter `name`, `description`, optional `disable-model-invocation: true`) plus optional `references/` and `scripts/`.
  - Discovery paths: `~/.agents/skills/` (user, supports grouping `~/.agents/skills/<group>/<skill>/`) and `<worktree>/.agents/skills/` (project).
  - Invoked via `skill` tool, `@skill`, `/skill-name`, or `/create-skill`; shareable via `zed://skill?data=...`.

## 3. Built-in Tools & Profiles

- **Read/Search**: `diagnostics`, `fetch`, `find_path`, `grep`, `list_directory`, `read_file`.
- **Web**: `search_web`.
- **Edit/Exec**: `copy_path`, `create_directory`, `delete_path`, `edit_file`, `move_path`, `write_file`, `terminal`.
- **Meta**: `skill`, `spawn_agent`.
- **Agent Profiles** (`agent-profiles.md`): Bind a default model + allowed built-in and MCP tools per profile.

## 4. Tool Permissions (Zed v0.224.0+)

- `agent.tool_permissions.default` (`"confirm"` | `"allow"` | `"deny"`) replaces legacy `agent.always_allow_tool_actions`.
- Per-tool rules under `agent.tool_permissions.tools.<tool>` (`terminal`, `edit_file`, `write_file`, `delete_path`, `move_path`, `copy_path`, `create_directory`, `fetch`, `search_web`, `skill`, or `mcp:<server>:<tool_name>`):
  - `default`, `always_allow`, `always_deny`, `always_confirm` using regex patterns (with built-in safety rules blocking destructive patterns like `rm -rf /` and protecting `.zed/` / sensitive paths).

## 5. OS-Level Sandboxing (`sandboxing.md`)

- Isolates `terminal` and `fetch` tool calls using OS primitives (`sandbox-exec` / Seatbelt on macOS, `bwrap` / Bubblewrap on Linux; protects `.git`, `$HOME/.ssh`, `$PATH`).
- Configured under `agent.sandbox_permissions` in `settings.json`:
  - `network_hosts` (exact hostnames or `*.domain` wildcards), `allow_all_hosts`
  - `write_paths`, `allow_fs_write_all`, `allow_unsandboxed`

## 6. MCP Servers (`mcp.md`)

- Configured in Agent Settings (`Add Local Server`, `Add Remote Server`, or `Install from Extensions`).
- Remote MCP servers without an explicit `"Authorization"` header trigger Zed's standard **MCP OAuth** flow.
- Supports dynamic `notifications/tools/list_changed` and granular `mcp:<server>:<tool_name>` permissions.

## 7. Models, Settings, Edit Prediction & Local Inference

- **Feature-specific models** (`agent-settings.md`): `agent.default_model`, `agent.subagent_model`, `agent.inline_assistant_model`, `agent.commit_message_model`, `agent.thread_summary_model`, `agent.compaction_model`; supports `enable_thinking`, `effort`, `speed` (`"fast"` | `"standard"`).
- **Edit Prediction** (`edit-prediction.md`): Multi-line completions via **Zeta** (built-in), GitHub Copilot, Codestral, Mercury Coder, or local models.
- **Local Models** (`use-a-local-model.md`): `llama.cpp`, Ollama, LM Studio, and local OpenAI-compatible servers (with configurable context length & local edit prediction).
- **Gateways & APIs** (`use-a-gateway.md`, `use-api-access.md`, `use-an-existing-subscription.md`): OpenRouter (custom models + provider routing), Vercel AI Gateway, Amazon Bedrock (cross-region, guardrails, Mantle), Anthropic/OpenAI/Google AI/Mistral/DeepSeek/xAI/OpenCode APIs, plus ChatGPT Plus/Pro, Claude Pro/Max, Copilot, Cursor, OpenCode Zen/Go subscriptions.

## Local Upstream Mirror (`../../docs/zed/ai/`)

Read the exact upstream file in `../../docs/zed/ai/` when verifying JSON schema details:
`overview.md`, `agents.md`, `zed-agent.md`, `agent-panel.md`, `agent-profiles.md`, `agent-settings.md`, `external-agents.md`, `parallel-agents.md`, `terminal-threads.md`, `text-threads.md`, `inline-assistant.md`, `edit-prediction.md`, `instructions.md`, `rules.md`, `skills.md`, `tools.md`, `tool-permissions.md`, `mcp.md`, `sandboxing.md`, `llm-providers.md`, `use-a-gateway.md`, `use-a-local-model.md`, `use-an-existing-subscription.md`, `use-api-access.md`, `by-company.md`, `quick-start.md`, `privacy-and-security.md`, `ai-improvement.md`.
