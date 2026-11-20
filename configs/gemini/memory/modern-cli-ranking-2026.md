# Modern Nov 2026 Rust CLI, Protocol, CLI-to-MCP, CLI-to-Skills & Hermes/OpenClaw Ranking

Verified live from GitHub API & crates.io on 2026-11-19. Prefer these Rust binaries & protocols over legacy bash/PowerShell text pipelines.

## 1. Modern Rust CLIs Replacing bash/pwsh
1. `uv` (`astral-sh/uv`, 92,397★) — replaces `pip`/`venv`/`poetry`
2. `rg` (`BurntSushi/ripgrep`, 70,058★) — replaces `grep`/`Select-String` (`--json`)
3. `bat` (`sharkdp/bat`, 61,307★) — replaces `cat`/`Get-Content`
4. `fd` (`sharkdp/fd`, 45,110★) — replaces `find`/`Get-ChildItem`
5. `yazi` (`sxyazi/yazi`, 43,515★) — async Tokio file manager
6. `nu` (`nushell/nushell`, 41,035★) — typed table/JSON shell replacing `bash`/`powershell.exe`
7. `just` (`casey/just`, 36,746★) — cross-OS recipe runner replacing `make`
8. `mise` (`jdx/mise`, 36,309★) — polyglot toolchain/env manager
9. `jj` (`jj-vcs/jj`, 32,547★) — Git-compatible conflict-first VCS
10. `hyperfine` (`sharkdp/hyperfine`, 29,300★) — statistical CLI benchmarker
11. `difft` (`Wilfred/difftastic`, 26,201★) — Tree-sitter structural AST diff
12. `eza` (`eza-community/eza`, 23,950★) — git-aware `ls`
13. `ast-grep` (`ast-grep/ast-grep`, 16,627★) — structural AST search/refactor replacing `sed`/`awk`
14. `xh` (`ducaale/xh`, 8,194★) — ergonomic Rust HTTP/JSON client replacing `curl`
15. `sd` (`chmln/sd`, 7,433★) — intuitive regex replace
16. `jaq` (`01mf02/jaq`, 3,844★) — memory-safe Rust `jq` clone (10-30x faster)

## 2. CLI-to-MCP Best Rust Crates
- **#1 `rmcp` v3.11.0 + `rmcp-macros` v3.11.0** (`modelcontextprotocol/rust-sdk`, 51.9M downloads, 4,125★) — official Rust MCP SDK (stdio + Streamable HTTP + `#[tool]` macros + `schemars` JSON Schema).
- **#2 `agent-client-protocol` v5.4.0** (`agentclientprotocol/rust-sdk`, 6.45M downloads) — official ACP Rust SDK (Zed External Agents).
- **#3 `a2a-rs`** (`a2aproject/a2a-rs`) — official Agent2Agent Rust SDK.
- **#4 `rig-agent` v0.46.0** (669k downloads) + `smg-mcp` v2.3.7 (1.94M downloads).

## 3. CLI-to-Skills Best Practices (`agentskills.io`, `agentskills/agentskills` 27,455★)
- One folder per skill containing `SKILL.md` with strict YAML frontmatter (`name`, `description`) + optional `references/` and `scripts/`.
- Keep `SKILL.md` concise (progressive disclosure: only `name` + `description` load into context; body loads on demand).
- Document deterministic non-interactive flags (`--json`, `--no-heading`, `--timeout`), read-only safety hints, exact parameter types, and an executable verification gate; test with `darkrishabh/agent-skills-eval`.

## 4. Hermes Agent & OpenClaw Ecosystem (Full-Rust vs Upstream)
- Upstream references: `openclaw/openclaw` (394,799★, TS) & `NousResearch/hermes-agent` (264,635★, Python).
- **Full-Rust Agent OS ranking**:
  1. `zeroclaw-labs/zeroclaw` (33,488★, Rust) — fast, small autonomous personal assistant infrastructure
  2. `RightNow-AI/openfang` (18,271★, Rust) — open-source Agent OS
  3. `nearai/ironclaw` (12,718★, Rust) — privacy/security/WASM-sandboxed Agent OS
  4. **In-repo fw `crates/fw-hermes-claw`** (`#![forbid(unsafe_code)]`, zero system deps): unified Hermes self-growing memory + skill distiller + OpenClaw/ZeroClaw sandboxed gateway + CLI-to-MCP JSON-RPC 2.0 server + CLI-to-Skill generator (`fw claw <status|ranking|mcp|skill-gen|run>`).
