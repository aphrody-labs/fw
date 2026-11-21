//! The SQLite schema of the graph and the writer that merges the per-file extractions into it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::graph::{FileOut, Language, NO_SOURCE, SCHEMA_VERSION, SYMBOL_KINDS, SymbolKind, TableView};

pub struct Identity {
    pub label: String,
    pub version: Option<String>,
    pub commit: Option<String>,
}

#[derive(Default)]
pub struct Counts {
    pub files: usize,
    pub files_with_parse_errors: usize,
    pub parse_errors: u64,
    pub symbols: usize,
    pub symbols_by_kind: BTreeMap<&'static str, usize>,
    pub includes: usize,
    pub includes_resolved: usize,
    pub calls: usize,
    pub refs: usize,
    pub exports: usize,
    pub registrations: usize,
    pub config_edges: usize,
    pub build_edges: usize,
    pub names: usize,
}

impl Counts {
    pub fn to_json(&self) -> Value {
        json!({
            "files": self.files,
            "files_with_parse_errors": self.files_with_parse_errors,
            "parse_errors": self.parse_errors,
            "symbols": self.symbols,
            "symbols_by_kind": self.symbols_by_kind,
            "includes": self.includes,
            "includes_resolved": self.includes_resolved,
            "calls": self.calls,
            "refs": self.refs,
            "exports": self.exports,
            "registrations": self.registrations,
            "config_edges": self.config_edges,
            "build_edges": self.build_edges,
            "names": self.names,
        })
    }
}

const SCHEMA: &str = "
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
CREATE TABLE names(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE kinds(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE files(id INTEGER PRIMARY KEY, path TEXT NOT NULL, language TEXT NOT NULL, bytes INTEGER NOT NULL, lines INTEGER NOT NULL, parse_errors INTEGER NOT NULL);
CREATE TABLE symbols(id INTEGER PRIMARY KEY, name_id INTEGER NOT NULL, kind INTEGER NOT NULL, file_id INTEGER NOT NULL, line INTEGER NOT NULL, end_line INTEGER NOT NULL, signature TEXT NOT NULL, storage TEXT NOT NULL, inline INTEGER NOT NULL, exported INTEGER NOT NULL);
CREATE TABLE includes(id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL, line INTEGER NOT NULL, target TEXT NOT NULL, system INTEGER NOT NULL, kind TEXT NOT NULL, resolved INTEGER);
CREATE TABLE calls(caller INTEGER NOT NULL, callee_id INTEGER NOT NULL, line INTEGER NOT NULL);
CREATE TABLE refs(source INTEGER NOT NULL, name_id INTEGER NOT NULL, kind INTEGER NOT NULL, line INTEGER NOT NULL);
CREATE TABLE exports(file_id INTEGER NOT NULL, line INTEGER NOT NULL, name_id INTEGER NOT NULL, license TEXT NOT NULL, namespace TEXT NOT NULL);
CREATE TABLE registrations(file_id INTEGER NOT NULL, line INTEGER NOT NULL, macro TEXT NOT NULL, name_id INTEGER NOT NULL);
CREATE TABLE config_edges(source INTEGER NOT NULL, kind TEXT NOT NULL, target_id INTEGER, expr TEXT NOT NULL, via TEXT NOT NULL, line INTEGER NOT NULL);
CREATE TABLE build_edges(file_id INTEGER NOT NULL, dir TEXT NOT NULL, line INTEGER NOT NULL, kind TEXT NOT NULL, config_id INTEGER, gate TEXT NOT NULL, condition TEXT NOT NULL, parent TEXT, target TEXT NOT NULL, source_file INTEGER);
";

const INDEXES: &str = "
CREATE UNIQUE INDEX names_name ON names(name);
CREATE UNIQUE INDEX files_path ON files(path);
CREATE INDEX symbols_name ON symbols(name_id);
CREATE INDEX symbols_file ON symbols(file_id, line);
CREATE INDEX includes_file ON includes(file_id);
CREATE INDEX includes_resolved ON includes(resolved);
CREATE INDEX calls_caller ON calls(caller);
CREATE INDEX calls_callee ON calls(callee_id);
CREATE INDEX refs_source ON refs(source);
CREATE INDEX refs_name ON refs(name_id);
CREATE INDEX exports_name ON exports(name_id);
CREATE INDEX registrations_name ON registrations(name_id);
CREATE INDEX config_edges_source ON config_edges(source);
CREATE INDEX config_edges_target ON config_edges(target_id);
CREATE INDEX build_edges_config ON build_edges(config_id);
CREATE INDEX build_edges_dir ON build_edges(dir);
CREATE INDEX build_edges_parent ON build_edges(parent);
";

const VIEWS: &str = "
CREATE VIEW v_symbols AS SELECT s.id, n.name, k.name AS kind, f.path AS file, s.line, s.end_line, s.signature, s.storage, s.inline, s.exported FROM symbols s JOIN names n ON n.id = s.name_id JOIN kinds k ON k.id = s.kind JOIN files f ON f.id = s.file_id;
CREATE VIEW v_calls AS SELECT c.caller, a.name AS caller_name, b.name AS callee, c.line FROM calls c JOIN symbols s ON s.id = c.caller JOIN names a ON a.id = s.name_id JOIN names b ON b.id = c.callee_id;
CREATE VIEW v_refs AS SELECT r.source, a.name AS source_name, b.name AS name, k.name AS kind, r.line FROM refs r JOIN symbols s ON s.id = r.source JOIN names a ON a.id = s.name_id JOIN names b ON b.id = r.name_id JOIN kinds k ON k.id = r.kind;
";

fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.to_owned(), source }
}

/// The commit a checkout is at, from `.git` (a directory, or a file pointing at the git dir of a submodule).
pub fn git_head(root: &Path) -> Option<String> {
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_file() {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        let target = Path::new(target);
        if target.is_absolute() { target.to_owned() } else { root.join(target) }
    } else {
        dot_git
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref: ") else {
        return Some(head.to_owned());
    };
    if let Ok(loose) = std::fs::read_to_string(git_dir.join(reference)) {
        return Some(loose.trim().to_owned());
    }
    let packed = std::fs::read_to_string(git_dir.join("packed-refs")).ok()?;
    packed.lines().find_map(|line| {
        let (sha, name) = line.split_once(' ')?;
        (name == reference).then(|| sha.to_owned())
    })
}

fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn directory_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(directory, _)| directory)
}

struct Resolver<'a> {
    files: HashMap<&'a str, u32>,
    architectures: Vec<&'a str>,
}

impl<'a> Resolver<'a> {
    fn new(outputs: &'a [FileOut]) -> Resolver<'a> {
        let files = outputs.iter().enumerate().map(|(index, file)| (file.path.as_str(), index as u32 + 1)).collect();
        let mut architectures: Vec<&str> = outputs.iter().filter_map(|file| file.path.strip_prefix("arch/")?.split('/').next()).collect();
        architectures.sort_unstable();
        architectures.dedup();
        Resolver { files, architectures }
    }

    fn lookup(&self, path: &str) -> Option<u32> {
        self.files.get(normalize(path).as_str()).copied()
    }

    fn include(&self, from: &str, target: &str, system: bool, kind: &str) -> Option<u32> {
        if kind == "source" {
            return self.lookup(target).or_else(|| self.lookup(&format!("{}/{target}", directory_of(from))));
        }
        if !system {
            let mut directory = directory_of(from);
            loop {
                if let Some(id) = self.lookup(&format!("{directory}/{target}")) {
                    return Some(id);
                }
                if directory.is_empty() {
                    break;
                }
                directory = directory_of(directory);
            }
        }
        let own = from.strip_prefix("arch/").and_then(|rest| rest.split('/').next());
        let mut bases: Vec<String> = Vec::new();
        if let Some(architecture) = own {
            bases.push(format!("arch/{architecture}/include"));
            bases.push(format!("arch/{architecture}/include/generated"));
        }
        bases.push("include".to_owned());
        if let Some(architecture) = own {
            bases.push(format!("arch/{architecture}/include/uapi"));
            bases.push(format!("arch/{architecture}/include/generated/uapi"));
        }
        bases.push("include/uapi".to_owned());
        if let Some(id) = bases.iter().find_map(|base| self.lookup(&format!("{base}/{target}"))) {
            return Some(id);
        }
        let mut found = None;
        for architecture in &self.architectures {
            let hit = ["include", "include/uapi", "include/generated"].iter().find_map(|base| self.lookup(&format!("arch/{architecture}/{base}/{target}")));
            if hit.is_some() {
                if found.is_some() {
                    return None;
                }
                found = hit;
            }
        }
        found
    }

    fn object_source(&self, object: &str) -> Option<u32> {
        let stem = object.strip_suffix(".o")?;
        ["c", "S"].iter().find_map(|extension| self.lookup(&format!("{stem}.{extension}")))
    }
}

/// Civil date of a unix time, RFC 3339.
fn timestamp(seconds: u64) -> String {
    let days = (seconds / 86400) as i64;
    let rest = seconds % 86400;
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

pub fn write(db: &Path, identity: &Identity, outputs: &[FileOut], names: &TableView<'_>, _jobs: usize) -> Result<Counts> {
    let mut temporary = db.as_os_str().to_owned();
    temporary.push(".building");
    let temporary = PathBuf::from(temporary);
    if let Some(parent) = db.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    }
    for stale in [temporary.clone(), PathBuf::from(format!("{}-journal", temporary.display()))] {
        if stale.exists() {
            std::fs::remove_file(&stale).map_err(|source| io_error(&stale, source))?;
        }
    }

    let total = names.total();
    let mut mask = vec![0_u16; total];
    let mut retained = vec![false; total];
    for file in outputs {
        let extracted = &file.extracted;
        for symbol in &extracted.symbols {
            let dense = names.dense(extracted.global[symbol.name as usize]);
            retained[dense] = true;
            if symbol.kind.is_c() {
                mask[dense] |= 1 << symbol.kind.code();
            }
        }
    }
    for file in outputs {
        let extracted = &file.extracted;
        for usage in &extracted.uses {
            let dense = names.dense(extracted.global[usage.name as usize]);
            if usage.source != NO_SOURCE && (usage.call || mask[dense] != 0) {
                retained[dense] = true;
            }
        }
        let others = extracted
            .exports
            .iter()
            .map(|row| row.name)
            .chain(extracted.registrations.iter().map(|row| row.name))
            .chain(extracted.config_edges.iter().filter_map(|row| row.target))
            .chain(extracted.build_edges.iter().filter_map(|row| row.config));
        for local in others {
            retained[names.dense(extracted.global[local as usize])] = true;
        }
    }
    let mut order: Vec<u32> = names.ids().filter(|&id| retained[names.dense(id)]).collect();
    order.sort_unstable_by(|left, right| names.text(*left).cmp(names.text(*right)));
    let mut final_id = vec![0_u32; total];
    for (rank, &id) in order.iter().enumerate() {
        final_id[names.dense(id)] = rank as u32 + 1;
    }

    let mut connection = Connection::open(&temporary)?;
    connection.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA locking_mode=EXCLUSIVE; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-262144;")?;
    connection.execute_batch(SCHEMA)?;
    let transaction = connection.transaction()?;
    let resolver = Resolver::new(outputs);
    let mut counts = Counts::default();
    {
        let mut insert_name = transaction.prepare("INSERT INTO names(id, name) VALUES (?1, ?2)")?;
        for (rank, &id) in order.iter().enumerate() {
            insert_name.execute(params![rank as i64 + 1, names.text(id)])?;
        }
        counts.names = order.len();
        let mut insert_kind = transaction.prepare("INSERT INTO kinds(id, name) VALUES (?1, ?2)")?;
        for kind in SYMBOL_KINDS {
            insert_kind.execute(params![i64::from(kind.code()), kind.as_str()])?;
        }
        let mut insert_file = transaction.prepare("INSERT INTO files(id, path, language, bytes, lines, parse_errors) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
        let mut insert_symbol = transaction.prepare("INSERT INTO symbols(id, name_id, kind, file_id, line, end_line, signature, storage, inline, exported) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")?;
        let mut insert_include = transaction.prepare("INSERT INTO includes(file_id, line, target, system, kind, resolved) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
        let mut insert_call = transaction.prepare("INSERT INTO calls(caller, callee_id, line) VALUES (?1, ?2, ?3)")?;
        let mut insert_ref = transaction.prepare("INSERT INTO refs(source, name_id, kind, line) VALUES (?1, ?2, ?3, ?4)")?;
        let mut insert_export = transaction.prepare("INSERT INTO exports(file_id, line, name_id, license, namespace) VALUES (?1, ?2, ?3, ?4, ?5)")?;
        let mut insert_registration = transaction.prepare("INSERT INTO registrations(file_id, line, macro, name_id) VALUES (?1, ?2, ?3, ?4)")?;
        let mut insert_config = transaction.prepare("INSERT INTO config_edges(source, kind, target_id, expr, via, line) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
        let mut insert_build = transaction.prepare("INSERT INTO build_edges(file_id, dir, line, kind, config_id, gate, condition, parent, target, source_file) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)")?;

        let mut next_symbol: i64 = 1;
        for (index, file) in outputs.iter().enumerate() {
            let file_id = index as i64 + 1;
            let extracted = &file.extracted;
            let new_name = |local: u32| i64::from(final_id[names.dense(extracted.global[local as usize])]);
            insert_file.execute(params![file_id, file.path, file.language.as_str(), file.bytes as i64, file.lines, extracted.parse_errors])?;
            counts.files += 1;
            if extracted.parse_errors > 0 {
                counts.files_with_parse_errors += 1;
                counts.parse_errors += u64::from(extracted.parse_errors);
            }
            let base = next_symbol;
            for symbol in &extracted.symbols {
                insert_symbol.execute(params![
                    next_symbol,
                    new_name(symbol.name),
                    i64::from(symbol.kind.code()),
                    file_id,
                    symbol.line,
                    symbol.end_line,
                    symbol.signature,
                    symbol.storage,
                    symbol.inline,
                    symbol.exported,
                ])?;
                *counts.symbols_by_kind.entry(symbol.kind.as_str()).or_default() += 1;
                next_symbol += 1;
            }
            counts.symbols += extracted.symbols.len();
            for include in &extracted.includes {
                let resolved = resolver.include(&file.path, &include.target, include.system, include.kind);
                insert_include.execute(params![file_id, include.line, include.target, include.system, include.kind, resolved])?;
                counts.includes += 1;
                counts.includes_resolved += usize::from(resolved.is_some());
            }
            for usage in &extracted.uses {
                if usage.source == NO_SOURCE {
                    continue;
                }
                let source = base + i64::from(usage.source);
                let global = extracted.global[usage.name as usize];
                let name_id = i64::from(final_id[names.dense(global)]);
                if usage.call {
                    insert_call.execute(params![source, name_id, usage.line])?;
                    counts.calls += 1;
                    continue;
                }
                let kinds = mask[names.dense(global)];
                let own = extracted.symbols.get(usage.source as usize).is_some_and(|symbol| symbol.name == usage.name);
                if kinds == 0 || own {
                    continue;
                }
                for kind in SYMBOL_KINDS {
                    if kinds & (1 << kind.code()) != 0 {
                        insert_ref.execute(params![source, name_id, i64::from(kind.code()), usage.line])?;
                        counts.refs += 1;
                    }
                }
            }
            for export in &extracted.exports {
                insert_export.execute(params![file_id, export.line, new_name(export.name), export.license, export.namespace])?;
                counts.exports += 1;
            }
            for registration in &extracted.registrations {
                insert_registration.execute(params![file_id, registration.line, registration.macro_name, new_name(registration.name)])?;
                counts.registrations += 1;
            }
            for edge in &extracted.config_edges {
                let target = edge.target.map(new_name);
                insert_config.execute(params![base + i64::from(edge.source), edge.kind, target, edge.expr, edge.via, edge.line])?;
                counts.config_edges += 1;
            }
            for edge in &extracted.build_edges {
                let source_file = resolver.object_source(&edge.target);
                insert_build.execute(params![
                    file_id,
                    directory_of(&file.path),
                    edge.line,
                    edge.kind,
                    edge.config.map(new_name),
                    edge.gate,
                    edge.condition,
                    edge.parent,
                    edge.target,
                    source_file
                ])?;
                counts.build_edges += 1;
            }
        }

        let languages = outputs.iter().filter(|file| file.language == Language::C).count();
        let built_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_secs());
        let mut insert_meta = transaction.prepare("INSERT INTO meta(key, value) VALUES (?1, ?2)")?;
        let mut meta = vec![
            ("schema_version".to_owned(), SCHEMA_VERSION.to_string()),
            ("builder_version".to_owned(), env!("CARGO_PKG_VERSION").to_owned()),
            ("built_at".to_owned(), timestamp(built_at)),
            ("label".to_owned(), identity.label.clone()),
            ("c_files".to_owned(), languages.to_string()),
        ];
        if let Some(version) = &identity.version {
            meta.push(("version".to_owned(), version.clone()));
        }
        if let Some(commit) = &identity.commit {
            meta.push(("commit".to_owned(), commit.clone()));
        }
        meta.push(("counts".to_owned(), counts.to_json().to_string()));
        for (key, value) in meta {
            insert_meta.execute(params![key, value])?;
        }
    }
    transaction.commit()?;
    connection.execute_batch(INDEXES)?;
    connection.execute_batch(VIEWS)?;
    connection.close().map_err(|(_, error)| Error::Sqlite(error))?;

    for stale in [db.to_owned(), PathBuf::from(format!("{}-journal", db.display()))] {
        if stale.exists() {
            std::fs::remove_file(&stale).map_err(|source| io_error(&stale, source))?;
        }
    }
    std::fs::rename(&temporary, db).map_err(|source| io_error(db, source))?;
    Ok(counts)
}

#[allow(dead_code)]
fn kind_name(code: u8) -> Option<&'static str> {
    SYMBOL_KINDS.iter().find(|kind| kind.code() == code).map(|kind| SymbolKind::as_str(*kind))
}
