#![forbid(unsafe_code)]

//! `fairwind-native` (`fw-bin`) — Single unified zero-system-dependency Fat-LTO binary
//! across Win 11 RTX 4070 (`msvc` static CRT), Linux 7.3 12-core 45 GB RAM (`musl` static),
//! 3 VPS Pingora/Dynamo/vLLM-Router nodes, 4 GPU hosts, and End-User WebGPU/FPU peers.

use fw_cluster::HybridClusterRouter;
use fw_compute::bench_sandbox::{
    bench_compare, bench_exec, bench_http, bench_silicon, universal_bench_matrix,
};
use fw_compute::{detect_hardware_profile, run_compute_smoke_bench};
use fw_ggml::{encode_gguf_header, parse_gguf_header, rpc_get_device_memory, spawn_rpc_server_once};
use fw_nix::compute_fixed_output_store_entry;
use fw_ostd_wasm::{boot_ostd_gpui_snapshot, GpuiWasmScene};
use fw_proto::{build_openapi_3_1, serve};
use fw_ui_blitz::render_cluster_hud_png;
use fw_unified_stack::{
    default_burn_rig_pipeline, encode_kitty_graphics_rgba_header, encode_kitty_keyboard_enable,
    encode_osc133_prompt_start, hex_voice_status, unified_v4_catalog, yazi_list_dir,
};
use fw_uutils::fast_cli::{fast_find, fast_grep, fast_ls, fast_table};
use fw_uutils::hardened::normalize_applet_name;
use fw_uutils::{run_applet, SUPPORTED_APPLETS};
use fw_wayland_wgpu::{WaylandWgpuCompositor, WAYLAND_WASM_GLOBALS};
use fw_hermes_claw::{
    default_cli_tools, generate_agent_skill_md, modern_cli_ranking, protocol_and_agent_ranking,
    ChannelKind, HermesClawAgent,
};
use serde_json::json;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

fn main() {
    let raw_argv0 = std::env::args().next().unwrap_or_default();
    let bin_stem = normalize_applet_name(&raw_argv0);
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Case-insensitive multicall argv[0] dispatch (fixes uutils #16228 on Windows/MSYS2)
    if SUPPORTED_APPLETS.contains(&bin_stem.as_str()) {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let res = run_applet(&bin_stem, &refs, "");
        if !res.stdout.is_empty() {
            print!("{}", res.stdout);
        }
        if !res.stderr.is_empty() {
            eprint!("{}", res.stderr);
        }
        std::process::exit(res.exit_code);
    }

    let cmd = args.first().map(|s| s.as_str()).unwrap_or("status");

    // Zero-overhead fast CLI paths (beats eza/fd/rg/nu startup & throughput with zero external deps)
    match cmd {
        "ls" => {
            let tsv = args.iter().any(|a| a == "--tsv");
            let dir = args.iter().skip(1).find(|a| !a.starts_with('-')).map(Path::new).unwrap_or(Path::new("."));
            if let Err(e) = fast_ls(dir, tsv) {
                eprintln!("fw ls: {e}");
                std::process::exit(1);
            }
            return;
        }
        "table" => {
            let as_json = args.iter().any(|a| a == "--json");
            let dir = args.iter().skip(1).find(|a| !a.starts_with('-')).map(Path::new).unwrap_or(Path::new("."));
            if let Err(e) = fast_table(dir, as_json) {
                eprintln!("fw table: {e}");
                std::process::exit(1);
            }
            return;
        }
        "find" => {
            let pat = args.get(1).map(String::as_str).unwrap_or("");
            let dir = args.get(2).map(Path::new).unwrap_or(Path::new("."));
            if let Err(e) = fast_find(pat, dir) {
                eprintln!("fw find: {e}");
                std::process::exit(1);
            }
            return;
        }
        "grep" => {
            let pat = args.get(1).map(String::as_str).unwrap_or("");
            let dir = args.get(2).map(Path::new).unwrap_or(Path::new("."));
            if let Err(e) = fast_grep(pat, dir) {
                eprintln!("fw grep: {e}");
                std::process::exit(1);
            }
            return;
        }
        "bench" => {
            let sub = args.get(1).map(String::as_str).unwrap_or("silicon");
            let flag_usize = |name: &str, def: usize| -> usize {
                args.windows(2)
                    .find(|w| w[0] == name)
                    .and_then(|w| w[1].parse().ok())
                    .unwrap_or(def)
            };
            let sandbox = args.iter().any(|a| a == "--sandbox");
            match sub {
                "silicon" => {
                    let mb = flag_usize("--mb", 8);
                    println!("{}", serde_json::to_string_pretty(&bench_silicon(mb)).unwrap());
                }
                "matrix" => {
                    println!("{}", serde_json::to_string_pretty(&universal_bench_matrix()).unwrap());
                }
                "exec" => {
                    let warmup = flag_usize("--warmup", 2);
                    let runs = flag_usize("--runs", 10);
                    let cmdline = args.last().map(String::as_str).unwrap_or("");
                    match bench_exec(cmdline, warmup, runs, sandbox) {
                        Ok(rep) => println!("{}", serde_json::to_string_pretty(&rep).unwrap()),
                        Err(e) => {
                            eprintln!("fw bench exec: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                "compare" => {
                    let warmup = flag_usize("--warmup", 2);
                    let runs = flag_usize("--runs", 10);
                    let mut cmds = Vec::new();
                    let mut i = 2usize;
                    while i < args.len() {
                        match args[i].as_str() {
                            "--warmup" | "--runs" => i += 2,
                            "--sandbox" => i += 1,
                            c => {
                                cmds.push(c.to_string());
                                i += 1;
                            }
                        }
                    }
                    match bench_compare(&cmds, warmup, runs, sandbox) {
                        Ok(rep) => println!("{}", serde_json::to_string_pretty(&rep).unwrap()),
                        Err(e) => {
                            eprintln!("fw bench compare: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                "http" => {
                    let conc = flag_usize("--concurrency", 8);
                    let reqs = flag_usize("--requests", 50);
                    let target = args.last().map(String::as_str).unwrap_or("127.0.0.1:4380/health");
                    match bench_http(target, conc, reqs) {
                        Ok(rep) => println!("{}", serde_json::to_string_pretty(&rep).unwrap()),
                        Err(e) => {
                            eprintln!("fw bench http: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                _ => {
                    println!(
                        "{{\"ok\":true,\"usage\":\"fw bench <silicon|matrix|exec|compare|http> [--warmup N] [--runs N] [--sandbox]\"}}"
                    );
                }
            }
            return;
        }
        "migrate" => {
            let target_kind = args.get(1).map(String::as_str).unwrap_or("help");
            let rest = &args[args.len().min(2)..];
            let status = match target_kind {
                "openclaw" | "claw" => std::process::Command::new("bun")
                    .arg("src/migrate_openclaw_to_fw.ts")
                    .args(rest)
                    .status(),
                "hermes" => std::process::Command::new("uv")
                    .args(["run", "python", "core/py/fw_py_ai/migrate_hermes_to_fw.py"])
                    .args(rest)
                    .status(),
                "vllm" | "llmd" | "llm-d" => std::process::Command::new("uv")
                    .args(["run", "python", "core/py/fw_py_ai/migrate_vllm_llmd_to_fw.py", target_kind])
                    .args(rest)
                    .status(),
                _ => {
                    println!(
                        "{{\"ok\":true,\"usage\":\"fw migrate <openclaw|hermes|vllm|llmd> [--source <path>] [--target <dir>] [--dry-run|--apply]\"}}"
                    );
                    return;
                }
            };
            match status {
                Ok(s) if s.success() => return,
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("fw migrate {target_kind}: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {}
    }

    match cmd {
        "status" => {
            let hw = detect_hardware_profile();
            let cluster = HybridClusterRouter::default_hybrid_cluster();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "product": "fairwind-native",
                    "version": "3.0.0",
                    "zeroSystemDeps": true,
                    "hardware": hw,
                    "cluster": {
                        "name": cluster.cluster_name,
                        "gpuHosts": 4,
                        "vpsProxyLbStorageCpuHosts": 3,
                        "userWebGpuFpuPeers": 1,
                        "router": "pingora + ai-dynamo + vllm-router",
                        "ggmlCompat": "ggml-org/ggml + llama.cpp GGUF v3 + ggml-rpc"
                    }
                }))
                .unwrap()
            );
        }
        "hardware" => {
            println!("{}", serde_json::to_string_pretty(&detect_hardware_profile()).unwrap());
        }
        "compute" => {
            println!("{}", serde_json::to_string_pretty(&run_compute_smoke_bench()).unwrap());
        }
        "ggml" => {
            let hdr = encode_gguf_header(3, 64, 256);
            let parsed = parse_gguf_header(&hdr).unwrap();
            let (addr, h) = spawn_rpc_server_once(10 * 1024 * 1024 * 1024, 12 * 1024 * 1024 * 1024).unwrap();
            let (free, total) = rpc_get_device_memory(addr).unwrap();
            let _ = h.join();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "gguf": parsed,
                    "rpcLoopback": { "freeBytes": free, "totalBytes": total }
                }))
                .unwrap()
            );
        }
        "cluster" => {
            let mut router = HybridClusterRouter::default_hybrid_cluster();
            let sub = args.get(1).map(|s| s.as_str()).unwrap_or("summary");
            if sub == "route" {
                let prompt = args.get(2).map(|s| s.as_str()).unwrap_or("Fairwind v3.0 hybrid inference");
                let d1 = router.route_request(prompt, true);
                let d2 = router.route_request(prompt, true);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "ok": true, "first": d1, "secondKvHit": d2 })).unwrap()
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&router.summary()).unwrap());
            }
        }
        "openapi" => {
            println!("{}", serde_json::to_string_pretty(&build_openapi_3_1()).unwrap());
        }
        "ui" => {
            let out = args
                .iter()
                .position(|a| a == "--out")
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("target/blitz-proof.png"));
            let rep = render_cluster_hud_png(&out, 640, 360).expect("render blitz proof PNG");
            println!("{}", serde_json::to_string_pretty(&rep).unwrap());
        }
        "ostd" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "kernel": boot_ostd_gpui_snapshot(),
                    "gpuiWgslPreview": GpuiWasmScene::quad_wgsl_shader().lines().next().unwrap_or("")
                }))
                .unwrap()
            );
        }
        "uutils" => {
            let applet = args.get(1).map(String::as_str).unwrap_or("uname");
            let rest: Vec<&str> = args.iter().skip(2).map(String::as_str).collect();
            let res = run_applet(applet, &rest, "");
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": res.exit_code == 0,
                    "applets": SUPPORTED_APPLETS,
                    "result": res
                }))
                .unwrap()
            );
        }
        "nix" => {
            let name = args.get(1).map(String::as_str).unwrap_or("fairwind-uutils-4.0.0");
            let entry = compute_fixed_output_store_entry(name, b"#!/bin/sh\nexec fairwind-native\n", true);
            println!("{}", serde_json::to_string_pretty(&json!({ "ok": true, "entry": entry })).unwrap());
        }
        "stack" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "catalog": unified_v4_catalog(),
                    "burnRig": default_burn_rig_pipeline(),
                    "voice": hex_voice_status(),
                    "yaziSampleCount": yazi_list_dir(".", 8).len(),
                    "kittyProtocol": {
                        "graphicsSample": encode_kitty_graphics_rgba_header(1, 1, "AAAA"),
                        "keyboardCsiU": encode_kitty_keyboard_enable(1),
                        "osc133": encode_osc133_prompt_start()
                    }
                }))
                .unwrap()
            );
        }
        "wayland" => {
            let mut comp = WaylandWgpuCompositor::new(1280, 720);
            let sid = comp.create_surface("Fairwind Wayland-WebGPU-GPUI", 1280, 720);
            let commit_hdr = comp.commit_surface(sid).unwrap();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "fork": "https://github.com/aphrody-labs/wayland-rs",
                    "zeroLibwaylandCDeps": true,
                    "globals": WAYLAND_WASM_GLOBALS,
                    "surfaceId": sid,
                    "commitHeader": commit_hdr,
                    "gpuiQuads": comp.scene.quads.len()
                }))
                .unwrap()
            );
        }
        "claw" | "hermes" => {
            let sub = args.get(1).map(String::as_str).unwrap_or("status");
            let mut agent = HermesClawAgent::new("fairwind-hermes-claw");
            match sub {
                "ranking" => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "ok": true,
                            "modernRustClis": modern_cli_ranking(),
                            "protocolsAndAgentOs": protocol_and_agent_ranking()
                        }))
                        .unwrap()
                    );
                }
                "mcp" => {
                    let raw = args
                        .get(2)
                        .cloned()
                        .unwrap_or_else(|| r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#.into());
                    let req: serde_json::Value = serde_json::from_str(&raw)
                        .unwrap_or_else(|_| json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}));
                    let resp = agent.mcp.handle_jsonrpc(&req);
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                }
                "skill-gen" => {
                    let name = args.get(2).map(String::as_str).unwrap_or("rg_search");
                    let tools = default_cli_tools();
                    let spec = tools.iter().find(|t| t.name == name).unwrap_or(&tools[0]);
                    println!("{}", generate_agent_skill_md(spec));
                }
                "run" => {
                    let goal = args
                        .get(2..)
                        .map(|s| s.join(" "))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "Audit Rust CLI MCP skills".into());
                    let report = agent.run_goal(ChannelKind::Cli, &goal);
                    println!("{}", serde_json::to_string_pretty(&report).unwrap());
                }
                _ => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "ok": true,
                            "agent": agent.name,
                            "sandboxAllowBinaries": agent.policy.allow_binaries.len(),
                            "mcpTools": agent.mcp.tools.len(),
                            "memories": agent.memories.len(),
                            "skills": agent.skills.len(),
                            "subcommands": ["status", "ranking", "mcp <jsonrpc>", "skill-gen <tool>", "run <goal>"]
                        }))
                        .unwrap()
                    );
                }
            }
        }
        "serve" => {
            let addr_str = args.get(1).map(|s| s.as_str()).unwrap_or("127.0.0.1:4380");
            let addr: SocketAddr = addr_str.parse().expect("valid host:port");
            let rt = tokio::runtime::Runtime::new().unwrap();
            eprintln!("fairwind-native v3.0 listening on http://{addr}");
            rt.block_on(async move {
                serve(addr).await.expect("serve failed");
            });
        }
        _ => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "usage": "fairwind-native <status|hardware|compute|ggml|cluster|openapi|ui|serve>"
                }))
                .unwrap()
            );
        }
    }
}
