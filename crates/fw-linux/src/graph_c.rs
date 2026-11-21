//! C extraction: a same-length rewrite of the kernel's annotation and definition macros so tree-sitter-c parses them, the symbol, include,
//! call and reference walk over the tree, a lexical scan of macro bodies, and a line scan of the markers (`EXPORT_SYMBOL`, initcalls) and
//! assembly entry points that have no C syntax.

use std::collections::HashSet;

use tree_sitter::{Node, Parser, Tree, TreeCursor};

use crate::error::{Error, Result};
use crate::graph::{ExportRow, Extracted, IncludeRow, NO_SOURCE, RegistrationRow, SymbolKind, SymbolRow, UseRow};

#[derive(Default)]
pub struct Parsers {
    c: Option<Parser>,
    kconfig: Option<Parser>,
    make: Option<Parser>,
}

fn parser_in<'a>(slot: &'a mut Option<Parser>, language: &tree_sitter::Language, name: &str) -> Result<&'a mut Parser> {
    if slot.is_none() {
        let mut parser = Parser::new();
        parser.set_language(language).map_err(|error| Error::message(format!("tree-sitter {name}: {error}")))?;
        *slot = Some(parser);
    }
    slot.as_mut().ok_or_else(|| Error::message(format!("tree-sitter {name}: parser unavailable")))
}

impl Parsers {
    pub fn c(&mut self) -> Result<&mut Parser> {
        parser_in(&mut self.c, &tree_sitter_c::LANGUAGE.into(), "c")
    }

    pub fn kconfig(&mut self) -> Result<&mut Parser> {
        parser_in(&mut self.kconfig, &tree_sitter_kconfig::LANGUAGE.into(), "kconfig")
    }

    pub fn make(&mut self) -> Result<&mut Parser> {
        parser_in(&mut self.make, &tree_sitter_make::LANGUAGE.into(), "make")
    }
}

pub fn parse(parser: &mut Parser, source: &[u8]) -> Result<Tree> {
    parser.parse(source, None).ok_or_else(|| Error::message("tree-sitter: parse returned no tree"))
}

/// The number of error regions: outermost ERROR nodes and MISSING nodes.
pub fn count_errors(root: Node<'_>) -> u32 {
    let mut count = 0;
    walk_tree(root, |cursor, _| {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            count += 1;
            return false;
        }
        node.has_error()
    });
    count
}

/// Iterative pre-order walk (kernel files nest deeply); `visit` returns whether to enter the children of the node under the cursor.
pub fn walk_tree<'tree>(root: Node<'tree>, mut visit: impl FnMut(&TreeCursor<'tree>, &str) -> bool) {
    let mut cursor = root.walk();
    let mut parents: Vec<&str> = Vec::new();
    loop {
        let parent = parents.last().copied().unwrap_or("");
        let kind = cursor.node().kind();
        if visit(&cursor, parent) && cursor.goto_first_child() {
            parents.push(kind);
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
            parents.pop();
        }
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Annotations that tree-sitter-c cannot place; `true` when the name takes a parenthesized argument list.
fn annotation(word: &[u8]) -> Option<bool> {
    let word = std::str::from_utf8(word).ok()?;
    match word {
        "__init" | "__exit" | "__initdata" | "__exitdata" | "__initconst" | "__exitconst" | "__meminit" | "__meminitdata" | "__meminitconst"
        | "__devinit" | "__devinitdata" | "__ref" | "__refdata" | "__refconst" | "__init_or_module" | "__initdata_or_module" | "__ro_after_init"
        | "__cold" | "__hot" | "__always_inline" | "noinline" | "__noinline" | "noinline_for_stack" | "notrace" | "__notrace" | "__noreturn"
        | "noreturn" | "__must_check" | "__user" | "__kernel" | "__iomem" | "__percpu" | "__rcu" | "__force" | "__bitwise" | "__nocast"
        | "__safe" | "__pure" | "__packed" | "__weak" | "__used" | "__maybe_unused" | "__always_unused" | "__unused" | "__visible"
        | "asmlinkage" | "__sched" | "__kprobes" | "noinstr" | "__latent_entropy" | "__randomize_layout" | "__no_randomize_layout"
        | "__naked" | "__nocfi" | "__no_kcsan" | "__no_sanitize_address" | "__no_sanitize_undefined" | "__no_sanitize_coverage"
        | "__no_profile" | "__no_stack_protector" | "__no_caller_saved_registers" | "__attribute_const__" | "____cacheline_aligned"
        | "____cacheline_aligned_in_smp" | "____cacheline_internodealigned_in_smp" | "__cacheline_aligned" | "__cacheline_aligned_in_smp"
        | "__read_mostly" | "__page_aligned_data" | "__page_aligned_bss" | "__nodiscard" | "__deprecated" | "__flatten" | "__designated_init"
        | "__noclone" | "__malloc" | "__bpf_kfunc" | "__gnu_inline" | "__no_kasan_or_inline" | "__no_kmsan_checks" | "__no_sanitize_thread"
        | "__init_memblock" | "__initdata_memblock" | "__sysfs_function_ref" | "__noendbr" | "__no_ubsan_or_inline" | "__no_gcov"
        | "__nostackprotector" | "__nonstring" | "__counted_by_ptr" | "__rcu_protected" | "__sparse_cache" | "__kfunc" | "__nonnull" => {
            Some(false)
        }
        "__aligned" | "__section" | "__printf" | "__scanf" | "__alloc_size" | "__alloc_align" | "__counted_by" | "__counted_by_le"
        | "__counted_by_be" | "__assume_aligned" | "__cleanup" | "__free" | "__alias" | "__acquires" | "__releases" | "__must_hold"
        | "__attr_access" | "__acquire" | "__release" | "__no_sanitize" | "__guarded_by" | "__aligned_largest" | "__optimize" | "__nonnull_params"
        | "__weak_alias" | "__noinstr_section" | "__cpuidle" | "__attribute_alias" | "__diag_ignore_all" | "__sanitize_address" => Some(true),
        _ => None,
    }
}

fn blank(buffer: &mut [u8], from: usize, to: usize) {
    for byte in &mut buffer[from..to] {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

fn skip_spaces(buffer: &[u8], mut index: usize) -> usize {
    while index < buffer.len() && (buffer[index] == b' ' || buffer[index] == b'\t') {
        index += 1;
    }
    index
}

/// The macro arguments of the `(` at `open`: `(start, end)` byte ranges (trimmed) and the index of the closing `)`.
struct MacroArguments {
    ranges: Vec<(usize, usize)>,
    commas: Vec<usize>,
    close: usize,
}

fn macro_arguments(buffer: &[u8], open: usize) -> Option<MacroArguments> {
    let mut depth = 0;
    let mut ranges = Vec::new();
    let mut commas = Vec::new();
    let mut start = open + 1;
    let limit = (open + 4000).min(buffer.len());
    let trim = |from: usize, to: usize| {
        let mut from = from;
        let mut to = to;
        while from < to && buffer[from].is_ascii_whitespace() {
            from += 1;
        }
        while to > from && buffer[to - 1].is_ascii_whitespace() {
            to -= 1;
        }
        (from, to)
    };
    let mut index = open;
    while index < limit {
        match buffer[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    ranges.push(trim(start, index));
                    return Some(MacroArguments { ranges, commas, close: index });
                }
            }
            b',' if depth == 1 => {
                ranges.push(trim(start, index));
                commas.push(index);
                start = index + 1;
            }
            b'"' => {
                index += 1;
                while index < limit && buffer[index] != b'"' && buffer[index] != b'\n' {
                    index += usize::from(buffer[index] == b'\\') + 1;
                }
            }
            b';' => return None,
            _ => {}
        }
        index += 1;
    }
    None
}

fn is_identifier(text: &[u8]) -> bool {
    !text.is_empty() && !text[0].is_ascii_digit() && text.iter().all(|&byte| is_identifier_byte(byte))
}

fn has_newline(buffer: &[u8], from: usize, to: usize) -> bool {
    buffer[from..to].contains(&b'\n')
}

/// `SYSCALL_DEFINEn(name, type, arg, ...)` becomes `long sys_name(type arg, ...)` in place.
fn rewrite_syscall(buffer: &mut [u8], start: usize, open: usize, compat: bool, count: usize) -> Option<usize> {
    let arguments = macro_arguments(buffer, open)?;
    let name = arguments.ranges.first().map(|&(from, to)| buffer[from..to].to_vec())?;
    if !is_identifier(&name) || (count == 0 && arguments.ranges.len() != 1) || (count > 0 && arguments.ranges.len() < 2) {
        return None;
    }
    let prefix = if compat { "long compat_sys_" } else { "long sys_" };
    let head: Vec<u8> = prefix.bytes().chain(name.iter().copied()).chain(std::iter::once(b'(')).collect();
    if count == 0 {
        let end = arguments.close + 1;
        let text: Vec<u8> = head.iter().copied().chain(b"void)".iter().copied()).collect();
        if text.len() > end - start || has_newline(buffer, start, end) {
            return None;
        }
        blank(buffer, start, end);
        buffer[start..start + text.len()].copy_from_slice(&text);
        return Some(end);
    }
    let first = arguments.ranges[1].0;
    if head.len() > first - start || has_newline(buffer, start, start + head.len()) {
        return None;
    }
    blank(buffer, start, first);
    buffer[start..start + head.len()].copy_from_slice(&head);
    // A trailing unpaired argument (a macro that stands for a type and a name, like `compat_arg_u64_dual(pos)`) is left as written.
    let pairs = (arguments.ranges.len() - 1) / 2;
    for (index, &comma) in arguments.commas.iter().enumerate() {
        if index % 2 == 1 && index < 2 * pairs {
            buffer[comma] = b' ';
        }
    }
    Some(first)
}

/// `DEFINE_X(name, ...)` becomes `int name=(...)`; `DEFINE_PER_CPU*(type, name, ...)` becomes `type name`.
fn rewrite_define(buffer: &mut [u8], start: usize, open: usize, word: &str) -> Option<usize> {
    const DERIVED: [&str; 14] =
        ["ATTRIBUTE", "GUARD", "CLASS", "FREE", "TRACE", "EVENT", "CONDITION", "SHOW", "STORE", "STATIC_CALL", "FOPS", "SEQ", "KUNIT", "TEST"];
    if DERIVED.iter().any(|derived| word.contains(derived)) {
        return None;
    }
    let arguments = macro_arguments(buffer, open)?;
    let close = arguments.close;
    if word.contains("PER_CPU") || word.contains("PERCPU") {
        if arguments.ranges.len() < 2 {
            return None;
        }
        let (type_start, _) = arguments.ranges[0];
        let (name_start, name_end) = arguments.ranges[1];
        if !is_identifier(&buffer[name_start..name_end]) || has_newline(buffer, start, type_start) {
            return None;
        }
        blank(buffer, start, type_start);
        buffer[arguments.commas[0]] = b' ';
        blank(buffer, name_end, close + 1);
        return Some(close + 1);
    }
    let (name_start, name_end) = *arguments.ranges.first()?;
    if !is_identifier(&buffer[name_start..name_end]) || name_start - start < 4 || has_newline(buffer, start, name_start) {
        return None;
    }
    if arguments.ranges.len() > 1 {
        let comma = arguments.commas[0];
        if !buffer.get(comma + 1).is_some_and(|byte| byte.is_ascii_whitespace() && *byte != b'\n') {
            return None;
        }
        buffer[comma] = b'=';
        buffer[comma + 1] = b'(';
    } else {
        buffer[close] = b' ';
    }
    blank(buffer, start, name_start);
    buffer[start..start + 3].copy_from_slice(b"int");
    Some(close + 1)
}

struct Rewritten {
    text: Vec<u8>,
    /// Loop macros (`for_each_*`) turned into `if`, as (line, name): their calls are no longer in the tree.
    loops: Vec<(u32, String)>,
}

/// The end of a string or character literal that starts at `index`; literals end at the line end.
fn skip_literal(buffer: &[u8], index: usize) -> usize {
    let quote = buffer[index];
    let mut cursor = index + 1;
    while cursor < buffer.len() && buffer[cursor] != b'\n' {
        if buffer[cursor] == quote {
            return cursor + 1;
        }
        if buffer[cursor] == b'\\' && buffer.get(cursor + 1).is_some_and(|&next| next != b'\n') {
            cursor += 1;
        }
        cursor += 1;
    }
    cursor
}

fn is_loop_macro(word: &str) -> bool {
    ["for_each", "FOR_EACH", "foreach", "FOREACH", "do_each", "DO_EACH"].iter().any(|part| word.contains(part)) || word.starts_with("scoped_")
}

/// `EXPORT_SYMBOL(x)` and initcall registrations are read by `scan_markers`; the parse does not need them (they often lack a `;`).
fn is_marker(word: &str) -> bool {
    (word.starts_with("EXPORT_") && word.contains("SYMBOL"))
        || word == "module_init"
        || word == "module_exit"
        || word.ends_with("_initcall")
        || word.ends_with("_initcall_sync")
        || ((word.starts_with("module_") || word.starts_with("builtin_")) && word.ends_with("_driver"))
}

fn previous_word(buffer: &[u8], index: usize) -> &[u8] {
    let mut end = index;
    while end > 0 && buffer[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_identifier_byte(buffer[start - 1]) {
        start -= 1;
    }
    &buffer[start..end]
}

/// `container_of(ptr, struct foo, member)`: the type name is a macro argument, which tree-sitter-c cannot parse as an expression.
fn is_type_argument(buffer: &[u8], end: usize) -> bool {
    let name_start = skip_spaces(buffer, end);
    let mut name_end = name_start;
    while name_end < buffer.len() && is_identifier_byte(buffer[name_end]) {
        name_end += 1;
    }
    name_end > name_start && matches!(buffer.get(skip_spaces(buffer, name_end)), Some(b',' | b')'))
}

/// Statement macros start their line; anything else in front of the name means an expression.
fn at_statement_start(buffer: &[u8], index: usize, line_start: usize) -> bool {
    buffer[line_start..index].iter().all(u8::is_ascii_whitespace)
        || buffer[..index].iter().rev().find(|byte| !byte.is_ascii_whitespace()).is_none_or(|byte| matches!(byte, b';' | b'{' | b'}' | b')' | b':'))
}

/// Same-length rewrite: byte offsets and line numbers of the parsed text equal those of the file.
fn rewrite(source: &[u8]) -> Rewritten {
    let mut buffer = source.to_vec();
    let length = buffer.len();
    let mut loops = Vec::new();
    let mut index = 0;
    let mut directive = false;
    let mut line_start = 0;
    let mut line: u32 = 1;
    // One entry per open brace: whether it opens an initializer, where tree-sitter-c cannot parse #if lines.
    let mut braces: Vec<bool> = Vec::new();
    let mut at_line_start = true;
    let mut calls: Vec<bool> = Vec::new();
    while index < length {
        let byte = buffer[index];
        if byte == b'\n' {
            let previous = buffer[..index].iter().rev().find(|&&byte| byte != b'\r').copied();
            directive = directive && previous == Some(b'\\');
            at_line_start = true;
            line_start = index + 1;
            line += 1;
            index += 1;
            continue;
        }
        if at_line_start && !byte.is_ascii_whitespace() {
            at_line_start = false;
            directive = directive || byte == b'#';
            if byte == b'#' && braces.last() == Some(&true) {
                let name_start = skip_spaces(&buffer, index + 1);
                let rest = &buffer[name_start..];
                let conditional = [&b"if"[..], b"elif", b"else", b"endif"].iter().any(|word| rest.starts_with(word));
                if conditional {
                    let mut stop = index;
                    while stop < length && buffer[stop] != b'\n' {
                        stop += 1;
                    }
                    blank(&mut buffer, index, stop);
                    index = stop;
                    directive = false;
                    continue;
                }
            }
        }
        match byte {
            b'/' if buffer.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index < length && !(buffer[index] == b'*' && buffer.get(index + 1) == Some(&b'/')) {
                    if buffer[index] == b'\n' {
                        line_start = index + 1;
                        line += 1;
                    }
                    index += 1;
                }
                index = (index + 2).min(length);
                continue;
            }
            b'/' if buffer.get(index + 1) == Some(&b'/') => {
                while index < length && buffer[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            b'"' | b'\'' => {
                index = skip_literal(&buffer, index);
                continue;
            }
            b'(' if !directive => {
                let before = previous_word(&buffer, index);
                let keyword = matches!(before, b"if" | b"while" | b"for" | b"switch" | b"return" | b"else" | b"do");
                calls.push(!braces.is_empty() && !keyword && before.first().is_some_and(|&first| first.is_ascii_alphabetic() || first == b'_'));
            }
            b')' if !directive => {
                calls.pop();
            }
            b'{' if !directive => {
                let before = buffer[..index].iter().rev().find(|byte| !byte.is_ascii_whitespace()).copied();
                let initializer = before == Some(b'=') || (braces.last() == Some(&true) && matches!(before, Some(b',' | b'{')));
                braces.push(initializer);
            }
            // Kernel style closes every top-level block at column 0, which resynchronizes the depth after unbalanced #if branches.
            b'}' if !directive => {
                if index == line_start {
                    calls.clear();
                }
                if index == line_start {
                    braces.clear();
                } else {
                    braces.pop();
                }
            }
            _ => {}
        }
        if !(byte.is_ascii_alphabetic() || byte == b'_') || (index > 0 && is_identifier_byte(buffer[index - 1])) {
            index += 1;
            continue;
        }
        let mut end = index;
        while end < length && is_identifier_byte(buffer[end]) {
            end += 1;
        }
        if directive {
            index = end;
            continue;
        }
        let word = std::str::from_utf8(&buffer[index..end]).unwrap_or("").to_owned();
        let open = skip_spaces(&buffer, end);
        let has_open = buffer.get(open) == Some(&b'(');
        if braces.is_empty()
            && has_open
            && index == line_start
            && is_marker(&word)
            && let Some(arguments) = macro_arguments(&buffer, open)
        {
            blank(&mut buffer, index, arguments.close + 1);
            index = arguments.close + 1;
            continue;
        }
        if braces.is_empty() && has_open {
            let digit = word.bytes().last().filter(u8::is_ascii_digit);
            let syscall = word.strip_prefix("COMPAT_SYSCALL_DEFINE").map(|rest| (true, rest)).or_else(|| word.strip_prefix("SYSCALL_DEFINE").map(|rest| (false, rest)));
            if let (Some((compat, rest)), Some(digit)) = (syscall, digit)
                && rest.len() == 1
            {
                index = rewrite_syscall(&mut buffer, index, open, compat, usize::from(digit - b'0')).unwrap_or(end);
                continue;
            }
            if word.starts_with("DEFINE_") {
                let prefix = String::from_utf8_lossy(&buffer[line_start..index]);
                if matches!(prefix.trim(), "" | "static" | "static const" | "extern" | "const") {
                    index = rewrite_define(&mut buffer, index, open, &word).unwrap_or(end);
                    continue;
                }
            }
        }
        if matches!(word.as_str(), "struct" | "union" | "enum") && calls.last() == Some(&true) && is_type_argument(&buffer, end) {
            blank(&mut buffer, index, end);
            index = end;
            continue;
        }
        if !braces.is_empty() && has_open && is_loop_macro(&word) && at_statement_start(&buffer, index, line_start) {
            loops.push((line, word));
            blank(&mut buffer, index, end);
            buffer[index..index + 2].copy_from_slice(b"if");
            index = end;
            continue;
        }
        if let Some(takes_arguments) = annotation(word.as_bytes()) {
            let mut stop = end;
            if takes_arguments
                && has_open
                && let Some(arguments) = macro_arguments(&buffer, open)
            {
                stop = arguments.close + 1;
            }
            blank(&mut buffer, index, stop);
            index = stop;
            continue;
        }
        index = end;
    }
    Rewritten { text: buffer, loops }
}

fn collapse(bytes: &[u8], limit: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len().min(limit));
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.len() >= limit {
            break;
        }
    }
    if out.len() > limit {
        let mut cut = limit;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

fn leading_identifier(line: &[u8]) -> &[u8] {
    let end = line.iter().position(|&byte| !is_identifier_byte(byte)).unwrap_or(line.len());
    &line[..end]
}

/// `EXPORT_SYMBOL*` and initcall-style registrations, which sit at column 0 and have no declaration syntax.
fn scan_markers(source: &[u8], out: &mut Extracted) {
    for (index, line) in source.split(|&byte| byte == b'\n').enumerate() {
        if !line.first().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            continue;
        }
        let word_bytes = leading_identifier(line);
        let Ok(word) = std::str::from_utf8(word_bytes) else { continue };
        let exports = word.starts_with("EXPORT_") && word.contains("SYMBOL");
        if !is_marker(word) {
            continue;
        }
        let open = skip_spaces(line, word_bytes.len());
        if line.get(open) != Some(&b'(') {
            continue;
        }
        let inner = &line[open + 1..];
        let inner = &inner[..inner.iter().position(|&byte| byte == b')').unwrap_or(inner.len())];
        let mut parts = inner.split(|&byte| byte == b',').map(|part| String::from_utf8_lossy(part).trim().to_owned());
        let Some(first) = parts.next().filter(|first| is_identifier(first.as_bytes())) else { continue };
        let name = out.name(&first);
        let line_number = index as u32 + 1;
        if exports {
            let namespace = parts.next().map(|part| part.trim_matches('"').to_owned()).unwrap_or_default();
            let license = if word.contains("GPL") { "GPL" } else { "" };
            out.exports.push(ExportRow { name, line: line_number, license, namespace });
        } else {
            out.registrations.push(RegistrationRow { name, line: line_number, macro_name: word.to_owned() });
        }
    }
}

const INLINE_WORDS: [&str; 5] = ["inline", "__inline", "__inline__", "__always_inline", "__forceinline"];

struct Header {
    storage: &'static str,
    inline: bool,
}

fn header_words(prefix: &[u8]) -> Header {
    let mut header = Header { storage: "", inline: false };
    for word in prefix.split(|&byte| !is_identifier_byte(byte)) {
        match word {
            b"static" => header.storage = "static",
            b"extern" if header.storage.is_empty() => header.storage = "extern",
            other => {
                if std::str::from_utf8(other).is_ok_and(|text| INLINE_WORDS.contains(&text)) {
                    header.inline = true;
                }
            }
        }
    }
    header
}

const KEYWORDS: [&str; 24] = [
    "if", "else", "for", "while", "do", "switch", "case", "default", "return", "sizeof", "typeof", "__typeof__", "defined", "__attribute__", "asm",
    "__asm__", "volatile", "__volatile__", "_Static_assert", "static_assert", "_Alignof", "__alignof__", "break", "continue",
];

fn declarator_name(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        match node.kind() {
            "identifier" | "type_identifier" | "field_identifier" => return Some(node),
            "function_declarator" | "pointer_declarator" | "array_declarator" | "attributed_declarator" | "init_declarator" => {
                node = node.child_by_field_name("declarator")?;
            }
            "parenthesized_declarator" => node = node.named_child(0)?,
            _ => return None,
        }
    }
}

/// A function prototype: a function declarator that directly names the function, as opposed to a function-pointer variable.
fn is_prototype(mut node: Node<'_>) -> bool {
    loop {
        match node.kind() {
            "pointer_declarator" | "attributed_declarator" => match node.child_by_field_name("declarator") {
                Some(inner) => node = inner,
                None => return false,
            },
            "function_declarator" => {
                return node.child_by_field_name("declarator").is_some_and(|inner| matches!(inner.kind(), "identifier" | "type_identifier"));
            }
            _ => return false,
        }
    }
}

struct Extractor<'a> {
    /// The rewritten text the tree was parsed from (names come from here).
    source: &'a [u8],
    /// The file as written (signatures and annotations come from here); same offsets as `source`.
    original: &'a [u8],
    out: &'a mut Extracted,
    seen: HashSet<(u32, u32, bool)>,
}

impl Extractor<'_> {
    fn text(&self, node: Node<'_>) -> String {
        String::from_utf8_lossy(&self.source[node.byte_range()]).into_owned()
    }

    fn lines(node: Node<'_>) -> (u32, u32) {
        let start = node.start_position();
        let end = node.end_position();
        let end_line = if end.column == 0 && end.row > start.row { end.row } else { end.row + 1 };
        (start.row as u32 + 1, end_line as u32)
    }

    fn add_symbol(&mut self, name: &str, kind: SymbolKind, node: Node<'_>, signature: String, header: Header) -> u32 {
        let (line, end_line) = Self::lines(node);
        let name = self.out.name(name);
        self.out.symbols.push(SymbolRow { name, kind, line, end_line, signature, storage: header.storage, inline: header.inline, exported: false });
        (self.out.symbols.len() - 1) as u32
    }

    fn record(&mut self, source: u32, name: &str, call: bool, line: u32) {
        let name = self.out.name(name);
        if self.seen.insert((source, name, call)) {
            self.out.uses.push(UseRow { source, name, call, line });
        }
    }

    fn visit_items(&mut self, node: Node<'_>, depth: u32) {
        if depth > 200 {
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "preproc_include" => self.include(child),
                "preproc_def" => self.macro_definition(child, false),
                "preproc_function_def" => self.macro_definition(child, true),
                "function_definition" => self.function(child),
                "declaration" => self.declaration(child),
                "type_definition" => self.type_definition(child),
                "struct_specifier" | "union_specifier" | "enum_specifier" => self.collect_types(child),
                "preproc_if" | "preproc_ifdef" | "preproc_else" | "preproc_elif" | "preproc_elifdef" | "linkage_specification"
                | "declaration_list" | "ERROR" => self.visit_items(child, depth + 1),
                _ => {}
            }
        }
    }

    fn include(&mut self, node: Node<'_>) {
        let Some(path) = node.child_by_field_name("path") else { return };
        let text = self.text(path);
        let (target, system) = match path.kind() {
            "system_lib_string" => (text.trim_start_matches('<').trim_end_matches('>').to_owned(), true),
            "string_literal" => (text.trim_matches('"').to_owned(), false),
            _ => return,
        };
        self.out.includes.push(IncludeRow { target, system, kind: "include", line: path.start_position().row as u32 + 1 });
    }

    fn macro_definition(&mut self, node: Node<'_>, function_like: bool) {
        let Some(name_node) = node.child_by_field_name("name") else { return };
        let name = self.text(name_node);
        let value = node.child_by_field_name("value");
        let mut parameters = HashSet::new();
        let mut signature = format!("#define {name}");
        if function_like && let Some(list) = node.child_by_field_name("parameters") {
            signature.push_str(&collapse(&self.original[list.byte_range()], 120));
            let mut cursor = list.walk();
            for parameter in list.named_children(&mut cursor) {
                parameters.insert(self.text(parameter));
            }
        }
        if let Some(value) = value {
            signature.push(' ');
            signature.push_str(&collapse(&self.original[value.byte_range()], 200));
        }
        let kind = if function_like { SymbolKind::MacroFunction } else { SymbolKind::Macro };
        let index = self.add_symbol(&name, kind, node, signature, Header { storage: "", inline: false });
        if let Some(value) = value {
            let body = &self.source[value.byte_range()];
            self.scan_macro_body(body, value.start_position().row as u32 + 1, index, &parameters);
        }
    }

    /// Identifiers of a macro body, found lexically: `name(` is a call, any other identifier a reference.
    fn scan_macro_body(&mut self, text: &[u8], first_line: u32, source: u32, parameters: &HashSet<String>) {
        let mut line = first_line;
        let mut index = 0;
        while index < text.len() {
            let byte = text[index];
            match byte {
                b'\n' => {
                    line += 1;
                    index += 1;
                }
                b'"' | b'\'' => {
                    index += 1;
                    while index < text.len() && text[index] != byte {
                        if text[index] == b'\\' {
                            index += 1;
                        }
                        if text.get(index) == Some(&b'\n') {
                            line += 1;
                        }
                        index += 1;
                    }
                    index += 1;
                }
                b'/' if text.get(index + 1) == Some(&b'*') => {
                    index += 2;
                    while index + 1 < text.len() && !(text[index] == b'*' && text[index + 1] == b'/') {
                        line += u32::from(text[index] == b'\n');
                        index += 1;
                    }
                    index += 2;
                }
                b'/' if text.get(index + 1) == Some(&b'/') => {
                    while index < text.len() && text[index] != b'\n' {
                        index += 1;
                    }
                }
                _ if byte.is_ascii_digit() => {
                    while index < text.len() && is_identifier_byte(text[index]) {
                        index += 1;
                    }
                }
                _ if byte.is_ascii_alphabetic() || byte == b'_' => {
                    let start = index;
                    while index < text.len() && is_identifier_byte(text[index]) {
                        index += 1;
                    }
                    let word = String::from_utf8_lossy(&text[start..index]).into_owned();
                    let before = text[..start].iter().rev().find(|byte| !matches!(byte, b' ' | b'\t' | b'\\' | b'\n' | b'\r'));
                    let field = matches!(before, Some(b'.')) || (matches!(before, Some(b'>')) && start >= 2 && text[start - 2] == b'-');
                    if field || parameters.contains(&word) || KEYWORDS.contains(&word.as_str()) {
                        continue;
                    }
                    let after = skip_spaces(text, index);
                    self.record(source, &word, text.get(after) == Some(&b'('), line);
                }
                _ => index += 1,
            }
        }
    }

    fn function(&mut self, node: Node<'_>) {
        let Some(declarator) = node.child_by_field_name("declarator") else { return };
        let Some(name_node) = declarator_name(declarator) else { return };
        let name = self.text(name_node);
        let body = node.child_by_field_name("body");
        let header_end = body.map_or(node.end_byte(), |body| body.start_byte());
        let signature = collapse(&self.original[node.start_byte()..header_end.max(node.start_byte())], 400);
        let header = header_words(&self.original[node.start_byte()..name_node.start_byte().max(node.start_byte())]);
        let index = self.add_symbol(&name, SymbolKind::Function, node, signature, header);
        if let Some(type_node) = node.child_by_field_name("type") {
            self.scan_uses(type_node, index);
        }
        self.scan_uses(declarator, index);
        if let Some(body) = body {
            self.scan_uses(body, index);
        }
    }

    fn declaration(&mut self, node: Node<'_>) {
        let type_node = node.child_by_field_name("type");
        if let Some(type_node) = type_node {
            self.collect_types(type_node);
        }
        let mut cursor = node.walk();
        let declarators: Vec<Node<'_>> = node.children_by_field_name("declarator", &mut cursor).collect();
        let Some(first) = declarators.first() else { return };
        let prefix_bytes = &self.original[node.start_byte()..first.start_byte().max(node.start_byte())];
        let prefix = collapse(prefix_bytes, 200);
        let rewritten = self.source[node.byte_range()] != self.original[node.byte_range()];
        for declarator in &declarators {
            let target = if declarator.kind() == "init_declarator" { declarator.child_by_field_name("declarator") } else { Some(*declarator) };
            let Some(target) = target else { continue };
            if is_prototype(target) {
                continue;
            }
            let Some(name_node) = declarator_name(target) else { continue };
            let name = self.text(name_node);
            let signature = if rewritten {
                let start = self.original[..node.start_byte()].iter().rposition(|&byte| byte == b'\n').map_or(0, |position| position + 1);
                let end = declarator.child_by_field_name("value").map_or(node.end_byte(), |value| value.start_byte());
                collapse(&self.original[start..end.max(start)], 200).trim_end_matches([';', '=', ' ']).to_owned()
            } else {
                format!("{prefix} {}", collapse(&self.original[target.byte_range()], 200))
            };
            let index = self.add_symbol(&name, SymbolKind::Variable, node, signature, header_words(prefix_bytes));
            if let Some(type_node) = type_node {
                self.scan_uses(type_node, index);
            }
            self.scan_uses(target, index);
            if let Some(value) = declarator.child_by_field_name("value") {
                self.scan_uses(value, index);
            }
        }
    }

    fn type_definition(&mut self, node: Node<'_>) {
        if let Some(type_node) = node.child_by_field_name("type") {
            self.collect_types(type_node);
        }
        let signature = collapse(&self.original[node.byte_range()], 300);
        let mut cursor = node.walk();
        let declarators: Vec<Node<'_>> = node.children_by_field_name("declarator", &mut cursor).collect();
        let mut first = None;
        for declarator in declarators {
            let Some(name_node) = declarator_name(declarator) else { continue };
            let name = self.text(name_node);
            let index = self.add_symbol(&name, SymbolKind::Typedef, node, signature.clone(), Header { storage: "", inline: false });
            first.get_or_insert(index);
        }
        if let Some(index) = first {
            self.scan_uses(node, index);
        }
    }

    /// Struct, union and enum definitions anywhere in a declaration's type (not inside function bodies).
    fn collect_types(&mut self, root: Node<'_>) {
        let mut found = Vec::new();
        walk_tree(root, |cursor, _| {
            let node = cursor.node();
            match node.kind() {
                "struct_specifier" | "union_specifier" | "enum_specifier" if node.child_by_field_name("body").is_some() => {
                    found.push(node);
                    false
                }
                "compound_statement" => false,
                _ => true,
            }
        });
        for node in found {
            self.type_specifier(node);
        }
    }

    fn type_specifier(&mut self, node: Node<'_>) {
        let (kind, keyword) = match node.kind() {
            "struct_specifier" => (SymbolKind::Struct, "struct"),
            "union_specifier" => (SymbolKind::Union, "union"),
            _ => (SymbolKind::Enum, "enum"),
        };
        let Some(body) = node.child_by_field_name("body") else { return };
        let mut owner = NO_SOURCE;
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = self.text(name_node);
            owner = self.add_symbol(&name, kind, node, format!("{keyword} {name}"), Header { storage: "", inline: false });
        }
        if kind == SymbolKind::Enum {
            let owner_name = node.child_by_field_name("name").map_or_else(|| "<anonymous>".to_owned(), |name| self.text(name));
            let mut cursor = body.walk();
            let enumerators: Vec<Node<'_>> = body.named_children(&mut cursor).filter(|child| child.kind() == "enumerator").collect();
            for enumerator in enumerators {
                let Some(name_node) = enumerator.child_by_field_name("name") else { continue };
                let name = self.text(name_node);
                let mut signature = name.clone();
                let value = enumerator.child_by_field_name("value");
                if let Some(value) = value {
                    signature.push_str(" = ");
                    signature.push_str(&collapse(&self.original[value.byte_range()], 120));
                }
                signature.push_str(&format!(" (enum {owner_name})"));
                let index = self.add_symbol(&name, SymbolKind::Enumerator, enumerator, signature, Header { storage: "", inline: false });
                if let Some(value) = value {
                    self.scan_uses(value, index);
                }
            }
        } else {
            self.collect_types(body);
        }
        if owner != NO_SOURCE {
            self.scan_uses(body, owner);
        }
    }

    /// The calls of loop macros that the rewrite hid, attributed to the function around their line.
    fn attribute_loops(&mut self, loops: &[(u32, String)]) {
        let functions: Vec<(u32, u32, u32)> = self
            .out
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, symbol)| symbol.kind == SymbolKind::Function)
            .map(|(index, symbol)| (symbol.line, symbol.end_line, index as u32))
            .collect();
        for (line, name) in loops {
            if let Some(&(_, _, index)) = functions.iter().find(|(start, end, _)| start <= line && line <= end) {
                self.record(index, name, true, *line);
            }
        }
    }

    /// Calls and identifier references inside `root`, attributed to the symbol `source`.
    fn scan_uses(&mut self, root: Node<'_>, source: u32) {
        let mut callee: Option<usize> = None;
        walk_tree(root, |cursor, parent| {
            let node = cursor.node();
            match node.kind() {
                "call_expression" => {
                    if let Some(function) = node.child_by_field_name("function").filter(|function| function.kind() == "identifier") {
                        let name = String::from_utf8_lossy(&self.source[function.byte_range()]).into_owned();
                        self.record(source, &name, true, function.start_position().row as u32 + 1);
                        callee = Some(function.id());
                    }
                    true
                }
                "identifier" | "type_identifier" => {
                    let declaring = cursor.field_name() == Some("declarator")
                        && matches!(
                            parent,
                            "init_declarator"
                                | "pointer_declarator"
                                | "array_declarator"
                                | "function_declarator"
                                | "parenthesized_declarator"
                                | "attributed_declarator"
                                | "parameter_declaration"
                                | "declaration"
                                | "field_declaration"
                                | "type_definition"
                                | "function_definition"
                        );
                    if Some(node.id()) != callee && !declaring {
                        let name = String::from_utf8_lossy(&self.source[node.byte_range()]).into_owned();
                        self.record(source, &name, false, node.start_position().row as u32 + 1);
                    }
                    false
                }
                "field_identifier" | "statement_identifier" | "string_literal" | "char_literal" | "comment" | "number_literal" | "primitive_type" => false,
                _ => true,
            }
        });
    }
}

pub fn extract_c(source: &[u8], parsers: &mut Parsers, out: &mut Extracted) -> Result<()> {
    let rewritten = rewrite(source);
    let tree = parse(parsers.c()?, &rewritten.text)?;
    let root = tree.root_node();
    out.parse_errors = count_errors(root);
    scan_markers(source, out);
    let mut extractor = Extractor { source: &rewritten.text, original: source, out, seen: HashSet::new() };
    extractor.visit_items(root, 0);
    extractor.attribute_loops(&rewritten.loops);
    let exported: HashSet<u32> = out.exports.iter().map(|export| export.name).collect();
    for symbol in &mut out.symbols {
        if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Variable) && exported.contains(&symbol.name) {
            symbol.exported = true;
        }
    }
    Ok(())
}

/// The first `limit` error regions of a C file as (line, text), for tuning the rewrite.
pub fn error_snippets(source: &[u8], parsers: &mut Parsers, limit: usize) -> Result<Vec<(u32, String)>> {
    let rewritten = rewrite(source);
    let tree = parse(parsers.c()?, &rewritten.text)?;
    let mut found = Vec::new();
    walk_tree(tree.root_node(), |cursor, _| {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            if found.len() < limit {
                let end = (node.start_byte() + 160).min(node.end_byte().max(node.start_byte() + 1)).min(source.len());
                let mut text = collapse(&source[node.start_byte().min(end)..end], 110);
                if node.is_missing() {
                    let from = source[..node.start_byte().min(source.len())].iter().rposition(|&byte| byte == b'\n').map_or(0, |position| position + 1);
                    let to = source[from..].iter().position(|&byte| byte == b'\n').map_or(source.len(), |position| from + position);
                    text = format!("MISSING {} in `{}`", node.kind(), collapse(&source[from..to], 100));
                }
                found.push((node.start_position().row as u32 + 1, text));
            }
            return false;
        }
        node.has_error()
    });
    Ok(found)
}

/// Assembly: `#include` lines and the entry points declared with the `SYM_*`, `ENTRY` and `.globl` directives.
pub fn extract_asm(source: &[u8], out: &mut Extracted) {
    for (index, raw) in source.split(|&byte| byte == b'\n').enumerate() {
        let line = String::from_utf8_lossy(raw);
        let trimmed = line.trim();
        let number = index as u32 + 1;
        if let Some(rest) = trimmed.strip_prefix('#') {
            if let Some(rest) = rest.trim_start().strip_prefix("include") {
                let rest = rest.trim();
                let target = match rest.as_bytes().first() {
                    Some(b'<') => rest.trim_start_matches('<').split('>').next().map(|target| (target, true)),
                    Some(b'"') => rest.trim_start_matches('"').split('"').next().map(|target| (target, false)),
                    _ => None,
                };
                if let Some((target, system)) = target {
                    out.includes.push(IncludeRow { target: target.to_owned(), system, kind: "include", line: number });
                }
            }
            continue;
        }
        let (kind, name) = if let Some(rest) = trimmed.strip_prefix(".globl").or_else(|| trimmed.strip_prefix(".global")) {
            (SymbolKind::Function, rest.trim().split(|ch: char| ch == ',' || ch.is_whitespace()).next().unwrap_or(""))
        } else {
            let word_end = trimmed.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).unwrap_or(trimmed.len());
            let word = &trimmed[..word_end];
            let argument = trimmed[word_end..].strip_prefix('(').and_then(|rest| rest.split([',', ')']).next()).map(str::trim).unwrap_or("");
            let kind = if word.starts_with("SYM_DATA") {
                SymbolKind::Variable
            } else if word.starts_with("SYM_FUNC") || word.starts_with("SYM_CODE") || word.starts_with("SYM_TYPED_FUNC") || word == "ENTRY" || word == "GLOBAL" {
                SymbolKind::Function
            } else {
                continue;
            };
            if word.ends_with("_END") || word.ends_with("_ALIAS") {
                continue;
            }
            (kind, argument)
        };
        if !is_identifier(name.as_bytes()) {
            continue;
        }
        let name = out.name(name);
        out.symbols.push(SymbolRow { name, kind, line: number, end_line: number, signature: collapse(trimmed.as_bytes(), 200), storage: "", inline: false, exported: false });
    }
    scan_markers(source, out);
    let exported: HashSet<u32> = out.exports.iter().map(|export| export.name).collect();
    for symbol in &mut out.symbols {
        symbol.exported = exported.contains(&symbol.name);
    }
}
