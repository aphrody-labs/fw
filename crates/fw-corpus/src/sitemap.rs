//! Sitemap-driven pages: plain HTML, DevSite `.md.txt` markdown, or Chrome-rendered DOM.

use crate::chunk;
use crate::html;
use crate::http::{Client, pipeline};
use crate::render;
use crate::sources::{Opts, Report, Sitemap};
use crate::store::{DocInput, Store};
use anyhow::Result;
use std::collections::HashSet;

pub struct Entry {
    pub loc: String,
    pub lastmod: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    /// Server-rendered HTML.
    Html,
    /// DevSite markdown at `<page>.md.txt`, HTML when absent.
    MarkdownText,
    /// JavaScript app rendered in headless Chrome.
    Chrome,
}

fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let start = block.find(&format!("<{name}>"))? + name.len() + 2;
    let end = block[start..].find(&format!("</{name}>"))? + start;
    Some(block[start..end].trim())
}

/// Expand a sitemap or sitemap index into page entries.
pub fn entries(client: &Client, url: &str) -> Result<Vec<Entry>> {
    let mut last_error = anyhow::anyhow!("{url}: empty sitemap");
    for _ in 0..4 {
        let response = client.get(url)?;
        let text = response.text();
        if response.status != 200 || !text.contains("<loc>") {
            last_error = anyhow::anyhow!("{url}: HTTP {} without <loc>", response.status);
            std::thread::sleep(std::time::Duration::from_secs(3));
            continue;
        }
        let mut out = Vec::new();
        if text.contains("<sitemapindex") {
            for block in text.split("<sitemap>").skip(1) {
                if let Some(child) = tag(block, "loc") {
                    out.extend(entries(client, &crate::unescape(child))?);
                }
            }
        } else {
            for block in text.split("<url>").skip(1) {
                if let Some(loc) = tag(block, "loc") {
                    out.push(Entry { loc: crate::unescape(loc), lastmod: tag(block, "lastmod").map(str::to_owned) });
                }
            }
        }
        return Ok(out);
    }
    Err(last_error)
}

fn humanize(url: &str, base: &str) -> String {
    url.strip_prefix(base).unwrap_or(url).trim_matches('/').replace(['-', '_'], " ")
}

struct Page {
    url: String,
    title: String,
    markdown: String,
    rev: String,
}

pub fn sync(client: &Client, store: &mut Store, spec: &Sitemap, opts: &Opts) -> Result<Report> {
    let mut report = Report::new(&spec.id);
    let mut all = entries(client, spec.sitemap)?;
    all.retain(|entry| !entry.loc.contains('?') && (spec.filter)(&entry.loc));
    let mut seen = HashSet::new();
    all.retain(|entry| seen.insert(entry.loc.clone()));
    report.listed = all.len();
    let complete = opts.limit.is_none();
    if let Some(limit) = opts.limit {
        all.truncate(limit);
    }
    let keep: HashSet<String> = all.iter().map(|entry| entry.loc.clone()).collect();

    let mut jobs = Vec::new();
    for entry in all {
        let stored = store.state(&entry.loc)?;
        let fresh = !opts.force && stored.as_ref().is_some_and(|(rev, _)| entry.lastmod.as_deref() == Some(rev.as_str()));
        if fresh {
            report.unchanged += 1;
        } else if !client.allowed(&entry.loc) {
            report.blocked_by_robots += 1;
        } else {
            jobs.push((entry, stored.map(|(rev, _)| rev)));
        }
    }

    let chrome = render::chrome_binary();
    let threads = if spec.fetch == Fetch::Chrome { opts.threads.min(4) } else { opts.threads };
    let mut touched = Vec::new();
    let mut failures = Vec::new();
    let mut pages = Vec::new();
    pipeline(
        jobs,
        threads,
        |(entry, stored_rev)| {
            let outcome = fetch_page(client, spec, &chrome, &entry, stored_rev.as_deref());
            Some((entry.loc, outcome))
        },
        |(loc, outcome)| match outcome {
            Ok(Some(page)) => pages.push(page),
            Ok(None) => touched.push(loc),
            Err(error) => failures.push(format!("{error:#}")),
        },
    );
    for loc in &touched {
        store.touch(loc)?;
        report.unchanged += 1;
    }
    for page in pages {
        let title = if page.title.is_empty() { humanize(&page.url, spec.base) } else { page.title.clone() };
        let chunks = chunk::markdown(&title, &page.markdown, 3000);
        let doc = DocInput { url: &page.url, title: &title, rev: &page.rev, kind: "page", origin: "", license: spec.license, bytes: page.markdown.len() };
        store.put(&doc, &chunks)?;
        report.fetched += 1;
        report.bytes += page.markdown.len();
    }
    report.failed = failures.len();
    report.errors = failures.into_iter().take(5).collect();
    if complete && report.failed == 0 {
        report.pruned = store.prune_except(&keep)?;
    }
    Ok(report)
}

/// `Ok(None)` means the server confirmed the stored copy (304).
fn fetch_page(client: &Client, spec: &Sitemap, chrome: &str, entry: &Entry, stored_rev: Option<&str>) -> Result<Option<Page>> {
    let validator = stored_rev.filter(|rev| rev.starts_with("etag:") || rev.starts_with("lm:"));
    let make_rev = |validator: Option<String>| entry.lastmod.clone().or(validator).unwrap_or_else(|| crate::iso(crate::now_unix()));
    if spec.fetch == Fetch::Chrome {
        let dom = render::dump_dom(chrome, &entry.loc, 20_000)?;
        let (title, markdown) = html::to_markdown(&dom)?;
        return Ok(Some(Page { url: entry.loc.clone(), title, markdown, rev: make_rev(None) }));
    }
    if spec.fetch == Fetch::MarkdownText {
        let response = client.get_conditional(&format!("{}.md.txt", entry.loc.trim_end_matches('/')), validator)?;
        match response.status {
            304 => return Ok(None),
            200 => {
                let markdown = response.text();
                let title = chunk::front_title(&markdown).unwrap_or_default();
                return Ok(Some(Page { url: entry.loc.clone(), title, markdown, rev: make_rev(response.validator()) }));
            }
            _ => {}
        }
    }
    let response = client.get_conditional(&entry.loc, validator)?;
    match response.status {
        304 => Ok(None),
        200 => {
            let (title, markdown) = html::to_markdown(&response.text())?;
            Ok(Some(Page { url: entry.loc.clone(), title, markdown, rev: make_rev(response.validator()) }))
        }
        status => anyhow::bail!("{}: HTTP {status}", entry.loc),
    }
}
