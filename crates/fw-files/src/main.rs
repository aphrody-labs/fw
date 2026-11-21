//! `fw-files`, the binary behind `fw files`: file-manager capabilities as filters. Paths in (arguments
//! or stdin), JSON lines out.
//!
//! ```text
//! fw-files ls <dir|sftp://host/dir> [--sort natural|alpha|mtime|size|ext|mime|none] [-r] [-a] [--mime]
//! fw-files mime [path...]            # no paths or `-` reads one path per line from stdin
//! fw-files preview <path|sftp://...> [--lines N] [--bytes N]
//! fw-files cp|mv <src>... <dst> [--jobs N] [--on-conflict fail|skip|overwrite|rename]
//! fw-files rm <path>... [--jobs N]
//! fw-files rules [previewers|preloaders|spotters|fetchers]
//! ```

use clap::{Args, Parser, Subcommand};
use fw_files::entry::{Entry, list_dir};
use fw_files::mime;
use fw_files::preview::{self, Options as PreviewOptions};
use fw_files::rules::{self, RuleSet};
use fw_files::sort::{SortBy, Sorter};
use fw_files::tasks::{Conflict, Job, Op, Scheduler, Status, target_for};
use serde::Serialize;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "fw-files", version, about = "File manager engine distilled from yazi; every command writes JSON lines")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone, Default)]
struct Ssh {
    /// Private key for sftp:// paths (default: the IdentityFile entries `ssh -G` reports).
    #[arg(long)]
    identity: Option<PathBuf>,
    /// Environment variable holding the key passphrase.
    #[arg(long, value_name = "VAR")]
    passphrase_env: Option<String>,
    /// Record an unknown host key in known_hosts instead of refusing it.
    #[arg(long)]
    accept_new: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// List a directory, one JSON object per entry.
    Ls {
        path: String,
        #[arg(long, default_value = "natural")]
        sort: String,
        #[arg(short, long)]
        reverse: bool,
        /// Show hidden entries.
        #[arg(short = 'a', long)]
        hidden: bool,
        /// Do not group directories first.
        #[arg(long)]
        no_dirs_first: bool,
        /// Compare names case-sensitively.
        #[arg(long)]
        sensitive: bool,
        /// Add the detected MIME type to each entry.
        #[arg(long)]
        mime: bool,
        /// Print bare names instead of JSON.
        #[arg(long)]
        names: bool,
        #[command(flatten)]
        ssh: Ssh,
    },
    /// Detect MIME types and the yazi previewer for each path.
    Mime { paths: Vec<String> },
    /// Structured preview of a file or directory.
    Preview {
        path: String,
        #[arg(long, default_value_t = 60)]
        lines: usize,
        #[arg(long, default_value_t = 65536)]
        bytes: usize,
        #[command(flatten)]
        ssh: Ssh,
    },
    /// Copy files and directories through the worker pool; progress events on stdout.
    Cp(Transfer),
    /// Move files and directories (rename on one filesystem, copy then delete across).
    Mv(Transfer),
    /// Remove files and directories permanently.
    Rm {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// Print yazi's rule table as JSON lines.
    Rules {
        #[arg(default_value = "previewers")]
        kind: String,
    },
}

#[derive(Args)]
struct Transfer {
    /// Sources followed by the destination.
    #[arg(required = true, num_args = 2..)]
    paths: Vec<PathBuf>,
    /// Concurrent tasks (default: yazi's tasks.file_workers).
    #[arg(long)]
    jobs: Option<usize>,
    #[arg(long, default_value = "fail", value_parser = parse_conflict)]
    on_conflict: Conflict,
}

fn parse_conflict(s: &str) -> Result<Conflict, String> {
    Conflict::parse(s).ok_or_else(|| format!("`{s}`: expected fail, skip, overwrite or rename"))
}

fn emit<T: Serialize>(v: &T) -> io::Result<()> {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, v).map_err(io::Error::other)?;
    out.write_all(b"\n")?;
    out.flush()
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        // A closed pipe (`| head`) is not a failure.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fw-files: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> io::Result<ExitCode> {
    match cli.cmd {
        Cmd::Ls { path, sort, reverse, hidden, no_dirs_first, sensitive, mime: with_mime, names, ssh } => {
            let by = SortBy::parse(&sort).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("--sort `{sort}`: expected natural, alpha, mtime, size, ext, mime or none")))?;
            let mut entries = list(&path, hidden, with_mime, &ssh)?;
            Sorter { by, reverse, dirs_first: !no_dirs_first, sensitive }.sort(&mut entries);
            for e in &entries {
                if names { println!("{}", e.path) } else { emit(e)? }
            }
        }
        Cmd::Mime { paths } => {
            let paths = if paths.is_empty() || paths == ["-"] { io::stdin().lock().lines().collect::<Result<Vec<_>, _>>()? } else { paths };
            let mut failed = false;
            for p in paths.into_iter().filter(|p| !p.is_empty()) {
                match mime::detect(Path::new(&p)) {
                    Ok(m) => {
                        let url = if m == mime::DIRECTORY { format!("{p}/") } else { p.clone() };
                        emit(&serde_json::json!({ "path": p, "mime": m, "previewer": rules::previewer(&m, &url) }))?;
                    }
                    Err(e) => {
                        failed = true;
                        emit(&serde_json::json!({ "path": p, "error": e.to_string() }))?;
                    }
                }
            }
            return Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS });
        }
        Cmd::Preview { path, lines, bytes, ssh } => {
            let opts = PreviewOptions { max_lines: lines, max_bytes: bytes };
            let p = if is_remote(&path) { remote_preview(&path, opts, &ssh)? } else { preview::preview(Path::new(&path), opts)? };
            emit(&p)?;
        }
        Cmd::Cp(t) => return transfer(Op::Copy, t),
        Cmd::Mv(t) => return transfer(Op::Move, t),
        Cmd::Rm { paths, jobs } => {
            let list = paths.into_iter().map(|src| Job { op: Op::Remove, src, dst: None, conflict: Conflict::Fail }).collect();
            return schedule(list, jobs);
        }
        Cmd::Rules { kind } => {
            let p = rules::preset();
            let set: &RuleSet = match kind.as_str() {
                "previewers" => &p.previewers,
                "preloaders" => &p.preloaders,
                "spotters" => &p.spotters,
                "fetchers" => &p.fetchers,
                other => return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("`{other}`: expected previewers, preloaders, spotters or fetchers"))),
            };
            for r in &set.rules {
                emit(r)?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn is_remote(path: &str) -> bool {
    path.starts_with("sftp://")
}

fn transfer(op: Op, t: Transfer) -> io::Result<ExitCode> {
    let mut paths = t.paths;
    let dst = paths.pop().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing destination"))?;
    let many = paths.len() > 1;
    let jobs = paths
        .into_iter()
        .map(|src| Ok(Job { op, dst: Some(target_for(&src, &dst, many)?), src, conflict: t.on_conflict }))
        .collect::<io::Result<Vec<_>>>()?;
    schedule(jobs, t.jobs)
}

/// Run the jobs on the pool, one JSON event per line; exits non-zero unless every task finished.
fn schedule(jobs: Vec<Job>, workers: Option<usize>) -> io::Result<ExitCode> {
    let workers = workers.unwrap_or_else(|| rules::preset().file_workers);
    let sched = Scheduler::new(workers, |event| {
        // stdout is gone when the reader quit; the work still completes.
        emit(&event).ok();
    });
    for job in jobs {
        sched.submit(job);
    }
    let all_done = sched.finish().iter().all(|s| s.status == Status::Done);
    Ok(if all_done { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn list(path: &str, hidden: bool, with_mime: bool, ssh: &Ssh) -> io::Result<Vec<Entry>> {
    if is_remote(path) {
        return remote_list(path, hidden, with_mime, ssh);
    }
    let mut entries = list_dir(Path::new(path), hidden)?;
    if with_mime {
        for e in &mut entries {
            e.mime = mime::detect(Path::new(&e.path)).ok();
        }
    }
    Ok(entries)
}

#[cfg(feature = "sftp")]
mod remote {
    use super::*;
    use fw_files::preview::Source;
    use fw_files::sftp::{Options, Remote, Url};

    fn runtime() -> io::Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread().enable_all().build()
    }

    fn annotate(path: &str, e: io::Error) -> io::Error {
        io::Error::new(e.kind(), format!("{path}: {e}"))
    }

    async fn connect(url: &Url, ssh: &Ssh) -> io::Result<Remote> {
        let passphrase = ssh.passphrase_env.as_deref().and_then(|v| std::env::var(v).ok());
        Remote::connect(url, &Options { identity: ssh.identity.clone(), passphrase, accept_new: ssh.accept_new }).await
    }

    pub fn list(path: &str, hidden: bool, with_mime: bool, ssh: &Ssh) -> io::Result<Vec<Entry>> {
        let url = Url::parse(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        runtime()?.block_on(async {
            let remote = connect(&url, ssh).await?;
            let target = remote.stat(&url.path).await.map_err(|e| annotate(path, e))?;
            // Like `ls`, a file lists as itself.
            let found = if target.is_dir() { remote.list(&url.path).await.map_err(|e| annotate(path, e))? } else { vec![target] };
            let mut entries: Vec<Entry> = found.into_iter().filter(|e| hidden || !e.hidden).collect();
            if with_mime {
                for e in &mut entries {
                    e.mime = Some(if e.is_dir() { mime::DIRECTORY.to_string() } else { mime::detect_bytes(&e.name, &remote.read_head(&e.path, mime::SNIFF_LEN).await.map(|h| h.0).unwrap_or_default()) });
                }
            }
            Ok(entries)
        })
    }

    pub fn preview(path: &str, opts: PreviewOptions, ssh: &Ssh) -> io::Result<preview::Preview> {
        let url = Url::parse(path).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        runtime()?.block_on(async {
            let remote = connect(&url, ssh).await?;
            let entry = remote.stat(&url.path).await.map_err(|e| annotate(path, e))?;
            if entry.is_dir() {
                let items = remote.list(&url.path).await.map_err(|e| annotate(path, e))?;
                return Ok(preview::render(&url.path, mime::DIRECTORY, 0, Source::Folder(items), opts));
            }
            let (head, more) = remote.read_head(&url.path, opts.max_bytes).await.map_err(|e| annotate(path, e))?;
            let m = mime::detect_bytes(&entry.name, &head);
            Ok(preview::render(&url.path, &m, entry.size, Source::Bytes { head, more }, opts))
        })
    }
}

#[cfg(feature = "sftp")]
use remote::{list as remote_list, preview as remote_preview};

#[cfg(not(feature = "sftp"))]
fn remote_list(_: &str, _: bool, _: bool, _: &Ssh) -> io::Result<Vec<Entry>> {
    Err(io::Error::other("built without the `sftp` feature"))
}

#[cfg(not(feature = "sftp"))]
fn remote_preview(_: &str, _: PreviewOptions, _: &Ssh) -> io::Result<preview::Preview> {
    Err(io::Error::other("built without the `sftp` feature"))
}
