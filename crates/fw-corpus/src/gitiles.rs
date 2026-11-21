//! Gitiles hosts (chromium.googlesource.com, android.googlesource.com): repo inventory from the HTML
//! index (the `?format=TEXT` root listing is disallowed by robots.txt) and docs trees read file by file
//! through `?format=TEXT` (base64). Blob shas are the revision, so unchanged files cost one tree listing.

use crate::chunk;
use crate::http::{Client, pipeline};
use crate::sources::{Gitiles, Opts, Report, Walk};
use crate::store::{DocInput, Store};
use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::collections::HashSet;

pub struct Entry {
    pub is_tree: bool,
    pub sha: String,
    pub name: String,
}

pub struct Commit {
    pub sha: String,
    pub time: String,
    pub subject: String,
}

const SKIP_DIRS: &[&str] = &["node_modules", "testdata", "LayoutTests", "web_tests", ".git"];
const README_CAP: usize = 200_000;

pub fn repo_list(html: &str) -> Vec<(String, String)> {
    html.split("<a class=\"RepoList-item\" href=\"")
        .skip(1)
        .filter_map(|block| {
            let href = block.split('"').next()?;
            block.split_once("RepoList-itemName\">")?;
            let description = block.split_once("RepoList-itemDescription\">").map_or("", |(_, rest)| rest.split("</span>").next().unwrap_or(""));
            let repo = href.trim_matches('/').to_owned();
            Some((repo, crate::unescape(description).trim().to_owned()))
        })
        .collect()
}

fn text_url(host: &str, repo: &str, reference: &str, path: &str) -> String {
    format!("https://{host}/{repo}/+/{reference}/{path}?format=TEXT")
}

fn fetch_decoded(client: &Client, url: &str) -> Result<Option<Vec<u8>>> {
    if !client.allowed(url) {
        return Ok(None);
    }
    let response = client.get(url)?;
    if response.status != 200 {
        return Ok(None);
    }
    let compact: Vec<u8> = response.body.into_iter().filter(|byte| !byte.is_ascii_whitespace()).collect();
    Ok(Some(STANDARD.decode(compact)?))
}

pub fn tree(client: &Client, host: &str, repo: &str, reference: &str, path: &str) -> Result<Vec<Entry>> {
    let Some(bytes) = fetch_decoded(client, &text_url(host, repo, reference, path))? else { return Ok(Vec::new()) };
    Ok(String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| {
            let (meta, name) = line.split_once('\t')?;
            let mut parts = meta.split(' ');
            let (_mode, kind, sha) = (parts.next()?, parts.next()?, parts.next()?);
            Some(Entry { is_tree: kind == "tree", sha: sha.to_owned(), name: name.to_owned() })
        })
        .collect())
}

pub fn commit(client: &Client, host: &str, repo: &str) -> Result<Option<Commit>> {
    let url = format!("https://{host}/{repo}/+/HEAD?format=JSON");
    if !client.allowed(&url) {
        return Ok(None);
    }
    let response = client.get(&url)?;
    if response.status != 200 {
        return Ok(None);
    }
    let text = response.text();
    let json: Value = serde_json::from_str(text.trim_start_matches(")]}'").trim())?;
    let sha = json["commit"].as_str().unwrap_or_default().to_owned();
    if sha.is_empty() {
        return Ok(None);
    }
    Ok(Some(Commit {
        sha,
        time: json["committer"]["time"].as_str().unwrap_or_default().to_owned(),
        subject: json["message"].as_str().unwrap_or_default().lines().next().unwrap_or_default().to_owned(),
    }))
}

struct RepoDoc {
    url: String,
    rev: String,
    origin: String,
    title: String,
    markdown: String,
}

fn index_repo(client: &Client, host: &str, repo: &str, description: &str, stored: Option<&str>) -> Result<Option<RepoDoc>> {
    let head = commit(client, host, repo)?;
    let rev = format!("{}#{}", head.as_ref().map_or("empty", |head| head.sha.as_str()), description.len());
    if stored == Some(rev.as_str()) {
        return Ok(None);
    }
    let mut body = format!("# {repo}\n\n{description}\n\n- Source: https://{host}/{repo}\n");
    let mut origin = String::new();
    if let Some(head) = &head {
        body.push_str(&format!("- HEAD: {}\n- Last commit: {} - {}\n", head.sha, head.time, head.subject));
        let entries = tree(client, host, repo, &head.sha, "")?;
        let names: Vec<&str> = entries.iter().take(60).map(|entry| entry.name.as_str()).collect();
        body.push_str(&format!("- Top level: {}\n", names.join(", ")));
        if let Some(readme) = entries.iter().find(|entry| !entry.is_tree && entry.name.to_ascii_lowercase().starts_with("readme")) {
            let url = text_url(host, repo, &head.sha, &readme.name);
            if let Some(bytes) = fetch_decoded(client, &url)? {
                let text = String::from_utf8_lossy(&bytes[..bytes.len().min(README_CAP)]);
                body.push_str(&format!("\n## README\n\n{text}\n"));
                origin = format!("https://{host}/{repo}/+/{}/{}", head.sha, readme.name);
            }
        }
    }
    Ok(Some(RepoDoc { url: format!("https://{host}/{repo}"), rev, origin, title: repo.to_owned(), markdown: body }))
}

fn sync_inventory(client: &Client, store: &mut Store, spec: &Gitiles, opts: &Opts, report: &mut Report, keep: &mut HashSet<String>) -> Result<()> {
    let index = client.get(&format!("https://{}/", spec.host))?;
    let mut repos = repo_list(&index.text());
    report.listed += repos.len();
    if let Some(limit) = opts.limit {
        repos.truncate(limit);
    }
    let mut jobs = Vec::new();
    for (repo, description) in repos {
        keep.insert(format!("https://{}/{repo}", spec.host));
        let stored = store.state(&format!("https://{}/{repo}", spec.host))?.map(|(rev, _)| rev).filter(|_| !opts.force);
        jobs.push((repo, description, stored));
    }
    let mut failures = Vec::new();
    let mut write_error: Option<anyhow::Error> = None;
    let total = jobs.len();
    pipeline(
        jobs,
        opts.threads,
        |(repo, description, stored)| Some((repo.clone(), index_repo(client, spec.host, &repo, &description, stored.as_deref()))),
        |(repo, outcome)| match outcome {
            Ok(Some(doc)) => {
                let chunks = chunk::markdown(&doc.title, &doc.markdown, 3000);
                let input = DocInput { url: &doc.url, title: &doc.title, rev: &doc.rev, kind: "repo", origin: &doc.origin, license: spec.license, bytes: doc.markdown.len() };
                match store.put(&input, &chunks) {
                    Ok(()) => {
                        report.fetched += 1;
                        report.bytes += doc.markdown.len();
                    }
                    Err(error) => {
                        write_error.get_or_insert(error);
                    }
                }
            }
            Ok(None) => report.unchanged += 1,
            Err(error) => failures.push(format!("{repo}: {error:#}")),
        },
    );
    if let Some(error) = write_error {
        return Err(error);
    }
    eprintln!("{}: inventory {total} repos, {} failed", spec.id, failures.len());
    report.failed += failures.len();
    report.errors.extend(failures.into_iter().take(5));
    Ok(())
}

fn joined(base: &str, name: &str) -> String {
    if base.is_empty() { name.to_owned() } else { format!("{base}/{name}") }
}

fn wanted(walk: &Walk, name: &str) -> bool {
    if walk.readme_only {
        name.eq_ignore_ascii_case("README.md")
    } else {
        name.to_ascii_lowercase().ends_with(".md")
    }
}

/// Breadth-first listing, one parallel round per directory level. A directory that cannot be listed is reported and skipped,
/// so a rate-limited host costs part of a walk instead of the whole source.
fn list_files(client: &Client, host: &str, walk: &Walk, commit_sha: &str, threads: usize) -> (Vec<(String, String)>, Vec<String>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut level = vec![walk.root.to_owned()];
    for _ in 0..=walk.depth {
        if level.is_empty() {
            break;
        }
        let mut next = Vec::new();
        pipeline(
            std::mem::take(&mut level),
            threads,
            |dir| {
                let listing = tree(client, host, walk.repo, commit_sha, &dir);
                Some((dir, listing))
            },
            |(dir, listing)| match listing {
                Ok(entries) => {
                    for entry in entries {
                        let path = joined(&dir, &entry.name);
                        if entry.is_tree {
                            if !SKIP_DIRS.contains(&entry.name.as_str()) {
                                next.push(path);
                            }
                        } else if wanted(walk, &entry.name) {
                            files.push((path, entry.sha));
                        }
                    }
                }
                Err(error) => errors.push(format!("{dir}: {error:#}")),
            },
        );
        level = next;
    }
    (files, errors)
}

fn sync_walks(client: &Client, store: &mut Store, spec: &Gitiles, opts: &Opts, report: &mut Report, keep: &mut HashSet<String>) -> Result<()> {
    for walk in spec.walks {
        let Some(head) = commit(client, spec.host, walk.repo)? else {
            report.errors.push(format!("{}: no HEAD", walk.repo));
            continue;
        };
        let (mut files, listing_errors) = list_files(client, spec.host, walk, &head.sha, opts.threads);
        report.listed += files.len();
        report.failed += listing_errors.len();
        report.errors.extend(listing_errors.into_iter().take(3).map(|error| format!("listing {}/{}: {error}", walk.repo, walk.root)));
        if let Some(limit) = opts.limit {
            files.truncate(limit);
        }
        let mut jobs = Vec::new();
        for (path, sha) in files {
            let url = format!("https://{}/{}/+/HEAD/{path}", spec.host, walk.repo);
            keep.insert(url.clone());
            let fresh = !opts.force && store.state(&url)?.is_some_and(|(rev, _)| rev == sha);
            if fresh {
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
                let outcome = fetch_decoded(client, &text_url(spec.host, walk.repo, &head.sha, &path));
                Some((url, path, sha, outcome))
            },
            |(url, path, sha, outcome)| match outcome {
                Ok(Some(bytes)) => {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let title = chunk::front_title(&text).unwrap_or_else(|| format!("{}/{path}", walk.repo));
                    let chunks = chunk::markdown(&title, &text, 3000);
                    let origin = format!("https://{}/{}/+/{}/{path}", spec.host, walk.repo, head.sha);
                    let doc = DocInput { url: &url, title: &title, rev: &sha, kind: "file", origin: &origin, license: spec.license, bytes: text.len() };
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
                Ok(None) => failures.push(format!("{path}: unreadable")),
                Err(error) => failures.push(format!("{path}: {error:#}")),
            },
        );
        if let Some(error) = write_error {
            return Err(error);
        }
        eprintln!("{}: {}/{} done, {} failed", spec.id, walk.repo, walk.root, failures.len());
        report.failed += failures.len();
        report.errors.extend(failures.into_iter().take(3));
    }
    Ok(())
}

pub fn sync(client: &Client, store: &mut Store, spec: &Gitiles, opts: &Opts) -> Result<Report> {
    let mut report = Report::new(spec.id);
    let mut keep = HashSet::new();
    if spec.inventory {
        sync_inventory(client, store, spec, opts, &mut report, &mut keep)?;
    }
    sync_walks(client, store, spec, opts, &mut report, &mut keep)?;
    if opts.limit.is_none() && report.failed == 0 {
        report.pruned = store.prune_except(&keep)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_list() {
        let html = r#"<a class="RepoList-item" href="/angle/angle/"><span class="RepoList-itemName">angle/angle</span><span class="RepoList-itemDescription">Portable &amp; fast</span></a><a class="RepoList-item" href="/x/"><span class="RepoList-itemName">x</span><span class="RepoList-itemDescription"></span></a>"#;
        let repos = repo_list(html);
        assert_eq!(repos, vec![("angle/angle".to_owned(), "Portable & fast".to_owned()), ("x".to_owned(), String::new())]);
    }
}
