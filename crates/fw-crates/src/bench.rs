//! Benchmarks that measure instead of quote. A suite is a directory under `bench/<category>/` with a `suite.toml`
//! and one source file per contestant; the harness builds them in one release workspace, starts each server on
//! loopback, verifies the endpoint bodies, and load-tests with `oha`, recording the hardware, toolchain, resolved
//! versions, command lines and every run. Adding a category (serializers, HTTP clients, async runtimes) is a new
//! directory and one line in `SUITES`.

use crate::awesome::{mdx_escape, thousands};
use crate::time::date_of;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Source = (&'static str, &'static str);
/// Category, suite.toml, contestant sources.
type Suite = (&'static str, &'static str, &'static [Source]);

const SUITES: &[Suite] = &[(
    "web",
    include_str!("../bench/web/suite.toml"),
    &[
        ("axum.rs", include_str!("../bench/web/axum.rs")),
        ("actix_web.rs", include_str!("../bench/web/actix_web.rs")),
        ("salvo.rs", include_str!("../bench/web/salvo.rs")),
        ("poem.rs", include_str!("../bench/web/poem.rs")),
        ("rocket.rs", include_str!("../bench/web/rocket.rs")),
        ("warp.rs", include_str!("../bench/web/warp.rs")),
        ("ntex.rs", include_str!("../bench/web/ntex.rs")),
        ("may_minihttp.rs", include_str!("../bench/web/may_minihttp.rs")),
    ],
)];

#[derive(Debug, Deserialize)]
struct SuiteConfig {
    title: String,
    category: String,
    profile: toml::Table,
    endpoint: Vec<Endpoint>,
    common: Common,
    contestant: Vec<Contestant>,
}

#[derive(Debug, Deserialize)]
struct Endpoint {
    name: String,
    path: String,
    contains: String,
}

#[derive(Debug, Deserialize)]
struct Common {
    deps: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Contestant {
    name: String,
    #[serde(rename = "crate")]
    krate: String,
    source: String,
    deps: Vec<String>,
}

fn suite(category: &str) -> Result<(SuiteConfig, &'static [Source]), String> {
    let (_, toml_src, sources) = SUITES.iter().find(|(c, _, _)| *c == category).ok_or_else(|| format!("no benchmark suite `{category}` (available: {})", SUITES.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")))?;
    Ok((toml::from_str(toml_src).map_err(|e| format!("bench/{category}/suite.toml: {e}"))?, sources))
}

/// The suites and their contestants.
pub fn list() -> Value {
    let suites: Vec<Value> = SUITES
        .iter()
        .filter_map(|(c, _, _)| suite(c).ok())
        .map(|(s, _)| json!({ "category": s.category, "title": s.title, "endpoints": s.endpoint.iter().map(|e| &e.path).collect::<Vec<_>>(), "contestants": s.contestant.iter().map(|c| &c.name).collect::<Vec<_>>() }))
        .collect();
    json!({ "op": "bench", "suites": suites })
}

#[derive(Debug, Clone)]
pub struct Params {
    pub workdir: PathBuf,
    pub out: Option<PathBuf>,
    pub duration: u32,
    pub warmup: u32,
    pub connections: u32,
    pub runs: u32,
    pub only: Vec<String>,
    pub server_cpus: Option<String>,
    pub load_cpus: Option<String>,
    pub label: String,
    pub base_port: u16,
    pub skip_build: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self { workdir: std::env::temp_dir().join("fw-crates-bench"), out: None, duration: 10, warmup: 3, connections: 64, runs: 3, only: Vec::new(), server_cpus: None, load_cpus: None, label: "local".into(), base_port: 18_080, skip_build: false }
    }
}

fn run_cmd(program: &str, args: &[&str]) -> String {
    Command::new(program).args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

fn proc_field(file: &str, key: &str) -> String {
    std::fs::read_to_string(file).unwrap_or_default().lines().find(|l| l.starts_with(key)).and_then(|l| l.split_once(':')).map(|(_, v)| v.trim().to_owned()).unwrap_or_default()
}

fn loadavg() -> String {
    std::fs::read_to_string("/proc/loadavg").unwrap_or_default().split_whitespace().take(3).collect::<Vec<_>>().join(" ")
}

fn environment(p: &Params) -> Value {
    let mem_kb: u64 = proc_field("/proc/meminfo", "MemTotal").split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let os = std::fs::read_to_string("/etc/os-release").unwrap_or_default().lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|v| v.trim_matches('"').to_owned()).unwrap_or_else(|| std::env::consts::OS.to_owned());
    json!({
        "label": p.label,
        "os": os,
        "kernel": run_cmd("uname", &["-sr"]),
        "cpu": proc_field("/proc/cpuinfo", "model name"),
        "logical_cpus": std::thread::available_parallelism().map_or(0, usize::from),
        "memory_gib": (mem_kb as f64 / 1_048_576.0 * 10.0).round() / 10.0,
        "rustc": run_cmd("rustc", &["-V"]),
        "cargo": run_cmd("cargo", &["-V"]),
        "oha": run_cmd("oha", &["--version"]),
        "server_cpus": p.server_cpus,
        "load_cpus": p.load_cpus,
        "loadavg_before": loadavg(),
    })
}

fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    crate::store::write_text(path, content)
}

fn generate_workspace(dir: &Path, cfg: &SuiteConfig, sources: &[Source], chosen: &[&Contestant]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let members: Vec<String> = chosen.iter().map(|c| format!("\"{}\"", c.name)).collect();
    let profile = toml::to_string(&cfg.profile).map_err(|e| e.to_string())?;
    write_if_changed(&dir.join("Cargo.toml"), &format!("[workspace]\nresolver = \"3\"\nmembers = [{}]\n\n[profile.release]\n{profile}", members.join(", ")))?;
    for c in chosen {
        let src = sources.iter().find(|(f, _)| *f == c.source).ok_or_else(|| format!("missing source {}", c.source))?.1;
        let deps: Vec<&str> = cfg.common.deps.iter().chain(&c.deps).map(String::as_str).collect();
        let manifest = format!("[package]\nname = \"bench-{n}\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n\n[[bin]]\nname = \"{n}\"\npath = \"main.rs\"\n\n[dependencies]\n{}\n", deps.join("\n"), n = c.name);
        write_if_changed(&dir.join(&c.name).join("Cargo.toml"), &manifest)?;
        write_if_changed(&dir.join(&c.name).join("main.rs"), src)?;
    }
    Ok(())
}

/// Highest resolved version of `name` in a Cargo.lock.
fn resolved_version(lock: &toml::Table, name: &str) -> Option<String> {
    let mut best: Option<semver::Version> = None;
    for p in lock.get("package")?.as_array()? {
        if p.get("name")?.as_str()? == name
            && let Ok(v) = semver::Version::parse(p.get("version")?.as_str()?)
            && best.as_ref().is_none_or(|b| v > *b)
        {
            best = Some(v);
        }
    }
    best.map(|v| v.to_string())
}

fn http_get(port: u16, path: &str) -> Option<(u16, String)> {
    let mut s = TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().ok()?, Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes()).ok()?;
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, text))
}

fn pinned(cpus: &Option<String>, program: &Path, args: &[String]) -> Command {
    match cpus {
        Some(c) if cfg!(target_os = "linux") => {
            let mut cmd = Command::new("taskset");
            cmd.arg("-c").arg(c).arg(program).args(args);
            cmd
        }
        _ => {
            let mut cmd = Command::new(program);
            cmd.args(args);
            cmd
        }
    }
}

fn oha(p: &Params, url: &str, seconds: u32) -> Result<(Vec<String>, Value), String> {
    let args: Vec<String> = ["-z".to_owned(), format!("{seconds}s"), "-c".into(), p.connections.to_string(), "--no-tui".into(), "--output-format".into(), "json".into(), url.into()].to_vec();
    let mut full = vec!["oha".to_owned()];
    full.extend(args.iter().cloned());
    let mut cmd = pinned(&p.load_cpus, Path::new("oha"), &args);
    let out = cmd.output().map_err(|e| format!("cannot run oha: {e} (install it with `cargo install oha`)"))?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("oha output: {e}: {}", String::from_utf8_lossy(&out.stderr)))?;
    Ok((full, v))
}

fn median_by_rps(mut runs: Vec<Value>) -> Value {
    runs.sort_by(|a, b| a["rps"].as_f64().unwrap_or(0.0).total_cmp(&b["rps"].as_f64().unwrap_or(0.0)));
    runs.get(runs.len() / 2).cloned().unwrap_or(Value::Null)
}

fn peak_rss_kb(child: &Child) -> Option<u64> {
    std::fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?.lines().find_map(|l| l.strip_prefix("VmHWM:"))?.split_whitespace().next()?.parse().ok()
}

fn bench_one(p: &Params, cfg: &SuiteConfig, c: &Contestant, bin: &Path, port: u16, commands: &mut Vec<String>, log: &mut dyn FnMut(&str)) -> Result<Value, String> {
    let args = vec![port.to_string()];
    let mut server = pinned(&p.server_cpus, bin, &args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("{}: {e}", bin.display()))?;
    let result = (|| -> Result<Value, String> {
        let first = &cfg.endpoint[0];
        let start = Instant::now();
        while http_get(port, &first.path).is_none_or(|(s, _)| s != 200) {
            if start.elapsed() > Duration::from_secs(20) {
                return Err(format!("{} did not answer 200 on {} within 20s", c.name, first.path));
            }
            if let Ok(Some(status)) = server.try_wait() {
                return Err(format!("{} exited early ({status})", c.name));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let mut endpoints = serde_json::Map::new();
        for e in &cfg.endpoint {
            let (status, body) = http_get(port, &e.path).ok_or_else(|| format!("{}: {} unreachable", c.name, e.path))?;
            if status != 200 || !body.contains(&e.contains) {
                return Err(format!("{}: {} answered {status} without `{}`", c.name, e.path, e.contains));
            }
            let url = format!("http://127.0.0.1:{port}{}", e.path);
            log(&format!("{} {}: warmup {}s", c.name, e.name, p.warmup));
            oha(p, &url, p.warmup)?;
            let mut runs = Vec::new();
            for i in 0..p.runs {
                let (cmdline, v) = oha(p, &url, p.duration)?;
                if commands.len() < cfg.endpoint.len() {
                    commands.push(cmdline.join(" "));
                }
                let ok_requests: f64 = v["statusCodeDistribution"]["200"].as_f64().unwrap_or(0.0);
                let total: f64 = v["statusCodeDistribution"].as_object().map_or(0.0, |m| m.values().filter_map(Value::as_f64).sum());
                let run = json!({
                    "rps": v["summary"]["requestsPerSec"], "success_rate": v["summary"]["successRate"],
                    "p50_ms": v["metrics"]["latency_ms"]["p50"], "p95_ms": v["metrics"]["latency_ms"]["p95"], "p99_ms": v["metrics"]["latency_ms"]["p99"],
                    "max_ms": v["metrics"]["latency_ms"]["max"], "non_200": total - ok_requests,
                });
                log(&format!("{} {} run {}: {:.0} req/s", c.name, e.name, i + 1, run["rps"].as_f64().unwrap_or(0.0)));
                runs.push(run);
            }
            let mut med = median_by_rps(runs.clone());
            med["runs"] = Value::Array(runs);
            endpoints.insert(e.name.clone(), med);
        }
        Ok(Value::Object(endpoints))
    })();
    let rss = peak_rss_kb(&server);
    let _ = server.kill();
    let _ = server.wait();
    let endpoints = result?;
    Ok(json!({ "endpoints": endpoints, "peak_rss_kb": rss }))
}

/// Builds and load-tests a suite; writes `<out>/<category>/<stamp>.json|mdx` and `latest.json|mdx` when `out` is set.
pub fn run(category: &str, p: &Params, log: &mut dyn FnMut(&str)) -> Result<Value, String> {
    let (cfg, sources) = suite(category)?;
    let chosen: Vec<&Contestant> = cfg.contestant.iter().filter(|c| p.only.is_empty() || p.only.contains(&c.name)).collect();
    if chosen.is_empty() {
        return Err("no contestant selected".to_owned());
    }
    let dir = p.workdir.join(&cfg.category);
    generate_workspace(&dir, &cfg, sources, &chosen)?;
    let env = environment(p);

    let mut build_seconds = None;
    if !p.skip_build {
        log(&format!("cargo build --release in {}", dir.display()));
        let t = Instant::now();
        let status = Command::new("cargo").args(["build", "--release"]).current_dir(&dir).status().map_err(|e| format!("cargo: {e}"))?;
        if !status.success() {
            return Err(format!("cargo build --release failed in {}", dir.display()));
        }
        build_seconds = Some(t.elapsed().as_secs());
    }
    let lock: toml::Table = std::fs::read_to_string(dir.join("Cargo.lock")).map_err(|e| format!("Cargo.lock: {e}"))?.parse().map_err(|e| format!("Cargo.lock: {e}"))?;

    let mut commands = Vec::new();
    let mut results = Vec::new();
    for (i, c) in chosen.iter().enumerate() {
        let bin = dir.join("target").join("release").join(format!("{}{}", c.name, std::env::consts::EXE_SUFFIX));
        let size = std::fs::metadata(&bin).map(|m| m.len()).map_err(|e| format!("{}: {e}", bin.display()))?;
        let port = p.base_port + u16::try_from(i).unwrap_or(0);
        let outcome = bench_one(p, &cfg, c, &bin, port, &mut commands, log);
        let (measured, error) = match outcome {
            Ok(v) => (v, Value::Null),
            Err(e) => {
                log(&format!("{}: {e}", c.name));
                (json!({}), json!(e))
            }
        };
        results.push(json!({
            "name": c.name, "crate": c.krate, "resolved_version": resolved_version(&lock, &c.krate),
            "live_latest": live_latest(&c.krate), "binary_bytes": size, "measured": measured, "error": error,
        }));
    }
    let mut env = env;
    env["loadavg_after"] = json!(loadavg());
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let result = json!({
        "op": "bench", "category": cfg.category, "title": cfg.title, "generated": format!("{}T{:02}:{:02}Z", date_of(unix), unix.rem_euclid(86_400) / 3600, unix.rem_euclid(3600) / 60),
        "environment": env,
        "params": { "duration_s": p.duration, "warmup_s": p.warmup, "connections": p.connections, "runs": p.runs, "profile": cfg.profile },
        "build_seconds": build_seconds,
        "endpoints": cfg.endpoint.iter().map(|e| json!({ "name": e.name, "path": e.path, "contains": e.contains })).collect::<Vec<_>>(),
        "load_commands": commands,
        "results": results,
    });
    if let Some(out) = &p.out {
        let stamp = format!("{}-{}", result["generated"].as_str().unwrap_or("run").replace([':', 'Z'], "").replace('T', "-"), p.label);
        let base = out.join(&cfg.category);
        for name in [stamp.as_str(), "latest"] {
            write_if_changed(&base.join(format!("{name}.json")), &(serde_json::to_string_pretty(&result).map_err(|e| e.to_string())? + "\n"))?;
            write_if_changed(&base.join(format!("{name}.mdx")), &mdx(&result))?;
        }
    }
    Ok(result)
}

fn live_latest(krate: &str) -> Value {
    #[cfg(feature = "live")]
    {
        crate::http::throttle();
        match fw_apm::unified_pm::resolve_live_package("cargo", krate) {
            Ok(v) => v["latest"].clone(),
            Err(_) => Value::Null,
        }
    }
    #[cfg(not(feature = "live"))]
    {
        let _ = krate;
        Value::Null
    }
}

fn f(v: &Value, path: &[&str]) -> f64 {
    path.iter().fold(v, |acc, k| &acc[*k]).as_f64().unwrap_or(0.0)
}

fn rows(v: &Value) -> Vec<Vec<String>> {
    let mut rs: Vec<&Value> = v["results"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    rs.sort_by(|a, b| f(b, &["measured", "endpoints", "hello", "rps"]).total_cmp(&f(a, &["measured", "endpoints", "hello", "rps"])));
    rs.iter()
        .enumerate()
        .map(|(i, r)| {
            let m = &r["measured"]["endpoints"];
            let failed = !r["error"].is_null();
            let cell = |x: f64, digits: usize| if failed { "failed".to_owned() } else { format!("{x:.digits$}") };
            vec![
                (i + 1).to_string(),
                r["name"].as_str().unwrap_or("").to_owned(),
                r["resolved_version"].as_str().unwrap_or("-").to_owned(),
                if failed { "failed".to_owned() } else { thousands(f(&m["hello"], &["rps"]).round() as u64) },
                cell(f(&m["hello"], &["p99_ms"]), 2),
                if failed { "failed".to_owned() } else { thousands(f(&m["json"], &["rps"]).round() as u64) },
                cell(f(&m["json"], &["p99_ms"]), 2),
                cell(f(&r["measured"], &["peak_rss_kb"]) / 1024.0, 1),
                format!("{:.1}", f(r, &["binary_bytes"]) / 1_048_576.0),
            ]
        })
        .collect()
}

const HEADERS: [&str; 9] = ["#", "framework", "version", "/hello req/s", "/hello p99 ms", "/json req/s", "/json p99 ms", "peak RSS MiB", "binary MiB"];

pub fn text(v: &Value) -> String {
    if v["suites"].is_array() {
        return v["suites"].as_array().map(|a| a.iter().map(|s| format!("{}: {} ({})\n", s["category"].as_str().unwrap_or(""), s["title"].as_str().unwrap_or(""), s["contestants"].as_array().map(|c| c.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default())).collect()).unwrap_or_default();
    }
    let e = &v["environment"];
    format!(
        "{} on {} ({} logical CPUs, {} GiB, {}), {}, {}\n{}",
        v["title"].as_str().unwrap_or(""),
        e["cpu"].as_str().unwrap_or(""),
        e["logical_cpus"],
        e["memory_gib"],
        e["kernel"].as_str().unwrap_or(""),
        e["rustc"].as_str().unwrap_or(""),
        e["oha"].as_str().unwrap_or(""),
        crate::render::text_table(&HEADERS, &rows(v))
    )
}

pub fn mdx(v: &Value) -> String {
    let e = &v["environment"];
    let p = &v["params"];
    let mut s = format!(
        "---\ntitle: {}\ngenerated: {}\ncategory: {}\n---\n\n# {}\n\nMeasured, not quoted: every framework serves `/hello` (plain text) and `/json` (one JSON object) on loopback, built in one release workspace and load-tested with oha for {} s per run, {} connections, median of {} runs after {} s of warm-up.\n\n",
        serde_json::to_string(v["title"].as_str().unwrap_or("")).unwrap_or_default(),
        v["generated"].as_str().unwrap_or(""),
        v["category"].as_str().unwrap_or(""),
        mdx_escape(v["title"].as_str().unwrap_or("")),
        p["duration_s"], p["connections"], p["runs"], p["warmup_s"]
    );
    s.push_str(&crate::render::mdx_table(&HEADERS, &rows(v)));
    s.push_str("\n## Environment\n\n");
    let env_rows: Vec<Vec<String>> = [
        ("host label", e["label"].clone()), ("os", e["os"].clone()), ("kernel", e["kernel"].clone()), ("cpu", e["cpu"].clone()),
        ("logical cpus", e["logical_cpus"].clone()), ("memory GiB", e["memory_gib"].clone()), ("rustc", e["rustc"].clone()), ("cargo", e["cargo"].clone()),
        ("load generator", e["oha"].clone()), ("server cpus", e["server_cpus"].clone()), ("load generator cpus", e["load_cpus"].clone()),
        ("load average before", e["loadavg_before"].clone()), ("load average after", e["loadavg_after"].clone()), ("build seconds", v["build_seconds"].clone()), ("generated", v["generated"].clone()),
    ]
    .iter()
    .map(|(k, val)| vec![(*k).to_owned(), val.as_str().map_or_else(|| val.to_string(), str::to_owned)])
    .collect();
    s.push_str(&crate::render::mdx_table(&["field", "value"], &env_rows));
    s.push_str("\n## Versions\n\n");
    let ver_rows: Vec<Vec<String>> = v["results"]
        .as_array()
        .map(|a| a.iter().map(|r| vec![r["name"].as_str().unwrap_or("").to_owned(), r["resolved_version"].as_str().unwrap_or("-").to_owned(), r["live_latest"].as_str().unwrap_or("-").to_owned()]).collect())
        .unwrap_or_default();
    s.push_str(&crate::render::mdx_table(&["crate", "resolved by cargo", "latest in the sparse index"], &ver_rows));
    s.push_str("\n## Commands\n\n```sh\n");
    s.push_str("cargo build --release   # in the generated workspace; profile: ");
    s.push_str(&p["profile"].to_string());
    s.push('\n');
    for c in v["load_commands"].as_array().into_iter().flatten().filter_map(|c| c.as_str()) {
        s.push_str(c);
        s.push('\n');
    }
    s.push_str("```\n\nEvery run, with p50, p95, p99 and max latency, is in the JSON file next to this page.\n");
    s
}
