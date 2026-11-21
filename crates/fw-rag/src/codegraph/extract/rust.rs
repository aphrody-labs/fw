// SPDX-License-Identifier: Apache-2.0
//! Rust extraction with tree-sitter: files, functions, methods, types, traits, impl blocks,
//! macros, `use` imports, type references of signatures and call sites.

use std::collections::{HashMap, HashSet};

use tree_sitter::{Node, Parser};

use super::{
    FileExtract, NodeClass, RawCall, RawEdge, RawNode, RawUse, decode_text,
    ids::{file_stem, make_id},
};
use crate::codegraph::graph::Confidence;

/// Std and prelude names that would otherwise become hubs of every Rust corpus.
const BUILTIN_TYPES: &[&str] = &[
    "String",
    "str",
    "Option",
    "Result",
    "Vec",
    "VecDeque",
    "Box",
    "Rc",
    "Arc",
    "Weak",
    "RefCell",
    "Cell",
    "Cow",
    "Pin",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "BinaryHeap",
    "Path",
    "PathBuf",
    "OsStr",
    "OsString",
    "CStr",
    "CString",
    "Duration",
    "Instant",
    "SystemTime",
    "Ordering",
    "Range",
    "RangeInclusive",
    "PhantomData",
    "Some",
    "None",
    "Ok",
    "Err",
    "Self",
    "Default",
    "Clone",
    "Copy",
    "Debug",
    "Display",
    "Error",
    "From",
    "Into",
    "TryFrom",
    "TryInto",
    "AsRef",
    "AsMut",
    "Iterator",
    "IntoIterator",
    "Extend",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Send",
    "Sync",
    "Sized",
    "Drop",
    "Deref",
    "DerefMut",
    "Future",
    "Fn",
    "FnMut",
    "FnOnce",
];

/// Method names too generic to resolve across files (every type has one).
const TRAIT_METHOD_BLOCKLIST: &[&str] = &[
    "new",
    "default",
    "parse",
    "from_str",
    "now",
    "clone",
    "into",
    "from",
    "to_string",
    "to_owned",
    "len",
    "is_empty",
    "iter",
    "next",
    "build",
    "start",
    "run",
    "init",
    "app",
    "get",
    "set",
    "push",
    "pop",
    "insert",
    "remove",
    "contains",
    "collect",
    "map",
    "filter",
    "unwrap",
    "expect",
    "ok",
    "err",
    "some",
    "none",
    "send",
    "recv",
    "lock",
    "read",
    "write",
];

/// Whether a callee name is too generic to be resolved across files.
#[must_use]
pub fn is_blocklisted(name: &str) -> bool {
    TRAIT_METHOD_BLOCKLIST.contains(&name)
}

/// Path roots of the standard library; calls through them are never corpus calls.
const STD_QUALIFIERS: &[&str] = &["std", "core", "alloc"];

/// Primitive types: `u32::try_from(x)` is a std call.
const PRIMITIVES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32",
    "f64", "bool", "char", "str",
];

/// Path words that name a place, never a symbol.
const PATH_KEYWORDS: &[&str] = &["crate", "self", "super", "Self"];

/// Whether a path root is the standard library or a primitive type.
#[must_use]
pub fn is_std_root(root: &str) -> bool {
    STD_QUALIFIERS.contains(&root) || PRIMITIVES.contains(&root)
}

/// Associated functions whose result is the type they are called on (`T::new()`).
fn is_constructor(name: &str) -> bool {
    is_plain_constructor(name) || matches!(name, "create" | "open" | "init")
}

/// Constructors whose result is the plain type; the others usually return a `Result`.
fn is_plain_constructor(name: &str) -> bool {
    name.starts_with("new") || name.starts_with("with_") || name == "default"
}

/// `&mut Foo<T>` or `Foo::<T>` gives `Foo`; generic arguments anywhere in a path are dropped.
#[must_use]
pub fn strip_generics(path: &str) -> String {
    let mut s = path.trim();
    loop {
        let next = s.trim_start_matches('&').trim_start();
        let next = ["mut ", "dyn ", "impl ", "'static "]
            .iter()
            .fold(next, |acc, p| acc.strip_prefix(p).unwrap_or(acc))
            .trim_start();
        if next == s {
            break;
        }
        s = next;
    }
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 && !c.is_whitespace() => out.push(c),
            _ => {}
        }
    }
    out.trim_end_matches("::").to_owned()
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// `std::fs`, `core::mem::take`, `u32::try_from`, or a bare std/prelude type such as `Vec`.
#[must_use]
pub fn is_std_path(path: &str) -> bool {
    let first = path.split("::").next().unwrap_or(path);
    is_std_root(first) || (!path.contains("::") && BUILTIN_TYPES.contains(&path))
}

fn join_path(head: &str, rest: &str) -> String {
    let rest = rest
        .strip_prefix("self")
        .filter(|r| r.is_empty() || r.starts_with("::"))
        .map_or(rest, |r| r.trim_start_matches("::"));
    match (head.is_empty(), rest.is_empty()) {
        (true, _) => rest.to_owned(),
        (_, true) => head.to_owned(),
        _ => format!("{head}::{rest}"),
    }
}

/// One name of a `use` declaration: `(local name, full path, glob)`.
type UseLeaf = (String, String, bool);

/// Record the names a `use` declaration brings into scope.
fn collect_uses(node: Node<'_>, src: &[u8], prefix: &str, out: &mut Vec<UseLeaf>) {
    match node.kind() {
        "scoped_use_list" => {
            let path = node
                .child_by_field_name("path")
                .map_or("", |n| text(n, src));
            let full = join_path(prefix, path);
            if let Some(list) = node.child_by_field_name("list") {
                collect_uses(list, src, &full, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_uses(child, src, prefix, out);
            }
        }
        "use_as_clause" => {
            if let (Some(path), Some(alias)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("alias"),
            ) {
                let full = join_path(prefix, text(path, src));
                out.push((text(alias, src).to_owned(), full, false));
            }
        }
        "use_wildcard" => {
            let path = text(node, src).trim_end_matches('*').trim_end_matches("::");
            out.push((String::new(), join_path(prefix, path), true));
        }
        "scoped_identifier" | "identifier" | "crate" | "super" => {
            let full = join_path(prefix, text(node, src));
            let name = last_segment(&full).to_owned();
            if !PATH_KEYWORDS.contains(&name.as_str()) || full.contains("::") {
                out.push((name, full, false));
            }
        }
        "self" => {
            let name = last_segment(prefix).to_owned();
            if !prefix.is_empty() {
                out.push((name, prefix.to_owned(), false));
            }
        }
        _ => {}
    }
}

/// A function body whose calls are collected once every node of the file is known.
struct Body<'a> {
    nid: String,
    func: Node<'a>,
    body: Node<'a>,
    impl_type: Option<String>,
    inline: u8,
}

/// A type reference of a signature or a field.
struct TypeRef {
    name: String,
    generic: bool,
    /// The path the file spells for it, expanded through its `use` aliases.
    path: Option<String>,
}

/// The type of an expression: `wrapped` when it is a `Result` or `Option` of `ty`.
struct Typed {
    ty: String,
    wrapped: bool,
}

struct Ctx<'a> {
    rel: &'a str,
    stem: &'a str,
    src: &'a [u8],
    file_nid: String,
    out: FileExtract,
    seen: HashSet<String>,
    local_types: HashSet<String>,
    bodies: Vec<Body<'a>>,
    macros: HashMap<String, String>,
    labels: HashMap<String, String>,
    type_nids: HashSet<String>,
    pairs: HashSet<(String, String)>,
    /// `use` aliases: local name to full path.
    aliases: HashMap<String, String>,
    /// Types of the bindings of the current body (`self`, typed parameters, typed `let`s).
    locals: HashMap<String, String>,
    /// Names bound in the current body (parameters, `let`, closures, `for`, `match` arms).
    bound: HashSet<String>,
    /// Base type of the impl block being walked.
    impl_type: Option<String>,
    /// Generic parameter names declared in the file (`T`, `S`): never types of the corpus.
    generics: HashSet<String>,
    /// Field types of the structs of this file: `(struct, field)` to the type path.
    fields: HashMap<(String, String), String>,
    /// Return types of the functions of this file: `(impl type, name)` to the type.
    rets: HashMap<(Option<String>, String), Typed>,
    /// Inline `mod` blocks around the node being visited.
    inline: u8,
}

fn text<'a>(node: Node<'_>, src: &'a [u8]) -> &'a str {
    std::str::from_utf8(&src[node.start_byte()..node.end_byte()])
        .expect("tree-sitter node ranges must be UTF-8 boundaries")
}

fn line(node: Node<'_>) -> usize {
    node.start_position().row + 1
}

/// Smart pointers a method call sees through.
fn is_pointer(name: &str) -> bool {
    matches!(name, "Arc" | "Rc" | "Box")
}

impl<'a> Ctx<'a> {
    fn add_node(&mut self, id: &str, label: &str, line: usize, class: NodeClass) {
        if self.seen.insert(id.to_owned()) {
            self.out.nodes.push(RawNode {
                id: id.to_owned(),
                label: label.to_owned(),
                file: self.rel.to_owned(),
                loc: format!("L{line}"),
                class,
            });
        }
    }

    fn add_edge(
        &mut self,
        src: &str,
        dst: &str,
        relation: &str,
        line: usize,
        context: Option<&str>,
    ) {
        self.out.edges.push(RawEdge {
            src: src.to_owned(),
            dst: dst.to_owned(),
            relation: relation.to_owned(),
            confidence: Confidence::Extracted,
            score: 1.0,
            file: self.rel.to_owned(),
            loc: format!("L{line}"),
            context: context.map(str::to_owned),
            path: None,
        });
    }

    /// Node id for a type or function name: local when defined in this file, otherwise a
    /// sourceless stub resolved at corpus level.
    fn named(&mut self, name: &str) -> String {
        let local = make_id(&[self.stem, name]);
        if self.seen.contains(&local) || self.local_types.contains(name) {
            return local;
        }
        let stub = make_id(&[name]);
        if self.seen.insert(stub.clone()) {
            self.out.nodes.push(RawNode {
                id: stub.clone(),
                label: name.to_owned(),
                file: String::new(),
                loc: String::new(),
                class: NodeClass::Stub,
            });
        } else if let Some(node) = self
            .out
            .nodes
            .iter_mut()
            .find(|n| n.id == stub && n.file.is_empty() && n.label == stub)
        {
            node.label = name.to_owned();
        }
        stub
    }

    /// Whether a type name is worth a node: not a keyword, a generic parameter or a std type.
    fn keep(&self, name: &str, path: Option<&str>) -> bool {
        if name.is_empty() || PATH_KEYWORDS.contains(&name) {
            return false;
        }
        if self.local_types.contains(name) {
            return true;
        }
        if self.generics.contains(name) {
            return false;
        }
        match path {
            Some(path) => !is_std_path(path),
            None => !BUILTIN_TYPES.contains(&name),
        }
    }

    fn type_refs(&self, node: Option<Node<'_>>, generic: bool, out: &mut Vec<TypeRef>) {
        let Some(node) = node else { return };
        match node.kind() {
            "primitive_type" => {}
            // `Stream<Item = T>`: the name is an associated type, only `T` is a reference.
            "type_binding" => self.type_refs(node.child_by_field_name("type"), true, out),
            // `<T as Trait>::Out`: the type and the trait are references, `Out` is not.
            "qualified_type" => {
                for field in ["type", "alias"] {
                    self.type_refs(node.child_by_field_name(field), generic, out);
                }
            }
            "type_identifier" => {
                let t = text(node, self.src);
                let path = self.aliases.get(t).cloned();
                if self.keep(t, path.as_deref()) {
                    out.push(TypeRef {
                        name: t.to_owned(),
                        generic,
                        path,
                    });
                }
            }
            "scoped_type_identifier" => {
                if let Some(path) = node
                    .child_by_field_name("path")
                    .filter(|p| p.kind() == "bracketed_type")
                {
                    // `<T as Trait>::Out`: the qualifying types are references, `Out` is not.
                    self.type_refs(Some(path), generic, out);
                    return;
                }
                let full = text(node, self.src);
                let first = full.split("::").next().unwrap_or(full);
                if first == "Self" || (self.generics.contains(first) && !self.local_types.contains(first)) {
                    // `Self::Item` and `T::Output` are associated types, not types of the corpus.
                    return;
                }
                let t = last_segment(full);
                let path = self.expand(full);
                if self.keep(t, Some(&path)) {
                    out.push(TypeRef {
                        name: t.to_owned(),
                        generic,
                        path: Some(path),
                    });
                }
            }
            "generic_type" => {
                if let Some(name) = node.child_by_field_name("type") {
                    self.type_refs(Some(name), generic, out);
                }
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "type_arguments" {
                        let mut inner = child.walk();
                        for arg in child.children(&mut inner) {
                            if arg.is_named() {
                                self.type_refs(Some(arg), true, out);
                            }
                        }
                    }
                }
            }
            _ if node.is_named() => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.is_named() {
                        self.type_refs(Some(child), generic, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// A `references`-like edge to the type of `tr`, carrying the path the file spelled for it.
    fn ref_edge(&mut self, src: &str, tr: &TypeRef, relation: &str, context: &str, at: usize) {
        let target = self.named(&tr.name);
        if target == src {
            return;
        }
        let path = tr
            .path
            .clone()
            .filter(|_| target != make_id(&[self.stem, &tr.name]));
        self.out.edges.push(RawEdge {
            src: src.to_owned(),
            dst: target,
            relation: relation.to_owned(),
            confidence: Confidence::Extracted,
            score: 1.0,
            file: self.rel.to_owned(),
            loc: format!("L{at}"),
            context: (!context.is_empty()).then(|| context.to_owned()),
            path,
        });
    }

    fn signature_refs(&mut self, func: Node<'a>, func_nid: &str, at: usize) {
        let mut refs: Vec<(TypeRef, &str)> = Vec::new();
        if let Some(params) = func.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            for p in params.children(&mut cursor) {
                if p.kind() == "parameter" {
                    let mut found = Vec::new();
                    self.type_refs(p.child_by_field_name("type"), false, &mut found);
                    refs.extend(found.into_iter().map(|t| (t, "parameter_type")));
                }
            }
        }
        let mut found = Vec::new();
        self.type_refs(func.child_by_field_name("return_type"), false, &mut found);
        refs.extend(found.into_iter().map(|t| (t, "return_type")));
        for (tr, ctx) in refs {
            let context = if tr.generic { "generic_arg" } else { ctx };
            self.ref_edge(func_nid, &tr, "references", context, at);
        }
    }

    /// First pass: local types, generic parameters, `use` declarations and declared modules.
    fn scan_local_types(&mut self, node: Node<'a>) {
        match node.kind() {
            "struct_item" | "enum_item" | "trait_item" | "union_item" | "type_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.local_types.insert(text(name, self.src).to_owned());
                }
            }
            "type_parameter" | "optional_type_parameter" | "const_parameter" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.generics.insert(text(name, self.src).to_owned());
                }
            }
            "constrained_type_parameter" => {
                if let Some(name) = node.child_by_field_name("left") {
                    self.generics.insert(text(name, self.src).to_owned());
                }
            }
            "use_declaration" => self.record_use(node),
            "mod_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    if !self.out.mods.contains(&name) {
                        self.out.mods.push(name);
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.inline = self.inline.saturating_add(1);
                    self.scan_local_types(body);
                    self.inline = self.inline.saturating_sub(1);
                }
                return;
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.scan_local_types(child);
        }
    }

    fn record_use(&mut self, node: Node<'a>) {
        let Some(arg) = node.child_by_field_name("argument") else {
            return;
        };
        let mut cursor = node.walk();
        let public = node
            .children(&mut cursor)
            .any(|c| c.kind() == "visibility_modifier");
        let mut leaves = Vec::new();
        collect_uses(arg, self.src, "", &mut leaves);
        for (local, path, glob) in leaves {
            if path.is_empty() {
                continue;
            }
            if !glob && local != "_" {
                self.aliases.insert(local.clone(), path.clone());
            }
            let entry = RawUse {
                local,
                path,
                glob,
                public,
                inline: self.inline,
                loc: format!("L{}", line(node)),
            };
            if !self.out.uses.contains(&entry) {
                self.out.uses.push(entry);
            }
        }
    }

    /// Second pass: field types of structs and return types of functions, once the local types
    /// and the `use` aliases are known.
    fn scan_signatures(&mut self, node: Node<'a>) {
        match node.kind() {
            "struct_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    self.scan_fields(node, &name);
                }
            }
            "impl_item" => {
                let outer = self.impl_type.take();
                self.impl_type = node
                    .child_by_field_name("type")
                    .map(|t| strip_generics(text(t, self.src)));
                if let Some(body) = node.child_by_field_name("body") {
                    self.scan_signatures(body);
                }
                self.impl_type = outer;
                return;
            }
            "function_item" | "function_signature_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    if let Some(ret) = self.return_type(node) {
                        self.rets.insert((self.impl_type.clone(), name), ret);
                    }
                }
                return;
            }
            "trait_item" => return,
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.scan_signatures(child);
        }
    }

    fn scan_fields(&mut self, node: Node<'a>, owner: &str) {
        let mut cursor = node.walk();
        let lists: Vec<Node<'a>> = node.children(&mut cursor).collect();
        for list in lists {
            let mut inner = list.walk();
            match list.kind() {
                "field_declaration_list" => {
                    for field in list.children(&mut inner) {
                        if field.kind() == "field_declaration"
                            && let (Some(name), Some(ty)) = (
                                field.child_by_field_name("name"),
                                field.child_by_field_name("type"),
                            )
                            && let Some(path) = self.type_path(ty)
                        {
                            self.fields
                                .insert((owner.to_owned(), text(name, self.src).to_owned()), path);
                        }
                    }
                }
                "ordered_field_declaration_list" => {
                    for (index, ty) in list.children(&mut inner).filter(|n| n.is_named()).enumerate() {
                        if let Some(path) = self.type_path(ty) {
                            self.fields.insert((owner.to_owned(), index.to_string()), path);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Declared return type of a function: `Self` and `T` give `T`; `Result<T, _>` and
    /// `Option<T>` give `T` marked wrapped.
    fn return_type(&self, func: Node<'_>) -> Option<Typed> {
        let ret = func.child_by_field_name("return_type")?;
        if ret.kind() == "generic_type"
            && let Some(name) = ret.child_by_field_name("type")
            && matches!(last_segment(&strip_generics(text(name, self.src))), "Result" | "Option")
        {
            let mut cursor = ret.walk();
            let args = ret
                .children(&mut cursor)
                .find(|c| c.kind() == "type_arguments")?;
            let mut inner = args.walk();
            let first = args.children(&mut inner).find(|c| c.is_named())?;
            return self.type_path(first).map(|ty| Typed { ty, wrapped: true });
        }
        self.type_path(ret).map(|ty| Typed { ty, wrapped: false })
    }

    fn emit_type_refs(&mut self, owner: &str, node: Option<Node<'a>>, at: usize) {
        let mut refs = Vec::new();
        self.type_refs(node, false, &mut refs);
        for tr in refs {
            let context = if tr.generic { "generic_arg" } else { "field" };
            self.ref_edge(owner, &tr, "references", context, at);
        }
    }

    fn struct_field_refs(&mut self, node: Node<'a>, owner: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
        for c in children {
            match c.kind() {
                "field_declaration_list" => {
                    let mut inner = c.walk();
                    let fields: Vec<Node<'a>> = c.children(&mut inner).collect();
                    for field in fields {
                        if field.kind() == "field_declaration" {
                            self.emit_type_refs(
                                owner,
                                field.child_by_field_name("type"),
                                line(field),
                            );
                        }
                    }
                }
                "ordered_field_declaration_list" => {
                    let at = line(c);
                    let mut inner = c.walk();
                    let types: Vec<Node<'a>> =
                        c.children(&mut inner).filter(|n| n.is_named()).collect();
                    for ty in types {
                        self.emit_type_refs(owner, Some(ty), at);
                    }
                }
                _ => {}
            }
        }
    }

    fn enum_variants(&mut self, node: Node<'a>, owner: &str) {
        let mut cursor = node.walk();
        let lists: Vec<Node<'a>> = node
            .children(&mut cursor)
            .filter(|c| c.kind() == "enum_variant_list")
            .collect();
        for list in lists {
            let mut inner = list.walk();
            let variants: Vec<Node<'a>> = list
                .children(&mut inner)
                .filter(|v| v.kind() == "enum_variant")
                .collect();
            for variant in variants {
                let at = line(variant);
                let mut vc = variant.walk();
                let parts: Vec<Node<'a>> = variant.children(&mut vc).collect();
                if let Some(id) = parts.iter().find(|p| p.kind() == "identifier") {
                    let name = text(*id, self.src).to_owned();
                    if !name.is_empty() {
                        let nid = make_id(&[owner, &name]);
                        self.add_node(&nid, &name, at, NodeClass::Variant);
                        self.add_edge(owner, &nid, "case_of", at, None);
                    }
                }
                for part in parts {
                    match part.kind() {
                        "ordered_field_declaration_list" => {
                            let mut pc = part.walk();
                            let types: Vec<Node<'a>> =
                                part.children(&mut pc).filter(|n| n.is_named()).collect();
                            for ty in types {
                                self.emit_type_refs(owner, Some(ty), at);
                            }
                        }
                        "field_declaration_list" => {
                            let mut pc = part.walk();
                            let fields: Vec<Node<'a>> = part.children(&mut pc).collect();
                            for field in fields {
                                if field.kind() == "field_declaration" {
                                    self.emit_type_refs(
                                        owner,
                                        field.child_by_field_name("type"),
                                        line(field),
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn walk(&mut self, node: Node<'a>, impl_nid: Option<&str>) {
        match node.kind() {
            "function_item" | "function_signature_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    let at = line(node);
                    let nid = if let Some(parent) = impl_nid {
                        let nid = make_id(&[parent, &name]);
                        self.add_node(&nid, &format!(".{name}()"), at, NodeClass::Function);
                        self.add_edge(parent, &nid, "method", at, None);
                        nid
                    } else {
                        let nid = make_id(&[self.stem, &name]);
                        self.add_node(&nid, &format!("{name}()"), at, NodeClass::Function);
                        let file = self.file_nid.clone();
                        self.add_edge(&file, &nid, "contains", at, None);
                        nid
                    };
                    self.signature_refs(node, &nid, at);
                    if let Some(body) = node.child_by_field_name("body") {
                        let impl_type = impl_nid.and_then(|_| self.impl_type.clone());
                        self.bodies.push(Body {
                            nid,
                            func: node,
                            body,
                            impl_type,
                            inline: self.inline,
                        });
                    }
                }
            }
            "macro_definition" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    let at = line(node);
                    let nid = make_id(&[self.stem, "macro", &name]);
                    self.add_node(&nid, &format!("{name}!"), at, NodeClass::Macro);
                    self.macros
                        .entry(name.clone())
                        .or_insert_with(|| nid.clone());
                    let file = self.file_nid.clone();
                    self.add_edge(&file, &nid, "contains", at, None);
                }
            }
            "struct_item" | "enum_item" | "trait_item" | "union_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    let at = line(node);
                    let nid = make_id(&[self.stem, &name]);
                    self.add_node(&nid, &name, at, NodeClass::Type);
                    let file = self.file_nid.clone();
                    self.add_edge(&file, &nid, "contains", at, None);
                    if node.kind() == "struct_item" {
                        self.struct_field_refs(node, &nid);
                    }
                    if node.kind() == "enum_item" {
                        self.enum_variants(node, &nid);
                    }
                    if node.kind() == "trait_item" {
                        let mut cursor = node.walk();
                        for child in node.children(&mut cursor) {
                            if child.kind() == "trait_bounds" {
                                let mut refs = Vec::new();
                                self.type_refs(Some(child), false, &mut refs);
                                for tr in refs {
                                    self.ref_edge(&nid, &tr, "inherits", "", at);
                                }
                            }
                        }
                        if let Some(body) = node.child_by_field_name("body") {
                            self.walk_children(body, Some(&nid));
                        }
                    }
                }
            }
            "type_item" if impl_nid.is_none() => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    let at = line(node);
                    let nid = make_id(&[self.stem, &name]);
                    self.add_node(&nid, &name, at, NodeClass::Type);
                    let file = self.file_nid.clone();
                    self.add_edge(&file, &nid, "contains", at, None);
                    self.emit_type_refs(&nid, node.child_by_field_name("type"), at);
                }
            }
            "impl_item" => {
                let Some(ty) = node.child_by_field_name("type") else {
                    return;
                };
                let at = line(node);
                let type_name = text(ty, self.src).trim().to_owned();
                let nid = make_id(&[self.stem, &type_name]);
                self.add_node(&nid, &type_name, at, NodeClass::Type);
                if let Some(tr) = node.child_by_field_name("trait") {
                    let mut trefs = Vec::new();
                    self.type_refs(Some(tr), false, &mut trefs);
                    for (idx, tr) in trefs.iter().enumerate() {
                        if idx == 0 {
                            self.ref_edge(&nid, tr, "implements", "", at);
                        } else {
                            self.ref_edge(&nid, tr, "references", "generic_arg", at);
                        }
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    let outer = self.impl_type.replace(strip_generics(&type_name));
                    self.walk_children(body, Some(&nid));
                    self.impl_type = outer;
                }
            }
            "static_item" | "const_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, self.src).to_owned();
                    if name == "_" {
                        return;
                    }
                    let at = line(node);
                    let nid = if let Some(parent) = impl_nid {
                        let nid = make_id(&[parent, &name]);
                        self.add_node(&nid, &format!(".{name}"), at, NodeClass::Const);
                        self.add_edge(parent, &nid, "contains", at, None);
                        nid
                    } else {
                        let nid = make_id(&[self.stem, &name]);
                        self.add_node(&nid, &name, at, NodeClass::Const);
                        let file = self.file_nid.clone();
                        self.add_edge(&file, &nid, "contains", at, None);
                        nid
                    };
                    self.emit_type_refs(&nid, node.child_by_field_name("type"), at);
                }
            }
            "use_declaration" => {}
            "mod_item" => {
                if let Some(body) = node.child_by_field_name("body") {
                    self.inline = self.inline.saturating_add(1);
                    self.walk_children(body, None);
                    self.inline = self.inline.saturating_sub(1);
                }
            }
            _ => self.walk_children(node, impl_nid),
        }
    }

    fn walk_children(&mut self, node: Node<'a>, impl_nid: Option<&str>) {
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
        for child in children {
            self.walk(child, impl_nid);
        }
    }

    fn prepare_resolution(&mut self) {
        for n in &self.out.nodes {
            let normalised = n
                .label
                .trim_matches(|c| c == '(' || c == ')')
                .trim_start_matches('.')
                .to_owned();
            self.labels.insert(normalised, n.id.clone());
            if !n.label.ends_with(')') {
                self.type_nids.insert(n.id.clone());
            }
        }
    }

    fn call_edge(&mut self, from: &str, target: &str, relation: &str, context: &str, at: usize) {
        if target == from || !self.pairs.insert((from.to_owned(), target.to_owned())) {
            return;
        }
        self.add_edge(from, target, relation, at, Some(context));
    }

    fn expand(&self, path: &str) -> String {
        let path = strip_generics(path);
        let (first, rest) = path
            .split_once("::")
            .map_or((path.as_str(), ""), |(f, r)| (f, r));
        let first = if first == "Self" {
            self.impl_type.as_deref().unwrap_or(first)
        } else {
            first
        };
        let head = self.aliases.get(first).map_or(first, String::as_str);
        join_path(head, rest)
    }

    /// Whether a type path names a type defined in this file (or the current impl type).
    fn is_local_type(&self, path: &str) -> bool {
        let last = last_segment(path);
        !path.contains("::")
            && (self.local_types.contains(last) || self.impl_type.as_deref() == Some(last))
    }

    /// Type path of a type node (`&mut Foo<T>` gives `Foo`, `Arc<Foo>` too), `None` for std,
    /// primitive or generic-parameter types.
    fn type_path(&self, node: Node<'_>) -> Option<String> {
        let raw = match node.kind() {
            "reference_type" | "pointer_type" => {
                return self.type_path(node.child_by_field_name("type")?);
            }
            "generic_type" => {
                let name = text(node.child_by_field_name("type")?, self.src);
                if is_pointer(last_segment(&strip_generics(name))) {
                    let mut cursor = node.walk();
                    let args = node
                        .children(&mut cursor)
                        .find(|c| c.kind() == "type_arguments")?;
                    let mut inner = args.walk();
                    let first = args.children(&mut inner).find(|c| c.is_named())?;
                    return self.type_path(first);
                }
                name
            }
            "type_identifier" | "scoped_type_identifier" => text(node, self.src),
            _ => return None,
        };
        let path = strip_generics(raw);
        let path = if path == "Self" {
            self.impl_type.clone()?
        } else {
            path
        };
        let last = last_segment(&path);
        if path.is_empty()
            || is_std_path(&path)
            || self.generics.contains(&path)
            || (BUILTIN_TYPES.contains(&last) && !self.local_types.contains(last))
        {
            return None;
        }
        Some(path)
    }

    /// Type of an expression when it is obvious: a typed binding, `self`, `T { .. }`,
    /// `T::new(..)`, a field of a struct of this file, a call to a function of this file with a
    /// declared return type, and what `?`, `.await`, `.unwrap()` and `.clone()` make of those.
    fn typed(&self, node: Node<'_>) -> Option<Typed> {
        let plain = |ty: String| Typed { ty, wrapped: false };
        match node.kind() {
            "await_expression" | "parenthesized_expression" | "reference_expression" => {
                let mut cursor = node.walk();
                let inner = node.named_children(&mut cursor).last()?;
                self.typed(inner)
            }
            "try_expression" => {
                let mut cursor = node.walk();
                let inner = node.named_children(&mut cursor).last()?;
                self.typed(inner).map(|t| plain(t.ty))
            }
            "struct_expression" => self.type_path(node.child_by_field_name("name")?).map(plain),
            "identifier" | "self" => self.locals.get(text(node, self.src)).cloned().map(plain),
            "field_expression" => {
                let base = self.typed(node.child_by_field_name("value")?)?;
                let field = text(node.child_by_field_name("field")?, self.src);
                if base.wrapped || !self.is_local_type(&base.ty) {
                    return None;
                }
                self.fields
                    .get(&(last_segment(&base.ty).to_owned(), field.to_owned()))
                    .cloned()
                    .map(plain)
            }
            "call_expression" => {
                let func = node.child_by_field_name("function")?;
                match func.kind() {
                    "identifier" => {
                        let ret = self.rets.get(&(None, text(func, self.src).to_owned()))?;
                        Some(Typed {
                            ty: ret.ty.clone(),
                            wrapped: ret.wrapped,
                        })
                    }
                    "scoped_identifier" => {
                        let name = text(func.child_by_field_name("name")?, self.src);
                        let path =
                            strip_generics(text(func.child_by_field_name("path")?, self.src));
                        let path = if path == "Self" {
                            self.impl_type.clone()?
                        } else {
                            path
                        };
                        if let Some(ret) = self.rets.get(&(Some(path.clone()), name.to_owned()))
                            && self.is_local_type(&path)
                        {
                            return Some(Typed {
                                ty: ret.ty.clone(),
                                wrapped: ret.wrapped,
                            });
                        }
                        let last = last_segment(&path);
                        (is_constructor(name)
                            && last.starts_with(char::is_uppercase)
                            && !is_std_path(&path))
                        .then(|| Typed {
                            ty: path,
                            wrapped: !is_plain_constructor(name),
                        })
                    }
                    "field_expression" => {
                        let field = text(func.child_by_field_name("field")?, self.src);
                        let receiver = self.typed(func.child_by_field_name("value")?)?;
                        if matches!(field, "unwrap" | "expect" | "unwrap_or_default" | "clone") {
                            return Some(plain(receiver.ty));
                        }
                        if receiver.wrapped || !self.is_local_type(&receiver.ty) {
                            return None;
                        }
                        let ret = self
                            .rets
                            .get(&(Some(last_segment(&receiver.ty).to_owned()), field.to_owned()))?;
                        Some(Typed {
                            ty: ret.ty.clone(),
                            wrapped: ret.wrapped,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn bind_pattern(&mut self, pattern: Node<'_>, ty: String) {
        match pattern.kind() {
            "identifier" => {
                self.locals.insert(text(pattern, self.src).to_owned(), ty);
            }
            "mut_pattern" | "ref_pattern" => {
                let mut cursor = pattern.walk();
                let inner = pattern
                    .named_children(&mut cursor)
                    .find(|n| n.kind() == "identifier");
                if let Some(inner) = inner {
                    self.locals.insert(text(inner, self.src).to_owned(), ty);
                }
            }
            _ => {}
        }
    }

    fn bind_parameters(&mut self, func: Node<'_>) {
        self.locals.clear();
        if let Some(t) = self.impl_type.clone() {
            self.locals.insert("self".into(), t);
        }
        let Some(params) = func.child_by_field_name("parameters") else {
            return;
        };
        let mut cursor = params.walk();
        let children: Vec<Node<'_>> = params.named_children(&mut cursor).collect();
        for p in children {
            if p.kind() == "parameter"
                && let (Some(pattern), Some(ty)) = (
                    p.child_by_field_name("pattern"),
                    p.child_by_field_name("type"),
                )
                && let Some(t) = self.type_path(ty)
            {
                self.bind_pattern(pattern, t);
            }
        }
    }

    /// Names a body binds: parameters, `let`, closure parameters, `for` and `match` patterns.
    fn collect_bound(&mut self, node: Node<'_>) {
        let patterns: Vec<Node<'_>> = match node.kind() {
            "parameter" | "let_declaration" | "for_expression" | "let_condition" | "match_arm" => {
                node.child_by_field_name("pattern").into_iter().collect()
            }
            "closure_expression" => node.child_by_field_name("parameters").into_iter().collect(),
            _ => Vec::new(),
        };
        for pattern in patterns {
            self.collect_identifiers(pattern);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_bound(child);
        }
    }

    fn collect_identifiers(&mut self, node: Node<'_>) {
        if node.kind() == "identifier" {
            self.bound.insert(text(node, self.src).to_owned());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_identifiers(child);
        }
    }

    fn defer_call(
        &mut self,
        from: &str,
        name: &str,
        method: bool,
        path: Option<String>,
        at: usize,
    ) {
        self.out.calls.push(RawCall {
            from: from.to_owned(),
            name: name.to_owned(),
            method,
            file: self.rel.to_owned(),
            loc: format!("L{at}"),
            path,
            module: None,
            bound: !method && self.bound.contains(name),
            inline: self.inline,
        });
    }

    /// A method of a type defined in this file, when that method is in this file too.
    fn local_method(&self, type_name: &str, name: &str) -> Option<String> {
        let id = make_id(&[&make_id(&[self.stem, type_name]), name]);
        self.seen.contains(&id).then_some(id)
    }

    fn label_edge(&mut self, from: &str, name: &str, at: usize) -> bool {
        let Some(target) = self.labels.get(name).cloned() else {
            return false;
        };
        let constructor = self.type_nids.contains(&target);
        let (relation, context) = if constructor {
            ("references", "constructor")
        } else {
            ("calls", "call")
        };
        self.call_edge(from, &target, relation, context, at);
        true
    }

    /// A tuple struct or enum variant used as a constructor: a reference to its type.
    fn constructor_ref(&mut self, from: &str, name: &str, path: Option<String>, at: usize) {
        let tr = TypeRef {
            name: name.to_owned(),
            generic: false,
            path,
        };
        if self.keep(&tr.name, tr.path.as_deref()) {
            let key = (from.to_owned(), make_id(&[name]));
            if self.pairs.insert(key) {
                self.ref_edge(from, &tr, "references", "constructor", at);
            }
        }
    }

    /// Resolve the calls of one function body: a name defined in this file is an EXTRACTED
    /// `calls` edge (a `references` edge for a type used as a constructor). Everything else is
    /// kept for corpus resolution with what the file knows about it: the `use` path of a plain
    /// call, the expanded qualifier of `path::name()`, or the receiver type of `x.name()` when
    /// the receiver is `self`, a typed parameter, a field of a struct of this file or an
    /// obviously typed `let` binding.
    fn collect_calls(&mut self, from: &str, node: Node<'a>) {
        if node.kind() == "function_item" {
            return;
        }
        if node.kind() == "let_declaration"
            && let Some(pattern) = node.child_by_field_name("pattern")
        {
            let ty = node
                .child_by_field_name("type")
                .and_then(|t| self.type_path(t))
                .or_else(|| {
                    node.child_by_field_name("value")
                        .and_then(|v| self.typed(v))
                        .filter(|t| !t.wrapped)
                        .map(|t| t.ty)
                });
            if let Some(ty) = ty {
                self.bind_pattern(pattern, ty);
            }
        }
        if node.kind() == "macro_invocation"
            && let Some(m) = node.child_by_field_name("macro")
            && m.kind() == "identifier"
        {
            let name = text(m, self.src).to_owned();
            if let Some(target) = self.macros.get(&name).cloned() {
                self.call_edge(from, &target, "calls", "call", line(node));
            }
        }
        if node.kind() == "call_expression"
            && let Some(func) = node.child_by_field_name("function")
        {
            let func = if func.kind() == "generic_function" {
                func.child_by_field_name("function").unwrap_or(func)
            } else {
                func
            };
            self.call_site(from, func, line(node));
        }
        let mut cursor = node.walk();
        let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
        for child in children {
            self.collect_calls(from, child);
        }
    }

    fn call_site(&mut self, from: &str, func: Node<'a>, at: usize) {
        match func.kind() {
            "identifier" => {
                let name = text(func, self.src).to_owned();
                if name.is_empty() {
                    return;
                }
                if name.starts_with(char::is_uppercase) {
                    // `Wrapper(x)`: a tuple struct (or imported variant) used as a constructor.
                    if !self.label_edge(from, &name, at) {
                        let path = self.aliases.get(&name).cloned();
                        self.constructor_ref(from, &name, path, at);
                    }
                    return;
                }
                if !self.label_edge(from, &name, at) {
                    let (callee, path) = match self.aliases.get(&name) {
                        Some(full) => match full.rsplit_once("::") {
                            Some((module, leaf)) => (leaf.to_owned(), Some(module.to_owned())),
                            None => (name.clone(), None),
                        },
                        None => (name.clone(), None),
                    };
                    if path.is_some() || !is_blocklisted(&callee.to_lowercase()) {
                        self.defer_call(from, &callee, false, path, at);
                    }
                }
            }
            "scoped_identifier" => {
                let name = func
                    .child_by_field_name("name")
                    .map_or("", |n| text(n, self.src))
                    .to_owned();
                let qualifier = strip_generics(
                    func.child_by_field_name("path")
                        .map_or("", |n| text(n, self.src)),
                );
                if name.is_empty() || qualifier.is_empty() {
                    return;
                }
                if name.starts_with(char::is_uppercase) {
                    // `Type::Variant(x)`: a constructor, a reference to the type.
                    let path = self.expand(&qualifier);
                    if self.is_local_type(&qualifier) || qualifier == "Self" {
                        let ty = if qualifier == "Self" {
                            self.impl_type.clone().unwrap_or_default()
                        } else {
                            qualifier
                        };
                        if let Some(variant) = self.local_method(&ty, &name) {
                            // A variant of an enum of this file: the call reaches its node.
                            self.call_edge(from, &variant, "calls", "call", at);
                        } else if !ty.is_empty() {
                            self.constructor_ref(from, &ty, None, at);
                        }
                    } else if !is_std_path(&path) {
                        self.constructor_ref(from, last_segment(&path), Some(path.clone()), at);
                    }
                    return;
                }
                if self.generics.contains(&qualifier) && !self.local_types.contains(&qualifier) {
                    // `E::custom(..)` on a generic parameter dispatches through its bounds.
                    return;
                }
                if self.is_local_type(&qualifier) || qualifier == "Self" {
                    let ty = if qualifier == "Self" {
                        self.impl_type.clone().unwrap_or_default()
                    } else {
                        qualifier
                    };
                    if let Some(target) = self.local_method(&ty, &name) {
                        self.call_edge(from, &target, "calls", "call", at);
                    } else if !ty.is_empty() {
                        self.defer_call(from, &name, true, Some(ty), at);
                    }
                    return;
                }
                let path = self.expand(&qualifier);
                if !is_std_path(&path) {
                    let method = last_segment(&path).starts_with(char::is_uppercase);
                    self.defer_call(from, &name, method, Some(path), at);
                }
            }
            "field_expression" => {
                let name = func
                    .child_by_field_name("field")
                    .map_or("", |n| text(n, self.src))
                    .to_owned();
                if name.is_empty() {
                    return;
                }
                let receiver = func
                    .child_by_field_name("value")
                    .and_then(|recv| self.typed(recv))
                    .filter(|t| !t.wrapped)
                    .map(|t| t.ty);
                match receiver {
                    Some(ty) if self.is_local_type(&ty) => {
                        if let Some(target) = self.local_method(&ty, &name) {
                            self.call_edge(from, &target, "calls", "call", at);
                        } else {
                            self.defer_call(from, &name, true, Some(ty), at);
                        }
                    }
                    Some(ty) => {
                        let path = self.expand(&ty);
                        self.defer_call(from, &name, true, Some(path), at);
                    }
                    None => {
                        if !is_blocklisted(&name) {
                            self.label_edge(from, &name, at);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract one Rust file.
#[must_use]
pub fn extract(rel_path: &str, source: &[u8]) -> FileExtract {
    if let Err(error) = decode_text(source) {
        return FileExtract {
            error: Some(error),
            ..FileExtract::default()
        };
    }
    let mut parser = Parser::new();
    if let Err(error) = parser.set_language(&tree_sitter_rust::LANGUAGE.into()) {
        return FileExtract {
            error: Some(error.to_string()),
            ..FileExtract::default()
        };
    }
    let Some(tree) = parser.parse(source, None) else {
        return FileExtract {
            error: Some("parse failed".into()),
            ..FileExtract::default()
        };
    };
    let stem = file_stem(rel_path);
    let file_nid = make_id(&[rel_path]);
    let mut ctx = Ctx {
        rel: rel_path,
        stem,
        src: source,
        file_nid: file_nid.clone(),
        out: FileExtract::default(),
        seen: HashSet::new(),
        local_types: HashSet::new(),
        bodies: Vec::new(),
        macros: HashMap::new(),
        labels: HashMap::new(),
        type_nids: HashSet::new(),
        pairs: HashSet::new(),
        aliases: HashMap::new(),
        locals: HashMap::new(),
        bound: HashSet::new(),
        impl_type: None,
        generics: HashSet::new(),
        fields: HashMap::new(),
        rets: HashMap::new(),
        inline: 0,
    };
    let file_name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    ctx.add_node(&file_nid, file_name, 1, NodeClass::File);
    let root = tree.root_node();
    ctx.scan_local_types(root);
    ctx.scan_signatures(root);
    ctx.walk(root, None);
    ctx.prepare_resolution();
    let bodies = std::mem::take(&mut ctx.bodies);
    for body in bodies {
        ctx.impl_type = body.impl_type;
        ctx.inline = body.inline;
        ctx.bind_parameters(body.func);
        ctx.bound.clear();
        ctx.collect_bound(body.func);
        ctx.collect_calls(&body.nid, body.body);
    }
    ctx.out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
use std::collections::HashMap;
use crate::sql::sqlite::{self, OpenPolicy};

pub struct RagStore { conn: Connection }
pub trait Embedder: Send { fn embed(&self, t: &str) -> Vec<f32>; }
macro_rules! shout { () => {}; }

impl RagStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = sqlite::open(path)?;
        init(&conn);
        Ok(Self { conn })
    }
    pub fn search(&self, q: &str) -> Vec<Hit> { self.open_reader(); helper(q) }
}
impl Embedder for RagStore { fn embed(&self, t: &str) -> Vec<f32> { vec![] } }
fn init(c: &Connection) {}
fn helper(q: &str) -> Hit { Hit }
"#;

    fn labels(f: &FileExtract) -> Vec<&str> {
        f.nodes.iter().map(|n| n.label.as_str()).collect()
    }

    #[test]
    fn symbols_get_graphify_style_ids_and_labels() {
        let f = extract("crates/ai/rag/src/store.rs", SRC.as_bytes());
        assert!(f.error.is_none());
        let l = labels(&f);
        for expected in [
            "store.rs",
            "RagStore",
            "Embedder",
            "shout!",
            ".open()",
            ".search()",
            "init()",
            "helper()",
            ".embed()",
        ] {
            assert!(l.contains(&expected), "missing {expected} in {l:?}");
        }
        assert!(
            f.nodes
                .iter()
                .any(|n| n.id == "crates_ai_rag_src_store_ragstore")
        );
        assert!(f.nodes.iter().any(|n| n.id == "crates_ai_rag_src_store_rs"));
    }

    #[test]
    fn structure_edges_are_contains_method_implements_inherits_imports() {
        let f = extract("crates/ai/rag/src/store.rs", SRC.as_bytes());
        let has = |s: &str, d: &str, r: &str| {
            f.edges
                .iter()
                .any(|e| e.src == s && e.dst == d && e.relation == r)
        };
        assert!(has(
            "crates_ai_rag_src_store_rs",
            "crates_ai_rag_src_store_ragstore",
            "contains"
        ));
        assert!(has(
            "crates_ai_rag_src_store_rs",
            "crates_ai_rag_src_store_init",
            "contains"
        ));
        assert!(has(
            "crates_ai_rag_src_store_ragstore",
            "crates_ai_rag_src_store_ragstore_open",
            "method"
        ));
        assert!(
            f.edges
                .iter()
                .any(|e| e.relation == "implements" && e.dst == "crates_ai_rag_src_store_embedder")
        );
        let used: Vec<(&str, &str)> = f
            .uses
            .iter()
            .map(|u| (u.local.as_str(), u.path.as_str()))
            .collect();
        assert_eq!(
            used,
            [
                ("HashMap", "std::collections::HashMap"),
                ("sqlite", "crate::sql::sqlite"),
                ("OpenPolicy", "crate::sql::sqlite::OpenPolicy"),
            ]
        );
        // `Embedder: Send` — Send is a builtin and does not become an edge.
        assert!(!f.edges.iter().any(|e| e.relation == "inherits"));
    }

    #[test]
    fn grouped_imports_record_each_leaf_alias_glob_and_reexport() {
        let src = b"use std::path::{Path, PathBuf};\nuse crate::codegraph::{model::Model, store::Store as KvStore};\nuse std::fmt::*;\npub use self::inner::Thing;\nmod inner { use super::*; }\n";
        let f = extract("imports.rs", src);
        let uses: Vec<(&str, &str, bool, bool, u8)> = f
            .uses
            .iter()
            .map(|u| (u.local.as_str(), u.path.as_str(), u.glob, u.public, u.inline))
            .collect();
        assert_eq!(
            uses,
            [
                ("Path", "std::path::Path", false, false, 0),
                ("PathBuf", "std::path::PathBuf", false, false, 0),
                ("Model", "crate::codegraph::model::Model", false, false, 0),
                ("KvStore", "crate::codegraph::store::Store", false, false, 0),
                ("", "std::fmt", true, false, 0),
                ("Thing", "inner::Thing", false, true, 0),
                ("", "super", true, false, 1),
            ]
        );
        assert_eq!(f.mods, ["inner"]);
        // Imports are resolved at assembly: the file itself emits no `imports_from` edge.
        assert!(!f.edges.iter().any(|e| e.relation == "imports_from"));
    }

    #[test]
    fn signature_types_become_references_but_builtins_do_not() {
        let f = extract("a.rs", SRC.as_bytes());
        let refs: Vec<&RawEdge> = f
            .edges
            .iter()
            .filter(|e| e.relation == "references")
            .collect();
        assert!(
            refs.iter()
                .any(|e| e.dst == "a_connection" || e.dst == "connection"),
            "{refs:?}"
        );
        assert!(
            refs.iter()
                .any(|e| e.context.as_deref() == Some("return_type"))
        );
        assert!(
            !refs
                .iter()
                .any(|e| e.dst == "string" || e.dst == "vec" || e.dst == "option")
        );
    }

    #[test]
    fn calls_inside_the_file_are_edges_and_unknown_plain_calls_wait_for_the_corpus() {
        let f = extract(
            "a.rs",
            format!("{SRC}\nfn caller() {{ outside(); self_less::scoped(); x.member(); }}")
                .as_bytes(),
        );
        let has = |s: &str, d: &str, r: &str| {
            f.edges
                .iter()
                .any(|e| e.src == s && e.dst == d && e.relation == r)
        };
        assert!(has("a_ragstore_open", "a_init", "calls"));
        assert!(has("a_ragstore_search", "a_helper", "calls"));
        // A plain unknown call and a qualified call wait for the corpus with their path; a member
        // call on a receiver of unknown type does not.
        let pending: Vec<(&str, Option<&str>)> = f
            .calls
            .iter()
            .filter(|c| c.from == "a_caller")
            .map(|c| (c.name.as_str(), c.path.as_deref()))
            .collect();
        assert_eq!(pending, [("outside", None), ("scoped", Some("self_less"))]);
    }

    #[test]
    fn use_aliases_self_and_typed_receivers_give_call_paths() {
        let src = "use bun_sys::{self as sys, File};\nuse crate::codegraph::store::*;\nstruct Local;\nimpl Local {\n fn a(&self) { self.b(); Self::c(); sys::open(); File::new(); }\n fn b(&self) {}\n fn c() {}\n}\nfn f(file: &mut File<'_>) { let l = Local; l.a(); file.read(); std::mem::take(); Vec::new(); }\n";
        let f = extract("src/x.rs", src.as_bytes());
        let has = |s: &str, d: &str| {
            f.edges
                .iter()
                .any(|e| e.src == s && e.dst == d && e.relation == "calls")
        };
        assert!(has("src_x_local_a", "src_x_local_b"));
        assert!(has("src_x_local_a", "src_x_local_c"));
        assert!(has("src_x_f", "src_x_local_a"));
        let pending: Vec<(&str, bool, Option<&str>)> = f
            .calls
            .iter()
            .map(|c| (c.name.as_str(), c.method, c.path.as_deref()))
            .collect();
        assert_eq!(
            pending,
            [
                ("open", false, Some("bun_sys")),
                ("new", true, Some("bun_sys::File")),
                ("read", true, Some("bun_sys::File"))
            ]
        );
        assert!(
            f.uses
                .iter()
                .any(|u| u.glob && u.path == "crate::codegraph::store")
        );
    }

    #[test]
    fn syntax_errors_do_not_panic_and_non_rust_is_ignored() {
        let broken = extract("b.rs", b"fn (((");
        assert!(broken.error.is_none()); // tree-sitter recovers; no panic
        assert!(
            super::super::extract_file("x.txt", b"hello")
                .nodes
                .is_empty()
        );
    }

    #[test]
    fn enum_variants_constants_and_field_types_become_nodes_and_edges() {
        let src = b"pub const LIMIT: usize = 3;\nstruct Holder { inner: Inner, list: Vec<Item> }\nenum Shape { Circle(Radius), Square { side: Side }, Empty }\n";
        let f = extract("s.rs", src);
        let labels: Vec<&str> = f.nodes.iter().map(|n| n.label.as_str()).collect();
        for expected in ["LIMIT", "Holder", "Shape", "Circle", "Square", "Empty"] {
            assert!(
                labels.contains(&expected),
                "missing {expected} in {labels:?}"
            );
        }
        let case_of = f.edges.iter().filter(|e| e.relation == "case_of").count();
        assert_eq!(case_of, 3);
        let ctx_field: Vec<&str> = f
            .edges
            .iter()
            .filter(|e| e.context.as_deref() == Some("field"))
            .map(|e| e.dst.as_str())
            .collect();
        for expected in ["inner", "radius", "side"] {
            assert!(
                ctx_field.iter().any(|d| d.ends_with(expected)),
                "missing field ref {expected} in {ctx_field:?}"
            );
        }
        assert!(
            f.edges
                .iter()
                .any(|e| e.context.as_deref() == Some("generic_arg") && e.dst.ends_with("item"))
        );
    }

    #[test]
    fn associated_type_bindings_and_paths_are_not_type_references() {
        let src = b"fn f<T: Tr>(s: impl Stream<Item = Chunk>, i: Self::Item, o: <T as Tr>::Out, v: Vec<T::Output>) {}\n";
        let f = extract("src/a.rs", src);
        let mut stubs: Vec<&str> = f
            .nodes
            .iter()
            .filter(|n| n.file.is_empty())
            .map(|n| n.label.as_str())
            .collect();
        stubs.sort_unstable();
        assert_eq!(stubs, ["Chunk", "Stream", "Tr"], "{stubs:?}");
    }

    #[test]
    fn keywords_and_generic_parameters_are_never_nodes() {
        let src = b"use super::*;\nuse crate::a::B;\nstruct Pair<A, B2> { a: A, b: B2, c: Config }\nfn run<T, const N: usize>(t: T, c: Config, s: Wrapper<S>) -> Self { }\nimpl<X> Holder<X> { fn get(&self, x: X, k: Key) -> Self { } }\n";
        let f = extract("src/g.rs", src);
        let stubs: Vec<&str> = f
            .nodes
            .iter()
            .filter(|n| n.file.is_empty())
            .map(|n| n.label.as_str())
            .collect();
        for forbidden in ["super", "self", "crate", "Self", "A", "B2", "T", "X", "N"] {
            assert!(!stubs.contains(&forbidden), "{forbidden} in {stubs:?}");
        }
        let mut wanted: Vec<&str> = stubs.clone();
        wanted.sort_unstable();
        assert_eq!(wanted, ["Config", "Key", "S", "Wrapper"]);
    }

    #[test]
    fn type_references_carry_the_path_the_file_spells() {
        let src = b"use serde_json::Value;\nuse crate::config::Config as Cfg;\nfn a(v: Value, c: &Cfg, m: serde_json::Map<String, Value>, f: std::fmt::Formatter) {}\n";
        let f = extract("src/p.rs", src);
        let mut refs: Vec<(&str, Option<&str>)> = f
            .edges
            .iter()
            .filter(|e| e.relation == "references")
            .map(|e| (e.dst.as_str(), e.path.as_deref()))
            .collect();
        refs.sort_unstable();
        assert_eq!(
            refs,
            [
                ("cfg", Some("crate::config::Config")),
                ("map", Some("serde_json::Map")),
                ("value", Some("serde_json::Value")),
                ("value", Some("serde_json::Value")),
            ],
            "std types carry no edge"
        );
    }

    #[test]
    fn receivers_are_typed_through_fields_return_types_and_pointers() {
        let src = b"use std::sync::Arc;\nuse crate::db::Db;\nstruct Runner;\nimpl Runner { fn run(&self) {} fn make() -> Runner { Runner } }\nstruct Store { db: Db, runner: Arc<Runner> }\nfn helper() -> Runner { Runner }\nfn opt() -> Option<Runner> { None }\nimpl Store {\n fn go(&self) {\n  self.db.query();\n  self.runner.run();\n  let r = helper();\n  r.run();\n  let o = opt();\n  o.run();\n  let m = Runner::make();\n  m.run();\n }\n}\n";
        let f = extract("src/s.rs", src);
        let calls_to = |dst: &str| {
            f.edges
                .iter()
                .filter(|e| e.relation == "calls" && e.src == "src_s_store_go" && e.dst == dst)
                .count()
        };
        assert_eq!(calls_to("src_s_runner_run"), 1, "deduplicated per caller");
        let pending: Vec<(&str, Option<&str>)> = f
            .calls
            .iter()
            .map(|c| (c.name.as_str(), c.path.as_deref()))
            .collect();
        assert_eq!(
            pending,
            [("query", Some("crate::db::Db"))],
            "`o` is an Option: its `run` is not Runner's"
        );
    }

    #[test]
    fn calls_through_local_bindings_are_marked_and_constructors_become_references() {
        let src = b"struct W(u8);\nenum Mode { Custom(u8) }\nfn f(cb: fn()) {\n cb();\n let g = |x| x;\n g(1);\n real();\n W(1);\n Mode::Custom(2);\n}\n";
        let f = extract("src/b.rs", src);
        let calls: Vec<(&str, bool)> = f.calls.iter().map(|c| (c.name.as_str(), c.bound)).collect();
        assert_eq!(calls, [("cb", true), ("g", true), ("real", false)]);
        let ctor: Vec<&str> = f
            .edges
            .iter()
            .filter(|e| e.context.as_deref() == Some("constructor"))
            .map(|e| e.dst.as_str())
            .collect();
        assert_eq!(ctor, ["src_b_w"]);
        assert!(
            f.edges
                .iter()
                .any(|e| e.relation == "calls" && e.dst == "src_b_mode_custom"),
            "`Mode::Custom(2)` reaches the variant"
        );
    }

    #[test]
    fn an_alias_names_the_real_function_and_inline_modules_are_counted() {
        let src = b"use a::b::original as renamed;\nfn f() { renamed(); }\nmod tests { use super::*; fn t() { super::f(); } }\n";
        let f = extract("src/c.rs", src);
        let first = &f.calls[0];
        assert_eq!(
            (first.name.as_str(), first.path.as_deref(), first.inline),
            ("original", Some("a::b"), 0)
        );
        let second = &f.calls[1];
        assert_eq!(
            (second.name.as_str(), second.path.as_deref(), second.inline),
            ("f", Some("super"), 1)
        );
    }
}
