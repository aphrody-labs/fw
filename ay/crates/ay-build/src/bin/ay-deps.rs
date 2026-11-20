//! `ay-deps` (crate `ay-build`, feature `deps`): print the dependencies and hooks of a DEPS file for a target as JSON, for parity tests and scripts.
//!   ay-deps list <DEPS> [--host-os linux] [--host-cpu x64] [--os linux] [--cpu x64] [--var name=value ...]

use std::process::ExitCode;

use ay_build::deps::{Deps, Env, Value};

const USAGE: &str = "usage: ay-deps list <DEPS> [--host-os os] [--host-cpu cpu] [--os os] [--cpu cpu] [--var name=value ...]";

fn run(arguments: &[String]) -> Result<String, String> {
    let [command, path, rest @ ..] = arguments else {
        return Err(USAGE.into());
    };
    if command != "list" {
        return Err(format!("unknown command {command}"));
    }
    let (mut host_os, mut host_cpu, mut os, mut cpu) = (
        "linux".to_owned(),
        "x64".to_owned(),
        "linux".to_owned(),
        "x64".to_owned(),
    );
    let mut overrides = Vec::new();
    let mut iterator = rest.iter();
    while let Some(flag) = iterator.next() {
        let value = iterator
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--host-os" => host_os = value.clone(),
            "--host-cpu" => host_cpu = value.clone(),
            "--os" => os = value.clone(),
            "--cpu" => cpu = value.clone(),
            "--var" => {
                let (name, text) = value.split_once('=').ok_or("--var needs name=value")?;
                let parsed = match text {
                    "True" => Value::Bool(true),
                    "False" => Value::Bool(false),
                    other => Value::Str(other.to_owned()),
                };
                overrides.push((name.to_owned(), parsed));
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    let source = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let deps = Deps::parse(&source).map_err(|error| error.to_string())?;
    let env = Env::new(&deps, &host_os, &host_cpu, &os, &cpu, &overrides)
        .map_err(|error| error.to_string())?;
    let resolved = deps.resolve(&env).map_err(|error| error.to_string())?;
    let hooks = deps.hooks(&env).map_err(|error| error.to_string())?;
    serde_json::to_string(
        &serde_json::json!({ "deps": resolved, "hooks": hooks, "recursedeps": deps.recursedeps }),
    )
    .map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("ay-deps: {message}");
            ExitCode::FAILURE
        }
    }
}
