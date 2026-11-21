---
name: hermes-claw
description: >-
  Full-Rust Hermes Agent + OpenClaw/ZeroClaw runtime in Fw (`crates/fw-hermes-claw`),
  zero-dependency CLI-to-MCP JSON-RPC 2.0 server, `agentskills.io` CLI-to-Skill generator,
  and verified November 2026 modern Rust CLI & protocol rankings replacing bash/pwsh.
---

# Fw Hermes Agent + OpenClaw (`fw-hermes-claw`)

Full-Rust (`#![forbid(unsafe_code)]`), zero-system-dependency runtime wired into `fw` (`fw-bin`) and the HTTP/MCP server (`fw-proto`).

## CLI Commands

- `fw claw status` — sandbox allowlist, MCP tools, memory & skill counts.
- `fw claw ranking` — verified Nov 2026 Rust CLIs (`uv`, `rg`, `bat`, `fd`, `yazi`, `nu`, `just`, `mise`, `jj`, `hyperfine`, `difft`, `eza`, `ast-grep`, `xh`, `sd`, `jaq`) & agent protocols (`rmcp`, `agent-client-protocol`, `a2a-rs`, `agentskills`, `zeroclaw`, `openfang`, `ironclaw`).
- `fw claw mcp '<jsonrpc>'` — executes MCP JSON-RPC 2.0 (`initialize`, `tools/list`, `tools/call`) with `SandboxPolicy`.
- `fw claw skill-gen <tool>` — emits an `agentskills.io`-compliant `SKILL.md`.
- `fw claw run "<goal>"` — runs the grounded Hermes memory + MCP discovery + skill distillation loop.

## HTTP Control Plane (`fw serve 127.0.0.1:4380`)

- `GET /` — embedded fullstack control-plane web dashboard (`web/index.html`).
- `POST /mcp` — Streamable HTTP MCP JSON-RPC 2.0 backed by `CliToMcpServer`.
- `GET /claw/status`, `GET /claw/ranking`, `POST /claw/run`.
