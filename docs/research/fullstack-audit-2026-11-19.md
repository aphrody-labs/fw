# Fw Fullstack Deep Research Audit (2026-11-19)

Dated source-verified audit following `context7-deep-research` (live GitHub HEAD commits, crates.io API, and local workspace inspection).

## 1. Verified Upstream Sources (2026-11-19)

| Component | Source | Verified Commit / Version | Date |
|---|---|---|---|
| MCP Rust SDK (`rmcp`) | `modelcontextprotocol/rust-sdk` (`main`) | `165f8fd514` (`v3.11.0`, 51.9M DL) | 2026-11-17 |
| ACP Rust SDK | `agentclientprotocol/rust-sdk` | `v5.4.0` (6.45M DL) | 2026-11-19 |
| Flake-Parts | `hercules-ci/flake-parts` (`main`) | `ea2630b747` | 2026-11-01 |
| Just | `casey/just` (`master`) | `62784c2a6e` | 2026-11-15 |
| Workers-RS | `cloudflare/workers-rs` (`main`) | `a61be7843e` | 2026-11-17 |
| Zed | `zed-industries/zed` (`main`) | `b51ffab015` (95,309★, `v1.29.2`) | 2026-11-19 |
| OpenClaw / Hermes / ZeroClaw | `openclaw/openclaw` (394,799★), `NousResearch/hermes-agent` (264,635★), `zeroclaw-labs/zeroclaw` (33,488★) | live HEAD | 2026-11-19 |

## 2. Gaps Found & Resolved in `fw` Monorepo

1. **Root Nix Flake (`flake.nix`)** — added `flake-parts` + `crane` root flake with full Nov 2026 Rust CLI `devShell` (`nu`, `rg`, `fd`, `ast-grep`, `jaq`, `xh`, `sd`, `bat`, `eza`, `jj`, `difft`, `hyperfine`, `just`, `bun`, `uv`) and `nixosModules.fw`.
2. **Task Runner (`justfile`)** — added cross-OS recipes (`clippy`, `test`, `build`, `musl`, `claw`, `serve`, `docker`, `plugin`, `ci`).
3. **Container Packaging (`Dockerfile`, `compose.yaml`)** — added Alpine Linux (`rust:1.91-alpine` -> `alpine:3.22`) static musl multi-stage build (zero Debian/Ubuntu, read-only rootfs, non-root user, `/health` check).
4. **Fullstack Control-Plane UI (`web/index.html`)** — embedded single-binary HTML/CSS/JS dashboard served at `GET /` by `fw-proto`, wired to live `/health`, `/cluster`, `/claw/status`, `/claw/ranking`, `/claw/run`, `/mcp`, and WebGPU adapter probe.
5. **Live MCP + Hermes/OpenClaw HTTP Routes (`crates/fw-proto`)** — `POST /mcp` now executes `fw-hermes-claw::CliToMcpServer::handle_jsonrpc` directly; added `/claw/status`, `/claw/ranking`, `/claw/run`.
6. **CI Pipeline (`.github/workflows/ci.yml`)** — upgraded to enforce `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `fw-bin` + `wasm32-unknown-unknown` builds, and live `fw-native claw` smoke checks.
7. **Plugin Completeness (`plugin/skills/hermes-claw/SKILL.md`)** — expanded Fw plugin to 10 skills.
