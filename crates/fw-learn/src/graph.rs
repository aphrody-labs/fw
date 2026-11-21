//! Code graph of each product through the `fw-graph` binary (tree-sitter Rust, oxc TS/JS, Markdown), stored under
//! `graphs/<repo>-<product>/`; the report's god nodes and communities become searchable items. Also the import of the
//! records the first engine wrote (observations, corpus pairs, doc chunks), so no earlier knowledge is lost.

use crate::docs::chunk_markdown;
use crate::products::{Product, Repo};
use crate::redact::redact;
use crate::store::{Item, Store};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn graph_program() -> PathBuf {
    let exe = format!("fw-graph{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe().ok().and_then(|path| path.parent().map(|dir| dir.join(&exe))).filter(|path| path.is_file()).unwrap_or_else(|| PathBuf::from(exe))
}

pub fn graph_dir(store: &Store, repo: &Repo, product: &Product) -> PathBuf {
    store.root.join("graphs").join(format!("{}-{}", repo.name, product.id))
}

fn run_with_timeout(program: &Path, args: &[String], limit: Duration) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot run `{}`; build it with `cargo build --release -p fw-rag --features codegraph --bin fw-graph` and keep it next to fw-learn", program.display()))?;
    let started = Instant::now();
    loop {
        if child.try_wait().context("poll fw-graph")?.is_some() {
            break;
        }
        if started.elapsed() > limit {
            child.kill().context("kill fw-graph after the time limit")?;
            bail!("fw-graph exceeded {}s", limit.as_secs());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let output = child.wait_with_output().context("collect fw-graph output")?;
    if !output.status.success() {
        bail!("fw-graph exited with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Builds the graph of one product directory; returns `(nodes, edges)`.
pub fn build(store: &Store, repo: &Repo, product: &Product, limit: Duration) -> Result<(u64, u64)> {
    let dir = graph_dir(store, repo, product);
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let report = dir.join("GRAPH_REPORT.md");
    let root = repo.path.join(&product.dir);
    let arg = |path: &Path| path.display().to_string();
    let out = run_with_timeout(&graph_program(), &["--state".into(), arg(&dir), "build".into(), arg(&root), "--json".into(), arg(&dir.join("graph.json")), "--report".into(), arg(&report)], limit)?;
    let summary: Value = serde_json::from_str(out.trim()).context("parse fw-graph build output")?;
    let (nodes, edges) = (summary["nodes"].as_u64().unwrap_or(0), summary["edges"].as_u64().unwrap_or(0));
    let markdown = std::fs::read_to_string(&report).unwrap_or_default();
    store.begin()?;
    for (index, (heading, body)) in chunk_markdown(&markdown).into_iter().enumerate() {
        let title = format!("code graph of {} > {heading}", product.id);
        store.upsert(&Item { kind: "graph", product: &product.id, repo: &repo.name, reference: &format!("{}#{index}", product.id), title: &title, body: &redact(&body), path: &product.dir, ts: 0 })?;
    }
    store.commit()?;
    Ok((nodes, edges))
}

/// Imports the first engine's JSONL shards once (`observations/`, `corpus/`, `docs/`); idempotent through the item key.
pub fn import_legacy(store: &Store) -> Result<usize> {
    if store.state("legacy-imported")?.is_some() {
        return Ok(0);
    }
    let mut imported = 0usize;
    store.begin()?;
    for (sub, kind) in [("observations", "observation"), ("corpus", "pair"), ("docs", "legacy-doc")] {
        for file in jsonl_files(&store.root.join(sub)) {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for line in text.lines() {
                let Ok(record) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                let field = |name: &str| record[name].as_str().unwrap_or_default().to_owned();
                let (title, body, reference) = match kind {
                    "observation" => (field("title"), format!("{} {}", field("narrative"), record["facts"]), field("sha256")),
                    "pair" => (field("prompt"), field("completion"), field("id")),
                    _ => (format!("{} {}", field("lib"), field("heading")), field("content"), field("sha256")),
                };
                let project = if kind == "observation" { field("project") } else { "legacy".to_owned() };
                if store.upsert(&Item { kind, product: &project, repo: "legacy", reference: &reference, title: &redact(&title), body: &redact(&body), path: &field("source_url"), ts: record["ts_unix"].as_i64().or(record["ts"].as_i64()).unwrap_or(0) })? {
                    imported += 1;
                }
            }
        }
    }
    store.commit()?;
    store.set_state("legacy-imported", &imported.to_string())?;
    Ok(imported)
}

fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(jsonl_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "jsonl") {
            files.push(path);
        }
    }
    files
}
