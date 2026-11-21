//! Embeds every `themes/<name>/theme.toml` of the repository so the binary can switch to a shipped theme without a checkout.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let themes = manifest.join("../../themes");
    println!("cargo:rerun-if-changed={}", themes.display());
    let mut names: Vec<String> = std::fs::read_dir(&themes)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().join("theme.toml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut generated = String::from("pub const BUNDLED: &[(&str, &str)] = &[\n");
    for name in &names {
        let file = themes.join(name).join("theme.toml");
        generated.push_str(&format!("    ({name:?}, include_str!({:?})),\n", file.display().to_string()));
        println!("cargo:rerun-if-changed={}", file.display());
    }
    generated.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default()).join("bundled.rs");
    if let Err(error) = std::fs::write(&out, generated) {
        panic!("fw-theme build: {}: {error}", out.display());
    }
}
