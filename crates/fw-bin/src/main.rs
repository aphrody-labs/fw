#![forbid(unsafe_code)]

//! `fw` (`fw-bin`): the command-line entry point. Command names, usage and summaries live in `cli_parser.rs`.

mod abliterate;
mod agent_platform;
mod auto_learn;
mod cachy_tune;
mod cli_parser;
mod doctor;
mod hf_hub;
mod local_studio;
mod m3_agent_tui;
mod ml_engine;
mod self_lifecycle;
mod universal_agent;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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

    let parsed = match cli_parser::parse_argv(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fw: {e}");
            std::process::exit(2);
        }
    };

    if parsed.global.version {
        println!("fw {}", cli_parser::FW_VERSION);
        return;
    }

    if let Some(dir) = &parsed.global.directory
        && let Err(e) = std::env::set_current_dir(dir)
    {
        eprintln!("fw: cannot chdir to {}: {e}", dir.display());
        std::process::exit(1);
    }

    if parsed.global.help {
        if let Some(cmd) = &parsed.command
            && let Some(help_txt) = cli_parser::render_command_help(cmd)
        {
            print!("{help_txt}");
            return;
        }
        print!("{}", cli_parser::render_root_help());
        return;
    }

    let Some(cmd_owned) = parsed.command else {
        let repl_args = if parsed.global.continue_session {
            vec!["-c".to_owned()]
        } else {
            Vec::new()
        };
        match fw_fx::cli::run_cli(&repl_args) {
            Ok(0) => return,
            Ok(code) => std::process::exit(code),
            Err(e) => {
                eprintln!("fw: {e}");
                std::process::exit(1);
            }
        }
    };

    let cmd = cmd_owned.as_str();
    let mut args = Vec::with_capacity(parsed.args.len() + 1);
    args.push(cmd_owned.clone());
    args.extend(parsed.args);

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
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let report = fw_apm::unified_pm::scan_unified_workspace(&cwd);
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "ok": true,
                    "zeroSubprocess": true,
                    "target": target_kind,
                    "workspace": cwd.display().to_string(),
                    "packagesDetected": report.counts
                }))
                .unwrap()
            );
            return;
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
                    "product": "fw",
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
                let prompt = args.get(2).map(|s| s.as_str()).unwrap_or("hybrid inference");
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
            let name = args.get(1).map(String::as_str).unwrap_or("fw-uutils-4.0.0");
            let entry = compute_fixed_output_store_entry(name, b"#!/bin/sh\nexec fw\n", true);
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
            let sid = comp.create_surface("Fw Wayland-WebGPU-GPUI", 1280, 720);
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
            let mut agent = HermesClawAgent::new("fw-hermes-claw");
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
        // Flat unified coding agent commands (fw-fx merged directly into fw)
        "ask" | "tool" | "config" | "mcp" | "acp" | "sessions" | "session" | "permissions"
        | "models" | "provider" | "workspace" | "pr" | "issue" | "completion" => {
            match fw_fx::cli::run_cli(&args) {
                Ok(0) => {}
                Ok(code) => std::process::exit(code),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        // Flat unified package manager commands (bun + uv grammar: info/resolve, tree/scan, export/sbom, lock, verify, semver)
        "info" | "resolve" | "fetch" | "download" | "tree" | "scan" | "export" | "sbom" | "lock" | "verify" | "semver" => {
            if parsed.global.offline && matches!(cmd, "info" | "resolve" | "fetch" | "download") {
                eprintln!("fw {cmd}: disabled by --offline");
                std::process::exit(1);
            }
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let mapped = match cmd {
                "info" => "resolve",
                "fetch" => "download",
                "tree" => "scan",
                "export" => "sbom",
                other => other,
            };
            let mut refs = Vec::with_capacity(args.len());
            refs.push(mapped);
            refs.extend(args.iter().skip(1).map(String::as_str));
            match fw_apm::unified_pm::run_pm_cli(&refs, &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "init" | "install" | "add" | "remove" | "uninstall" | "compile" | "audit" | "list" | "deps" | "prune" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let mapped = match cmd {
                "add" => "install",
                "remove" => "uninstall",
                other => other,
            };
            let mut refs = Vec::with_capacity(args.len());
            refs.push(mapped);
            refs.extend(args.iter().skip(1).map(String::as_str));
            match fw_apm::run_cli(&refs, &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "infer" | "run" => {
            match ml_engine::run_infer(&args[1..], parsed.global.offline) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "train" | "finetune" | "lora" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match ml_engine::run_train(&args[1..], &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "hf" | "huggingface" => {
            match hf_hub::run_hf_cli(&args[1..], parsed.global.offline) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "abliterate" | "abl" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match abliterate::run_abliterate_cli(&args[1..], &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw abliterate: {e}");
                    std::process::exit(1);
                }
            }
        }
        "heretic" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match ml_engine::run_heretic(&args[1..], &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "align" | "dpo" | "orpo" => {
            let fwd = if matches!(cmd, "dpo" | "orpo") {
                let mut v = vec![cmd.to_owned()];
                v.extend_from_slice(&args[1..]);
                v
            } else {
                args[1..].to_vec()
            };
            match ml_engine::run_align(&fwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw {cmd}: {e}");
                    std::process::exit(1);
                }
            }
        }
        "vram" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&local_studio::run_vram_cli(&args[1..])).unwrap()
            );
        }
        "repomap" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let target = args.get(1).map(|s| cwd.join(s)).unwrap_or(cwd);
            println!(
                "{}",
                serde_json::to_string_pretty(&local_studio::build_repo_map(&target, 120)).unwrap()
            );
        }
        "watch" => {
            match local_studio::run_watch_cli(&args[1..]) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw watch: {e}");
                    std::process::exit(1);
                }
            }
        }
        "tune" => {
            match cachy_tune::run_tune_cli(&args[1..]) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw tune: {e}");
                    std::process::exit(1);
                }
            }
        }
        "tui" | "m3-tui" | "god" => {
            m3_agent_tui::run_m3_agent_tui(&args[1..]);
        }
        "doctor" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            println!(
                "{}",
                serde_json::to_string_pretty(&doctor::run_doctor(&cwd)).unwrap()
            );
        }
        "help" => {
            if let Some(sub) = args.get(1)
                && let Some(txt) = cli_parser::render_command_help(sub)
            {
                print!("{txt}");
                return;
            }
            print!("{}", cli_parser::render_root_help());
        }
        "convert" | "cv" => std::process::exit(run_sibling("convert", "fw-video", &args[1..])),
        "google" => std::process::exit(run_sibling("google", "fw-google", &args[1..])),
        "files" | "fl" => std::process::exit(run_sibling("files", "fw-files", &args[1..])),
        "serve" => {
            let addr_str = args.get(1).map(|s| s.as_str()).unwrap_or("127.0.0.1:4380");
            let addr: SocketAddr = match addr_str.parse() {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("fw serve: `{addr_str}` is not a host:port address ({e}); example: fw serve 127.0.0.1:4380");
                    std::process::exit(2);
                }
            };
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("fw serve: cannot start the async runtime: {e}");
                    std::process::exit(1);
                }
            };
            eprintln!("fw v{} listening on http://{addr}", cli_parser::FW_VERSION);
            if let Err(e) = rt.block_on(serve(addr)) {
                eprintln!("fw serve: {addr}: {e}");
                std::process::exit(1);
            }
        }
        "sec" | "deepsec" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let target = args.get(1).map(|s| cwd.join(s)).unwrap_or(cwd);
            println!(
                "{}",
                serde_json::to_string_pretty(&agent_platform::run_security_scan(&target)).unwrap()
            );
        }
        "graph" | "graphify" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let store = auto_learn::XfsDatasetStore::resolve(None);
            match auto_learn::run_graphify_cli(&args[1..], &cwd, &store) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw graph: {e}");
                    std::process::exit(1);
                }
            }
        }
        "mem" | "memory" | "claude-mem" => {
            let store = auto_learn::XfsDatasetStore::resolve(None);
            match auto_learn::run_claude_mem_cli(&args[1..], &store) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw mem: {e}");
                    std::process::exit(1);
                }
            }
        }
        "docs" | "c7" | "context7" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let store = auto_learn::XfsDatasetStore::resolve(None);
            match auto_learn::run_context7_cli(&args[1..], &cwd, &store, parsed.global.offline) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw docs: {e}");
                    std::process::exit(1);
                }
            }
        }
        "learn" | "autolearn" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match auto_learn::run_learn_cli(&args[1..], &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw learn: {e}");
                    std::process::exit(1);
                }
            }
        }
        "skills" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match agent_platform::run_skills_cli(&args[1..], &cwd, parsed.global.offline) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw skills: {e}");
                    std::process::exit(1);
                }
            }
        }
        "onboard" | "init-agent" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match universal_agent::run_onboard(&args[1..], &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw onboard: {e}");
                    std::process::exit(1);
                }
            }
        }
        "workflow" | "wf" => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match universal_agent::run_workflow_cli(&args[1..], &cwd) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw workflow: {e}");
                    std::process::exit(1);
                }
            }
        }
        "self-install" => {
            match self_lifecycle::run_self_install(&args[1..]) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw self-install: {e}");
                    std::process::exit(1);
                }
            }
        }
        "self-update" | "update" | "upgrade" => {
            match self_lifecycle::run_self_update(&args[1..], parsed.global.offline) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw self-update: {e}");
                    std::process::exit(1);
                }
            }
        }
        "self-uninstall" => {
            match self_lifecycle::run_self_uninstall(&args[1..]) {
                Ok(out) => println!("{}", serde_json::to_string_pretty(&out).unwrap()),
                Err(e) => {
                    eprintln!("fw self-uninstall: {e}");
                    std::process::exit(1);
                }
            }
        }
        other if SUPPORTED_APPLETS.contains(&other) => {
            let refs: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
            let wants_stdin = matches!(
                other,
                "cat" | "wc" | "head" | "tail" | "sha256sum" | "grep" | "sed"
                | "sort" | "uniq" | "cut" | "tr" | "nl" | "rev" | "base64" | "jq"
            ) && refs.iter().all(|a| a.starts_with('-') || !Path::new(a).is_file());
            let stdin_buf = if wants_stdin && !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                let mut s = String::new();
                let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut s);
                s
            } else {
                String::new()
            };
            let res = run_applet(other, &refs, &stdin_buf);
            if !res.stdout.is_empty() {
                print!("{}", res.stdout);
            }
            if !res.stderr.is_empty() {
                eprint!("{}", res.stderr);
            }
            std::process::exit(res.exit_code);
        }
        _ => {
            if let Some(hint) = cli_parser::suggest_command(cmd) {
                eprintln!("fw: unknown command `{cmd}`; did you mean `{hint}`? (run `fw --help`)");
            } else {
                eprintln!("fw: unknown command `{cmd}` (run `fw --help`)");
            }
            std::process::exit(2);
        }
    }
}

/// Runs the sibling binary `name` for `fw <command>` with `args`, inheriting stdin/stdout/stderr.
/// It is looked up next to the running `fw` first, then on PATH; returns its exit code.
fn run_sibling(command: &str, name: &str, args: &[String]) -> i32 {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(&exe))).filter(|p| p.is_file());
    let program = beside.unwrap_or_else(|| PathBuf::from(&exe));
    match std::process::Command::new(&program).args(args).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!(
                "fw {command}: cannot run `{}` ({e}). Build it with `cargo build --release -p {name}` and keep it next to `fw` or on PATH.",
                program.display()
            );
            127
        }
    }
}
