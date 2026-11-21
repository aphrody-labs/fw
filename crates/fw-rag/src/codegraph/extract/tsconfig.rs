// SPDX-License-Identifier: Apache-2.0
//! `tsconfig.json` module resolution settings: `baseUrl` and `paths` (with `extends`).
//!
//! Only what an import specifier needs is read. The file is JSON with comments and trailing
//! commas, so it is cleaned before parsing.

use serde_json::Value;

/// `a/b` + `../c/d` gives `a/c/d`; `None` when it escapes the root.
pub(super) fn join_relative(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// One `paths` entry: a specifier pattern and its root-relative targets (both may hold one `*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TsPath {
    pub pattern: String,
    pub targets: Vec<String>,
}

/// Module resolution settings of one `tsconfig.json`, relative to the corpus root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TsConfig {
    /// Directory of the config: it applies to the files below it.
    pub dir: String,
    /// `baseUrl` as a root-relative directory.
    pub base_url: Option<String>,
    /// `paths`, targets already joined to their base.
    pub paths: Vec<TsPath>,
}

/// Remove `//` and `/* */` comments and trailing commas, leaving string contents alone.
#[must_use]
pub fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}' | ']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// The `extends` target of a config as a root-relative file, when it is a relative path.
#[must_use]
pub fn extends_of(dir: &str, jsonc: &str) -> Option<String> {
    let value: Value = serde_json::from_str(&strip_jsonc(jsonc)).ok()?;
    let target = value.get("extends")?.as_str()?;
    if !(target.starts_with("./") || target.starts_with("../")) {
        return None;
    }
    let joined = join_relative(dir, target)?;
    Some(if std::path::Path::new(&joined)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        joined
    } else {
        format!("{joined}.json")
    })
}

impl TsConfig {
    /// Parse the config at `dir`, overriding what `inherited` (its `extends` chain) sets.
    #[must_use]
    pub fn parse(dir: &str, jsonc: &str, inherited: Option<&Self>) -> Option<Self> {
        let value: Value = serde_json::from_str(&strip_jsonc(jsonc)).ok()?;
        let options = value.get("compilerOptions");
        let base_url = match options.and_then(|o| o.get("baseUrl")).and_then(Value::as_str) {
            Some(base) => join_relative(dir, base),
            None => inherited.and_then(|i| i.base_url.clone()),
        };
        let paths = match options.and_then(|o| o.get("paths")).and_then(Value::as_object) {
            Some(map) => {
                let base = base_url.clone().unwrap_or_else(|| dir.to_owned());
                map.iter()
                    .filter_map(|(pattern, targets)| {
                        let targets: Vec<String> = targets
                            .as_array()?
                            .iter()
                            .filter_map(Value::as_str)
                            .filter_map(|t| join_relative(&base, t))
                            .collect();
                        Some(TsPath {
                            pattern: pattern.clone(),
                            targets,
                        })
                    })
                    .collect()
            }
            None => inherited.map(|i| i.paths.clone()).unwrap_or_default(),
        };
        Some(Self {
            dir: dir.to_owned(),
            base_url,
            paths,
        })
    }

    /// Root-relative base paths an import specifier may name, best match first: the targets of
    /// the longest matching `paths` pattern, then `baseUrl` joined with the specifier.
    #[must_use]
    pub fn candidates(&self, spec: &str) -> Vec<String> {
        let mut best: Option<(usize, &TsPath, &str)> = None;
        for entry in &self.paths {
            let captured = match entry.pattern.split_once('*') {
                None => (entry.pattern == spec).then_some(""),
                Some((prefix, suffix)) => spec
                    .strip_prefix(prefix)
                    .and_then(|rest| rest.strip_suffix(suffix))
                    .filter(|_| spec.len() >= prefix.len() + suffix.len()),
            };
            if let Some(captured) = captured
                && best.is_none_or(|(len, ..)| entry.pattern.len() > len)
            {
                best = Some((entry.pattern.len(), entry, captured));
            }
        }
        let mut out: Vec<String> = best
            .into_iter()
            .flat_map(|(_, entry, captured)| {
                entry
                    .targets
                    .iter()
                    .map(move |t| t.replacen('*', captured, 1))
            })
            .collect();
        if let Some(base) = &self.base_url
            && let Some(joined) = join_relative(base, spec)
        {
            out.push(joined);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{
        // comment with "quotes" and a , comma
        "extends": "./base",
        "compilerOptions": {
            "baseUrl": "src", /* block */
            "paths": { "@app/*": ["app/*", "legacy/*"], "~utils": ["lib/utils/index.ts"], },
        },
    }"#;

    #[test]
    fn jsonc_comments_and_trailing_commas_are_removed() {
        let value: Value = serde_json::from_str(&strip_jsonc(CONFIG)).unwrap();
        assert_eq!(value["compilerOptions"]["baseUrl"], "src");
        assert_eq!(extends_of("web", CONFIG).as_deref(), Some("web/base.json"));
        assert_eq!(extends_of("web", r#"{"extends":"@tsconfig/node"}"#), None);
    }

    #[test]
    fn paths_are_relative_to_base_url_and_the_longest_pattern_wins() {
        let config = TsConfig::parse("web", CONFIG, None).unwrap();
        assert_eq!(config.base_url.as_deref(), Some("web/src"));
        assert_eq!(config.candidates("@app/ui/button"), ["web/src/app/ui/button", "web/src/legacy/ui/button", "web/src/@app/ui/button"]);
        assert_eq!(config.candidates("~utils"), ["web/src/lib/utils/index.ts", "web/src/~utils"]);
        assert_eq!(config.candidates("plain"), ["web/src/plain"]);
    }

    #[test]
    fn paths_without_base_url_are_relative_to_the_config_and_inheritance_fills_gaps() {
        let parent = TsConfig::parse("", r##"{"compilerOptions":{"paths":{"#/*":["./src/*"]}}}"##, None).unwrap();
        assert_eq!(parent.candidates("#/a"), ["src/a"]);
        let child = TsConfig::parse("pkg", r#"{"extends":"../tsconfig.json"}"#, Some(&parent)).unwrap();
        assert_eq!(child.candidates("#/b"), ["src/b"]);
    }
}
