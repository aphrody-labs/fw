//! Build script for C and NASM x86_64 media kernels.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/luma.c");
    println!("cargo:rerun-if-changed=src/luma_x86_64.asm");
    println!("cargo:rustc-check-cfg=cfg(has_nasm_simd)");

    cc::Build::new()
        .file("src/luma.c")
        .opt_level(3)
        .compile("ay_media_c");

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if arch == "x86_64" {
        let format = if os == "windows" { "win64" } else { "elf64" };
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_default());
        let obj_path = out_dir.join("luma_x86_64.o");
        let status = Command::new("nasm")
            .args(["-f", format, "-o"])
            .arg(&obj_path)
            .arg("src/luma_x86_64.asm")
            .status();

        if let Ok(s) = status
            && s.success()
        {
            println!("cargo:rustc-link-arg={}", obj_path.display());
            println!("cargo:rustc-cfg=has_nasm_simd");
        }
    }
}
