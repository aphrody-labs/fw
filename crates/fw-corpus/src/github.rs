//! GitHub sources: markdown trees (git trees API for the listing, raw.githubusercontent.com at the
//! pinned commit for the bytes) and organisation repo inventories.

use crate::chunk;
use crate::http::{Client, pipeline};
use crate::sources::{Github, Opts, Report, Repo, UrlMap};
use crate::store::{DocInput, Store};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;

const MAX_FILE: u64 = 1_500_000;

fn token() -> Option<String> {
    for name in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(value) = std::env::var(name)
            && !value.trim().is_empty()
        {
            return Some(value);
        }
    }
    let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned()).filter(|token| !token.is_empty())
}

fn api(client: &Client, token: Option<&str>, path: &str) -> Result<Value> {
    let bearer = token.map(|token| format!("Bearer {token}"));
    let mut headers = vec![("Accept", "application/vnd.github+json"), ("X-GitHub-Api-Version", "2022-11-28")];
    if let Some(bearer) = &bearer {
        headers.push(("Authorization", bearer.as_str()));
    }
    let response = client.get_with(&format!("https://api.github.com/{path}"), &headers)?;
    if response.status != 200 {
        bail!("GitHub API {path}: HTTP {}: {}", response.status, response.text().chars().take(200).collect::<String>());
    }
    serde_json::from_slice(&response.body).with_context(|| format!("GitHub API {path}"))
}

fn humanize(path: &str) -> String {
    let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md");
    stem.replace(['-', '_'], " ")
}

/// Published URL for a source file (or the commit-pinned blob URL).
pub fn map_url(map: UrlMap, repo: &str, prefix: &str, path: &str, commit: &str) -> String {
    let relative = path.strip_prefix(prefix).unwrap_or(path);
    let stem = relative.trim_end_matches(".md");
    match map {
        UrlMap::Blob => format!("https://github.com/{repo}/blob/{commit}/{path}"),
        UrlMap::MdBook(base) => {
            let stem = stem.strip_suffix("README").map_or(stem.to_owned(), |head| format!("{head}index"));
            format!("{base}/{stem}.html")
        }
        UrlMap::Kotlin => format!("https://kotlinlang.org/docs/{stem}.html"),
        UrlMap::GithubDocs => {
            let page = stem.strip_suffix("/index").or_else(|| (stem == "index").then_some("")).unwrap_or(stem);
            format!("https://docs.github.com/en/{page}").trim_end_matches('/').to_owned()
        }
    }
}

fn wanted(repo: &Repo, path: &str, size: u64) -> bool {
    let lower = path.to_ascii_lowercase();
    path.starts_with(repo.prefix)
        && (lower.ends_with(".md") || lower.ends_with(".markdown"))
        && size <= MAX_FILE
        && !lower.ends_with("/summary.md")
        && !["/node_modules/", "/testdata/", "/build/", "/.github/"].iter().any(|part| lower.contains(part))
}

pub fn sync_docs(client: &Client, store: &mut Store, spec: &Github, opts: &Opts) -> Result<Report> {
    let mut report = Report::new(spec.id);
    let token = token();
    let mut keep = HashSet::new();
    let mut budget = opts.limit;
    for repo in spec.repos {
        let head = api(client, token.as_deref(), &format!("repos/{}/commits/HEAD", repo.repo))?;
        let commit = head["sha"].as_str().context("commit sha")?.to_owned();
        let date = head["commit"]["committer"]["date"].as_str().unwrap_or_default();
        store.set_meta(&format!("commit:{}", repo.repo), &format!("{commit} {date}"))?;
        // A nested prefix (llvm/docs/) is listed as its own subtree: the recursive tree of a monorepo is truncated.
        let subtree = repo.prefix.trim_end_matches('/');
        let (tree_ref, path_base) = if subtree.contains('/') { (format!("{commit}:{subtree}"), repo.prefix) } else { (commit.clone(), "") };
        let listing = api(client, token.as_deref(), &format!("repos/{}/git/trees/{tree_ref}?recursive=1", repo.repo))?;
        if listing["truncated"].as_bool().unwrap_or(false) {
            report.errors.push(format!("{}: git tree truncated", repo.repo));
        }
        let mut files: Vec<(String, String)> = listing["tree"]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| entry["type"] == "blob")
                    .filter_map(|entry| {
                        let path = format!("{path_base}{}", entry["path"].as_str()?);
                        wanted(repo, &path, entry["size"].as_u64().unwrap_or(0)).then(|| (path, entry["sha"].as_str().unwrap_or_default().to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        report.listed += files.len();
        if let Some(left) = budget {
            files.truncate(left);
            budget = Some(left.saturating_sub(files.len()));
        }
        let mut jobs = Vec::new();
        for (path, sha) in files {
            let url = map_url(repo.map, repo.repo, repo.prefix, &path, &commit);
            keep.insert(url.clone());
            if !opts.force && store.state(&url)?.is_some_and(|(rev, _)| rev == sha) {
                report.unchanged += 1;
            } else {
                jobs.push((url, path, sha));
            }
        }
        let mut failures = Vec::new();
        let mut write_error: Option<anyhow::Error> = None;
        pipeline(
            jobs,
            opts.threads,
            |(url, path, sha)| {
                let raw = client.get(&format!("https://raw.githubusercontent.com/{}/{commit}/{path}", repo.repo));
                Some((url, path, sha, raw))
            },
            |(url, path, sha, raw)| match raw {
                Ok(response) if response.status == 200 => {
                    let text = response.text();
                    let title = chunk::front_title(&text).unwrap_or_else(|| humanize(&path));
                    let chunks = chunk::markdown(&title, &text, 3000);
                    let origin = format!("https://github.com/{}/blob/{commit}/{path}", repo.repo);
                    let doc = DocInput { url: &url, title: &title, rev: &sha, kind: "file", origin: &origin, license: repo.license, bytes: text.len() };
                    match store.put(&doc, &chunks) {
                        Ok(()) => {
                            report.fetched += 1;
                            report.bytes += text.len();
                        }
                        Err(error) => {
                        write_error.get_or_insert(error);
                    }
                    }
                }
                Ok(response) => failures.push(format!("{path}: HTTP {}", response.status)),
                Err(error) => failures.push(format!("{path}: {error:#}")),
            },
        );
        if let Some(error) = write_error {
            return Err(error);
        }
        eprintln!("{}: {} done at {}, {} failed", spec.id, repo.repo, &commit[..12], failures.len());
        report.failed += failures.len();
        report.errors.extend(failures.into_iter().take(3));
    }
    if opts.limit.is_none() && report.failed == 0 {
        report.pruned = store.prune_except(&keep)?;
    }
    Ok(report)
}

pub fn sync_orgs(client: &Client, store: &mut Store, id: &str, orgs: &[&str], opts: &Opts) -> Result<Report> {
    let mut report = Report::new(id);
    let token = token();
    let mut keep = HashSet::new();
    let login = api(client, token.as_deref(), "user").ok().and_then(|user| user["login"].as_str().map(str::to_owned));
    for org in orgs {
        // The owner may be an organisation or a personal account; a personal account is listed through `user/repos` when it is
        // the authenticated one (private repositories included), else through its public listing.
        let kind = api(client, token.as_deref(), &format!("users/{org}"))?["type"].as_str().unwrap_or_default().to_owned();
        let listing = if kind == "Organization" {
            format!("orgs/{org}/repos?per_page=100&type=all")
        } else if login.as_deref() == Some(org) {
            "user/repos?per_page=100&affiliation=owner".to_owned()
        } else {
            format!("users/{org}/repos?per_page=100&type=owner")
        };
        for page in 1.. {
            let repos = api(client, token.as_deref(), &format!("{listing}&page={page}"))?;
            let Some(repos) = repos.as_array().filter(|repos| !repos.is_empty()) else { break };
            for repo in repos {
                report.listed += 1;
                let text = |key: &str| repo[key].as_str().unwrap_or_default().to_owned();
                let full_name = text("full_name");
                let url = format!("https://github.com/{full_name}");
                let rev = format!("{}#{}#{}", text("pushed_at"), repo["size"], text("description").len());
                keep.insert(url.clone());
                if !opts.force && store.state(&url)?.is_some_and(|(stored, _)| stored == rev) {
                    report.unchanged += 1;
                    continue;
                }
                let topics: Vec<&str> = repo["topics"].as_array().map(|topics| topics.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                let body = format!(
                    "# {full_name}\n\n{}\n\n- Visibility: {}\n- Language: {}\n- Topics: {}\n- License: {}\n- Default branch: {}\n- Last push: {}\n- Size (KB): {}\n- Fork: {}\n- Archived: {}\n- Source: {url}\n",
                    text("description"),
                    text("visibility"),
                    text("language"),
                    topics.join(", "),
                    repo["license"]["spdx_id"].as_str().unwrap_or("none"),
                    text("default_branch"),
                    text("pushed_at"),
                    repo["size"],
                    repo["fork"],
                    repo["archived"],
                );
                let chunks = chunk::markdown(&full_name, &body, 3000);
                store.put(&DocInput { url: &url, title: &full_name, rev: &rev, kind: "repo", origin: "", license: "metadata", bytes: body.len() }, &chunks)?;
                report.fetched += 1;
                report.bytes += body.len();
            }
        }
    }
    if opts.limit.is_none() {
        report.pruned = store.prune_except(&keep)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_published_urls() {
        let book = map_url(UrlMap::MdBook("https://doc.rust-lang.org/cargo"), "rust-lang/cargo", "src/doc/src/", "src/doc/src/reference/resolver.md", "c");
        assert_eq!(book, "https://doc.rust-lang.org/cargo/reference/resolver.html");
        let docs = map_url(UrlMap::GithubDocs, "github/docs", "content/", "content/actions/index.md", "c");
        assert_eq!(docs, "https://docs.github.com/en/actions");
        assert_eq!(map_url(UrlMap::Kotlin, "x", "docs/topics/", "docs/topics/coroutines-guide.md", "c"), "https://kotlinlang.org/docs/coroutines-guide.html");
    }
}
