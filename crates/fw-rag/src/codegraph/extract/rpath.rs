// SPDX-License-Identifier: Apache-2.0
//! The module tree of the Rust files of a corpus: where `crate::a::b::Name`, `super::x`,
//! `use` aliases and `pub use` re-exports lead.
//!
//! Paths are located (which crate, which module, which external crate) and then looked up in
//! the files of the module. A path that cannot be followed to a definition stays unknown; a
//! name is never guessed from another module.

use std::collections::{HashMap, HashSet};

use super::{
    NodeClass, RawNode, RawUse,
    rust::{is_std_root, strip_generics},
};

/// Deepest chain of aliases and re-exports followed.
const MAX_DEPTH: usize = 8;

/// A definition at the top level of a file.
#[derive(Debug, Clone)]
pub(super) struct Item {
    pub id: String,
    pub class: NodeClass,
}

/// What the resolver knows of one Rust file.
#[derive(Debug, Default)]
pub(super) struct RustFile {
    pub package: String,
    /// Module path of the file inside its crate (`src/a/mod.rs` is `["a"]`).
    pub module: Vec<String>,
    pub uses: Vec<RawUse>,
    /// Modules declared in the file (`mod a;`, `mod a { .. }`).
    pub mods: HashSet<String>,
    /// Top-level definitions by name.
    pub items: HashMap<String, Vec<Item>>,
}

/// What a name must be to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Want {
    Any,
    Type,
    Function,
}

impl Want {
    fn accepts(self, class: NodeClass) -> bool {
        match self {
            Self::Any => true,
            Self::Type => class == NodeClass::Type,
            Self::Function => class == NodeClass::Function,
        }
    }
}

/// Where a path starts once its leading modules are consumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Loc {
    /// Inside `module` of the crate `pkg`; `rest` still has to be found there.
    Internal {
        pkg: String,
        module: Vec<String>,
        rest: Vec<String>,
    },
    /// A crate outside the corpus; `std` for the standard library and primitives.
    External { krate: String, std: bool },
    Unknown,
}

/// What a path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Res {
    Item { id: String, file: String },
    Module { file: String },
    External { krate: String, std: bool },
    Unknown,
}

/// Module path of a Rust file inside its crate directory `dir` (`fs/mod.rs` gives `["fs"]`,
/// `lib.rs` gives `[]`); without a known directory, every path segment counts.
pub(super) fn module_segments<'f>(file: &'f str, dir: Option<&str>) -> Vec<&'f str> {
    let inside = dir
        .filter(|d| !d.is_empty())
        .and_then(|d| file.strip_prefix(d).and_then(|rest| rest.strip_prefix('/')))
        .unwrap_or(file);
    let inside = inside.strip_prefix("src/").unwrap_or(inside);
    let mut segs: Vec<&str> = inside
        .strip_suffix(".rs")
        .unwrap_or(inside)
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if segs
        .last()
        .is_some_and(|last| matches!(*last, "lib" | "main" | "mod"))
    {
        segs.pop();
    }
    if dir.is_none() {
        segs.retain(|s| *s != "src");
    }
    segs
}

/// The module tree of every Rust file of the corpus.
#[derive(Debug, Default)]
pub(super) struct ModIndex {
    files: HashMap<String, RustFile>,
    /// `(crate, module path)` to the files that are that module.
    modules: HashMap<(String, Vec<String>), Vec<String>>,
    /// Crate identifiers of the corpus.
    crates: HashSet<String>,
    /// Dependency identifiers declared by each crate manifest.
    deps: HashMap<String, HashSet<String>>,
    /// Every module name of a crate, file or inline.
    module_names: HashMap<String, HashSet<String>>,
}

impl ModIndex {
    pub fn new(crates: HashSet<String>, deps: HashMap<String, HashSet<String>>) -> Self {
        Self {
            crates,
            deps,
            ..Self::default()
        }
    }

    /// Register one Rust file.
    pub fn add_file(
        &mut self,
        path: &str,
        package: &str,
        package_dir: Option<&str>,
        nodes: &[RawNode],
        uses: &[RawUse],
        mods: &[String],
    ) {
        let module: Vec<String> = module_segments(path, package_dir)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut items: HashMap<String, Vec<Item>> = HashMap::new();
        for node in nodes.iter().filter(|n| n.file == path) {
            let name = match node.class {
                NodeClass::Function => node.label.strip_suffix("()"),
                NodeClass::Type | NodeClass::Const => Some(node.label.as_str()),
                _ => None,
            };
            let Some(name) = name.filter(|n| !n.starts_with('.')) else {
                continue;
            };
            let name = strip_generics(name);
            if name.is_empty() || name.contains(|c: char| !(c.is_alphanumeric() || c == '_')) {
                continue;
            }
            // An impl block shares its type's label but not its id; the declaration wins.
            let entry = items.entry(name).or_default();
            if !entry.iter().any(|i| i.id == node.id) {
                entry.push(Item {
                    id: node.id.clone(),
                    class: node.class,
                });
            }
        }
        let names = self.module_names.entry(package.to_owned()).or_default();
        names.extend(module.iter().cloned());
        names.extend(mods.iter().cloned());
        self.modules
            .entry((package.to_owned(), module.clone()))
            .or_default()
            .push(path.to_owned());
        self.files.insert(
            path.to_owned(),
            RustFile {
                package: package.to_owned(),
                module,
                uses: uses.to_vec(),
                mods: mods.iter().cloned().collect(),
                items,
            },
        );
    }

    pub fn finish(&mut self) {
        for files in self.modules.values_mut() {
            files.sort();
        }
    }

    /// Package of a file.
    pub fn package_of(&self, file: &str) -> Option<&str> {
        self.files.get(file).map(|f| f.package.as_str())
    }

    /// Uses of a file.
    pub fn uses_of(&self, file: &str) -> &[RawUse] {
        self.files.get(file).map_or(&[], |f| f.uses.as_slice())
    }

    fn files_of(&self, pkg: &str, module: &[String]) -> &[String] {
        self.modules
            .get(&(pkg.to_owned(), module.to_vec()))
            .map_or(&[], Vec::as_slice)
    }

    fn is_dependency(&self, pkg: &str, root: &str) -> bool {
        match self.deps.get(pkg).filter(|d| !d.is_empty()) {
            Some(deps) => deps.contains(root),
            // Without a manifest, a lower-case root that is no module of the crate is a crate.
            None => {
                root.starts_with(char::is_lowercase)
                    && !self
                        .module_names
                        .get(pkg)
                        .is_some_and(|names| names.contains(root))
            }
        }
    }

    /// Consume the leading modules of `rest`.
    fn descend(&self, pkg: &str, mut module: Vec<String>, rest: &[&str]) -> Loc {
        let mut i = 0;
        while i < rest.len() {
            let mut next = module.clone();
            next.push(rest[i].to_owned());
            if self.modules.contains_key(&(pkg.to_owned(), next.clone())) {
                module = next;
            } else if self
                .files_of(pkg, &module)
                .iter()
                .any(|f| self.files.get(f).is_some_and(|f| f.mods.contains(rest[i])))
            {
                // An inline module: its items live in the file of the enclosing module.
            } else {
                break;
            }
            i += 1;
        }
        Loc::Internal {
            pkg: pkg.to_owned(),
            module,
            rest: rest[i..].iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Locate the start of a path written in `file`, `inline` blocks deep.
    pub fn locate(&self, file: &str, inline: u8, segs: &[&str]) -> Loc {
        self.locate_at(file, inline, segs, 0)
    }

    fn locate_at(&self, file: &str, inline: u8, segs: &[&str], depth: usize) -> Loc {
        let (Some(info), Some(&root)) = (self.files.get(file), segs.first()) else {
            return Loc::Unknown;
        };
        let pkg = info.package.as_str();
        match root {
            "crate" => self.descend(pkg, Vec::new(), &segs[1..]),
            "self" => self.descend(pkg, info.module.clone(), &segs[1..]),
            "super" => {
                let supers = segs.iter().take_while(|s| **s == "super").count();
                let mut module = info.module.clone();
                for _ in 0..supers.saturating_sub(usize::from(inline)) {
                    if module.pop().is_none() {
                        return Loc::Unknown;
                    }
                }
                self.descend(pkg, module, &segs[supers..])
            }
            r if is_std_root(r) => Loc::External {
                krate: r.to_owned(),
                std: true,
            },
            r if self.crates.contains(r) => self.descend(r, Vec::new(), &segs[1..]),
            r => {
                let mut child = info.module.clone();
                child.push(r.to_owned());
                if info.mods.contains(r)
                    || self.modules.contains_key(&(pkg.to_owned(), child.clone()))
                {
                    return self.descend(pkg, info.module.clone(), segs);
                }
                if depth < MAX_DEPTH
                    && let Some(alias) = info.uses.iter().find(|u| !u.glob && u.local == r)
                {
                    let target: Vec<&str> = alias
                        .path
                        .split("::")
                        .chain(segs[1..].iter().copied())
                        .collect();
                    if target.first() != Some(&r) {
                        return self.locate_at(file, inline, &target, depth + 1);
                    }
                }
                if self.is_dependency(pkg, r) {
                    Loc::External {
                        krate: r.to_owned(),
                        std: false,
                    }
                } else {
                    Loc::Unknown
                }
            }
        }
    }

    /// Resolve a whole path written in `file`.
    pub fn resolve(&self, file: &str, inline: u8, path: &str, want: Want) -> Res {
        let segs: Vec<&str> = path.split("::").filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            return Res::Unknown;
        }
        match self.locate(file, inline, &segs) {
            Loc::External { krate, std } => Res::External { krate, std },
            Loc::Unknown => Res::Unknown,
            Loc::Internal { pkg, module, rest } => self.in_module(&pkg, &module, &rest, want, 0),
        }
    }

    /// What `rest` (starting at its first segment) names inside a module.
    pub fn in_module(
        &self,
        pkg: &str,
        module: &[String],
        rest: &[String],
        want: Want,
        depth: usize,
    ) -> Res {
        let Some(name) = rest.first() else {
            return self
                .files_of(pkg, module)
                .first()
                .map_or(Res::Unknown, |file| Res::Module { file: file.clone() });
        };
        // `Type::method` and `Enum::Variant` name the type.
        let want = if rest.len() > 1 { Want::Type } else { want };
        let files = self.files_of(pkg, module);
        let mut found: Vec<(&Item, &String)> = Vec::new();
        for file in files {
            if let Some(info) = self.files.get(file)
                && let Some(items) = info.items.get(name.as_str())
            {
                found.extend(items.iter().filter(|i| want.accepts(i.class)).map(|i| (i, file)));
            }
        }
        match found.as_slice() {
            [(item, file)] => {
                return Res::Item {
                    id: item.id.clone(),
                    file: (*file).clone(),
                };
            }
            [_, _, ..] => return Res::Unknown,
            [] => {}
        }
        if rest.len() == 1 {
            let mut child = module.to_vec();
            child.push(name.clone());
            if let Some(file) = self.modules.get(&(pkg.to_owned(), child)).and_then(|f| f.first()) {
                return Res::Module { file: file.clone() };
            }
        }
        if depth >= MAX_DEPTH {
            return Res::Unknown;
        }
        for file in files {
            let Some(info) = self.files.get(file) else {
                continue;
            };
            for u in info.uses.iter().filter(|u| !u.glob && &u.local == name) {
                let segs: Vec<&str> = u.path.split("::").collect();
                let tail: Vec<&str> = segs
                    .iter()
                    .copied()
                    .chain(rest[1..].iter().map(String::as_str))
                    .collect();
                if tail.first() == Some(&name.as_str()) && tail.len() == rest.len() {
                    continue;
                }
                match self.locate_at(file, u.inline, &tail, depth + 1) {
                    Loc::Internal { pkg, module, rest } => {
                        let res = self.in_module(&pkg, &module, &rest, want, depth + 1);
                        if res != Res::Unknown {
                            return res;
                        }
                    }
                    Loc::External { krate, std } => return Res::External { krate, std },
                    Loc::Unknown => {}
                }
            }
        }
        for file in files {
            let Some(info) = self.files.get(file) else {
                continue;
            };
            for u in info.uses.iter().filter(|u| u.glob && u.public) {
                if let Some(id) = self.through_glob(file, u, rest, want, depth) {
                    return id;
                }
            }
        }
        Res::Unknown
    }

    fn through_glob(&self, file: &str, glob: &RawUse, rest: &[String], want: Want, depth: usize) -> Option<Res> {
        let tail: Vec<&str> = glob
            .path
            .split("::")
            .chain(rest.iter().map(String::as_str))
            .collect();
        match self.locate_at(file, glob.inline, &tail, depth + 1) {
            Loc::Internal { pkg, module, rest } => {
                match self.in_module(&pkg, &module, &rest, want, depth + 1) {
                    res @ Res::Item { .. } => Some(res),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A name used without a path in `file`: found through the glob imports of the file.
    pub fn bare(&self, file: &str, name: &str, want: Want) -> Option<Res> {
        let info = self.files.get(file)?;
        let rest = [name.to_owned()];
        for glob in info.uses.iter().filter(|u| u.glob) {
            let segs: Vec<&str> = glob.path.split("::").collect();
            if let Loc::Internal { pkg, module, rest: left } =
                self.locate_at(file, glob.inline, &segs, 0)
                && left.is_empty()
                && let res @ Res::Item { .. } = self.in_module(&pkg, &module, &rest, want, 1)
            {
                return Some(res);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::extract::extract_file;

    fn index(files: &[(&str, &str)], crates: &[(&str, &str)]) -> ModIndex {
        let names: HashSet<String> = crates.iter().map(|(_, k)| (*k).to_owned()).collect();
        let mut index = ModIndex::new(names, HashMap::new());
        for (path, source) in files {
            let extract = extract_file(path, source.as_bytes());
            let (package, dir) = crates
                .iter()
                .find(|(dir, _)| path.starts_with(dir))
                .map_or(("", None), |(dir, name)| (*name, Some(*dir)));
            index.add_file(path, package, dir, &extract.nodes, &extract.uses, &extract.mods);
        }
        index.finish();
        index
    }

    #[test]
    fn paths_follow_modules_supers_and_reexports() {
        let index = index(
            &[
                ("src/a/lib.rs", "pub mod store;\npub mod util;\npub use store::Store;\n"),
                ("src/a/store.rs", "pub struct Store;\npub fn open() {}\n"),
                ("src/a/util.rs", "use super::Store;\nuse crate::store::open;\npub fn help() {}\n"),
                ("src/b/lib.rs", "use a::Store;\nuse a::store::open as open_it;\nfn f() {}\n"),
            ],
            &[("src/a", "a"), ("src/b", "b")],
        );
        let item = |file: &str, path: &str, want| match index.resolve(file, 0, path, want) {
            Res::Item { file, .. } => file,
            other => format!("{other:?}"),
        };
        assert_eq!(item("src/b/lib.rs", "a::Store", Want::Type), "src/a/store.rs");
        assert_eq!(item("src/b/lib.rs", "a::store::open", Want::Function), "src/a/store.rs");
        assert_eq!(item("src/a/util.rs", "super::Store", Want::Type), "src/a/store.rs");
        assert_eq!(item("src/a/util.rs", "crate::store::open", Want::Function), "src/a/store.rs");
        assert_eq!(item("src/a/lib.rs", "store::Store", Want::Type), "src/a/store.rs");
        assert!(matches!(
            index.resolve("src/a/lib.rs", 0, "util", Want::Any),
            Res::Module { file } if file == "src/a/util.rs"
        ));
    }

    #[test]
    fn external_roots_are_classified_and_unknown_names_stay_unknown() {
        let index = index(
            &[("src/a/lib.rs", "use serde::Serialize;\nmod inner;\npub struct T;\n"), ("src/a/inner.rs", "pub fn f() {}\n")],
            &[("src/a", "a")],
        );
        assert_eq!(
            index.resolve("src/a/lib.rs", 0, "serde::Serialize", Want::Any),
            Res::External { krate: "serde".into(), std: false }
        );
        assert_eq!(
            index.resolve("src/a/lib.rs", 0, "std::fmt::Display", Want::Any),
            Res::External { krate: "std".into(), std: true }
        );
        assert_eq!(
            index.resolve("src/a/lib.rs", 0, "u32::try_from", Want::Any),
            Res::External { krate: "u32".into(), std: true }
        );
        assert!(matches!(index.resolve("src/a/lib.rs", 0, "inner::f", Want::Function), Res::Item { .. }));
        assert_eq!(index.resolve("src/a/lib.rs", 0, "inner::missing", Want::Any), Res::Unknown);
        assert_eq!(index.resolve("src/a/lib.rs", 0, "crate::nothing::here", Want::Any), Res::Unknown);
        // `inner` is a module of the crate, so it is not an external crate.
        assert!(matches!(index.resolve("src/a/lib.rs", 0, "inner", Want::Any), Res::Module { .. }));
    }
}
