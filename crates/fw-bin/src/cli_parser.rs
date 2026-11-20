#![forbid(unsafe_code)]

//! Zero-dependency CLI argument parser & command registry for `fw`.
//! Modeled on `bun_clap` (zero external `clap_derive` proc-macro bloat, `--flag=val` & `--flag val`,
//! global flags anywhere, short aliases `i`/`a`/`rm`/`x`/`s`, per-command `--help`, typo suggestions)
//! and `uv-cli` (`-C/--directory`, `--offline`, `--config`, `--color`, `-q`/`-v`, `FW_*` env vars).

use std::path::PathBuf;

pub const FW_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone)]
pub struct GlobalOpts {
    pub directory: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub offline: bool,
    pub quiet: u8,
    pub verbose: u8,
    pub color: ColorChoice,
    pub json: bool,
    pub help: bool,
    pub version: bool,
    pub continue_session: bool,
}

impl Default for GlobalOpts {
    fn default() -> Self {
        let directory = std::env::var("FW_WORKING_DIR")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let config_file = std::env::var("FW_CONFIG")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        let offline = matches!(
            std::env::var("FW_OFFLINE").ok().as_deref(),
            Some("1" | "true" | "yes")
        );
        Self {
            directory,
            config_file,
            offline,
            quiet: 0,
            verbose: 0,
            color: ColorChoice::Auto,
            json: false,
            help: false,
            version: false,
            continue_session: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedCli {
    pub global: GlobalOpts,
    pub command: Option<String>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct CommandMeta {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub group: &'static str,
    pub usage: &'static str,
    pub summary: &'static str,
}

pub const COMMANDS: &[CommandMeta] = &[
    // Agent & Interactive
    CommandMeta { name: "ask", aliases: &["chat"], group: "Agent & Interactive", usage: "fw ask [flags] <prompt>", summary: "Run multi-turn tool-calling coding agent (--model --url --resume --full-access --json)" },
    CommandMeta { name: "tool", aliases: &["x"], group: "Agent & Interactive", usage: "fw tool <name> '<json>'", summary: "Execute a workspace tool directly (read_file, write_file, edit_file, glob_files, grep_files, shell)" },
    CommandMeta { name: "tui", aliases: &["m3-tui", "god"], group: "Agent & Interactive", usage: "fw tui", summary: "Launch interactive Material 3 TUI" },
    CommandMeta { name: "serve", aliases: &["s"], group: "Agent & Interactive", usage: "fw serve [host:port]", summary: "Start Fw Chat web UI + OpenAI/MCP/ACP/A2A/WebGPU server" },
    // ML, Inference, Training & Heretic
    CommandMeta { name: "infer", aliases: &["run"], group: "ML, Inference & Heretic", usage: "fw infer [prompt] [--gguf <path>] [--model <m>] [--url <u>]", summary: "Native GGUF v3 Q8_0 or OpenAI/Ollama/Gateway inference (zero subprocess)" },
    CommandMeta { name: "train", aliases: &["finetune", "lora"], group: "ML, Inference & Heretic", usage: "fw train [dataset.jsonl] [--rank 8] [--alpha 16] [--steps 60] [--out <file>]", summary: "Native pure-Rust LoRA fine-tuning & checkpoint writer (zero subprocess)" },
    CommandMeta { name: "hf", aliases: &["huggingface"], group: "ML, Inference & Heretic", usage: "fw hf <info|files|pull> <repo_id> [filename]", summary: "Native Hugging Face Hub API & model download client (zero subprocess)" },
    CommandMeta { name: "heretic", aliases: &["abliterate"], group: "ML, Inference & Heretic", usage: "fw heretic <presets|inspect|abliterate> [preset] [--strength 1.0]", summary: "Native Heretic preset loader & directional orthogonalization (zero subprocess)" },
    // Packages & Dependencies (bun + uv grammar)
    CommandMeta { name: "init", aliases: &[], group: "Packages & Dependencies", usage: "fw init", summary: "Initialize a project manifest" },
    CommandMeta { name: "install", aliases: &["i"], group: "Packages & Dependencies", usage: "fw install [pkg...]", summary: "Install project dependencies" },
    CommandMeta { name: "add", aliases: &["a"], group: "Packages & Dependencies", usage: "fw add <pkg>", summary: "Add a dependency to the project" },
    CommandMeta { name: "remove", aliases: &["rm", "uninstall"], group: "Packages & Dependencies", usage: "fw remove <pkg>", summary: "Remove a dependency from the project" },
    CommandMeta { name: "lock", aliases: &[], group: "Packages & Dependencies", usage: "fw lock [dir]", summary: "Generate deterministic fw-pm.lock.json (Cargo + npm + APM)" },
    CommandMeta { name: "verify", aliases: &[], group: "Packages & Dependencies", usage: "fw verify [dir]", summary: "Verify fw-pm.lock.json against workspace manifests" },
    CommandMeta { name: "info", aliases: &["resolve"], group: "Packages & Dependencies", usage: "fw info <cargo|npm> <pkg>", summary: "Fetch live crates.io sparse index or npm registry metadata (zero subprocess)" },
    CommandMeta { name: "fetch", aliases: &["download"], group: "Packages & Dependencies", usage: "fw fetch <cargo|npm> <pkg>", summary: "Download & SHA-256 verify a .crate or npm .tgz into ~/.fw/cache (zero subprocess)" },
    CommandMeta { name: "tree", aliases: &["scan"], group: "Packages & Dependencies", usage: "fw tree [dir]", summary: "Inspect workspace dependency graph (Cargo + npm + APM)" },
    CommandMeta { name: "export", aliases: &["sbom"], group: "Packages & Dependencies", usage: "fw export [dir]", summary: "Export CycloneDX 1.5 SBOM" },
    CommandMeta { name: "audit", aliases: &[], group: "Packages & Dependencies", usage: "fw audit", summary: "Audit installed dependencies" },
    CommandMeta { name: "compile", aliases: &[], group: "Packages & Dependencies", usage: "fw compile", summary: "Compile APM instructions & targets" },
    CommandMeta { name: "semver", aliases: &[], group: "Packages & Dependencies", usage: "fw semver <ver> <range>", summary: "Evaluate Bun-compatible semver range" },
    // Workspace, Config & Protocols
    CommandMeta { name: "config", aliases: &[], group: "Workspace & Protocols", usage: "fw config <list|get|set>", summary: "Read or update ~/.fx/config.json" },
    CommandMeta { name: "mcp", aliases: &[], group: "Workspace & Protocols", usage: "fw mcp <list|add|remove>", summary: "Manage MCP servers in ~/.fx/mcp.json" },
    CommandMeta { name: "acp", aliases: &[], group: "Workspace & Protocols", usage: "fw acp", summary: "Run Agent Client Protocol JSON-RPC 2.0 server over stdio" },
    CommandMeta { name: "sessions", aliases: &["session"], group: "Workspace & Protocols", usage: "fw sessions | session <show|delete> <id>", summary: "List, inspect, or delete saved agent sessions" },
    CommandMeta { name: "permissions", aliases: &[], group: "Workspace & Protocols", usage: "fw permissions [ask|auto|full-access]", summary: "View or set agent permission mode" },
    CommandMeta { name: "models", aliases: &["provider"], group: "Workspace & Protocols", usage: "fw models", summary: "List configured LLM providers & endpoints" },
    CommandMeta { name: "workspace", aliases: &[], group: "Workspace & Protocols", usage: "fw workspace", summary: "Inspect git & file index (zero subprocess)" },
    CommandMeta { name: "pr", aliases: &["issue"], group: "Workspace & Protocols", usage: "fw pr", summary: "Safe read-only git status & diff snapshot" },
    CommandMeta { name: "doctor", aliases: &[], group: "Workspace & Protocols", usage: "fw doctor", summary: "Probe toolchain, FX_HOME, git, API keys, and Ollama :11434" },
    CommandMeta { name: "completion", aliases: &[], group: "Workspace & Protocols", usage: "fw completion [bash|zsh|fish|pwsh]", summary: "Generate shell completion script" },
    CommandMeta { name: "bench", aliases: &[], group: "Workspace & Protocols", usage: "fw bench <silicon|exec|compare|http>", summary: "Run local compute, exec, or HTTP benchmarks" },
    CommandMeta { name: "help", aliases: &[], group: "Workspace & Protocols", usage: "fw help [command]", summary: "Display concise help for fw or a command" },
];

pub fn resolve_alias(cmd: &str) -> &str {
    for meta in COMMANDS {
        if meta.name == cmd || meta.aliases.contains(&cmd) {
            return meta.name;
        }
    }
    cmd
}

pub fn find_command(cmd: &str) -> Option<&'static CommandMeta> {
    let canonical = resolve_alias(cmd);
    COMMANDS.iter().find(|m| m.name == canonical)
}

/// Parses raw argv (`std::env::args().skip(1)`) into global options + canonical command + remaining args.
pub fn parse_argv(raw: &[String]) -> Result<ParsedCli, String> {
    let mut global = GlobalOpts::default();
    let mut command: Option<String> = None;
    let mut args = Vec::new();
    let mut passthrough = false;
    let mut i = 0usize;

    while i < raw.len() {
        let tok = raw[i].as_str();
        if passthrough {
            args.push(tok.to_owned());
            i += 1;
            continue;
        }
        if tok == "--" {
            passthrough = true;
            i += 1;
            continue;
        }

        // Global options recognized anywhere (uv + bun convention)
        if tok == "-h" || tok == "--help" {
            global.help = true;
            i += 1;
            continue;
        }
        if (tok == "-V" || tok == "--version") && command.is_none() {
            global.version = true;
            i += 1;
            continue;
        }
        if tok == "-c" && command.is_none() {
            global.continue_session = true;
            i += 1;
            continue;
        }
        if tok == "--offline" {
            global.offline = true;
            i += 1;
            continue;
        }
        if tok == "--json" {
            global.json = true;
            if command.is_some() {
                args.push(tok.to_owned());
            }
            i += 1;
            continue;
        }
        if tok == "-q" || tok == "--quiet" {
            global.quiet = global.quiet.saturating_add(1);
            i += 1;
            continue;
        }
        if tok == "-v" || tok == "--verbose" {
            global.verbose = global.verbose.saturating_add(1);
            i += 1;
            continue;
        }
        if let Some(val) = tok
            .strip_prefix("--directory=")
            .or_else(|| tok.strip_prefix("--cwd="))
        {
            global.directory = Some(PathBuf::from(val));
            i += 1;
            continue;
        }
        if matches!(tok, "-C" | "--directory" | "--cwd") {
            let val = raw
                .get(i + 1)
                .ok_or_else(|| format!("flag `{tok}` requires a directory path"))?;
            global.directory = Some(PathBuf::from(val));
            i += 2;
            continue;
        }
        if let Some(val) = tok
            .strip_prefix("--config=")
            .or_else(|| tok.strip_prefix("--config-file="))
        {
            global.config_file = Some(PathBuf::from(val));
            i += 1;
            continue;
        }
        if matches!(tok, "--config" | "--config-file") {
            let val = raw
                .get(i + 1)
                .ok_or_else(|| format!("flag `{tok}` requires a file path"))?;
            global.config_file = Some(PathBuf::from(val));
            i += 2;
            continue;
        }
        if let Some(val) = tok.strip_prefix("--color=") {
            global.color = parse_color(val)?;
            i += 1;
            continue;
        }
        if tok == "--color" {
            let val = raw
                .get(i + 1)
                .ok_or_else(|| "flag `--color` requires auto|always|never".to_owned())?;
            global.color = parse_color(val)?;
            i += 2;
            continue;
        }

        if command.is_none() && !tok.starts_with('-') {
            command = Some(resolve_alias(tok).to_owned());
        } else {
            args.push(tok.to_owned());
        }
        i += 1;
    }

    Ok(ParsedCli {
        global,
        command,
        args,
    })
}

fn parse_color(val: &str) -> Result<ColorChoice, String> {
    match val {
        "auto" => Ok(ColorChoice::Auto),
        "always" => Ok(ColorChoice::Always),
        "never" => Ok(ColorChoice::Never),
        other => Err(format!("invalid --color `{other}` (expected auto|always|never)")),
    }
}

pub fn render_root_help() -> String {
    let mut out = format!(
        "fw {FW_VERSION} — Fast native Rust coding agent, package manager & server.\n\nUsage: fw [OPTIONS] [COMMAND] [ARGS...]\n\n"
    );
    let groups = [
        "Agent & Interactive",
        "ML, Inference & Heretic",
        "Packages & Dependencies",
        "Workspace & Protocols",
    ];
    for group in groups {
        out.push_str(group);
        out.push_str(":\n");
        for m in COMMANDS.iter().filter(|c| c.group == group) {
            let alias_hint = if m.aliases.is_empty() {
                String::new()
            } else {
                format!(" ({})", m.aliases.join(", "))
            };
            let label = format!("{}{}", m.name, alias_hint);
            out.push_str(&format!("  {label:<24} {}\n", m.summary));
        }
        out.push('\n');
    }
    out.push_str(
        "Global options:\n\
         \x20 -C, --directory <DIR>    Change working directory before running [env: FW_WORKING_DIR]\n\
         \x20     --config <FILE>      Path to config file [env: FW_CONFIG]\n\
         \x20     --offline            Disable network access [env: FW_OFFLINE]\n\
         \x20     --json               Emit machine-readable JSON output\n\
         \x20     --color <WHEN>       Color output: auto, always, never\n\
         \x20 -q, --quiet              Reduce output verbosity\n\
         \x20 -v, --verbose            Increase output verbosity\n\
         \x20 -c                       Resume latest interactive session (when no command given)\n\
         \x20 -h, --help               Print help (or `fw <command> --help`)\n\
         \x20 -V, --version            Print version and exit\n",
    );
    out
}

pub fn render_command_help(cmd: &str) -> Option<String> {
    let meta = find_command(cmd)?;
    let aliases = if meta.aliases.is_empty() {
        "none".to_owned()
    } else {
        meta.aliases.join(", ")
    };
    Some(format!(
        "{}\n\nUsage: {}\nAliases: {}\nGroup: {}\n\nGlobal flags: -C/--directory <DIR>, --offline, --json, -q/--quiet, -v/--verbose, -h/--help\n",
        meta.summary, meta.usage, aliases, meta.group
    ))
}

/// Suggest closest command on typo (Levenshtein distance <= 2).
pub fn suggest_command(unknown: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for meta in COMMANDS {
        let d = levenshtein(unknown, meta.name);
        if d <= 2 && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, meta.name));
        }
    }
    best.map(|(_, n)| n)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        core::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bun_and_uv_style_global_flags_and_aliases() {
        let argv: Vec<String> = ["-C", "crates/fw-fx", "i", "--offline", "--json"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        let parsed = parse_argv(&argv).expect("parse ok");
        assert_eq!(parsed.command.as_deref(), Some("install"));
        assert_eq!(parsed.global.directory, Some(PathBuf::from("crates/fw-fx")));
        assert!(parsed.global.offline);
        assert!(parsed.global.json);

        let eq: Vec<String> = ["x", "--cwd=/tmp", "grep_files"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        let p2 = parse_argv(&eq).expect("parse ok");
        assert_eq!(p2.command.as_deref(), Some("tool"));
        assert_eq!(p2.global.directory, Some(PathBuf::from("/tmp")));
        assert_eq!(p2.args, vec!["grep_files"]);
    }

    #[test]
    fn suggests_close_command_on_typo() {
        assert_eq!(suggest_command("doctr"), Some("doctor"));
        assert_eq!(suggest_command("serv"), Some("serve"));
        assert_eq!(suggest_command("instal"), Some("install"));
    }
}
