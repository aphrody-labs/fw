//! `gclient getdep` and `gclient setdep`: read and change a revision or a variable of a DEPS file in place. The edit replaces one string literal
//! of the source text, so comments, ordering and quoting are left as they are.

use tree_sitter::{Node, Parser};

use crate::deps::{Deps, Env, Platform, Value};
use crate::error::{Error, Result};

fn parse(text: &str) -> Result<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|error| Error::message(error.to_string()))?;
    parser
        .parse(text, None)
        .ok_or_else(|| Error::message("DEPS does not parse"))
}

fn literal_text(node: Node<'_>, source: &str) -> Option<String> {
    (node.kind() == "string").then(|| {
        let raw = &source[node.byte_range()];
        raw.trim_matches(|ch| ch == '\'' || ch == '"').to_owned()
    })
}

/// The value node of `key` in the dictionary assigned to top-level `name`.
fn dict_entry<'tree>(root: Node<'tree>, source: &str, name: &str, key: &str) -> Option<Node<'tree>> {
    let mut cursor = root.walk();
    for statement in root.named_children(&mut cursor) {
        let Some(assignment) = statement
            .named_child(0)
            .filter(|child| child.kind() == "assignment")
        else {
            continue;
        };
        let left = assignment.child_by_field_name("left")?;
        if &source[left.byte_range()] != name {
            continue;
        }
        let dictionary = assignment.child_by_field_name("right")?;
        let mut pairs = dictionary.walk();
        for pair in dictionary.named_children(&mut pairs) {
            let Some(pair_key) = pair.child_by_field_name("key") else {
                continue;
            };
            if literal_text(pair_key, source).as_deref() == Some(key) {
                return pair.child_by_field_name("value");
            }
        }
    }
    None
}

fn pair_value<'tree>(dictionary: Node<'tree>, source: &str, key: &str) -> Option<Node<'tree>> {
    let mut cursor = dictionary.walk();
    dictionary.named_children(&mut cursor).find_map(|pair| {
        let pair_key = pair.child_by_field_name("key")?;
        if literal_text(pair_key, source).as_deref() == Some(key) {
            pair.child_by_field_name("value")
        } else {
            None
        }
    })
}

fn replace(source: &str, node: Node<'_>, replacement: &str) -> String {
    let range = node.byte_range();
    format!(
        "{}{}{}",
        &source[..range.start],
        replacement,
        &source[range.end..]
    )
}

fn quoted(original: &str, value: &str) -> String {
    let quote = if original.starts_with('"') { '"' } else { '\'' };
    format!("{quote}{value}{quote}")
}

pub fn get_var(text: &str, name: &str) -> Result<Option<String>> {
    let deps = Deps::parse(text)?;
    Ok(deps
        .vars
        .iter()
        .find(|(key, _)| key == name)
        .and_then(|(_, value)| Env::text(value)))
}

pub fn set_var(text: &str, name: &str, value: &str) -> Result<String> {
    let tree = parse(text)?;
    let node = dict_entry(tree.root_node(), text, "vars", name)
        .ok_or_else(|| Error::message(format!("Could not find any variable called {name}.")))?;
    let replacement = match node.kind() {
        "true" | "false" => match value {
            "True" | "False" => value.to_owned(),
            other => return Err(Error::message(format!("{name} is a boolean, not {other}"))),
        },
        "string" => quoted(&text[node.byte_range()], value),
        other => {
            return Err(Error::message(format!(
                "Variable {name} is a {other} expression, not a literal"
            )));
        }
    };
    Ok(replace(text, node, &replacement))
}

fn operands<'tree>(node: Node<'tree>, out: &mut Vec<Node<'tree>>) {
    if node.kind() == "binary_operator"
        && let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        )
    {
        operands(left, out);
        operands(right, out);
        return;
    }
    out.push(node);
}

/// The revision of a Git dependency (what follows the last `@` of its url), or the version of the first CIPD package of a CIPD dependency.
pub fn get_revision(text: &str, path: &str) -> Result<Option<String>> {
    let deps = Deps::parse(text)?;
    let env = Env::build(&deps, &Platform::detect(), None, &[])?;
    let Some((_, value)) = deps.deps.iter().find(|(key, _)| key == path) else {
        return Ok(None);
    };
    let raw = match value {
        Value::Str(raw) => Some(raw.as_str()),
        Value::Dict(_) => match value.get("dep_type").and_then(Value::text) {
            Some("cipd") => {
                let version = match value.get("packages") {
                    Some(Value::List(packages)) => packages
                        .first()
                        .and_then(|package| package.get("version"))
                        .and_then(Value::text),
                    _ => None,
                };
                return Ok(version.map(|version| env.format(version)).transpose()?);
            }
            _ => value.get("url").and_then(Value::text),
        },
        _ => None,
    };
    let Some(raw) = raw else { return Ok(None) };
    let formatted = env.format(raw)?;
    Ok(formatted
        .rsplit_once('@')
        .map(|(_, revision)| revision.to_owned()))
}

/// Sets the revision of a Git dependency. When the url ends in `Var('x')`, the variable `x` is set.
pub fn set_revision(text: &str, path: &str, revision: &str) -> Result<String> {
    let tree = parse(text)?;
    let node = dict_entry(tree.root_node(), text, "deps", path)
        .ok_or_else(|| Error::message(format!("Could not find any dependency called {path}.")))?;
    let node = if node.kind() == "dictionary" {
        pair_value(node, text, "url").ok_or_else(|| Error::message(format!("{path} has no url")))?
    } else {
        node
    };
    let mut parts = Vec::new();
    operands(node, &mut parts);
    let Some(last) = parts.last().copied() else {
        return Err(Error::message(format!("{path}: empty url")));
    };
    if last.kind() == "call" {
        let function = last
            .child_by_field_name("function")
            .map(|node| &text[node.byte_range()]);
        let argument = last
            .child_by_field_name("arguments")
            .and_then(|list| list.named_child(0))
            .and_then(|argument| literal_text(argument, text));
        if let (Some("Var"), Some(name)) = (function, argument) {
            return set_var(text, &name, revision);
        }
    }
    let Some(literal) = literal_text(last, text) else {
        return Err(Error::message(format!(
            "{path}: the url does not end in a literal or Var()"
        )));
    };
    let previous_is_at = parts
        .len()
        .checked_sub(2)
        .and_then(|index| parts.get(index))
        .and_then(|node| literal_text(*node, text))
        .is_some_and(|text| text.ends_with('@'));
    let updated = if let Some((base, _)) = literal.rsplit_once('@') {
        format!("{base}@{revision}")
    } else if previous_is_at {
        revision.to_owned()
    } else {
        return Err(Error::message(format!("{path}: the url has no revision")));
    };
    Ok(replace(
        text,
        last,
        &quoted(&text[last.byte_range()], &updated),
    ))
}

/// Sets the version of the CIPD package `package` of the dependency at `path`.
pub fn set_cipd(text: &str, path: &str, package: &str, version: &str) -> Result<String> {
    let tree = parse(text)?;
    let node = dict_entry(tree.root_node(), text, "deps", path)
        .ok_or_else(|| Error::message(format!("Could not find any dependency called {path}.")))?;
    let packages = pair_value(node, text, "packages")
        .ok_or_else(|| Error::message(format!("{path} is not a cipd dependency")))?;
    let mut cursor = packages.walk();
    for entry in packages.named_children(&mut cursor) {
        if entry.kind() != "dictionary" {
            continue;
        }
        let name = pair_value(entry, text, "package").map(|value| {
            let mut leaves = Vec::new();
            operands(value, &mut leaves);
            leaves
                .iter()
                .map(|leaf| {
                    literal_text(*leaf, text).unwrap_or_else(|| text[leaf.byte_range()].to_owned())
                })
                .collect::<String>()
        });
        if !name.is_some_and(|name| name.contains(package)) {
            continue;
        }
        let target = pair_value(entry, text, "version")
            .ok_or_else(|| Error::message(format!("{package} has no literal version")))?;
        if target.kind() == "call" {
            let variable = target
                .child_by_field_name("arguments")
                .and_then(|list| list.named_child(0))
                .and_then(|argument| literal_text(argument, text));
            if let Some(variable) = variable {
                return set_var(text, &variable, version);
            }
        }
        return Ok(replace(
            text,
            target,
            &quoted(&text[target.byte_range()], version),
        ));
    }
    Err(Error::message(format!("{path} has no package {package}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "vars = {\n  'chromium_git': 'https://c.googlesource.com',\n  'foo_rev': 'aaa',  # keep\n  'flag': False,\n}\ndeps = {\n  'src/foo': Var('chromium_git') + '/foo.git' + '@' + Var('foo_rev'),\n  'src/bar': 'https://x/bar.git@bbb',\n  'src/baz': {'url': Var('chromium_git') + '/baz.git@' + 'ccc', 'condition': 'flag'},\n  'src/tool': {'packages': [{'package': 'tools/x/${{platform}}', 'version': 'version:1'}], 'dep_type': 'cipd'},\n}\n";

    #[test]
    fn reads_and_sets_variables_revisions_and_versions_in_place() {
        assert_eq!(get_revision(SAMPLE, "src/foo").unwrap().as_deref(), Some("aaa"));
        assert_eq!(get_revision(SAMPLE, "src/bar").unwrap().as_deref(), Some("bbb"));
        let edited = set_revision(SAMPLE, "src/foo", "111").unwrap();
        assert!(edited.contains("'foo_rev': '111',  # keep"));
        let edited = set_revision(&edited, "src/bar", "222").unwrap();
        assert!(edited.contains("'https://x/bar.git@222'"));
        let edited = set_revision(&edited, "src/baz", "333").unwrap();
        assert!(edited.contains("'/baz.git@' + '333'") || edited.contains("'/baz.git@333'"));
        let edited = set_var(&edited, "flag", "True").unwrap();
        assert!(edited.contains("'flag': True,"));
        let edited = set_cipd(&edited, "src/tool", "tools/x", "version:2").unwrap();
        assert!(edited.contains("'version': 'version:2'"));
        assert_eq!(get_var(&edited, "foo_rev").unwrap().as_deref(), Some("111"));
    }
}
