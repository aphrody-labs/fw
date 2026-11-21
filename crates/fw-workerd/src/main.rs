//! `fw-workerd compile|units|parity`: see the crate documentation.

use fw_workerd::compile::compile;
use fw_workerd::{parity, topology, unit, wrangler, write_text, Error, Result};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "fw-workerd <command>

  compile --service <name> --host <host> --bundle <dir> --out <dir> [--root <repo>]
      wrangler config + `wrangler deploy --dry-run --outdir <dir>` bundle -> <out>/<name>.capnp, shim modules, <name>.report.json
  units --host <host> --out <dir> [--root <repo>]
      systemd units, Garage config and the bootstrap script of one node from cluster/dual-mode.toml and cluster/build.toml
  bindings --wrangler <file> [--env <name>] [--link <service>=<addr>...]
      what the compiler wires for that wrangler config and what it cannot (JSON report, no bundle needed)
  probe --out <dir>
      writes the probe Worker (worker.js and wrangler.jsonc) that exercises every wired binding
  parity --spec <file.toml> --cloud <url> --self <url> [--repeat <n>] [--json <file>]
      same requests to both modes: status, headers, body hash, latency; exit 1 on any difference
";

fn option(arguments: &[String], name: &str) -> Option<String> {
    arguments.iter().position(|argument| argument == name).and_then(|index| arguments.get(index + 1)).cloned()
}

fn required(arguments: &[String], name: &str) -> Result<String> {
    option(arguments, name).ok_or_else(|| Error::Invalid(format!("missing {name}\n\n{USAGE}")))
}

fn run(arguments: &[String]) -> Result<ExitCode> {
    let root = PathBuf::from(option(arguments, "--root").unwrap_or_else(|| ".".to_owned()));
    match arguments.first().map(String::as_str) {
        Some("compile") => {
            let mode = topology::load_dual_mode(&root.join("cluster/dual-mode.toml"))?;
            let hosts = topology::load_hosts(&root.join("cluster/build.toml"))?;
            let host_name = required(arguments, "--host")?;
            let host = hosts.get(&host_name).ok_or_else(|| Error::Invalid(format!("no host `{host_name}` in cluster/build.toml")))?;
            let edge = mode.service(&required(arguments, "--service")?)?;
            let service = wrangler::load(&root.join(&edge.wrangler), edge.env.as_deref())?;
            let settings = unit::settings(host, &mode, edge, root.join(&edge.wrangler).parent().map(PathBuf::from).unwrap_or_default())?;
            let compiled = compile(&service, &PathBuf::from(required(arguments, "--bundle")?), &PathBuf::from(required(arguments, "--out")?), &settings)?;
            println!("{}", compiled.config_path.display());
            println!("directories: {}", compiled.directories.join(" "));
            println!("{}", serde_json::to_string_pretty(&compiled.report).map_err(|error| Error::Invalid(error.to_string()))?);
            Ok(ExitCode::SUCCESS)
        }
        Some("units") => {
            let mode = topology::load_dual_mode(&root.join("cluster/dual-mode.toml"))?;
            let hosts = topology::load_hosts(&root.join("cluster/build.toml"))?;
            let host_name = required(arguments, "--host")?;
            let host = hosts.get(&host_name).ok_or_else(|| Error::Invalid(format!("no host `{host_name}` in cluster/build.toml")))?;
            let out = PathBuf::from(required(arguments, "--out")?);
            let plan = unit::plan(&host_name, host, &mode, &root)?;
            for (name, content) in &plan.files {
                write_text(&out.join(name), content)?;
                println!("{}", out.join(name).display());
            }
            println!("buckets: {}", plan.buckets.join(", "));
            Ok(ExitCode::SUCCESS)
        }
        Some("bindings") => {
            let path = PathBuf::from(required(arguments, "--wrangler")?);
            let service = wrangler::load(&path, option(arguments, "--env").as_deref())?;
            let mut settings = fw_workerd::compile::Settings {
                listen_address: "127.0.0.1:0".to_owned(),
                state_directory: "state".to_owned(),
                storage: fw_workerd::compile::Storage::S3Bridge { bucket: "r2".to_owned(), endpoint: "127.0.0.1:0".to_owned(), region: "garage".to_owned() },
                linked_services: std::collections::BTreeMap::new(),
                secrets: Vec::new(),
                project_directory: path.parent().map(PathBuf::from).unwrap_or_default(),
            };
            for (index, argument) in arguments.iter().enumerate() {
                if argument == "--link"
                    && let Some((name, address)) = arguments.get(index + 1).and_then(|pair| pair.split_once('='))
                {
                    settings.linked_services.insert(name.to_owned(), address.to_owned());
                }
            }
            let (_, report) = fw_workerd::compile::render(&service, &[], &settings)?;
            println!("{}", serde_json::to_string_pretty(&report).map_err(|error| Error::Invalid(error.to_string()))?);
            Ok(ExitCode::SUCCESS)
        }
        Some("probe") => {
            let out = PathBuf::from(required(arguments, "--out")?);
            write_text(&out.join("worker.js"), fw_workerd::probe::WORKER)?;
            write_text(&out.join("wrangler.jsonc"), fw_workerd::probe::WRANGLER)?;
            println!("{}", out.display());
            Ok(ExitCode::SUCCESS)
        }
        Some("parity") => {
            let spec = parity::load_spec(&PathBuf::from(required(arguments, "--spec")?))?;
            let repeat = option(arguments, "--repeat").and_then(|text| text.parse().ok()).unwrap_or(0);
            let report = parity::run(&spec, &required(arguments, "--cloud")?, &required(arguments, "--self")?, repeat)?;
            let text = serde_json::to_string_pretty(&report).map_err(|error| Error::Invalid(error.to_string()))?;
            if let Some(path) = option(arguments, "--json") {
                write_text(&PathBuf::from(path), &text)?;
            }
            for verdict in &report.verdicts {
                println!("{} {}", if verdict.passed { "PASS" } else { "FAIL" }, verdict.id);
                for difference in &verdict.differences {
                    println!("     {difference}");
                }
            }
            for entry in &report.latency {
                println!(
                    "latency {}: cloud p50 {:.1} ms p95 {:.1} ms / self-hosted p50 {:.1} ms p95 {:.1} ms ({} samples each)",
                    entry.id, entry.cloud.p50_ms, entry.cloud.p95_ms, entry.selfhosted.p50_ms, entry.selfhosted.p95_ms, entry.cloud.samples
                );
            }
            Ok(if report.passed { ExitCode::SUCCESS } else { ExitCode::FAILURE })
        }
        _ => {
            print!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("fw-workerd: {error}");
            ExitCode::FAILURE
        }
    }
}
