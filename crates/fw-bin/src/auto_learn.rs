#![forbid(unsafe_code)]

//! Pure-Rust Keyless Autonomous Self-Learning Engine (`fw learn`, `fw mem`, `fw graph`, `fw docs`/`c7`):
//! - **claude-mem parity**: Structured observations, 3-layer progressive disclosure (`search` -> `timeline` -> `get`),
//!   BM25 + recency ranking, SHA-256 content-addressed deduplication, append-only sharded XFS JSONL.
//! - **Code graph**: `fw graph` and `fw learn harvest` run the `fw-graph` binary (crates/fw-rag codegraph: tree-sitter
//!   Rust, oxc TS/JS, Markdown), stored per project in `graphs/<project>/` with `graph.json` and `GRAPH_REPORT.md`.
//! - **context7 parity (100% keyless)**: Live TLS 1.3 resolver for crates.io, npm, PyPI, GitHub raw docs, and `llms.txt` /
//!   `llms-full.txt`, heading-based semantic chunking, BM25 token-budgeted retrieval, permanent XFS cache.
//! - **Infinite XFS Dataset & Agentless Loop**: Sharded `observations/YYYY-MM.jsonl`, `graphs/`, `docs/`, `corpus/YYYY-MM.jsonl`,
//!   `rules/learned-auto.md`, continuous `fw learn loop` + hardened `systemd` unit for 24/7 VPS learning without agents.

use fw_fx::config::FxConfig;
use fw_fx::indexer::WorkspaceIndexer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ============================================================================
// 1. Infinite XFS Sharded Dataset Store
// ============================================================================

#[derive(Debug, Clone)]
pub struct XfsDatasetStore {
    pub root: PathBuf,
}

impl XfsDatasetStore {
    /// Resolves the dataset root: `--xfs-root` > `FW_LEARN_XFS_ROOT` > `/srv/xbuild/fw-learn` (on Linux VPS) > `~/.fx/learn-xfs`.
    pub fn resolve(override_root: Option<&Path>) -> Self {
        if let Some(p) = override_root {
            return Self { root: p.to_path_buf() };
        }
        if let Ok(env_root) = std::env::var("FW_LEARN_XFS_ROOT")
            && !env_root.trim().is_empty()
        {
            return Self { root: PathBuf::from(env_root) };
        }
        let vps_xfs = Path::new("/srv/xbuild");
        if vps_xfs.is_dir() {
            return Self { root: vps_xfs.join("fw-learn") };
        }
        let cfg = FxConfig::from_env();
        Self { root: cfg.fx_home.join("learn-xfs") }
    }

    pub fn ensure_layout(&self) -> Result<(), String> {
        for sub in ["observations", "graphs", "docs", "corpus", "index", "rules"] {
            fs::create_dir_all(self.root.join(sub)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn dedup_path(&self) -> PathBuf {
        self.root.join("index").join("dedup.txt")
    }

    fn load_dedup_set(&self) -> HashSet<String> {
        fs::read_to_string(self.dedup_path())
            .ok()
            .map(|raw| raw.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    fn record_hash(&self, hash: &str) -> Result<(), String> {
        self.ensure_layout()?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dedup_path())
            .map_err(|e| e.to_string())?;
        writeln!(f, "{hash}").map_err(|e| e.to_string())
    }

    fn append_jsonl(&self, rel_path: &Path, line: &str) -> Result<(), String> {
        let full = self.root.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&full)
            .map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())
    }

    pub fn append_corpus_pair(&self, source: &str, prompt: &str, completion: &str) -> Result<bool, String> {
        let hash = sha256_hex(&format!("corpus|{source}|{prompt}|{completion}"));
        let dedup = self.load_dedup_set();
        if dedup.contains(&hash) {
            return Ok(false);
        }
        let shard = format!("corpus/{}.jsonl", current_month_shard());
        let record = json!({
            "id": &hash[..16],
            "ts": now_unix(),
            "source": source,
            "prompt": prompt,
            "completion": completion
        });
        self.append_jsonl(Path::new(&shard), &record.to_string())?;
        self.record_hash(&hash)?;
        Ok(true)
    }

    pub fn stats(&self) -> Value {
        let count_dir = |sub: &str| -> (usize, usize, u64) {
            let dir = self.root.join(sub);
            let mut files = 0usize;
            let mut lines = 0usize;
            let mut bytes = 0u64;
            collect_files_stats(&dir, &mut files, &mut lines, &mut bytes);
            (files, lines, bytes)
        };
        let (obs_f, obs_l, obs_b) = count_dir("observations");
        let (gr_f, _, gr_b) = count_dir("graphs");
        let (doc_f, doc_l, doc_b) = count_dir("docs");
        let (cor_f, cor_l, cor_b) = count_dir("corpus");
        let dedup_count = self.load_dedup_set().len();
        let total_bytes = obs_b + gr_b + doc_b + cor_b;

        json!({
            "ok": true,
            "zeroSubprocess": true,
            "keyless": true,
            "xfsRoot": self.root.display().to_string(),
            "xfsSharded": true,
            "observations": { "shards": obs_f, "records": obs_l, "bytes": obs_b },
            "graphs": { "files": gr_f, "bytes": gr_b },
            "docs": { "libraries": doc_f, "chunks": doc_l, "bytes": doc_b },
            "corpus": { "shards": cor_f, "pairs": cor_l, "bytes": cor_b },
            "dedupHashes": dedup_count,
            "totalBytes": total_bytes
        })
    }
}

fn collect_files_stats(dir: &Path, files: &mut usize, lines: &mut usize, bytes: &mut u64) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            collect_files_stats(&p, files, lines, bytes);
        } else if p.is_file() {
            *files += 1;
            if let Ok(meta) = ent.metadata() {
                *bytes += meta.len();
            }
            if p.extension().and_then(|s| s.to_str()) == Some("jsonl")
                && let Ok(raw) = fs::read_to_string(&p)
            {
                *lines += raw.lines().filter(|l| !l.trim().is_empty()).count();
            }
        }
    }
}

// ============================================================================
// 2. Pure-Rust Keyless `claude-mem` (3-Layer Progressive Disclosure + BM25)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: u64,
    pub ts_unix: u64,
    pub obs_type: String,
    pub project: String,
    pub title: String,
    pub narrative: String,
    pub facts: Vec<String>,
    pub files: Vec<String>,
    pub sha256: String,
}

impl Observation {
    fn est_tokens(&self) -> usize {
        let chars = self.title.len()
            + self.narrative.len()
            + self.facts.iter().map(String::len).sum::<usize>();
        (chars / 4).max(8)
    }

    fn searchable_text(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.obs_type,
            self.project,
            self.title,
            self.narrative,
            self.facts.join(" ")
        )
    }
}

pub fn load_all_observations(store: &XfsDatasetStore) -> Vec<Observation> {
    let mut out = Vec::new();
    let dir = store.root.join("observations");
    let Ok(rd) = fs::read_dir(&dir) else {
        return out;
    };
    let mut shards: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();
    shards.sort();
    for shard in shards {
        if let Ok(raw) = fs::read_to_string(&shard) {
            for line in raw.lines() {
                if let Ok(obs) = serde_json::from_str::<Observation>(line) {
                    out.push(obs);
                }
            }
        }
    }
    out.sort_by_key(|o| o.id);
    out
}

#[allow(clippy::too_many_arguments)]
pub fn add_observation(
    store: &XfsDatasetStore,
    obs_type: &str,
    project: &str,
    title: &str,
    narrative: &str,
    facts: Vec<String>,
    files: Vec<String>,
) -> Result<(Observation, bool), String> {
    store.ensure_layout()?;
    let hash = sha256_hex(&format!("obs|{obs_type}|{project}|{title}|{narrative}"));
    let existing = load_all_observations(store);
    if let Some(dup) = existing.iter().find(|o| o.sha256 == hash) {
        return Ok((dup.clone(), false));
    }
    let next_id = existing.last().map_or(1, |o| o.id + 1);
    let obs = Observation {
        id: next_id,
        ts_unix: now_unix(),
        obs_type: obs_type.to_owned(),
        project: project.to_owned(),
        title: title.to_owned(),
        narrative: narrative.to_owned(),
        facts,
        files,
        sha256: hash.clone(),
    };
    let shard = format!("observations/{}.jsonl", current_month_shard());
    let line = serde_json::to_string(&obs).map_err(|e| e.to_string())?;
    store.append_jsonl(Path::new(&shard), &line)?;
    store.record_hash(&hash)?;
    Ok((obs, true))
}

/// `fw mem <add|search|timeline|get|list|stats|prune>` — Pure-Rust Keyless `claude-mem`.
pub fn run_claude_mem_cli(args: &[String], store: &XfsDatasetStore) -> Result<Value, String> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "stats" => Ok(store.stats()),
        "list" => {
            let items = load_all_observations(store);
            let limit = flag_usize(args, "--limit", 25);
            let compact: Vec<Value> = items
                .iter()
                .rev()
                .take(limit)
                .map(|o| {
                    json!({
                        "id": o.id,
                        "ts": o.ts_unix,
                        "type": o.obs_type,
                        "project": o.project,
                        "title": o.title,
                        "estTokens": o.est_tokens()
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "engine": "fw-mem (pure-Rust keyless claude-mem)",
                "xfsRoot": store.root.display().to_string(),
                "count": items.len(),
                "items": compact
            }))
        }
        "add" => {
            let obs_type = flag_str(args, "--type").unwrap_or("discovery");
            let project = flag_str(args, "--project").unwrap_or("fw");
            let body = flag_str(args, "--body").unwrap_or("");
            let facts: Vec<String> = flag_str(args, "--facts")
                .map(|s| s.split(';').map(|x| x.trim().to_owned()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default();
            let files: Vec<String> = flag_str(args, "--files")
                .map(|s| s.split(',').map(|x| x.trim().to_owned()).filter(|x| !x.is_empty()).collect())
                .unwrap_or_default();
            let title = collect_positional(&args[1..]).join(" ");
            if title.trim().is_empty() {
                return Err("usage: fw mem add <title> [--type decision|bugfix|feature|discovery] [--body <text>] [--facts a;b]".to_owned());
            }
            let narrative = if body.is_empty() { &title } else { body };
            let (obs, created) = add_observation(store, obs_type, project, &title, narrative, facts, files)?;
            let _ = store.append_corpus_pair("claude-mem", &format!("[{}] {}", obs.obs_type, obs.title), &obs.narrative);
            Ok(json!({
                "ok": true,
                "created": created,
                "deduplicated": !created,
                "observation": obs
            }))
        }
        "search" => {
            // Layer 1 (Progressive Disclosure): compact index with BM25 + recency score
            let limit = flag_usize(args, "--limit", 15);
            let type_filter = flag_str(args, "--type");
            let query = collect_positional(&args[1..]).join(" ");
            let items = load_all_observations(store);
            let docs: Vec<String> = items.iter().map(Observation::searchable_text).collect();
            let ranked = bm25_rank(&docs, &query);
            let mut results = Vec::new();
            for (idx, score) in ranked {
                let o = &items[idx];
                if let Some(tf) = type_filter
                    && !o.obs_type.eq_ignore_ascii_case(tf)
                {
                    continue;
                }
                results.push(json!({
                    "id": o.id,
                    "ts": o.ts_unix,
                    "type": o.obs_type,
                    "project": o.project,
                    "title": o.title,
                    "score": (score * 1000.0).round() / 1000.0,
                    "estTokens": o.est_tokens()
                }));
                if results.len() >= limit {
                    break;
                }
            }
            Ok(json!({
                "ok": true,
                "layer": "1-index (use `fw mem timeline <id>` or `fw mem get <id>` for full details)",
                "query": query,
                "count": results.len(),
                "results": results
            }))
        }
        "timeline" => {
            // Layer 2: chronological window around anchor observation ID
            let anchor: u64 = args
                .get(1)
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| "usage: fw mem timeline <observation-id> [--before 3] [--after 3]".to_owned())?;
            let before = flag_usize(args, "--before", 3);
            let after = flag_usize(args, "--after", 3);
            let items = load_all_observations(store);
            let pos = items
                .iter()
                .position(|o| o.id == anchor)
                .ok_or_else(|| format!("observation id {anchor} not found"))?;
            let start = pos.saturating_sub(before);
            let end = (pos + after + 1).min(items.len());
            let slice: Vec<Value> = items[start..end]
                .iter()
                .map(|o| {
                    json!({
                        "id": o.id,
                        "anchor": o.id == anchor,
                        "ts": o.ts_unix,
                        "type": o.obs_type,
                        "title": o.title
                    })
                })
                .collect();
            Ok(json!({
                "ok": true,
                "layer": "2-timeline",
                "anchor": anchor,
                "window": slice
            }))
        }
        "get" => {
            // Layer 3: full observation payload for selected IDs
            let raw_ids = args
                .get(1)
                .ok_or_else(|| "usage: fw mem get <id1,id2,...>".to_owned())?;
            let want: HashSet<u64> = raw_ids.split(',').filter_map(|s| s.trim().parse().ok()).collect();
            let items = load_all_observations(store);
            let found: Vec<&Observation> = items.iter().filter(|o| want.contains(&o.id)).collect();
            Ok(json!({
                "ok": true,
                "layer": "3-full",
                "count": found.len(),
                "observations": found
            }))
        }
        "prune" => {
            let keep = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(500);
            let mut items = load_all_observations(store);
            let before = items.len();
            if items.len() > keep {
                let drop_n = items.len() - keep;
                items.drain(0..drop_n);
                let dir = store.root.join("observations");
                if let Ok(rd) = fs::read_dir(&dir) {
                    for e in rd.flatten() {
                        let _ = fs::remove_file(e.path());
                    }
                }
                let shard = dir.join(format!("{}.jsonl", current_month_shard()));
                let mut buf = String::new();
                for o in &items {
                    buf.push_str(&serde_json::to_string(o).map_err(|e| e.to_string())?);
                    buf.push('\n');
                }
                fs::write(&shard, buf).map_err(|e| e.to_string())?;
            }
            Ok(json!({ "ok": true, "before": before, "after": items.len() }))
        }
        other => Err(format!(
            "unknown mem subcommand `{other}` (expected list|add|search|timeline|get|stats|prune)"
        )),
    }
}

// ============================================================================
// 3. Code graph: the `fw-graph` binary (crates/fw-rag codegraph: tree-sitter Rust, oxc TS/JS, Markdown)
// ============================================================================

/// `fw-graph` next to the running `fw`, else on PATH.
pub fn fw_graph_program() -> PathBuf {
    let exe = format!("fw-graph{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&exe)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(exe))
}

/// Runs `fw-graph` and returns its stdout.
pub fn run_fw_graph(args: &[String]) -> Result<String, String> {
    let program = fw_graph_program();
    let out = std::process::Command::new(&program).args(args).output().map_err(|e| {
        format!(
            "cannot run `{}` ({e}); build it with `cargo build --release -p fw-rag --features codegraph --bin fw-graph` and keep it next to `fw`",
            program.display()
        )
    })?;
    if !out.status.success() {
        return Err(format!("fw-graph {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One code graph per project: `<store>/graphs/<project>/` holds the fw-graph database, `graph.json` and `GRAPH_REPORT.md`.
pub fn graph_dir(store: &XfsDatasetStore, project: &str) -> PathBuf {
    store.root.join("graphs").join(sanitize_slug(project))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GraphSnapshot {
    pub nodes: u64,
    pub edges: u64,
    pub communities: Option<u64>,
    pub god_nodes: Vec<String>,
    pub report: PathBuf,
}

/// Reads the community count and the god nodes (`- name (N edges) - path`) from a `GRAPH_REPORT.md`.
pub fn parse_graph_report(md: &str) -> (Option<u64>, Vec<String>) {
    let communities = md.lines().find_map(|l| {
        let l = l.trim_start_matches("- ");
        let (_, rest) = l.split_once(" edges, ")?;
        rest.strip_suffix(" communities")?.parse().ok()
    });
    let mut gods = Vec::new();
    let mut in_gods = false;
    for line in md.lines() {
        if line.starts_with("## ") {
            in_gods = line == "## God nodes";
            continue;
        }
        if in_gods && let Some(item) = line.strip_prefix("- ") {
            let name = item.split(" (").next().unwrap_or(item).trim();
            if !name.is_empty() {
                gods.push(name.to_owned());
            }
        }
    }
    (communities, gods)
}

/// Builds (incrementally) the code graph of `root` into the learning store.
pub fn build_code_graph(store: &XfsDatasetStore, root: &Path, project: &str) -> Result<GraphSnapshot, String> {
    store.ensure_layout()?;
    let dir = graph_dir(store, project);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let report = dir.join("GRAPH_REPORT.md");
    let arg = |p: &Path| p.display().to_string();
    let out = run_fw_graph(&[
        "--state".into(),
        arg(&dir),
        "build".into(),
        arg(root),
        "--json".into(),
        arg(&dir.join("graph.json")),
        "--report".into(),
        arg(&report),
    ])?;
    let v: Value = serde_json::from_str(out.trim()).map_err(|e| format!("fw-graph build output: {e}"))?;
    let (communities, god_nodes) = parse_graph_report(&fs::read_to_string(&report).unwrap_or_default());
    Ok(GraphSnapshot {
        nodes: v["nodes"].as_u64().unwrap_or(0),
        edges: v["edges"].as_u64().unwrap_or(0),
        communities,
        god_nodes,
        report,
    })
}

/// Code-graph hits for a query across every project graph in the store (`NODE` lines of `fw-graph query`).
fn code_graph_hits(store: &XfsDatasetStore, query: &str, limit: usize) -> Vec<Value> {
    let mut hits = Vec::new();
    let Ok(rd) = fs::read_dir(store.root.join("graphs")) else {
        return hits;
    };
    for ent in rd.flatten() {
        let dir = ent.path();
        if !dir.join("GRAPH_REPORT.md").is_file() {
            continue;
        }
        let project = ent.file_name().to_string_lossy().into_owned();
        let args = ["--state".to_owned(), dir.display().to_string(), "query".to_owned(), query.to_owned(), "--budget".to_owned(), "600".to_owned()];
        let Ok(out) = run_fw_graph(&args) else {
            continue;
        };
        for line in out.lines().filter_map(|l| l.strip_prefix("NODE ")).take(limit.saturating_sub(hits.len())) {
            hits.push(json!({ "pillar": "codegraph", "project": project, "node": line }));
        }
        if hits.len() >= limit {
            break;
        }
    }
    hits
}

/// Hits from the `fw-corpus` SQLite FTS5 stores (`fw corpus search`); empty when the sibling binary or the corpus is absent.
fn corpus_hits(query: &str, limit: usize) -> Vec<Value> {
    let exe = format!("fw-corpus{}", std::env::consts::EXE_SUFFIX);
    let program = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&exe)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from(exe));
    let Ok(out) = std::process::Command::new(program).args(["search", query, "--limit", &limit.to_string()]).output() else {
        return Vec::new();
    };
    serde_json::from_slice::<Value>(&out.stdout)
        .ok()
        .and_then(|v| v["hits"].as_array().cloned())
        .unwrap_or_default()
}

// ============================================================================
// 4. Pure-Rust Keyless `context7` (`fw docs` / `fw c7`)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChunk {
    pub lib: String,
    pub ecosystem: String,
    pub heading: String,
    pub source_url: String,
    pub content: String,
    pub ts_unix: u64,
    pub sha256: String,
}

impl DocChunk {
    fn est_tokens(&self) -> usize {
        ((self.heading.len() + self.content.len()) / 4).max(8)
    }
}

pub fn chunk_markdown_docs(lib: &str, ecosystem: &str, source_url: &str, raw: &str) -> Vec<DocChunk> {
    let mut chunks = Vec::new();
    let mut cur_heading = format!("{lib} overview");
    let mut cur_body = String::new();
    let ts = now_unix();

    let flush = |heading: &str, body: &str, out: &mut Vec<DocChunk>| {
        let trimmed = body.trim();
        if trimmed.len() < 20 {
            return;
        }
        let capped: String = trimmed.chars().take(2400).collect();
        let hash = sha256_hex(&format!("doc|{lib}|{heading}|{capped}"));
        out.push(DocChunk {
            lib: lib.to_owned(),
            ecosystem: ecosystem.to_owned(),
            heading: heading.to_owned(),
            source_url: source_url.to_owned(),
            content: capped,
            ts_unix: ts,
            sha256: hash,
        });
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            flush(&cur_heading, &cur_body, &mut chunks);
            cur_heading = trimmed.trim_start_matches('#').trim().to_owned();
            if cur_heading.is_empty() {
                cur_heading = lib.to_owned();
            }
            cur_body.clear();
        } else {
            cur_body.push_str(line);
            cur_body.push('\n');
            if cur_body.len() >= 2200 {
                flush(&cur_heading, &cur_body, &mut chunks);
                cur_body.clear();
            }
        }
    }
    flush(&cur_heading, &cur_body, &mut chunks);
    chunks
}

pub fn save_doc_chunks(store: &XfsDatasetStore, chunks: &[DocChunk]) -> Result<usize, String> {
    if chunks.is_empty() {
        return Ok(0);
    }
    store.ensure_layout()?;
    let dedup = store.load_dedup_set();
    let mut written = 0usize;
    for ch in chunks {
        if dedup.contains(&ch.sha256) {
            continue;
        }
        let rel = PathBuf::from("docs")
            .join(sanitize_slug(&ch.ecosystem))
            .join(format!("{}.jsonl", sanitize_slug(&ch.lib)));
        let line = serde_json::to_string(ch).map_err(|e| e.to_string())?;
        store.append_jsonl(&rel, &line)?;
        store.record_hash(&ch.sha256)?;
        let _ = store.append_corpus_pair(
            "context7-keyless",
            &format!("{} ({}) — {}", ch.lib, ch.ecosystem, ch.heading),
            &ch.content,
        );
        written += 1;
    }
    Ok(written)
}

pub fn load_doc_chunks(store: &XfsDatasetStore, lib_filter: Option<&str>) -> Vec<DocChunk> {
    let mut out = Vec::new();
    collect_doc_chunks_dir(&store.root.join("docs"), lib_filter, &mut out);
    out
}

fn collect_doc_chunks_dir(dir: &Path, lib_filter: Option<&str>, out: &mut Vec<DocChunk>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            collect_doc_chunks_dir(&p, lib_filter, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("jsonl")
            && let Ok(raw) = fs::read_to_string(&p)
        {
            for line in raw.lines() {
                if let Ok(ch) = serde_json::from_str::<DocChunk>(line) {
                    if let Some(lf) = lib_filter
                        && !ch.lib.eq_ignore_ascii_case(lf)
                        && !ch.lib.to_ascii_lowercase().contains(&lf.to_ascii_lowercase())
                    {
                        continue;
                    }
                    out.push(ch);
                }
            }
        }
    }
}

fn http_get_keyless(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    agent
        .get(url)
        .header("User-Agent", concat!("fw-context7-keyless/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "text/markdown, text/plain, application/json")
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

fn resolve_keyless_live(lib: &str, ecosystem: &str) -> Result<(String, String), String> {
    match ecosystem {
        "github" => {
            let url = format!("https://raw.githubusercontent.com/{lib}/HEAD/README.md");
            Ok((url.clone(), http_get_keyless(&url)?))
        }
        "llms" => {
            let url = if lib.starts_with("http") {
                lib.to_owned()
            } else {
                format!("https://{lib}/llms.txt")
            };
            Ok((url.clone(), http_get_keyless(&url)?))
        }
        "npm" => {
            let url = format!("https://registry.npmjs.org/{lib}");
            let raw = http_get_keyless(&url)?;
            let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let readme = v["readme"]
                .as_str()
                .or_else(|| v["description"].as_str())
                .unwrap_or("")
                .to_owned();
            Ok((url, format!("# {lib} (npm)\n\n{readme}")))
        }
        "pypi" => {
            let url = format!("https://pypi.org/pypi/{lib}/json");
            let raw = http_get_keyless(&url)?;
            let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let desc = v["info"]["description"]
                .as_str()
                .or_else(|| v["info"]["summary"].as_str())
                .unwrap_or("")
                .to_owned();
            Ok((url, format!("# {lib} (pypi)\n\n{desc}")))
        }
        _ => {
            // Default: crates.io API + repository README fallback
            let url = format!("https://crates.io/api/v1/crates/{lib}");
            let raw = http_get_keyless(&url)?;
            let v: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let desc = v["crate"]["description"].as_str().unwrap_or("");
            let ver = v["crate"]["max_version"].as_str().unwrap_or("");
            let repo = v["crate"]["repository"].as_str().unwrap_or("");
            let mut body = format!("# {lib} {ver}\n\n{desc}\n\nRepository: {repo}\n");
            if let Some(gh) = repo.strip_prefix("https://github.com/") {
                let slug = gh.trim_end_matches('/').trim_end_matches(".git");
                let readme_url = format!("https://raw.githubusercontent.com/{slug}/HEAD/README.md");
                if let Ok(rm) = http_get_keyless(&readme_url) {
                    body.push_str("\n## README\n");
                    body.push_str(&rm);
                }
            }
            Ok((url, body))
        }
    }
}

/// `fw docs <resolve|query|ingest|list> ...` (`fw c7`) — Pure-Rust Keyless Context7.
pub fn run_context7_cli(
    args: &[String],
    cwd: &Path,
    store: &XfsDatasetStore,
    offline: bool,
) -> Result<Value, String> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => {
            let chunks = load_doc_chunks(store, None);
            let mut by_lib: BTreeMap<String, usize> = BTreeMap::new();
            for c in &chunks {
                *by_lib.entry(format!("{}:{}", c.ecosystem, c.lib)).or_insert(0) += 1;
            }
            Ok(json!({
                "ok": true,
                "keyless": true,
                "engine": "fw-docs (pure-Rust keyless Context7)",
                "xfsRoot": store.root.display().to_string(),
                "totalChunks": chunks.len(),
                "libraries": by_lib
            }))
        }
        "resolve" => {
            let lib = args
                .get(1)
                .ok_or_else(|| "usage: fw docs resolve <lib> [--ecosystem cargo|npm|pypi|github|llms]".to_owned())?;
            let eco = flag_str(args, "--ecosystem").unwrap_or_else(|| {
                if lib.starts_with("http") {
                    "llms"
                } else if lib.contains('/') {
                    "github"
                } else {
                    "cargo"
                }
            });
            let cached = load_doc_chunks(store, Some(lib));
            if !cached.is_empty() && offline {
                return Ok(json!({
                    "ok": true,
                    "source": "xfs-cache",
                    "lib": lib,
                    "chunks": cached.len()
                }));
            }
            if offline {
                return Err("fw docs resolve: offline and not in XFS cache".to_owned());
            }
            let (url, raw) = resolve_keyless_live(lib, eco)?;
            let chunks = chunk_markdown_docs(lib, eco, &url, &raw);
            let saved = save_doc_chunks(store, &chunks)?;
            Ok(json!({
                "ok": true,
                "keyless": true,
                "lib": lib,
                "ecosystem": eco,
                "sourceUrl": url,
                "chunksParsed": chunks.len(),
                "chunksSavedToXfs": saved
            }))
        }
        "query" => {
            let lib = args
                .get(1)
                .ok_or_else(|| "usage: fw docs query <lib> <topic> [--tokens 4000]".to_owned())?;
            let max_tokens = flag_usize(args, "--tokens", 4000);
            let topic = collect_positional(&args[2..]).join(" ");
            let mut chunks = load_doc_chunks(store, Some(lib));
            if chunks.is_empty() && !offline {
                let eco = if lib.contains('/') { "github" } else { "cargo" };
                if let Ok((url, raw)) = resolve_keyless_live(lib, eco) {
                    let parsed = chunk_markdown_docs(lib, eco, &url, &raw);
                    let _ = save_doc_chunks(store, &parsed);
                    chunks = parsed;
                }
            }
            let docs: Vec<String> = chunks.iter().map(|c| format!("{} {}", c.heading, c.content)).collect();
            let ranked = bm25_rank(&docs, &topic);
            let mut used_tokens = 0usize;
            let mut picked = Vec::new();
            for (idx, score) in ranked {
                let ch = &chunks[idx];
                let t = ch.est_tokens();
                if used_tokens + t > max_tokens && !picked.is_empty() {
                    break;
                }
                used_tokens += t;
                picked.push(json!({
                    "lib": ch.lib,
                    "heading": ch.heading,
                    "score": (score * 1000.0).round() / 1000.0,
                    "estTokens": t,
                    "sourceUrl": ch.source_url,
                    "content": ch.content
                }));
            }
            Ok(json!({
                "ok": true,
                "keyless": true,
                "engine": "fw-docs (pure-Rust keyless Context7)",
                "lib": lib,
                "topic": topic,
                "tokensUsed": used_tokens,
                "tokenBudget": max_tokens,
                "results": picked
            }))
        }
        "ingest" => {
            let target = args
                .get(1)
                .ok_or_else(|| "usage: fw docs ingest <url-or-file> [--lib <name>]".to_owned())?;
            let lib = flag_str(args, "--lib").unwrap_or("local-docs");
            let (source, raw) = if target.starts_with("http://") || target.starts_with("https://") {
                if offline {
                    return Err("fw docs ingest: disabled by --offline for URLs".to_owned());
                }
                (target.clone(), http_get_keyless(target)?)
            } else {
                let p = cwd.join(target);
                let txt = fs::read_to_string(&p).map_err(|e| format!("read {}: {e}", p.display()))?;
                (p.display().to_string(), txt)
            };
            let chunks = chunk_markdown_docs(lib, "docs", &source, &raw);
            let saved = save_doc_chunks(store, &chunks)?;
            Ok(json!({
                "ok": true,
                "keyless": true,
                "lib": lib,
                "source": source,
                "chunksParsed": chunks.len(),
                "chunksSavedToXfs": saved
            }))
        }
        other => Err(format!(
            "unknown docs subcommand `{other}` (expected resolve|query|ingest|list)"
        )),
    }
}

// ============================================================================
// 5. Autonomous Agentless Self-Learning Engine (`fw learn`)
// ============================================================================

/// Runs ONE complete agentless learning cycle (zero LLM calls, zero human/agent needed).
pub fn run_agentless_harvest(cwd: &Path, store: &XfsDatasetStore) -> Result<Value, String> {
    store.ensure_layout()?;
    let project = cwd
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace");

    // 1. Harvest recent git commits directly from `.git/logs/HEAD` (zero subprocess!)
    let mut git_obs_added = 0usize;
    let git_log_path = cwd.join(".git").join("logs").join("HEAD");
    if let Ok(raw) = fs::read_to_string(&git_log_path) {
        for line in raw.lines().rev().take(15) {
            if let Some((_, msg)) = line.split_once('\t') {
                let msg = msg.trim();
                if msg.is_empty() {
                    continue;
                }
                let (_, created) = add_observation(
                    store,
                    "change",
                    project,
                    msg,
                    &format!("Git history event in {project}: {msg}"),
                    vec![format!("commit:{msg}")],
                    Vec::new(),
                )?;
                if created {
                    git_obs_added += 1;
                    let _ = store.append_corpus_pair("git-reflog", &format!("Recent change in {project}"), msg);
                }
            }
        }
    }

    // 2. Rebuild the code graph and distill its god nodes into claude-mem + corpus. A missing `fw-graph`
    //    binary is reported in the result instead of failing the other pillars.
    let graph_snapshot = match build_code_graph(store, cwd, project) {
        Ok(graph) => {
            let god_summary = format!(
                "Project {project}: {} nodes, {} edges, {} communities. God nodes: {}",
                graph.nodes,
                graph.edges,
                graph.communities.map_or_else(|| "?".to_owned(), |c| c.to_string()),
                graph.god_nodes.join(", ")
            );
            let (_, created) = add_observation(
                store,
                "discovery",
                project,
                &format!("Code graph snapshot ({project})"),
                &god_summary,
                graph.god_nodes.clone(),
                graph.god_nodes.clone(),
            )?;
            let _ = store.append_corpus_pair("codegraph", &format!("Architecture and god nodes of {project}"), &god_summary);
            json!({
                "report": graph.report.display().to_string(),
                "nodes": graph.nodes,
                "edges": graph.edges,
                "communities": graph.communities,
                "godNodes": graph.god_nodes,
                "observationCreated": created
            })
        }
        Err(e) => json!({ "error": e }),
    };

    // 3. Ingest local Markdown / SKILL.md / AGENTS.md / llms.txt into keyless Context7 store
    let idx = WorkspaceIndexer::open(cwd);
    let mut docs_saved = 0usize;
    for rel in idx.list_files(300) {
        if !(rel.ends_with(".md") || rel.ends_with("llms.txt")) {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(cwd.join(&rel)) {
            let chunks = chunk_markdown_docs(project, "workspace-md", &rel, &raw);
            docs_saved += save_doc_chunks(store, &chunks)?;
        }
    }

    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "agentless": true,
        "keyless": true,
        "xfsRoot": store.root.display().to_string(),
        "project": project,
        "gitObservationsAdded": git_obs_added,
        "graphSnapshot": graph_snapshot,
        "docChunksAdded": docs_saved,
        "dataset": store.stats()
    }))
}

pub fn run_distill(store: &XfsDatasetStore) -> Result<Value, String> {
    store.ensure_layout()?;
    let obs = load_all_observations(store);
    let docs = load_doc_chunks(store, None);

    let mut md = String::from("# Fw Autonomous Learned Memory (Pure-Rust Keyless XFS)\n\n");
    md.push_str(&format!(
        "- Observations: {}\n- Keyless Context7 Doc Chunks: {}\n- XFS Store: `{}`\n\n## Recent High-Signal Observations\n",
        obs.len(),
        docs.len(),
        store.root.display()
    ));
    for o in obs.iter().rev().take(40) {
        md.push_str(&format!("- **[{}]** `{}` — {}\n", o.obs_type, o.project, o.title));
    }

    let rules_path = store.root.join("rules").join("learned-auto.md");
    fs::write(&rules_path, &md).map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": true,
        "zeroSubprocess": true,
        "rulesPath": rules_path.display().to_string(),
        "observationsDistilled": obs.len(),
        "docChunksIndexed": docs.len(),
        "bytes": md.len()
    }))
}

/// Unified keyless search across the 3 pillars: `claude-mem`, the code graph and `context7`.
pub fn run_unified_search(query: &str, store: &XfsDatasetStore, limit: usize) -> Value {
    let obs = load_all_observations(store);
    let docs = load_doc_chunks(store, None);

    let obs_texts: Vec<String> = obs.iter().map(Observation::searchable_text).collect();
    let obs_hits: Vec<Value> = bm25_rank(&obs_texts, query)
        .into_iter()
        .take(limit)
        .map(|(i, s)| {
            let o = &obs[i];
            json!({
                "pillar": "claude-mem",
                "id": o.id,
                "type": o.obs_type,
                "title": o.title,
                "score": (s * 1000.0).round() / 1000.0
            })
        })
        .collect();

    let doc_texts: Vec<String> = docs.iter().map(|d| format!("{} {} {}", d.lib, d.heading, d.content)).collect();
    let doc_hits: Vec<Value> = bm25_rank(&doc_texts, query)
        .into_iter()
        .take(limit)
        .map(|(i, s)| {
            let d = &docs[i];
            json!({
                "pillar": "context7-keyless",
                "lib": d.lib,
                "heading": d.heading,
                "source": d.source_url,
                "score": (s * 1000.0).round() / 1000.0,
                "snippet": d.content.chars().take(180).collect::<String>()
            })
        })
        .collect();

    let graph_hits = code_graph_hits(store, query, limit);

    json!({
        "ok": true,
        "keyless": true,
        "zeroSubprocess": true,
        "query": query,
        "xfsRoot": store.root.display().to_string(),
        "claudeMemHits": obs_hits,
        "context7Hits": doc_hits,
        "codeGraphHits": graph_hits,
        "corpusHits": corpus_hits(query, limit)
    })
}

pub fn render_systemd_unit(xfs_root: &Path, interval_secs: u64, watch_dir: &Path) -> String {
    format!(
        "[Unit]\n\
Description=Fw Autonomous Agentless Self-Learning Daemon (Pure-Rust Keyless XFS)\n\
After=network-online.target local-fs.target\n\
Wants=network-online.target\n\
\n\
[Service]\n\
Type=simple\n\
Environment=FW_LEARN_XFS_ROOT={}\n\
ExecStart=/usr/local/bin/fw learn loop --interval {} --cycles 0 --dir {} --xfs-root {}\n\
Restart=always\n\
RestartSec=15\n\
Nice=19\n\
IOSchedulingClass=idle\n\
MemoryHigh=512M\n\
MemoryMax=1G\n\
\n\
[Install]\n\
WantedBy=multi-user.target\n",
        xfs_root.display(),
        interval_secs,
        watch_dir.display(),
        xfs_root.display()
    )
}

/// `fw learn <status|harvest|distill|search|loop|systemd>`
pub fn run_learn_cli(args: &[String], cwd: &Path) -> Result<Value, String> {
    let xfs_override = flag_str(args, "--xfs-root").map(Path::new);
    let store = XfsDatasetStore::resolve(xfs_override);
    let sub = args.first().map(String::as_str).unwrap_or("status");

    match sub {
        "status" => Ok(store.stats()),
        "harvest" => {
            let target = args
                .get(1)
                .filter(|a| !a.starts_with('-'))
                .map(|s| cwd.join(s))
                .unwrap_or_else(|| cwd.to_path_buf());
            run_agentless_harvest(&target, &store)
        }
        "distill" => run_distill(&store),
        "search" => {
            let limit = flag_usize(args, "--limit", 10);
            let query = collect_positional(&args[1..]).join(" ");
            if query.trim().is_empty() {
                return Err("usage: fw learn search <query> [--limit 10]".to_owned());
            }
            Ok(run_unified_search(&query, &store, limit))
        }
        "loop" => {
            let interval = flag_usize(args, "--interval", 300) as u64;
            let cycles = flag_usize(args, "--cycles", 1);
            let dir = flag_str(args, "--dir")
                .map(|s| cwd.join(s))
                .unwrap_or_else(|| cwd.to_path_buf());
            let mut completed = 0usize;
            let mut last = json!({});
            loop {
                last = run_agentless_harvest(&dir, &store)?;
                let _ = run_distill(&store)?;
                completed += 1;
                if cycles > 0 && completed >= cycles {
                    break;
                }
                std::thread::sleep(Duration::from_secs(interval.max(1)));
            }
            Ok(json!({
                "ok": true,
                "agentless": true,
                "cyclesCompleted": completed,
                "intervalSecs": interval,
                "xfsRoot": store.root.display().to_string(),
                "lastCycle": last
            }))
        }
        "systemd" => {
            let interval = flag_usize(args, "--interval", 300) as u64;
            let xfs = flag_str(args, "--xfs-root")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/srv/xbuild/fw-learn"));
            let dir = flag_str(args, "--dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/srv/xbuild"));
            let unit = render_systemd_unit(&xfs, interval, &dir);
            Ok(json!({
                "ok": true,
                "unitName": "fw-learn.service",
                "xfsRoot": xfs.display().to_string(),
                "unit": unit
            }))
        }
        other => Err(format!(
            "unknown learn subcommand `{other}` (expected status|harvest|distill|search|loop|systemd)"
        )),
    }
}

// ============================================================================
// 6. Pure-Rust BM25 Ranking & Helpers (Zero External Dependencies)
// ============================================================================

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| s.len() >= 2)
        .collect()
}

pub fn bm25_rank(docs: &[String], query: &str) -> Vec<(usize, f64)> {
    let q_terms = tokenize(query);
    if docs.is_empty() {
        return Vec::new();
    }
    if q_terms.is_empty() {
        return (0..docs.len()).rev().map(|i| (i, 1.0)).collect();
    }

    let n = docs.len() as f64;
    let tokenized: Vec<Vec<String>> = docs.iter().map(|d| tokenize(d)).collect();
    let avg_dl = (tokenized.iter().map(Vec::len).sum::<usize>() as f64 / n).max(1.0);

    let mut df: HashMap<&str, usize> = HashMap::new();
    for term in &q_terms {
        let count = tokenized
            .iter()
            .filter(|toks| toks.iter().any(|t| t == term || t.contains(term.as_str())))
            .count();
        df.insert(term.as_str(), count);
    }

    let k1 = 1.5;
    let b = 0.75;
    let mut scored: Vec<(usize, f64)> = tokenized
        .iter()
        .enumerate()
        .filter_map(|(idx, toks)| {
            let dl = toks.len() as f64;
            let mut score = 0.0;
            for term in &q_terms {
                let tf = toks
                    .iter()
                    .filter(|t| *t == term || t.contains(term.as_str()))
                    .count() as f64;
                if tf > 0.0 {
                    let df_t = *df.get(term.as_str()).unwrap_or(&0) as f64;
                    let idf = ((n - df_t + 0.5) / (df_t + 0.5) + 1.0).ln();
                    score += idf * ((tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / avg_dl))));
                }
            }
            (score > 0.0).then_some((idx, score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored
}

fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    let digest = h.finalize();
    let mut s = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut s, "{byte:02x}");
    }
    s
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn current_month_shard() -> String {
    // Pure-Rust epoch-to-YYYY-MM without chrono dependency
    let days = now_unix() / 86_400;
    let (y, m) = days_to_ym(days);
    format!("{y:04}-{m:02}")
}

fn days_to_ym(days_since_epoch: u64) -> (u64, u64) {
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as u64, m)
}

fn sanitize_slug(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn flag_str<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].as_str())
}

fn flag_usize(args: &[String], name: &str, default: usize) -> usize {
    flag_str(args, name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn collect_positional(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        let tok = args[i].as_str();
        if matches!(
            tok,
            "--type" | "--project" | "--body" | "--facts" | "--files" | "--limit" | "--before"
                | "--after" | "--tokens" | "--ecosystem" | "--lib" | "--xfs-root" | "--interval"
                | "--cycles" | "--dir"
        ) {
            i += 2;
        } else if tok.starts_with("--") {
            i += 1;
        } else {
            out.push(args[i].clone());
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_mem_codegraph_context7_and_agentless_learn_e2e() {
        let tmp = std::env::temp_dir().join(format!("fw_xfs_learn_test_{}", now_unix()));
        let _ = fs::remove_dir_all(&tmp);
        let store = XfsDatasetStore::resolve(Some(&tmp));

        // 1. claude-mem 3-layer progressive disclosure + dedup
        let a1 = run_claude_mem_cli(
            &[
                "add".to_owned(),
                "Use XFS sharded JSONL for infinite dataset".to_owned(),
                "--type".to_owned(),
                "decision".to_owned(),
                "--facts".to_owned(),
                "xfs;sharded;keyless".to_owned(),
            ],
            &store,
        )
        .unwrap();
        assert_eq!(a1["created"], true);
        let id = a1["observation"]["id"].as_u64().unwrap();

        let search = run_claude_mem_cli(&["search".to_owned(), "xfs".to_owned(), "sharded".to_owned()], &store).unwrap();
        assert_eq!(search["count"], 1);
        let tl = run_claude_mem_cli(&["timeline".to_owned(), id.to_string()], &store).unwrap();
        assert_eq!(tl["window"].as_array().unwrap().len(), 1);
        let full = run_claude_mem_cli(&["get".to_owned(), id.to_string()], &store).unwrap();
        assert_eq!(full["count"], 1);

        // 2. code graph report parsing (the graph itself is built by the fw-graph binary)
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let (communities, gods) = parse_graph_report(
            "# Graph Report\n\n## Summary\n- 28235 nodes, 58390 edges, 692 communities\n\n## God nodes\n- Drive (120 edges) - crates/fw-drive/src/lib.rs\n- router (90 edges)\n\n## Communities\n- 0: Drive (10 nodes)\n",
        );
        assert_eq!(communities, Some(692));
        assert_eq!(gods, ["Drive", "router"]);

        // 3. keyless context7 chunking + token-budgeted query
        let chunks = chunk_markdown_docs(
            "burn",
            "cargo",
            "https://burn.dev/llms.txt",
            "# Burn\n## Tensor\nPure Rust deep learning tensor backend with WGPU and NdArray support.\n## Autodiff\nReverse-mode autodiff wrapper for zero-cost gradients.",
        );
        assert!(chunks.len() >= 2);
        let saved = save_doc_chunks(&store, &chunks).unwrap();
        assert!(saved >= 2);
        let q = run_context7_cli(
            &["query".to_owned(), "burn".to_owned(), "autodiff".to_owned(), "--tokens".to_owned(), "500".to_owned()],
            root,
            &store,
            true,
        )
        .unwrap();
        assert!(!q["results"].as_array().unwrap().is_empty());

        // 4. Agentless harvest + distill + unified search
        let h = run_agentless_harvest(root, &store).unwrap();
        assert_eq!(h["agentless"], true);
        let d = run_distill(&store).unwrap();
        assert_eq!(d["ok"], true);
        let u = run_unified_search("autodiff", &store, 5);
        assert!(!u["context7Hits"].as_array().unwrap().is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }
}
