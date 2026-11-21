//! Kconfig extraction: `config`, `menuconfig`, named `choice` and `menu` entries as symbols, and the edges between them
//! (`depends on`, `select`, `imply`, `default`, `range`), including the conditions inherited from enclosing `if` and `menu` blocks.

use tree_sitter::Node;

use crate::error::Result;
use crate::graph::{ConfigEdgeRow, Extracted, IncludeRow, SymbolKind, SymbolRow};
use crate::graph_c::{Parsers, count_errors, parse};

fn is_config_symbol(text: &str) -> bool {
    !matches!(text, "y" | "m" | "n") && text.bytes().any(|byte| byte.is_ascii_alphabetic()) && !text.starts_with("0x") && !text.starts_with("0X")
}

fn collapse(text: &str, limit: usize) -> String {
    let mut out = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.len() > limit {
        let mut cut = limit;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
    }
    out
}

struct Extractor<'a> {
    source: &'a [u8],
    out: &'a mut Extracted,
    /// Conditions of the enclosing `if` and `menu` blocks: (expression text, the block kind).
    context: Vec<(String, &'static str)>,
}

impl Extractor<'_> {
    fn text(&self, node: Node<'_>) -> String {
        String::from_utf8_lossy(&self.source[node.byte_range()]).into_owned()
    }

    fn symbols_in(&self, node: Node<'_>) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut stack = vec![node];
        while let Some(current) = stack.pop() {
            if current.kind() == "symbol" {
                let text = self.text(current);
                if is_config_symbol(&text) && !found.contains(&text) {
                    found.push(text);
                }
                continue;
            }
            if matches!(current.kind(), "macro_variable" | "string") {
                continue;
            }
            stack.extend((0..current.child_count()).rev().filter_map(|index| current.child(index)));
        }
        found
    }

    fn edge(&mut self, source: u32, kind: &'static str, target: Option<&str>, expr: &str, via: &'static str, line: u32) {
        let target = target.map(|target| self.out.name(target));
        self.out.config_edges.push(ConfigEdgeRow { source, kind, target, expr: expr.to_owned(), via, line });
    }

    fn entries(&mut self, node: Node<'_>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor).filter(Node::is_named) {
            match child.kind() {
                "config" | "menuconfig" => self.config(child),
                "choice" => self.choice(child),
                "menu" => self.menu(child),
                "if" => self.conditional(child),
                "source" => self.source(child),
                _ => {}
            }
        }
    }

    fn source(&mut self, node: Node<'_>) {
        let mut cursor = node.walk();
        let Some(path) = node.named_children(&mut cursor).find(|child| child.kind() == "string") else { return };
        let target = self.text(path).trim_matches(['"', '\'']).to_owned();
        self.out.includes.push(IncludeRow { target, system: false, kind: "source", line: node.start_position().row as u32 + 1 });
    }

    fn conditional(&mut self, node: Node<'_>) {
        let condition = node.child_by_field_name("condition").map(|condition| collapse(&self.text(condition), 300)).unwrap_or_default();
        self.context.push((condition, "if"));
        self.entries(node);
        self.context.pop();
    }

    fn menu(&mut self, node: Node<'_>) {
        let title = node.child_by_field_name("name").map(|name| self.text(name).trim_matches(['"', '\'']).to_owned()).unwrap_or_default();
        let (line, end_line) = lines(node);
        let name = self.out.name(&title);
        self.out.symbols.push(SymbolRow { name, kind: SymbolKind::Menu, line, end_line, signature: format!("menu \"{title}\""), storage: "", inline: false, exported: false });
        let index = (self.out.symbols.len() - 1) as u32;
        let mut pushed = 0;
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
        for child in &children {
            if child.kind() == "dependencies"
                && let Some(expression) = child.named_child(0)
            {
                let expr = collapse(&self.text(expression), 300);
                self.edge_for_symbols(index, "depends", expression, &expr, "direct", child.start_position().row as u32 + 1);
                self.context.push((expr, "menu"));
                pushed += 1;
            }
        }
        self.entries(node);
        for _ in 0..pushed {
            self.context.pop();
        }
    }

    fn edge_for_symbols(&mut self, source: u32, kind: &'static str, node: Node<'_>, expr: &str, via: &'static str, line: u32) {
        let symbols = self.symbols_in(node);
        if symbols.is_empty() {
            self.edge(source, kind, None, expr, via, line);
        }
        for symbol in symbols {
            self.edge(source, kind, Some(&symbol), expr, via, line);
        }
    }

    fn choice(&mut self, node: Node<'_>) {
        if let Some(name) = node.child_by_field_name("name") {
            let name = self.text(name);
            self.symbol(node, name.trim(), "choice".to_owned());
        } else {
            self.entries(node);
        }
    }

    fn config(&mut self, node: Node<'_>) {
        let Some(name) = node.child_by_field_name("name") else { return };
        let name = self.text(name).trim().to_owned();
        let mut signature = String::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if matches!(child.kind(), "type_definition" | "type_definition_default") {
                signature = collapse(&self.text(child), 160);
                break;
            }
        }
        self.symbol(node, &name, signature);
    }

    fn symbol(&mut self, node: Node<'_>, name: &str, signature: String) {
        let (line, end_line) = lines(node);
        let local = self.out.name(name);
        self.out.symbols.push(SymbolRow { name: local, kind: SymbolKind::Kconfig, line, end_line, signature, storage: "", inline: false, exported: false });
        let index = (self.out.symbols.len() - 1) as u32;
        for position in 0..self.context.len() {
            let (expr, via) = self.context[position].clone();
            let symbols: Vec<String> = expr.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).filter(|word| is_config_symbol(word)).map(str::to_owned).collect();
            if symbols.is_empty() {
                self.edge(index, "depends", None, &expr, via, line);
            }
            for symbol in symbols {
                self.edge(index, "depends", Some(&symbol), &expr, via, line);
            }
        }
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
        for child in children {
            let child_line = child.start_position().row as u32 + 1;
            match child.kind() {
                "dependencies" => {
                    if let Some(expression) = child.named_child(0) {
                        let expr = collapse(&self.text(expression), 300);
                        self.edge_for_symbols(index, "depends", expression, &expr, "direct", child_line);
                    }
                }
                "reverse_dependencies" | "weak_reverse_dependencies" => {
                    let kind = if child.kind() == "reverse_dependencies" { "select" } else { "imply" };
                    let mut inner = child.walk();
                    let parts: Vec<Node<'_>> = child.named_children(&mut inner).collect();
                    let Some(target) = parts.iter().find(|part| matches!(part.kind(), "name" | "symbol")) else { continue };
                    let target = self.text(*target).trim().to_owned();
                    let condition = parts.iter().find(|part| part.kind() == "conditional_clause").and_then(|clause| clause.named_child(0));
                    let expr = condition.map(|condition| collapse(&self.text(condition), 300)).unwrap_or_default();
                    self.edge(index, kind, Some(&target), &expr, "direct", child_line);
                }
                "default_value" | "type_definition_default" => {
                    let mut inner = child.walk();
                    let parts: Vec<Node<'_>> = child.named_children(&mut inner).collect();
                    let value = parts.iter().find(|part| part.kind() != "conditional_clause").map(|value| collapse(&self.text(*value), 200)).unwrap_or_default();
                    let condition = parts.iter().find(|part| part.kind() == "conditional_clause").and_then(|clause| clause.named_child(0));
                    let expr = match condition {
                        Some(condition) => format!("{value} if {}", collapse(&self.text(condition), 300)),
                        None => value,
                    };
                    let mut targets = self.symbols_in(child);
                    targets.retain(|target| !expr.is_empty() && target != "default");
                    if targets.is_empty() {
                        self.edge(index, "default", None, &expr, "direct", child_line);
                    }
                    for target in targets {
                        self.edge(index, "default", Some(&target), &expr, "direct", child_line);
                    }
                }
                "numerical_ranges" => {
                    let expr = collapse(&self.text(child), 200);
                    self.edge_for_symbols(index, "range", child, &expr, "direct", child_line);
                }
                _ => {}
            }
        }
        self.entries(node);
    }
}

fn lines(node: Node<'_>) -> (u32, u32) {
    let start = node.start_position();
    let end = node.end_position();
    let end_line = if end.column == 0 && end.row > start.row { end.row } else { end.row + 1 };
    (start.row as u32 + 1, end_line as u32)
}

pub fn extract(source: &[u8], parsers: &mut Parsers, out: &mut Extracted) -> Result<()> {
    let tree = parse(parsers.kconfig()?, source)?;
    let root = tree.root_node();
    out.parse_errors = count_errors(root);
    Extractor { source, out, context: Vec::new() }.entries(root);
    Ok(())
}
