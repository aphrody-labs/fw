//! Docs, rule sources and manifests read from the git tree: Markdown becomes heading chunks, the agent contract files
//! (AGENTS.md, CLAUDE.md, ...) also yield their imperative lines as rule sources, and every Cargo.toml, package.json and
//! pyproject.toml becomes a unit with its description and dependencies.

use crate::products::{Product, Repo, TreeEntry, UNIT_MANIFESTS, product_of, skipped};
use crate::redact::redact;
use crate::store::{Item, Store};
use anyhow::Result;
use rusqlite::params;

const DOC_LIMIT: usize = 400 * 1024;
const CHUNK_CHARS: usize = 1800;
const RULE_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", "REVIEW.md", "LIFETIMES_AND_STYLE.md", "CONTRIBUTING.md"];
const IMPERATIVES: &[&str] = &["never ", "always ", "must ", "do not ", "don't ", "avoid ", "prefer ", "no ai ", "only "];
pub const BLOCK_MARKER: &str = "<!-- learn:begin";

fn is_doc(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    (lower.ends_with(".md") || lower.ends_with(".mdx") || lower.ends_with("llms.txt")) && !skipped(path) && !lower.contains("changelog") && !lower.contains("/licenses/") && !lower.starts_with("license")
}

fn blob_state_key(repo: &Repo, path: &str) -> String {
    format!("blob:{}:{path}", repo.name)
}

/// Stores the doc chunks and rule sources of every Markdown file whose blob changed since the last harvest.
pub fn harvest_docs(store: &Store, repo: &Repo, products: &[Product], tree: &[TreeEntry]) -> Result<usize> {
    let mut known = store.conn.prepare("SELECT DISTINCT path FROM item WHERE repo = ?1 AND kind = 'doc'")?;
    let stored_paths: Vec<String> = known.query_map([&repo.name], |row| row.get(0))?.collect::<std::result::Result<_, _>>()?;
    let live: std::collections::HashSet<&str> = tree.iter().map(|entry| entry.path.as_str()).collect();
    for path in stored_paths.iter().filter(|path| !live.contains(path.as_str())) {
        store.conn.execute("DELETE FROM item WHERE repo = ?1 AND kind IN ('doc', 'rule-source') AND path = ?2", params![repo.name, path])?;
        store.conn.execute("DELETE FROM state WHERE key = ?1", [blob_state_key(repo, path)])?;
    }
    let mut changed: Vec<&TreeEntry> = Vec::new();
    for entry in tree.iter().filter(|entry| is_doc(&entry.path)) {
        if store.state(&blob_state_key(repo, &entry.path))?.as_deref() != Some(entry.sha.as_str()) {
            changed.push(entry);
        }
    }
    let mut stored = 0usize;
    for batch in changed.chunks(500) {
        let blobs = repo.blobs(&batch.iter().map(|entry| entry.sha.as_str()).collect::<Vec<_>>(), DOC_LIMIT)?;
        store.begin()?;
        for (entry, blob) in batch.iter().zip(blobs) {
            let text = String::from_utf8_lossy(&blob);
            let product = product_of(products, &entry.path);
            stored += store_doc(store, repo, product, &entry.path, &text)?;
            store.set_state(&blob_state_key(repo, &entry.path), &entry.sha)?;
        }
        store.commit()?;
    }
    Ok(stored)
}

fn store_doc(store: &Store, repo: &Repo, product: &Product, path: &str, text: &str) -> Result<usize> {
    store.conn.execute("DELETE FROM item WHERE repo = ?1 AND kind IN ('doc', 'rule-source') AND path = ?2", params![repo.name, path])?;
    let file = path.rsplit('/').next().unwrap_or(path);
    let mut stored = 0usize;
    for (index, (heading, body)) in chunk_markdown(text).into_iter().enumerate() {
        let reference = format!("{path}#{index}");
        let title = if heading.is_empty() { file.to_owned() } else { format!("{file} > {heading}") };
        if store.upsert(&Item { kind: "doc", product: &product.id, repo: &repo.name, reference: &reference, title: &title, body: &redact(&body), path, ts: 0 })? {
            stored += 1;
        }
    }
    let directory = path.rsplit_once('/').map_or("", |(directory, _)| directory);
    if RULE_FILES.contains(&file) && directory == product.dir {
        for (index, line) in rule_lines(text).into_iter().enumerate() {
            let reference = format!("{path}:{index}");
            store.upsert(&Item { kind: "rule-source", product: &product.id, repo: &repo.name, reference: &reference, title: &redact(&line), body: "", path, ts: 0 })?;
        }
    }
    Ok(stored)
}

/// Splits Markdown into `(heading, text)` chunks at headings, and at paragraph breaks above the size limit.
pub fn chunk_markdown(text: &str) -> Vec<(String, String)> {
    let text = strip_front_matter(text);
    let mut chunks = Vec::new();
    let mut heading = String::new();
    let mut buffer = String::new();
    let mut fenced = false;
    let flush = |heading: &str, buffer: &mut String, chunks: &mut Vec<(String, String)>| {
        if buffer.trim().len() >= 40 {
            chunks.push((heading.to_owned(), buffer.trim().to_owned()));
        }
        buffer.clear();
    };
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with('#') && line.trim_start_matches('#').starts_with(' ') {
            flush(&heading, &mut buffer, &mut chunks);
            heading = line.trim_start_matches('#').trim().to_owned();
            continue;
        }
        if !fenced && line.trim().is_empty() && buffer.len() >= CHUNK_CHARS {
            flush(&heading, &mut buffer, &mut chunks);
            continue;
        }
        buffer.push_str(line);
        buffer.push('\n');
    }
    flush(&heading, &mut buffer, &mut chunks);
    chunks
}

fn strip_front_matter(text: &str) -> &str {
    text.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---").map(|(_, after)| after)).unwrap_or(text)
}

/// Imperative bullet lines of an agent contract file, outside the generated learn blocks. A bullet continues on the
/// indented lines that follow it.
pub fn rule_lines(text: &str) -> Vec<String> {
    let mut bullets: Vec<String> = Vec::new();
    let mut generated = false;
    let mut open = false;
    for line in text.lines() {
        if line.contains(BLOCK_MARKER) {
            generated = true;
        }
        if generated {
            generated = !line.contains("<!-- learn:end");
            open = false;
            continue;
        }
        let trimmed = line.trim();
        let item = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.split_once(". ").filter(|(number, _)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())).map(|(_, rest)| rest));
        if let Some(item) = item.filter(|_| !line.starts_with(char::is_whitespace) || open) {
            bullets.push(item.to_owned());
            open = true;
        } else if open && line.starts_with(char::is_whitespace) && !trimmed.is_empty() {
            if let Some(last) = bullets.last_mut() {
                last.push(' ');
                last.push_str(trimmed);
            }
        } else {
            open = false;
        }
    }
    bullets
        .into_iter()
        .filter(|item| {
            let lower = item.to_ascii_lowercase();
            item.len() >= 24 && IMPERATIVES.iter().any(|word| lower.contains(word))
        })
        .map(|item| squeeze(&item, 300))
        .collect()
}


fn squeeze(text: &str, limit: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= limit { flat } else { format!("{}...", flat.chars().take(limit).collect::<String>()) }
}

/// Reads every unit manifest of the tree into the `unit` table and as searchable `unit` items.
pub fn harvest_units(store: &Store, repo: &Repo, products: &[Product], tree: &[TreeEntry]) -> Result<usize> {
    let manifests: Vec<&TreeEntry> = tree
        .iter()
        .filter(|entry| entry.path.rsplit('/').next().is_some_and(|file| UNIT_MANIFESTS.contains(&file)) && entry.path.contains('/') && !skipped(&entry.path))
        .collect();
    let blobs = repo.blobs(&manifests.iter().map(|entry| entry.sha.as_str()).collect::<Vec<_>>(), 1 << 20)?;
    store.conn.execute("DELETE FROM unit WHERE repo = ?1", [&repo.name])?;
    let mut stored = 0usize;
    store.begin()?;
    for (entry, blob) in manifests.iter().zip(blobs) {
        let (dir, file) = entry.path.rsplit_once('/').unwrap_or(("", &entry.path));
        let Some(unit) = parse_unit(file, &String::from_utf8_lossy(&blob)) else {
            continue;
        };
        let product = product_of(products, &entry.path);
        let deps = unit.deps.join(",");
        store.conn.execute(
            "INSERT OR REPLACE INTO unit(repo, product, dir, kind, name, version, description, deps) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![repo.name, product.id, dir, file, unit.name, unit.version, unit.description, deps],
        )?;
        let body = format!("{}\ndepends on: {deps}", unit.description);
        store.upsert(&Item { kind: "unit", product: &product.id, repo: &repo.name, reference: &entry.path, title: &unit.name, body: &redact(&body), path: dir, ts: 0 })?;
        stored += 1;
    }
    store.commit()?;
    Ok(stored)
}

pub struct UnitManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub deps: Vec<String>,
}

pub fn parse_unit(file: &str, text: &str) -> Option<UnitManifest> {
    match file {
        "package.json" => {
            let value: serde_json::Value = serde_json::from_str(text).ok()?;
            let name = value["name"].as_str()?.to_owned();
            let keys = |field: &str| value[field].as_object().map(|map| map.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
            let mut deps = keys("dependencies");
            deps.extend(keys("devDependencies"));
            Some(UnitManifest { name, version: value["version"].as_str().unwrap_or_default().to_owned(), description: value["description"].as_str().unwrap_or_default().to_owned(), deps })
        }
        _ => {
            let table: toml::Table = text.parse().ok()?;
            let package = table.get("package").or_else(|| table.get("project")).and_then(toml::Value::as_table)?;
            let field = |key: &str| package.get(key).and_then(toml::Value::as_str).unwrap_or_default().to_owned();
            let mut deps = Vec::new();
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(entries) = table.get(section).and_then(toml::Value::as_table) {
                    deps.extend(entries.keys().cloned());
                }
            }
            Some(UnitManifest { name: package.get("name").and_then(toml::Value::as_str)?.to_owned(), version: field("version"), description: field("description"), deps })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_at_headings_and_keeps_fenced_hashes() {
        let text = "# Title\nintro text that is long enough to be kept as one chunk of prose\n## Part\n```sh\n# not a heading\n``\nbody of the part that is also long enough to be kept\n";
        let chunks = chunk_markdown(text);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1].0, "Part");
        assert!(chunks[1].1.contains("# not a heading"));
    }

    #[test]
    fn picks_imperative_lines_outside_generated_blocks() {
        let text = "- Never add a branch to the shared checkout\n- plain description line of the crate\n<!-- learn:begin x -->\n- Never read this generated line again\n<!-- learn:end -->\n1. Always commit by path before pushing";
        assert_eq!(rule_lines(text), vec!["Never add a branch to the shared checkout", "Always commit by path before pushing"]);
    }

    #[test]
    fn reads_cargo_and_package_manifests() {
        let cargo = parse_unit("Cargo.toml", "[package]\nname = \"fw-x\"\nversion = \"1.0.0\"\ndescription = \"d\"\n[dependencies]\nfw-y = { path = \"../fw-y\" }\n").map(|unit| (unit.name, unit.deps));
        assert_eq!(cargo, Some(("fw-x".to_owned(), vec!["fw-y".to_owned()])));
        let json = parse_unit("package.json", "{\"name\":\"@a/b\",\"dependencies\":{\"c\":\"1\"}}").map(|unit| unit.deps);
        assert_eq!(json, Some(vec!["c".to_owned()]));
    }
}
