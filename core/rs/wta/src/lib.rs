//! `wta_core` — Optimized reusable Rust library surface extracted from Microsoft
//! Intelligent Terminal (`microsoft/intelligent-terminal`, `tools/wta`), adding:
//! - OSC 7501 program/agent sidebar status protocol (commit `4f4c8345`, 2026-11-17)
//! - Native Windows NamedPipe / ConPTY / SDDL bridge descriptors
//! - Cloudflare Workers AI / AI Gateway / Streamable HTTP MCP+ACP+A2A routing for Rio & `fw`

pub mod osc52;
pub mod rio_bridge;
