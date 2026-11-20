# aphrody

Unified cross-CLI engineering plugin for Claude Code, Antigravity (`agy`), and Gemini CLI.

## Features

- **Autonomous Recursive Agents**: `worker` and `researcher` agent profiles designed for non-interactive, parallel, self-verifying delivery on shared git repositories.
- **Non-Blocking Bun Hooks**: automatic background scheduling for heavy builds/tests, async TypeScript checks on file edits, continuous error/recovery learning journal capture with background distillation, and idempotent git config sync.
- **Unified MCP Configuration**: local AphrodyAiOs hub (`http://127.0.0.1:8790/mcp`) plus cloud infrastructure endpoints (`hostinger`, `ovhcloud`). The ai-os runs with `AI_OS_HUB=0`: clients see its own tools and reach the aggregated servers (rea, chrome-devtools, stitch, ...) through `hub_tools` and `hub_call`, which keeps hundreds of tool names out of the context.
- **Curated Skills**: focused cross-project engineering, browser verification, cloud, Rust, Material 3, and self-learning skills.

## Install

```text
/plugin marketplace add aphrody-labs/claude
/plugin install aphrody --marketplace aphrody
```

## Privacy

This plugin operates locally inside your workspace and sends zero telemetry or user data externally. Configured remote MCP servers (`hostinger`, `ovhcloud`, local `ai-os`) are invoked only when the agent explicitly executes one of their tools, and secret tokens in learning capture logs are automatically redacted.
