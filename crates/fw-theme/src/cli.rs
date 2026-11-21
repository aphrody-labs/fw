//! `fw theme ...`

use crate::apply::Context;
use crate::error::{Error, Result};
use crate::schema;
use crate::settings::{ModePreference, Settings};
use crate::targets::{self, Target};
use crate::watch;
use std::path::PathBuf;
use std::time::Duration;

pub const USAGE: &str = "fw theme <list|current|switch|apply|render|schema|watch|path>\n\
  list                                  every theme (shipped, user, bundled); * marks the active one\n\
  current                               the active theme and mode\n\
  switch <name> [--mode light|dark|system]  select a theme, render all targets, run hooks\n\
  apply                                 render the theme the settings select\n\
  render [name] [--target T] [--mode M] [--out DIR]  print one target, or write all of them to DIR\n\
  schema [settings|theme] [--write DIR|--check DIR]  the JSON Schema; DIR defaults to crates/fw-theme/schema\n\
  watch                                 re-render when settings, themes or templates change\n\
  path                                  settings file, output directory and theme search path\n\
targets: css, gpui, rio, hyprland, web";

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1)).cloned()
}

fn positional(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if arg.starts_with("--") {
            skip = true;
        } else {
            out.push(arg);
        }
    }
    out
}

fn parse_preference(text: &str) -> Result<ModePreference> {
    match text {
        "system" => Ok(ModePreference::System),
        "light" => Ok(ModePreference::Light),
        "dark" => Ok(ModePreference::Dark),
        other => Err(Error::Invalid(format!("mode `{other}` is not light, dark or system"))),
    }
}

fn schema_dir(args: &[String]) -> Result<PathBuf> {
    if let Some(dir) = flag(args, "--write").or_else(|| flag(args, "--check")) {
        return Ok(PathBuf::from(dir));
    }
    let cwd = std::env::current_dir().map_err(|source| Error::io(".", source))?;
    cwd.ancestors().map(|dir| dir.join("crates/fw-theme/schema")).find(|dir| dir.is_dir()).ok_or_else(|| Error::Invalid("crates/fw-theme/schema not found; pass a directory".to_owned()))
}

fn report(applied: &crate::apply::Applied) {
    println!("{} ({}): {} files", applied.theme, applied.mode.as_str(), applied.files.len());
    for failure in &applied.hook_failures {
        eprintln!("hook failed: {failure}");
    }
}

fn dispatch(args: &[String]) -> Result<i32> {
    let Some(command) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(0);
    };
    let rest = &args[1..];
    match command {
        "schema" => {
            let fragments = Vec::new();
            if rest.iter().any(|arg| arg == "--check") {
                let dir = schema_dir(rest)?;
                let stale = schema::stale(&dir, &fragments)?;
                for name in &stale {
                    eprintln!("{}: out of date; run `fw theme schema --write`", dir.join(name).display());
                }
                return Ok(i32::from(!stale.is_empty()));
            }
            if rest.iter().any(|arg| arg == "--write") {
                for path in schema::write(&schema_dir(rest)?, &fragments)? {
                    println!("{}", path.display());
                }
                return Ok(0);
            }
            let value = match positional(rest).first().map(|arg| arg.as_str()) {
                None | Some("settings") => schema::settings_schema(&fragments)?,
                Some("theme") => schema::theme_schema()?,
                Some(other) => return Err(Error::Invalid(format!("schema `{other}` is not settings or theme"))),
            };
            println!("{}", serde_json::to_string_pretty(&value).map_err(|source| Error::Json { path: PathBuf::from("<schema>"), source })?);
            Ok(0)
        }
        "list" => {
            let context = Context::discover()?;
            let settings = Settings::load(&context.settings_path)?;
            let active = context.resolve(&settings).map(|theme| theme.name).ok();
            for name in context.store.names() {
                let theme = context.store.load(&name)?;
                let mark = if active.as_deref() == Some(name.as_str()) { '*' } else { ' ' };
                println!("{mark} {name}\t{:?}\t{}", theme.file.mode, theme.file.title.as_deref().unwrap_or(&name));
            }
            Ok(0)
        }
        "current" => {
            let context = Context::discover()?;
            let resolved = context.resolve(&Settings::load(&context.settings_path)?)?;
            println!("{} {}", resolved.name, resolved.active.as_str());
            Ok(0)
        }
        "switch" => {
            let name = positional(rest).first().map(|arg| arg.to_string()).ok_or_else(|| Error::Invalid("usage: fw theme switch <name> [--mode light|dark|system]".to_owned()))?;
            let mode = flag(rest, "--mode").map(|text| parse_preference(&text)).transpose()?;
            report(&Context::discover()?.switch(&name, mode)?);
            Ok(0)
        }
        "apply" => {
            let context = Context::discover()?;
            report(&context.apply(&Settings::load(&context.settings_path)?)?);
            Ok(0)
        }
        "render" => {
            let context = Context::discover()?;
            let mut settings = Settings::load(&context.settings_path)?;
            if let Some(name) = positional(rest).first() {
                settings.theme.name = Some((*name).clone());
            }
            if let Some(mode) = flag(rest, "--mode") {
                settings.theme.mode = parse_preference(&mode)?;
            }
            let resolved = context.resolve(&settings)?;
            match (flag(rest, "--out"), flag(rest, "--target")) {
                (Some(out), target) => {
                    let selected = match target {
                        Some(slug) => vec![Target::from_slug(&slug).ok_or_else(|| Error::Invalid(format!("target `{slug}` is not one of css, gpui, rio, hyprland, web")))?],
                        None => Target::ALL.to_vec(),
                    };
                    for path in targets::render_all(&resolved, &selected, &PathBuf::from(out))? {
                        println!("{}", path.display());
                    }
                }
                (None, Some(slug)) => {
                    let target = Target::from_slug(&slug).ok_or_else(|| Error::Invalid(format!("target `{slug}` is not one of css, gpui, rio, hyprland, web")))?;
                    print!("{}", targets::render(&resolved, target)?);
                }
                (None, None) => return Err(Error::Invalid("render needs --target T or --out DIR".to_owned())),
            }
            Ok(0)
        }
        "watch" => {
            let context = Context::discover()?;
            let mut paths = vec![context.settings_path.clone()];
            paths.extend(context.store.roots().iter().cloned());
            let apply = || {
                let applied = context.apply(&Settings::load(&context.settings_path)?)?;
                report(&applied);
                Ok(())
            };
            apply()?;
            watch::run(&paths, Duration::from_millis(250), apply, |error| eprintln!("fw theme: {error}"))?;
            Ok(0)
        }
        "path" => {
            let context = Context::discover()?;
            println!("settings {}", context.settings_path.display());
            println!("output {}", context.out_dir.display());
            for root in context.store.roots() {
                println!("themes {}", root.display());
            }
            Ok(0)
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => Err(Error::Invalid(format!("unknown subcommand `{other}`\n{USAGE}"))),
    }
}

/// Runs `fw theme <args>` and returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    match dispatch(args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("fw theme: {error}");
            2
        }
    }
}

