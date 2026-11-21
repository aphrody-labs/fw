//! The code graph of a kernel tree: one SQLite file built from tree-sitter parses of the C, Kconfig and Kbuild sources, queried by `fw kernel graph`.
//!
//! Resolution is name-based: nothing is preprocessed and no type is resolved, so a name that is defined several times (static functions,
//! per-architecture symbols) matches every definition, and an identifier is a reference to every known symbol that carries its name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::graph_c::{self, Parsers};
use crate::graph_store::{self, Counts};

pub const SCHEMA_VERSION: u32 = 1;
const DEFAULT_LIMIT: usize = 50;
const MAX_FILE_BYTES: u64 = 32 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    C,
    Asm,
    Kconfig,
    Make,
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Language::C => "c",
            Language::Asm => "asm",
            Language::Kconfig => "kconfig",
            Language::Make => "make",
        }
    }

    fn of(file_name: &str) -> Option<Language> {
        if file_name.ends_with(".c") || file_name.ends_with(".h") {
            Some(Language::C)
        } else if file_name.ends_with(".S") {
            Some(Language::Asm)
        } else if file_name.starts_with("Kconfig") || file_name.ends_with(".kconfig") {
            Some(Language::Kconfig)
        } else if file_name == "Makefile"
            || file_name == "Kbuild"
            || file_name == "GNUmakefile"
            || file_name.starts_with("Makefile.")
            || file_name.starts_with("Kbuild.")
            || file_name.ends_with(".mk")
        {
            Some(Language::Make)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SymbolKind {
    Function = 1,
    Struct = 2,
    Union = 3,
    Enum = 4,
    Enumerator = 5,
    Typedef = 6,
    Macro = 7,
    MacroFunction = 8,
    Variable = 9,
    Kconfig = 10,
    Menu = 11,
}

pub const SYMBOL_KINDS: [SymbolKind; 11] = [
    SymbolKind::Function,
    SymbolKind::Struct,
    SymbolKind::Union,
    SymbolKind::Enum,
    SymbolKind::Enumerator,
    SymbolKind::Typedef,
    SymbolKind::Macro,
    SymbolKind::MacroFunction,
    SymbolKind::Variable,
    SymbolKind::Kconfig,
    SymbolKind::Menu,
];

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Struct => "struct",
            SymbolKind::Union => "union",
            SymbolKind::Enum => "enum",
            SymbolKind::Enumerator => "enumerator",
            SymbolKind::Typedef => "typedef",
            SymbolKind::Macro => "macro",
            SymbolKind::MacroFunction => "macro_function",
            SymbolKind::Variable => "variable",
            SymbolKind::Kconfig => "kconfig",
            SymbolKind::Menu => "menu",
        }
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    /// Kinds that C identifiers can refer to.
    pub fn is_c(self) -> bool {
        !matches!(self, SymbolKind::Kconfig | SymbolKind::Menu)
    }
}

/// Index of a name in the per-file table, before the names of all files are merged.
pub type LocalName = u32;
pub const NO_SOURCE: u32 = u32::MAX;

#[derive(Default)]
pub struct LocalNames {
    map: HashMap<String, u32>,
    names: Vec<String>,
}

impl LocalNames {
    pub fn intern(&mut self, name: &str) -> LocalName {
        if let Some(&id) = self.map.get(name) {
            return id;
        }
        let id = self.names.len() as u32;
        self.names.push(name.to_owned());
        self.map.insert(name.to_owned(), id);
        id
    }
}

pub struct SymbolRow {
    pub name: LocalName,
    pub kind: SymbolKind,
    pub line: u32,
    pub end_line: u32,
    pub signature: String,
    pub storage: &'static str,
    pub inline: bool,
    pub exported: bool,
}

pub struct IncludeRow {
    pub target: String,
    pub system: bool,
    pub kind: &'static str,
    pub line: u32,
}

/// An identifier use inside the symbol `source` (a local symbol index): a call when `call`, else a plain reference.
pub struct UseRow {
    pub source: u32,
    pub name: LocalName,
    pub call: bool,
    pub line: u32,
}

pub struct ExportRow {
    pub name: LocalName,
    pub line: u32,
    pub license: &'static str,
    pub namespace: String,
}

pub struct RegistrationRow {
    pub name: LocalName,
    pub line: u32,
    pub macro_name: String,
}

pub struct ConfigEdgeRow {
    pub source: u32,
    pub kind: &'static str,
    pub target: Option<LocalName>,
    pub expr: String,
    pub via: &'static str,
    pub line: u32,
}

pub struct BuildEdgeRow {
    pub line: u32,
    pub kind: String,
    pub config: Option<LocalName>,
    pub gate: String,
    pub condition: String,
    pub parent: Option<String>,
    pub target: String,
}

#[derive(Default)]
pub struct Extracted {
    pub parse_errors: u32,
    pub names: LocalNames,
    pub symbols: Vec<SymbolRow>,
    pub includes: Vec<IncludeRow>,
    pub uses: Vec<UseRow>,
    pub exports: Vec<ExportRow>,
    pub registrations: Vec<RegistrationRow>,
    pub config_edges: Vec<ConfigEdgeRow>,
    pub build_edges: Vec<BuildEdgeRow>,
    /// Filled by `finish`: the global name id of each local name.
    pub global: Vec<u32>,
}

impl Extracted {
    pub fn name(&mut self, text: &str) -> LocalName {
        self.names.intern(text)
    }

    fn finish(&mut self, interner: &Interner) {
        self.global = self.names.names.iter().map(|name| interner.intern(name)).collect();
        self.names = LocalNames::default();
    }
}

const SHARD_BITS: u32 = 6;
const SHARDS: usize = 1 << SHARD_BITS;

#[derive(Default)]
struct Shard {
    map: HashMap<Box<str>, u32>,
    names: Vec<Box<str>>,
}

/// Concurrent string interner; ids are `index << 6 | shard`, later renumbered in sorted order.
pub struct Interner {
    shards: Vec<Mutex<Shard>>,
}

impl Interner {
    fn new() -> Interner {
        Interner { shards: (0..SHARDS).map(|_| Mutex::new(Shard::default())).collect() }
    }

    fn intern(&self, name: &str) -> u32 {
        let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3));
        let shard_index = (hash >> 32) as usize % SHARDS;
        let mut shard = self.shards[shard_index].lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(&id) = shard.map.get(name) {
            return id;
        }
        let id = ((shard.names.len() as u32) << SHARD_BITS) | shard_index as u32;
        shard.names.push(name.into());
        shard.map.insert(name.into(), id);
        id
    }

    fn into_table(self) -> NameTable {
        let shards: Vec<Vec<Box<str>>> =
            self.shards.into_iter().map(|shard| shard.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner).names).collect();
        let mut offsets = [0_usize; SHARDS];
        let mut total = 0;
        for (index, names) in shards.iter().enumerate() {
            offsets[index] = total;
            total += names.len();
        }
        NameTable { shards, offsets, total }
    }
}

struct NameTable {
    shards: Vec<Vec<Box<str>>>,
    offsets: [usize; SHARDS],
    total: usize,
}

impl NameTable {
    fn dense(&self, id: u32) -> usize {
        self.offsets[(id as usize) & (SHARDS - 1)] + (id >> SHARD_BITS) as usize
    }

    fn text(&self, id: u32) -> &str {
        &self.shards[(id as usize) & (SHARDS - 1)][(id >> SHARD_BITS) as usize]
    }
}

pub struct FileOut {
    pub path: String,
    pub language: Language,
    pub bytes: u64,
    pub lines: u32,
    pub extracted: Extracted,
}

struct Options {
    db: Option<PathBuf>,
    limit: usize,
    tree_root: Option<PathBuf>,
    jobs: Option<usize>,
    subdirs: Vec<String>,
    excludes: Vec<String>,
    label: Option<String>,
    commit: Option<String>,
    depth: usize,
    positional: Vec<String>,
}

fn parse_options(arguments: &[String]) -> Result<Options> {
    let mut options = Options {
        db: None,
        limit: DEFAULT_LIMIT,
        tree_root: None,
        jobs: None,
        subdirs: Vec::new(),
        excludes: Vec::new(),
        label: None,
        commit: None,
        depth: 12,
        positional: Vec::new(),
    };
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        let Some(flag) = argument.strip_prefix("--") else {
            options.positional.push(argument.clone());
            continue;
        };
        let (name, inline) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value.to_owned())),
            None => (flag, None),
        };
        let mut value = || -> Result<String> {
            match &inline {
                Some(value) => Ok(value.clone()),
                None => iterator.next().cloned().ok_or_else(|| Error::message(format!("graph: --{name} needs a value"))),
            }
        };
        let number = |text: String| -> Result<usize> {
            text.parse().map_err(|_| Error::message(format!("graph: --{name} needs a number, got `{text}`")))
        };
        match name {
            "db" => options.db = Some(PathBuf::from(value()?)),
            "limit" => options.limit = number(value()?)?.max(1),
            "tree-root" => options.tree_root = Some(PathBuf::from(value()?)),
            "jobs" => options.jobs = Some(number(value()?)?.max(1)),
            "subdir" => options.subdirs.push(value()?.trim_matches('/').to_owned()),
            "exclude" => options.excludes.push(value()?.trim_matches('/').to_owned()),
            "label" => options.label = Some(value()?),
            "commit" => options.commit = Some(value()?),
            "depth" => options.depth = number(value()?)?.clamp(1, 64),
            other => return Err(Error::message(format!("graph: unknown option --{other}"))),
        }
    }
    Ok(options)
}

/// `fw kernel graph <args>`: build or query the graph store at `default_db`.
pub fn cli(arguments: &[String], tree_root: &Path, default_db: &Path) -> Result<Value> {
    let Some((command, rest)) = arguments.split_first() else {
        return Err(Error::message(USAGE));
    };
    let options = parse_options(rest)?;
    let db = options.db.clone().unwrap_or_else(|| default_db.to_owned());
    if command == "build" {
        let root = options.tree_root.clone().unwrap_or_else(|| tree_root.to_owned());
        return build(&root, &db, &options);
    }
    if command == "probe" {
        return probe(tree_root, &options);
    }
    crate::graph_query::run(command, &options.positional, &db, options.limit, options.depth)
}

/// `probe FILE...`: parse C files and report symbols and error regions, without a database.
fn probe(root: &Path, options: &Options) -> Result<Value> {
    let mut parsers = Parsers::default();
    let mut files = Vec::new();
    for path in &options.positional {
        let full = root.join(path);
        let source = std::fs::read(&full).map_err(|source| Error::Io { path: full.clone(), source })?;
        let mut extracted = Extracted::default();
        graph_c::extract_c(&source, &mut parsers, &mut extracted)?;
        let functions = extracted.symbols.iter().filter(|symbol| symbol.kind == SymbolKind::Function).count();
        let errors: Vec<Value> = graph_c::error_snippets(&source, &mut parsers, options.limit)?.into_iter().map(|(line, text)| json!({ "line": line, "text": text })).collect();
        files.push(json!({ "file": path, "parse_errors": extracted.parse_errors, "symbols": extracted.symbols.len(), "functions": functions, "errors": errors }));
    }
    Ok(json!({ "ok": true, "command": "probe", "files": files }))
}

const USAGE: &str = "usage: fw kernel graph build [--tree-root DIR] [--db FILE] [--jobs N] [--subdir PATH ...] [--exclude PATH ...] [--label TEXT] [--commit SHA] | stats | def NAME | refs NAME | callers NAME | callees NAME | includes FILE | included-by FILE | file PATH | config SYMBOL | search PATTERN | path FROM TO [--depth N] | probe FILE...  (all: [--db FILE] [--limit N])";

fn collect_files(root: &Path, subdirs: &[String], excludes: &[String]) -> Result<Vec<(String, Language)>> {
    let starts: Vec<PathBuf> = if subdirs.is_empty() { vec![root.to_owned()] } else { subdirs.iter().map(|subdir| root.join(subdir)).collect() };
    let mut files = Vec::new();
    for start in starts {
        let walker = ignore::WalkBuilder::new(&start).standard_filters(false).hidden(true).follow_links(false).build();
        for entry in walker {
            let entry = entry.map_err(|error| Error::message(format!("walk {}: {error}", start.display())))?;
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(name) = entry.file_name().to_str() else { continue };
            let Some(language) = Language::of(name) else { continue };
            let Ok(relative) = entry.path().strip_prefix(root) else { continue };
            let path = relative.to_string_lossy().replace('\\', "/");
            if excludes.iter().any(|excluded| path == *excluded || path.starts_with(&format!("{excluded}/"))) {
                continue;
            }
            files.push((path, language));
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn process_file(root: &Path, path: &str, language: Language, parsers: &mut Parsers, interner: &Interner) -> Result<FileOut> {
    let full = root.join(path);
    let metadata = std::fs::metadata(&full).map_err(|source| Error::Io { path: full.clone(), source })?;
    let bytes = metadata.len();
    let mut extracted = Extracted::default();
    let mut lines = 0;
    if bytes <= MAX_FILE_BYTES {
        let source = std::fs::read(&full).map_err(|source| Error::Io { path: full.clone(), source })?;
        lines = source.iter().filter(|&&byte| byte == b'\n').count() as u32 + u32::from(source.last().is_some_and(|&byte| byte != b'\n'));
        match language {
            Language::C => graph_c::extract_c(&source, parsers, &mut extracted)?,
            Language::Asm => graph_c::extract_asm(&source, &mut extracted),
            Language::Kconfig => crate::graph_kconfig::extract(&source, parsers, &mut extracted)?,
            Language::Make => crate::graph_kbuild::extract(&source, path, parsers, &mut extracted)?,
        }
    }
    extracted.finish(interner);
    Ok(FileOut { path: path.to_owned(), language, bytes, lines, extracted })
}

/// The kernel version from the top Makefile and the checked-out commit from `.git`, both read without running git.
fn tree_identity(root: &Path) -> (Option<String>, Option<String>) {
    let version = std::fs::read_to_string(root.join("Makefile")).ok().and_then(|makefile| {
        let field = |name: &str| {
            makefile.lines().take(8).find_map(|line| line.strip_prefix(name).and_then(|rest| rest.trim_start().strip_prefix('=')).map(|value| value.trim().to_owned()))
        };
        Some(format!("{}.{}.{}{}", field("VERSION")?, field("PATCHLEVEL")?, field("SUBLEVEL")?, field("EXTRAVERSION").unwrap_or_default()))
    });
    (version, graph_store::git_head(root))
}

fn build(root: &Path, db: &Path, options: &Options) -> Result<Value> {
    if !root.is_dir() {
        return Err(Error::message(format!("graph build: tree root {} is not a directory", root.display())));
    }
    let started = Instant::now();
    let files = collect_files(root, &options.subdirs, &options.excludes)?;
    let walked = started.elapsed();

    let jobs = options.jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from));
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .stack_size(16 << 20)
        .build()
        .map_err(|error| Error::message(format!("graph build: thread pool: {error}")))?;
    let interner = Interner::new();
    let outputs: Vec<FileOut> = pool.install(|| {
        files
            .par_iter()
            .map_init(Parsers::default, |parsers, (path, language)| process_file(root, path, *language, parsers, &interner))
            .collect::<Result<Vec<_>>>()
    })?;
    let table = interner.into_table();
    let parsed = started.elapsed();

    let (version, commit) = tree_identity(root);
    let commit = options.commit.clone().or(commit);
    let label = options.label.clone().or_else(|| version.as_ref().map(|version| format!("v{version}"))).or_else(|| commit.clone()).unwrap_or_else(|| "unlabeled".to_owned());
    let identity = graph_store::Identity { label, version, commit };
    let bytes_read: u64 = outputs.iter().map(|file| file.bytes).sum();
    let counts: Counts = graph_store::write(db, &identity, &outputs, &table_view(&table), jobs)?;
    let total = started.elapsed();

    Ok(json!({
        "ok": true,
        "command": "build",
        "db": db.display().to_string(),
        "tree": { "label": identity.label, "version": identity.version, "commit": identity.commit },
        "jobs": jobs,
        "counts": counts.to_json(),
        "bytes_read": bytes_read,
        "db_bytes": std::fs::metadata(db).map_or(0, |metadata| metadata.len()),
        "seconds": {
            "walk": walked.as_secs_f64(),
            "parse": (parsed - walked).as_secs_f64(),
            "write": (total - parsed).as_secs_f64(),
            "total": total.as_secs_f64(),
        },
        "files_per_second": files.len() as f64 / total.as_secs_f64().max(1e-9),
        "megabytes_per_second": bytes_read as f64 / 1e6 / total.as_secs_f64().max(1e-9),
    }))
}

/// A borrowed view of the merged name table for the writer.
pub struct TableView<'a> {
    table: &'a NameTable,
}

fn table_view(table: &NameTable) -> TableView<'_> {
    TableView { table }
}

impl TableView<'_> {
    pub fn total(&self) -> usize {
        self.table.total
    }

    pub fn dense(&self, id: u32) -> usize {
        self.table.dense(id)
    }

    pub fn text(&self, id: u32) -> &str {
        self.table.text(id)
    }

    /// Every global id, in dense order.
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.table.shards.iter().enumerate().flat_map(|(shard, names)| (0..names.len()).map(move |index| ((index as u32) << SHARD_BITS) | shard as u32))
    }
}
