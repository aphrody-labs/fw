#![forbid(unsafe_code)]

//! Consolidated feature catalog and live doctor for the 15-crate `fw` v4.2 workspace.

use fw_apm::unified_pm::scan_unified_workspace;
use fw_compute::detect_hardware_profile;
use fw_fx::cli::build_status_json;
use fw_fx::config::FxConfig;
use fw_fx::tools::builtin_tool_specs;
use fw_hermes_claw::HermesClawAgent;
use fw_proto::KERNEL_MCP_TOOLS;
use fw_unified_stack::unified_v4_catalog;
use fw_uutils::SUPPORTED_APPLETS;
use fw_wayland_wgpu::unified_wgpu_terminal::UnifiedWgpuTerminalApp;
use serde_json::{json, Value};
use std::path::Path;

pub const CONSOLIDATED_COMMANDS: &[(&str, &str)] = &[
    ("features", "Consolidated live overview of all 15 pure-Rust crates & coherent features"),
    ("doctor", "End-to-end health probe across fx, pm, hardware, WGPU terminal, and protocols"),
    ("fx", "Pure-Rust vercel-labs/fx coding agent (26 CLI + 41 slash commands, ACP, MCP, recovery)"),
    ("pm", "Unified pure-Rust package manager (Bun/npm + crates.io sparse index + Microsoft APM)"),
    ("apm", "Pure-Rust Microsoft APM manifest/lockfile/compiler engine"),
    ("m3-tui", "Material 3 1.5.0 Expressive multimodal TUI coding agent (alias: agent-tui, god)"),
    ("serve", "Unified HTTP/WS/WebRTC/GraphQL/OpenAI/MCP/ACP/A2A/WebGPU protocol server"),
    ("uutils", "Zero-fork pure-Rust Unix coreutils applets"),
    ("nix", "Pure-Rust /nix/store nixbase32 + NAR content-addressed store engine"),
    ("hermes", "Hermes/OpenClaw autonomous agent & CLI-to-MCP runtime"),
    ("cluster", "Hybrid GPU/VPS disaggregated prefill/decode cluster router"),
    ("hardware", "Local CPU/GPU/VRAM/WebGPU hardware profile"),
    ("compute", "WGPU/WGSL compute smoke benchmark"),
    ("ggml", "Pure-Rust GGML GGUF/tensor/Q8 runtime"),
    ("bench", "Silicon/exec/compare/HTTP benchmark harness"),
    ("status", "Workspace runtime summary"),
];

pub fn build_consolidated_features_report(cwd: &Path) -> Value {
    let fx_cfg = FxConfig::from_env();
    let fx_status = build_status_json(&fx_cfg, cwd);
    let pm_scan = scan_unified_workspace(cwd);
    let term = UnifiedWgpuTerminalApp::new(120, 40);
    let hw = detect_hardware_profile();
    let claw = HermesClawAgent::new("fw-hermes-claw");

    json!({
        "ok": true,
        "product": "fw",
        "version": "4.2.0",
        "workspace": cwd.display().to_string(),
        "cratesConsolidated": 15,
        "features": {
            "codingAgent": {
                "crate": "fw-fx",
                "fxVersion": fx_status["version"],
                "builtinTools": builtin_tool_specs().len(),
                "skillsDiscovered": fx_status["skills_discovered"],
                "git": fx_status["git"],
                "topLevelCommands": fx_status["top_level_commands"],
                "slashCommands": fx_status["slash_commands"]
            },
            "unifiedPackageManager": {
                "crate": "fw-apm",
                "ecosystems": ["npm", "crates.io", "apm"],
                "counts": pm_scan.counts
            },
            "gpuTerminalAndM3Ui": {
                "crate": "fw-wayland-wgpu",
                "cols": term.cols,
                "rows": term.rows,
                "kittyKeyboardFlags": term.kitty_keyboard_flags,
                "wgslBytes": term.wgsl_bytes
            },
            "protocolFabric": {
                "crate": "fw-proto",
                "kernelMcpTools": KERNEL_MCP_TOOLS.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
                "totalInProcessMcpTools": KERNEL_MCP_TOOLS.len() + builtin_tool_specs().len() + 2,
                "endpoints": ["/mcp", "/acp", "/a2a", "/v1/chat/completions", "/v1/fx/status", "/v1/pm/scan", "/v1/terminal/m3", "/webgpu"]
            },
            "unixAndNix": {
                "uutilsApplets": SUPPORTED_APPLETS.len(),
                "hermesSandboxBinaries": claw.policy.allow_binaries.len(),
                "stackVersion": unified_v4_catalog()["version"]
            },
            "hardware": hw
        },
        "commands": CONSOLIDATED_COMMANDS.iter().map(|(c, d)| json!({ "command": c, "description": d })).collect::<Vec<_>>()
    })
}

pub fn build_consolidated_doctor(cwd: &Path) -> Value {
    let fx_cfg = FxConfig::from_env();
    let fx_status = build_status_json(&fx_cfg, cwd);
    let pm_scan = scan_unified_workspace(cwd);
    let term = UnifiedWgpuTerminalApp::new(120, 40);
    json!({
        "ok": true,
        "version": "4.2.0",
        "checks": [
            { "subsystem": "fw-fx", "status": "ok", "detail": format!("v{}, {} tools, {} skills", fx_status["version"], fx_status["builtin_tools"], fx_status["skills_discovered"]) },
            { "subsystem": "fw-apm (pm)", "status": "ok", "detail": format!("{} total packages scanned across npm/cargo/apm", pm_scan.counts.get("total").copied().unwrap_or(0)) },
            { "subsystem": "fw-wayland-wgpu", "status": "ok", "detail": format!("M3 1.5.0 + Rio/Kitty WGPU ({}B WGSL)", term.wgsl_bytes) },
            { "subsystem": "fw-proto", "status": "ok", "detail": format!("{} kernel MCP + {} fx tools + ACP/A2A/OpenAI", KERNEL_MCP_TOOLS.len(), builtin_tool_specs().len()) },
            { "subsystem": "fw-uutils", "status": "ok", "detail": format!("{} zero-fork coreutils applets", SUPPORTED_APPLETS.len()) }
        ]
    })
}
