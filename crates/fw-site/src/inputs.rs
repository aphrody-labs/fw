//! Readers for every input of the docs site. Each reader runs the real tool or parses the real file; nothing is typed in by hand.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command as Process, Stdio};

#[derive(Debug, Clone)]
pub struct Flag {
    pub name: String,
    pub argument: String,
}

#[derive(Debug, Clone)]
pub struct CliCommand {
    pub name: String,
    pub aliases: Vec<String>,
    pub group: String,
    pub summary: String,
    pub usage: String,
    pub flags: Vec<Flag>,
    pub subcommands: Vec<String>,
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct GlobalOption {
    pub flags: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct CliHelp {
    pub version: String,
    pub commands: Vec<CliCommand>,
    pub global_options: Vec<GlobalOption>,
    pub raw_root: String,
}

#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Debug, Clone)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub repo: String,
    pub path: String,
    pub license: String,
    pub edition: String,
    pub dependencies: Vec<String>,
    pub external_dependencies: usize,
    pub dependents: Vec<String>,
    pub features: Vec<String>,
    pub targets: Vec<(String, String)>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReleaseTarget {
    pub product: String,
    pub asset: String,
    pub os: String,
    pub arch: String,
    pub abi: String,
    pub kind: String,
    pub ship: bool,
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub method: String,
    pub path: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct ApiSurface {
    pub key: String,
    pub title: String,
    pub description: String,
    pub source: String,
    pub endpoints: Vec<Endpoint>,
}

fn run_capture(program: &str, args: &[&str], dir: Option<&Path>) -> Result<String> {
    let mut process = Process::new(program);
    process
        .args(args)
        .env("FW_OFFLINE", "1")
        .stdin(Stdio::null());
    if let Some(dir) = dir {
        process.current_dir(dir);
    }
    let output = process
        .output()
        .with_context(|| format!("cannot run `{program} {}`", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "`{program} {}` exited with {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"))
}

/// Parse `fw --help`, then `fw <command> --help` for each command it lists.
pub fn read_cli(fw: &str, dir: &Path) -> Result<CliHelp> {
    let raw_root = run_capture(fw, &["--help"], Some(dir))?;
    let mut lines = raw_root.lines();
    let first = lines.next().unwrap_or_default();
    let version = first
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let mut group = String::new();
    let mut listed: Vec<(String, Vec<String>, String, String)> = Vec::new();
    let mut global_options = Vec::new();
    let mut in_globals = false;
    for line in raw_root.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            let heading = line.trim_end_matches(':');
            in_globals = heading.eq_ignore_ascii_case("Global options");
            if line.ends_with(':') && !line.starts_with("Usage") {
                group = heading.to_owned();
            }
            continue;
        }
        if in_globals {
            let trimmed = line.trim();
            if let Some(split) = trimmed.find("  ") {
                global_options.push(GlobalOption {
                    flags: trimmed[..split].trim().to_owned(),
                    description: trimmed[split..].trim().to_owned(),
                });
            }
            continue;
        }
        if let Some((name, aliases, summary)) = parse_listing(line) {
            listed.push((name, aliases, summary, group.clone()));
        }
    }
    if listed.is_empty() {
        bail!("`{fw} --help` listed no commands; the output format changed");
    }
    let mut commands = Vec::new();
    for (name, aliases, summary, group) in listed {
        let raw = run_capture(fw, &[&name, "--help"], Some(dir)).unwrap_or_default();
        let usage = raw
            .lines()
            .find_map(|line| line.strip_prefix("Usage:"))
            .map(|usage| usage.trim().to_owned())
            .unwrap_or_else(|| format!("fw {name}"));
        let own_summary = raw
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or(summary);
        let flags = parse_flags(&usage, &own_summary);
        let subcommands = parse_subcommands(&usage, &name);
        commands.push(CliCommand {
            name,
            aliases,
            group,
            summary: own_summary,
            usage,
            flags,
            subcommands,
            raw,
        });
    }
    Ok(CliHelp {
        version,
        commands,
        global_options,
        raw_root,
    })
}

fn parse_listing(line: &str) -> Option<(String, Vec<String>, String)> {
    let body = line.trim_start();
    let name_end = body.find(char::is_whitespace)?;
    let name = body[..name_end].to_owned();
    let mut rest = body[name_end..].trim_start();
    let mut aliases = Vec::new();
    if let Some(after) = rest.strip_prefix('(') {
        let close = after.find(')')?;
        aliases = after[..close]
            .split(',')
            .map(|alias| alias.trim().to_owned())
            .filter(|alias| !alias.is_empty())
            .collect();
        rest = after[close + 1..].trim_start();
    }
    if rest.is_empty() {
        return None;
    }
    Some((name, aliases, rest.to_owned()))
}

fn parse_flags(usage: &str, summary: &str) -> Vec<Flag> {
    let mut flags: Vec<Flag> = Vec::new();
    for text in [usage, summary] {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        for (index, token) in tokens.iter().enumerate() {
            let cleaned = token.trim_matches(|c: char| "[](),`;".contains(c));
            if !cleaned.starts_with("--") || cleaned.len() < 3 {
                continue;
            }
            let (name, inline) = match cleaned.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (cleaned, None),
            };
            if !name[2..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                continue;
            }
            if flags.iter().any(|flag| flag.name == name) {
                continue;
            }
            let argument = inline.unwrap_or_else(|| {
                tokens
                    .get(index + 1)
                    .map(|next| next.trim_matches(|c: char| "[](),`;".contains(c)))
                    .filter(|next| {
                        next.starts_with('<')
                            || next.chars().next().is_some_and(|c| c.is_ascii_digit())
                    })
                    .map(str::to_owned)
                    .unwrap_or_default()
            });
            flags.push(Flag {
                name: name.to_owned(),
                argument,
            });
        }
    }
    flags
}

/// The first `<a|b|c>` or `[a|b|c]` group of the usage line, split at depth zero, first word of each alternative.
fn parse_subcommands(usage: &str, name: &str) -> Vec<String> {
    let after = usage
        .find(&format!("fw {name}"))
        .map(|index| &usage[index + 3 + name.len()..])
        .unwrap_or(usage);
    let Some(open) = after.find(['<', '[']) else {
        return Vec::new();
    };
    let mut depth = 0usize;
    let mut current = String::new();
    let mut parts: Vec<String> = Vec::new();
    for c in after[open + 1..].chars() {
        match c {
            '<' | '[' | '(' => {
                depth += 1;
                current.push(c);
            }
            '>' | ']' | ')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                current.push(c);
            }
            '|' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    if parts.len() < 2 {
        return Vec::new();
    }
    let mut names = Vec::new();
    for part in parts {
        let word = part
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        let word = word.trim_start_matches('-');
        if !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            && !names.iter().any(|existing| existing == word)
        {
            names.push(word.to_owned());
        }
    }
    if names.len() < 2 { Vec::new() } else { names }
}

/// Run the MCP server of the checkout over stdio and ask it for `tools/list`.
pub fn read_mcp_tools(bun: &str, root: &Path) -> Result<Vec<McpTool>> {
    let entry = root.join("src").join("fw_cli.ts");
    if !entry.is_file() {
        bail!("{} not found", entry.display());
    }
    let mut child = Process::new(bun)
        .arg(&entry)
        .arg("mcp")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot start `{bun} {} mcp`", entry.display()))?;
    {
        let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        stdin.write_all(
            br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fw-site","version":"0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
"#,
        )?;
    }
    let output = child.wait_with_output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("id") != Some(&Value::from(2)) {
            continue;
        }
        let tools = value
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("tools/list returned no tools: {line}"))?;
        let mut result: Vec<McpTool> = tools
            .iter()
            .filter_map(|tool| {
                Some(McpTool {
                    name: tool.get("name")?.as_str()?.to_owned(),
                    description: tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    schema: tool.get("inputSchema").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        result.sort_by(|left, right| left.name.cmp(&right.name));
        return Ok(result);
    }
    bail!("the MCP server answered no tools/list; stdout was {text:?}")
}

/// Workspace packages from `cargo metadata --no-deps`, with reverse workspace dependencies.
pub fn read_crates(root: &Path) -> Result<Vec<CrateInfo>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let text = run_capture(
        &cargo,
        &[
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ],
        Some(root),
    )?;
    let metadata: Value = serde_json::from_str(&text)?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| anyhow!("cargo metadata has no packages"))?;
    let names: Vec<&str> = packages
        .iter()
        .filter_map(|package| package["name"].as_str())
        .collect();
    let root_text = root.to_string_lossy().replace('\\', "/");
    let mut crates: Vec<CrateInfo> = packages
        .iter()
        .map(|package| {
            let manifest = package["manifest_path"]
                .as_str()
                .unwrap_or_default()
                .replace('\\', "/");
            let relative = manifest
                .strip_prefix(&format!("{}/", root_text.trim_end_matches('/')))
                .unwrap_or(&manifest)
                .trim_end_matches("/Cargo.toml")
                .to_owned();
            let repo = match relative.split('/').next().unwrap_or_default() {
                "aphrody" => "aphrody",
                "ay" => "ay",
                _ => "fw",
            }
            .to_owned();
            let mut dependencies: Vec<String> = Vec::new();
            let mut external = 0usize;
            for dependency in package["dependencies"].as_array().into_iter().flatten() {
                let dependency_name = dependency["name"].as_str().unwrap_or_default();
                if dependency["kind"].as_str() == Some("dev") {
                    continue;
                }
                if dependency["path"].is_string() || names.contains(&dependency_name) {
                    if !dependencies.iter().any(|known| known == dependency_name) {
                        dependencies.push(dependency_name.to_owned());
                    }
                } else {
                    external += 1;
                }
            }
            dependencies.sort();
            let mut features: Vec<String> = package["features"]
                .as_object()
                .map(|map| map.keys().cloned().collect())
                .unwrap_or_default();
            features.sort();
            let targets = package["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|target| {
                    let kind = target["kind"]
                        .as_array()
                        .and_then(|kinds| kinds.first())
                        .and_then(Value::as_str)
                        .unwrap_or("lib")
                        .to_owned();
                    (kind, target["name"].as_str().unwrap_or_default().to_owned())
                })
                .filter(|(kind, _)| {
                    matches!(kind.as_str(), "bin" | "lib" | "cdylib" | "proc-macro")
                })
                .collect();
            CrateInfo {
                name: package["name"].as_str().unwrap_or_default().to_owned(),
                version: package["version"].as_str().unwrap_or_default().to_owned(),
                description: package["description"]
                    .as_str()
                    .unwrap_or_default()
                    .replace('\n', " ")
                    .trim()
                    .to_owned(),
                repo,
                path: relative,
                license: package["license"].as_str().unwrap_or_default().to_owned(),
                edition: package["edition"].as_str().unwrap_or_default().to_owned(),
                dependencies,
                external_dependencies: external,
                dependents: Vec::new(),
                features,
                targets,
                keywords: package["keywords"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|keyword| keyword.as_str().map(str::to_owned))
                    .collect(),
            }
        })
        .collect();
    crates.sort_by(|left, right| left.name.cmp(&right.name));
    let edges: Vec<(String, String)> = crates
        .iter()
        .flat_map(|krate| {
            krate
                .dependencies
                .iter()
                .map(|dependency| (dependency.clone(), krate.name.clone()))
        })
        .collect();
    for krate in &mut crates {
        krate.dependents = edges
            .iter()
            .filter(|(dependency, _)| *dependency == krate.name)
            .map(|(_, dependent)| dependent.clone())
            .collect();
        krate.dependents.sort();
    }
    Ok(crates)
}

pub fn read_toml(path: &Path) -> Result<toml::Table> {
    let text = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    text.parse::<toml::Table>()
        .map_err(|error| anyhow!("{}: {error}", path.display()))
}

pub fn read_release_targets(root: &Path) -> Result<Vec<ReleaseTarget>> {
    let table = read_toml(&root.join("release.toml"))?;
    let mut targets = Vec::new();
    let Some(products) = table.get("products").and_then(toml::Value::as_table) else {
        return Ok(targets);
    };
    for (product, entry) in products {
        for target in entry
            .get("targets")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            let text = |key: &str| {
                target
                    .get(key)
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            targets.push(ReleaseTarget {
                product: product.clone(),
                asset: text("asset"),
                os: text("os"),
                arch: text("arch"),
                abi: text("abi"),
                kind: text("kind"),
                ship: target
                    .get("ship")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(true),
            });
        }
    }
    Ok(targets)
}

/// The live release manifest. With `live` it is fetched and the snapshot rewritten; otherwise the stored snapshot is used.
pub fn read_release_manifest(snapshot: &Path, live: bool, url: &str) -> Result<(String, Value)> {
    if live {
        let agent = ureq::Agent::new_with_defaults();
        let mut response = agent.get(url).call().with_context(|| url.to_owned())?;
        let body = response.body_mut().read_to_string()?;
        let value: Value = serde_json::from_str(&body)?;
        let pretty = serde_json::to_string_pretty(&value)? + "\n";
        return Ok((pretty, value));
    }
    let text = std::fs::read_to_string(snapshot).with_context(|| {
        format!(
            "{} is missing; run `fw site gen --live` once",
            snapshot.display()
        )
    })?;
    let value = serde_json::from_str(&text)?;
    Ok((text, value))
}

fn surface_from_paths(
    key: &str,
    title: &str,
    description: &str,
    source: &str,
    paths: &Value,
) -> ApiSurface {
    let mut endpoints = Vec::new();
    if let Some(map) = paths.as_object() {
        for (path, methods) in map {
            for (method, operation) in methods.as_object().into_iter().flatten() {
                if !matches!(
                    method.as_str(),
                    "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
                ) {
                    continue;
                }
                endpoints.push(Endpoint {
                    method: method.to_uppercase(),
                    path: path.clone(),
                    summary: operation
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                });
            }
        }
    }
    endpoints.sort_by(|left, right| {
        (left.path.as_str(), left.method.as_str())
            .cmp(&(right.path.as_str(), right.method.as_str()))
    });
    ApiSurface {
        key: key.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        source: source.to_owned(),
        endpoints,
    }
}

/// The OpenAPI documents: `fw openapi` (fw serve), the fw-edge Worker data files and the ay document.
pub fn read_api_surfaces(fw: &str, root: &Path) -> Result<Vec<ApiSurface>> {
    let mut surfaces = Vec::new();
    let serve: Value = serde_json::from_str(&run_capture(fw, &["openapi"], Some(root))?)?;
    surfaces.push(surface_from_paths(
        "fw-serve",
        "fw serve API",
        serve
            .pointer("/info/title")
            .and_then(Value::as_str)
            .unwrap_or("fw serve"),
        "fw openapi",
        &serve["paths"],
    ));
    let edge_paths = root.join("crates/fw-edge/data/openapi_paths.json");
    if edge_paths.is_file() {
        let paths: Value = serde_json::from_str(&std::fs::read_to_string(&edge_paths)?)?;
        let description =
            std::fs::read_to_string(root.join("crates/fw-edge/data/openapi_description.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<String>(&text).ok())
                .unwrap_or_default();
        surfaces.push(surface_from_paths(
            "fw-edge",
            "fw edge Worker API",
            &description,
            "crates/fw-edge/data/openapi_paths.json",
            &paths,
        ));
    }
    let ay_path = root.join("ay/docs/auto/openapi.json");
    if ay_path.is_file() {
        let document: Value = serde_json::from_str(&std::fs::read_to_string(&ay_path)?)?;
        let title = document
            .pointer("/info/title")
            .and_then(Value::as_str)
            .unwrap_or("ay API");
        surfaces.push(surface_from_paths(
            "ay",
            "ay Auto API",
            title,
            "ay/docs/auto/openapi.json",
            &document["paths"],
        ));
    }
    Ok(surfaces)
}

/// A level-two section of a markdown file, without its heading.
pub fn markdown_section(text: &str, heading: &str) -> Option<String> {
    let mut collecting = false;
    let mut found = false;
    let mut out = String::new();
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            if collecting {
                break;
            }
            collecting = title.trim() == heading;
            found |= collecting;
            continue;
        }
        if collecting {
            out.push_str(line);
            out.push('\n');
        }
    }
    found.then(|| out.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_lines_with_and_without_aliases() {
        let (name, aliases, summary) =
            parse_listing("  ask (chat)               Run the agent").unwrap();
        assert_eq!(
            (name.as_str(), aliases, summary.as_str()),
            ("ask", vec!["chat".to_owned()], "Run the agent")
        );
        let (name, aliases, _) = parse_listing("  mem (memory, claude-mem) Memory store").unwrap();
        assert_eq!(name, "mem");
        assert_eq!(aliases, vec!["memory", "claude-mem"]);
        assert!(parse_listing("  lonely").is_none());
    }

    #[test]
    fn subcommands_are_split_at_depth_zero() {
        assert_eq!(
            parse_subcommands(
                "fw learn <status|harvest|search> [--xfs-root <dir>]",
                "learn"
            ),
            vec!["status", "harvest", "search"]
        );
        assert_eq!(
            parse_subcommands(
                "fw cluster [summary|route <prompt>|k8s <status|nodes>]",
                "cluster"
            ),
            vec!["summary", "route", "k8s"]
        );
        assert!(parse_subcommands("fw ask [flags] <prompt>", "ask").is_empty());
    }

    #[test]
    fn flags_carry_their_placeholder() {
        let flags = parse_flags(
            "fw train [dataset.jsonl] [--rank 8] [--out <file>]",
            "LoRA (--steps 60 --json)",
        );
        let by_name = |name: &str| {
            flags
                .iter()
                .find(|flag| flag.name == name)
                .map(|flag| flag.argument.clone())
        };
        assert_eq!(by_name("--rank").as_deref(), Some("8"));
        assert_eq!(by_name("--out").as_deref(), Some("<file>"));
        assert_eq!(by_name("--json").as_deref(), Some(""));
    }

    #[test]
    fn markdown_section_stops_at_the_next_heading() {
        let text = "# t\n## A\none\ntwo\n## B\nthree\n";
        assert_eq!(markdown_section(text, "A").as_deref(), Some("one\ntwo"));
        assert_eq!(markdown_section(text, "B").as_deref(), Some("three"));
        assert_eq!(markdown_section(text, "C"), None);
    }
}
