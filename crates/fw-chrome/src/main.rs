use anyhow::{anyhow, bail, Context, Result};
use fw_chrome::{install_script, ssh, ssh_target, BrowserConfig};
use std::path::{Path, PathBuf};

const USAGE: &str = "fw chrome <render|apply|status> [--config cluster/chrome.toml]
  render   write the Windows registry file (windows_out) and print the Linux policy JSON; --linux or --windows selects one
  apply    install the Linux policy JSON on every node of [apply] (--nodes n1,n2 narrows); the Windows file is never applied here
  status   compare the policy file on every node with the rendered one; exit 1 on any difference";

fn main() {
    if let Err(error) = run() {
        eprintln!("fw chrome: {error:#}");
        std::process::exit(2);
    }
}

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|index| args.get(index + 1)).cloned()
}

fn load(args: &[String]) -> Result<(BrowserConfig, PathBuf)> {
    let path = option(args, "--config").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cluster/chrome.toml"));
    let root = path.parent().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let product = std::fs::read_to_string(root.join("product.toml")).with_context(|| format!("read {}", root.join("product.toml").display()))?;
    let product: toml::Table = toml::from_str(&product)?;
    let seed = product
        .get("brand")
        .and_then(|brand| brand.get("seed"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow!("product.toml has no [brand] seed"))?
        .to_string();
    let text = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok((BrowserConfig::parse(&text, &seed)?, root))
}

fn nodes(config: &BrowserConfig, args: &[String]) -> Vec<String> {
    match option(args, "--nodes") {
        Some(list) => list.split(',').map(str::to_string).collect(),
        None => config.apply.nodes.clone(),
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).ok_or_else(|| anyhow!(USAGE))?;
    let (config, root) = load(&args)?;
    let build_toml = std::fs::read_to_string(root.join("cluster").join("build.toml")).unwrap_or_default();
    match command {
        "render" => {
            let only_linux = args.iter().any(|arg| arg == "--linux");
            let only_windows = args.iter().any(|arg| arg == "--windows");
            if !only_windows {
                print!("{}", config.linux_json()?);
            }
            if !only_linux {
                let path = root.join(&config.apply.windows_out);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, config.windows_reg()?)?;
                eprintln!("wrote {}", std::path::absolute(&path).unwrap_or(path).display());
            }
        }
        "apply" => {
            let json = config.linux_json()?;
            for node in nodes(&config, &args) {
                let target = ssh_target(&build_toml, &node)?;
                ssh(&target, &install_script(&config), Some(&json))?;
                println!("{node}: {} written", config.linux_paths().join(", "));
            }
        }
        "status" => {
            let json = config.linux_json()?;
            let mut different = 0;
            for node in nodes(&config, &args) {
                let target = ssh_target(&build_toml, &node)?;
                let mut in_sync = true;
                for path in config.linux_paths() {
                    let live = ssh(&target, &format!("cat '{path}' 2>/dev/null || true"), None)?;
                    in_sync &= live == json;
                }
                let state = if in_sync { "in sync" } else { "differs" };
                if !in_sync {
                    different += 1;
                }
                println!("{node}: {state}");
            }
            if different > 0 {
                bail!("{different} node(s) differ from cluster/chrome.toml");
            }
        }
        _ => bail!(USAGE),
    }
    Ok(())
}
