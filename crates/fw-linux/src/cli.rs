//! The `fw-linux` command line, run by `fw kernel`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::pins::{self, Pins};
use crate::repo::Remotes;
use crate::{graph, pin, status, sync};

const USAGE: &str = "usage: fw kernel <command> [args]
  status [--check] [--strict] [--offline] [--json]   pins against kernel.org (mainline, stable, longterm, linux-next) and CachyOS; --check exits 1 on inconsistent pins, --strict also on pins that are behind
  pin <target> [--tree ID] [--no-index] [--json]   move pins: mainline | stable | longterm[:series] | cachyos | cachyos-rc | a version (7.3.5, 7.4-rc3) | a tag (v6.18.64, cachyos-7.3.5-2); updates kernel.toml and the gitlink in the index
  sync [TREE ...] [--profile NAME] [--json]   fetch the pinned commit of each tree (all when none is named), shallow, with the profile of its tree (uapi: sparse, blobless; full: whole tree)
  graph build|stats|def|refs|callers|callees|includes|included-by|file|config|search ... [--tree ID] [--db FILE] [--limit N]   the tree-sitter code graph of a pinned tree
  types [--check] [--tree-dir DIR] [--out DIR]   generate (or with --check, verify) packages/linux-types from the sanitized UAPI headers of the pinned stable tree (syncs it with the uapi profile when needed; runs bun, clang, pahole, bpftool)
  help
global: --root DIR (the fw checkout holding kernel.toml; default: nearest ancestor of the working directory or $FW_ROOT); $FW_KERNEL_GIT_BASE replaces https://github.com for the forks";

/// Flags with a value and bare flags, in order.
struct Arguments {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Arguments {
    fn parse(arguments: &[String], bare: &[&str]) -> Arguments {
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut iterator = arguments.iter();
        while let Some(argument) = iterator.next() {
            let Some(flag) = argument.strip_prefix("--") else {
                positional.push(argument.clone());
                continue;
            };
            if let Some((name, value)) = flag.split_once('=') {
                flags.push((name.to_owned(), Some(value.to_owned())));
            } else if bare.contains(&flag) {
                flags.push((flag.to_owned(), None));
            } else {
                flags.push((flag.to_owned(), iterator.next().cloned()));
            }
        }
        Arguments { positional, flags }
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(flag, _)| flag == name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(flag, _)| flag == name).and_then(|(_, value)| value.as_deref())
    }
}

const BARE: &[&str] = &["json", "check", "strict", "offline", "no-index"];

pub fn run(arguments: &[String]) -> Result<u8> {
    run_with(arguments, &Remotes::from_env())
}

/// [`run`] with the forks taken from `remotes` instead of the environment.
pub fn run_with(arguments: &[String], remotes: &Remotes) -> Result<u8> {
    let Some((command, rest)) = arguments.split_first() else {
        println!("{USAGE}");
        return Ok(0);
    };
    if matches!(command.as_str(), "help" | "--help" | "-h") {
        println!("{USAGE}");
        return Ok(0);
    }
    // The graph command forwards its own flags untouched; only --root, --tree and --json are read here.
    let parsed = Arguments::parse(rest, BARE);
    let working_directory = std::env::current_dir().map_err(|source| Error::Io { path: PathBuf::from("."), source })?;
    let root = pins::find_root(parsed.value("root").map(Path::new), &working_directory)?;
    let pins_path = root.join(pins::FILE_NAME);
    let mut pins = Pins::load(&pins_path)?;
    let json = parsed.has("json");

    match command.as_str() {
        "status" => {
            let report = status::compute(&root, &pins, remotes, &status::Options { offline: parsed.has("offline") })?;
            emit(json, &report, &status::render(&report))?;
            let failed = parsed.has("check") && (!report.problems.is_empty() || (parsed.has("strict") && !report.behind.is_empty()));
            Ok(u8::from(failed))
        }
        "pin" => {
            let target = parsed.positional.first().ok_or_else(|| Error::message("pin needs a target (see `fw kernel help`)"))?;
            let options = pin::Options { tree: parsed.value("tree").map(str::to_owned), no_index: parsed.has("no-index") };
            let outcomes = pin::apply(&root, &mut pins, remotes, target, &options)?;
            pins.save(&pins_path)?;
            let text: String = outcomes
                .iter()
                .map(|outcome| format!("{} {} -> {} ({}){}\n", outcome.tree, outcome.previous.as_deref().unwrap_or("-"), outcome.tag.as_deref().unwrap_or("HEAD"), &outcome.commit[..outcome.commit.len().min(12)], if outcome.fork_tag_created { " tag created in the fork" } else { "" }))
                .collect();
            emit(json, &outcomes, &text)?;
            Ok(0)
        }
        "sync" => {
            let options = sync::Options { trees: parsed.positional.clone(), profile: parsed.value("profile").map(str::to_owned) };
            let outcomes = sync::run(&root, &pins, remotes, &options)?;
            let text: String = outcomes.iter().map(|outcome| format!("{:<18} {:<10} {:<8} {} {:.1}s {} MB\n", outcome.tree, outcome.action, outcome.profile, &outcome.commit[..outcome.commit.len().min(12)], outcome.seconds, outcome.bytes / 1_000_000)).collect();
            emit(json, &outcomes, &text)?;
            Ok(0)
        }
        "graph" => {
            let tree_id = parsed.value("tree").unwrap_or("stable");
            let tree = pins.tree(tree_id)?;
            let label = tree.tag.clone().unwrap_or_else(|| tree.commit[..tree.commit.len().min(12)].to_owned());
            let database = std::env::var_os("FW_KERNEL_GRAPH_DB").map(PathBuf::from).unwrap_or_else(|| root.join(".cache").join("kernel-graph").join(format!("{tree_id}-{label}.db")));
            let forwarded = strip_flag(rest, "tree");
            let value = graph::cli(&forwarded, &root.join(&tree.path), &database)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(0)
        }
        "types" => types(&root, &parsed),
        other => Err(Error::message(format!("unknown command `{other}`\n{USAGE}"))),
    }
}

/// Headers of the pinned tree through `headers.ts`, then the generator; both are Bun scripts of packages/linux-types.
fn types(root: &Path, parsed: &Arguments) -> Result<u8> {
    let package = root.join("packages").join("linux-types");
    let headers = match parsed.value("out") {
        Some(directory) => PathBuf::from(directory),
        None => root.join(".cache").join("kernel-types").join("headers"),
    };
    let me = std::env::current_exe().map_err(|source| Error::Io { path: PathBuf::from("fw-linux"), source })?;
    let bun = |arguments: Vec<String>| -> Result<u8> {
        let status = std::process::Command::new("bun")
            .args(&arguments)
            .current_dir(&package)
            .env("FW_LINUX_BIN", &me)
            .status()
            .map_err(|error| Error::Command { command: format!("bun {}", arguments.join(" ")), detail: error.to_string() })?;
        Ok(u8::try_from(status.code().unwrap_or(1)).unwrap_or(1))
    };
    let mut prepare = vec!["run".to_owned(), "headers".to_owned(), "--out".to_owned(), headers.display().to_string()];
    if let Some(directory) = parsed.value("tree-dir") {
        prepare.extend(["--tree-dir".to_owned(), directory.to_owned()]);
    }
    let code = bun(prepare)?;
    if code != 0 {
        return Ok(code);
    }
    let mut generate = vec!["run".to_owned(), "gen".to_owned()];
    if parsed.has("check") {
        generate.push("--check".to_owned());
    }
    generate.extend(["--headers".to_owned(), headers.display().to_string()]);
    bun(generate)
}

/// The arguments without `--name VALUE` / `--name=VALUE`.
fn strip_flag(arguments: &[String], name: &str) -> Vec<String> {
    let bare = format!("--{name}");
    let prefixed = format!("--{name}=");
    let mut kept = Vec::new();
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        if *argument == bare {
            iterator.next();
        } else if !argument.starts_with(&prefixed) {
            kept.push(argument.clone());
        }
    }
    kept
}

fn emit<T: Serialize>(json: bool, value: &T, text: &str) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        print!("{text}");
    }
    Ok(())
}
