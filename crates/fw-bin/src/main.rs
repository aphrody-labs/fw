//! `fairwind-native` (`fw-bin`) — Single unified zero-system-dependency Fat-LTO binary
//! across Win 11 RTX 4070 (`msvc` static CRT), Linux 7.3 12-core 45 GB RAM (`musl` static),
//! 3 VPS Pingora/Dynamo/vLLM-Router nodes, 4 GPU hosts, and End-User WebGPU/FPU peers.

use fw_cluster::HybridClusterRouter;
use fw_compute::{detect_hardware_profile, run_compute_smoke_bench};
use fw_ggml::{encode_gguf_header, parse_gguf_header, rpc_get_device_memory, spawn_rpc_server_once};
use fw_proto::{build_openapi_3_1, serve};
use fw_ui_blitz::render_cluster_hud_png;
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("status");

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
