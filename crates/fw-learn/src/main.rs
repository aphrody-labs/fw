//! `fw-learn`: the engine behind `fw learn`.

use anyhow::{Result, bail};
use fw_learn::products::{Repo, discover_products, discover_repos};
use fw_learn::store::{Store, resolve_root};
use fw_learn::{HarvestOptions, distill, publish, remote, search, systemd_unit};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: fw learn <status|harvest|distill|search|rules|publish|loop|systemd> [options]
  status                         counts by kind and product, repositories and their harvested heads
  harvest                        read git history, manifests, docs, code graphs, CI runs, releases, issues and pull requests
  distill                        turn the corpus into rules (stored, searchable)
  search <words> [--limit N] [--kind K] [--product P]
  rules [--product P] [--full]   print the distilled rules as Markdown
  publish [--dry-run] [--publish-all]  write the rules (the first repository, or all) into AGENTS.md, CLAUDE.md, docs/knowledge/learned and llms.txt on origin/main
  loop [--interval S] [--cycles N] [--publish-every S]
  systemd [--bin PATH] [--repo DIR]... [--user NAME] [--interval S]
options: --root DIR (store), --repo DIR (repeatable), --repos DIR (every product repository below it), --rev REV,
  --max-commits N, --max-issues N, --releases URL, --no-graph, --no-remote";

struct Args(Vec<String>);

impl Args {
    const WITH_VALUE: &[&str] = &["--root", "--xfs-root", "--repo", "--repos", "--rev", "--max-commits", "--max-issues", "--releases", "--limit", "--kind", "--product", "--interval", "--cycles", "--publish-every", "--bin", "--user", "--dir"];

    fn values(&self, name: &str) -> Vec<&str> {
        self.0.windows(2).filter(|pair| pair[0] == name).map(|pair| pair[1].as_str()).collect()
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values(name).into_iter().next()
    }

    fn number(&self, name: &str, default: usize) -> usize {
        self.value(name).and_then(|text| text.parse().ok()).unwrap_or(default)
    }

    fn flag(&self, name: &str) -> bool {
        self.0.iter().any(|arg| arg == name)
    }

    fn positional(&self) -> Vec<&str> {
        let mut out = Vec::new();
        let mut skip = false;
        for arg in &self.0 {
            if skip {
                skip = false;
            } else if Self::WITH_VALUE.contains(&arg.as_str()) {
                skip = true;
            } else if !arg.starts_with("--") {
                out.push(arg.as_str());
            }
        }
        out
    }
}

fn open_store(args: &Args) -> Result<Store> {
    Store::open(&resolve_root(args.value("--root").or_else(|| args.value("--xfs-root")).map(Path::new)))
}

fn repo_paths(args: &Args) -> Vec<PathBuf> {
    let mut repos: Vec<PathBuf> = args.values("--repo").into_iter().chain(args.values("--dir")).map(PathBuf::from).collect();
    for dir in args.values("--repos") {
        repos.extend(discover_repos(Path::new(dir)));
    }
    if repos.is_empty() {
        repos.push(PathBuf::from("."));
    }
    repos
}

fn harvest_options(args: &Args) -> HarvestOptions {
    HarvestOptions {
        repos: repo_paths(args),
        rev: args.value("--rev").map(str::to_owned),
        max_commits: args.number("--max-commits", 150_000),
        max_issues: args.number("--max-issues", 1000),
        releases_url: args.value("--releases").unwrap_or(remote::DEFAULT_RELEASES).to_owned(),
        graphs: !args.flag("--no-graph"),
        remote: !args.flag("--no-remote"),
        graph_limit: Duration::from_secs(1800),
        graph_interval: 3600,
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_secs()).unwrap_or(0)
}

fn publish_all(store: &Store, args: &Args, push: bool) -> Result<Value> {
    let mut results = Vec::new();
    let paths = repo_paths(args);
    let published = if args.flag("--publish-all") { paths.len() } else { 1 };
    for path in paths.iter().take(published) {
        let repo = Repo::open(path, Some("origin/main"))?;
        let tree = repo.tree()?;
        let products = discover_products(&repo, &tree)?;
        let build = || publish::plan(store, &repo, &products);
        let message = format!("{}, llms.txt and learned rules (fw learn publish)", fw_learn::git::PUBLISH_SUBJECT);
        let outcome = publish::publish(&repo, &build, &message, push)?;
        results.push(json!({ "repo": repo.name, "changed": outcome.changed, "commit": outcome.commit }));
    }
    Ok(json!({ "ok": true, "pushed": push, "repos": results }))
}

fn run(args: &Args) -> Result<Value> {
    let command = args.positional().first().copied().unwrap_or("status").to_owned();
    match command.as_str() {
        "status" => open_store(args)?.stats(),
        "harvest" => {
            let store = open_store(args)?;
            let report = fw_learn::harvest(&store, &harvest_options(args))?;
            Ok(json!({ "ok": true, "harvest": report, "dataset": store.stats()? }))
        }
        "distill" => {
            let store = open_store(args)?;
            let rules = distill::distill(&store)?;
            Ok(json!({ "ok": true, "rules": rules, "dataset": store.stats()? }))
        }
        "search" => {
            let words = args.positional()[1..].join(" ");
            if words.trim().is_empty() {
                bail!("usage: fw learn search <words> [--limit N] [--kind K] [--product P]");
            }
            search(&open_store(args)?, &words, args.number("--limit", 10), args.value("--kind").unwrap_or(""), args.value("--product").unwrap_or(""))
        }
        "rules" => {
            let store = open_store(args)?;
            let mut statement = store.conn.prepare("SELECT id, display FROM product ORDER BY repo, dir")?;
            let products: Vec<(String, String)> = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
            let mut text = String::new();
            for (id, display) in products.iter().filter(|(id, _)| args.value("--product").is_none_or(|wanted| wanted == id)) {
                text.push_str(&format!("## {id} ({display})\n\n{}\n", distill::render(&store, id, !args.flag("--full"))?));
            }
            Ok(Value::String(text))
        }
        "publish" => publish_all(&open_store(args)?, args, !args.flag("--dry-run")),
        "loop" => {
            let store = open_store(args)?;
            let (interval, cycles, publish_every) = (args.number("--interval", 300) as u64, args.number("--cycles", 1), args.number("--publish-every", 21_600) as u64);
            let options = harvest_options(args);
            let mut completed = 0usize;
            let mut last = json!({});
            loop {
                last = json!({ "harvest": fw_learn::harvest(&store, &options)?, "rules": distill::distill(&store)? });
                let due = store.state("published-at")?.and_then(|at| at.parse::<u64>().ok()).is_none_or(|at| now() - at >= publish_every);
                if due && !args.flag("--no-publish") {
                    match publish_all(&store, args, true) {
                        Ok(report) => {
                            store.set_state("published-at", &now().to_string())?;
                            last["publish"] = report;
                        }
                        Err(error) => last["publish"] = json!({ "error": format!("{error:#}") }),
                    }
                }
                completed += 1;
                if cycles > 0 && completed >= cycles {
                    break;
                }
                std::thread::sleep(Duration::from_secs(interval.max(1)));
            }
            Ok(json!({ "ok": true, "cyclesCompleted": completed, "intervalSecs": interval, "lastCycle": last }))
        }
        "systemd" => {
            let root = resolve_root(args.value("--root").map(Path::new));
            let binary = args.value("--bin").map_or_else(|| PathBuf::from("/usr/local/bin/fw-learn"), PathBuf::from);
            let repos = if args.values("--repo").is_empty() { vec![PathBuf::from("/srv/xbuild/src/fw")] } else { repo_paths(args) };
            let unit = systemd_unit(&binary, &root, &repos, args.number("--interval", 300) as u64, args.value("--user").unwrap_or("ubuntu"));
            Ok(json!({ "ok": true, "unitName": "fw-learn.service", "unit": unit }))
        }
        other => bail!("unknown subcommand `{other}`\n{USAGE}"),
    }
}

fn main() {
    let args = Args(std::env::args().skip(1).collect());
    if args.flag("--help") || args.flag("-h") {
        println!("{USAGE}");
        return;
    }
    match run(&args) {
        Ok(Value::String(text)) => print!("{text}"),
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default()),
        Err(error) => {
            eprintln!("fw learn: {error:#}");
            std::process::exit(1);
        }
    }
}
