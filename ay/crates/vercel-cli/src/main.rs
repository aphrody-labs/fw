#![doc = "AOT-compiled unified Rust CLI (`vercel-rs`) for Vercel, Next.js, Turbopack, and Turborepo."]
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::{env, fs, path::Path, process::ExitCode};

use vercel_config::{AyVercelManifest, validate_vercel_json};
use vercel_skills::{WellKnownSkillsIndex, verify_entry_bytes, verify_frontmatter};

fn run_audit(root: &Path) -> Result<usize, String> {
    let manifest_raw = fs::read_to_string(root.join("ay-vercel.toml"))
        .map_err(|e| format!("failed reading ay-vercel.toml: {e}"))?;
    AyVercelManifest::from_toml_str(&manifest_raw).map_err(|e| e.to_string())?;

    let vercel_json_raw = fs::read_to_string(root.join("apps/web/vercel.json"))
        .map_err(|e| format!("failed reading apps/web/vercel.json: {e}"))?;
    validate_vercel_json(&vercel_json_raw).map_err(|e| e.to_string())?;

    let idx_raw = fs::read_to_string(root.join(".well-known/agent-skills/index.json"))
        .map_err(|e| format!("failed reading .well-known/agent-skills/index.json: {e}"))?;
    let idx: WellKnownSkillsIndex =
        serde_json::from_str(&idx_raw).map_err(|e| format!("invalid index.json: {e}"))?;

    for entry in &idx.skills {
        let rel = entry.url.trim_start_matches('/');
        let bytes =
            fs::read(root.join(rel)).map_err(|e| format!("missing skill file `{rel}`: {e}"))?;
        verify_entry_bytes(entry, &bytes).map_err(|e| e.to_string())?;
        let text =
            std::str::from_utf8(&bytes).map_err(|e| format!("invalid UTF-8 in `{rel}`: {e}"))?;
        verify_frontmatter(text).map_err(|e| format!("`{rel}`: {e}"))?;
    }
    Ok(idx.skills.len())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map_or("info", String::as_str);

    match cmd {
        "audit" => {
            let root = args.get(1).map_or_else(|| Path::new("."), Path::new);
            match run_audit(root) {
                Ok(count) => {
                    println!(
                        "vercel-rs audit OK: {} unified crates, {} SHA-256 skills verified (host={})",
                        next_bun::UNIFIED_CRATES.len(),
                        count,
                        next_bun::bun_host_tag()
                    );
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("vercel-rs audit error: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            println!(
                "vercel-rs {} — unified Vercel + Next.js + Turbopack + Turborepo AOT engine ({} crates, host={})",
                env!("CARGO_PKG_VERSION"),
                next_bun::UNIFIED_CRATES.len(),
                next_bun::bun_host_tag()
            );
            ExitCode::SUCCESS
        }
    }
}
