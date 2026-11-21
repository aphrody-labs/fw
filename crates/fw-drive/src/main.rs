//! `fw-drive`: serve a drive over HTTP, or work on one from the shell.
//!
//! fw-drive serve --root DIR [--addr 127.0.0.1:8790] (--owner EMAIL | --behind-access) [--quota-gb N]
//! fw-drive ls    --root DIR --owner EMAIL [FOLDER_ID]          JSON lines on stdout
//! fw-drive put   --root DIR --owner EMAIL [--parent ID] FILE…  uploads, prints the new nodes
//! fw-drive get   --root DIR --owner EMAIL ID                   file bytes on stdout
//! fw-drive mkdir --root DIR --owner EMAIL [--parent ID] NAME

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use fw_drive::web::{App, Auth, router};
use fw_drive::{Drive, Sort, User};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "fw-drive",
    version,
    about = "A Drive-style file service: folders, uploads, trash, search, sharing"
)]
struct Cli {
    /// Directory holding drive.db and blobs/.
    #[arg(
        long,
        global = true,
        env = "FW_DRIVE_ROOT",
        default_value = "fw-drive-data"
    )]
    root: PathBuf,
    /// Acting user (single-user drive, or the shell commands).
    #[arg(long, global = true, env = "FW_DRIVE_OWNER")]
    owner: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve the web pages and the JSON API.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8790")]
        addr: String,
        /// Trust Cloudflare Access identity headers (only behind an Access app reached through a Tunnel).
        #[arg(long, conflicts_with = "owner")]
        behind_access: bool,
        /// Storage limit per owner, in gigabytes.
        #[arg(long)]
        quota_gb: Option<f64>,
    },
    /// List a folder (default: My Drive) as JSON lines.
    Ls { folder: Option<String> },
    /// Upload local files.
    Put {
        #[arg(long)]
        parent: Option<String>,
        files: Vec<PathBuf>,
    },
    /// Write a file's bytes to stdout.
    Get { id: String },
    /// Create a folder.
    Mkdir {
        #[arg(long)]
        parent: Option<String>,
        name: String,
    },
}

fn user(owner: Option<&str>) -> Result<User, String> {
    let email = owner.ok_or("--owner EMAIL (or FW_DRIVE_OWNER) is required")?;
    User::new(email).map_err(|e| e.to_string())
}

fn run(cli: Cli) -> Result<(), String> {
    let quota = match &cli.cmd {
        Cmd::Serve {
            quota_gb: Some(gb), ..
        } => Some((gb * 1024.0 * 1024.0 * 1024.0) as u64),
        _ => None,
    };
    let drive =
        Drive::open(&cli.root, quota).map_err(|e| format!("{}: {e}", cli.root.display()))?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut out = io::stdout().lock();
    let line = |out: &mut io::StdoutLock<'_>, v: &fw_drive::Node| -> Result<(), String> {
        writeln!(
            out,
            "{}",
            serde_json::to_string(v).map_err(|e| e.to_string())?
        )
        .map_err(|e| e.to_string())
    };
    match cli.cmd {
        Cmd::Serve {
            addr,
            behind_access,
            ..
        } => {
            let auth = if behind_access {
                Auth::Access
            } else {
                Auth::Owner(user(cli.owner.as_deref())?)
            };
            let app = Arc::new(App { drive, auth });
            rt.block_on(async move {
                let listener = tokio::net::TcpListener::bind(&addr)
                    .await
                    .map_err(|e| format!("{addr}: {e}"))?;
                eprintln!("fw-drive: serving {} on http://{addr}", cli.root.display());
                fw_http::server::serve_listener(listener, router(app))
                    .await
                    .map_err(|e| e.to_string())
            })
        }
        Cmd::Ls { folder } => {
            let u = user(cli.owner.as_deref())?;
            rt.block_on(async {
                let id = match folder {
                    Some(f) => f,
                    None => drive.my_drive(&u).await.map_err(|e| e.to_string())?.id,
                };
                for n in drive
                    .list(&u, &id, Sort::Name, false)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    line(&mut out, &n)?;
                }
                Ok(())
            })
        }
        Cmd::Put { parent, files } => {
            let u = user(cli.owner.as_deref())?;
            rt.block_on(async {
                let parent = match parent {
                    Some(p) => p,
                    None => drive.my_drive(&u).await.map_err(|e| e.to_string())?.id,
                };
                for f in files {
                    let name = f
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .ok_or(format!("{}: no file name", f.display()))?;
                    let file =
                        std::fs::File::open(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                    let node = drive
                        .upload(&u, &parent, &name, None, file)
                        .await
                        .map_err(|e| format!("{}: {e}", f.display()))?;
                    line(&mut out, &node)?;
                }
                Ok(())
            })
        }
        Cmd::Get { id } => {
            let u = user(cli.owner.as_deref())?;
            rt.block_on(async {
                let node = drive.content(&u, &id).await.map_err(|e| e.to_string())?;
                let mut body = drive.read(&node, None).await.map_err(|e| e.to_string())?;
                while let Some(chunk) = body.next().await {
                    out.write_all(&chunk.map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        }
        Cmd::Mkdir { parent, name } => {
            let u = user(cli.owner.as_deref())?;
            rt.block_on(async {
                let parent = match parent {
                    Some(p) => p,
                    None => drive.my_drive(&u).await.map_err(|e| e.to_string())?.id,
                };
                let n = drive
                    .create_folder(&u, &parent, &name)
                    .await
                    .map_err(|e| e.to_string())?;
                line(&mut out, &n)
            })
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fw-drive: {e}");
            ExitCode::from(2)
        }
    }
}
