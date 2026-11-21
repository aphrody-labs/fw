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
    CommandMeta { name: "ask", aliases: &["chat"], group: "Agent & Interactive", usage: "fw ask [flags] <prompt>", summary: "Run the multi-turn coding agent with tool calls (--model --url --resume --full-access --json)" },
    CommandMeta { name: "tool", aliases: &["x"], group: "Agent & Interactive", usage: "fw tool <name> '<json>'", summary: "Execute a workspace tool directly (read_file, write_file, edit_file, glob_files, grep_files, shell)" },
    CommandMeta { name: "tui", aliases: &["m3-tui", "god"], group: "Agent & Interactive", usage: "fw tui", summary: "Launch interactive Material 3 TUI" },
    CommandMeta { name: "convert", aliases: &["cv"], group: "Files & Media", usage: "fw convert <input> <out-dir> [--to svg,md,html,jsx,gpui] [--fps N] [--width N]", summary: "Convert a file into other files: video to SVG frames, Markdown, HTML, JSX and GPUI source (runs fw-video)" },
    CommandMeta { name: "google", aliases: &[], group: "Files & Media", usage: "fw google <drive|sheets|gmail|calendar|docs|youtube> <op> [args] [--access-token T] [--auth auto|gcloud|adc]", summary: "Google Workspace and YouTube: Drive list/export, Sheets get/update, Gmail list/draft (never sends), Calendar events, Docs text, YouTube channels/videos/playlists and video update behind --apply (runs fw-google)" },
    CommandMeta { name: "files", aliases: &["fl"], group: "Files & Media", usage: "fw files <ls|mime|preview|cp|mv|rm|rules> [args]", summary: "File manager engine from yazi: sorted listings, MIME and previewer rules, previews, copy/move/remove with progress events, sftp:// paths; JSON lines (runs fw-files)" },
    CommandMeta { name: "serve", aliases: &["s"], group: "Agent & Interactive", usage: "fw serve [host:port]", summary: "Serve the chat web UI plus /v1/chat/completions, /mcp, /acp, /a2a and /webgpu" },
    // ML, Inference, Training & Heretic
    CommandMeta { name: "infer", aliases: &["run"], group: "ML, Inference & Heretic", usage: "fw infer [prompt] [--gguf <path>] [--model <m>] [--url <u>]", summary: "Send a prompt to an OpenAI-compatible endpoint; with --gguf, read the GGUF header and run a Q8_0 dot-product sample (no token generation)" },
    CommandMeta { name: "train", aliases: &["finetune", "lora"], group: "ML, Inference & Heretic", usage: "fw train [dataset.jsonl] [--rank 8] [--alpha 16] [--steps 60] [--out <file>]", summary: "LoRA SGD demo on a synthetic 32-dim layer; a dataset.jsonl is validated and counted, not trained on; writes an fw-lora-v1 JSON" },
    CommandMeta { name: "hf", aliases: &["huggingface"], group: "ML, Inference & Heretic", usage: "fw hf <search|info|files|pull|datasets|spaces|papers|collections|whoami|sitemap|safetensors|tokenizer|cache>", summary: "Hugging Face Hub client: search, info, files, pull, safetensors header and tokenizer.json inspection" },
    CommandMeta { name: "abliterate", aliases: &["abl"], group: "ML, Inference & Heretic", usage: "fw abliterate <plan|biprojected|ara|sweep|moe|strip-cot|presets> [flags]", summary: "Abliteration: `plan` sizes a 7-step pipeline; biprojected, ara, sweep and moe run reference math on synthetic vectors, not model weights" },
    CommandMeta { name: "vram", aliases: &[], group: "ML, Inference & Heretic", usage: "fw vram [--vram-gb 12] [--model-gb 5.5] [--layers 32] [--ctx 16384] [--swa-layers 0] [--kv-type q8_0]", summary: "Estimate llama.cpp VRAM, SWA KV cache and the -ngl layer split" },
    CommandMeta { name: "repomap", aliases: &[], group: "Workspace & Protocols", usage: "fw repomap [dir]", summary: "Repository symbol map (Rust, TS/JS, Python, Go, Kotlin)" },
    CommandMeta { name: "watch", aliases: &[], group: "Workspace & Protocols", usage: "fw watch <list|add|remove> ...", summary: "Manage persistent scheduled agent watchers (~/.fx/watchers.json)" },
    CommandMeta { name: "tune", aliases: &["cachy"], group: "Workspace & Protocols", usage: "fw tune <hardware|sysctl|scx|ananicy|bench>", summary: "CachyOS hardware, sysctl, scx_lavd and ananicy-cpp profiles; x86-64-v3 and mimalloc microbenchmarks" },
    CommandMeta { name: "sec", aliases: &["deepsec"], group: "Workspace & Protocols", usage: "fw sec [dir]", summary: "Pattern-based scan for secrets and risky code" },
    CommandMeta { name: "cluster", aliases: &[], group: "Workspace & Protocols", usage: "fw cluster [summary|route <prompt>|k8s <status|nodes|pods|apply -f FILE|topology>]", summary: "Inference router (KV-cache affinity, prefill/decode split) over the sample topology, or a Kubernetes cluster through kube-rs (runs fw-k8s)" },
    CommandMeta { name: "depot", aliases: &["gclient"], group: "Workspace & Protocols", usage: "fw depot <sync|revinfo|getdep|setdep|runhooks|fetch|cipd|autoninja|cl|presubmit|repo|lunch|deps|model> [args]", summary: "depot_tools in Rust for Chromium and AOSP: gclient sync/revinfo/getdep/setdep, CIPD, fetch, autoninja, git cl and Gerrit, presubmit, repo manifests, lunch (runs fw-depot)" },
    CommandMeta { name: "kernel", aliases: &["linux"], group: "Workspace & Protocols", usage: "fw kernel <status|pin|sync|types|graph> [args]", summary: "The Linux kernel as pinned files: kernel.toml pins (mainline, stable, CachyOS) against kernel.org releases, shallow sparse submodule sync, UAPI types for bun:ffi, tree-sitter code graph (runs fw-linux)" },
    CommandMeta { name: "graph", aliases: &["graphify"], group: "Workspace & Protocols", usage: "fw graph <build <dir> [--json f] [--report f]|query <question>|explain <node>|path <a> <b>|report|export>", summary: "Code graph of a tree (tree-sitter Rust, oxc TS/JS, Markdown): calls, imports, communities, god nodes; runs fw-graph" },
    CommandMeta { name: "mem", aliases: &["memory", "claude-mem"], group: "Workspace & Protocols", usage: "fw mem <add|search|timeline|get|list|stats|prune>", summary: "Memory store: add, then search (BM25), timeline and get; monthly JSONL shards" },
    CommandMeta { name: "docs", aliases: &["c7", "context7"], group: "Workspace & Protocols", usage: "fw docs <resolve|query|ingest|list> [lib] [topic] [--tokens 4000]", summary: "Resolve and query library docs from crates.io, npm, PyPI, GitHub and llms.txt, cached locally" },
    CommandMeta { name: "corpus", aliases: &[], group: "Workspace & Protocols", usage: "fw corpus <sources|status|sync|search|read|systemd> [--root <dir>]", summary: "Polite incremental corpus of Google design, Android, Chromium, GitHub, Kotlin, Rust and Cargo docs in per-source SQLite FTS5 stores; `fw learn search` includes it" },
    CommandMeta { name: "learn", aliases: &["autolearn"], group: "Workspace & Protocols", usage: "fw learn <status|harvest|distill|search|loop|systemd> [--xfs-root <dir>]", summary: "Harvest git history, the code graph and docs into a local corpus; search all three; `systemd` prints a unit for a VPS" },
    CommandMeta { name: "skills", aliases: &[], group: "Workspace & Protocols", usage: "fw skills <list|read|install>", summary: "List, read and install SKILL.md skills" },
    CommandMeta { name: "onboard", aliases: &["init-agent"], group: "Workspace & Protocols", usage: "fw onboard [dir] | fw onboard export <all|mcp|opencode|zed> [--dir <dir>]", summary: "Describe the workspace for agents and export MCP, OpenCode and Zed configs" },
    CommandMeta { name: "workflow", aliases: &["wf"], group: "Workspace & Protocols", usage: "fw workflow <list|run> [discover|abliterate-e2e|pkg-audit|perf-check] [dir]", summary: "Run a built-in workflow (discover, abliterate-e2e, pkg-audit, perf-check)" },
    CommandMeta { name: "self-install", aliases: &[], group: "Workspace & Protocols", usage: "fw self-install [--dir <bin_dir>]", summary: "Install fw binary into ~/.local/bin with SHA-256 verification" },
    CommandMeta { name: "self-update", aliases: &["update", "upgrade"], group: "Workspace & Protocols", usage: "fw self-update [--check] [--version <tag>]", summary: "Update from GitHub Releases, verify SHA-256, keep the previous binary as .prev" },
    CommandMeta { name: "self-uninstall", aliases: &[], group: "Workspace & Protocols", usage: "fw self-uninstall [--dir <bin_dir>] [--purge-data]", summary: "Remove the fw binary (--purge-data also removes ~/.fx)" },
    CommandMeta { name: "sh", aliases: &[], group: "GNU/Linux & Wasm Userland", usage: "fw sh -c \"cmd1 | cmd2 | cmd3\"", summary: "In-process pipeline shell over 40 built-in applets, no child processes" },
    CommandMeta { name: "jq", aliases: &[], group: "GNU/Linux & Wasm Userland", usage: "fw jq '<expr>' [file.json]", summary: "JSON query filter (.a.b[0], keys, length)" },
    CommandMeta { name: "tokei", aliases: &[], group: "GNU/Linux & Wasm Userland", usage: "fw tokei [dir]", summary: "Lines-of-code counter by language" },
    CommandMeta { name: "du", aliases: &[], group: "GNU/Linux & Wasm Userland", usage: "fw du [dir]", summary: "Disk usage by directory" },
    CommandMeta { name: "bat", aliases: &[], group: "GNU/Linux & Wasm Userland", usage: "fw bat <file>", summary: "Line-numbered file viewer" },
    CommandMeta { name: "heretic", aliases: &[], group: "ML, Inference & Heretic", usage: "fw heretic <presets|inspect|abliterate> [preset] [--strength 1.0]", summary: "Heretic TOML preset inspector; `abliterate` runs directional orthogonalization on synthetic vectors" },
    CommandMeta { name: "align", aliases: &["dpo", "orpo"], group: "ML, Inference & Heretic", usage: "fw align <dpo|orpo|kto> [--beta 0.1] [--steps 60]", summary: "DPO, ORPO and KTO loss demo on one synthetic preference pair; no model is aligned" },
    // Packages & Dependencies (bun + uv grammar)
    CommandMeta { name: "init", aliases: &[], group: "Packages & Dependencies", usage: "fw init", summary: "Initialize a project manifest" },
    CommandMeta { name: "install", aliases: &["i"], group: "Packages & Dependencies", usage: "fw install [pkg...]", summary: "Install project dependencies" },
    CommandMeta { name: "add", aliases: &["a"], group: "Packages & Dependencies", usage: "fw add <pkg>", summary: "Add a dependency to the project" },
    CommandMeta { name: "remove", aliases: &["rm", "uninstall"], group: "Packages & Dependencies", usage: "fw remove <pkg>", summary: "Remove a dependency from the project" },
    CommandMeta { name: "lock", aliases: &[], group: "Packages & Dependencies", usage: "fw lock [dir]", summary: "Generate deterministic fw-pm.lock.json (Cargo + npm + APM)" },
    CommandMeta { name: "verify", aliases: &[], group: "Packages & Dependencies", usage: "fw verify [dir]", summary: "Verify fw-pm.lock.json against workspace manifests" },
    CommandMeta { name: "crates", aliases: &["crate"], group: "Packages & Dependencies", usage: "fw crates <search|info|compare|rank|awesome|bench|stack|sync> [args] [--json|--mdx]", summary: "Crate intelligence over crates.io: ranked search, per-crate score, side-by-side comparison, per-category awesome lists, load-tested framework benchmarks, a stack and project-skeleton generator, and the daily dump sync (runs fw-crates)" },
    CommandMeta { name: "info", aliases: &["resolve"], group: "Packages & Dependencies", usage: "fw info <cargo|npm> <pkg>", summary: "Fetch crates.io sparse index or npm registry metadata" },
    CommandMeta { name: "fetch", aliases: &["download"], group: "Packages & Dependencies", usage: "fw fetch <cargo|npm> <pkg>", summary: "Download a .crate or npm .tgz into ~/.fw/cache and verify its SHA-256" },
    CommandMeta { name: "tree", aliases: &["scan"], group: "Packages & Dependencies", usage: "fw tree [dir]", summary: "Inspect workspace dependency graph (Cargo + npm + APM)" },
    CommandMeta { name: "export", aliases: &["sbom"], group: "Packages & Dependencies", usage: "fw export [dir]", summary: "Export CycloneDX 1.5 SBOM" },
    CommandMeta { name: "audit", aliases: &[], group: "Packages & Dependencies", usage: "fw audit", summary: "Audit installed dependencies" },
    CommandMeta { name: "compile", aliases: &[], group: "Packages & Dependencies", usage: "fw compile", summary: "Compile APM instructions & targets" },
    CommandMeta { name: "semver", aliases: &[], group: "Packages & Dependencies", usage: "fw semver <ver> <range>", summary: "Evaluate Bun-compatible semver range" },
    // Workspace, Config & Protocols
    CommandMeta { name: "config", aliases: &[], group: "Workspace & Protocols", usage: "fw config <list|get|set>", summary: "Read or update ~/.fx/config.json" },
    CommandMeta { name: "mcp", aliases: &[], group: "Workspace & Protocols", usage: "fw mcp | fw mcp <list|add|remove>", summary: "Serve fw's tools as an MCP server over stdio, or manage client MCP servers in ~/.fx/mcp.json" },
    CommandMeta { name: "acp", aliases: &[], group: "Workspace & Protocols", usage: "fw acp", summary: "Run Agent Client Protocol JSON-RPC 2.0 server over stdio" },
    CommandMeta { name: "sessions", aliases: &["session"], group: "Workspace & Protocols", usage: "fw sessions | session <show|delete> <id>", summary: "List, inspect, or delete saved agent sessions" },
    CommandMeta { name: "permissions", aliases: &[], group: "Workspace & Protocols", usage: "fw permissions [ask|auto|full-access]", summary: "View or set agent permission mode" },
    CommandMeta { name: "models", aliases: &["provider"], group: "Workspace & Protocols", usage: "fw models", summary: "List configured LLM providers & endpoints" },
    CommandMeta { name: "workspace", aliases: &[], group: "Workspace & Protocols", usage: "fw workspace", summary: "Show git state and the file index" },
    CommandMeta { name: "pr", aliases: &["issue"], group: "Workspace & Protocols", usage: "fw pr", summary: "Read-only git status and diff snapshot" },
    CommandMeta { name: "doctor", aliases: &[], group: "Workspace & Protocols", usage: "fw doctor", summary: "Check the toolchain, FX_HOME, git, API keys and Ollama on :11434" },
    CommandMeta { name: "completion", aliases: &[], group: "Workspace & Protocols", usage: "fw completion [bash|zsh|fish|pwsh]", summary: "Generate shell completion script" },
    CommandMeta { name: "bench", aliases: &[], group: "Workspace & Protocols", usage: "fw bench <silicon|exec|compare|http|perf>", summary: "Run local compute, exec or HTTP benchmarks; `perf run` is the monorepo build, size, startup, link, wasm, Workers and GPU harness" },
    CommandMeta { name: "help", aliases: &[], group: "Workspace & Protocols", usage: "fw help [command]", summary: "Show help for fw or one command" },
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
        "fw {FW_VERSION} — Rust coding agent, package manager and server.\n\nUsage: fw [OPTIONS] [COMMAND] [ARGS...]\n\n"
    );
    // Groups in the order they first appear in COMMANDS, so a new group can never be hidden.
    let mut groups: Vec<&str> = Vec::new();
    for c in COMMANDS {
        if !groups.contains(&c.group) {
            groups.push(c.group);
        }
    }
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

/// Subcommands dispatched in `main.rs` that are not part of the grouped `fw --help` list: (name, usage, summary).
const DIRECT_COMMANDS: &[(&str, &str, &str)] = &[
    ("status", "fw status", "Print the product, hardware profile and cluster summary as JSON"),
    ("hardware", "fw hardware", "Print the detected CPU, GPU and OS hardware profile as JSON"),
    ("compute", "fw compute", "Run the compute smoke benchmark and print the result as JSON"),
    ("ggml", "fw ggml", "Encode and parse a GGUF v3 header and query a loopback ggml-rpc device"),
    ("openapi", "fw openapi", "Print the OpenAPI 3.1 document of `fw serve`"),
    ("ui", "fw ui [--out <file.png>]", "Render the cluster HUD to a PNG (default target/blitz-proof.png)"),
    ("ostd", "fw ostd", "Boot the OSTD GPUI kernel snapshot and print it as JSON"),
    ("uutils", "fw uutils <applet> [args...]", "Run one built-in uutils applet and print the result as JSON (default applet: uname)"),
    ("nix", "fw nix [name]", "Compute a fixed-output Nix store entry for a name"),
    ("stack", "fw stack", "Print the unified v4 stack catalog, burn rig, voice status and Kitty protocol samples"),
    ("wayland", "fw wayland", "Create and commit a Wayland-WebGPU surface and print the result as JSON"),
    ("claw", "fw claw [status|ranking|mcp <jsonrpc>|skill-gen <tool>|run <goal>]", "Hermes/OpenClaw runtime status, rankings, MCP, skill generation and goal loop (alias: hermes)"),
    ("migrate", "fw migrate [target]", "Scan the working directory for packages to migrate and print the counts as JSON"),
    ("ls", "fw ls [dir] [--tsv]", "Fast directory listing"),
    ("table", "fw table [dir] [--json]", "Fast directory table"),
    ("find", "fw find <pattern> [dir]", "Fast file-name search"),
    ("grep", "fw grep <pattern> [dir]", "Fast content search"),
];

fn render_direct_help(cmd: &str) -> Option<String> {
    let cmd = if cmd == "hermes" { "claw" } else { cmd };
    let (name, usage, summary) = DIRECT_COMMANDS.iter().find(|(n, _, _)| *n == cmd)?;
    Some(format!(
        "{summary}\n\nUsage: {usage}\nCommand: {name}\n\nGlobal flags: -C/--directory <DIR>, --offline, --json, -q/--quiet, -v/--verbose, -h/--help\n"
    ))
}

pub fn render_command_help(cmd: &str) -> Option<String> {
    let Some(meta) = find_command(cmd) else {
        return render_direct_help(cmd);
    };
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

    #[test]
    fn every_dispatched_command_has_help() {
        for (name, _, _) in DIRECT_COMMANDS {
            let help = render_command_help(name).unwrap_or_default();
            assert!(help.contains("Usage: fw"), "`fw help {name}` has no usage");
        }
        assert!(render_command_help("hermes").is_some());
        assert!(render_command_help("no-such-command").is_none());
    }

    #[test]
    fn every_command_appears_in_root_help() {
        let help = render_root_help();
        for c in COMMANDS {
            assert!(help.contains(c.name), "`{}` missing from fw --help", c.name);
        }
    }
}
