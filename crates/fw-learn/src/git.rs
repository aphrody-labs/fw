//! Git history harvest: every commit of the revision becomes one `commit` item (subject, body, touched areas) and one
//! `commit_area` row per touched unit, so the distiller can count fixes, hot spots and co-changes without reading git again.

use crate::products::{AreaIndex, Product, Repo, git, product_of};
use crate::redact::redact;
use crate::store::{Item, Store};
use anyhow::{Context, Result};
use rusqlite::params;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

const FIX_WORDS: &[&str] = &["fix", "revert", "regress", "broken", "bug", "crash", "repair", "hotfix"];
const MAX_FILES_PER_COMMIT: usize = 400;
const MAX_BODY_CHARS: usize = 1500;
/// Commits made by `fw learn publish` are not learned from, or each publish would change what the next one writes.
pub const PUBLISH_SUBJECT: &str = "learn: refresh the generated rules";

pub fn is_fix(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|word| FIX_WORDS.iter().any(|fix| word.starts_with(fix)))
}

/// Harvests the commits between the last harvested head and the revision; returns the number of commits stored.
pub fn harvest(store: &Store, repo: &Repo, products: &[Product], areas: &AreaIndex, max_commits: usize) -> Result<usize> {
    let state_key = format!("git:{}", repo.name);
    let previous = store.state(&state_key)?.filter(|sha| git(&repo.path, &["merge-base", "--is-ancestor", sha, &repo.head]).is_ok());
    let range = previous.map_or_else(|| repo.head.clone(), |sha| format!("{sha}..{}", repo.head));
    let limit = format!("--max-count={max_commits}");
    let mut child = Command::new("git")
        .arg("-C")
        .arg(&repo.path)
        .args(["log", &range, "--no-merges", "--no-renames", &limit, "--name-only", "--pretty=format:%x1e%H%x1f%ct%x1f%s%x1f%b%x1d"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("spawn git log")?;
    let mut reader = BufReader::new(child.stdout.take().context("git log stdout")?);
    let (mut stored, mut seen) = (0usize, 0usize);
    let mut record = Vec::new();
    store.begin()?;
    while reader.read_until(0x1e, &mut record).context("read git log")? > 0 {
        if record.last() == Some(&0x1e) {
            record.pop();
        }
        if !record.is_empty() {
            stored += store_record(store, repo, products, areas, &String::from_utf8_lossy(&record))?;
        }
        record.clear();
        seen += 1;
        if seen % 5000 == 0 {
            store.commit()?;
            store.begin()?;
        }
    }
    store.commit()?;
    child.wait().context("wait for git log")?;
    store.set_state(&state_key, &repo.head)?;
    Ok(stored)
}

fn store_record(store: &Store, repo: &Repo, products: &[Product], areas: &AreaIndex, record: &str) -> Result<usize> {
    let Some((header, files)) = record.split_once('\x1d') else {
        return Ok(0);
    };
    let mut fields = header.splitn(4, '\x1f');
    let (Some(sha), Some(ts), Some(subject), Some(body)) = (fields.next(), fields.next(), fields.next(), fields.next()) else {
        return Ok(0);
    };
    let ts: i64 = ts.parse().unwrap_or(0);
    let paths: Vec<&str> = files.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let mut by_product: BTreeMap<&str, usize> = BTreeMap::new();
    let mut touched: BTreeMap<(String, String), ()> = BTreeMap::new();
    let mut at_root: BTreeMap<&str, usize> = BTreeMap::new();
    for path in paths.iter().take(MAX_FILES_PER_COMMIT) {
        let path = areas.resolve(path);
        let area = areas.area_of(&path);
        let product = product_of(products, &path);
        if area == "(root)" || products.iter().any(|candidate| candidate.dir == area) {
            *at_root.entry(product.id.as_str()).or_default() += 1;
            continue;
        }
        if !areas.is_live(&area) {
            continue;
        }
        *by_product.entry(product.id.as_str()).or_default() += 1;
        touched.insert((product.id.clone(), area), ());
    }
    let product = by_product.iter().chain(at_root.iter()).max_by_key(|(_, count)| **count).map_or("history", |(id, _)| id);
    if subject.starts_with(PUBLISH_SUBJECT) {
        return Ok(0);
    }
    let subject = redact(subject.trim());
    let body: String = redact(body.trim()).chars().take(MAX_BODY_CHARS).collect();
    let listing = paths.iter().take(40).copied().collect::<Vec<_>>().join(" ");
    let fix = i64::from(is_fix(&subject));
    let inserted = store.upsert(&Item { kind: "commit", product, repo: &repo.name, reference: sha, title: &subject, body: &format!("{body}\n{listing}"), path: "", ts })?;
    if inserted {
        store.conn.execute("DELETE FROM commit_area WHERE repo = ?1 AND sha = ?2", params![repo.name, sha])?;
        for (product, area) in touched.keys() {
            store.conn.execute("INSERT INTO commit_area(sha, repo, product, area, ts, fix) VALUES (?1,?2,?3,?4,?5,?6)", params![sha, repo.name, product, area, ts, fix])?;
        }
    }
    Ok(usize::from(inserted))
}
