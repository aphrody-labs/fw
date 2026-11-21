//! `fw-corpus`: sync, search and inspect the corpus (`fw corpus ...`, also `fw learn search`).

use anyhow::{Result, bail};
use fw_corpus::sources::{self, Opts};
use fw_corpus::store::{Hit, Store};
use serde_json::{Value, json};
use std::path::Path;

const USAGE: &str = "usage: fw-corpus <sources|status|sync|search|read|systemd> [--root DIR] [--json]
  sources                      list sources and licences
  status                       per-source documents, chunks, bytes, last fetch
  sync <id...|all> [--limit N] [--threads N] [--rate-ms N] [--force]
  search <query> [--source ID] [--limit N]
  read <url-substring> [--source ID]
  systemd [--user] [--bin PATH] [--calendar SPEC]   print the refresh service and timer units";

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|arg| arg == name).and_then(|at| args.get(at + 1)).map(String::as_str)
}

fn positional(args: &[String]) -> Vec<&str> {
    const WITH_VALUE: &[&str] = &["--root", "--limit", "--threads", "--rate-ms", "--source", "--bin", "--calendar"];
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if WITH_VALUE.contains(&arg.as_str()) {
            skip = true;
        } else if !arg.starts_with("--") {
            out.push(arg.as_str());
        }
    }
    out
}

fn open_all(root: &Path) -> Result<Vec<Store>> {
    let mut stores = Vec::new();
    for source in sources::all() {
        if root.join(format!("{}.db", source.id)).exists() {
            stores.push(Store::open(root, source.id)?);
        }
    }
    Ok(stores)
}

fn hit_json(hit: &Hit) -> Value {
    let cite = if hit.anchor.is_empty() || hit.url.contains('#') { hit.url.clone() } else { format!("{}#{}", hit.url, hit.anchor) };
    json!({
        "pillar": "corpus", "source": hit.source, "title": hit.title, "heading": hit.heading, "url": cite,
        "pinned": hit.origin, "license": hit.license, "fetched": fw_corpus::iso(hit.fetched_at),
        "score": (hit.score * 1000.0).round() / 1000.0, "snippet": hit.snippet,
    })
}

fn status(root: &Path) -> Result<Value> {
    let mut rows = Vec::new();
    let (mut docs, mut chunks, mut bytes, mut db_bytes) = (0, 0, 0, 0u64);
    for store in open_all(root)? {
        let stats = store.stats()?;
        docs += stats.docs;
        chunks += stats.chunks;
        bytes += stats.bytes;
        db_bytes += stats.db_bytes;
        rows.push(json!({
            "source": store.id, "title": store.meta("title")?, "license": store.meta("license")?, "docs": stats.docs,
            "chunks": stats.chunks, "textBytes": stats.bytes, "dbBytes": stats.db_bytes,
            "lastFetch": stats.last_fetch.map(fw_corpus::iso),
            "lastSync": store.meta("last_sync")?.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
        }));
    }
    Ok(json!({"root": root.display().to_string(), "total": {"docs": docs, "chunks": chunks, "textBytes": bytes, "dbBytes": db_bytes}, "sources": rows}))
}

fn systemd(args: &[String]) -> String {
    let user = args.iter().any(|arg| arg == "--user");
    let bin = flag(args, "--bin").unwrap_or("/usr/local/bin/fw-corpus");
    let calendar = flag(args, "--calendar").unwrap_or("*-*-* 03:30:00");
    let wanted_by = if user { "default.target" } else { "multi-user.target" };
    format!(
        "# fw-corpus-refresh.service\n[Unit]\nDescription=fw corpus incremental refresh (polite, resumable)\nAfter=network-online.target\nWants=network-online.target\n\n\
         [Service]\nType=oneshot\nExecStart={bin} sync all\nNice=19\nIOSchedulingClass=idle\nTimeoutStartSec=6h\n\n\
         # fw-corpus-refresh.timer\n[Unit]\nDescription=Daily fw corpus refresh\n\n[Timer]\nOnCalendar={calendar}\nRandomizedDelaySec=20m\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n# WantedBy for the service when run as a unit: {wanted_by}\n"
    )
}

fn run() -> Result<Value> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = fw_corpus::resolve_root(flag(&args, "--root"));
    let words = positional(&args);
    let number = |name: &str, default: usize| flag(&args, name).and_then(|value| value.parse().ok()).unwrap_or(default);
    match words.first().copied() {
        Some("sources") => Ok(json!(sources::all().iter().map(|source| json!({"id": source.id, "title": source.title, "license": sources::license(source)})).collect::<Vec<_>>())),
        Some("status") => status(&root),
        Some("sync") => {
            let requested: Vec<&str> = words[1..].to_vec();
            if requested.is_empty() {
                bail!("sync needs a source id or `all`");
            }
            let ids: Vec<String> = if requested.contains(&"all") { sources::all().iter().map(|source| source.id.to_owned()).collect() } else { requested.iter().map(|id| (*id).to_owned()).collect() };
            let opts = Opts {
                root: root.clone(),
                limit: flag(&args, "--limit").and_then(|value| value.parse().ok()),
                threads: number("--threads", 6),
                rate_ms: number("--rate-ms", 250) as u64,
                force: args.iter().any(|arg| arg == "--force"),
            };
            let mut reports = Vec::new();
            for id in ids {
                let Some(source) = sources::find(&id) else { bail!("unknown source `{id}`") };
                match sources::sync(&source, &opts) {
                    Ok(report) => {
                        eprintln!("{}: fetched {} unchanged {} failed {} bytes {}", id, report.fetched, report.unchanged, report.failed, report.bytes);
                        reports.push(report.to_json());
                    }
                    Err(error) => {
                        eprintln!("{id}: {error:#}");
                        reports.push(json!({"source": id, "error": format!("{error:#}")}));
                    }
                }
            }
            Ok(json!(reports))
        }
        Some("search") => {
            let query = words[1..].join(" ");
            if query.trim().is_empty() {
                bail!("search needs a query");
            }
            let limit = number("--limit", 8);
            let only = flag(&args, "--source");
            let mut hits = Vec::new();
            for store in open_all(&root)?.iter().filter(|store| only.is_none_or(|id| id == store.id)) {
                hits.extend(store.search(&query, limit)?);
            }
            hits.sort_by(|left, right| left.score.total_cmp(&right.score));
            hits.truncate(limit);
            Ok(json!({"query": query, "hits": hits.iter().map(hit_json).collect::<Vec<_>>()}))
        }
        Some("read") => {
            let target = words.get(1).copied().unwrap_or_default();
            let only = flag(&args, "--source");
            for store in open_all(&root)?.iter().filter(|store| only.is_none_or(|id| id == store.id)) {
                if let Some((url, origin, body)) = store.read(target)? {
                    return Ok(json!({"source": store.id, "url": url, "pinned": origin, "text": body}));
                }
            }
            bail!("no document matching `{target}`")
        }
        Some("systemd") => Ok(json!({"units": systemd(&args)})),
        _ => bail!("{USAGE}"),
    }
}

fn main() {
    match run() {
        Ok(value) => {
            if let Some(units) = value["units"].as_str() {
                println!("{units}");
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default());
            }
        }
        Err(error) => {
            eprintln!("fw-corpus: {error:#}");
            std::process::exit(1);
        }
    }
}

