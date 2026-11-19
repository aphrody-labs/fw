---
name: hermes-claw
description: >-
  Full-Rust Hermes Agent + OpenClaw/ZeroClaw runtime in Fairwind (`crates/fw-hermes-claw`),
  zero-dependency CLI-to-MCP JSON-RPC 2.0 server, `agentskills.io` CLI-to-Skill generator,
  and verified November 2026 modern Rust CLI & protocol rankings replacing bash/pwsh.
---

# Fairwind Hermes Agent + OpenClaw (`fw-hermes-claw`)

Full-Rust (`#![forbid(unsafe_code)]`), zero-system-dependency runtime wired into `fairwind-native` (`fw-bin`) and the HTTP/MCP server (`fw-proto`).

## CLI Commands

- `fairwind-native claw status` — sandbox allowlist, MCP tools, memory & skill counts.
- `fairwind-native claw ranking` — verified Nov 2026 Rust CLIs (`uv`, `rg`, `bat`, `fd`, `yazi`, `nu`, `just`, `mise`, `jj`, `hyperfine`, `difft`, `eza`, `ast-grep`, `xh`, `sd`, `jaq`) & agent protocols (`rmcp`, `agent-client-protocol`, `a2a-rs`, `agentskills`, `zeroclaw`, `openfang`, `ironclaw`).
- `fairwind-native claw mcp '<jsonrpc>'` — executes MCP JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`) with `SandboxPolicy`.
- `fairwind-native claw skill-gen <tool>` — emits an `agentskills.io`-compliant `SKILL.md`.
- `fairwind-native claw run "<goal>"` — runs the grounded Hermes memory + MCP discovery + skill distillation loop.

## HTTP Control Plane (`fairwind-native serve 127.0.0.1:4380`)

- `GET /` — embedded fullstack control-plane web dashboard (`web/index.html`).
- `POST /mcp` — Streamable HTTP MCP JSON-RPC 2.0 backed by `CliToMcpServer`.
- `GET /claw/status`, `GET /claw/ranking`, `POST /claw/run`.
