#![forbid(unsafe_code)]

//! `cargo xtask` — Zero-dependency pure-Rust orchestration replacing all fragmented
//! TS/shell infra scripts (`tools/ci.ts`, `src/repo_map.ts`, `src/upstream_skills.ts`).
//!
//! Commands:
//!   cargo xtask ci [fast|all]   — Multi-threaded parallel multi-language CI lanes
//!   cargo xtask map             — Deterministic byte-exact MAP.tsv generator
//!   cargo xtask purity          — Zero-hardcoded-path & workspace purity verifier
//!   cargo xtask sync-upstreams  — Live Git-pinned upstreams skill + TSV generator
//!   cargo xtask package         — Verify crates.io packaging of fw-* crates
//!   cargo xtask doctor          — Verify installed local toolchain

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Instant;

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf).unwrap_or(manifest_dir)
}

fn run_cmd(root: &Path, program: &str, args: &[&str]) -> (bool, i32, u128, String) {
    let t0 = Instant::now();
    let out = Command::new(program).args(args).current_dir(root).output();
    let ms = t0.elapsed().as_millis();
    match out {
        Ok(o) => {
            let code = o.status.code().unwrap_or(-1);
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            let tail = combined
                .lines()
                .map(str::trim_end)
                .filter(|l| !l.is_empty())
                .rev()
                .take(3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join(" | ");
            (o.status.success(), code, ms, tail)
        }
        Err(e) => (false, -1, ms, format!("spawn error ({program}): {e}")),
    }
}

fn role_for(dir: &str) -> (&'static str, &'static str) {
    match dir {
        "crates/fw-bin" => ("rust-core", "cargo build --release -p fw-bin --bin fw"),
        "crates/fw-wasm" => ("rust-wasm", "cargo build --release --target wasm32-unknown-unknown -p fw-wasm"),
        d if d.starts_with("crates/fw-") => ("rust-core", "cargo test --workspace"),
        "xtask" => ("rust-xtask", "cargo xtask purity"),
        "native" => ("rust-ffi-abi", "cargo test --manifest-path native/fairwind-abi/Cargo.toml"),
        "os/fairwind-os" => ("asterinas-osdk", "bun test tests/fairwind_v4_os_uutils_nix.test.ts"),
        "os/fairwind-wsl" => ("wsl-zero-legacy", "bun test tests/fairwind_v4_os_uutils_nix.test.ts"),
        "nix" => ("nix-fleet", "bun test tests/fairwind_v4_os_uutils_nix.test.ts"),
        "cloudflare" => ("edge-worker", "bun run cf:dry-run"),
        "vercel" => ("edge-vercel", "bun test tests/fairwind_decisions_vercel.test.ts"),
        "web" => ("control-plane-ui", "bun run cf:dry-run"),
        "src" => ("bun-cli-aot", "bun test"),
        "tests" => ("e2e-tests", "bun test"),
        "plugin" => ("agy-plugin", "dist/fairwind.exe plugin sync --install"),
        "cluster" => ("topology-spec", "bun test tests/fairwind_v3_cluster_webgpu.test.ts"),
        "configs/heretic" => ("heretic-presets", "uvx ruff check ."),
        "docs" => ("architecture-docs", "read-only"),
        "core/py" => ("python-core", "uvx ruff check ."),
        "core/rs" => ("rust-embedded", "cargo check --manifest-path core/rs/wta/Cargo.toml"),
        _ => ("misc", "just ci"),
    }
}

fn cmd_map(root: &Path) -> ExitCode {
    let out = match Command::new("git").args(["ls-files"]).current_dir(root).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => {
            eprintln!("git ls-files failed");
            return ExitCode::FAILURE;
        }
    };
    let mut buckets: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for rel in String::from_utf8_lossy(&out).lines().filter(|l| !l.is_empty()) {
        let abs = root.join(rel);
        let Ok(meta) = fs::metadata(&abs) else { continue };
        if !meta.is_file() {
            continue;
        }
        let parts: Vec<&str> = rel.split('/').collect();
        let key = if parts.len() == 1 {
            "(root)".to_string()
        } else {
            let p2 = format!("{}/{}", parts[0], parts[1]);
            if p2.starts_with("crates/fw-")
                || p2 == "os/fairwind-os"
                || p2 == "os/fairwind-wsl"
                || p2 == "configs/heretic"
                || p2 == "core/py"
                || p2 == "core/rs"
            {
                p2
            } else {
                parts[0].to_string()
            }
        };
        let entry = buckets.entry(key).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += meta.len();
    }

    let mut lines = vec!["dir\ttier\tfiles\tbytes\tgate".to_string()];
    for (dir, (files, bytes)) in &buckets {
        let (tier, gate) = role_for(dir);
        lines.push(format!("{dir}\t{tier}\t{files}\t{bytes}\t{gate}"));
    }
    let body = format!("{}\n", lines.join("\n"));
    let map_path = root.join("MAP.tsv");
    if let Err(e) = fs::write(&map_path, &body) {
        eprintln!("failed writing MAP.tsv: {e}");
        return ExitCode::FAILURE;
    }
    println!(
        "{{\"ok\":true,\"rows\":{},\"bytes\":{},\"path\":\"MAP.tsv\"}}",
        buckets.len(),
        body.len()
    );
    ExitCode::SUCCESS
}

fn cmd_purity(root: &Path) -> ExitCode {
    let out = match Command::new("git").args(["ls-files"]).current_dir(root).output() {
        Ok(o) if o.status.success() => o.stdout,
        _ => return ExitCode::FAILURE,
    };
    let forbidden = [
        concat!("C:\\", "Users\\", "aphro"),
        concat!("C:\\", "src\\", "fairwind"),
        "defaultUser = \"aphro\"",
    ];
    let text = String::from_utf8_lossy(&out);
    let files: Vec<&str> = text
        .lines()
        .filter(|rel| !rel.is_empty() && !rel.starts_with("packages/") && !rel.starts_with("third_party/") && !rel.ends_with(".lock") && !rel.ends_with(".png"))
        .collect();
    let workers = thread::available_parallelism().map(usize::from).unwrap_or(4).min(files.len().max(1));
    let chunk_size = files.len().div_ceil(workers);
    let mut violations: Vec<String> = thread::scope(|s| {
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = files
            .chunks(chunk_size.max(1))
            .map(|chunk| {
                s.spawn(move || {
                    let mut local = Vec::new();
                    for &rel in chunk {
                        let Ok(txt) = fs::read_to_string(root.join(rel)) else { continue };
                        for needle in &forbidden {
                            if txt.contains(needle) {
                                local.push(format!("{rel}: contains hardcoded `{needle}`"));
                            }
                        }
                    }
                    local
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    let product_ok = root.join("product.toml").exists() && root.join("LICENSE").exists();
    if !product_ok {
        violations.push("missing product.toml or LICENSE".into());
    }
    if violations.is_empty() {
        println!("{{\"ok\":true,\"purity\":\"PASS\",\"violations\":0}}");
        ExitCode::SUCCESS
    } else {
        for v in &violations {
            eprintln!("PURITY VIOLATION: {v}");
        }
        ExitCode::FAILURE
    }
}

fn cmd_ci(root: &Path, mode: &str) -> ExitCode {
    let mut lanes: Vec<(&str, &str, Vec<&str>)> = vec![
        ("rust-clippy", "cargo", vec!["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
        ("rust-test", "cargo", vec!["test", "--workspace"]),
        ("python-ruff", "uvx", vec!["ruff", "check", "."]),
        ("bun-test", "bun", vec!["test", "--timeout=30000"]),
    ];
    if mode != "fast" {
        lanes.extend([
            ("native-pe", "cargo", vec!["test", "--manifest-path", "native/fairwind-pe/Cargo.toml"]),
            ("native-sniff", "cargo", vec!["test", "--manifest-path", "native/fairwind-sniff/Cargo.toml"]),
            ("native-abi", "cargo", vec!["test", "--manifest-path", "native/fairwind-abi/Cargo.toml"]),
            ("wasm32", "cargo", vec!["build", "--release", "--target", "wasm32-unknown-unknown", "-p", "fw-wasm"]),
            ("cf-dry-run", "bunx", vec!["wrangler", "deploy", "--dry-run", "-c", "cloudflare/wrangler.jsonc"]),
        ]);
    }

    let t0 = Instant::now();
    let (purity_ok, results) = thread::scope(|s| {
        let purity_h = s.spawn(|| cmd_purity(root) == ExitCode::SUCCESS);
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = lanes
            .into_iter()
            .map(|(id, prog, args)| {
                s.spawn(move || {
                    let (ok, code, ms, tail) = run_cmd(root, prog, &args);
                    (id, ok, code, ms, tail)
                })
            })
            .collect();
        let res: Vec<(&str, bool, i32, u128, String)> = handles
            .into_iter()
            .map(|h| h.join().unwrap_or(("join-panic", false, -1, 0, "thread panicked".into())))
            .collect();
        (purity_h.join().unwrap_or(false), res)
    });

    let wall_ms = t0.elapsed().as_millis();
    let map_ok = cmd_map(root) == ExitCode::SUCCESS;
    let all_ok = purity_ok && map_ok && results.iter().all(|r| r.1);

    println!("=== fw xtask ci ({mode}) — wall={wall_ms}ms ok={all_ok} ===");
    for (id, ok, code, ms, tail) in &results {
        println!("  [{}] {:<14} {:>5}ms (exit {}) {}", if *ok { "PASS" } else { "FAIL" }, id, ms, code, tail);
    }
    if all_ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn cmd_package(root: &Path) -> ExitCode {
    let (ok, code, ms, tail) = run_cmd(
        root,
        "cargo",
        &["package", "--allow-dirty", "-p", "fw-wasm", "-p", "fw-ggml", "-p", "fw-uutils", "-p", "fw-nix", "-p", "fw-hermes-claw"],
    );
    println!("{{\"ok\":{ok},\"exit\":{code},\"ms\":{ms},\"tail\":\"{}\"}}", tail.replace('"', "'"));
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn cmd_doctor(root: &Path) -> ExitCode {
    let tools = ["cargo", "rustc", "just", "bun", "uv", "rg", "fd", "nu", "duckdb", "git"];
    let results: Vec<(&str, bool)> = thread::scope(|s| {
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = tools
            .into_iter()
            .map(|t| s.spawn(move || (t, run_cmd(root, t, &["--version"]).0)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(("?", false))).collect()
    });
    let mut ok_all = true;
    for (t, ok) in results {
        println!("  {:<8} {}", t, if ok { "OK" } else { "MISSING" });
        ok_all &= ok;
    }
    if ok_all { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn cmd_sync_upstreams(root: &Path) -> ExitCode {
    let toml_path = root.join("upstreams.toml");
    let Ok(src) = fs::read_to_string(&toml_path) else {
        eprintln!("missing upstreams.toml");
        return ExitCode::FAILURE;
    };
    let specs: Vec<(String, String, String, String, String, String)> = src
        .split("[[upstream]]")
        .skip(1)
        .map(|block| {
            let get = |key: &str| -> String {
                for line in block.lines() {
                    let l = line.trim();
                    if let Some(rest) = l.strip_prefix(key) {
                        if let Some(val) = rest.trim().strip_prefix('=') {
                            return val.trim().trim_matches('"').to_string();
                        }
                    }
                }
                String::new()
            };
            let branch = {
                let b = get("branch");
                if b.is_empty() { "main".to_string() } else { b }
            };
            (get("id"), get("repo"), branch, get("docs"), get("fairwind"), get("role"))
        })
        .collect();

    let t0 = Instant::now();
    let rows: Vec<(String, String, String, String, String, String, String)> = thread::scope(|s| {
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = specs
            .into_iter()
            .map(|(id, repo, branch, docs, fairwind, role)| {
                s.spawn(move || {
                    let url = format!("https://github.com/{repo}.git");
                    let refspec = format!("refs/heads/{branch}");
                    let (ok, _, _, out) = run_cmd(root, "git", &["ls-remote", &url, &refspec]);
                    let sha = if ok {
                        out.split_whitespace().next().unwrap_or("unknown").chars().take(10).collect()
                    } else {
                        "offline".to_string()
                    };
                    (id, repo, branch, sha, fairwind, docs, role)
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect()
    });

    let ref_dir = root.join("plugin").join("skills").join("upstreams").join("references");
    let _ = fs::create_dir_all(&ref_dir);
    let mut tsv = String::with_capacity(rows.len() * 160);
    tsv.push_str("id\trepo\tbranch\thead_sha\tfairwind\tdocs\trole\n");
    let mut md = String::with_capacity(rows.len() * 140 + 512);
    md.push_str(
        "---\nname: upstreams\ndescription: >-\n  Auto-generated Git-pinned upstream reference catalog for fw (llama.cpp, Asterinas,\n  Cloudflare OS/Computer/Pingora/Kitesurf/lol-html, rmcp, ZeroClaw, Zed, nix-cachyos-kernel,\n  scx, Cloud Hypervisor, uutils, OpenVMM, IronRDP). Refresh anytime via `just sync-upstreams`.\n---\n\n# fw Auto-Generated Upstream Skill Catalog\n\nRefresh via `just sync-upstreams` (`cargo xtask sync-upstreams`).\nIndex: [references/UPSTREAMS.tsv](references/UPSTREAMS.tsv).\n\n| Upstream | HEAD SHA | Target | Docs |\n|---|---|---|---|\n",
    );
    for (id, repo, branch, sha, fairwind, docs, role) in &rows {
        tsv.push_str(&format!("{id}\t{repo}\t{branch}\t{sha}\t{fairwind}\t{docs}\t{role}\n"));
        md.push_str(&format!("| `{repo}` | `{sha}` | `{fairwind}` | [docs]({docs}) |\n"));
    }
    let _ = fs::write(ref_dir.join("UPSTREAMS.tsv"), tsv);
    let _ = fs::write(root.join("plugin").join("skills").join("upstreams").join("SKILL.md"), md);
    println!("{{\"ok\":true,\"upstreams\":{},\"ms\":{}}}", rows.len(), t0.elapsed().as_millis());
    ExitCode::SUCCESS
}

fn cmd_upgrade(root: &Path) -> ExitCode {
    let t0 = Instant::now();
    let (sync_ok, ci_ok) = thread::scope(|s| {
        let sync_h = s.spawn(|| cmd_sync_upstreams(root) == ExitCode::SUCCESS);
        let ci_h = s.spawn(|| cmd_ci(root, "fast") == ExitCode::SUCCESS);
        (sync_h.join().unwrap_or(false), ci_h.join().unwrap_or(false))
    });
    println!("{{\"ok\":{},\"sync\":{},\"ci\":{},\"wall_ms\":{}}}", sync_ok && ci_ok, sync_ok, ci_ok, t0.elapsed().as_millis());
    if sync_ok && ci_ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Unified parallel Feature-to-Deploy pipeline (`cargo xtask ship`):
/// Stage 1: Parallel fast CI gate (purity + clippy + rust-test + ruff + bun-test + MAP.tsv)
/// Stage 2: Parallel Fat-LTO fw + Wasm32 + Bun 1.4.5 AOT `--trim` + Cloudflare dry-run
fn cmd_ship(root: &Path) -> ExitCode {
    let t0 = Instant::now();
    if cmd_ci(root, "fast") != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    let build_lanes: Vec<(&str, &str, Vec<&str>)> = vec![
        ("fw-fat-lto", "cargo", vec!["build", "--release", "-p", "fw-bin", "--bin", "fw"]),
        ("fw-wasm32", "cargo", vec!["build", "--release", "--target", "wasm32-unknown-unknown", "-p", "fw-wasm"]),
        (
            "bun-aot-cli",
            "bun",
            vec![
                "build",
                "--compile",
                "--aot",
                "--trim",
                "--no-compile-function-source",
                "--no-compile-autoload-dotenv",
                "--no-compile-autoload-bunfig",
                "src/fairwind_cli.ts",
                "--outfile",
                "dist/fairwind.exe",
            ],
        ),
        ("cf-dry-run", "bunx", vec!["wrangler", "deploy", "--dry-run", "-c", "cloudflare/wrangler.jsonc"]),
    ];
    let results: Vec<(&str, bool, i32, u128, String)> = thread::scope(|s| {
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = build_lanes
            .into_iter()
            .map(|(id, prog, args)| {
                s.spawn(move || {
                    let (ok, code, ms, tail) = run_cmd(root, prog, &args);
                    (id, ok, code, ms, tail)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(("join-panic", false, -1, 0, "thread panicked".into())))
            .collect()
    });
    let all_ok = results.iter().all(|r| r.1);
    let wall_ms = t0.elapsed().as_millis();
    println!("=== fw xtask ship — wall={wall_ms}ms ok={all_ok} ===");
    for (id, ok, code, ms, tail) in &results {
        println!("  [{}] {:<14} {:>5}ms (exit {}) {}", if *ok { "PASS" } else { "FAIL" }, id, ms, code, tail);
    }
    if all_ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn main() -> ExitCode {
    let root = repo_root();
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ci") => cmd_ci(&root, args.get(1).map(String::as_str).unwrap_or("all")),
        Some("map") => cmd_map(&root),
        Some("purity") => cmd_purity(&root),
        Some("package") => cmd_package(&root),
        Some("doctor") => cmd_doctor(&root),
        Some("sync-upstreams") => cmd_sync_upstreams(&root),
        Some("upgrade") => cmd_upgrade(&root),
        Some("ship") => cmd_ship(&root),
        _ => {
            println!("Usage: cargo xtask <ci [fast|all]|map|purity|package|doctor|sync-upstreams|upgrade|ship>");
            ExitCode::SUCCESS
        }
    }
}
