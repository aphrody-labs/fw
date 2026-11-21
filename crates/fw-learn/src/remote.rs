//! Live sources: the release manifest served by the release host, and CI runs, issues and pull requests of the repository
//! through the `gh` CLI. A missing `gh` or an offline host is reported by the caller, never fatal to the other pillars.

use crate::products::Repo;
use crate::redact::redact;
use crate::store::{Item, Store};
use anyhow::{Context, Result, bail};
use rusqlite::params;
use serde_json::Value;
use std::process::Command;
use std::time::Duration;

pub const DEFAULT_RELEASES: &str = "https://releases.aphrody.com/manifest.json";

fn gh(args: &[&str]) -> Result<Value> {
    let output = Command::new("gh").args(args).output().context("run gh (install the GitHub CLI and log in)")?;
    if !output.status.success() {
        bail!("gh {}: {}", args.first().copied().unwrap_or(""), String::from_utf8_lossy(&output.stderr).trim());
    }
    serde_json::from_slice(&output.stdout).context("parse gh output")
}

fn epoch(iso: &str) -> i64 {
    // 2026-11-21T05:27:44Z: days from civil date, so no date crate is needed.
    let number = |range: std::ops::Range<usize>| iso.get(range).and_then(|part| part.parse::<i64>().ok()).unwrap_or(0);
    let (year, month, day) = (number(0..4), number(5..7), number(8..10));
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let year_of_era = shifted - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400 + number(11..13) * 3600 + number(14..16) * 60 + number(17..19)
}

pub fn parse_epoch(iso: &str) -> i64 {
    epoch(iso)
}

/// One `release` item per product, channel and version of the manifest.
pub fn harvest_releases(store: &Store, url: &str) -> Result<usize> {
    let body = ureq::get(url).config().timeout_global(Some(Duration::from_secs(20))).build().call().with_context(|| format!("fetch {url}"))?.body_mut().read_to_string().context("read release manifest")?;
    let manifest: Value = serde_json::from_str(&body).context("parse release manifest")?;
    let mut stored = 0usize;
    store.begin()?;
    for (product, channels) in manifest["products"].as_object().into_iter().flatten() {
        for (channel, release) in channels.as_object().into_iter().flatten() {
            let version = release["version"].as_str().unwrap_or_default();
            let reference = format!("{product}:{channel}:{version}");
            let ts = release["date"].as_str().map_or(0, epoch);
            let tag = release["tag"].as_str().unwrap_or_default();
            if store.upsert(&Item { kind: "release", product, repo: "releases", reference: &reference, title: version, body: &format!("{product} {channel} {version} tag {tag}"), path: channel, ts })? {
                stored += 1;
            }
        }
    }
    store.commit()?;
    Ok(stored)
}

/// The latest 100 workflow runs; the failed jobs of new failed runs (at most 15 per cycle) become the item body.
pub fn harvest_ci(store: &Store, repo: &Repo, product: &str) -> Result<usize> {
    let slug = repo.slug.as_deref().context("the origin remote is not a GitHub repository")?;
    let runs = gh(&["run", "list", "-R", slug, "-L", "100", "--json", "databaseId,workflowName,conclusion,status,createdAt,event,displayTitle,headBranch"])?;
    let mut stored = 0usize;
    let mut detailed = 0usize;
    for run in runs.as_array().into_iter().flatten() {
        let id = run["databaseId"].as_i64().unwrap_or(0);
        let workflow = run["workflowName"].as_str().unwrap_or_default();
        let conclusion = run["conclusion"].as_str().filter(|value| !value.is_empty()).unwrap_or_else(|| run["status"].as_str().unwrap_or("unknown"));
        let ts = run["createdAt"].as_str().map_or(0, epoch);
        let known: Option<String> = store.conn.query_row("SELECT conclusion FROM ci_run WHERE repo = ?1 AND id = ?2", params![repo.name, id], |row| row.get(0)).ok();
        if known.as_deref() == Some(conclusion) {
            continue;
        }
        store.conn.execute(
            "INSERT OR REPLACE INTO ci_run(repo, id, workflow, conclusion, branch, event, ts) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![repo.name, id, workflow, conclusion, run["headBranch"].as_str().unwrap_or_default(), run["event"].as_str().unwrap_or_default(), ts],
        )?;
        let mut body = String::new();
        if conclusion == "failure" && detailed < 15 {
            detailed += 1;
            body = failed_jobs(slug, id).unwrap_or_default();
        }
        let title = redact(&format!("{workflow} {conclusion}: {}", run["displayTitle"].as_str().unwrap_or_default()));
        store.upsert(&Item { kind: "ci", product, repo: &repo.name, reference: &id.to_string(), title: &title, body: &redact(&body), path: workflow, ts })?;
        stored += 1;
    }
    Ok(stored)
}

fn failed_jobs(slug: &str, id: i64) -> Result<String> {
    let view = gh(&["run", "view", &id.to_string(), "-R", slug, "--json", "jobs"])?;
    let mut lines = Vec::new();
    for job in view["jobs"].as_array().into_iter().flatten().filter(|job| job["conclusion"] == "failure") {
        let steps: Vec<&str> = job["steps"].as_array().into_iter().flatten().filter(|step| step["conclusion"] == "failure").filter_map(|step| step["name"].as_str()).collect();
        lines.push(format!("job {} failed at {}", job["name"].as_str().unwrap_or_default(), steps.join(", ")));
    }
    Ok(lines.join("\n"))
}

/// Issues and pull requests, newest update first, up to `max` of them.
pub fn harvest_issues(store: &Store, repo: &Repo, product: &str, max: usize) -> Result<usize> {
    let slug = repo.slug.as_deref().context("the origin remote is not a GitHub repository")?;
    let mut stored = 0usize;
    store.begin()?;
    for page in 1..=max.div_ceil(100) {
        let endpoint = format!("repos/{slug}/issues?state=all&per_page=100&sort=updated&direction=desc&page={page}");
        let items = gh(&["api", &endpoint])?;
        let Some(items) = items.as_array().filter(|items| !items.is_empty()) else {
            break;
        };
        for issue in items {
            let number = issue["number"].as_i64().unwrap_or(0);
            let kind = if issue.get("pull_request").is_some() { "pr" } else { "issue" };
            let labels: Vec<&str> = issue["labels"].as_array().into_iter().flatten().filter_map(|label| label["name"].as_str()).collect();
            let body: String = redact(issue["body"].as_str().unwrap_or_default()).chars().take(1500).collect();
            let text = format!("state {} labels {}\n{body}", issue["state"].as_str().unwrap_or_default(), labels.join(","));
            if store.upsert(&Item { kind, product, repo: &repo.name, reference: &number.to_string(), title: &redact(issue["title"].as_str().unwrap_or_default()), body: &text, path: "", ts: issue["updated_at"].as_str().map_or(0, epoch) })? {
                stored += 1;
            }
        }
    }
    store.commit()?;
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::epoch;

    #[test]
    fn converts_iso_timestamps() {
        assert_eq!(epoch("1970-01-01T00:00:00Z"), 0);
        assert_eq!(epoch("2026-11-21T05:27:44Z"), 1_795_238_864);
    }
}
