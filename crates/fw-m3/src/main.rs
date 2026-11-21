//! `fw-m3 css [--seed #rrggbb] [--variant tonal-spot] [--write FILE]`
//!
//! Prints the `--md-sys-color-*` roles for both modes, or splices them into FILE between the
//! `m3-colors` markers. Seed and variant default to `[brand]` in the nearest `product.toml`.

use fw_m3::scheme::{SchemeVariant, css_custom_properties, parse_seed, splice_css};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn brand_from_product_toml() -> Result<(String, String), String> {
    let start = std::env::current_dir().map_err(|e| e.to_string())?;
    let file = start
        .ancestors()
        .map(|d| d.join("product.toml"))
        .find(|p| p.is_file())
        .ok_or("no product.toml in this directory or its parents; pass --seed")?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let doc: toml::Table = text
        .parse()
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let brand = doc
        .get("brand")
        .and_then(|b| b.as_table())
        .ok_or(format!("{}: no [brand] table", file.display()))?;
    let get = |k: &str| brand.get(k).and_then(|v| v.as_str()).map(str::to_owned);
    Ok((
        get("seed").ok_or(format!("{}: [brand] seed missing", file.display()))?,
        get("variant").unwrap_or_else(|| "tonal-spot".into()),
    ))
}

fn run(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("css") {
        return Err(
            "usage: fw-m3 css [--seed #rrggbb] [--variant tonal-spot] [--write FILE]".into(),
        );
    }
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let (seed, variant) = match (flag("--seed"), flag("--variant")) {
        (Some(s), v) => (s, v.unwrap_or_else(|| "tonal-spot".into())),
        (None, v) => {
            let (s, pv) = brand_from_product_toml()?;
            (s, v.unwrap_or(pv))
        }
    };
    let seed_rgb = parse_seed(&seed).ok_or(format!("seed `{seed}` is not #rrggbb"))?;
    let variant = SchemeVariant::from_slug(&variant).ok_or(format!(
        "variant `{variant}` is not one of {}",
        SchemeVariant::SLUGS.join(", ")
    ))?;
    let usable = fw_m3::scheme::usable_seed(seed_rgb, variant);
    if usable != seed_rgb {
        eprintln!(
            "fw-m3: seed {seed} has HCT chroma {:.1}, under the AOSP minimum of {}; using #{usable:06x} as Android does",
            fw_m3::scheme::seed_chroma(seed_rgb),
            fw_m3::scheme::MIN_SEED_CHROMA
        );
    }
    let seed_rgb = usable;
    let css = css_custom_properties(seed_rgb, variant);
    match flag("--write").map(PathBuf::from) {
        None => print!("{css}"),
        Some(path) => write_into(&path, &css)?,
    }
    Ok(())
}

fn write_into(path: &Path, css: &str) -> Result<(), String> {
    let file = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let next = splice_css(&file, css).ok_or(format!(
        "{}: m3-colors:begin/end markers not found",
        path.display()
    ))?;
    let changed = next != file;
    if changed {
        std::fs::write(path, next).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    eprintln!(
        "{}: m3 colors {}",
        path.display(),
        if changed { "updated" } else { "unchanged" }
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fw-m3: {e}");
            ExitCode::from(2)
        }
    }
}
