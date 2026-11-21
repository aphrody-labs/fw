//! Git access: remotes, tags and the gitlinks of the fw checkout. Everything goes through the `git` binary.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Where the forks live: `https://github.com/<repo>.git` unless `$FW_KERNEL_GIT_BASE` names a mirror or a local directory.
#[derive(Debug, Clone)]
pub struct Remotes {
    base: String,
}

impl Remotes {
    pub fn from_env() -> Remotes {
        Remotes::new(&std::env::var("FW_KERNEL_GIT_BASE").unwrap_or_else(|_| "https://github.com".to_owned()))
    }

    pub fn new(base: &str) -> Remotes {
        Remotes { base: base.trim_end_matches('/').to_owned() }
    }

    pub fn url(&self, repo: &str) -> String {
        format!("{}/{repo}.git", self.base)
    }
}

/// Runs git and returns stdout; a non-zero exit is an error carrying stderr.
pub fn git(directory: Option<&Path>, arguments: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(directory) = directory {
        command.arg("-C").arg(directory);
    }
    command.args(arguments).env("GIT_TERMINAL_PROMPT", "0");
    let output = command.output().map_err(|error| Error::Command { command: format!("git {}", arguments.join(" ")), detail: error.to_string() })?;
    if !output.status.success() {
        return Err(Error::Command { command: format!("git {}", arguments.join(" ")), detail: String::from_utf8_lossy(&output.stderr).trim().to_owned() });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// [`git`] with `input` on stdin.
pub fn git_with_input(directory: Option<&Path>, arguments: &[&str], input: &str) -> Result<String> {
    use std::io::Write;
    use std::process::Stdio;
    let failed = |detail: String| Error::Command { command: format!("git {}", arguments.join(" ")), detail };
    let mut command = Command::new("git");
    if let Some(directory) = directory {
        command.arg("-C").arg(directory);
    }
    let mut child = command.args(arguments).env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|error| failed(error.to_string()))?;
    let mut stdin = child.stdin.take().ok_or_else(|| failed("no stdin".to_owned()))?;
    stdin.write_all(input.as_bytes()).map_err(|error| failed(error.to_string()))?;
    drop(stdin);
    let output = child.wait_with_output().map_err(|error| failed(error.to_string()))?;
    if !output.status.success() {
        return Err(failed(String::from_utf8_lossy(&output.stderr).trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A tag of a remote: the object it names and the commit it points at (they differ for annotated tags).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTag {
    pub name: String,
    pub object: String,
    pub commit: String,
}

/// Tags matching `pattern` (a glob such as `cachyos-*`) with their peeled commits.
pub fn remote_tags(url: &str, pattern: &str) -> Result<Vec<RemoteTag>> {
    list_tags(url, &[format!("refs/tags/{pattern}")])
}

fn list_tags(url: &str, patterns: &[String]) -> Result<Vec<RemoteTag>> {
    let mut arguments = vec!["ls-remote", "--tags", url];
    arguments.extend(patterns.iter().map(String::as_str));
    let listing = git(None, &arguments)?;
    let mut tags: Vec<RemoteTag> = Vec::new();
    for line in listing.lines() {
        let Some((sha, reference)) = line.split_once('\t') else { continue };
        let Some(name) = reference.strip_prefix("refs/tags/") else { continue };
        match name.strip_suffix("^{}") {
            Some(base) => {
                if let Some(tag) = tags.iter_mut().find(|tag| tag.name == base) {
                    tag.commit = sha.to_owned();
                }
            }
            None => tags.push(RemoteTag { name: name.to_owned(), object: sha.to_owned(), commit: sha.to_owned() }),
        }
    }
    Ok(tags)
}

pub fn remote_tag(url: &str, name: &str) -> Result<Option<RemoteTag>> {
    let tags = list_tags(url, &[format!("refs/tags/{name}"), format!("refs/tags/{name}^{{}}")])?;
    Ok(tags.into_iter().find(|tag| tag.name == name))
}

/// The commit a remote ref (`HEAD`, `refs/heads/master`) points at.
pub fn remote_head(url: &str, reference: &str) -> Result<Option<String>> {
    let listing = git(None, &["ls-remote", url, reference])?;
    Ok(listing.lines().next().and_then(|line| line.split_once('\t')).map(|(sha, _)| sha.to_owned()))
}

pub fn is_work_tree(root: &Path) -> bool {
    git(Some(root), &["rev-parse", "--git-dir"]).is_ok()
}

/// The commit recorded for `path` in the index of the fw checkout when it is a gitlink.
pub fn gitlink(root: &Path, path: &str) -> Option<String> {
    let listing = git(Some(root), &["ls-files", "-s", "--", path]).ok()?;
    let line = listing.lines().next()?;
    let mut fields = line.split_whitespace();
    (fields.next()? == "160000").then(|| fields.next().map(str::to_owned))?
}

/// Whether `.gitmodules` of the fw checkout declares `path` as a submodule that is also a gitlink of the index.
pub fn declared_submodule(root: &Path, path: &str) -> bool {
    let declared = git(Some(root), &["config", "-f", ".gitmodules", "--get-regexp", r"^submodule\..*\.path$"]).is_ok_and(|listing| listing.lines().any(|line| line.split_once(' ').is_some_and(|(_, value)| value == path)));
    declared && gitlink(root, path).is_some()
}

/// Records `commit` as the gitlink of `path` in the index (the commit itself is the caller's to make).
pub fn set_gitlink(root: &Path, path: &str, commit: &str) -> Result<()> {
    git(Some(root), &["update-index", "--add", "--cacheinfo", &format!("160000,{commit},{path}")]).map(|_| ())
}

/// `HEAD` of a checked out tree, when it is one.
pub fn checkout_head(directory: &Path) -> Option<String> {
    if !directory.join(".git").exists() {
        return None;
    }
    git(Some(directory), &["rev-parse", "--verify", "-q", "HEAD"]).ok().map(|head| head.trim().to_owned())
}
