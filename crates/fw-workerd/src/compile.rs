//! Wrangler service + wrangler bundle -> a workerd capnp config. R2, KV, D1, Queues, cron, assets and the Cache API are the implementations
//! workerd bundles (`/workerd/r2.js` and its siblings, the sources `workerd serve wrangler.json` uses); this module wires them to our disks and to
//! Garage. R2 objects live in an S3 bucket through a small bridge Worker (see `bridge`); D1, KV, Queues and Durable Objects live in SQLite files
//! under the state directory.

use crate::bridge::{BRIDGE_FILE, BRIDGE_MODULE};
use crate::wrangler::Service;
use crate::{write_text, Error, Result};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum Storage {
    /// R2 objects as files under the state directory (what `workerd serve wrangler.json` does).
    LocalDisk,
    /// R2 objects as plain objects of one S3 bucket, `<r2 bucket>/<key>`, through the bridge Worker (credentials from AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY).
    S3Bridge { bucket: String, endpoint: String, region: String },
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub listen_address: String,
    /// Directory that holds the SQLite files and the cache of this service.
    pub state_directory: String,
    pub storage: Storage,
    /// Other self-hosted services a service binding may point at: service name to listen address.
    pub linked_services: BTreeMap<String, String>,
    /// Names of the secrets the worker reads; their values come from the process environment, never from the config.
    pub secrets: Vec<String>,
    /// Directory the wrangler config's relative paths (assets) are resolved against.
    pub project_directory: PathBuf,
}

#[derive(Debug)]
pub struct Compiled {
    pub config_path: PathBuf,
    pub report: serde_json::Value,
    pub directories: Vec<String>,
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn module_line(file: &str) -> Option<String> {
    let extension = file.rsplit('.').next().unwrap_or_default();
    let kind = match extension {
        "js" | "mjs" => "esModule",
        "wasm" => "wasm",
        "txt" | "html" | "sql" | "css" | "svg" => "text",
        "bin" | "ttf" | "woff2" => "data",
        "json" => "json",
        _ => return None,
    };
    Some(format!("(name = {}, {kind} = embed {})", quote(file), quote(file)))
}

fn disk(name: &str, path: &str) -> String {
    format!("(name = {}, disk = (path = {}, writable = true))", quote(name), quote(path))
}

/// A bundled workerd Worker (`/workerd/<module>`) with its Durable Object class on the disk `<name>-storage`.
fn bundled(name: &str, module: &str, flags: &[&str], class: Option<(&str, &str)>, bindings: &[String]) -> String {
    let mut text = format!("(name = {}, worker = (modules = [(name = {}, bundled = {})], compatibilityDate = \"2026-01-01\"", quote(name), quote(module), quote(&format!("/workerd/{module}")));
    if !flags.is_empty() {
        let list: Vec<String> = flags.iter().map(|flag| quote(flag)).collect();
        text.push_str(&format!(", compatibilityFlags = [{}]", list.join(", ")));
    }
    if let Some((_, class_name)) = class {
        text.push_str(&format!(", durableObjectNamespaces = [(className = {}, uniqueKey = {}, enableSql = true)]", quote(class_name), quote(class_name)));
    }
    text.push_str(&format!(", durableObjectStorage = (localDisk = {})", quote(&format!("{name}-storage"))));
    let mut all: Vec<String> = Vec::new();
    if let Some((binding, class_name)) = class {
        all.push(format!("(name = {}, durableObjectNamespace = (className = {}))", quote(binding), quote(class_name)));
    }
    all.extend(bindings.iter().cloned());
    text.push_str(&format!(", bindings = [{}]))", all.join(", ")));
    text
}

pub fn state_directories(service: &Service, settings: &Settings) -> Vec<String> {
    let root = settings.state_directory.trim_end_matches('/');
    let mut directories = vec![format!("{root}/do")];
    if !service.r2.is_empty() {
        directories.push(format!("{root}/r2"));
    }
    if !service.kv.is_empty() {
        directories.push(format!("{root}/kv/blobs"));
    }
    if !service.d1.is_empty() {
        directories.push(format!("{root}/d1"));
    }
    if !service.crons.is_empty() {
        directories.push(format!("{root}/cron"));
    }
    if !service.queues.is_empty() || !service.queue_consumers.is_empty() {
        directories.push(format!("{root}/queues"));
    }
    directories.push(format!("{root}/cache"));
    directories
}

pub fn render(service: &Service, bundle_files: &[String], settings: &Settings) -> Result<(String, serde_json::Value)> {
    let name = &service.name;
    let root = settings.state_directory.trim_end_matches('/').to_owned();
    let mut services = vec!["(name = \"main\", worker = .main)".to_owned(), disk("state", &format!("{root}/do"))];
    let mut bindings = Vec::new();
    let mut supported = Vec::new();
    let mut unsupported: Vec<serde_json::Value> = service.unsupported.iter().map(|entry| json!({ "kind": entry.kind, "binding": entry.binding })).collect();

    for (variable, value) in &service.text_vars {
        bindings.push(format!("(name = {}, text = {})", quote(variable), quote(value)));
    }
    for (variable, value) in &service.json_vars {
        bindings.push(format!("(name = {}, json = {})", quote(variable), quote(value)));
    }
    for secret in &settings.secrets {
        bindings.push(format!("(name = {}, fromEnvironment = {})", quote(secret), quote(secret)));
        supported.push(json!({ "kind": "secret", "binding": secret, "backend": "process environment" }));
    }

    if !service.r2.is_empty() {
        let blobs = format!("{name}-r2-blobs");
        services.push(disk(&format!("{name}-r2-storage"), &format!("{root}/r2")));
        match &settings.storage {
            Storage::LocalDisk => services.push(format!("(name = {}, bucket = (path = \"r2\", localDisk = {}))", quote(&blobs), quote(&format!("{name}-r2-storage")))),
            Storage::S3Bridge { bucket, endpoint, region } => {
                services.push(format!("(name = \"s3\", external = (address = {}, http = ()))", quote(endpoint)));
                services.push(format!(
                    "(name = {}, worker = (modules = [(name = {}, esModule = embed {})], compatibilityDate = \"2026-01-01\", bindings = [(name = \"S3\", service = \"s3\"), (name = \"S3_HOST\", text = {}), (name = \"BUCKET\", text = {}), (name = \"REGION\", text = {}), (name = \"ACCESS_KEY_ID\", fromEnvironment = \"AWS_ACCESS_KEY_ID\"), (name = \"SECRET_ACCESS_KEY\", fromEnvironment = \"AWS_SECRET_ACCESS_KEY\")]))",
                    quote(&blobs),
                    quote(BRIDGE_FILE),
                    quote(BRIDGE_FILE),
                    quote(endpoint),
                    quote(bucket),
                    quote(region)
                ));
            }
        }
        services.push(bundled(&format!("{name}-r2"), "r2.js", &[], None, &[format!("(name = \"BLOBS\", service = {})", quote(&blobs))]));
        let backend = match &settings.storage {
            Storage::LocalDisk => "workerd bucket service on local disk".to_owned(),
            Storage::S3Bridge { bucket, endpoint, .. } => format!("S3 bridge to http://{endpoint}/{bucket}/<r2 bucket>/<key> (Garage)"),
        };
        for entry in &service.r2 {
            bindings.push(format!("(name = {}, r2Bucket = (name = {}, props = (json = {})))", quote(&entry.binding), quote(&format!("{name}-r2")), quote(&json!({ "bucket": entry.bucket }).to_string())));
            supported.push(json!({ "kind": "r2", "binding": entry.binding, "bucket": entry.bucket, "backend": backend }));
        }
    }
    if !service.kv.is_empty() {
        services.push(disk(&format!("{name}-kv-storage"), &format!("{root}/kv")));
        services.push(disk(&format!("{name}-kv-blobs"), &format!("{root}/kv/blobs")));
        services.push(bundled(&format!("{name}-kv"), "kv.js", &[], Some(("SHARD", "KVNamespace")), &[format!("(name = \"BLOBS\", service = {})", quote(&format!("{name}-kv-blobs")))]));
        for entry in &service.kv {
            bindings.push(format!("(name = {}, kvNamespace = (name = {}, props = (json = {})))", quote(&entry.binding), quote(&format!("{name}-kv")), quote(&json!({ "namespace": entry.namespace }).to_string())));
            supported.push(json!({ "kind": "kv", "binding": entry.binding, "backend": "workerd kv.js, SQLite file" }));
        }
    }
    if !service.d1.is_empty() {
        services.push(disk(&format!("{name}-d1-storage"), &format!("{root}/d1")));
        services.push(bundled(&format!("{name}-d1"), "d1.js", &[], Some(("DATABASE", "D1Database")), &[]));
        for entry in &service.d1 {
            bindings.push(format!(
                "(name = {}, wrapped = (moduleName = \"cloudflare-internal:d1-api\", innerBindings = [(name = \"fetcher\", service = (name = {}, props = (json = {})))]))",
                quote(&entry.binding),
                quote(&format!("{name}-d1")),
                quote(&json!({ "database": entry.database }).to_string())
            ));
            supported.push(json!({ "kind": "d1", "binding": entry.binding, "database": entry.database, "backend": "workerd d1.js, SQLite file" }));
        }
    }
    if !service.queues.is_empty() || !service.queue_consumers.is_empty() {
        let queues_name = format!("{name}-queues");
        services.push(disk(&format!("{queues_name}-storage"), &format!("{root}/queues")));
        let mut consumers = serde_json::Map::new();
        for consumer in &service.queue_consumers {
            let mut settings_json = consumer.settings.clone();
            settings_json.insert("consumer".to_owned(), json!("worker"));
            consumers.insert(consumer.queue.clone(), serde_json::Value::Object(settings_json));
        }
        let mut inner = vec![format!("(name = \"CONSUMERS\", json = {})", quote(&serde_json::Value::Object(consumers.clone()).to_string()))];
        if !consumers.is_empty() {
            inner.push("(name = \"worker\", service = \"main\")".to_owned());
        }
        services.push(bundled(&queues_name, "queues.js", &["service_binding_extra_handlers"], Some(("SHARD", "QueueShard")), &inner));
        for entry in &service.queues {
            bindings.push(format!("(name = {}, queue = (name = {}, props = (json = {})))", quote(&entry.binding), quote(&queues_name), quote(&json!({ "queue": entry.queue }).to_string())));
            supported.push(json!({ "kind": "queue_producer", "binding": entry.binding, "queue": entry.queue, "backend": "workerd queues.js, SQLite file" }));
        }
        for consumer in &service.queue_consumers {
            supported.push(json!({ "kind": "queue_consumer", "queue": consumer.queue, "backend": "workerd queues.js, delivers to the main worker's queue()" }));
        }
    }
    if !service.crons.is_empty() {
        let cron_name = format!("{name}-cron");
        services.push(disk(&format!("{cron_name}-storage"), &format!("{root}/cron")));
        let schedule = json!({ name: service.crons }).to_string();
        services.push(bundled(
            &cron_name,
            "cron.js",
            &["service_binding_extra_handlers"],
            Some(("SCHEDULE", "Schedule")),
            &[format!("(name = \"CRONS\", json = {})", quote(&schedule)), format!("(name = {}, service = \"main\")", quote(name))],
        ));
        supported.push(json!({ "kind": "cron", "expressions": service.crons, "backend": "workerd cron.js (Durable Object alarms)" }));
    }
    if let Some(assets) = &service.assets {
        let directory = settings.project_directory.join(&assets.directory);
        let mut config = assets.config.clone();
        for (key, file) in [("headers", "_headers"), ("redirects", "_redirects"), ("ignore", ".assetsignore")] {
            if let Ok(text) = std::fs::read_to_string(directory.join(file)) {
                config.insert(key.to_owned(), json!(text));
            }
        }
        services.push(format!(
            "(name = {}, assets = (path = {}, config = {}, userWorker = \"main\"))",
            quote(&format!("{name}-assets")),
            quote(&directory.to_string_lossy()),
            quote(&serde_json::Value::Object(config).to_string())
        ));
        if let Some(binding) = &assets.binding {
            bindings.push(format!("(name = {}, service = (name = {}, entrypoint = \"AssetWorker\"))", quote(binding), quote(&format!("{name}-assets"))));
        }
        supported.push(json!({ "kind": "assets", "binding": assets.binding, "directory": directory.to_string_lossy(), "backend": "workerd assets service" }));
    }
    services.push(format!("(name = {}, cache = (path = {}))", quote(&format!("{name}-cache")), quote(&format!("{root}/cache"))));
    supported.push(json!({ "kind": "cache_api", "backend": "workerd cache service on local disk" }));

    for entry in &service.services {
        match settings.linked_services.get(&entry.service) {
            Some(address) => {
                let target = format!("link-{}", entry.service);
                services.push(format!("(name = {}, external = (address = {}, http = ()))", quote(&target), quote(address)));
                bindings.push(format!("(name = {}, service = {})", quote(&entry.binding), quote(&target)));
                supported.push(json!({ "kind": "service", "binding": entry.binding, "service": entry.service, "backend": address }));
            }
            None => unsupported.push(json!({ "kind": "service", "binding": entry.binding, "service": entry.service, "reason": "target is not a self-hosted service" })),
        }
    }

    let mut modules: Vec<String> = Vec::new();
    for file in bundle_files {
        if let Some(line) = module_line(file) {
            modules.push(line);
        }
    }
    let classes: Vec<String> = service
        .durable_objects
        .iter()
        .map(|entry| format!("(className = {}, uniqueKey = {}, enableSql = {})", quote(&entry.class_name), quote(&format!("{name}-{}", entry.class_name)), service.sqlite_classes.contains(&entry.class_name)))
        .collect();
    for entry in &service.durable_objects {
        bindings.push(format!("(name = {}, durableObjectNamespace = (className = {}))", quote(&entry.binding), quote(&entry.class_name)));
        supported.push(json!({ "kind": "durable_object", "binding": entry.binding, "class": entry.class_name, "backend": "workerd, SQLite file" }));
    }

    let mut out = String::new();
    out.push_str("using Workerd = import \"/workerd/workerd.capnp\";\n\nconst config :Workerd.Config = (\n  services = [\n");
    for line in &services {
        out.push_str(&format!("    {line},\n"));
    }
    out.push_str("  ],\n  sockets = [\n");
    out.push_str(&format!("    (name = \"http\", address = {}, http = (), service = \"main\"),\n", quote(&settings.listen_address)));
    out.push_str("  ],\n");
    out.push_str(");\n\nconst main :Workerd.Worker = (\n  modules = [\n");
    for line in &modules {
        out.push_str(&format!("    {line},\n"));
    }
    out.push_str("  ],\n");
    out.push_str(&format!("  compatibilityDate = {},\n", quote(&service.compatibility_date)));
    if !service.compatibility_flags.is_empty() {
        let flags: Vec<String> = service.compatibility_flags.iter().map(|flag| quote(flag)).collect();
        out.push_str(&format!("  compatibilityFlags = [{}],\n", flags.join(", ")));
    }
    out.push_str(&format!("  cacheApiOutbound = {},\n", quote(&format!("{name}-cache"))));
    out.push_str("  bindings = [\n");
    for line in &bindings {
        out.push_str(&format!("    {line},\n"));
    }
    out.push_str("  ],\n");
    if !classes.is_empty() {
        out.push_str(&format!("  durableObjectNamespaces = [{}],\n", classes.join(", ")));
    }
    out.push_str("  durableObjectStorage = (localDisk = \"state\"),\n);\n");

    let report = json!({
        "service": name,
        "compatibility_date": service.compatibility_date,
        "supported": supported,
        "unsupported": unsupported,
        "routes": service.routes,
        "crons": service.crons,
    });
    Ok((out, report))
}

/// Copies the bundle next to the config, writes `<name>.capnp` and `<name>.report.json`.
pub fn compile(service: &Service, bundle: &Path, out_directory: &Path, settings: &Settings) -> Result<Compiled> {
    let mut files: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(bundle).map_err(|source| Error::Io { path: bundle.to_path_buf(), source })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io { path: bundle.to_path_buf(), source })?;
        if entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
            files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files.sort();
    let main_module = main_module_name(service, &files)?;
    files.retain(|file| *file != main_module);
    files.insert(0, main_module);
    std::fs::create_dir_all(out_directory).map_err(|source| Error::Io { path: out_directory.to_path_buf(), source })?;
    for file in &files {
        if module_line(file).is_some() {
            let target = out_directory.join(file);
            std::fs::copy(bundle.join(file), &target).map_err(|source| Error::Io { path: target, source })?;
        }
    }
    if matches!(settings.storage, Storage::S3Bridge { .. }) && !service.r2.is_empty() {
        write_text(&out_directory.join(BRIDGE_FILE), BRIDGE_MODULE)?;
    }
    let (text, report) = render(service, &files, settings)?;
    let config_path = out_directory.join(format!("{}.capnp", service.name));
    write_text(&config_path, &text)?;
    write_text(&out_directory.join(format!("{}.report.json", service.name)), &serde_json::to_string_pretty(&report).map_err(|error| Error::Invalid(error.to_string()))?)?;
    Ok(Compiled { config_path, report, directories: state_directories(service, settings) })
}

fn main_module_name(service: &Service, files: &[String]) -> Result<String> {
    let stem = Path::new(&service.main).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    for candidate in [format!("{stem}.js"), format!("{stem}.mjs"), "worker.js".to_owned(), "index.js".to_owned()] {
        if files.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(Error::Invalid("the bundle has no main module; run `wrangler deploy --dry-run --outdir <dir>` first".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wrangler::{Assets, D1Binding, QueueBinding, R2Binding};

    fn settings(storage: Storage) -> Settings {
        Settings {
            listen_address: "127.0.0.1:8811".into(),
            state_directory: "/state".into(),
            storage,
            linked_services: BTreeMap::new(),
            secrets: vec!["TOKEN".into()],
            project_directory: PathBuf::from("/project"),
        }
    }

    fn s3() -> Storage {
        Storage::S3Bridge { bucket: "fw-edge-r2".into(), endpoint: "127.0.0.1:3910".into(), region: "garage".into() }
    }

    fn material() -> Service {
        Service {
            name: "material-cdn".into(),
            compatibility_date: "2026-11-21".into(),
            r2: vec![R2Binding { binding: "MATERIAL".into(), bucket: "aphrody-material".into() }],
            ..Service::default()
        }
    }

    #[test]
    fn r2_is_the_bundled_worker_over_the_s3_bridge() {
        let (text, report) = render(&material(), &["worker.js".into(), "a-woff2.wasm".into()], &settings(s3())).expect("render");
        assert!(text.contains("(name = \"material-cdn-r2-blobs\", worker = (modules = [(name = \"fw-s3-bridge.js\""));
        assert!(text.contains("(name = \"BUCKET\", text = \"fw-edge-r2\")"));
        assert!(text.contains("(name = \"ACCESS_KEY_ID\", fromEnvironment = \"AWS_ACCESS_KEY_ID\")"));
        assert!(text.contains("bundled = \"/workerd/r2.js\""));
        assert!(text.contains("(name = \"MATERIAL\", r2Bucket = (name = \"material-cdn-r2\", props = (json = \"{\\\"bucket\\\":\\\"aphrody-material\\\"}\")))"));
        assert!(text.contains("(name = \"TOKEN\", fromEnvironment = \"TOKEN\")"));
        assert!(text.contains("(name = \"a-woff2.wasm\", wasm = embed \"a-woff2.wasm\")"));
        assert!(text.contains("cacheApiOutbound = \"material-cdn-cache\""));
        assert!(!text.contains("cluster"));
        assert!(report["supported"].as_array().expect("array").iter().any(|entry| entry["kind"] == "r2"));
    }

    #[test]
    fn local_disk_storage_uses_the_bucket_service() {
        let (text, _) = render(&material(), &["worker.js".into()], &settings(Storage::LocalDisk)).expect("render");
        assert!(text.contains("bucket = (path = \"r2\", localDisk = \"material-cdn-r2-storage\")"));
    }

    #[test]
    fn d1_goes_through_the_internal_api_module_and_a_sqlite_disk() {
        let service = Service { name: "auto".into(), compatibility_date: "2026-11-18".into(), d1: vec![D1Binding { binding: "DB".into(), database: "auto".into() }], ..Service::default() };
        let (text, _) = render(&service, &["worker.js".into()], &settings(Storage::LocalDisk)).expect("render");
        assert!(text.contains("moduleName = \"cloudflare-internal:d1-api\""));
        assert!(text.contains("(className = \"D1Database\", uniqueKey = \"D1Database\", enableSql = true)"));
        assert!(text.contains("(name = \"auto-d1-storage\", disk = (path = \"/state/d1\", writable = true))"));
    }

    #[test]
    fn queues_cron_assets_and_service_links() {
        let service = Service {
            name: "w".into(),
            compatibility_date: "2026-11-18".into(),
            queues: vec![QueueBinding { binding: "EVENTS".into(), queue: "github-events".into() }],
            crons: vec!["*/15 * * * *".into()],
            assets: Some(Assets { directory: "ui".into(), binding: Some("ASSETS".into()), config: serde_json::Map::new() }),
            services: vec![
                crate::wrangler::ServiceBinding { binding: "SVC_AUTO".into(), service: "auto".into() },
                crate::wrangler::ServiceBinding { binding: "SVC_OTHER".into(), service: "other".into() },
            ],
            ..Service::default()
        };
        let mut linked = settings(Storage::LocalDisk);
        linked.linked_services.insert("auto".into(), "127.0.0.1:8812".into());
        let (text, report) = render(&service, &["worker.js".into()], &linked).expect("render");
        assert!(text.contains("(name = \"EVENTS\", queue = (name = \"w-queues\""));
        assert!(text.contains("bundled = \"/workerd/cron.js\""));
        assert!(text.contains("(name = \"ASSETS\", service = (name = \"w-assets\", entrypoint = \"AssetWorker\"))"));
        assert!(text.contains("(name = \"link-auto\", external = (address = \"127.0.0.1:8812\", http = ()))"));
        assert_eq!(report["unsupported"][0]["service"], "other");
    }
}
