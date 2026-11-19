#![cfg(feature = "tree-sitter-all")]

use serde::Serialize;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};

pub const SUPPORTED_GRAMMARS: &[&str] = &[
    "rust",
    "typescript",
    "tsx",
    "javascript",
    "python",
    "c",
    "cpp",
    "java",
    "kotlin",
    "go",
    "json",
    "bash",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AstSummary {
    pub ok: bool,
    pub lang: String,
    pub root_kind: String,
    pub node_count: usize,
    pub has_error: bool,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AstSymbol {
    pub kind: String,
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AstSymbolsReport {
    pub ok: bool,
    pub lang: String,
    pub symbol_count: usize,
    pub symbols: Vec<AstSymbol>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AstQueryMatch {
    pub capture: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AstQueryReport {
    pub ok: bool,
    pub lang: String,
    pub match_count: usize,
    pub matches: Vec<AstQueryMatch>,
}

pub fn resolve_lang(lang: &str) -> Option<Language> {
    match lang.to_ascii_lowercase().as_str() {
        "rust" | "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" | "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" | "js" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python" | "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "c" | "h" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" | "c++" | "cc" | "cxx" | "hpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "kotlin" | "kt" | "kts" => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        "bash" | "sh" => Some(tree_sitter_bash::LANGUAGE.into()),
        _ => None,
    }
}

fn count_nodes_and_errors(n: Node<'_>) -> (usize, usize) {
    let mut nodes = 1usize;
    let mut errors = usize::from(n.is_error() || n.is_missing());
    let mut cur = n.walk();
    for ch in n.children(&mut cur) {
        let (cn, ce) = count_nodes_and_errors(ch);
        nodes += cn;
        errors += ce;
    }
    (nodes, errors)
}

pub fn parse_summary(lang_str: &str, src: &[u8]) -> Result<AstSummary, String> {
    let lang = resolve_lang(lang_str).ok_or_else(|| format!("unsupported grammar: {lang_str}"))?;
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .map_err(|e| format!("set_language failed: {e}"))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| "parse returned None".to_string())?;
    let root = tree.root_node();
    let (node_count, error_count) = count_nodes_and_errors(root);
    Ok(AstSummary {
        ok: true,
        lang: lang_str.to_string(),
        root_kind: root.kind().to_string(),
        node_count,
        has_error: root.has_error(),
        error_count,
    })
}

fn is_symbol_node(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "impl_item"
            | "function_declaration"
            | "method_definition"
            | "class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "function_definition"
            | "class_definition"
            | "method_declaration"
            | "constructor_declaration"
            | "enum_declaration"
            | "type_spec"
            | "struct_specifier"
            | "class_specifier"
            | "object_declaration"
            | "type_alias"
    )
}

fn extract_node_name(node: Node<'_>, src: &[u8]) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name")
        && let Ok(t) = name_node.utf8_text(src)
    {
        return Some(t.trim().to_string());
    }
    if let Some(type_node) = node.child_by_field_name("type")
        && let Ok(t) = type_node.utf8_text(src)
    {
        return Some(t.trim().to_string());
    }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if matches!(
            child.kind(),
            "identifier"
                | "simple_identifier"
                | "type_identifier"
                | "field_identifier"
                | "property_identifier"
        ) && let Ok(t) = child.utf8_text(src)
        {
            return Some(t.trim().to_string());
        }
        if child.kind() == "function_declarator"
            && let Some(nested) = extract_node_name(child, src)
        {
            return Some(nested);
        }
    }
    None
}

fn collect_symbols(node: Node<'_>, src: &[u8], out: &mut Vec<AstSymbol>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    if is_symbol_node(node.kind())
        && let Some(name) = extract_node_name(node, src)
    {
        out.push(AstSymbol {
            kind: node.kind().to_string(),
            name,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
        });
    }
    let mut cur = node.walk();
    for ch in node.children(&mut cur) {
        collect_symbols(ch, src, out, limit);
    }
}

pub fn extract_symbols(lang_str: &str, src: &[u8], limit: usize) -> Result<AstSymbolsReport, String> {
    let lang = resolve_lang(lang_str).ok_or_else(|| format!("unsupported grammar: {lang_str}"))?;
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .map_err(|e| format!("set_language failed: {e}"))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| "parse returned None".to_string())?;
    let mut symbols = Vec::new();
    collect_symbols(tree.root_node(), src, &mut symbols, limit);
    Ok(AstSymbolsReport {
        ok: true,
        lang: lang_str.to_string(),
        symbol_count: symbols.len(),
        symbols,
    })
}

pub fn run_query(lang_str: &str, src: &[u8], query_src: &str, limit: usize) -> Result<AstQueryReport, String> {
    let lang = resolve_lang(lang_str).ok_or_else(|| format!("unsupported grammar: {lang_str}"))?;
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .map_err(|e| format!("set_language failed: {e}"))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| "parse returned None".to_string())?;
    let query = Query::new(&lang, query_src).map_err(|e| format!("invalid query: {e}"))?;
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches_iter = cursor.matches(&query, tree.root_node(), src);
    let mut matches = Vec::new();

    while let Some(m) = matches_iter.next() {
        for cap in m.captures() {
            if matches.len() >= limit {
                break;
            }
            let node = cap.node;
            let text = node
                .utf8_text(src)
                .unwrap_or("")
                .chars()
                .take(120)
                .collect::<String>();
            let cname = capture_names
                .get(cap.index as usize)
                .copied()
                .unwrap_or("capture");
            matches.push(AstQueryMatch {
                capture: cname.to_string(),
                kind: node.kind().to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                text,
            });
        }
        if matches.len() >= limit {
            break;
        }
    }

    Ok(AstQueryReport {
        ok: true,
        lang: lang_str.to_string(),
        match_count: matches.len(),
        matches,
    })
}
