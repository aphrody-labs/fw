//! Per-category "awesome" lists: JSON for tools, MDX for people, both from the same ranked records.

use crate::model::{Category, Meta, Record, SCORE_VERSION};
use crate::store::{self, Dataset};
use serde_json::{Value, json};
use std::path::Path;

/// Entries per list.
pub const PER_CATEGORY: usize = 25;
/// A category needs at least this many eligible crates to get a list.
pub const MIN_ENTRIES: usize = 3;

pub fn file_stem(slug: &str) -> String {
    slug.replace("::", "--")
}

/// Escapes text for MDX: braces and angle brackets would start JSX expressions, pipes would break table cells.
pub fn mdx_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '{' | '}' | '<' | '>' | '|' | '\\' | '`' | '[' | ']' | '*' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn notes(r: &Record) -> String {
    let mut n = Vec::new();
    if !r.advisories.open.is_empty() {
        n.push(format!("open advisory {}", r.advisories.open.join(" ")));
    }
    if !r.advisories.informational.is_empty() {
        n.push(format!("informational {}", r.advisories.informational.join(" ")));
    }
    n.join("; ")
}

/// The list of one category from the dataset, `None` when the category does not exist or has too few eligible crates.
pub fn list(ds: &Dataset, slug: &str, limit: usize) -> Option<Value> {
    let cat = ds.category(slug)?;
    let entries: Vec<&Record> = ds.category_members(slug).take(limit).collect();
    if entries.len() < MIN_ENTRIES {
        return None;
    }
    Some(list_value(&ds.meta, cat, &entries))
}

fn list_value(meta: &Meta, cat: &Category, entries: &[&Record]) -> Value {
    json!({
        "category": { "slug": cat.slug, "name": cat.name, "description": cat.description, "crates": cat.crates },
        "dump_date": meta.dump_date,
        "score_version": meta.score_version,
        "entries": entries,
    })
}

/// Every awesome list the dataset supports, best-populated categories first.
pub fn all_lists(ds: &Dataset, limit: usize) -> Vec<Value> {
    ds.categories.iter().filter_map(|c| list(ds, &c.slug, limit)).collect()
}

pub fn list_mdx(v: &Value) -> String {
    let cat = &v["category"];
    let name = cat["name"].as_str().unwrap_or("");
    let slug = cat["slug"].as_str().unwrap_or("");
    let desc = cat["description"].as_str().unwrap_or("");
    let date = v["dump_date"].as_str().unwrap_or("");
    let mut s = format!(
        "---\ntitle: {}\ndescription: {}\ncategory: {}\ndumpDate: {}\nscoreVersion: {}\n---\n\n# Best Rust crates: {}\n\n{}\n\nRanked by the fw crates score ([method](../RANKING.md)) from the crates.io database dump of {}.\n\n",
        serde_json::to_string(&format!("Best Rust crates: {name}")).unwrap_or_default(),
        serde_json::to_string(desc).unwrap_or_default(),
        serde_json::to_string(slug).unwrap_or_default(),
        date,
        v["score_version"],
        mdx_escape(name),
        mdx_escape(desc),
        date
    );
    s.push_str("| # | Crate | Score | Downloads (90d) | Dependents | Last release | MSRV | License | Notes |\n|---|---|---|---|---|---|---|---|---|\n");
    let empty = Vec::new();
    let entries = v["entries"].as_array().unwrap_or(&empty);
    let recs: Vec<Record> = entries.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect();
    for (i, r) in recs.iter().enumerate() {
        s.push_str(&format!(
            "| {} | [{}](https://crates.io/crates/{}) {} | {:.2} | {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            mdx_escape(&r.name),
            r.name,
            mdx_escape(&r.version),
            r.score,
            thousands(r.recent_downloads),
            thousands(u64::from(r.dependents)),
            r.last_release,
            if r.msrv.is_empty() { "-".to_owned() } else { mdx_escape(&r.msrv) },
            if r.license.is_empty() { "-".to_owned() } else { mdx_escape(&r.license) },
            mdx_escape(&notes(r)),
        ));
    }
    s.push_str("\n## About each crate\n\n");
    for r in &recs {
        s.push_str(&format!("- **{}**: {}\n", mdx_escape(&r.name), mdx_escape(&r.description)));
    }
    s
}

/// Writes `awesome/<category>.json|mdx`, `awesome/index.json|mdx`, `index.jsonl`, `meta.json`, `categories.json`
/// and `RANKING.md` into `dir`. Every file is rewritten only when its content changes.
pub fn publish(dir: &Path, meta: &Meta, categories: &[Category], records: &[Record], index_top: usize) -> Result<usize, String> {
    let ds = Dataset::new(dir.to_path_buf(), meta.clone(), categories.to_vec(), records.to_vec());
    let lists = all_lists(&ds, PER_CATEGORY);

    // The index holds the overall top and every list entry, so lists and lookups work from the snapshot alone.
    let mut keep: std::collections::BTreeSet<String> = records.iter().take(index_top).map(|r| r.name.clone()).collect();
    let mut summary = Vec::new();
    for l in &lists {
        let slug = l["category"]["slug"].as_str().unwrap_or_default();
        let stem = file_stem(slug);
        store::write_text(&dir.join("awesome").join(format!("{stem}.json")), &(serde_json::to_string_pretty(l).map_err(|e| e.to_string())? + "\n"))?;
        store::write_text(&dir.join("awesome").join(format!("{stem}.mdx")), &list_mdx(l))?;
        let names: Vec<&str> = l["entries"].as_array().map(|a| a.iter().filter_map(|e| e["name"].as_str()).collect()).unwrap_or_default();
        keep.extend(names.iter().map(|n| (*n).to_owned()));
        summary.push(json!({
            "slug": slug,
            "name": l["category"]["name"],
            "file": stem,
            "entries": names.len(),
            "top": names.iter().take(3).collect::<Vec<_>>(),
        }));
    }
    store::write_text(
        &dir.join("awesome").join("index.json"),
        &(serde_json::to_string_pretty(&json!({ "dump_date": meta.dump_date, "score_version": SCORE_VERSION, "categories": summary })).map_err(|e| e.to_string())? + "\n"),
    )?;
    store::write_text(&dir.join("awesome").join("index.mdx"), &index_mdx(meta, &summary))?;

    let kept: Vec<&Record> = records.iter().filter(|r| keep.contains(&r.name)).collect();
    store::write_text(&dir.join(store::INDEX), &store::jsonl(&kept)?)?;
    store::write_text(&dir.join("meta.json"), &(serde_json::to_string_pretty(meta).map_err(|e| e.to_string())? + "\n"))?;
    store::write_text(&dir.join("categories.json"), &(serde_json::to_string_pretty(categories).map_err(|e| e.to_string())? + "\n"))?;
    store::write_text(&dir.join("RANKING.md"), &crate::rank::ranking_doc())?;
    Ok(lists.len())
}

fn index_mdx(meta: &Meta, summary: &[Value]) -> String {
    let mut s = format!(
        "---\ntitle: \"Best Rust crates by category\"\ndumpDate: {}\nscoreVersion: {}\n---\n\n# Best Rust crates by category\n\n{} lists from the crates.io database dump of {} ([ranking method](../RANKING.md)).\n\n| Category | Entries | Top 3 |\n|---|---|---|\n",
        meta.dump_date,
        SCORE_VERSION,
        summary.len(),
        meta.dump_date
    );
    for c in summary {
        let top: Vec<String> = c["top"].as_array().map(|a| a.iter().filter_map(|n| n.as_str()).map(mdx_escape).collect()).unwrap_or_default();
        s.push_str(&format!(
            "| [{}]({}.mdx) | {} | {} |\n",
            mdx_escape(c["name"].as_str().unwrap_or("")),
            c["file"].as_str().unwrap_or(""),
            c["entries"],
            top.join(", ")
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_and_thousands() {
        assert_eq!(mdx_escape("a|b {c} <d>"), "a\\|b \\{c\\} \\<d\\>");
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(file_stem("web-programming::http-server"), "web-programming--http-server");
    }
}
