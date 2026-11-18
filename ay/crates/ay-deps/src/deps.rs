//! gclient's DEPS files. A DEPS file is a Python module of plain assignments (`vars`, `deps`, `hooks`, `recursedeps`, ...) whose values are
//! literals, `Var('name')` calls and string concatenations; dependencies and hooks carry a `condition` written as a Python expression.
//! This crate reads that subset with a real Python grammar and resolves it the way gclient does: `Var('x')` stays `{x}` until the variables
//! of the checkout are known, then every string is formatted and every condition evaluated.

use std::collections::BTreeMap;

use serde::Serialize;
use thiserror::Error;
use tree_sitter::{Node, Parser};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("syntax error near line {0}")]
    Syntax(usize),
    #[error("line {line}: {what} is not supported in a DEPS file")]
    Unsupported { line: usize, what: String },
    #[error("unknown variable {0}")]
    UnknownVariable(String),
    #[error("dependency {path}: {what}")]
    Dependency { path: String, what: String },
    #[error("invalid condition: {0}")]
    Condition(String),
}

type Result<T> = std::result::Result<T, Error>;

/// A value of a DEPS file. Dictionaries keep the order of the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Str(String),
    Bool(bool),
    Int(i64),
    None,
    List(Vec<Value>),
    Dict(Vec<(String, Value)>),
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::Str(text) => !text.is_empty(),
            Value::Bool(flag) => *flag,
            Value::Int(number) => *number != 0,
            Value::None => false,
            Value::List(items) => !items.is_empty(),
            Value::Dict(items) => !items.is_empty(),
        }
    }

    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(items) => items
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Value::Str(text) => Some(text),
            _ => None,
        }
    }
}

/// The parsed top-level assignments of a DEPS file.
#[derive(Debug, Default, Clone)]
pub struct Deps {
    pub vars: Vec<(String, Value)>,
    pub deps: Vec<(String, Value)>,
    pub hooks: Vec<Value>,
    pub recursedeps: Vec<String>,
    /// Every other assignment (`gclient_gn_args_file`, `include_rules`, `allowed_hosts`, ...), by name.
    pub other: BTreeMap<String, Value>,
}

fn parse_tree(text: &str) -> Result<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|_| Error::Syntax(0))?;
    let tree = parser.parse(text, None).ok_or(Error::Syntax(0))?;
    if tree.root_node().has_error() {
        let line = first_error(tree.root_node()).map_or(0, |node| node.start_position().row + 1);
        return Err(Error::Syntax(line));
    }
    Ok(tree)
}

fn first_error(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).find_map(first_error)
}

fn unsupported(node: Node<'_>, source: &str) -> Error {
    let excerpt: String = source[node.byte_range()].chars().take(40).collect();
    Error::Unsupported {
        line: node.start_position().row + 1,
        what: format!("{} `{excerpt}`", node.kind()),
    }
}

/// The text of a Python string literal (all prefixes, all quote styles).
fn string_literal(raw: &str) -> Option<String> {
    let body = raw.trim_start_matches(|ch: char| ch.is_ascii_alphabetic());
    let prefix = &raw[..raw.len() - body.len()];
    let is_raw = prefix.chars().any(|ch| ch == 'r' || ch == 'R');
    let quote = if body.starts_with("\"\"\"") || body.starts_with("'''") {
        3
    } else {
        1
    };
    if body.len() < quote * 2 {
        return None;
    }
    let inner = &body[quote..body.len() - quote];
    if is_raw {
        return Some(inner.to_owned());
    }
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '\n' => {}
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    Some(out)
}

fn eval(node: Node<'_>, source: &str) -> Result<Value> {
    let text = &source[node.byte_range()];
    let mut cursor = node.walk();
    match node.kind() {
        "string" => string_literal(text)
            .map(Value::Str)
            .ok_or_else(|| unsupported(node, source)),
        "concatenated_string" => {
            let mut joined = String::new();
            for part in node.named_children(&mut cursor) {
                match eval(part, source)? {
                    Value::Str(piece) => joined.push_str(&piece),
                    _ => return Err(unsupported(part, source)),
                }
            }
            Ok(Value::Str(joined))
        }
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        "none" => Ok(Value::None),
        "integer" => text
            .replace('_', "")
            .parse()
            .map(Value::Int)
            .map_err(|_| unsupported(node, source)),
        "parenthesized_expression" => {
            let inner = node
                .named_children(&mut cursor)
                .find(|child| child.kind() != "comment")
                .ok_or_else(|| unsupported(node, source))?;
            eval(inner, source)
        }
        "list" | "tuple" => node
            .named_children(&mut cursor)
            .filter(|child| child.kind() != "comment")
            .map(|child| eval(child, source))
            .collect::<Result<Vec<_>>>()
            .map(Value::List),
        "dictionary" => {
            let mut items = Vec::new();
            for pair in node
                .named_children(&mut cursor)
                .filter(|child| child.kind() != "comment")
            {
                if pair.kind() != "pair" {
                    return Err(unsupported(pair, source));
                }
                let key = pair
                    .child_by_field_name("key")
                    .ok_or_else(|| unsupported(pair, source))?;
                let value = pair
                    .child_by_field_name("value")
                    .ok_or_else(|| unsupported(pair, source))?;
                match eval(key, source)? {
                    Value::Str(name) => items.push((name, eval(value, source)?)),
                    _ => return Err(unsupported(key, source)),
                }
            }
            Ok(Value::Dict(items))
        }
        "call" => {
            let function = node
                .child_by_field_name("function")
                .map(|function| &source[function.byte_range()]);
            let mut argument_cursor = node.walk();
            let arguments: Vec<Node<'_>> = node
                .child_by_field_name("arguments")
                .map(|list| {
                    list.named_children(&mut argument_cursor)
                        .filter(|argument| argument.kind() != "comment")
                        .collect()
                })
                .unwrap_or_default();
            match (function, arguments.as_slice()) {
                (Some("Var"), [name]) => match eval(*name, source)? {
                    Value::Str(name) => Ok(Value::Str(format!("{{{name}}}"))),
                    _ => Err(unsupported(node, source)),
                },
                (Some("Str"), [value]) => eval(*value, source),
                _ => Err(unsupported(node, source)),
            }
        }
        "binary_operator" => {
            let left = node
                .child_by_field_name("left")
                .ok_or_else(|| unsupported(node, source))?;
            let right = node
                .child_by_field_name("right")
                .ok_or_else(|| unsupported(node, source))?;
            let operator = node
                .child_by_field_name("operator")
                .map(|operator| &source[operator.byte_range()]);
            match (operator, eval(left, source)?, eval(right, source)?) {
                (Some("+"), Value::Str(first), Value::Str(second)) => {
                    Ok(Value::Str(first + &second))
                }
                (Some("+"), Value::List(mut first), Value::List(second)) => {
                    first.extend(second);
                    Ok(Value::List(first))
                }
                _ => Err(unsupported(node, source)),
            }
        }
        _ => Err(unsupported(node, source)),
    }
}

impl Deps {
    /// Reads the top-level assignments of a DEPS file.
    pub fn parse(source: &str) -> Result<Deps> {
        let tree = parse_tree(source)?;
        let root = tree.root_node();
        let mut deps = Deps::default();
        let mut cursor = root.walk();
        for statement in root.named_children(&mut cursor) {
            if statement.kind() == "comment" {
                continue;
            }
            let assignment = statement.named_child(0).filter(|child| {
                statement.kind() == "expression_statement" && child.kind() == "assignment"
            });
            let Some(assignment) = assignment else {
                return Err(unsupported(statement, source));
            };
            let name = assignment
                .child_by_field_name("left")
                .map(|left| source[left.byte_range()].to_owned())
                .ok_or_else(|| unsupported(assignment, source))?;
            let right = assignment
                .child_by_field_name("right")
                .ok_or_else(|| unsupported(assignment, source))?;
            let value = eval(right, source)?;
            match (name.as_str(), value) {
                ("vars", Value::Dict(items)) => deps.vars = items,
                ("deps", Value::Dict(items)) => deps.deps = items,
                ("hooks", Value::List(items)) => deps.hooks = items,
                ("recursedeps", Value::List(items)) => {
                    deps.recursedeps = items
                        .into_iter()
                        .filter_map(|item| item.text().map(str::to_owned))
                        .collect()
                }
                (_, value) => {
                    deps.other.insert(name, value);
                }
            }
        }
        Ok(deps)
    }
}

/// The variables of a checkout: the file's own `vars`, gclient's built-ins for the host and target, and the caller's overrides.
#[derive(Debug, Clone)]
pub struct Env(BTreeMap<String, Value>);

// The built-in variables of gclient (depot_tools gclient.py, Dependency.get_builtin_vars): one checkout_<os> and one checkout_<cpu> per name,
// plus host_os and host_cpu. There is no target_os or target_cpu variable: those are keys of the .gclient file.
const OSES: &[&str] = &[
    "android", "chromeos", "fuchsia", "ios", "linux", "mac", "win",
];
const CPUS: &[&str] = &[
    "arm", "arm64", "x86", "mips", "mips64", "ppc", "riscv64", "s390", "x64", "loong64",
];

impl Env {
    /// The variables of a checkout, as gclient computes them: the file's vars with their own `{name}` placeholders expanded once against the
    /// declared, built-in (`checkout_<os>`, `checkout_<cpu>`, `host_os`, `host_cpu`) and overridden variables, then the built-ins, then the
    /// overrides. An override replaces a variable that exists; it never introduces one.
    pub fn new(
        deps: &Deps,
        host_os: &str,
        host_cpu: &str,
        target_os: &str,
        target_cpu: &str,
        overrides: &[(String, Value)],
    ) -> Result<Env> {
        let mut builtin = BTreeMap::new();
        for os in OSES {
            builtin.insert(format!("checkout_{os}"), Value::Bool(*os == target_os));
        }
        for cpu in CPUS {
            builtin.insert(format!("checkout_{cpu}"), Value::Bool(*cpu == target_cpu));
        }
        builtin.insert("host_os".to_owned(), Value::Str(host_os.to_owned()));
        builtin.insert("host_cpu".to_owned(), Value::Str(host_cpu.to_owned()));

        let mut raw: BTreeMap<String, Value> = deps.vars.iter().cloned().collect();
        raw.extend(builtin.clone());
        for (name, value) in overrides {
            if let Some(slot) = raw.get_mut(name) {
                *slot = value.clone();
            }
        }
        let mut map = BTreeMap::new();
        for (name, value) in &deps.vars {
            let expanded = match value {
                Value::Str(text) => Value::Str(format_with(&raw, text)?),
                other => other.clone(),
            };
            map.insert(name.clone(), expanded);
        }
        map.extend(builtin);
        for (name, value) in overrides {
            if let Some(slot) = map.get_mut(name) {
                *slot = value.clone();
            }
        }
        Ok(Env(map))
    }

    fn text(value: &Value) -> Option<String> {
        match value {
            Value::Str(text) => Some(text.clone()),
            Value::Bool(flag) => Some(if *flag { "True" } else { "False" }.to_owned()),
            Value::Int(number) => Some(number.to_string()),
            _ => None,
        }
    }

    /// Replaces every `{name}` of a string by the variable's value; `{{` and `}}` stand for a literal brace, as in Python's `str.format`.
    pub fn format(&self, template: &str) -> Result<String> {
        format_with(&self.0, template)
    }

    /// Evaluates a condition the way gclient's `EvaluateCondition` does: `and`, `or` and `not` take booleans only, `==`, `!=` and `in` (against a
    /// tuple) are the only comparisons, a string-valued variable is itself a condition, and a name that is not a variable stands for its own text.
    pub fn condition(&self, expression: &str) -> Result<bool> {
        Ok(self.condition_value(expression, &mut Vec::new())?.truthy())
    }

    fn condition_value(&self, expression: &str, seen: &mut Vec<String>) -> Result<Value> {
        let tree = parse_tree(expression)?;
        let root = tree.root_node();
        let statement = root
            .named_child(0)
            .and_then(|statement| statement.named_child(0))
            .ok_or(Error::Syntax(1))?;
        self.expression(statement, expression, false, seen)
    }

    fn boolean(
        &self,
        node: Node<'_>,
        source: &str,
        seen: &mut Vec<String>,
        operator: &str,
    ) -> Result<bool> {
        match self.expression(node, source, false, seen)? {
            Value::Bool(flag) => Ok(flag),
            other => Err(Error::Condition(format!(
                "invalid \"{operator}\" operand {other:?} (inside `{source}`)"
            ))),
        }
    }

    fn expression(
        &self,
        node: Node<'_>,
        source: &str,
        allow_tuple: bool,
        seen: &mut Vec<String>,
    ) -> Result<Value> {
        let mut cursor = node.walk();
        match node.kind() {
            "identifier" => {
                let name = &source[node.byte_range()];
                if seen.iter().any(|visited| visited == name) {
                    return Err(Error::Condition(format!(
                        "cyclic reference to {name} (inside `{source}`)"
                    )));
                }
                match self.0.get(name) {
                    Some(Value::Str(text)) => {
                        seen.push(name.to_owned());
                        let value = self.condition_value(text, seen);
                        seen.pop();
                        value
                    }
                    Some(other) => Ok(other.clone()),
                    None => Ok(Value::Str(name.to_owned())),
                }
            }
            "parenthesized_expression" => {
                let inner = node
                    .named_children(&mut cursor)
                    .next()
                    .ok_or_else(|| unsupported(node, source))?;
                self.expression(inner, source, false, seen)
            }
            "boolean_operator" => {
                let operator = node
                    .child_by_field_name("operator")
                    .map(|operator| &source[operator.byte_range()]);
                let left = node
                    .child_by_field_name("left")
                    .ok_or_else(|| unsupported(node, source))?;
                let right = node
                    .child_by_field_name("right")
                    .ok_or_else(|| unsupported(node, source))?;
                match operator {
                    Some("or") => Ok(Value::Bool(
                        self.boolean(left, source, seen, "or")?
                            | self.boolean(right, source, seen, "or")?,
                    )),
                    Some("and") => Ok(Value::Bool(
                        self.boolean(left, source, seen, "and")?
                            & self.boolean(right, source, seen, "and")?,
                    )),
                    _ => Err(unsupported(node, source)),
                }
            }
            "not_operator" => {
                let argument = node
                    .child_by_field_name("argument")
                    .ok_or_else(|| unsupported(node, source))?;
                Ok(Value::Bool(!self.boolean(argument, source, seen, "not")?))
            }
            "comparison_operator" => {
                let operands: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
                let mut operator_cursor = node.walk();
                let operators: Vec<&str> = node
                    .children_by_field_name("operators", &mut operator_cursor)
                    .map(|operator| &source[operator.byte_range()])
                    .collect();
                let [left, right] = operands.as_slice() else {
                    return Err(Error::Condition(format!(
                        "exactly 1 comparator required (inside `{source}`)"
                    )));
                };
                let [operator] = operators.as_slice() else {
                    return Err(Error::Condition(format!(
                        "exactly 1 operator required (inside `{source}`)"
                    )));
                };
                let left = self.expression(*left, source, false, seen)?;
                let right = self.expression(*right, source, *operator == "in", seen)?;
                Ok(Value::Bool(match *operator {
                    "==" => left == right,
                    "!=" => left != right,
                    "in" => match &right {
                        Value::List(items) => items.contains(&left),
                        Value::Str(text) => left.text().is_some_and(|piece| text.contains(piece)),
                        _ => false,
                    },
                    other => {
                        return Err(Error::Condition(format!(
                            "unexpected operator {other} (inside `{source}`)"
                        )));
                    }
                }))
            }
            "tuple" if allow_tuple => node
                .named_children(&mut cursor)
                .map(|child| self.expression(child, source, false, seen))
                .collect::<Result<Vec<_>>>()
                .map(Value::List),
            "string" | "concatenated_string" | "true" | "false" | "none" | "integer" => {
                eval(node, source)
            }
            _ => Err(Error::Condition(format!(
                "unexpected {} (inside `{source}`)",
                node.kind()
            ))),
        }
    }
}

fn format_with(variables: &BTreeMap<String, Value>, template: &str) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find(['{', '}']) {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if rest[start..].starts_with("{{") || rest[start..].starts_with("}}") {
            out.push_str(&rest[start..=start]);
            rest = &after[1..];
            continue;
        }
        let Some(end) = after.find('}').filter(|_| rest[start..].starts_with('{')) else {
            out.push_str(&rest[start..=start]);
            rest = after;
            continue;
        };
        let name = &after[..end];
        let value = variables
            .get(name)
            .and_then(Env::text)
            .ok_or_else(|| Error::UnknownVariable(name.to_owned()))?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// How a dependency is fetched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Source {
    Git { url: String, rev: Option<String> },
    Cipd { packages: Vec<String> },
    Gcs { objects: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolved {
    pub path: String,
    #[serde(flatten)]
    pub source: Source,
}

/// A hook that applies to the checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hook {
    pub name: String,
    pub action: Vec<String>,
}

/// Splits `url@revision`: the revision is what follows the last `@` when it cannot be part of an address (no `/` or `:`).
fn split_revision(formatted: &str) -> (String, Option<String>) {
    match formatted.rsplit_once('@') {
        Some((url, rev)) if !rev.is_empty() && !rev.contains('/') && !rev.contains(':') => {
            (url.to_owned(), Some(rev.to_owned()))
        }
        _ => (formatted.to_owned(), None),
    }
}

fn dependency(path: &str, value: &Value, env: &Env) -> Result<Option<Resolved>> {
    let fail = |what: &str| Error::Dependency {
        path: path.to_owned(),
        what: what.to_owned(),
    };
    if let Some(condition) = value.get("condition").and_then(Value::text)
        && !env.condition(&env.format(condition)?)?
    {
        return Ok(None);
    }
    let kind = value.get("dep_type").and_then(Value::text).unwrap_or("git");
    let source = match (kind, value) {
        ("git", Value::Str(raw)) => {
            let (url, rev) = split_revision(&env.format(raw)?);
            Source::Git { url, rev }
        }
        ("git", Value::Dict(_)) => {
            let raw = value
                .get("url")
                .and_then(Value::text)
                .ok_or_else(|| fail("url is not a string"))?;
            let (url, rev) = split_revision(&env.format(raw)?);
            Source::Git { url, rev }
        }
        ("cipd", Value::Dict(_)) => {
            let Some(Value::List(packages)) = value.get("packages") else {
                return Err(fail("a cipd dependency needs packages"));
            };
            let mut names = Vec::new();
            for package in packages {
                let name = package
                    .get("package")
                    .and_then(Value::text)
                    .ok_or_else(|| fail("a package has no name"))?;
                let version = match (
                    package.get("version").and_then(Value::text),
                    package.get("version_file").and_then(Value::text),
                ) {
                    (Some(version), _) => env.format(version)?,
                    (None, Some(file)) => format!("file:{}", env.format(file)?),
                    (None, None) => {
                        return Err(fail("a package has neither version nor version_file"));
                    }
                };
                names.push(format!("{}@{}", env.format(name)?, version));
            }
            Source::Cipd { packages: names }
        }
        ("gcs", Value::Dict(_)) => Source::Gcs {
            objects: match value.get("objects") {
                Some(Value::List(objects)) => objects.len(),
                _ => 0,
            },
        },
        _ => return Err(fail("unknown dep_type or shape")),
    };
    Ok(Some(Resolved {
        path: path.to_owned(),
        source,
    }))
}

impl Deps {
    /// The dependencies that apply to the checkout, in file order.
    pub fn resolve(&self, env: &Env) -> Result<Vec<Resolved>> {
        let mut out = Vec::new();
        for (path, value) in &self.deps {
            let path = env.format(path)?;
            if let Some(resolved) = dependency(&path, value, env)? {
                out.push(resolved);
            }
        }
        Ok(out)
    }

    /// The hooks that apply to the checkout, in file order.
    pub fn hooks(&self, env: &Env) -> Result<Vec<Hook>> {
        let mut out = Vec::new();
        for hook in &self.hooks {
            if let Some(condition) = hook.get("condition").and_then(Value::text)
                && !env.condition(&env.format(condition)?)?
            {
                continue;
            }
            let name = hook
                .get("name")
                .and_then(Value::text)
                .unwrap_or_default()
                .to_owned();
            let action = match hook.get("action") {
                Some(Value::List(items)) => items
                    .iter()
                    .filter_map(Value::text)
                    .map(|text| env.format(text))
                    .collect::<Result<Vec<_>>>()?,
                _ => Vec::new(),
            };
            out.push(Hook { name, action });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# A DEPS file in miniature.
gclient_gn_args_file = 'src/build/config/gclient_args.gni'
vars = {
  'chromium_git': 'https://chromium.googlesource.com',
  'foo_rev': 'abc123',
  'checkout_android': False,
  'checkout_flag': True,
}
deps = {
  'src/foo': Var('chromium_git') + '/foo.git' + '@' + Var('foo_rev'),
  'src/android_only': {
    'url': '{chromium_git}/android.git@deadbeef',
    'condition': 'checkout_android',
  },
  'src/not_win': {
    'url': 'https://example.com/x.git@1',
    'condition': 'checkout_linux and not checkout_win and host_os in ("linux", "mac")',
  },
  'src/tool': {
    'packages': [{'package': 'tools/x/${{platform}}', 'version': 'version:2@1.0'}],
    'condition': 'checkout_flag',
    'dep_type': 'cipd',
  },
  'src/no_rev': 'https://example.com/y.git',
}
hooks = [
  {'name': 'always', 'pattern': '.', 'action': ['python3', 'a.py', '--git={chromium_git}']},
  {'name': 'android', 'pattern': '.', 'condition': 'checkout_android', 'action': ['b']},
]
recursedeps = ['src/foo']
"#;

    fn env(deps: &Deps, os: &str) -> Env {
        Env::new(deps, "linux", "x64", os, "x64", &[]).unwrap()
    }

    #[test]
    fn parses_the_assignments_and_keeps_var_as_a_placeholder() {
        let deps = Deps::parse(SAMPLE).unwrap();
        assert_eq!(deps.vars.len(), 4);
        assert_eq!(deps.deps.len(), 5);
        assert_eq!(deps.recursedeps, ["src/foo"]);
        assert!(deps.other.contains_key("gclient_gn_args_file"));
        assert_eq!(
            deps.deps[0].1,
            Value::Str("{chromium_git}/foo.git@{foo_rev}".into())
        );
    }

    #[test]
    fn resolves_urls_revisions_conditions_and_cipd_packages_for_a_target() {
        let deps = Deps::parse(SAMPLE).unwrap();
        let linux = deps.resolve(&env(&deps, "linux")).unwrap();
        let paths: Vec<&str> = linux
            .iter()
            .map(|dependency| dependency.path.as_str())
            .collect();
        assert_eq!(paths, ["src/foo", "src/not_win", "src/tool", "src/no_rev"]);
        assert_eq!(
            linux[0].source,
            Source::Git {
                url: "https://chromium.googlesource.com/foo.git".into(),
                rev: Some("abc123".into())
            }
        );
        assert_eq!(
            linux[2].source,
            Source::Cipd {
                packages: vec!["tools/x/${platform}@version:2@1.0".into()]
            }
        );
        assert_eq!(
            linux[3].source,
            Source::Git {
                url: "https://example.com/y.git".into(),
                rev: None
            }
        );
        let android = deps.resolve(&env(&deps, "android")).unwrap();
        assert!(
            android
                .iter()
                .any(|dependency| dependency.path == "src/android_only")
        );
        assert!(
            !android
                .iter()
                .any(|dependency| dependency.path == "src/not_win")
        );
    }

    #[test]
    fn overrides_win_over_the_file_and_the_builtins() {
        let deps = Deps::parse(SAMPLE).unwrap();
        let env = Env::new(
            &deps,
            "linux",
            "x64",
            "linux",
            "x64",
            &[
                ("checkout_android".into(), Value::Bool(true)),
                ("foo_rev".into(), Value::Str("zzz".into())),
            ],
        )
        .unwrap();
        let resolved = deps.resolve(&env).unwrap();
        assert!(
            resolved
                .iter()
                .any(|dependency| dependency.path == "src/android_only")
        );
        assert_eq!(
            resolved[0].source,
            Source::Git {
                url: "https://chromium.googlesource.com/foo.git".into(),
                rev: Some("zzz".into())
            }
        );
    }

    #[test]
    fn hooks_follow_their_conditions() {
        let deps = Deps::parse(SAMPLE).unwrap();
        let hooks = deps.hooks(&env(&deps, "linux")).unwrap();
        assert_eq!(
            hooks,
            [Hook {
                name: "always".into(),
                action: vec![
                    "python3".into(),
                    "a.py".into(),
                    "--git=https://chromium.googlesource.com".into()
                ]
            }]
        );
        assert_eq!(deps.hooks(&env(&deps, "android")).unwrap().len(), 2);
    }

    #[test]
    fn conditions_follow_gclients_evaluator() {
        let deps = Deps::parse("vars = {'flag': True, 'name': 'x', 'alias': 'checkout_win and flag', 'loop': 'loop2', 'loop2': 'loop'}").unwrap();
        let env = Env::new(&deps, "linux", "x64", "win", "arm64", &[]).unwrap();
        for (expression, expected) in [
            ("checkout_win and checkout_arm64", true),
            ("checkout_win and checkout_x64", false),
            ("not checkout_mac or flag", true),
            ("(checkout_linux or checkout_win) and flag", true),
            ("name == 'x'", true),
            ("name != 'x'", false),
            ("host_os in ('win', 'linux')", true),
            ("host_cpu in ('arm64', 'x86')", false),
            ("False", false),
            ("not (flag and checkout_win)", false),
            ("alias", true),
            ("alias and not checkout_mac", true),
            ("never_declared == 'never_declared'", true),
        ] {
            assert_eq!(env.condition(expression), Ok(expected), "{expression}");
        }
    }

    #[test]
    fn conditions_reject_what_gclient_rejects() {
        let deps = Deps::parse("vars = {'name': 'x', 'loop': 'loop2', 'loop2': 'loop'}").unwrap();
        let env = Env::new(&deps, "linux", "x64", "linux", "x64", &[]).unwrap();
        for expression in [
            "name and checkout_linux",
            "not name",
            "host_os in ['linux']",
            "host_os not in ('mac',)",
            "1 < 2",
            "loop",
            "checkout_win if 1 else 2",
        ] {
            assert!(
                matches!(
                    env.condition(expression),
                    Err(Error::Condition(_) | Error::Unsupported { .. })
                ),
                "{expression}"
            );
        }
    }

    #[test]
    fn an_override_never_introduces_a_variable() {
        let deps = Deps::parse("vars = {'known': 'a'}").unwrap();
        let env = Env::new(
            &deps,
            "linux",
            "x64",
            "linux",
            "x64",
            &[
                ("known".into(), Value::Str("b".into())),
                ("unknown".into(), Value::Str("c".into())),
            ],
        )
        .unwrap();
        assert_eq!(env.format("{known}"), Ok("b".to_owned()));
        assert_eq!(
            env.format("{unknown}"),
            Err(Error::UnknownVariable("unknown".into()))
        );
    }

    #[test]
    fn errors_name_the_line_and_the_construct() {
        assert!(matches!(Deps::parse("vars = {"), Err(Error::Syntax(_))));
        assert!(matches!(
            Deps::parse("deps = {'a': open('x')}"),
            Err(Error::Unsupported { line: 1, .. })
        ));
        assert!(matches!(
            Deps::parse("import os"),
            Err(Error::Unsupported { .. })
        ));
        let deps = Deps::parse("deps = {'a': 'https://x/y.git@{nope}'}").unwrap();
        assert_eq!(
            deps.resolve(&Env::new(&deps, "linux", "x64", "linux", "x64", &[]).unwrap()),
            Err(Error::UnknownVariable("nope".into()))
        );
    }

    #[test]
    fn a_var_is_expanded_once_against_the_other_vars_and_a_string_var_can_be_a_condition() {
        let deps = Deps::parse(
            "vars = {'boards': '', 'simple': '\"{boards}\" != \"\"', 'with_boards': '{boards}x'}",
        )
        .unwrap();
        let env = Env::new(&deps, "linux", "x64", "linux", "x64", &[]).unwrap();
        assert_eq!(env.condition("simple"), Ok(false));
        let env = Env::new(
            &deps,
            "linux",
            "x64",
            "linux",
            "x64",
            &[("boards".into(), Value::Str("amd64".into()))],
        )
        .unwrap();
        // The override reaches the strings of the other vars, but not the declared value of `boards` itself.
        assert_eq!(env.format("{with_boards}"), Ok("amd64x".to_owned()));
        assert_eq!(env.condition("simple"), Ok(true));
    }

    #[test]
    fn a_cipd_package_may_name_a_version_file() {
        let deps = Deps::parse("deps = {'a': {'dep_type': 'cipd', 'packages': [{'package': 'p/x', 'version_file': 'v.txt'}]}}").unwrap();
        let resolved = deps
            .resolve(&Env::new(&deps, "linux", "x64", "linux", "x64", &[]).unwrap())
            .unwrap();
        assert_eq!(
            resolved[0].source,
            Source::Cipd {
                packages: vec!["p/x@file:v.txt".into()]
            }
        );
    }

    #[test]
    fn an_address_with_an_at_sign_is_not_split_into_a_revision() {
        assert_eq!(
            split_revision("git@github.com:o/r.git"),
            ("git@github.com:o/r.git".to_owned(), None)
        );
        assert_eq!(
            split_revision("https://h/r.git@abc"),
            ("https://h/r.git".to_owned(), Some("abc".to_owned()))
        );
    }

    #[test]
    fn braces_are_escaped_by_doubling_and_a_string_is_expanded_once() {
        let deps = Deps::parse("vars = {'a': '1', 'b': '{a}'}").unwrap();
        let env = Env::new(&deps, "linux", "x64", "linux", "x64", &[]).unwrap();
        assert_eq!(
            env.format("${{platform}}-{a}"),
            Ok("${platform}-1".to_owned())
        );
        // A var's own placeholders were expanded once when the variables were built, so {b} is already "1"; a string is never expanded twice.
        assert_eq!(env.format("{b}"), Ok("1".to_owned()));
        assert_eq!(env.format("{{b}}"), Ok("{b}".to_owned()));
        assert_eq!(env.format("a } b { c"), Ok("a } b { c".to_owned()));
    }

    #[test]
    fn python_string_literals_are_decoded() {
        let deps = Deps::parse(r#"vars = {'a': 'x\ny', 'b': r'x\ny', 'c': "q\"q", 'd': 'p' 'q'}"#)
            .unwrap();
        let get = |name: &str| {
            deps.vars
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(get("a"), Some(Value::Str("x\ny".into())));
        assert_eq!(get("b"), Some(Value::Str("x\\ny".into())));
        assert_eq!(get("c"), Some(Value::Str("q\"q".into())));
        assert_eq!(get("d"), Some(Value::Str("pq".into())));
    }
}
