//! Picks the generated Material Symbols tables (tools/material-cdn) and copies them to OUT_DIR.
//!
//! * default: the vendored tables in `vendor/` (offline, outlined only);
//! * features `rounded` / `sharp`, or `FW_MATERIAL_SOURCE=cdn`: the versioned tarball from the CDN, downloaded with
//!   `curl`, verified against the SHA-256 pinned in `vendor/PIN`, unpacked with `tar`;
//! * `FW_MATERIAL_TARBALL=<file>`: use a local tarball instead of downloading (still verified against the pin);
//! * `FW_MATERIAL_CDN=<base url>` overrides the base URL from the pin.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn pin() -> toml::Table {
    let raw = fs::read_to_string("vendor/PIN").expect("vendor/PIN");
    raw.parse().expect("vendor/PIN is not valid TOML")
}

fn sha256_hex(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    assert!(status.success(), "{cmd:?} failed: {status}");
}

fn tarball_source(out: &Path, pin: &toml::Table) -> PathBuf {
    let version = pin["version"].as_str().expect("pin.version");
    let want = pin["sha256"].as_str().expect("pin.sha256");
    let tgz = match env::var_os("FW_MATERIAL_TARBALL") {
        Some(p) => PathBuf::from(p),
        None => {
            let base = env::var("FW_MATERIAL_CDN")
                .unwrap_or_else(|_| pin["base"].as_str().expect("pin.base").to_string());
            let dest = out.join("material-symbols-data.tar.gz");
            let url = format!("{}/rust/{version}.tar.gz", base.trim_end_matches('/'));
            run(Command::new("curl")
                .args([
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--location",
                    "--retry",
                    "3",
                    "-o",
                ])
                .arg(&dest)
                .arg(&url));
            dest
        }
    };
    let got = sha256_hex(&tgz);
    assert_eq!(
        got, want,
        "material-symbols-data {version}: SHA-256 {got} does not match the pin {want} in vendor/PIN"
    );
    let dir = out.join("material-symbols-data");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create extract dir");
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&tgz)
        .arg("-C")
        .arg(&dir)
        .args(["--strip-components=1"]));
    dir.join("src")
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor");
    for var in [
        "FW_MATERIAL_SOURCE",
        "FW_MATERIAL_CDN",
        "FW_MATERIAL_TARBALL",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let styles: Vec<&str> = ["outlined", "rounded", "sharp"]
        .into_iter()
        .filter(|s| env::var_os(format!("CARGO_FEATURE_{}", s.to_uppercase())).is_some())
        .collect();
    let cdn = env::var("FW_MATERIAL_SOURCE").is_ok_and(|v| v == "cdn")
        || env::var_os("FW_MATERIAL_TARBALL").is_some()
        || styles.iter().any(|s| *s != "outlined");
    let pin = pin();
    let src = if cdn {
        tarball_source(&out, &pin)
    } else {
        PathBuf::from("vendor")
    };
    let mut tables = vec!["codepoints".to_string()];
    for s in &styles {
        tables.push((*s).to_string());
        tables.push(format!("{s}_fill"));
    }
    for t in tables {
        let from = src.join(format!("{t}.rs"));
        fs::copy(&from, out.join(format!("{t}.rs")))
            .unwrap_or_else(|e| panic!("copy {}: {e}", from.display()));
    }
    println!(
        "cargo:rustc-env=FW_MATERIAL_VERSION={}",
        pin["version"].as_str().expect("pin.version")
    );
    println!(
        "cargo:rustc-env=FW_MATERIAL_SOURCE_KIND={}",
        if cdn { "cdn" } else { "vendored" }
    );
}
