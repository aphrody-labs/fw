//! std, core and alloc item documentation from the nightly `rust-docs-json` component.

use crate::chunk;
use crate::sources::{Opts, Report};
use crate::store::{DocInput, Store};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

const CRATES: &[&str] = &["std", "core", "alloc"];
const BASE: &str = "https://doc.rust-lang.org/nightly";

fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).output().with_context(|| format!("run {program}"))?;
    if !output.status.success() {
        bail!("{program} {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn json_dir() -> Result<PathBuf> {
    let sysroot = run("rustc", &["+nightly", "--print", "sysroot"])?;
    let dir = PathBuf::from(sysroot).join("share/doc/rust/json");
    if !dir.join("std.json").exists() {
        run("rustup", &["component", "add", "rust-docs-json", "--toolchain", "nightly"])?;
    }
    if !dir.join("std.json").exists() {
        bail!("{} has no std.json after installing rust-docs-json", dir.display());
    }
    Ok(dir)
}

fn kind_key(item: &Value) -> &str {
    item["inner"].as_object().and_then(|inner| inner.keys().next()).map_or("", String::as_str)
}

fn item_url(kind: &str, path: &[String]) -> Option<String> {
    let (name, modules) = path.split_last()?;
    let prefix = match kind {
        "module" => return Some(format!("{BASE}/{}/index.html", path.join("/"))),
        "struct" => "struct",
        "enum" => "enum",
        "trait" => "trait",
        "function" => "fn",
        "macro" | "proc_macro" => "macro",
        "constant" => "constant",
        "static" => "static",
        "type_alias" | "typedef" => "type",
        "union" => "union",
        "primitive" => "primitive",
        "keyword" => "keyword",
        "trait_alias" => "traitalias",
        _ => return None,
    };
    Some(format!("{BASE}/{}/{prefix}.{name}.html", modules.join("/")))
}

fn member_anchor(item: &Value, in_trait: bool) -> Option<String> {
    let name = item["name"].as_str()?;
    Some(match kind_key(item) {
        "function" if in_trait && item["inner"]["function"]["has_body"] == false => format!("tymethod.{name}"),
        "function" => format!("method.{name}"),
        "assoc_const" => format!("associatedconstant.{name}"),
        "assoc_type" => format!("associatedtype.{name}"),
        "variant" => format!("variant.{name}"),
        _ => return None,
    })
}

struct Doc {
    url: String,
    title: String,
    text: String,
}

fn collect(json: &Value, crate_name: &str, out: &mut Vec<Doc>) {
    let Some(index) = json["index"].as_object() else { return };
    let paths = &json["paths"];
    let path_of = |id: &str| -> Option<(String, Vec<String>)> {
        let entry = paths.get(id)?;
        let path = entry["path"].as_array()?.iter().filter_map(|part| part.as_str().map(str::to_owned)).collect();
        Some((entry["kind"].as_str()?.to_owned(), path))
    };

    // member id -> (parent id or primitive name, parent is trait)
    let mut parent: HashMap<String, (String, bool)> = HashMap::new();
    for item in index.values() {
        match kind_key(item) {
            "impl" => {
                let inner = &item["inner"]["impl"];
                if !inner["trait"].is_null() {
                    continue;
                }
                let owner = inner["for"]["resolved_path"]["id"].as_u64().map(|id| id.to_string()).or_else(|| inner["for"]["primitive"].as_str().map(|name| format!("primitive:{name}")));
                if let (Some(owner), Some(items)) = (owner, inner["items"].as_array()) {
                    for member in items.iter().filter_map(Value::as_u64) {
                        parent.insert(member.to_string(), (owner.clone(), false));
                    }
                }
            }
            "trait" => {
                let id = item["id"].to_string();
                for member in item["inner"]["trait"]["items"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
                    parent.insert(member.to_string(), (id.clone(), true));
                }
            }
            "enum" => {
                let id = item["id"].to_string();
                for member in item["inner"]["enum"]["variants"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
                    parent.insert(member.to_string(), (id.clone(), false));
                }
            }
            _ => {}
        }
    }

    let mut seen = HashSet::new();
    for (id, item) in index {
        let Some(text) = item["docs"].as_str().filter(|docs| docs.len() >= 12) else { continue };
        if item["visibility"] != "public" && kind_key(item) != "variant" {
            continue;
        }
        let located = if let Some((kind, path)) = path_of(id) {
            item_url(&kind, &path).map(|url| (url, path.join("::")))
        } else if let Some((owner, in_trait)) = parent.get(id) {
            let owner_path = match owner.strip_prefix("primitive:") {
                Some(name) => Some(("primitive".to_owned(), vec![crate_name.to_owned(), name.to_owned()])),
                None => path_of(owner),
            };
            owner_path.and_then(|(kind, path)| {
                let base = item_url(&kind, &path)?;
                let anchor = member_anchor(item, *in_trait)?;
                Some((format!("{base}#{anchor}"), format!("{}::{}", path.join("::"), item["name"].as_str()?)))
            })
        } else {
            None
        };
        if let Some((url, title)) = located
            && seen.insert(url.clone())
        {
            out.push(Doc { url, title, text: text.to_owned() });
        }
    }
}

pub fn sync(store: &mut Store, opts: &Opts) -> Result<Report> {
    let mut report = Report::new("rust-std");
    let version = run("rustc", &["+nightly", "--version"])?;
    if !opts.force && opts.limit.is_none() && store.meta("rustc")?.as_deref() == Some(version.as_str()) {
        report.unchanged = store.stats()?.docs as usize;
        return Ok(report);
    }
    let dir = json_dir()?;
    let mut keep = HashSet::new();
    let mut remaining = opts.limit;
    for crate_name in CRATES {
        let bytes = std::fs::read(dir.join(format!("{crate_name}.json"))).with_context(|| format!("read {crate_name}.json"))?;
        let json: Value = serde_json::from_slice(&bytes)?;
        let format_version = json["format_version"].as_u64().unwrap_or(0);
        let mut docs = Vec::new();
        collect(&json, crate_name, &mut docs);
        drop(json);
        eprintln!("rust-std: {crate_name} format {format_version}, {} documented items", docs.len());
        report.listed += docs.len();
        for doc in docs {
            if remaining == Some(0) {
                break;
            }
            remaining = remaining.map(|left| left - 1);
            keep.insert(doc.url.clone());
            let chunks = chunk::markdown(&doc.title, &doc.text, 3000);
            store.put(&DocInput { url: &doc.url, title: &doc.title, rev: &version, kind: "item", origin: "", license: "MIT OR Apache-2.0", bytes: doc.text.len() }, &chunks)?;
            report.fetched += 1;
            report.bytes += doc.text.len();
        }
    }
    if opts.limit.is_none() {
        report.pruned = store.prune_except(&keep)?;
        store.set_meta("rustc", &version)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn urls_for_items_and_methods() {
        let path = vec!["std".to_owned(), "collections".to_owned(), "HashMap".to_owned()];
        assert_eq!(item_url("struct", &path).as_deref(), Some("https://doc.rust-lang.org/nightly/std/collections/struct.HashMap.html"));
        assert_eq!(member_anchor(&json!({"name":"insert","inner":{"function":{"has_body":true}}}), false).as_deref(), Some("method.insert"));
        assert_eq!(member_anchor(&json!({"name":"next","inner":{"function":{"has_body":false}}}), true).as_deref(), Some("tymethod.next"));
    }

    #[test]
    fn collects_inherent_method() {
        let json = json!({
            "index": {
                "1": {"id": 1, "name": "Vec", "visibility": "public", "docs": "A contiguous growable array type.", "inner": {"struct": {}}},
                "2": {"id": 2, "name": null, "visibility": "default", "docs": null, "inner": {"impl": {"trait": null, "for": {"resolved_path": {"id": 1}}, "items": [3]}}},
                "3": {"id": 3, "name": "push", "visibility": "public", "docs": "Appends an element to the back.", "inner": {"function": {"has_body": true}}}
            },
            "paths": {"1": {"crate_id": 0, "path": ["alloc", "vec", "Vec"], "kind": "struct"}}
        });
        let mut docs = Vec::new();
        collect(&json, "alloc", &mut docs);
        let urls: Vec<&str> = docs.iter().map(|doc| doc.url.as_str()).collect();
        assert!(urls.contains(&"https://doc.rust-lang.org/nightly/alloc/vec/struct.Vec.html#method.push"));
    }
}
