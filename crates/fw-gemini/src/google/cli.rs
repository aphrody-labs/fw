//! Entry point of the former `fw-gemini/google` binary.

use std::{ffi::OsString, path::PathBuf};

use crate::google::{GoogleProject, GoogleRuntime};
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "fw-gemini/google", about = "Unified Aphrody Google/Gemini runtime")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the resolved public runtime contract as JSON.
    Status,
    /// Print shell exports for the shared non-secret runtime contract.
    Env,
    /// Validate the resolved project mapping and required local roots.
    Doctor,
}

#[derive(Debug, Serialize)]
struct StatusReport {
    provider: &'static str,
    project: &'static str,
    project_kind: &'static str,
    location: String,
    home: String,
    auth: String,
    capabilities: [&'static str; 6],
}

fn report(runtime: &GoogleRuntime) -> StatusReport {
    StatusReport {
        provider: "fw-gemini",
        project: runtime.project.id(),
        project_kind: match runtime.project {
            GoogleProject::Aphrody => "aphrody",
        },
        location: runtime.location.clone(),
        home: runtime.home.clone(),
        auth: runtime.auth.clone(),
        capabilities: [
            "vertex",
            "gemini-oauth",
            "gemini-web",
            "google-web",
            "mcp",
            "bxc",
        ],
    }
}

fn resolved_home(runtime: &GoogleRuntime) -> Option<PathBuf> {
    if let Some(relative) = runtime.home.strip_prefix("~/") {
        return Some(
            PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?)
                .join(relative),
        );
    }
    Some(PathBuf::from(&runtime.home))
}

fn run(cli: Cli) -> Result<i32, Box<dyn std::error::Error>> {
    let runtime = GoogleRuntime::from_env();
    match cli.command.unwrap_or(Command::Status) {
        Command::Status => {
            println!(
                "{}",
                serde_json::to_string_pretty(&report(&runtime)).expect("serializable")
            )
        }
        Command::Env => {
            println!("export GOOGLE_CLOUD_PROJECT={}", runtime.project.id());
            println!("export GOOGLE_CLOUD_LOCATION={}", runtime.location);
            println!("export APHRODY_HOME={}", runtime.home);
            println!("export APHRODY_GCP_AUTH={}", runtime.auth);
        }
        Command::Doctor => {
            if runtime.location.is_empty() {
                return Err("Google location is empty".into());
            }
            if runtime.home.is_empty() {
                return Err("Aphrody home is empty".into());
            }
            let home = resolved_home(&runtime).ok_or("home directory is not resolvable")?;
            if !home.is_dir() {
                return Err(format!("Aphrody home does not exist: {}", home.display()).into());
            }
            println!("APHRODY_GOOGLE_DOCTOR=PASS");
            println!("project={}", runtime.project.id());
            println!("home={}", home.display());
            println!("capabilities=vertex,gemini-oauth,gemini-web,google-web,mcp,bxc");
        }
    }
    Ok(0)
}

/// Entry point of the former `fw-gemini/google` binary, now reached through the `aphrody`
/// multi-call binary. `args[0]` is the program name (argv convention). Returns the process exit
/// code.
#[must_use]
pub fn cli_main(args: Vec<OsString>) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return error.exit_code();
        }
    };
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Error: {error}");
            1
        }
    }
}
