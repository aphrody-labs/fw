//! Kbuild extraction: the object lists of Makefiles and Kbuild files (`obj-$(CONFIG_X) += a.o b/`, `foo-y := a.o`, `foo-objs := ...`)
//! as edges from a gate (a CONFIG symbol, `y` or `m`) to an object, a directory or a part of a composite object.

use crate::error::Result;
use crate::graph::{BuildEdgeRow, Extracted};
use crate::graph_c::{Parsers, count_errors, parse};

const LISTS: [&str; 16] = [
    "obj", "lib", "subdir", "core", "drivers", "libs", "net", "virt", "head", "init", "always", "extra", "hostprogs", "userprogs", "targets", "mod",
];
const ROOT_LISTS: [&str; 7] = ["core", "drivers", "libs", "net", "virt", "head", "init"];

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

struct Gate {
    config: Option<String>,
    gate: &'static str,
}

/// `obj-$(CONFIG_X)` -> (`obj`, gate X); the suffix after the last dash is `y`, `m`, `objs` or `$(CONFIG_X)`.
fn split_name(name: &str) -> Option<(&str, Gate)> {
    let (prefix, suffix) = name.rsplit_once('-')?;
    if prefix.is_empty() || prefix.contains('$') {
        return None;
    }
    let gate = match suffix {
        "y" => Gate { config: None, gate: "y" },
        "m" => Gate { config: None, gate: "m" },
        "objs" => Gate { config: None, gate: "objs" },
        other => {
            let config = other.strip_prefix("$(CONFIG_")?.strip_suffix(')')?;
            if config.contains('$') || config.is_empty() {
                return None;
            }
            Gate { config: Some(config.to_owned()), gate: "config" }
        }
    };
    Some((prefix, gate))
}

struct Extractor<'a> {
    path: &'a str,
    out: &'a mut Extracted,
    conditions: Vec<String>,
}

fn is_condition(word: &str) -> bool {
    matches!(word, "ifeq" | "ifneq" | "ifdef" | "ifndef")
}

impl Extractor<'_> {
    /// Logical lines (continuations joined) with the number of their first physical line.
    fn scan(&mut self, text: &str) {
        let mut pending = String::new();
        let mut first = 0;
        for (index, raw) in text.lines().enumerate() {
            if pending.is_empty() {
                first = index as u32 + 1;
            }
            if let Some(stripped) = raw.strip_suffix('\\') {
                pending.push_str(stripped);
                pending.push(' ');
                continue;
            }
            pending.push_str(raw);
            let line = std::mem::take(&mut pending);
            self.logical(&line, first);
        }
        if !pending.is_empty() {
            self.logical(&pending, first);
        }
    }

    fn logical(&mut self, line: &str, number: u32) {
        if line.starts_with('\t') {
            return;
        }
        let line = line.split_once('#').map_or(line, |(code, _)| if code.is_empty() || code.ends_with(char::is_whitespace) { code } else { line }).trim();
        let Some(word) = line.split(|ch: char| ch.is_whitespace() || ch == '(').next().filter(|word| !word.is_empty()) else { return };
        if is_condition(word) {
            self.conditions.push(line.split_whitespace().collect::<Vec<_>>().join(" "));
        } else if word == "else" {
            let rest = line.strip_prefix("else").unwrap_or("").trim();
            if !self.conditions.is_empty() {
                self.conditions.pop();
            }
            self.conditions.push(if rest.is_empty() { "else".to_owned() } else { format!("else {rest}") });
        } else if word == "endif" {
            self.conditions.pop();
        } else if let Some(position) = line.find('=') {
            let (left, right) = line.split_at(position);
            let name = left.trim_end_matches([':', '+', '?', ' ', '\t']);
            if !name.is_empty() && !name.contains(char::is_whitespace) {
                self.assignment(name, &right[1..], number);
            }
        }
    }

    fn assignment(&mut self, name: &str, value: &str, line: u32) {
        let Some((prefix, gate)) = split_name(name) else { return };
        let directory = self.path.rsplit_once('/').map_or("", |(directory, _)| directory);
        let list = LISTS.contains(&prefix);
        let root_relative = list && ROOT_LISTS.contains(&prefix) && (self.path == "Makefile" || (self.path.starts_with("arch/") && self.path.matches('/').count() == 2));
        let parent = (!list).then(|| normalize(&format!("{directory}/{prefix}.o")));
        let condition = self.conditions.join(" && ");
        for token in value.split_whitespace() {
            if token.contains('$') || token.starts_with('-') {
                continue;
            }
            let directory_entry = token.ends_with('/');
            if !directory_entry && !token.ends_with(".o") {
                continue;
            }
            let joined = if root_relative { token.to_owned() } else { format!("{directory}/{token}") };
            let mut target = normalize(&joined);
            if directory_entry {
                target.push('/');
            }
            let kind = if list { prefix.to_owned() } else { "composite".to_owned() };
            let config = gate.config.as_deref().map(|config| self.out.name(config));
            self.out.build_edges.push(BuildEdgeRow { line, kind, config, gate: gate.gate.to_owned(), condition: condition.clone(), parent: parent.clone(), target });
        }
    }
}

/// tree-sitter-make rejects `name-$(VAR) op value` assignments, the core Kbuild idiom, so its parse only measures the error rate
/// and the edges come from the logical lines of the file.
pub fn extract(source: &[u8], path: &str, parsers: &mut Parsers, out: &mut Extracted) -> Result<()> {
    let tree = parse(parsers.make()?, source)?;
    out.parse_errors = count_errors(tree.root_node());
    Extractor { path, out, conditions: Vec::new() }.scan(&String::from_utf8_lossy(source));
    Ok(())
}
