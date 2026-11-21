//! `fw kernel sync`: fetch the pinned commit of a tree into its submodule path, shallow, with a partial/sparse profile.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::pins::{Pins, Profile, Tree};
use crate::repo::{self, Remotes, git, git_with_input};

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub tree: String,
    pub path: String,
    pub commit: String,
    pub profile: String,
    /// `cloned`, `updated`, `reprofiled` (same commit, other profile) or `unchanged`.
    pub action: &'static str,
    pub seconds: f64,
    /// Bytes of the work tree and of the git directory together.
    pub bytes: u64,
}

pub struct Options {
    pub trees: Vec<String>,
    pub profile: Option<String>,
}

pub fn run(root: &Path, pins: &Pins, remotes: &Remotes, options: &Options) -> Result<Vec<Outcome>> {
    let selected: Vec<&Tree> = if options.trees.is_empty() { pins.trees.iter().collect() } else { options.trees.iter().map(|id| pins.tree(id)).collect::<Result<_>>()? };
    selected.into_iter().map(|tree| sync_tree(root, pins, remotes, tree, options.profile.as_deref())).collect()
}

fn sync_tree(root: &Path, pins: &Pins, remotes: &Remotes, tree: &Tree, profile_override: Option<&str>) -> Result<Outcome> {
    let started = Instant::now();
    let profile_name = profile_override.unwrap_or(&tree.profile);
    let profile = pins.profile(profile_name)?;
    let directory = root.join(&tree.path);
    std::fs::create_dir_all(&directory).map_err(|source| Error::Io { path: directory.clone(), source })?;

    let url = remotes.url(&tree.repo);
    let fresh = !directory.join(".git").exists();
    if fresh {
        init(root, &directory, &tree.path)?;
        // Kernel files are byte-exact: no end-of-line conversion whatever the user's global git config says.
        git(Some(&directory), &["config", "core.autocrlf", "false"])?;
    }
    if git(Some(&directory), &["remote", "get-url", "origin"]).is_ok() {
        git(Some(&directory), &["remote", "set-url", "origin", &url])?;
    } else {
        git(Some(&directory), &["remote", "add", "origin", &url])?;
    }

    let previous_head = repo::checkout_head(&directory);
    let previous_profile = git(Some(&directory), &["config", "--get", "fw.kernelProfile"]).ok().map(|value| value.trim().to_owned());
    let had_filter = git(Some(&directory), &["config", "--get", "extensions.partialClone"]).is_ok();

    configure_filter(&directory, profile)?;
    // A lazy fetch here would download the whole history of a partial clone for one existence check.
    let present = git(Some(&directory), &["--no-lazy-fetch", "cat-file", "-e", &format!("{}^{{commit}}", tree.commit)]).is_ok();
    if !present || (had_filter && profile.filter.is_none()) {
        fetch(&directory, profile, &tree.commit, had_filter && profile.filter.is_none())?;
    }
    configure_sparse(&directory, profile)?;
    git(Some(&directory), &["-c", "advice.detachedHead=false", "checkout", "-q", "--detach", "-f", &tree.commit])?;
    git(Some(&directory), &["read-tree", "-mu", "HEAD"])?;
    git(Some(&directory), &["config", "fw.kernelProfile", profile_name])?;
    if repo::declared_submodule(root, &tree.path) {
        git(Some(root), &["submodule", "init", "--", &tree.path])?;
    }

    let action = match (&previous_head, fresh) {
        (_, true) => "cloned",
        (Some(head), false) if *head == tree.commit && previous_profile.as_deref() == Some(profile_name) => "unchanged",
        (Some(head), false) if *head == tree.commit => "reprofiled",
        _ => "updated",
    };
    Ok(Outcome { tree: tree.id.clone(), path: tree.path.clone(), commit: tree.commit.clone(), profile: profile_name.to_owned(), action, seconds: started.elapsed().as_secs_f64(), bytes: size_of_directory(&directory) })
}

/// Creates the repository with its git directory under the superproject's `.git/modules` when it has one,
/// so that the path behaves as an initialised submodule.
fn init(root: &Path, directory: &Path, path: &str) -> Result<()> {
    let modules: PathBuf = root.join(".git").join("modules").join(path);
    if root.join(".git").is_dir() {
        if let Some(parent) = modules.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io { path: parent.to_owned(), source })?;
        }
        let separate = format!("--separate-git-dir={}", modules.display());
        git(None, &["init", "-q", &separate, &directory.display().to_string()]).map(|_| ())
    } else {
        git(None, &["init", "-q", &directory.display().to_string()]).map(|_| ())
    }
}

fn configure_filter(directory: &Path, profile: &Profile) -> Result<()> {
    match &profile.filter {
        Some(filter) => {
            git(Some(directory), &["config", "extensions.partialClone", "origin"])?;
            git(Some(directory), &["config", "remote.origin.promisor", "true"])?;
            git(Some(directory), &["config", "remote.origin.partialclonefilter", filter])?;
        }
        None => {
            if git(Some(directory), &["config", "--get", "remote.origin.partialclonefilter"]).is_ok() {
                git(Some(directory), &["config", "--unset", "remote.origin.partialclonefilter"])?;
            }
        }
    }
    Ok(())
}

fn fetch(directory: &Path, profile: &Profile, commit: &str, refetch: bool) -> Result<()> {
    let mut arguments = vec!["fetch", "-q", "--depth", "1"];
    if let Some(filter) = &profile.filter {
        arguments.push("--filter");
        arguments.push(filter);
    }
    if refetch {
        arguments.push("--refetch");
    }
    arguments.extend(["origin", commit]);
    git(Some(directory), &arguments).map(|_| ())
}

fn configure_sparse(directory: &Path, profile: &Profile) -> Result<()> {
    if profile.sparse.is_empty() {
        if git(Some(directory), &["config", "--get", "core.sparseCheckout"]).is_ok_and(|value| value.trim() == "true") {
            git(Some(directory), &["sparse-checkout", "disable"])?;
        }
        return Ok(());
    }
    // `sparse-checkout` keeps the patterns and the enabled flag in the same place (the worktree config once it has been
    // disabled), where editing .git/config by hand would be overridden.
    git_with_input(Some(directory), &["sparse-checkout", "set", "--no-cone", "--stdin"], &format!("{}\n", profile.sparse.join("\n")))?;
    Ok(())
}

/// Bytes under `directory`, the git directory behind a `.git` file included.
fn size_of_directory(directory: &Path) -> u64 {
    let mut total = tree_bytes(directory);
    if let Ok(git_dir) = git(Some(directory), &["rev-parse", "--absolute-git-dir"]) {
        let git_dir = PathBuf::from(git_dir.trim());
        if !git_dir.starts_with(directory) {
            total += tree_bytes(&git_dir);
        }
    }
    total
}

fn tree_bytes(directory: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(directory) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => tree_bytes(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}
