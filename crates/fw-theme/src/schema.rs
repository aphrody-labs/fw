//! The JSON Schema (draft 2020-12) of `settings.json` and of `theme.toml`.
//!
//! Other crates describe their sections with [`SchemaFragment`]s that [`settings_schema`] merges under `properties`.
//! Extension keywords for generated UIs: `x-icon` (Material Symbol name), `x-group` (section id), `x-unit`,
//! `x-restart-required` (bool) and `x-secret` (bool: masked, never written to settings.json).

use crate::error::{Error, Result};
use crate::settings::Settings;
use crate::theme::ThemeFile;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub const X_ICON: &str = "x-icon";
pub const X_GROUP: &str = "x-group";
pub const X_UNIT: &str = "x-unit";
pub const X_RESTART_REQUIRED: &str = "x-restart-required";
pub const X_SECRET: &str = "x-secret";

/// The schema of one setting or section, contributed by a component.
#[derive(Debug, Clone)]
pub struct SchemaFragment {
    /// Dotted settings path, for example `terminal.font_size` or `uutils.ls`.
    pub path: String,
    pub schema: Value,
}

fn to_value(schema: &schemars::Schema) -> Result<Value> {
    serde_json::to_value(schema).map_err(|source| Error::Json { path: PathBuf::from("<schema>"), source })
}

fn properties_of(node: &mut Value) -> Result<&mut Map<String, Value>> {
    let object = node.as_object_mut().ok_or_else(|| Error::Invalid("schema node is not an object".to_owned()))?;
    object.entry("type").or_insert_with(|| json!("object"));
    object
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("schema `properties` is not an object".to_owned()))
}

/// The settings schema with `fragments` merged in. A fragment's `$defs` move to the root so `$ref`s keep resolving.
pub fn settings_schema(fragments: &[SchemaFragment]) -> Result<Value> {
    let mut root = to_value(&schemars::schema_for!(Settings))?;
    if let Some(object) = root.as_object_mut() {
        object.insert("title".to_owned(), json!("fw settings"));
    }
    for fragment in fragments {
        let mut schema = fragment.schema.clone();
        let defs = schema.as_object_mut().and_then(|object| object.remove("$defs"));
        if let Some(Value::Object(defs)) = defs {
            let root_defs = root
                .as_object_mut()
                .ok_or_else(|| Error::Invalid("settings schema is not an object".to_owned()))?
                .entry("$defs")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(target) = root_defs.as_object_mut() {
                target.extend(defs);
            }
        }
        let segments: Vec<&str> = fragment.path.split('.').filter(|segment| !segment.is_empty()).collect();
        let Some((last, parents)) = segments.split_last() else {
            return Err(Error::Invalid("schema fragment with an empty path".to_owned()));
        };
        let mut node = &mut root;
        for segment in parents {
            node = properties_of(node)?.entry((*segment).to_owned()).or_insert_with(|| json!({ "type": "object", "properties": {} }));
        }
        let properties = properties_of(node)?;
        if properties.contains_key(*last) {
            return Err(Error::Invalid(format!("schema fragment `{}` is already defined", fragment.path)));
        }
        properties.insert((*last).to_owned(), schema);
    }
    Ok(root)
}

pub fn theme_schema() -> Result<Value> {
    let mut schema = to_value(&schemars::schema_for!(ThemeFile))?;
    if let Some(object) = schema.as_object_mut() {
        object.insert("title".to_owned(), json!("fw theme.toml"));
    }
    Ok(schema)
}

pub const SETTINGS_FILE: &str = "settings.schema.json";
pub const THEME_FILE: &str = "theme.schema.json";

fn pretty(value: &Value) -> Result<String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|source| Error::Json { path: PathBuf::from("<schema>"), source })?;
    text.push('\n');
    Ok(text)
}

/// The schema files with the registered fragments applied, as `(file name, text)`.
pub fn files(fragments: &[SchemaFragment]) -> Result<Vec<(&'static str, String)>> {
    Ok(vec![(SETTINGS_FILE, pretty(&settings_schema(fragments)?)?), (THEME_FILE, pretty(&theme_schema()?)?)])
}

/// Writes the schema files into `dir`.
pub fn write(dir: &Path, fragments: &[SchemaFragment]) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).map_err(|source| Error::io(dir, source))?;
    let mut written = Vec::new();
    for (name, text) in files(fragments)? {
        let path = dir.join(name);
        crate::targets::write_atomic(&path, &text)?;
        written.push(path);
    }
    Ok(written)
}

/// Names of the schema files in `dir` that differ from the generated ones.
pub fn stale(dir: &Path, fragments: &[SchemaFragment]) -> Result<Vec<&'static str>> {
    let mut stale = Vec::new();
    for (name, text) in files(fragments)? {
        if std::fs::read_to_string(dir.join(name)).ok().as_deref() != Some(text.as_str()) {
            stale.push(name);
        }
    }
    Ok(stale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_schema_is_2020_12_with_extension_keywords() -> Result<()> {
        let schema = settings_schema(&[])?;
        assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(schema["properties"]["theme"][X_GROUP], "appearance");
        assert_eq!(schema["properties"]["locale"][X_ICON], "language");
        Ok(())
    }

    #[test]
    fn fragments_merge_at_dotted_paths_and_hoist_defs() -> Result<()> {
        let fragments = vec![
            SchemaFragment { path: "terminal.font_size".to_owned(), schema: json!({ "type": "number", "x-unit": "pt" }) },
            SchemaFragment { path: "uutils.ls".to_owned(), schema: json!({ "$ref": "#/$defs/Ls", "$defs": { "Ls": { "type": "object" } } }) },
        ];
        let schema = settings_schema(&fragments)?;
        assert_eq!(schema["properties"]["terminal"]["properties"]["font_size"][X_UNIT], "pt");
        assert_eq!(schema["properties"]["uutils"]["properties"]["ls"]["$ref"], "#/$defs/Ls");
        assert_eq!(schema["$defs"]["Ls"]["type"], "object");
        Ok(())
    }

    #[test]
    fn a_fragment_cannot_redefine_a_builtin_setting() {
        let clash = SchemaFragment { path: "locale".to_owned(), schema: json!({ "type": "string" }) };
        assert!(settings_schema(&[clash]).is_err());
    }

    #[test]
    fn the_theme_schema_lists_the_colors_map() -> Result<()> {
        assert!(theme_schema()?["properties"]["colors"].is_object());
        Ok(())
    }
}
