//! Text and MDX renderings of the JSON results (`op` field), so one engine feeds a terminal and a docs site.

use crate::awesome::{mdx_escape, thousands};
use crate::model::Record;
use serde_json::Value;
use std::fmt::Write;

fn records(v: &Value, key: &str) -> Vec<Record> {
    v[key].as_array().map(|a| a.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect()).unwrap_or_default()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

fn dash(x: &str) -> &str {
    if x.is_empty() { "-" } else { x }
}

pub fn text_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut w: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |cells: Vec<&str>| cells.iter().enumerate().map(|(i, c)| format!("{c:<width$}", width = w[i])).collect::<Vec<_>>().join("  ").trim_end().to_owned();
    let mut out = line(headers.to_vec()) + "\n";
    for r in rows {
        out.push_str(&line(r.iter().map(String::as_str).collect()));
        out.push('\n');
    }
    out
}

pub fn mdx_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = format!("| {} |\n|{}|\n", headers.join(" | "), headers.iter().map(|_| "---").collect::<Vec<_>>().join("|"));
    for r in rows {
        let _ = writeln!(out, "| {} |", r.iter().map(|c| mdx_escape(c)).collect::<Vec<_>>().join(" | "));
    }
    out
}

fn record_rows(recs: &[Record]) -> Vec<Vec<String>> {
    recs.iter()
        .enumerate()
        .map(|(i, r)| {
            vec![
                (i + 1).to_string(),
                r.name.clone(),
                r.version.clone(),
                format!("{:.2}", r.score),
                thousands(r.recent_downloads),
                thousands(u64::from(r.dependents)),
                r.last_release.clone(),
                dash(&r.msrv).to_owned(),
                dash(&r.license).to_owned(),
            ]
        })
        .collect()
}

const REC_HEADERS: [&str; 9] = ["#", "crate", "version", "score", "downloads 90d", "dependents", "last release", "msrv", "license"];

fn info_pairs(r: &Record, rank: &Value) -> Vec<(&'static str, String)> {
    let mut p = vec![
        ("crate", format!("{} {}", r.name, r.version)),
        ("description", r.description.clone()),
        ("score", format!("{:.2} (rank {} of {})", r.score, rank["overall"], rank["of"])),
        ("downloads", format!("{} total, {} in the 90 days to the dump", thousands(r.downloads), thousands(r.recent_downloads))),
        ("dependents", thousands(u64::from(r.dependents))),
        ("releases", format!("{} versions, {} in the last 365 days, latest {}", r.versions, r.releases_365d, r.last_release)),
        ("created", r.created.clone()),
        ("owners", r.owners.to_string()),
        ("license", dash(&r.license).to_owned()),
        ("msrv", dash(&r.msrv).to_owned()),
        ("edition", dash(&r.edition).to_owned()),
        ("repository", dash(&r.repository).to_owned()),
        ("categories", r.categories.join(", ")),
        ("keywords", r.keywords.join(", ")),
        (
            "components",
            format!(
                "downloads {} dependents {} recency {} maintenance {} license {} msrv {} security {}",
                r.components.downloads, r.components.dependents, r.components.recency, r.components.maintenance, r.components.license, r.components.msrv, r.components.security
            ),
        ),
    ];
    if let Some(cats) = rank["categories"].as_object() {
        for (slug, pos) in cats {
            p.push(("category rank", format!("#{pos} in {slug}")));
        }
    }
    if !r.advisories.is_empty() {
        p.push(("advisories", format!("open: {}; informational: {}", r.advisories.open.join(" "), r.advisories.informational.join(" "))));
    }
    p
}

fn live_line(l: &Value) -> Option<String> {
    if l.is_null() {
        return None;
    }
    if l["ok"].as_bool() == Some(true) {
        Some(format!("{} (sha256 {}, {} direct dependencies, {} versions in the sparse index)", s(l, "latest"), s(l, "sha256"), l["depsCount"], l["totalVersions"]))
    } else {
        Some(format!("unavailable: {}", s(l, "error")))
    }
}

pub fn text(v: &Value) -> String {
    let date = s(v, "dump_date");
    match s(v, "op") {
        "search" | "rank" => {
            let recs = records(v, "results");
            format!("dump {date}, {} crates\n{}", recs.len(), text_table(&REC_HEADERS, &record_rows(&recs)))
        }
        "info" => {
            let Ok(r) = serde_json::from_value::<Record>(v["crate"].clone()) else { return String::new() };
            let mut out = format!("dump {date}\n");
            for (k, val) in info_pairs(&r, &v["rank"]) {
                let _ = writeln!(out, "{k:<14}{val}");
            }
            if let Some(l) = live_line(&v["live"]) {
                let _ = writeln!(out, "{:<14}{l}", "live latest");
            }
            out
        }
        "compare" => {
            let recs = records(v, "crates");
            let mut out = format!("dump {date}\n{}", text_table(&REC_HEADERS, &record_rows(&recs)));
            if let Some(w) = v["winners"].as_object() {
                for (k, n) in w {
                    let _ = writeln!(out, "best {k}: {}", n.as_str().unwrap_or("-"));
                }
            }
            out
        }
        "awesome" if v["entries"].is_array() => {
            let recs = records(v, "entries");
            format!("{} (dump {date})\n{}", s(&v["category"], "name"), text_table(&REC_HEADERS, &record_rows(&recs)))
        }
        "awesome" => {
            let rows: Vec<Vec<String>> = v["categories"]
                .as_array()
                .map(|a| a.iter().map(|c| vec![s(c, "slug").to_owned(), c["entries"].to_string(), c["top"].as_array().map(|t| t.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()]).collect())
                .unwrap_or_default();
            format!("{} categories (dump {date})\n{}", rows.len(), text_table(&["category", "entries", "top 3"], &rows))
        }
        "stack" => stack_text(v),
        "bench" => crate::bench::text(v),
        _ => serde_json::to_string_pretty(v).unwrap_or_default(),
    }
}

fn stack_rows(v: &Value) -> Vec<Vec<String>> {
    v["roles"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| vec![s(r, "title").to_owned(), format!("{} {}", s(r, "crate"), s(r, "version")), format!("{:.2}", r["score"].as_f64().unwrap_or(0.0)), s(r, "reason").to_owned(), r["alternatives"].as_array().map(|x| x.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()])
                .collect()
        })
        .unwrap_or_default()
}

fn stack_text(v: &Value) -> String {
    let mut out = format!("goal: {}\nprofile: {} (dump {})\n", s(v, "goal"), s(v, "profile"), s(v, "dump_date"));
    out.push_str(&text_table(&["role", "crate", "score", "why", "alternatives"], &stack_rows(v)));
    if let Some(files) = v["files"].as_object() {
        let _ = writeln!(out, "files: {}", files.keys().cloned().collect::<Vec<_>>().join(", "));
    }
    out
}

pub fn mdx(v: &Value) -> String {
    let date = s(v, "dump_date");
    match s(v, "op") {
        "search" | "rank" => {
            let recs = records(v, "results");
            format!("---\ntitle: \"Crates\"\ndumpDate: {date}\n---\n\n{}", mdx_table(&REC_HEADERS, &record_rows(&recs)))
        }
        "info" => {
            let Ok(r) = serde_json::from_value::<Record>(v["crate"].clone()) else { return String::new() };
            let mut out = format!("---\ntitle: {}\ndumpDate: {date}\n---\n\n# {} {}\n\n", serde_json::to_string(&r.name).unwrap_or_default(), mdx_escape(&r.name), mdx_escape(&r.version));
            let rows: Vec<Vec<String>> = info_pairs(&r, &v["rank"]).into_iter().map(|(k, val)| vec![k.to_owned(), val]).collect();
            out.push_str(&mdx_table(&["field", "value"], &rows));
            if let Some(l) = live_line(&v["live"]) {
                let _ = writeln!(out, "\nLive sparse index: {}", mdx_escape(&l));
            }
            out
        }
        "compare" => {
            let recs = records(v, "crates");
            let names: Vec<String> = recs.iter().map(|r| r.name.clone()).collect();
            let mut out = format!("---\ntitle: {}\ndumpDate: {date}\n---\n\n# {}\n\n", serde_json::to_string(&format!("Compare: {}", names.join(" vs "))).unwrap_or_default(), mdx_escape(&names.join(" vs ")));
            out.push_str(&mdx_table(&REC_HEADERS, &record_rows(&recs)));
            if let Some(w) = v["winners"].as_object() {
                out.push('\n');
                for (k, n) in w {
                    let _ = writeln!(out, "- best {}: {}", mdx_escape(k), mdx_escape(n.as_str().unwrap_or("-")));
                }
            }
            out
        }
        "awesome" if v["entries"].is_array() => crate::awesome::list_mdx(v),
        "awesome" => {
            let rows: Vec<Vec<String>> = v["categories"]
                .as_array()
                .map(|a| a.iter().map(|c| vec![s(c, "name").to_owned(), c["entries"].to_string(), c["top"].as_array().map(|t| t.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default()]).collect())
                .unwrap_or_default();
            format!("---\ntitle: \"Best Rust crates by category\"\ndumpDate: {date}\n---\n\n{}", mdx_table(&["category", "entries", "top 3"], &rows))
        }
        "stack" => {
            let mut out = format!("---\ntitle: {}\ndumpDate: {date}\n---\n\n# Stack for: {}\n\nProfile `{}`.\n\n", serde_json::to_string(&format!("Stack: {}", s(v, "goal"))).unwrap_or_default(), mdx_escape(s(v, "goal")), s(v, "profile"));
            out.push_str(&mdx_table(&["role", "crate", "score", "why", "alternatives"], &stack_rows(v)));
            out
        }
        "bench" => crate::bench::mdx(v),
        _ => format!("```json\n{}\n```\n", serde_json::to_string_pretty(v).unwrap_or_default()),
    }
}
