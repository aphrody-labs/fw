//! `git cl` and the branch helpers of depot_tools: `issue`, `status`, `upload`, `land`, `patch`, `description`, `new-branch`, `map-branches`.
//! The branch-to-change association lives where git cl keeps it (`branch.<name>.gerritissue` and friends in the repository's git config), so a
//! repository can move between the two tools. `--dry-run` describes the push or the request and does nothing.

use std::path::Path;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::gerrit::{self, Client, Outcome, UploadOptions};
use crate::git;

/// `chromium.googlesource.com/x` is reviewed on `chromium-review.googlesource.com`.
pub fn review_host(remote_url: &str) -> Option<String> {
    let rest = remote_url.split("://").nth(1).unwrap_or(remote_url);
    let rest = rest.rsplit('@').next().unwrap_or(rest);
    let host = rest.split(['/', ':']).next()?;
    let (label, tail) = host.split_once('.')?;
    if tail != "googlesource.com" || label.ends_with("-review") {
        return (label.ends_with("-review") && tail == "googlesource.com").then(|| host.to_owned());
    }
    Some(format!("{label}-review.{tail}"))
}

fn config(repo: &Path, key: &str) -> Option<String> {
    git::run(repo, &["config", "--get", key]).ok().filter(|value| !value.is_empty())
}

pub fn current_branch(repo: &Path) -> Result<String> {
    git::run(repo, &["symbolic-ref", "--short", "HEAD"])
        .map_err(|_| Error::message("not on a branch (detached HEAD)"))
}

/// The change a branch is associated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Association {
    pub issue: u64,
    pub server: String,
    pub patchset: Option<u64>,
}

pub fn association(repo: &Path, branch: &str) -> Option<Association> {
    let issue = config(repo, &format!("branch.{branch}.gerritissue"))?.parse().ok()?;
    let server = config(repo, &format!("branch.{branch}.gerritserver"))?;
    let patchset = config(repo, &format!("branch.{branch}.gerritpatchset")).and_then(|value| value.parse().ok());
    Some(Association { issue, server, patchset })
}

pub fn associate(repo: &Path, branch: &str, issue: u64, server: &str, patchset: Option<u64>) -> Result<()> {
    git::run(repo, &["config", &format!("branch.{branch}.gerritissue"), &issue.to_string()])?;
    git::run(repo, &["config", &format!("branch.{branch}.gerritserver"), server])?;
    if let Some(patchset) = patchset {
        git::run(repo, &["config", &format!("branch.{branch}.gerritpatchset"), &patchset.to_string()])?;
    }
    Ok(())
}

pub fn dissociate(repo: &Path, branch: &str) {
    for key in ["gerritissue", "gerritserver", "gerritpatchset", "gerritsquashhash"] {
        let _ = git::run(repo, &["config", "--unset", &format!("branch.{branch}.{key}")]);
    }
}

/// The server to talk to: the override, the association, `gerrit.host`, else derived from the remote.
pub fn server(repo: &Path, branch: Option<&str>, override_url: Option<&str>) -> Result<String> {
    if let Some(url) = override_url {
        return Ok(normalize_server(url));
    }
    if let Some(found) = branch.and_then(|branch| association(repo, branch)) {
        return Ok(found.server);
    }
    if let Some(host) = config(repo, "gerrit.host") {
        return Ok(normalize_server(&host));
    }
    let remote = git::run(repo, &["remote", "get-url", "origin"])?;
    review_host(&remote)
        .map(|host| format!("https://{host}"))
        .ok_or_else(|| Error::message(format!("cannot derive a Gerrit host from {remote}; pass --gerrit-server")))
}

fn normalize_server(url: &str) -> String {
    if url.contains("://") {
        url.trim_end_matches('/').to_owned()
    } else {
        format!("https://{}", url.trim_end_matches('/'))
    }
}

/// A `Change-Id:` footer, added to the HEAD commit message when it has none.
pub fn ensure_change_id(repo: &Path) -> Result<bool> {
    let message = git::run(repo, &["log", "-1", "--format=%B"])?;
    if message.lines().any(|line| line.starts_with("Change-Id: I")) {
        return Ok(false);
    }
    let tree = git::run(repo, &["write-tree"])?;
    let parent = git::run(repo, &["rev-parse", "HEAD"])?;
    let seed = format!("{tree}\n{parent}\n{message}\n{}", std::process::id());
    let mut child = std::process::Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(repo)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|source| Error::io(Path::new("git"), source))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin
            .write_all(seed.as_bytes())
            .map_err(|source| Error::io(Path::new("git"), source))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|source| Error::io(Path::new("git"), source))?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let amended = format!("{}\n\nChange-Id: I{id}\n", message.trim_end());
    git::run(repo, &["commit", "--amend", "--quiet", "-m", &amended])?;
    Ok(true)
}

/// The command `upload` runs, as an argument list.
pub fn push_command(remote: &str, target: &str, options: &UploadOptions) -> Vec<String> {
    vec![
        "push".to_owned(),
        remote.to_owned(),
        format!("HEAD:{}", gerrit::upload_ref(target, options)),
    ]
}

/// `upload`: add a Change-Id when missing, push to `refs/for/<target>`, and when the push reports the change URL, associate the branch.
pub fn upload(repo: &Path, target: &str, options: &UploadOptions, dry_run: bool) -> Result<String> {
    let command = push_command("origin", target, options);
    if dry_run {
        return Ok(format!("git {}", command.join(" ")));
    }
    ensure_change_id(repo)?;
    let arguments: Vec<&str> = command.iter().map(String::as_str).collect();
    let output = std::process::Command::new("git")
        .args(&arguments)
        .current_dir(repo)
        .output()
        .map_err(|source| Error::io(Path::new("git"), source))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        return Err(Error::Command {
            command: format!("git {}", command.join(" ")),
            status: output.status.to_string(),
            stderr: text,
        });
    }
    if let Some((server, issue)) = parse_change_url(&text)
        && let Ok(branch) = current_branch(repo)
    {
        associate(repo, &branch, issue, &server, None)?;
    }
    Ok(text)
}

/// `https://host/c/project/+/123` (or `https://host/123`) in the text a push prints.
pub fn parse_change_url(text: &str) -> Option<(String, u64)> {
    for word in text.split_whitespace() {
        let Some(rest) = word.strip_prefix("https://") else {
            continue;
        };
        let (host, path) = rest.split_once('/')?;
        let number = path.rsplit('/').next()?.trim_end_matches(|ch: char| !ch.is_ascii_digit());
        if let Ok(issue) = number.parse::<u64>()
            && host.contains("-review.")
        {
            return Some((format!("https://{host}"), issue));
        }
    }
    None
}

/// `patch`: fetch a patch set into a new branch.
pub fn patch(repo: &Path, client: &Client, spec: &str, branch: Option<&str>) -> Result<String> {
    let (number, requested) = match spec.split_once('/') {
        Some((number, patchset)) => (number, patchset.parse::<u64>().ok()),
        None => (spec.rsplit('/').next().unwrap_or(spec), None),
    };
    let number: u64 = number
        .parse()
        .map_err(|_| Error::message(format!("{spec}: not a change number")))?;
    let patchset = match requested {
        Some(patchset) => patchset,
        None => {
            let change = client.change(&number.to_string())?;
            let current = change["current_revision"].as_str().unwrap_or_default();
            change["revisions"][current]["_number"]
                .as_u64()
                .ok_or_else(|| Error::message(format!("change {number} has no current patch set")))?
        }
    };
    let reference = gerrit::change_ref(number, patchset);
    git::run(repo, &["fetch", "--quiet", "origin", &reference])?;
    let name = branch.map_or_else(|| format!("change-{number}"), str::to_owned);
    git::run(repo, &["checkout", "--quiet", "-b", &name, "FETCH_HEAD"])?;
    associate(repo, &name, number, &client.base, Some(patchset))?;
    Ok(format!("{name}: change {number} patch set {patchset} ({reference})"))
}

/// One line per branch with a change: `branch  url (status)`.
pub fn status(repo: &Path, client: &Client, branch: &str, found: &Association) -> Result<String> {
    let change = client.change(&found.issue.to_string())?;
    let state = change["status"].as_str().unwrap_or("UNKNOWN");
    let subject = change["subject"].as_str().unwrap_or_default();
    let _ = repo;
    Ok(format!("{branch:<20} {}/c/{} ({state}) {subject}", found.server, found.issue))
}

/// `land`: submit the change associated with the branch.
pub fn land(client: &Client, issue: u64) -> Result<Value> {
    match client.submit(&issue.to_string())? {
        Outcome::Sent(value) => Ok(value),
        Outcome::Skipped(described) => Ok(serde_json::json!({
            "dry_run": true,
            "method": described.method,
            "url": described.url,
            "body": described.body,
        })),
    }
}

/// `new-branch NAME [--upstream REF]`.
pub fn new_branch(repo: &Path, name: &str, upstream: Option<&str>) -> Result<()> {
    let upstream = match upstream {
        Some(upstream) => upstream.to_owned(),
        None => current_branch(repo).unwrap_or_else(|_| "origin/main".to_owned()),
    };
    git::run(repo, &["checkout", "--quiet", "-b", name, "--track", &upstream])?;
    Ok(())
}

/// A text tree of local branches under their upstreams, with the change of each branch.
pub fn map_branches(repo: &Path) -> Result<String> {
    let listing = git::run(repo, &["for-each-ref", "--format=%(refname:short)\t%(upstream:short)", "refs/heads"])?;
    let branches: Vec<(String, String)> = listing
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(branch, upstream)| (branch.to_owned(), upstream.to_owned()))
        .collect();
    let current = current_branch(repo).ok();
    let mut out = String::new();
    fn render(
        repo: &Path,
        branches: &[(String, String)],
        current: Option<&str>,
        parent: &str,
        depth: usize,
        out: &mut String,
    ) {
        for (branch, upstream) in branches {
            if upstream != parent {
                continue;
            }
            let marker = if current == Some(branch.as_str()) { "*" } else { " " };
            let change = association(repo, branch).map_or(String::new(), |found| format!(" (change {})", found.issue));
            out.push_str(&format!("{}{marker} {branch}{change}\n", "  ".repeat(depth)));
            render(repo, branches, current, branch, depth + 1, out);
        }
    }
    let known: Vec<&str> = branches.iter().map(|(branch, _)| branch.as_str()).collect();
    let mut roots: Vec<&str> = branches
        .iter()
        .map(|(_, upstream)| upstream.as_str())
        .filter(|upstream| !known.contains(upstream))
        .collect();
    roots.sort_unstable();
    roots.dedup();
    for root in roots {
        let label = if root.is_empty() { "(no upstream)" } else { root };
        out.push_str(&format!("{label}\n"));
        render(repo, &branches, current.as_deref(), root, 1, &mut out);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_hosts_follow_googlesource_naming() {
        assert_eq!(
            review_host("https://chromium.googlesource.com/chromium/src.git").as_deref(),
            Some("chromium-review.googlesource.com")
        );
        assert_eq!(
            review_host("https://chromium-review.googlesource.com/x").as_deref(),
            Some("chromium-review.googlesource.com")
        );
        assert_eq!(review_host("https://github.com/a/b"), None);
    }

    #[test]
    fn the_change_url_of_a_push_is_found() {
        let text = "remote: SUCCESS\nremote:\nremote:   https://chromium-review.googlesource.com/c/chromium/src/+/123456 subject [NEW]\n";
        assert_eq!(
            parse_change_url(text),
            Some(("https://chromium-review.googlesource.com".to_owned(), 123456))
        );
    }
}
