// SPDX-License-Identifier: Apache-2.0
//! Incremental code-graph build over the home database.
//!
//! scan (store, BLAKE3, deny list) -> filter (`.gitignore`, `.rgignore`, `.fdignore`,
//! `.graphifyignore`, generated code) -> extract (tree-sitter, only files without a valid cache
//! row) -> assemble (call resolution, stub folding, communities) -> save. The per-file raw
//! extraction is cached by content hash, so a second run with no change parses nothing and
//! rewrites an identical graph, and an interrupted run resumes at the files still missing a
//! cache row.
//!
//! Generated code (see [`generated`](crate::codegraph::extract::generated)) is left out of the
//! graph unless [`BuildOptions::include_generated`] is set, in which case its nodes are marked.

use fw_proc::NoWindow as _;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::localstore::{
    HomeDb,
    deny::is_denied,
    files::Stage,
    scan::{ScanOptions, ScanOutcome, scan_tree},
};
use ignore::WalkBuilder;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::codegraph::{
    error::Result,
    extract::{
        EXTRACT_VERSION, FileExtract, Language, extract_file, generated,
        resolve::{AssembleContext, AssembleStats, assemble_with},
        tsconfig::{TsConfig, extends_of},
    },
    graph::{Graph, GraphMeta},
};

/// Environment variable that includes generated files in every build of this process.
pub const INCLUDE_GENERATED_ENV: &str = "FW_GRAPH_INCLUDE_GENERATED";

/// Ignore files honoured below the root next to `.gitignore`: ripgrep's, fd's and Graphify's.
pub const IGNORE_FILES: [&str; 3] = [".rgignore", ".fdignore", ".graphifyignore"];

/// Options of one build.
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Scan options.
    pub scan: ScanOptions,
    /// Keep generated files (protobuf output, `wrangler types`, ...) in the graph, their nodes
    /// marked [`generated`](crate::codegraph::Node::generated).
    pub include_generated: bool,
}

impl BuildOptions {
    /// Options with `include_generated` set when [`INCLUDE_GENERATED_ENV`] is `1` or `true`.
    #[must_use]
    pub fn from_env(scan: ScanOptions) -> Self {
        let include_generated = std::env::var(INCLUDE_GENERATED_ENV)
            .is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"));
        Self {
            scan,
            include_generated,
        }
    }
}

/// Counters of one build.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildReport {
    /// Scan counters.
    pub scan: ScanOutcome,
    /// Files of a supported language left out by `.gitignore`, `.rgignore`, `.fdignore` or
    /// `.graphifyignore`.
    #[serde(default)]
    pub ignored: usize,
    /// Generated files left out of the graph.
    #[serde(default)]
    pub generated_skipped: usize,
    /// Files parsed in this run.
    pub extracted: usize,
    /// Files served from the cache.
    pub cached: usize,
    /// Files with a parse problem.
    pub errors: usize,
    /// Assembly counters.
    pub assemble: AssembleStats,
    /// Nodes of the saved graph.
    pub nodes: usize,
    /// Edges of the saved graph.
    pub edges: usize,
    /// Modularity of the detected communities.
    pub modularity: f64,
    /// `HEAD` of the tree when it is a Git checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at_commit: Option<String>,
}

/// Absolute form of `root` without the Windows verbatim prefix, so the stored root is usable.
#[must_use]
pub fn absolute_root(root: &Path) -> PathBuf {
    let absolute = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let text = absolute.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    text.strip_prefix(r"\\?\")
        .map_or_else(|| absolute.clone(), PathBuf::from)
}

/// `git rev-parse HEAD` of `root`, when it is a Git checkout.
#[must_use]
pub fn head_commit(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .no_window()
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && sha.len() >= 40).then_some(sha)
}

/// Value of `key = "value"` inside the `[section]` of a TOML manifest (enough for Cargo
/// `[package] name` and `[lib] name`).
fn toml_key(manifest: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == format!("[{section}]");
            continue;
        }
        if inside
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            let v = v.trim();
            if let Some(v) = v.strip_prefix('"').and_then(|v| v.split('"').next()) {
                return Some(v.to_owned());
            }
        }
    }
    None
}

/// Dependency identifiers (`serde_json`) a Cargo manifest declares: the keys of its
/// `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` and target-specific tables,
/// and the names of `[dependencies.name]` tables.
fn manifest_deps(manifest: &str) -> HashSet<String> {
    let mut deps = HashSet::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[') {
            let header = header.trim_end_matches(']').trim();
            let table = header.rsplit_once('.').map_or(header, |(_, last)| last);
            inside = matches!(
                table,
                "dependencies" | "dev-dependencies" | "build-dependencies"
            );
            // `[dependencies.name]`
            if let Some((head, name)) = header.rsplit_once('.')
                && matches!(
                    head.rsplit_once('.').map_or(head, |(_, last)| last),
                    "dependencies" | "dev-dependencies" | "build-dependencies"
                )
            {
                deps.insert(name.trim_matches('"').replace('-', "_"));
            }
            continue;
        }
        if inside
            && !line.starts_with('#')
            && let Some((key, _)) = line.split_once('=')
        {
            let key = key.trim().split('.').next().unwrap_or("").trim_matches('"');
            if !key.is_empty() {
                deps.insert(key.replace('-', "_"));
            }
        }
    }
    deps
}

/// Package declared by the manifest of `dir`: the Rust crate identifier, else the npm name.
fn manifest_package(dir: &Path) -> Option<String> {
    if let Ok(cargo) = std::fs::read_to_string(dir.join("Cargo.toml")) {
        if let Some(lib) = toml_key(&cargo, "lib", "name") {
            return Some(lib);
        }
        if let Some(name) = toml_key(&cargo, "package", "name") {
            return Some(name.replace('-', "_"));
        }
    }
    let json = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    value
        .get("name")?
        .as_str()
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

/// Package of every file (its nearest enclosing Cargo or npm manifest inside `root`) and the
/// directory of every package manifest.
#[must_use]
pub fn packages_of(root: &Path, files: &[String]) -> AssembleContext {
    let mut by_dir: HashMap<String, Option<(String, String)>> = HashMap::new();
    let mut out = AssembleContext::default();
    for file in files {
        let mut dir = file.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        let mut walked = Vec::new();
        let found = loop {
            if let Some(hit) = by_dir.get(&dir) {
                break hit.clone();
            }
            walked.push(dir.clone());
            if let Some(package) = manifest_package(&root.join(&dir)) {
                break Some((package, dir.clone()));
            }
            if dir.is_empty() {
                break None;
            }
            dir = dir.rsplit_once('/').map_or("", |(d, _)| d).to_owned();
        };
        for d in walked {
            by_dir.insert(d, found.clone());
        }
        if let Some((package, dir)) = found {
            if !out.package_dir.contains_key(&package) {
                if let Ok(cargo) = std::fs::read_to_string(root.join(&dir).join("Cargo.toml")) {
                    out.package_deps
                        .insert(package.clone(), manifest_deps(&cargo));
                }
                out.package_dir.insert(package.clone(), dir);
            }
            out.package_of.insert(file.clone(), package);
        }
    }
    out
}

/// Load the `tsconfig.json` or `jsconfig.json` at `rel` (root-relative), following relative
/// `extends` chains.
fn load_ts_config(root: &Path, rel: &str, depth: usize) -> Option<TsConfig> {
    let text = std::fs::read_to_string(root.join(rel)).ok()?;
    let dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
    let parent = (depth < 5)
        .then(|| extends_of(dir, &text))
        .flatten()
        .and_then(|parent| load_ts_config(root, &parent, depth + 1));
    TsConfig::parse(dir, &text, parent.as_ref())
}

/// The module resolution settings of every `tsconfig.json` among `files`.
#[must_use]
pub fn ts_configs_of(root: &Path, files: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<TsConfig> {
    let mut configs: Vec<TsConfig> = files
        .into_iter()
        .filter(|f| {
            let name = f.as_ref().rsplit('/').next().unwrap_or("");
            matches!(name, "tsconfig.json" | "jsconfig.json")
        })
        .filter_map(|f| load_ts_config(root, f.as_ref(), 0))
        .collect();
    configs.sort_by(|a, b| a.dir.cmp(&b.dir));
    configs
}

/// Files of a supported language the ignore files and `.gitignore` leave in the tree. A
/// repository's rules apply with or without a `.git` directory.
fn walk_allowed(root: &Path, options: &ScanOptions) -> HashSet<String> {
    let policy = options.policy;
    let root_owned = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false);
    for name in IGNORE_FILES {
        builder.add_custom_ignore_filename(name);
    }
    builder.filter_entry(move |entry| {
        let rel = entry
            .path()
            .strip_prefix(&root_owned)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        rel.is_empty() || !is_denied(&rel, policy)
    });
    builder
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|entry| {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            Language::of_path(&rel).map(|_| rel)
        })
        .collect()
}

/// Build (or refresh) the graph of the tree at `root` into the source `label`. Generated files
/// are left out unless [`INCLUDE_GENERATED_ENV`] asks for them; see [`build_graph_with`].
///
/// # Errors
///
/// Database failures; unparsable files are counted, not fatal.
pub fn build_graph(
    db: &mut HomeDb,
    label: &str,
    root: &Path,
    options: &ScanOptions,
) -> Result<(Graph, BuildReport)> {
    build_graph_with(db, label, root, &BuildOptions::from_env(options.clone()))
}

/// Build (or refresh) the graph of the tree at `root` into the source `label`.
///
/// # Errors
///
/// Database failures; unparsable files are counted, not fatal.
pub fn build_graph_with(
    db: &mut HomeDb,
    label: &str,
    root: &Path,
    options: &BuildOptions,
) -> Result<(Graph, BuildReport)> {
    let root = &absolute_root(root);
    let source = db.upsert_source("code-graph", label, Some(&root.to_string_lossy()))?;
    let run = db.begin_run(Some(source), "graph")?;
    let result = build_inner(db, source, label, root, options);
    match &result {
        Ok((_, report)) => db.finish_run(
            run,
            crate::localstore::RunStatus::Succeeded,
            &serde_json::to_value(report)?,
            None,
        )?,
        Err(error) => db.finish_run(
            run,
            crate::localstore::RunStatus::Failed,
            &serde_json::json!({}),
            Some(&error.to_string()),
        )?,
    }
    result
}

/// What the cache holds for one file.
struct CachedRow {
    blake3: String,
    /// The file was detected as generated when the row was written.
    generated: bool,
    /// Only the generated marker was stored, the file was not extracted.
    skipped: bool,
}

#[allow(clippy::too_many_lines)]
fn build_inner(
    db: &mut HomeDb,
    source: i64,
    label: &str,
    root: &Path,
    options: &BuildOptions,
) -> Result<(Graph, BuildReport)> {
    let mut report = BuildReport {
        scan: scan_tree(db, source, root, &options.scan)?,
        ..BuildReport::default()
    };

    // Drop cache rows of files that are gone or whose hash changed.
    db.conn().execute(
        "DELETE FROM graph_file_cache WHERE source_id = ?1 AND NOT EXISTS (
             SELECT 1 FROM files f WHERE f.source_id = graph_file_cache.source_id
               AND f.rel_path = graph_file_cache.rel_path AND f.blake3 = graph_file_cache.blake3)",
        [source],
    )?;
    db.conn().execute(
        "DELETE FROM graph_file_cache WHERE source_id = ?1
           AND COALESCE(json_extract(extract_json, '$.version'), 0) <> ?2",
        params![source, EXTRACT_VERSION],
    )?;

    // The files the graph covers: the right class, and not ignored by `.gitignore`, `.rgignore`,
    // `.fdignore` or `.graphifyignore`.
    let allowed = walk_allowed(root, &options.scan);
    let all_files = db.stored_files(source)?;
    let mut wanted: Vec<(String, String)> = Vec::new();
    for (path, (hash, _, _, class, _)) in &all_files {
        let Some(language) = Language::of_path(path) else {
            continue;
        };
        let class_ok = match language {
            Language::Markdown => class == "doc",
            _ => class == "code",
        };
        let generated_class = class == "generated";
        if !allowed.contains(path) {
            report.ignored += 1;
        } else if generated_class && !options.include_generated {
            report.generated_skipped += 1;
        } else if class_ok || generated_class {
            wanted.push((path.clone(), hash.clone()));
        }
    }
    let wanted_paths: HashSet<&str> = wanted.iter().map(|(p, _)| p.as_str()).collect();
    let mut cached_rows: HashMap<String, CachedRow> = HashMap::new();
    {
        let mut statement = db.conn().prepare(
            "SELECT rel_path, blake3,
                    json_extract(extract_json, '$.generated') IS NOT NULL,
                    COALESCE(json_extract(extract_json, '$.skipped'), 0)
             FROM graph_file_cache WHERE source_id = ?1",
        )?;
        let rows = statement.query_map([source], |r| {
            Ok((
                r.get::<_, String>(0)?,
                CachedRow {
                    blake3: r.get(1)?,
                    generated: r.get(2)?,
                    skipped: r.get::<_, i64>(3)? != 0,
                },
            ))
        })?;
        for row in rows {
            let (path, row) = row?;
            cached_rows.insert(path, row);
        }
    }
    // Rows of files that left the graph (ignored since the last build).
    for path in cached_rows.keys().filter(|p| !wanted_paths.contains(p.as_str())) {
        db.conn().execute(
            "DELETE FROM graph_file_cache WHERE source_id = ?1 AND rel_path = ?2",
            params![source, path],
        )?;
    }
    let todo: Vec<(String, String)> = wanted
        .into_iter()
        .filter(|(path, hash)| {
            !cached_rows.get(path).is_some_and(|row| {
                row.blake3 == *hash && (!row.generated || row.skipped != options.include_generated)
            })
        })
        .collect();
    for (rel_path, blake3) in todo {
        let Ok(bytes) = std::fs::read(root.join(&rel_path)) else {
            report.errors += 1;
            continue;
        };
        let head = &bytes[..bytes.len().min(4096)];
        let reason = generated::detect(&rel_path, head).map(str::to_owned);
        let extract = match reason {
            Some(reason) if !options.include_generated => FileExtract {
                generated: Some(reason),
                skipped: true,
                version: EXTRACT_VERSION,
                ..FileExtract::default()
            },
            reason => {
                let mut extract = extract_file(&rel_path, &bytes);
                extract.generated = reason;
                extract
            }
        };
        if extract.error.is_some() {
            report.errors += 1;
        }
        db.conn().execute(
            "INSERT OR REPLACE INTO graph_file_cache (source_id, rel_path, blake3, extract_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![source, rel_path, blake3, serde_json::to_string(&extract)?],
        )?;
        db.complete_stage(source, &rel_path, &blake3, Stage::Extract)?;
        report.extracted += 1;
    }

    let mut files: Vec<FileExtract> = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    {
        let mut statement = db.conn().prepare(
            "SELECT extract_json FROM graph_file_cache WHERE source_id = ?1 ORDER BY rel_path",
        )?;
        let rows = statement.query_map([source], |r| r.get::<_, String>(0))?;
        for row in rows {
            let extract: FileExtract = serde_json::from_str(&row?)?;
            if extract.skipped {
                report.generated_skipped += 1;
                continue;
            }
            if let Some(file) = extract
                .nodes
                .iter()
                .find(|n| n.class == crate::codegraph::extract::NodeClass::File)
            {
                paths.push(file.file.clone());
            }
            files.push(extract);
        }
    }
    report.cached = files.len().saturating_sub(report.extracted);
    let mut context = packages_of(root, &paths);
    context.ts_configs = ts_configs_of(root, all_files.keys());
    let (mut graph, stats) = assemble_with(files, &context);
    report.assemble = stats;
    report.modularity = graph.detect_communities();
    report.nodes = graph.nodes.len();
    report.edges = graph.edges.len();
    let source = graph.save(db, label, Some(&root.to_string_lossy()))?;
    report.built_at_commit = head_commit(root);
    db.conn().execute(
        "UPDATE sources SET meta_json = json_set(
             CASE WHEN ?2 IS NULL THEN json_remove(meta_json, '$.built_at_commit')
                  ELSE json_set(meta_json, '$.built_at_commit', ?2) END,
             '$.generated_skipped', ?3)
         WHERE id = ?1",
        params![
            source,
            report.built_at_commit,
            i64::try_from(report.generated_skipped).unwrap_or(i64::MAX)
        ],
    )?;
    graph.meta = GraphMeta {
        root: Some(root.to_string_lossy().into_owned()),
        built_at_commit: report.built_at_commit.clone(),
        generated_skipped: Some(report.generated_skipped),
    };
    Ok((graph, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/a.rs"),
            "pub struct Thing;\nfn run() { helper(); }\nfn helper() {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/b.rs"),
            "fn use_it(t: Thing) { remote(); }\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("src/c.rs"), "pub fn remote() {}\n").unwrap();
        std::fs::write(dir.path().join("README.md"), "# not code").unwrap();
        dir
    }

    #[test]
    fn the_first_build_extracts_everything_and_the_second_parses_nothing() {
        let dir = repo();
        let mut db = HomeDb::open_in_memory().unwrap();
        let (g1, r1) =
            build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        assert_eq!(r1.extracted, 4, "three Rust files and README.md");
        assert!(
            g1.nodes
                .iter()
                .any(|n| n.label == "not code" && n.kind == crate::codegraph::graph::NodeKind::Document)
        );
        assert!(g1.nodes.iter().any(|n| n.label == "run()"));
        assert!(
            g1.edges
                .iter()
                .any(|e| e.relation == "calls" && g1.nodes[e.dst].label == "remote()")
        );
        let (g2, r2) =
            build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        assert_eq!((r2.extracted, r2.cached), (0, 4));
        assert_eq!(
            (g1.nodes.len(), g1.edges.len()),
            (g2.nodes.len(), g2.edges.len())
        );
        let stored: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM graph_nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored as usize, g2.nodes.len());
        assert_eq!(
            db.last_run("graph").unwrap().unwrap().status,
            crate::localstore::RunStatus::Succeeded
        );
    }

    #[test]
    fn only_changed_files_are_parsed_again() {
        let dir = repo();
        let mut db = HomeDb::open_in_memory().unwrap();
        build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        std::fs::write(
            dir.path().join("src/c.rs"),
            "pub fn remote() {}\npub fn extra() {}\n",
        )
        .unwrap();
        let (g, r) = build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        assert_eq!(r.extracted, 1);
        assert_eq!(r.cached, 3);
        assert!(g.nodes.iter().any(|n| n.label == "extra()"));
        std::fs::remove_file(dir.path().join("src/c.rs")).unwrap();
        let (g, _) = build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        assert!(!g.nodes.iter().any(|n| n.label == "remote()"));
    }

    #[test]
    fn rows_of_an_older_extractor_are_parsed_again_and_packages_come_from_manifests() {
        let dir = repo();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]
name = \"my-crate\"
",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("web")).unwrap();
        std::fs::write(dir.path().join("web/package.json"), r#"{"name":"@x/web"}"#).unwrap();
        std::fs::write(
            dir.path().join("web/main.ts"),
            "export function go() {}
",
        )
        .unwrap();
        let mut db = HomeDb::open_in_memory().unwrap();
        let (g, _) = build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        let package = |label: &str| {
            g.nodes
                .iter()
                .find(|n| n.label == label)
                .and_then(|n| n.package.clone())
        };
        assert_eq!(package("run()").as_deref(), Some("my_crate"));
        assert_eq!(package("go()").as_deref(), Some("@x/web"));
        let reloaded = Graph::load(&db, "graph:t").unwrap();
        assert!(
            reloaded
                .nodes
                .iter()
                .any(|n| n.package.as_deref() == Some("@x/web"))
        );
        db.conn()
            .execute(
                "UPDATE graph_file_cache SET extract_json = json_set(extract_json, '$.version', 1)",
                [],
            )
            .unwrap();
        let (_, r) = build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        assert_eq!(r.cached, 0);
        let root: String = db
            .conn()
            .query_row(
                "SELECT root FROM sources WHERE label = 'graph:t'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            Path::new(&root).is_absolute() && !root.starts_with(r"\\?\"),
            "{root}"
        );
    }

    #[test]
    fn a_resumed_build_parses_only_files_missing_a_cache_row() {
        let dir = repo();
        let mut db = HomeDb::open_in_memory().unwrap();
        build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        db.conn()
            .execute(
                "DELETE FROM graph_file_cache WHERE rel_path = 'src/b.rs'",
                [],
            )
            .unwrap();
        let (_, r) = build_graph(&mut db, "graph:t", dir.path(), &ScanOptions::default()).unwrap();
        assert_eq!(r.extracted, 1);
    }

    fn write(dir: &Path, path: &str, text: &str) {
        let full = dir.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    fn labels(g: &Graph) -> Vec<&str> {
        let mut out: Vec<&str> = g
            .nodes
            .iter()
            .filter(|n| n.kind == crate::codegraph::graph::NodeKind::Code && n.file.is_some())
            .map(|n| n.label.as_str())
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn ignore_files_and_generated_code_leave_the_graph_unless_included() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "src/ok.rs", "pub fn kept() {}\n");
        write(root, "src/header.rs", "// This file is @generated by a tool.\npub fn from_header() {}\n");
        write(root, "src/pb/gen/proto.rs", "pub fn from_pb_gen() {}\n");
        write(root, "cloudflare/worker-configuration.d.ts", "// Generated by Wrangler by running `wrangler types`\ninterface Env {}\n");
        write(root, "skipped_by_rg/a.rs", "pub fn from_rgignore() {}\n");
        write(root, "fd_only.rs", "pub fn from_fdignore() {}\n");
        write(root, "git_only.rs", "pub fn from_gitignore() {}\n");
        write(root, ".rgignore", "skipped_by_rg/\n");
        write(root, ".fdignore", "fd_only.rs\n");
        write(root, ".gitignore", "git_only.rs\n");
        let mut db = HomeDb::open_in_memory().unwrap();
        let excluded = BuildOptions::default();
        let (g, r) = build_graph_with(&mut db, "graph:g", root, &excluded).unwrap();
        assert_eq!(labels(&g), ["kept()", "ok.rs"]);
        assert_eq!((r.ignored, r.generated_skipped), (3, 3), "{r:?}");
        assert_eq!(g.meta.generated_skipped, Some(3));
        assert!(g.report("g", 5).contains("- Generated files left out: 3"));

        let included = BuildOptions {
            include_generated: true,
            ..BuildOptions::default()
        };
        let (g, r) = build_graph_with(&mut db, "graph:g", root, &included).unwrap();
        assert_eq!(
            labels(&g),
            ["Env", "from_header()", "from_pb_gen()", "header.rs", "kept()", "ok.rs", "proto.rs", "worker-configuration.d.ts"]
        );
        assert_eq!(r.generated_skipped, 0);
        let marked: Vec<&str> = g.nodes.iter().filter(|n| n.generated).map(|n| n.label.as_str()).collect();
        assert!(marked.contains(&"from_pb_gen()") && marked.contains(&"Env") && !marked.contains(&"kept()"), "{marked:?}");
        let hubs: Vec<&str> = g.god_nodes(20).iter().map(|&(i, _)| g.nodes[i].label.as_str()).collect();
        assert!(!hubs.iter().any(|l| marked.contains(l)), "generated nodes are never hubs");

        // The cache serves both modes correctly in either order.
        let (g, r) = build_graph_with(&mut db, "graph:g", root, &excluded).unwrap();
        assert_eq!(labels(&g), ["kept()", "ok.rs"]);
        assert_eq!(r.generated_skipped, 3);
        let reloaded = Graph::load(&db, "graph:g").unwrap();
        assert_eq!(labels(&reloaded), ["kept()", "ok.rs"]);
        assert_eq!(reloaded.meta.generated_skipped, Some(3));
    }

    #[test]
    fn manifests_give_dependencies_and_tsconfigs_give_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "crates/app/Cargo.toml",
            "[package]\nname = \"app-core\"\n[dependencies]\nserde_json = \"1\"\ntokio.workspace = true\n\
             [target.\"cfg(not(target_family = \\\"wasm\\\"))\".dependencies]\nreqwest = { version = \"1\", optional = true }\n\
             [dev-dependencies.tempfile]\nversion = \"3\"\n",
        );
        write(
            root,
            "crates/app/src/lib.rs",
            "pub fn go() { serde_json::from_str(\"1\"); reqwest::get(\"u\"); not_a_dependency::run(); }\n",
        );
        write(
            root,
            "web/tsconfig.json",
            "{ // comments are fine\n \"extends\": \"../tsconfig.base.json\", \"compilerOptions\": { \"baseUrl\": \".\", },\n}\n",
        );
        write(root, "tsconfig.base.json", r#"{"compilerOptions":{"paths":{"@ui/*":["web/ui/*"]}}}"#);
        write(root, "web/ui/button.ts", "export function press() {}\n");
        write(root, "web/app.ts", "import { press } from '@ui/button';\nexport function main() { press(); }\n");
        let mut db = HomeDb::open_in_memory().unwrap();
        let (g, r) = build_graph_with(&mut db, "graph:m", root, &BuildOptions::default()).unwrap();
        let calls: Vec<String> = g
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| format!("{} -> {}", g.nodes[e.src].label, g.nodes[e.dst].label))
            .collect();
        assert!(calls.contains(&"go() -> serde_json::from_str".to_owned()), "{calls:?}");
        assert!(calls.contains(&"go() -> reqwest::get".to_owned()), "{calls:?}");
        assert!(!calls.iter().any(|c| c.contains("not_a_dependency")), "{calls:?}");
        assert!(calls.contains(&"main() -> press()".to_owned()), "{calls:?}");
        assert_eq!(r.assemble.unresolved_by.get("path_unresolved"), Some(&1), "{r:?}");
        assert_eq!(
            manifest_deps("[dependencies]\na-b = 1\n[build-dependencies]\nc = { package = \"d\" }\n[features]\nx = []\n"),
            ["a_b", "c"].into_iter().map(str::to_owned).collect()
        );
    }
}
