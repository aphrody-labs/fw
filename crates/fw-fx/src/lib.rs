#![forbid(unsafe_code)]
//! `fw-fx`: Full pure-Rust Bun-in-Rust migration of `vercel-labs/fx` (`0.0.17`).
//!
//! Zero-abstraction, `#![forbid(unsafe_code)]`, strictly bounded modules matching
//! upstream Zig architecture:
//! - `recovery`: zero-I/O, zero-clock, zero-alloc state machine (`step(state, event, now_ms) -> Plan`)
//! - `indexer`: native `.gitignore`/`.git` data-only workspace indexer (zero subprocesses)
//! - `compactor`: context compaction preserving full turns on disk
//! - `permissions`: `ask` / `auto` / `yolo` ("full access") engine + workspace boundary checks
//! - `session`: `~/.fx/sessions/` persistence, undo stack, fork, usage totals
//! - `config`: `FX_*` env overrides, multi-dir skills loader, hierarchical `AGENTS.md`
//! - `tools`: 13 built-in tool specs & executors + `safe_git_readonly` + `AI_AGENT=fx`
//! - `gateway`: Vercel AI Gateway, Codex, Grok, Anthropic, OpenAI-compatible + SSE parser
//! - `mcp`: pure JSON-RPC 2.0 MCP engine + lazy tool search
//! - `acp`: Agent Client Protocol JSON-RPC 2.0 server (`protocol_version = 1`, `_meta.fx.*`)
//! - `cli`: all 26 `TopLevelKind` + 41 `SlashKind` commands + Program Status Protocol

pub mod acp;
pub mod agent;
pub mod cli;
pub mod compactor;
pub mod config;
pub mod gateway;
pub mod indexer;
pub mod mcp;
pub mod permissions;
pub mod recovery;
pub mod session;
pub mod tools;

pub use config::FX_VERSION;
