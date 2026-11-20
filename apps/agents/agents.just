# Agent fleet. `just <recipe>` is the single agent interface.

# Headless Claude Code, no human in the loop (JSON result on stdout)
# Free text is passed as exported variables ($prompt), never interpolated into the script (tribunal 2026-11-13).
[group('agents')]
agent $prompt $turns="100":
    claude -p "$prompt" --permission-mode bypassPermissions --output-format json --max-turns "$turns"

# Same, on a GitHub-hosted runner through GitHub Actions (.github/workflows/claude.yml)
[group('agents')]
agent-ci $prompt:
    gh workflow run claude.yml -f "prompt=$prompt"

# Runner on this host: up | health | status | work <role> | enqueue --role R --prompt P [--host H] ...
[group('agents')]
agents cmd="status" *args:
    bun "{{root}}/apps/agents/runner.ts" {{cmd}} {{args}}

# Install/refresh the runner on a Linux host (usage: just agents-install build|test|prod|<ssh alias>): user agent everywhere (sudo NOPASSWD, docker)
[group('agents')]
agents-install host:
    timeout 120 ssh "$(bun "{{root}}/packages/env-kit/src/hosts.ts" alias {{host}})" 'sudo sh -s' < "{{root}}/apps/agents/bootstrap-user.sh"
    timeout 1800 ssh "agent@$(bun "{{root}}/packages/env-kit/src/hosts.ts" alias {{host}})" 'sh -s' < "{{root}}/apps/agents/install.sh"

# Agent tooling (apps/agents/toolchain.toml): skills, plugins, MCP for Claude Code, Codex, agy, Gemini CLI (usage: just agents-sync [--check])
[group('agents')]
agents-sync *args:
    bun "{{root}}/system/machine-ts/src/toolchain.ts" {{args}}

# Continuous self-improvement loop: collect signals, distill with the ai-os routed model, apply, log (usage: just learn [run|collect|install|status] [--since 48] [--no-apply])
[group('agents')]
learn *args:
    bun "{{root}}/apps/agents/learn-loop.ts" {{args}}

# Install the runner on this Windows PC (scheduled tasks + psmux session)
[group('agents')]
agents-install-pc:
    sh "{{root}}/apps/agents/install-pc.sh"

# Attach to a host's agent tmux session (WireGuard or public ssh; see AGENT-FIRST.md, Tunnels)
[group('agents')]
agents-attach host="prod":
    ssh -t "$(bun "{{root}}/packages/env-kit/src/hosts.ts" alias {{host}})" tmux attach -t agents
