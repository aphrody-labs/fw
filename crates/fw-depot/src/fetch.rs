//! `fetch`: write the `.gclient` of a named project (`chromium`, `android`, `v8`, `webrtc`, ...) and sync it. The table of projects is the one of
//! depot_tools' `fetch_configs/` (`fetch_configs.py` here is generated from it), and the `.gclient` text is byte for byte what `fetch.py` hands to
//! `gclient config --spec`.

use crate::deps::{Deps, Value};
use crate::error::{Error, Result};

const TABLE: &str = include_str!("fetch_configs.py");

/// What a project's `.gclient` holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub solutions: Vec<Vec<(String, Value)>>,
    pub extra: Vec<(String, Value)>,
    pub with_branch_heads: bool,
    /// The directory `fetch` enters after the sync.
    pub root: String,
}

fn table() -> Result<Vec<(String, Value)>> {
    let parsed = Deps::parse(TABLE)?;
    match parsed.other.get("configs") {
        Some(Value::Dict(items)) => Ok(items.clone()),
        _ => Err(Error::message("fetch_configs.py has no configs table")),
    }
}

/// Names of every project `fetch` knows.
pub fn names() -> Result<Vec<String>> {
    Ok(table()?.into_iter().map(|(name, _)| name).collect())
}

fn property(properties: &[(String, String)], name: &str) -> Option<String> {
    properties
        .iter()
        .rev()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

/// The spec of project `name` for the given `--property=value` pairs; aliases are followed.
pub fn resolve(name: &str, properties: &[(String, String)]) -> Result<Spec> {
    let table = table()?;
    let entry = table
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| Error::message(format!("fetch: no config named {name}")))?;
    let spec = entry
        .get("spec")
        .ok_or_else(|| Error::message(format!("fetch: {name} has no spec")))?;
    let root = entry.get("root").and_then(Value::text).unwrap_or_default();
    if let Some(alias) = spec.get("alias") {
        let target = alias
            .get("config")
            .and_then(Value::text)
            .ok_or_else(|| Error::message(format!("fetch: alias {name} has no config")))?;
        let mut all = Vec::new();
        if let Some(Value::List(items)) = alias.get("props") {
            for item in items {
                if let Some((key, value)) = item.text().and_then(|text| text.strip_prefix("--")).and_then(|text| text.split_once('=')) {
                    all.push((key.to_owned(), value.to_owned()));
                }
            }
        }
        all.extend(properties.iter().cloned());
        let mut resolved = resolve(target, &all)?;
        if !root.is_empty() {
            resolved.root = root.to_owned();
        }
        return Ok(resolved);
    }
    let inner = spec
        .get("gclient_git_spec")
        .ok_or_else(|| Error::message(format!("fetch: {name} is not a gclient_git project")))?;
    let Some(Value::List(solutions)) = inner.get("solutions") else {
        return Err(Error::message(format!("fetch: {name} has no solutions")));
    };
    let mut solutions: Vec<Vec<(String, Value)>> = solutions
        .iter()
        .filter_map(|solution| match solution {
            Value::Dict(items) => Some(items.clone()),
            _ => None,
        })
        .collect();
    let mut extra = Vec::new();
    if name == "chromium" {
        if let Some(solution) = solutions.first_mut()
            && let Some((_, Value::Dict(vars))) = solution.iter_mut().find(|(key, _)| key == "custom_vars")
        {
            if property(properties, "webkit_revision").as_deref() == Some("ToT") {
                vars.push(("webkit_revision".to_owned(), Value::Str(String::new())));
            }
            if property(properties, "internal").as_deref() == Some("True") {
                vars.push(("checkout_src_internal".to_owned(), Value::Bool(true)));
            }
        }
        if let Some(os) = property(properties, "target_os") {
            extra.push((
                "target_os".to_owned(),
                Value::List(os.split(',').map(|item| Value::Str(item.to_owned())).collect()),
            ));
        }
        if let Some(only) = property(properties, "target_os_only") {
            extra.push(("target_os_only".to_owned(), Value::Str(only)));
        }
    }
    if name == "webrtc" {
        if let Some(os) = property(properties, "target_os") {
            extra.push((
                "target_os".to_owned(),
                Value::List(os.split(',').map(|item| Value::Str(item.to_owned())).collect()),
            ));
        }
        if let Some(directory) = property(properties, "cache_dir") {
            extra.push(("cache_dir".to_owned(), Value::Str(directory)));
        }
    }
    Ok(Spec {
        solutions,
        extra,
        with_branch_heads: inner.get("with_branch_heads").is_some_and(Value::truthy),
        root: root.to_owned(),
    })
}

/// Python's `%r` for what a spec holds.
fn literal(value: &Value, in_list: bool) -> String {
    match value {
        Value::Str(text) if in_list => format!("\"{text}\""),
        Value::Str(text) => format!("\"{text}\""),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Int(number) => number.to_string(),
        Value::None => "None".to_owned(),
        Value::List(items) => format!(
            "[{}]",
            items.iter().map(|item| literal(item, true)).collect::<Vec<_>>().join(", ")
        ),
        Value::Dict(items) => {
            format!(
                "{{{}}}",
                items
                    .iter()
                    .map(|(key, item)| format!("{}: {}", repr_string(key), repr(item)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

fn repr_string(text: &str) -> String {
    if text.contains('\'') && !text.contains('"') {
        format!("\"{text}\"")
    } else {
        format!("'{}'", text.replace('\'', "\\'"))
    }
}

fn repr(value: &Value) -> String {
    match value {
        Value::Str(text) => repr_string(text),
        Value::List(items) => format!("[{}]", items.iter().map(repr).collect::<Vec<_>>().join(", ")),
        other => literal(other, false),
    }
}

/// The text `fetch` passes to `gclient config --spec`.
pub fn format_spec(spec: &Spec) -> String {
    let solutions: Vec<String> = spec
        .solutions
        .iter()
        .map(|solution| {
            let lines: Vec<String> = solution
                .iter()
                .map(|(key, value)| format!("    \"{key}\": {},", literal(value, false)))
                .collect();
            format!("  {{\n{}\n  }},", lines.join("\n"))
        })
        .collect();
    let mut text = format!("solutions = [\n{}\n]\n", solutions.join("\n"));
    for key in ["target_os", "target_os_only", "cache_dir", "cache_mode"] {
        if let Some((_, value)) = spec.extra.iter().find(|(name, _)| name == key) {
            text.push_str(&format!("{key} = {}\n", literal(value, false)));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_android_internal_matches_fetch_py() {
        let spec = resolve(
            "chromium",
            &[
                ("target_os".into(), "android".into()),
                ("internal".into(), "True".into()),
            ],
        )
        .unwrap();
        assert_eq!(
            format_spec(&spec),
            "solutions = [\n  {\n    \"name\": \"src\",\n    \"url\": \"https://chromium.googlesource.com/chromium/src.git\",\n    \"custom_deps\": {},\n    \"custom_vars\": {'checkout_src_internal': True},\n  },\n]\ntarget_os = [\"android\"]\n"
        );
        assert_eq!(spec.root, "src");
    }

    #[test]
    fn aliases_follow_to_their_target_with_their_properties() {
        let spec = resolve("webrtc_android", &[]).unwrap();
        assert!(format_spec(&spec).ends_with("target_os = [\"android\", \"unix\"]\n"));
        assert_eq!(spec.root, "src");
        assert!(spec.with_branch_heads);
    }
}
