//! Reads a wrangler config (`wrangler.jsonc`, `wrangler.json` or `wrangler.toml`, optionally one `env`) into the bindings workerd has to provide.

use crate::{read_text, Error, Result};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct R2Binding {
    pub binding: String,
    pub bucket: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1Binding {
    pub binding: String,
    pub database: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvBinding {
    pub binding: String,
    pub namespace: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueBinding {
    pub binding: String,
    pub queue: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableObjectBinding {
    pub binding: String,
    pub class_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceBinding {
    pub binding: String,
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueConsumer {
    pub queue: String,
    /// Settings as workerd's Queues Worker names them (`maxBatchSize`, `maxRetries`, ...), already translated from wrangler's snake case.
    pub settings: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assets {
    pub directory: String,
    pub binding: Option<String>,
    pub config: serde_json::Map<String, Value>,
}

/// A binding kind workerd cannot provide on its own; compile reports it instead of dropping it silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub kind: String,
    pub binding: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub main: String,
    pub compatibility_date: String,
    pub compatibility_flags: Vec<String>,
    pub text_vars: Vec<(String, String)>,
    pub json_vars: Vec<(String, String)>,
    pub r2: Vec<R2Binding>,
    pub d1: Vec<D1Binding>,
    pub kv: Vec<KvBinding>,
    pub queues: Vec<QueueBinding>,
    pub queue_consumers: Vec<QueueConsumer>,
    pub assets: Option<Assets>,
    pub durable_objects: Vec<DurableObjectBinding>,
    pub sqlite_classes: Vec<String>,
    pub services: Vec<ServiceBinding>,
    pub routes: Vec<String>,
    pub crons: Vec<String>,
    pub unsupported: Vec<Unsupported>,
}

/// Removes `//` and `/* */` comments and trailing commas outside of strings.
pub fn strip_jsonc(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    let mut in_string = false;
    while index < chars.len() {
        let current = chars[index];
        if in_string {
            output.push(current);
            if current == '\\' && index + 1 < chars.len() {
                output.push(chars[index + 1]);
                index += 1;
            } else if current == '"' {
                in_string = false;
            }
        } else if current == '"' {
            in_string = true;
            output.push(current);
        } else if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        } else if current == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            index += 2;
            continue;
        } else {
            output.push(current);
        }
        index += 1;
    }
    remove_trailing_commas(&output)
}

fn remove_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut in_string = false;
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        if in_string {
            output.push(current);
            if current == '\\' && index + 1 < chars.len() {
                output.push(chars[index + 1]);
                index += 1;
            } else if current == '"' {
                in_string = false;
            }
        } else if current == '"' {
            in_string = true;
            output.push(current);
        } else if current == ',' {
            let next = chars[index + 1..].iter().find(|candidate| !candidate.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                output.push(current);
            }
        } else {
            output.push(current);
        }
        index += 1;
    }
    output
}

pub fn load(path: &Path, environment: Option<&str>) -> Result<Service> {
    let text = read_text(path)?;
    let value: Value = if path.extension().is_some_and(|extension| extension == "toml") {
        let table: toml::Table = toml::from_str(&text).map_err(|error| Error::Parse { path: path.display().to_string(), message: error.to_string() })?;
        serde_json::to_value(table).map_err(|error| Error::Parse { path: path.display().to_string(), message: error.to_string() })?
    } else {
        serde_json::from_str(&strip_jsonc(&text)).map_err(|error| Error::Parse { path: path.display().to_string(), message: error.to_string() })?
    };
    from_value(&value, environment)
}

/// Keys wrangler lets an environment inherit; every other key (bindings, vars, routes) must be repeated in the environment.
const INHERITED: [&str; 5] = ["main", "compatibility_date", "compatibility_flags", "limits", "observability"];

pub fn from_value(root: &Value, environment: Option<&str>) -> Result<Service> {
    let mut merged = root.clone();
    if let Some(name) = environment {
        let selected = root
            .get("env")
            .and_then(|environments| environments.get(name))
            .ok_or_else(|| Error::Invalid(format!("no env `{name}` in the wrangler config")))?;
        let mut replacement = serde_json::Map::new();
        for key in INHERITED {
            if let Some(inherited) = root.get(key) {
                replacement.insert(key.to_owned(), inherited.clone());
            }
        }
        if let Some(object) = selected.as_object() {
            for (key, value) in object {
                replacement.insert(key.clone(), value.clone());
            }
        }
        merged = Value::Object(replacement);
    }
    let text = |key: &str| merged.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let mut service = Service {
        name: text("name"),
        main: text("main"),
        compatibility_date: text("compatibility_date"),
        ..Service::default()
    };
    if service.name.is_empty() {
        return Err(Error::Invalid("the wrangler config has no `name`".into()));
    }
    service.compatibility_flags = strings(merged.get("compatibility_flags"));
    if let Some(vars) = merged.get("vars").and_then(Value::as_object) {
        for (key, value) in vars {
            match value {
                Value::String(string) => service.text_vars.push((key.clone(), string.clone())),
                other => service.json_vars.push((key.clone(), other.to_string())),
            }
        }
    }
    for entry in array(&merged, "r2_buckets") {
        service.r2.push(R2Binding { binding: field(entry, "binding"), bucket: field(entry, "bucket_name") });
    }
    for entry in array(&merged, "d1_databases") {
        service.d1.push(D1Binding { binding: field(entry, "binding"), database: field(entry, "database_name") });
    }
    for entry in array(&merged, "kv_namespaces") {
        let namespace = if entry.get("id").is_some() { field(entry, "id") } else { field(entry, "binding") };
        service.kv.push(KvBinding { binding: field(entry, "binding"), namespace });
    }
    for entry in merged.pointer("/queues/producers").and_then(Value::as_array).into_iter().flatten() {
        service.queues.push(QueueBinding { binding: field(entry, "binding"), queue: field(entry, "queue") });
    }
    for entry in merged.pointer("/durable_objects/bindings").and_then(Value::as_array).into_iter().flatten() {
        service.durable_objects.push(DurableObjectBinding { binding: field(entry, "name"), class_name: field(entry, "class_name") });
    }
    for migration in array(&merged, "migrations") {
        service.sqlite_classes.extend(strings(migration.get("new_sqlite_classes")));
    }
    if let Some(exports) = merged.get("exports").and_then(Value::as_object) {
        for (class_name, descriptor) in exports {
            if descriptor.get("storage").and_then(Value::as_str) == Some("sqlite") {
                service.sqlite_classes.push(class_name.clone());
            }
        }
    }
    for entry in array(&merged, "services") {
        service.services.push(ServiceBinding { binding: field(entry, "binding"), service: field(entry, "service") });
    }
    for entry in array(&merged, "routes") {
        let pattern = entry.as_str().map(str::to_owned).unwrap_or_else(|| field(entry, "pattern"));
        if !pattern.is_empty() {
            service.routes.push(pattern);
        }
    }
    service.crons = strings(merged.pointer("/triggers/crons"));
    for entry in merged.pointer("/queues/consumers").and_then(Value::as_array).into_iter().flatten() {
        let mut settings = serde_json::Map::new();
        for (wrangler_name, workerd_name) in [
            ("max_batch_size", "maxBatchSize"),
            ("max_batch_timeout", "maxBatchTimeout"),
            ("max_retries", "maxRetries"),
            ("retry_delay", "retryDelay"),
            ("max_concurrency", "maxConcurrency"),
            ("dead_letter_queue", "deadLetterQueue"),
        ] {
            if let Some(value) = entry.get(wrangler_name) {
                settings.insert(workerd_name.to_owned(), value.clone());
            }
        }
        service.queue_consumers.push(QueueConsumer { queue: field(entry, "queue"), settings });
    }
    if let Some(assets) = merged.get("assets").and_then(Value::as_object) {
        let mut config = serde_json::Map::new();
        for key in ["html_handling", "not_found_handling", "base_path", "run_worker_first"] {
            if let Some(value) = assets.get(key) {
                config.insert(key.to_owned(), value.clone());
            }
        }
        service.assets = Some(Assets {
            directory: assets.get("directory").and_then(Value::as_str).unwrap_or_default().to_owned(),
            binding: assets.get("binding").and_then(Value::as_str).map(str::to_owned),
            config,
        });
    }
    for (key, kind) in [("ai", "ai"), ("version_metadata", "version_metadata"), ("images", "images"), ("browser", "browser"), ("analytics_engine_datasets", "analytics_engine")] {
        match merged.get(key) {
            Some(Value::Object(object)) => service.unsupported.push(Unsupported { kind: kind.to_owned(), binding: object.get("binding").and_then(Value::as_str).unwrap_or_default().to_owned() }),
            Some(Value::Array(entries)) => {
                for entry in entries {
                    service.unsupported.push(Unsupported { kind: kind.to_owned(), binding: field(entry, "binding") });
                }
            }
            _ => {}
        }
    }
    for (key, kind) in [("vectorize", "vectorize"), ("hyperdrive", "hyperdrive"), ("workflows", "workflows")] {
        for entry in array(&merged, key) {
            service.unsupported.push(Unsupported { kind: kind.to_owned(), binding: field(entry, "binding") });
        }
    }
    for entry in merged.pointer("/unsafe/bindings").and_then(Value::as_array).into_iter().flatten() {
        service.unsupported.push(Unsupported { kind: field(entry, "type"), binding: field(entry, "name") });
    }
    Ok(service)
}

fn array<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value.get(key).and_then(Value::as_array).into_iter().flatten()
}

fn field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(|entry| entry.as_str().map(str::to_owned)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_comments_and_trailing_commas_go_but_strings_stay() {
        let source = "{ // line\n \"a\": \"x // not a comment\", /* block */ \"b\": [1, 2,], }";
        let value: Value = serde_json::from_str(&strip_jsonc(source)).expect("valid json after stripping");
        assert_eq!(value["a"], "x // not a comment");
        assert_eq!(value["b"][1], 2);
    }

    #[test]
    fn material_cdn_shape() {
        let value: Value = serde_json::from_str(&strip_jsonc(
            r#"{ "name": "material-cdn", "main": "worker.ts", "compatibility_date": "2026-11-21",
                 "r2_buckets": [{ "binding": "MATERIAL", "bucket_name": "aphrody-material" }],
                 "routes": [{ "pattern": "material.aphrody.com", "custom_domain": true }] }"#,
        ))
        .expect("json");
        let service = from_value(&value, None).expect("service");
        assert_eq!(service.r2, vec![R2Binding { binding: "MATERIAL".into(), bucket: "aphrody-material".into() }]);
        assert_eq!(service.routes, vec!["material.aphrody.com".to_owned()]);
    }

    #[test]
    fn environment_replaces_bindings_but_inherits_the_compatibility_date() {
        let value: Value = serde_json::from_str(
            r#"{ "name": "a", "compatibility_date": "2026-11-18", "r2_buckets": [{ "binding": "X", "bucket_name": "x" }],
                 "env": { "web": { "name": "b", "r2_buckets": [{ "binding": "Y", "bucket_name": "y" }] } } }"#,
        )
        .expect("json");
        let service = from_value(&value, Some("web")).expect("service");
        assert_eq!(service.name, "b");
        assert_eq!(service.compatibility_date, "2026-11-18");
        assert_eq!(service.r2.len(), 1);
        assert_eq!(service.r2[0].binding, "Y");
    }
}
