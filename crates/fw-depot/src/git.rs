//! Git through the `git` executable. Measured against gitoxide in `docs/depot-tools.md`: the executable handles partial and shallow fetches of
//! a pinned object from googlesource, which is what every dependency of a DEPS file needs.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

/// Runs `git <args>` in `dir` and returns trimmed stdout.
pub fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|source| Error::io(Path::new("git"), source))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    Err(Error::Command {
        command: format!("git {}", args.join(" ")),
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

pub fn is_repository(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// The commit `HEAD` points to, if any.
pub fn head(dir: &Path) -> Option<String> {
    run(dir, &["rev-parse", "--verify", "-q", "HEAD^{commit}"]).ok()
}

/// Whether `revision` names a commit that is already in the object database.
pub fn has_commit(dir: &Path, revision: &str) -> bool {
    run(dir, &["cat-file", "-e", &format!("{revision}^{{commit}}")]).is_ok()
}

pub fn is_full_hash(revision: &str) -> bool {
    revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// How much history a fetch brings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum History {
    #[default]
    Full,
    /// `--depth=1`: the pinned commit alone.
    Shallow,
    /// `--filter=blob:none`: every commit, no blobs until checkout.
    BlobLess,
}

/// Makes `dir` a checkout of `url` at `revision` (a commit, branch, tag or none for the remote's default branch) and returns the commit.
pub fn checkout(dir: &Path, url: &str, revision: Option<&str>, history: History) -> Result<String> {
    crate::error::fs::create_dir_all(dir)?;
    if !is_repository(dir) {
        run(dir, &["init", "--quiet"])?;
        run(dir, &["remote", "add", "origin", url])?;
    } else if run(dir, &["remote", "get-url", "origin"]).is_ok_and(|current| current != url) {
        run(dir, &["remote", "set-url", "origin", url])?;
    }
    let wanted = revision.filter(|revision| !revision.is_empty());
    let already = wanted.filter(|revision| is_full_hash(revision) && has_commit(dir, revision));
    if already.is_none() {
        let reference = match wanted {
            Some(revision) if is_full_hash(revision) => revision.to_owned(),
            Some(revision) if revision.starts_with("refs/") => revision.to_owned(),
            Some(revision) => {
                let tag = format!("refs/tags/{revision}");
                if run(dir, &["ls-remote", "--exit-code", "origin", &tag]).is_ok() {
                    tag
                } else {
                    format!("refs/heads/{}", revision.trim_start_matches("origin/"))
                }
            }
            None => "HEAD".to_owned(),
        };
        let mut arguments = vec!["fetch", "--quiet", "--no-tags"];
        match history {
            History::Shallow => arguments.push("--depth=1"),
            History::BlobLess => arguments.push("--filter=blob:none"),
            History::Full => {}
        }
        arguments.extend(["origin", &reference]);
        run(dir, &arguments)?;
    }
    let target = match already {
        Some(revision) => revision.to_owned(),
        None => run(dir, &["rev-parse", "FETCH_HEAD^{commit}"])?,
    };
    if head(dir).as_deref() != Some(target.as_str()) {
        run(dir, &["checkout", "--quiet", "--detach", &target])?;
    }
    Ok(target)
}
