//! `fw learn`: one engine that harvests every product of a repository (git history, manifests, docs, code graph, CI runs,
//! release manifest, issues and pull requests) into one SQLite FTS5 store, distills rules and publishes them to the files
//! agents read (AGENTS.md, CLAUDE.md, llms.txt).

pub mod distill;
pub mod docs;
pub mod git;
pub mod graph;
pub mod products;
pub mod publish;
pub mod redact;
pub mod remote;
pub mod store;

use anyhow::{Context, Result};
use products::{AreaIndex, Repo, discover_products};
use rusqlite::params;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use store::Store;

pub struct HarvestOptions {
    pub repos: Vec<PathBuf>,
    pub rev: Option<String>,
    pub max_commits: usize,
    pub max_issues: usize,
    pub releases_url: String,
    pub graphs: bool,
    pub remote: bool,
    pub graph_limit: Duration,
    pub graph_interval: i64,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_secs() as i64).unwrap_or(0)
}

fn timed<T>(report: &mut serde_json::Map<String, Value>, key: &str, work: impl FnOnce() -> Result<T>, describe: impl FnOnce(T) -> Value) {
    let started = Instant::now();
    let value = match work() {
        Ok(done) => describe(done),
        Err(error) => json!({ "error": format!("{error:#}") }),
    };
    let mut value = value;
    if let Some(object) = value.as_object_mut() {
        object.insert("ms".to_owned(), json!(started.elapsed().as_millis() as u64));
    }
    report.insert(key.to_owned(), value);
}

/// Runs every harvester on every repository; a failing pillar is reported under its key and does not stop the others.
pub fn harvest(store: &Store, options: &HarvestOptions) -> Result<Value> {
    let mut report = serde_json::Map::new();
    timed(&mut report, "legacy", || graph::import_legacy(store), |count| json!({ "imported": count }));
    let mut repos = Vec::new();
    for (index, path) in options.repos.iter().enumerate() {
        let fetched = if options.remote && options.rev.is_none() { Some(products::git(path, &["fetch", "--quiet", "origin", "main"]).map_err(|error| format!("{error:#}"))) } else { None };
        let repo = Repo::open(path, options.rev.as_deref()).with_context(|| format!("open {}", path.display()))?;
        let mut entry = serde_json::Map::new();
        let tree = repo.tree()?;
        let list = discover_products(&repo, &tree)?;
        let areas = AreaIndex::from_tree(&tree, &list);
        store.conn.execute("DELETE FROM product WHERE repo = ?1", [&repo.name])?;
        for product in &list {
            store.conn.execute("INSERT INTO product(repo, id, display, description, dir, docs_url) VALUES (?1,?2,?3,?4,?5,?6)", params![repo.name, product.id, product.display, product.description, product.dir, product.docs_url])?;
        }
        entry.insert("rev".to_owned(), json!(repo.short_head()));
        if let Some(Err(error)) = fetched {
            entry.insert("fetchError".to_owned(), json!(error));
        }
        entry.insert("products".to_owned(), json!(list.iter().map(|product| product.id.as_str()).collect::<Vec<_>>()));
        let limit = if index == 0 { options.max_commits } else { options.max_commits.min(5000) };
        timed(&mut entry, "commits", || git::harvest(store, &repo, &list, &areas, limit), |count| json!({ "stored": count }));
        timed(&mut entry, "docs", || docs::harvest_docs(store, &repo, &list, &tree), |count| json!({ "stored": count }));
        timed(&mut entry, "units", || docs::harvest_units(store, &repo, &list, &tree), |count| json!({ "stored": count }));
        let root_product = list.iter().find(|product| product.dir.is_empty()).unwrap_or(&list[0]);
        if options.remote && repo.slug.is_some() {
            timed(&mut entry, "ci", || remote::harvest_ci(store, &repo, &root_product.id), |count| json!({ "stored": count }));
            timed(&mut entry, "issues", || remote::harvest_issues(store, &repo, &root_product.id, options.max_issues), |count| json!({ "stored": count }));
        }
        if options.graphs {
            for product in &list {
                let key = format!("graph:{}:{}", repo.name, product.id);
                let due = store.state(&key)?.and_then(|state| state.split_once(' ').map(|(head, at)| (head.to_owned(), at.parse::<i64>().unwrap_or(0)))).is_none_or(|(head, at)| head != repo.head && now() - at >= options.graph_interval);
                if due {
                    let relative = graph::scan_root(store, &repo, product, &list)?;
                    timed(&mut entry, &format!("graph:{}", product.id), || graph::build(store, &repo, product, &relative, options.graph_limit), |(nodes, edges)| json!({ "nodes": nodes, "edges": edges, "scanned": relative }));
                    if entry.get(&format!("graph:{}", product.id)).is_some_and(|value| value.get("error").is_none()) {
                        store.set_state(&key, &format!("{} {}", repo.head, now()))?;
                    }
                }
            }
        }
        report.insert(repo.name.clone(), Value::Object(entry));
        repos.push(repo);
    }
    if options.remote {
        timed(&mut report, "releases", || remote::harvest_releases(store, &options.releases_url), |count| json!({ "stored": count }));
    }
    Ok(Value::Object(report))
}

/// Searches the store, then the code graphs and the `fw-corpus` stores next to it.
pub fn search(store: &Store, query: &str, limit: usize, kind: &str, product: &str) -> Result<Value> {
    let tokens: Vec<String> = query.split(|c: char| !c.is_alphanumeric()).filter(|token| !token.is_empty()).map(|token| format!("\"{token}\"")).collect();
    let mut hits = Vec::new();
    for joiner in [" AND ", " OR "] {
        if tokens.is_empty() || !hits.is_empty() {
            break;
        }
        hits = fts(store, &tokens.join(joiner), limit, kind, product)?;
    }
    Ok(json!({
        "ok": true,
        "query": query,
        "root": store.root.display().to_string(),
        "hits": hits,
        "codeGraphHits": code_graph_hits(store, query, limit),
        "corpusHits": corpus_hits(query, limit),
    }))
}

fn fts(store: &Store, expression: &str, limit: usize, kind: &str, product: &str) -> Result<Vec<Value>> {
    let mut statement = store.conn.prepare(
        "SELECT i.kind, i.product, i.repo, i.ref, i.title, snippet(item_fts, 1, '[', ']', '...', 24), i.path, i.ts, bm25(item_fts, 6.0, 1.0, 2.0) AS score
         FROM item_fts JOIN item i ON i.id = item_fts.rowid
         WHERE item_fts MATCH ?1 AND (?2 = '' OR i.kind = ?2) AND (?3 = '' OR i.product = ?3)
         ORDER BY score * CASE i.kind WHEN 'rule' THEN 1.6 WHEN 'rule-source' THEN 1.5 WHEN 'unit' THEN 1.2 WHEN 'doc' THEN 1.1 WHEN 'commit' THEN 0.85 ELSE 1.0 END
         LIMIT ?4",
    )?;
    let rows = statement.query_map(params![expression, kind, product, limit as i64], |row| {
        Ok(json!({
            "kind": row.get::<_, String>(0)?, "product": row.get::<_, String>(1)?, "repo": row.get::<_, String>(2)?, "ref": row.get::<_, String>(3)?,
            "title": row.get::<_, String>(4)?, "snippet": row.get::<_, String>(5)?, "path": row.get::<_, String>(6)?, "ts": row.get::<_, i64>(7)?,
            "score": (-row.get::<_, f64>(8)? * 1000.0).round() / 1000.0,
        }))
    })?;
    rows.collect::<std::result::Result<_, _>>().context("search the store")
}

fn sibling(name: &str) -> PathBuf {
    let exe = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe().ok().and_then(|path| path.parent().map(|dir| dir.join(&exe))).filter(|path| path.is_file()).unwrap_or_else(|| PathBuf::from(exe))
}

fn code_graph_hits(store: &Store, query: &str, limit: usize) -> Vec<Value> {
    let mut hits = Vec::new();
    let Ok(entries) = std::fs::read_dir(store.root.join("graphs")) else {
        return hits;
    };
    for entry in entries.flatten().filter(|entry| entry.path().join("GRAPH_REPORT.md").is_file()) {
        let state = entry.path().display().to_string();
        let Ok(output) = Command::new(graph::graph_program()).args(["--state", &state, "query", query, "--budget", "600"]).output() else {
            continue;
        };
        for line in String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| line.strip_prefix("NODE ")).take(limit.saturating_sub(hits.len())) {
            hits.push(json!({ "project": entry.file_name().to_string_lossy(), "node": line }));
        }
    }
    hits
}

fn corpus_hits(query: &str, limit: usize) -> Vec<Value> {
    let Ok(output) = Command::new(sibling("fw-corpus")).args(["search", query, "--limit", &limit.to_string()]).output() else {
        return Vec::new();
    };
    serde_json::from_slice::<Value>(&output.stdout).ok().and_then(|value| value["hits"].as_array().cloned()).unwrap_or_default()
}

/// Hardened system unit for the harvest loop on a build host; the account named must own the repositories and its `gh` login.
pub fn systemd_unit(binary: &Path, root: &Path, repos: &[PathBuf], interval: u64, user: &str) -> String {
    let flags: String = repos.iter().map(|repo| format!(" --repo {}", repo.display())).collect();
    let writable: Vec<String> = std::iter::once(root.display().to_string()).chain(repos.iter().map(|repo| repo.display().to_string())).collect();
    format!(
        "[Unit]\nDescription=fw learn: harvest, distill and publish rules\nAfter=network-online.target local-fs.target\nWants=network-online.target\n\n\
[Service]\nType=simple\nUser={user}\nEnvironment=FW_LEARN_XFS_ROOT={root}\nExecStart={binary} loop --interval {interval} --cycles 0{flags}\n\
Restart=always\nRestartSec=30\nNice=19\nIOSchedulingClass=idle\nMemoryHigh=1G\nMemoryMax=2G\nNoNewPrivileges=true\nPrivateTmp=true\nProtectSystem=strict\nReadWritePaths={writable}\n\n\
[Install]\nWantedBy=multi-user.target\n",
        binary = binary.display(),
        root = root.display(),
        writable = writable.join(" "),
    )
}
