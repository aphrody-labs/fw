// SPDX-License-Identifier: Apache-2.0
//! Corpus assembly: merge per-file extractions, resolve imports and call sites, and fold
//! sourceless type stubs onto their unique definition.
//!
//! Call resolution never links a call to an arbitrary homonym of another package:
//! - a call to a name defined in the same file resolves there (EXTRACTED);
//! - a Rust call carrying a path (`use` alias, `crate::a::b::f()`, `super::f()`, `bun_sys::open()`,
//!   `Type::f()`, `x.f()` on a receiver of known type) is located in the module tree
//!   ([`rpath`](super::rpath)), through `pub use` re-exports, and resolves inside the named
//!   module or type (INFERRED); a path into a crate outside the corpus is external;
//! - a plain Rust call resolves through a glob import, then to a unique definition of the same
//!   crate;
//! - a JS/TS call to an imported name resolves inside the module its specifier names
//!   (EXTRACTED for a relative or `tsconfig` specifier); a built-in or package specifier is
//!   external;
//! - a plain JS/TS call resolves only to an ambient `.d.ts` declaration.
//!
//! Symbols of crates and packages outside the corpus become one external node per item
//! (`serde_json::Value`, `node:fs::readFileSync`), marked [`Node::external`]; the standard
//! library and the JS globals are counted but get no call edges. A type reference never
//! folds onto an enum variant, and a keyword or generic parameter is never a node.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::{
    FileExtract, ImportKind, NodeClass, RawCall, RawEdge, RawImport,
    ids::make_id,
    rpath::{Loc, ModIndex, Res, Want, module_segments},
    rust::{is_blocklisted, strip_generics},
    tsconfig::{TsConfig, join_relative},
};
use crate::codegraph::graph::{Confidence, Edge, Graph, Node, NodeKind};

/// Counters describing an assembly.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssembleStats {
    /// Call sites seen.
    pub calls: usize,
    /// Resolved inside the same file.
    pub same_file: usize,
    /// Resolved to a definition in another file.
    pub cross_file: usize,
    /// Calls into code outside the corpus: Rust crates, the standard library, Node/Bun
    /// built-ins, npm packages and JS globals.
    #[serde(default)]
    pub external: usize,
    /// Calls through a local value (a parameter, a `let`, a closure) that nothing defines.
    #[serde(default)]
    pub local: usize,
    /// Left unresolved (unknown, ambiguous or too generic).
    pub unresolved: usize,
    /// Why calls stayed unresolved.
    #[serde(default)]
    pub unresolved_by: BTreeMap<String, usize>,
    /// Stubs folded onto a definition.
    pub stubs_folded: usize,
    /// JS/TS imports and Markdown links resolved to a file of the corpus.
    #[serde(default)]
    pub imports_resolved: usize,
    /// JS/TS imports of an external module.
    #[serde(default)]
    pub imports_external: usize,
    /// Rust `use` names resolved to a definition or module of the corpus.
    #[serde(default)]
    pub uses_resolved: usize,
    /// Rust `use` names of a crate outside the corpus.
    #[serde(default)]
    pub uses_external: usize,
    /// Rust `use` names that lead nowhere known (macros, enum variants, generated code).
    #[serde(default)]
    pub uses_unresolved: usize,
    /// External nodes of the graph.
    #[serde(default)]
    pub external_nodes: usize,
    /// Nodes of generated files included on request.
    #[serde(default)]
    pub generated_nodes: usize,
}

/// Corpus facts the per-file extractions cannot know.
#[derive(Debug, Clone, Default)]
pub struct AssembleContext {
    /// Package of each file: the Rust crate identifier (`bun_sys`) or the npm package name.
    pub package_of: HashMap<String, String>,
    /// Directory of each package manifest, relative to the root (`src/sys`), so a Rust file's
    /// module path is known (`src/sys/lib.rs` is the crate root, `src/sys/fd.rs` is `fd`).
    pub package_dir: HashMap<String, String>,
    /// Dependency identifiers declared by each Rust crate manifest (`serde_json`), by package.
    pub package_deps: HashMap<String, HashSet<String>>,
    /// `tsconfig.json` settings (`baseUrl`, `paths`) of the corpus.
    pub ts_configs: Vec<TsConfig>,
}

fn lang_of(file: &str) -> Option<String> {
    Some(
        match file.rsplit_once('.')?.1 {
            "rs" => "rust",
            "ts" | "tsx" | "mts" | "cts" => "typescript",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "py" | "pyw" | "pyi" => "python",
            "md" | "mdx" => "markdown",
            _ => return None,
        }
        .to_owned(),
    )
}

/// File nodes whose bare name is shared by several files get the shortest path suffix (at
/// least two components) that is unique among them, as Graphify labels them.
fn disambiguate_file_labels(nodes: &mut [Node]) {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if let Some(file) = &n.file
            && n.id == make_id(&[file])
            && n.loc.as_deref() == Some("L1")
            && file.rsplit('/').next() == Some(n.label.as_str())
        {
            groups.entry(n.label.clone()).or_default().push(i);
        }
    }
    for members in groups.into_values().filter(|m| m.len() > 1) {
        let paths: Vec<Vec<&str>> = members
            .iter()
            .map(|&i| nodes[i].file.as_deref().unwrap_or("").split('/').collect())
            .collect();
        let suffix =
            |parts: &Vec<&str>, depth: usize| parts[parts.len().saturating_sub(depth)..].join("/");
        let labels: Vec<String> = paths
            .iter()
            .map(|parts| {
                for depth in 2..=parts.len() {
                    let candidate = suffix(parts, depth);
                    if paths
                        .iter()
                        .filter(|other| suffix(other, depth) == candidate)
                        .count()
                        == 1
                    {
                        return candidate;
                    }
                }
                parts.join("/")
            })
            .collect();
        for (&i, label) in members.iter().zip(labels) {
            nodes[i].label = label;
        }
    }
}

/// A callable definition.
struct Def {
    id: String,
    file: String,
    method: bool,
    /// Base name of the owning type for a method.
    owner: Option<String>,
}

const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

const JS_EXTENSIONS: &[&str] = &[
    ".ts", ".tsx", ".d.ts", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts",
];

fn is_builtin_module(spec: &str) -> bool {
    if spec.starts_with("node:") || spec.starts_with("bun:") || spec == "bun" {
        return true;
    }
    NODE_BUILTINS.contains(&spec)
        || spec.split_once('/').is_some_and(|(head, _)| {
            NODE_BUILTINS.contains(&head)
                && matches!(
                    head,
                    "fs" | "stream"
                        | "util"
                        | "path"
                        | "dns"
                        | "timers"
                        | "assert"
                        | "readline"
                        | "inspector"
                )
        })
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// Where an import specifier points.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// A file of the corpus; `true` when found through a relative specifier.
    File(String, bool),
    /// A built-in or package module.
    External,
}

fn module_stem(path: &str) -> &str {
    path.strip_suffix(".d.ts")
        .unwrap_or_else(|| path.rsplit_once('.').map_or(path, |(s, _)| s))
}

struct Files {
    all: HashSet<String>,
    /// Lower-case file name without extension to paths.
    by_stem: HashMap<String, Vec<String>>,
    cache: HashMap<(String, String), Option<Target>>,
    /// `tsconfig.json` settings, deepest directory first.
    configs: Vec<TsConfig>,
}

impl Files {
    fn new(paths: impl Iterator<Item = String>, configs: &[TsConfig]) -> Self {
        let all: HashSet<String> = paths.collect();
        let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
        for p in &all {
            let name = p.rsplit('/').next().unwrap_or(p);
            by_stem
                .entry(module_stem(name).to_lowercase())
                .or_default()
                .push(p.clone());
        }
        for v in by_stem.values_mut() {
            v.sort();
        }
        let mut configs = configs.to_vec();
        configs.sort_by(|a, b| b.dir.len().cmp(&a.dir.len()).then_with(|| a.dir.cmp(&b.dir)));
        Self {
            all,
            by_stem,
            cache: HashMap::new(),
            configs,
        }
    }

    fn probe(&self, base: &str) -> Option<String> {
        if self.all.contains(base) {
            return Some(base.to_owned());
        }
        let stripped = [".js", ".mjs", ".cjs", ".jsx"]
            .iter()
            .find_map(|e| base.strip_suffix(e))
            .unwrap_or(base);
        for root in [base, stripped] {
            for ext in JS_EXTENSIONS {
                let candidate = format!("{root}{ext}");
                if self.all.contains(&candidate) {
                    return Some(candidate);
                }
            }
            for ext in JS_EXTENSIONS {
                let candidate = format!("{root}/index{ext}");
                if self.all.contains(&candidate) {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// The file a non-relative specifier names through the nearest `tsconfig.json`
    /// (`baseUrl`, `paths`).
    fn configured(&self, importer: &str, spec: &str) -> Option<String> {
        if is_builtin_module(spec) {
            return None;
        }
        let dir = parent_dir(importer);
        let config = self.configs.iter().find(|c| {
            c.dir.is_empty() || dir == c.dir || dir.starts_with(&format!("{}/", c.dir))
        })?;
        config
            .candidates(spec)
            .iter()
            .find_map(|base| self.probe(base))
    }

    fn module(&mut self, importer: &str, spec: &str) -> Option<Target> {
        let key = (parent_dir(importer).to_owned(), spec.to_owned());
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let relative =
            spec.starts_with("./") || spec.starts_with("../") || matches!(spec, "." | "..");
        let found = if relative {
            join_relative(parent_dir(importer), spec)
                .and_then(|base| self.probe(&base))
                .map(|p| Target::File(p, true))
        } else if let Some(path) = self.configured(importer, spec) {
            Some(Target::File(path, true))
        } else if is_builtin_module(spec) {
            Some(Target::External)
        } else if spec.contains('/') && !spec.starts_with('@') {
            // Path aliases such as Bun's `internal/fs/streams`: a unique file ending with it.
            let suffix = format!("/{spec}");
            let index = format!("{suffix}/index");
            let mut hits: Vec<&String> = self
                .all
                .iter()
                .filter(|p| !p.contains("node_modules/"))
                .filter(|p| {
                    let stem = module_stem(p);
                    stem.ends_with(&suffix) || stem.ends_with(&index)
                })
                .collect();
            hits.sort();
            hits.dedup();
            match hits.as_slice() {
                [only] => Some(Target::File((*only).clone(), false)),
                _ => Some(Target::External),
            }
        } else {
            // A bare name: a unique first-party file of that name (a `paths` alias such as
            // `harness`), otherwise an npm package.
            match self.by_stem.get(&spec.to_lowercase()).map(Vec::as_slice) {
                Some([only]) if !only.contains("node_modules/") => {
                    Some(Target::File(only.clone(), false))
                }
                _ => Some(Target::External),
            }
        };
        self.cache.insert(key, found.clone());
        found
    }

    fn markdown(&self, importer: &str, import: &RawImport) -> Option<(String, Confidence)> {
        match import.kind {
            ImportKind::Link => {
                let path = join_relative(parent_dir(importer), &import.specifier)?;
                self.all
                    .contains(&path)
                    .then_some((path, Confidence::Extracted))
            }
            ImportKind::Wiki => {
                let wanted = import.specifier.trim_end_matches(".md").to_lowercase();
                let candidates = self.by_stem.get(&wanted)?;
                let md: Vec<&String> = candidates
                    .iter()
                    .filter(|p| p.ends_with(".md") || p.ends_with(".mdx"))
                    .collect();
                let near: Vec<&&String> = md
                    .iter()
                    .filter(|p| parent_dir(p) == parent_dir(importer))
                    .collect();
                match (near.as_slice(), md.as_slice()) {
                    ([only], _) => Some(((**only).clone(), Confidence::Inferred)),
                    ([], [only]) => Some(((*only).clone(), Confidence::Inferred)),
                    _ => None,
                }
            }
            ImportKind::Module => None,
        }
    }
}

/// How well a definition file matches the module path of a call: the crate root for a bare
/// `krate::f()`, a file whose module path ends with (or contains) the named modules otherwise.
fn module_score(file: &str, dir: Option<&str>, modules: &[&str]) -> usize {
    let segs = module_segments(file, dir);
    if modules.is_empty() {
        return if dir.is_some() && segs.is_empty() {
            3
        } else {
            1
        };
    }
    if segs.ends_with(modules) {
        3
    } else if modules.iter().all(|m| segs.contains(m)) {
        2
    } else {
        0
    }
}

struct Resolver<'a> {
    defs: HashMap<String, Vec<Def>>,
    package_of: &'a HashMap<String, String>,
    package_dir: &'a HashMap<String, String>,
    crates: HashSet<String>,
    rust: ModIndex,
    /// Base label of every type node.
    type_label: HashMap<String, String>,
}

impl Resolver<'_> {
    fn package(&self, file: &str) -> &str {
        self.package_of.get(file).map_or("", String::as_str)
    }

    /// The best of `candidates` by module match; a tie leaves the call unresolved.
    fn best<'d>(&self, candidates: &[&'d Def], modules: &[&str]) -> Option<&'d Def> {
        let scored: Vec<(usize, &Def)> = candidates
            .iter()
            .map(|d| {
                let dir = self
                    .package_dir
                    .get(self.package(&d.file))
                    .map(String::as_str);
                (module_score(&d.file, dir, modules), *d)
            })
            .filter(|(s, _)| *s > 0)
            .collect();
        let top = scored.iter().map(|(s, _)| *s).max()?;
        let winners: Vec<&Def> = scored
            .into_iter()
            .filter(|(s, _)| *s == top)
            .map(|(_, d)| d)
            .collect();
        if let [only] = winners.as_slice() {
            Some(*only)
        } else {
            None
        }
    }

    /// Resolve a Rust call that carries a path.
    fn rust_path(&self, call: &RawCall, path: &str) -> Option<(String, f64)> {
        let own = self.package(&call.file);
        let segs: Vec<&str> = path.split("::").filter(|s| !s.is_empty()).collect();
        let mut i = 0;
        while i < segs.len() && matches!(segs[i], "crate" | "self" | "super") {
            i += 1;
        }
        let mut krate = own;
        if i == 0 && !segs.is_empty() && self.crates.contains(segs[0]) {
            krate = segs[0];
            i = 1;
        }
        let rest = &segs[i..];
        let (modules, ty) = match rest.split_last() {
            Some((last, before)) if last.starts_with(char::is_uppercase) => (before, Some(*last)),
            _ => (rest, None),
        };
        let all = self.defs.get(&call.name)?;
        if let Some(ty) = ty {
            let typed: Vec<&Def> = all
                .iter()
                .filter(|d| d.owner.as_deref() == Some(ty))
                .collect();
            let in_crate: Vec<&Def> = typed
                .iter()
                .copied()
                .filter(|d| self.package(&d.file) == krate)
                .collect();
            if let Some(d) = self.best(&in_crate, modules) {
                return Some((d.id.clone(), 0.9));
            }
            if in_crate.is_empty()
                && let [only] = typed.as_slice()
            {
                // A re-exported type: the only type of that name in the corpus with this method.
                return Some((only.id.clone(), 0.7));
            }
            return None;
        }
        let free: Vec<&Def> = all
            .iter()
            .filter(|d| !d.method && self.package(&d.file) == krate)
            .collect();
        self.best(&free, modules)
            .map(|d| (d.id.clone(), if modules.is_empty() { 0.8 } else { 0.9 }))
    }

    /// Resolve a plain Rust call that matched nothing in its file: same crate, then globs.
    fn rust_plain(&self, call: &RawCall) -> Option<(String, f64)> {
        let own = self.package(&call.file);
        let all = self.defs.get(&call.name)?;
        let local: Vec<&Def> = all
            .iter()
            .filter(|d| !d.method && self.package(&d.file) == own)
            .collect();
        if let [only] = local.as_slice() {
            return Some((only.id.clone(), 0.8));
        }
        if let Some(Res::Item { id, .. }) = self.rust.bare(&call.file, &call.name, Want::Function) {
            return Some((id, 0.8));
        }
        for glob in self.rust.uses_of(&call.file).iter().filter(|u| u.glob) {
            if let Some((id, _)) = self.rust_path(call, &glob.path) {
                return Some((id, 0.7));
            }
        }
        None
    }

    /// The same-file definition a call names, when it is unique.
    fn same_file(&self, call: &RawCall, rust: bool) -> Option<String> {
        let found = self.defs.get(&call.name)?;
        let same: Vec<&Def> = found
            .iter()
            .filter(|d| d.file == call.file && (d.method == call.method || !rust))
            .collect();
        if let [only] = same.as_slice() {
            Some(only.id.clone())
        } else {
            None
        }
    }

    fn missing(&self, call: &RawCall) -> Outcome {
        if call.file.ends_with(".rs") && RUST_PRELUDE_FNS.contains(&call.name.as_str()) {
            return Outcome::External(None);
        }
        let named = self
            .defs
            .get(&call.name)
            .is_some_and(|found| found.iter().any(|d| d.method == call.method));
        Outcome::Unresolved(if named { "ambiguous" } else { "no_definition" })
    }

    /// The method `name` of the type node `type_id` (a definition in `type_file`).
    fn method_of(&self, type_id: &str, type_file: &str, name: &str) -> Outcome {
        let (Some(label), Some(found)) = (self.type_label.get(type_id), self.defs.get(name)) else {
            return Outcome::Unresolved("method_not_found");
        };
        let krate = self.package(type_file);
        let methods: Vec<&Def> = found
            .iter()
            .filter(|d| {
                d.method && d.owner.as_deref() == Some(label) && self.package(&d.file) == krate
            })
            .collect();
        // Several types of one crate share a name (`Client`, `blocking::Client`): the impl of
        // the type usually sits in the file that defines it.
        let beside: Vec<&&Def> = methods.iter().filter(|d| d.file == type_file).collect();
        match (methods.as_slice(), beside.as_slice()) {
            (_, [only]) => Outcome::Edge(only.id.clone(), Confidence::Inferred, 0.9),
            ([only], _) => Outcome::Edge(only.id.clone(), Confidence::Inferred, 0.9),
            ([], _) if is_blocklisted(name) => Outcome::External(None),
            ([], _) => Outcome::Unresolved("method_not_found"),
            _ => Outcome::Unresolved("ambiguous"),
        }
    }

    /// A Rust call that carries a path: located in the module tree first, then by the module
    /// score of the crate's definitions.
    fn rust_path_call(&self, call: &RawCall, path: &str) -> Outcome {
        let segs: Vec<&str> = path.split("::").filter(|s| !s.is_empty()).collect();
        let external = |krate: String, std: bool| {
            if std {
                Outcome::External(None)
            } else {
                let item = if call.method {
                    path.to_owned()
                } else {
                    format!("{path}::{}", call.name)
                };
                Outcome::External(Some(("rs", krate, item)))
            }
        };
        match self.rust.locate(&call.file, call.inline, &segs) {
            Loc::External { krate, std } => return external(krate, std),
            // The path is placed in a module of the corpus: what that module does not define
            // (a macro, generated code, an unresolved re-export) is not guessed from a homonym
            // elsewhere.
            Loc::Internal { pkg, module, rest } => {
                let found = if call.method {
                    self.rust.in_module(&pkg, &module, &rest, Want::Type, 0)
                } else if rest.is_empty() {
                    self.rust.in_module(
                        &pkg,
                        &module,
                        std::slice::from_ref(&call.name),
                        Want::Function,
                        0,
                    )
                } else {
                    Res::Unknown
                };
                return match found {
                    Res::Item { id, file } if call.method => self.method_of(&id, &file, &call.name),
                    Res::Item { id, .. } => Outcome::Edge(id, Confidence::Inferred, 0.9),
                    Res::External { krate, std } => external(krate, std),
                    Res::Module { .. } | Res::Unknown if call.method && is_blocklisted(&call.name) => {
                        Outcome::External(None)
                    }
                    Res::Module { .. } | Res::Unknown => Outcome::Unresolved("path_unresolved"),
                };
            }
            Loc::Unknown => {}
        }
        match self.rust_path(call, path) {
            Some((id, score)) => Outcome::Edge(id, Confidence::Inferred, score),
            // `Type::default()` of a type the corpus does not define: a derive or a std trait.
            None if call.method && is_blocklisted(&call.name) => Outcome::External(None),
            None => Outcome::Unresolved("path_unresolved"),
        }
    }

    fn rust_call(&self, call: &RawCall) -> Outcome {
        if let Some(path) = &call.path {
            return self.rust_path_call(call, path);
        }
        if let Some(id) = self.same_file(call, true) {
            return Outcome::Edge(id, Confidence::Extracted, 0.85);
        }
        if call.method {
            return Outcome::Unresolved("method_receiver_unknown");
        }
        if call.bound {
            return Outcome::Local;
        }
        if is_blocklisted(&call.name) {
            return Outcome::Unresolved("generic_name");
        }
        match self.rust_plain(call) {
            Some((id, score)) => Outcome::Edge(id, Confidence::Inferred, score),
            None => self.missing(call),
        }
    }

    /// Resolve one call site.
    fn call(
        &self,
        call: &RawCall,
        module_of: &HashMap<(String, String), Option<Target>>,
    ) -> Outcome {
        if let Some(module) = &call.module {
            return match module_of
                .get(&(call.file.clone(), module.clone()))
                .cloned()
                .flatten()
            {
                Some(Target::File(path, relative)) => {
                    let found = self.defs.get(&call.name).map(|found| {
                        found
                            .iter()
                            .filter(|d| d.file == path && !d.method)
                            .collect::<Vec<&Def>>()
                    });
                    match found.as_deref() {
                        Some([only]) if relative => {
                            Outcome::Edge(only.id.clone(), Confidence::Extracted, 1.0)
                        }
                        Some([only]) => Outcome::Edge(only.id.clone(), Confidence::Inferred, 0.85),
                        _ => Outcome::Unresolved("not_exported"),
                    }
                }
                Some(Target::External) => Outcome::External(Some((
                    "ts",
                    npm_package(module),
                    format!("{module}::{}", call.name),
                ))),
                None => Outcome::Unresolved("module_unresolved"),
            };
        }
        if call.file.ends_with(".rs") {
            return self.rust_call(call);
        }
        if let Some(id) = self.same_file(call, false) {
            return Outcome::Edge(id, Confidence::Extracted, 0.85);
        }
        if call.method {
            return Outcome::Unresolved("method_receiver_unknown");
        }
        // JS/TS: a name neither local nor imported is a global; only an ambient declaration
        // (`declare function` in a `.d.ts`) can define it.
        let ambient: Vec<&Def> = self
            .defs
            .get(&call.name)
            .map(|found| {
                found
                    .iter()
                    .filter(|d| d.file.ends_with(".d.ts") && !d.method)
                    .collect()
            })
            .unwrap_or_default();
        if let [only] = ambient.as_slice() {
            return Outcome::Edge(only.id.clone(), Confidence::Inferred, 0.6);
        }
        if call.bound {
            return Outcome::Local;
        }
        if JS_GLOBALS.contains(&call.name.as_str()) {
            return Outcome::External(None);
        }
        self.missing(call)
    }
}

/// JS globals called as plain functions: calls to them are external, not unresolved.
const JS_GLOBALS: &[&str] = &[
    "String",
    "Number",
    "Boolean",
    "BigInt",
    "Symbol",
    "Array",
    "Object",
    "Date",
    "Error",
    "TypeError",
    "RangeError",
    "RegExp",
    "Function",
    "Promise",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURIComponent",
    "decodeURIComponent",
    "encodeURI",
    "decodeURI",
    "escape",
    "unescape",
    "setTimeout",
    "setInterval",
    "setImmediate",
    "clearTimeout",
    "clearInterval",
    "queueMicrotask",
    "structuredClone",
    "fetch",
    "atob",
    "btoa",
    "eval",
    "alert",
    "require",
];

/// Functions of the Rust prelude that a plain call may name.
const RUST_PRELUDE_FNS: &[&str] = &["drop", "size_of", "size_of_val", "align_of", "align_of_val"];

/// A symbol outside the corpus.
struct ExternalNode {
    label: String,
    package: String,
}

/// External nodes, one per item.
#[derive(Default)]
struct Externals {
    nodes: BTreeMap<String, ExternalNode>,
}

impl Externals {
    /// The node of `path` (`serde_json::Value`) in `package`, created on first use.
    fn node(&mut self, lang: &str, package: &str, path: &str) -> String {
        let id = make_id(&["external", lang, path]);
        self.nodes.entry(id.clone()).or_insert_with(|| ExternalNode {
            label: path.to_owned(),
            package: package.to_owned(),
        });
        id
    }
}

/// How a call site ends up.
enum Outcome {
    Edge(String, Confidence, f64),
    /// Into code outside the corpus; `Some((language, package, path))` gets an external node.
    External(Option<(&'static str, String, String)>),
    /// Through a local value.
    Local,
    Unresolved(&'static str),
}

fn raw_edge(src: &str, dst: &str, relation: &str, file: &str, loc: &str, context: &str) -> RawEdge {
    RawEdge {
        src: src.to_owned(),
        dst: dst.to_owned(),
        relation: relation.to_owned(),
        confidence: Confidence::Extracted,
        score: 1.0,
        file: file.to_owned(),
        loc: loc.to_owned(),
        context: Some(context.to_owned()),
        path: None,
    }
}

/// Assemble a graph from per-file extractions, without package information (every file is
/// treated as part of one package).
#[must_use]
pub fn assemble(files: Vec<FileExtract>) -> (Graph, AssembleStats) {
    assemble_with(files, &AssembleContext::default())
}

/// Assemble a graph from per-file extractions.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn assemble_with(files: Vec<FileExtract>, context: &AssembleContext) -> (Graph, AssembleStats) {
    let mut stats = AssembleStats::default();
    let mut externals = Externals::default();
    // 1. Merge nodes: a sourced definition replaces a stub with the same id.
    let mut nodes: BTreeMap<String, super::RawNode> = BTreeMap::new();
    let mut edges: Vec<RawEdge> = Vec::new();
    let mut calls = Vec::new();
    let mut imports: Vec<(String, RawImport)> = Vec::new();
    let mut generated_files: HashSet<String> = HashSet::new();
    let crates: HashSet<String> = context
        .package_of
        .iter()
        .filter(|(file, _)| file.ends_with(".rs"))
        .map(|(_, krate)| krate.clone())
        .collect();
    let mut rust = ModIndex::new(crates.clone(), context.package_deps.clone());
    let mut rust_uses: Vec<(String, super::RawUse)> = Vec::new();
    for f in files {
        let file = f
            .nodes
            .iter()
            .find(|n| n.class == NodeClass::File)
            .map(|n| n.file.clone());
        if let Some(file) = &file {
            if file.ends_with(".rs") {
                let package = context.package_of.get(file).map_or("", String::as_str);
                rust.add_file(
                    file,
                    package,
                    context.package_dir.get(package).map(String::as_str),
                    &f.nodes,
                    &f.uses,
                    &f.mods,
                );
                rust_uses.extend(f.uses.iter().map(|u| (file.clone(), u.clone())));
            }
            if f.generated.is_some() {
                generated_files.insert(file.clone());
            }
        }
        for n in f.nodes {
            match nodes.get_mut(&n.id) {
                Some(existing) if existing.file.is_empty() && n.file.is_empty() => {
                    // Two stubs of one id: keep the proper-case label (`Json` over `json`).
                    if existing.label == existing.id && n.label != n.id {
                        existing.label = n.label;
                    }
                }
                Some(existing) if !existing.file.is_empty() || n.file.is_empty() => {}
                _ => {
                    nodes.insert(n.id.clone(), n);
                }
            }
        }
        edges.extend(f.edges);
        calls.extend(f.calls);
        if let Some(file) = file {
            imports.extend(f.imports.into_iter().map(|i| (file.clone(), i)));
        }
    }
    rust.finish();

    // 2. Rust type references that name a stub: follow the path the file spelled (a definition
    // of the corpus, or one external node per item); the rest fold onto a unique type of the
    // same crate, then of the corpus. Variants never absorb a type reference.
    let mut by_label: HashMap<String, Vec<String>> = HashMap::new();
    for n in nodes
        .values()
        .filter(|n| n.class == NodeClass::Type && !n.file.is_empty())
    {
        by_label
            .entry(strip_generics(&n.label).to_lowercase())
            .or_default()
            .push(n.id.clone());
    }
    let package_of_node = |id: &str, nodes: &BTreeMap<String, super::RawNode>| -> String {
        nodes
            .get(id)
            .and_then(|n| context.package_of.get(&n.file))
            .cloned()
            .unwrap_or_default()
    };
    let mut folded: HashSet<String> = HashSet::new();
    let mut dropped: HashSet<usize> = HashSet::new();
    for (i, e) in edges.iter_mut().enumerate() {
        let is_stub = nodes.get(&e.dst).is_some_and(|n| n.file.is_empty());
        if !is_stub || !matches!(e.relation.as_str(), "references" | "implements" | "inherits") {
            continue;
        }
        let stub = e.dst.clone();
        let label = nodes[&stub].label.clone();
        let mut target: Option<String> = None;
        if let Some(path) = &e.path {
            match rust.resolve(&e.file, 0, path, Want::Type) {
                Res::Item { id, .. } => target = Some(id),
                Res::External { std: true, .. } => {
                    dropped.insert(i);
                    continue;
                }
                Res::External { krate, .. } => {
                    e.dst = externals.node("rs", &krate, path);
                    continue;
                }
                Res::Module { .. } | Res::Unknown => {}
            }
        }
        if target.is_none()
            && let Some(Res::Item { id, .. }) = rust.bare(&e.file, &label, Want::Type)
        {
            target = Some(id);
        }
        if target.is_none()
            && let Some(candidates) = by_label.get(&label.to_lowercase())
        {
            target = match candidates.as_slice() {
                [only] => Some(only.clone()),
                many => {
                    let own = rust.package_of(&e.file).unwrap_or_default().to_owned();
                    let same: Vec<&String> = many
                        .iter()
                        .filter(|id| package_of_node(id, &nodes) == own)
                        .collect();
                    match same.as_slice() {
                        [only] if !own.is_empty() => Some((*only).clone()),
                        _ => None,
                    }
                }
            };
        }
        if let Some(target) = target {
            folded.insert(stub);
            e.dst = target;
        }
    }
    stats.stubs_folded = folded.len();
    let mut kept = 0;
    edges.retain(|_| {
        kept += 1;
        !dropped.contains(&(kept - 1))
    });

    // 2b. Rust `use` declarations: a file imports the definition, the module or the external
    // item each name leads to.
    let mut import_pairs: HashSet<(String, String)> = HashSet::new();
    for (file, u) in &rust_uses {
        let from = make_id(&[file]);
        let res = if u.glob {
            match rust.resolve(file, u.inline, &u.path, Want::Any) {
                res @ (Res::Module { .. } | Res::External { .. }) => res,
                _ => Res::Unknown,
            }
        } else {
            rust.resolve(file, u.inline, &u.path, Want::Any)
        };
        let (target, external) = match res {
            Res::Item { id, file: def } if &def != file => (id, false),
            Res::Module { file: def } if &def != file => (make_id(&[&def]), false),
            Res::Item { .. } | Res::Module { .. } => {
                stats.uses_resolved += 1;
                continue;
            }
            Res::External { krate, .. } => (externals.node("rs", &krate, &u.path), true),
            Res::Unknown => {
                stats.uses_unresolved += 1;
                continue;
            }
        };
        if external {
            stats.uses_external += 1;
        } else {
            stats.uses_resolved += 1;
        }
        if import_pairs.insert((from.clone(), target.clone())) {
            edges.push(raw_edge(&from, &target, "imports_from", file, &u.loc, "import"));
        }
    }

    // 3. JS/TS imports and Markdown links.
    let mut file_index = Files::new(
        nodes
            .values()
            .filter(|n| n.class == NodeClass::File)
            .map(|n| n.file.clone()),
        &context.ts_configs,
    );
    let mut symbols_by_file: HashMap<(String, String), String> = HashMap::new();
    for n in nodes
        .values()
        .filter(|n| !n.file.is_empty() && n.class != NodeClass::File)
    {
        if !n.label.starts_with('.') {
            let bare = n.label.trim_end_matches("()").to_owned();
            symbols_by_file
                .entry((n.file.clone(), bare))
                .or_insert_with(|| n.id.clone());
        }
    }
    let mut module_of: HashMap<(String, String), Option<Target>> = HashMap::new();
    for (file, import) in &imports {
        let from = make_id(&[file]);
        if import.kind != ImportKind::Module {
            if let Some((target, confidence)) = file_index.markdown(file, import) {
                stats.imports_resolved += 1;
                edges.push(RawEdge {
                    src: from,
                    dst: make_id(&[&target]),
                    relation: "references".into(),
                    confidence,
                    score: if confidence == Confidence::Extracted {
                        1.0
                    } else {
                        0.9
                    },
                    file: file.clone(),
                    loc: import.loc.clone(),
                    context: Some("link".into()),
                    path: None,
                });
            }
            continue;
        }
        let target = file_index.module(file, &import.specifier);
        module_of.insert((file.clone(), import.specifier.clone()), target.clone());
        match target {
            Some(Target::File(path, relative)) => {
                stats.imports_resolved += 1;
                let (confidence, score) = if relative {
                    (Confidence::Extracted, 1.0)
                } else {
                    (Confidence::Inferred, 0.85)
                };
                edges.push(RawEdge {
                    src: from.clone(),
                    dst: make_id(&[&path]),
                    relation: "imports_from".into(),
                    confidence,
                    score,
                    file: file.clone(),
                    loc: import.loc.clone(),
                    context: Some("import".into()),
                    path: None,
                });
                for name in &import.names {
                    if let Some(symbol) = symbols_by_file.get(&(path.clone(), name.clone())) {
                        edges.push(RawEdge {
                            src: from.clone(),
                            dst: symbol.clone(),
                            relation: "imports".into(),
                            confidence,
                            score,
                            file: file.clone(),
                            loc: import.loc.clone(),
                            context: Some("import".into()),
                            path: None,
                        });
                    }
                }
            }
            Some(Target::External) => {
                stats.imports_external += 1;
                let package = npm_package(&import.specifier);
                let module = externals.node("ts", &package, &import.specifier);
                edges.push(raw_edge(
                    &from,
                    &module,
                    "imports_from",
                    file,
                    &import.loc,
                    "import",
                ));
                for name in &import.names {
                    let item = externals.node(
                        "ts",
                        &package,
                        &format!("{}::{name}", import.specifier),
                    );
                    edges.push(raw_edge(&from, &item, "imports", file, &import.loc, "import"));
                }
            }
            None => {}
        }
    }

    // 4. Resolve call sites.
    let mut owner_of: HashMap<String, String> = HashMap::new();
    for e in edges.iter().filter(|e| e.relation == "method") {
        if let Some(owner) = nodes.get(&e.src) {
            owner_of.insert(e.dst.clone(), strip_generics(&owner.label));
        }
    }
    let mut defs: HashMap<String, Vec<Def>> = HashMap::new();
    for n in nodes.values().filter(|n| !n.file.is_empty()) {
        if let Some(inner) = n.label.strip_suffix("()") {
            let (name, method) = inner
                .strip_prefix('.')
                .map_or((inner, false), |name| (name, true));
            defs.entry(name.to_owned()).or_default().push(Def {
                id: n.id.clone(),
                file: n.file.clone(),
                method,
                owner: if method {
                    owner_of.get(&n.id).cloned()
                } else {
                    None
                },
            });
        }
    }
    let type_label: HashMap<String, String> = nodes
        .values()
        .filter(|n| n.class == NodeClass::Type)
        .map(|n| (n.id.clone(), strip_generics(&n.label)))
        .collect();
    let resolver = Resolver {
        defs,
        package_of: &context.package_of,
        package_dir: &context.package_dir,
        crates,
        rust,
        type_label,
    };
    let mut seen_calls: HashSet<(String, String)> = HashSet::new();
    for call in calls {
        stats.calls += 1;
        let outcome = resolver.call(&call, &module_of);
        let (target, confidence, score) = match outcome {
            Outcome::Edge(target, confidence, score) => (target, confidence, score),
            Outcome::External(node) => {
                stats.external += 1;
                if let Some((lang, package, path)) = node {
                    let target = externals.node(lang, &package, &path);
                    if seen_calls.insert((call.from.clone(), target.clone())) {
                        edges.push(raw_edge(
                            &call.from, &target, "calls", &call.file, &call.loc, "call",
                        ));
                    }
                }
                continue;
            }
            Outcome::Local => {
                stats.local += 1;
                continue;
            }
            Outcome::Unresolved(reason) => {
                stats.unresolved += 1;
                *stats.unresolved_by.entry(reason.to_owned()).or_insert(0) += 1;
                continue;
            }
        };
        if target == call.from || !seen_calls.insert((call.from.clone(), target.clone())) {
            continue;
        }
        let local = resolver
            .defs
            .get(&call.name)
            .is_some_and(|f| f.iter().any(|d| d.id == target && d.file == call.file));
        if local {
            stats.same_file += 1;
        } else {
            stats.cross_file += 1;
        }
        edges.push(RawEdge {
            src: call.from,
            dst: target,
            relation: "calls".into(),
            confidence,
            score,
            file: call.file,
            loc: call.loc,
            context: Some("call".into()),
            path: None,
        });
    }

    // 5. Build the graph; edges to nodes that do not exist are dropped, and so are stubs nothing
    // points at any more.
    let used: HashSet<&str> = edges
        .iter()
        .flat_map(|e| [e.src.as_str(), e.dst.as_str()])
        .collect();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut graph_nodes: Vec<Node> = Vec::new();
    for (id, n) in &nodes {
        if n.class == NodeClass::Stub && n.file.is_empty() && !used.contains(id.as_str()) {
            continue;
        }
        index.insert(id.clone(), graph_nodes.len());
        let mut node = Node::code(
            id,
            &n.label,
            if n.file.is_empty() {
                None
            } else {
                Some(&n.file)
            },
        );
        node.lang = lang_of(&n.file);
        node.kind = if n.file.is_empty() {
            NodeKind::Concept
        } else if node.lang.as_deref() == Some("markdown") {
            NodeKind::Document
        } else {
            NodeKind::Code
        };
        node.loc = if n.loc.is_empty() {
            None
        } else {
            Some(n.loc.clone())
        };
        node.package = context.package_of.get(&n.file).cloned();
        if generated_files.contains(&n.file) {
            node.generated = true;
            stats.generated_nodes += 1;
        }
        graph_nodes.push(node);
    }
    disambiguate_file_labels(&mut graph_nodes);
    stats.external_nodes = externals.nodes.len();
    for (id, external) in externals.nodes {
        index.insert(id.clone(), graph_nodes.len());
        let mut node = Node::code(&id, &external.label, None);
        node.kind = NodeKind::Concept;
        node.package = Some(external.package);
        node.external = true;
        graph_nodes.push(node);
    }
    let mut graph_edges = Vec::new();
    for e in edges {
        let (Some(&s), Some(&d)) = (index.get(&e.src), index.get(&e.dst)) else {
            continue;
        };
        graph_edges.push(Edge {
            src: s,
            dst: d,
            relation: e.relation,
            confidence: e.confidence,
            score: e.score,
            file: Some(e.file),
            loc: Some(e.loc),
            weight: 1.0,
            context: e.context,
        });
    }
    (Graph::from_parts(graph_nodes, graph_edges), stats)
}

/// Package name of an npm specifier: `@scope/name/sub` gives `@scope/name`, `lodash/fp` gives
/// `lodash`, `node:fs` stays.
fn npm_package(spec: &str) -> String {
    let mut parts = spec.split('/');
    match parts.next() {
        Some(scope) if scope.starts_with('@') => {
            format!("{scope}/{}", parts.next().unwrap_or_default())
        }
        Some(head) => head.to_owned(),
        None => spec.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::extract::{extract_file, tsconfig::TsConfig};

    fn edge<'g>(g: &'g Graph, from: &str, to: &str) -> Option<&'g Edge> {
        g.edges.iter().find(|e| {
            g.nodes[e.src].label == from && g.nodes[e.dst].label == to && e.relation == "calls"
        })
    }

    #[test]
    fn calls_resolve_same_file_first_then_uniquely_within_the_package() {
        let a = extract_file(
            "a.rs",
            b"fn run() { local(); remote(); missing(); new(); }\nfn local() {}",
        );
        let b = extract_file("b.rs", b"pub fn remote() {}\npub fn new() {}");
        let (g, stats) = assemble(vec![a, b]);
        assert_eq!(
            edge(&g, "run()", "local()").map(|e| e.confidence),
            Some(Confidence::Extracted)
        );
        assert_eq!(
            edge(&g, "run()", "remote()").map(|e| e.confidence),
            Some(Confidence::Inferred)
        );
        assert!(
            edge(&g, "run()", "new()").is_none(),
            "generic names are not resolved across files"
        );
        assert_eq!(stats.cross_file, 1);
        assert_eq!(stats.unresolved, 1);
    }

    fn crates() -> AssembleContext {
        let mut package_of = HashMap::new();
        for (file, krate) in [
            ("src/sys/lib.rs", "bun_sys"),
            ("src/sys/file.rs", "bun_sys"),
            ("src/sys/windows/mod.rs", "bun_sys"),
            ("src/core/lib.rs", "bun_core"),
            ("src/core/strings.rs", "bun_core"),
            ("src/runtime/node/fs.rs", "bun_runtime"),
            ("src/install/lib.rs", "bun_install"),
        ] {
            package_of.insert(file.to_owned(), krate.to_owned());
        }
        let package_dir = [("bun_sys", "src/sys"), ("bun_core", "src/core")]
            .into_iter()
            .map(|(k, d)| (k.to_owned(), d.to_owned()))
            .collect();
        AssembleContext {
            package_of,
            package_dir,
            ..AssembleContext::default()
        }
    }

    #[test]
    fn rust_calls_cross_crates_through_use_paths_qualified_names_and_receiver_types() {
        let sys = extract_file(
            "src/sys/lib.rs",
            b"pub mod file;\npub use file::File;\npub fn open(p: &str) {}\npub fn close() {}\n",
        );
        let file = extract_file(
            "src/sys/file.rs",
            b"pub struct File;\nimpl File {\n pub fn read_all(&self) {}\n pub fn new() -> Self { File }\n}\n",
        );
        let windows = extract_file(
            "src/sys/windows/mod.rs",
            b"pub fn open(p: &str) {}\npub fn close() {}\n",
        );
        let core = extract_file(
            "src/core/strings.rs",
            b"pub fn eql(a: &str, b: &str) -> bool { a == b }\n",
        );
        let install = extract_file(
            "src/install/lib.rs",
            b"pub fn open(p: &str) {}\npub fn eql() {}\npub fn close() {}\n",
        );
        let fs = extract_file(
            "src/runtime/node/fs.rs",
            b"use bun_sys::{self as sys, File};\nuse bun_core::strings;\n\
              fn read(path: &str) {\n  sys::open(path);\n  let g = File::new();\n  g.read_all();\n  strings::eql(path, path);\n  bun_sys::close();\n}\n\
              fn typed(file: &File) { file.read_all(); }\n",
        );
        let (g, stats) = assemble_with(vec![sys, file, windows, core, install, fs], &crates());
        let to = |from: &str, to: &str| {
            edge(&g, from, to).map(|e| {
                (
                    g.nodes[e.dst].file.clone().unwrap_or_default(),
                    e.confidence,
                )
            })
        };
        assert_eq!(
            to("read()", "open()"),
            Some(("src/sys/lib.rs".into(), Confidence::Inferred))
        );
        assert_eq!(
            to("read()", "close()"),
            Some(("src/sys/lib.rs".into(), Confidence::Inferred))
        );
        assert_eq!(
            to("read()", ".new()"),
            Some(("src/sys/file.rs".into(), Confidence::Inferred))
        );
        assert_eq!(
            to("read()", ".read_all()"),
            Some(("src/sys/file.rs".into(), Confidence::Inferred))
        );
        assert_eq!(
            to("typed()", ".read_all()"),
            Some(("src/sys/file.rs".into(), Confidence::Inferred))
        );
        assert_eq!(
            to("read()", "eql()"),
            Some(("src/core/strings.rs".into(), Confidence::Inferred))
        );
        assert!(
            !g.edges.iter().any(|e| e.relation == "calls"
                && g.nodes[e.dst].file.as_deref() == Some("src/install/lib.rs")),
            "no homonym of another crate"
        );
        assert!(stats.cross_file >= 6, "{stats:?}");
        assert!(
            g.nodes
                .iter()
                .any(|n| n.package.as_deref() == Some("bun_sys"))
        );
    }

    #[test]
    fn plain_calls_never_reach_a_homonym_of_another_package() {
        let a = extract_file("src/install/lib.rs", b"fn go() { helper(); }\n");
        let b = extract_file("src/core/lib.rs", b"pub fn helper() {}\n");
        let (g, stats) = assemble_with(vec![a, b], &crates());
        assert!(edge(&g, "go()", "helper()").is_none());
        assert_eq!(stats.unresolved, 1);
    }

    #[test]
    fn js_imports_resolve_relative_and_alias_paths_and_skip_builtins() {
        let main = extract_file(
            "src/js/node/fs.ts",
            b"import { join } from \"node:path\";\nconst { validate } = require(\"internal/validators\");\nimport { square } from './math';\nexport function run() { join(); validate(); square(); local(); }\nfunction local() {}\n",
        );
        let math = extract_file("src/js/node/math.ts", b"export function square() {}\n");
        let validators = extract_file(
            "src/js/internal/validators.ts",
            b"export function validate() {}\n",
        );
        let path = extract_file("src/node-fallbacks/path.js", b"export function join() {}\n");
        let (g, stats) = assemble(vec![main, math, validators, path]);
        assert_eq!(
            edge(&g, "run()", "square()").map(|e| e.confidence),
            Some(Confidence::Extracted)
        );
        assert_eq!(
            edge(&g, "run()", "validate()").map(|e| e.confidence),
            Some(Confidence::Inferred)
        );
        assert_eq!(
            edge(&g, "run()", "local()").map(|e| e.confidence),
            Some(Confidence::Extracted)
        );
        assert!(
            edge(&g, "run()", "join()").is_none(),
            "node:path is not src/node-fallbacks/path.js"
        );
        assert_eq!(stats.external, 1);
        assert_eq!(stats.imports_external, 1);
        let imports_from: Vec<&str> = g
            .edges
            .iter()
            .filter(|e| e.relation == "imports_from")
            .map(|e| g.nodes[e.dst].label.as_str())
            .collect();
        assert!(
            imports_from.contains(&"math.ts") && imports_from.contains(&"validators.ts"),
            "{imports_from:?}"
        );
    }

    #[test]
    fn symbols_of_same_named_files_stay_distinct() {
        let a = extract_file("pkg/a/index.ts", b"export function run() {}\n");
        let b = extract_file("pkg/b/index.ts", b"export function run() {}\n");
        let (g, _) = assemble(vec![a, b]);
        assert_eq!(g.nodes.iter().filter(|n| n.label == "run()").count(), 2);
    }

    #[test]
    fn markdown_links_connect_notes() {
        let index = extract_file(
            "memory/MEMORY.md",
            b"# Index\n- [Build](build.md)\n- [[idioms]]\n",
        );
        let build = extract_file("memory/build.md", b"# Build\n");
        let idioms = extract_file("memory/idioms.md", b"# Idioms\n");
        let (g, stats) = assemble(vec![index, build, idioms]);
        let refs: Vec<(&str, Confidence)> = g
            .edges
            .iter()
            .filter(|e| e.relation == "references")
            .map(|e| (g.nodes[e.dst].label.as_str(), e.confidence))
            .collect();
        assert_eq!(
            refs,
            [
                ("build.md", Confidence::Extracted),
                ("idioms.md", Confidence::Inferred)
            ]
        );
        assert_eq!(stats.imports_resolved, 2);
        assert!(
            g.nodes
                .iter()
                .any(|n| n.label == "Index" && n.kind == NodeKind::Document)
        );
    }

    #[test]
    fn duplicate_file_names_get_the_shortest_unique_path_suffix() {
        let a = extract_file("crates/a/src/error.rs", b"fn x() {}");
        let b = extract_file("crates/b/src/error.rs", b"fn y() {}");
        let c = extract_file("crates/c/src/only.rs", b"fn z() {}");
        let (g, _) = assemble(vec![a, b, c]);
        let labels: Vec<&str> = g
            .nodes
            .iter()
            .filter(|n| n.loc.as_deref() == Some("L1"))
            .map(|n| n.label.as_str())
            .collect();
        assert!(
            labels.contains(&"a/src/error.rs") && labels.contains(&"b/src/error.rs"),
            "{labels:?}"
        );
        assert!(labels.contains(&"only.rs"));
    }

    #[test]
    fn type_stubs_fold_onto_their_unique_definition() {
        let a = extract_file("a.rs", b"pub struct Thing;");
        let b = extract_file("b.rs", b"fn use_it(t: Thing) {}");
        let (g, stats) = assemble(vec![a, b]);
        assert_eq!(stats.stubs_folded, 1);
        let r = g
            .edges
            .iter()
            .find(|e| e.relation == "references")
            .expect("reference edge");
        assert_eq!(g.nodes[r.dst].file.as_deref(), Some("a.rs"));
        assert!(
            !g.nodes
                .iter()
                .any(|n| n.file.is_none() && n.label == "Thing")
        );
    }

    /// One crate `app` rooted at `src` (module paths strip the `src/` prefix).
    fn app(deps: &[&str]) -> AssembleContext {
        let mut context = AssembleContext::default();
        context
            .package_deps
            .insert("app".into(), deps.iter().map(|d| (*d).to_owned()).collect());
        context.package_dir.insert("app".into(), String::new());
        context
    }

    fn assemble_app(files: &[(&str, &str)], deps: &[&str]) -> (Graph, AssembleStats) {
        let mut context = app(deps);
        let extracts = files
            .iter()
            .map(|(path, source)| {
                context.package_of.insert((*path).to_owned(), "app".into());
                extract_file(path, source.as_bytes())
            })
            .collect();
        assemble_with(extracts, &context)
    }

    /// `(source label, source file, target label, target file, confidence)` of every `calls` or
    /// `imports_from` edge between nodes of the corpus.
    fn dependency_edges(g: &Graph) -> Vec<String> {
        let mut out: Vec<String> = g
            .edges
            .iter()
            .filter(|e| matches!(e.relation.as_str(), "calls" | "imports_from"))
            .map(|e| {
                let (s, d) = (&g.nodes[e.src], &g.nodes[e.dst]);
                format!(
                    "{} {}|{}|{}|{}|{}",
                    e.relation,
                    s.label,
                    s.file.as_deref().unwrap_or("-"),
                    d.label,
                    d.file.as_deref().unwrap_or("-"),
                    e.confidence.as_str()
                )
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn rust_paths_follow_modules_supers_aliases_reexports_and_typed_receivers() {
        let (g, stats) = assemble_app(
            &[
                (
                    "src/lib.rs",
                    "pub mod net;\npub mod util;\nmod store;\npub use store::Store;\n\
                     pub fn start() {\n util::helper();\n net::client::connect();\n let s = Store::new();\n s.save();\n}\n",
                ),
                (
                    "src/store.rs",
                    "pub struct Store { cache: Cache }\npub struct Cache;\nimpl Cache { pub fn hit(&self) {} }\n\
                     impl Store {\n pub fn new() -> Self { Store { cache: Cache } }\n pub fn save(&self) { self.cache.hit(); }\n}\n",
                ),
                ("src/util.rs", "pub fn helper() {}\npub fn other() {}\n"),
                ("src/net/mod.rs", "pub mod client;\n"),
                (
                    "src/net/client.rs",
                    "use crate::util::helper;\nuse super::super::util as u;\npub fn connect() {\n helper();\n crate::util::other();\n u::other();\n}\n",
                ),
                ("src/elsewhere.rs", "pub fn helper() {}\npub fn other() {}\npub struct Store;\nimpl Store { pub fn save(&self) {} }\n"),
            ],
            &[],
        );
        let wanted = [
            "calls connect()|src/net/client.rs|helper()|src/util.rs|INFERRED",
            "calls connect()|src/net/client.rs|other()|src/util.rs|INFERRED",
            "calls start()|src/lib.rs|.new()|src/store.rs|INFERRED",
            "calls start()|src/lib.rs|.save()|src/store.rs|INFERRED",
            "calls start()|src/lib.rs|connect()|src/net/client.rs|INFERRED",
            "calls start()|src/lib.rs|helper()|src/util.rs|INFERRED",
        ];
        let calls: Vec<String> = dependency_edges(&g)
            .into_iter()
            .filter(|e| e.starts_with("calls"))
            .collect();
        // `.save()` of `store.rs` calls `.hit()` through the typed field: an EXTRACTED edge of
        // the extractor, listed with the rest.
        let mut expected: Vec<String> = wanted.iter().map(|w| (*w).to_owned()).collect();
        expected.push("calls .save()|src/store.rs|.hit()|src/store.rs|EXTRACTED".into());
        expected.sort();
        assert_eq!(calls, expected);
        assert!(
            !g.edges.iter().any(|e| g.nodes[e.dst].file.as_deref() == Some("src/elsewhere.rs")
                && e.relation == "calls"),
            "no homonym of another module"
        );
        assert_eq!(stats.unresolved, 0, "{stats:?}");
    }

    #[test]
    fn rust_imports_link_files_to_definitions_modules_and_external_items() {
        let (g, stats) = assemble_app(
            &[
                ("src/lib.rs", "mod a;\nmod b;\nuse a::Thing;\nuse serde::Serialize;\nuse std::collections::HashMap;\nuse b::*;\n"),
                ("src/a.rs", "pub struct Thing;\n"),
                ("src/b.rs", "use super::Thing;\npub fn from_b() {}\n#[cfg(test)] mod tests { use super::*; }\n"),
            ],
            &["serde"],
        );
        assert_eq!(
            dependency_edges(&g),
            [
                "imports_from b.rs|src/b.rs|Thing|src/a.rs|EXTRACTED",
                "imports_from lib.rs|src/lib.rs|Thing|src/a.rs|EXTRACTED",
                "imports_from lib.rs|src/lib.rs|b.rs|src/b.rs|EXTRACTED",
                "imports_from lib.rs|src/lib.rs|serde::Serialize|-|EXTRACTED",
                "imports_from lib.rs|src/lib.rs|std::collections::HashMap|-|EXTRACTED",
            ],
            "{stats:?}"
        );
        assert_eq!((stats.uses_external, stats.uses_unresolved), (2, 0), "{stats:?}");
    }

    #[test]
    fn external_symbols_are_one_node_per_item_never_a_file_and_never_a_variant() {
        let (g, stats) = assemble_app(
            &[
                (
                    "src/error.rs",
                    "pub enum Error { Serialize(String), Deserialize(String) }\n",
                ),
                (
                    "src/lib.rs",
                    "mod error;\nuse serde::Deserialize;\nuse serde_json::Value;\n\
                     pub struct B;\nimpl Deserialize for B {}\n\
                     fn parse(v: Value, w: serde_json::Value) -> serde_json::Value {\n let x: Value = serde_json::from_str(\"1\").unwrap();\n std::fs::read(\"a\"); tokio::spawn(async {});\n x\n}\n",
                ),
            ],
            &["serde", "serde_json"],
        );
        let external: Vec<&str> = g
            .nodes
            .iter()
            .filter(|n| n.external)
            .map(|n| n.label.as_str())
            .collect();
        assert_eq!(
            external,
            ["serde::Deserialize", "serde_json::from_str", "serde_json::Value"],
            "one node per item"
        );
        assert!(g.nodes.iter().filter(|n| n.external).all(|n| n.file.is_none() && n.package.is_some()));
        let to = |relation: &str, label: &str| {
            g.edges
                .iter()
                .filter(|e| e.relation == relation && g.nodes[e.dst].label == label)
                .count()
        };
        assert_eq!(to("implements", "serde::Deserialize"), 1);
        assert_eq!(to("references", "serde_json::Value"), 3);
        assert_eq!(to("calls", "serde_json::from_str"), 1);
        assert!(
            !g.edges.iter().any(|e| {
                matches!(e.relation.as_str(), "references" | "implements" | "imports_from")
                    && g.nodes[e.dst].label == "Deserialize"
                    && g.nodes[e.dst].file.is_some()
            }),
            "a type reference never folds onto an enum variant"
        );
        // `std::fs::read` is never a call of the corpus; `tokio` is no dependency of the crate.
        assert_eq!(stats.external, 1, "{stats:?}");
        assert_eq!(stats.unresolved_by.get("path_unresolved"), Some(&1), "{stats:?}");
    }

    #[test]
    fn calls_through_local_bindings_and_std_derives_are_not_unresolved() {
        let (g, stats) = assemble_app(
            &[(
                "src/lib.rs",
                "#[derive(Default, Clone)]\npub struct Opts;\n\
                 fn f(cb: fn(), o: &Opts) {\n cb();\n drop(o);\n let a = Opts::default();\n let b = Opts::clone(o);\n missing();\n}\n",
            )],
            &[],
        );
        assert_eq!(stats.local, 1, "{stats:?}");
        assert_eq!(stats.external, 3, "drop, default and clone: {stats:?}");
        assert_eq!(stats.unresolved_by.get("no_definition"), Some(&1), "{stats:?}");
        assert!(!g.nodes.iter().any(|n| n.label == "Opts" && n.file.is_none()));
    }

    #[test]
    fn keywords_and_generic_parameters_never_become_nodes() {
        let (g, _) = assemble_app(
            &[
                ("src/lib.rs", "mod inner;\nuse self::inner::Item;\nfn run<T>(t: T, i: Item) {}\n"),
                ("src/inner.rs", "use super::*;\nuse crate::run;\npub struct Item<A> { a: A }\n"),
            ],
            &[],
        );
        for n in &g.nodes {
            assert!(
                !matches!(n.label.as_str(), "super" | "self" | "crate" | "Self" | "T" | "A"),
                "{n:?}"
            );
        }
        assert!(
            g.nodes.iter().all(|n| n.file.is_some() || n.external),
            "nothing is left without a file: {:?}",
            g.nodes.iter().filter(|n| n.file.is_none()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_type_alias_is_a_definition_references_resolve_to() {
        let (g, _) = assemble_app(
            &[
                ("src/lib.rs", "mod error;\nuse error::Result;\npub fn f() -> Result<u8> { Ok(1) }\n"),
                ("src/error.rs", "pub type Result<T> = std::result::Result<T, String>;\n"),
            ],
            &[],
        );
        assert!(g.edges.iter().any(|e| {
            e.relation == "references"
                && g.nodes[e.src].label == "f()"
                && g.nodes[e.dst].file.as_deref() == Some("src/error.rs")
        }));
    }

    fn ts_app(files: &[(&str, &str)], configs: Vec<TsConfig>) -> (Graph, AssembleStats) {
        let extracts = files
            .iter()
            .map(|(path, source)| extract_file(path, source.as_bytes()))
            .collect();
        assemble_with(
            extracts,
            &AssembleContext {
                ts_configs: configs,
                ..AssembleContext::default()
            },
        )
    }

    #[test]
    fn tsconfig_paths_and_base_url_resolve_non_relative_specifiers() {
        let files = [
            (
                "apps/web/src/main.ts",
                "import { helper } from '@lib/util';\nimport { other } from 'shared/other';\nexport function run() { helper(); other(); }\n",
            ),
            ("apps/web/src/lib/util.ts", "export function helper() {}\n"),
            ("apps/web/shared/other.ts", "export function other() {}\n"),
            ("packages/x/src/lib/util.ts", "export function helper() {}\n"),
        ];
        let config = TsConfig::parse(
            "apps/web",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@lib/*":["src/lib/*"]}}}"#,
            None,
        )
        .unwrap();
        let (g, stats) = ts_app(&files, vec![config]);
        assert_eq!(
            dependency_edges(&g)
                .into_iter()
                .filter(|e| e.starts_with("calls"))
                .collect::<Vec<_>>(),
            [
                "calls run()|apps/web/src/main.ts|helper()|apps/web/src/lib/util.ts|EXTRACTED",
                "calls run()|apps/web/src/main.ts|other()|apps/web/shared/other.ts|EXTRACTED",
            ]
        );
        assert_eq!((stats.imports_resolved, stats.imports_external), (2, 0), "{stats:?}");
        // Without the config both are packages outside the corpus.
        let (g, stats) = ts_app(&files, Vec::new());
        assert_eq!(stats.imports_external, 1, "`shared/other` is a unique path suffix: {stats:?}");
        assert!(g.nodes.iter().any(|n| n.external && n.label == "@lib/util"));
    }

    #[test]
    fn js_globals_and_callbacks_are_not_unresolved_and_packages_become_external_items() {
        let (g, stats) = ts_app(
            &[(
                "src/a.ts",
                "import { Hono } from 'hono';\nimport * as path from 'node:path';\nexport function f(cb: () => void) {\n cb();\n String(1);\n parseInt('2');\n path.join('a');\n real();\n}\n",
            )],
            Vec::new(),
        );
        assert_eq!(
            (stats.local, stats.external, stats.unresolved),
            (1, 3, 1),
            "{stats:?}"
        );
        let external: Vec<(&str, Option<&str>)> = g
            .nodes
            .iter()
            .filter(|n| n.external)
            .map(|n| (n.label.as_str(), n.package.as_deref()))
            .collect();
        assert_eq!(
            external,
            [
                ("hono", Some("hono")),
                ("hono::Hono", Some("hono")),
                ("node:path", Some("node:path")),
                ("node:path::join", Some("node:path")),
            ]
        );
    }
}
