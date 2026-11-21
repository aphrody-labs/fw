//! `fw-crates`, the binary behind `fw crates`: search, info, compare, rank, awesome, bench, stack and sync over the
//! crates.io dataset. `--json` prints the engine's JSON result, `--mdx` an MDX page, the default is a text table.

use clap::{Args, Parser, Subcommand};
use fw_crates::{awesome, bench, query, render, stack, store};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "fw-crates", version, about = "Crate intelligence over crates.io: ranking, awesome lists, benchmarks, stack generator")]
struct Cli {
    /// Print the JSON result.
    #[arg(long, global = true)]
    json: bool,
    /// Print an MDX page.
    #[arg(long, global = true)]
    mdx: bool,
    /// Do not contact crates.io hosts (skips the live sparse-index lookup).
    #[arg(long, global = true)]
    offline: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct Limit {
    /// Maximum number of rows.
    #[arg(long, short = 'n', default_value_t = 20)]
    limit: usize,
}

#[derive(Subcommand)]
enum Cmd {
    /// Words matched against name, keywords, categories and description.
    Search {
        query: Vec<String>,
        /// crates.io category slug, or a unique part of it.
        #[arg(long, short)]
        category: Option<String>,
        #[command(flatten)]
        limit: Limit,
    },
    /// One crate: score components, usage, releases, advisories, ranks, and the live sparse-index record.
    Info { name: String },
    /// Two or more crates side by side.
    Compare {
        #[arg(required = true, num_args = 2..)]
        names: Vec<String>,
    },
    /// Top crates overall or in a category.
    Rank {
        #[arg(long, short)]
        category: Option<String>,
        #[command(flatten)]
        limit: Limit,
    },
    /// The awesome list of a category, or the index of all lists.
    Awesome {
        category: Option<String>,
        #[command(flatten)]
        limit: Limit,
        /// Write every list (JSON and MDX) plus the index into this directory.
        #[arg(long, value_name = "DIR")]
        write: Option<PathBuf>,
    },
    /// Build and load-test a benchmark suite (`list` shows the suites).
    Bench {
        /// Suite category, or `list`.
        suite: String,
        #[arg(long, default_value_t = 10)]
        duration: u32,
        #[arg(long, default_value_t = 3)]
        warmup: u32,
        #[arg(long, default_value_t = 64)]
        connections: u32,
        #[arg(long, default_value_t = 3)]
        runs: u32,
        /// Contestants to run (comma-separated); default all.
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// CPU list for the servers (taskset -c), for example `0-5`.
        #[arg(long)]
        server_cpus: Option<String>,
        /// CPU list for the load generator, for example `6-11`.
        #[arg(long)]
        load_cpus: Option<String>,
        /// Name recorded for the machine (for example `fr1`).
        #[arg(long, default_value = "local")]
        label: String,
        #[arg(long)]
        workdir: Option<PathBuf>,
        /// Write `<category>/<stamp>.json|mdx` and `latest.*` here.
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 18_080)]
        base_port: u16,
        /// Reuse the binaries already built in the workdir.
        #[arg(long)]
        skip_build: bool,
    },
    /// Choose crates for a goal and generate a Cargo project skeleton.
    Stack {
        goal: Vec<String>,
        /// Write the project here.
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// Run `cargo check` in the generated project (needs --out; run it on the build host).
        #[arg(long)]
        check: bool,
    },
    /// Download the crates.io dump and the advisory-db, rank every crate, write the data and the awesome lists.
    #[cfg(feature = "sync")]
    Sync {
        /// URL or path of db-dump.tar.gz.
        #[arg(long, default_value = fw_crates::sync::DUMP_URL)]
        dump: String,
        /// URL or path of an advisory-db tar.gz, or `none`; default the latest rustsec/advisory-db.
        #[arg(long)]
        advisories: Option<String>,
        /// Working data directory (default: FW_CRATES_HOME or ~/.fx/crates).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the committed snapshot (index, awesome lists, ranking doc) here, for example `docs/crates`.
        #[arg(long)]
        publish: Option<PathBuf>,
        #[arg(long, default_value_t = 3000)]
        index_top: usize,
    },
    /// Where the data lives and which dump it is from.
    Status,
}

fn emit(cli: &Cli, v: &Value) {
    if cli.json {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
    } else if cli.mdx {
        print!("{}", render::mdx(v));
    } else {
        print!("{}", render::text(v));
    }
}

fn run(cli: &Cli) -> Result<(), String> {
    match &cli.cmd {
        Cmd::Search { query: q, category, limit } => emit(cli, &query::search(&*store::load()?, &q.join(" "), category.as_deref(), limit.limit)?),
        Cmd::Info { name } => emit(cli, &query::info(&*store::load()?, name, !cli.offline)?),
        Cmd::Compare { names } => emit(cli, &query::compare(&*store::load()?, names, !cli.offline)?),
        Cmd::Rank { category, limit } => emit(cli, &query::rank(&*store::load()?, category.as_deref(), limit.limit)?),
        Cmd::Awesome { category, limit, write } => {
            let ds = store::load()?;
            if let Some(dir) = write {
                let n = awesome::publish(dir, &ds.meta, &ds.categories, &ds.records, 3000)?;
                eprintln!("wrote {n} lists to {}", dir.display());
            }
            emit(cli, &query::awesome(&ds, category.as_deref(), limit.limit)?);
        }
        Cmd::Bench { suite, duration, warmup, connections, runs, only, server_cpus, load_cpus, label, workdir, out, base_port, skip_build } => {
            if suite == "list" {
                emit(cli, &bench::list());
                return Ok(());
            }
            let mut p = bench::Params { duration: *duration, warmup: *warmup, connections: *connections, runs: *runs, only: only.clone(), server_cpus: server_cpus.clone(), load_cpus: load_cpus.clone(), label: label.clone(), out: out.clone(), base_port: *base_port, skip_build: *skip_build, ..bench::Params::default() };
            if let Some(w) = workdir {
                p.workdir = w.clone();
            }
            emit(cli, &bench::run(suite, &p, &mut |m| eprintln!("{m}"))?);
        }
        Cmd::Stack { goal, out, check } => {
            let goal = goal.join(" ");
            if goal.trim().is_empty() {
                return Err("usage: fw crates stack <goal in words>".to_owned());
            }
            let mut v = stack::generate(&*store::load()?, &goal)?;
            if let Some(dir) = out {
                let files = stack::write(&v, dir)?;
                v["written"] = json!({ "dir": dir, "files": files });
                if *check {
                    v["check"] = stack::check(dir);
                }
            } else if *check {
                return Err("--check needs --out".to_owned());
            }
            emit(cli, &v);
            if v["check"]["ok"].as_bool() == Some(false) {
                return Err("cargo check failed in the generated project".to_owned());
            }
        }
        #[cfg(feature = "sync")]
        Cmd::Sync { dump, advisories, out, publish, index_top } => {
            let opts = fw_crates::sync::SyncOptions { dump: dump.clone(), advisories: advisories.clone(), out: out.clone().unwrap_or_else(store::home), publish: publish.clone(), index_top: *index_top };
            let r = fw_crates::sync::sync(&opts, &mut |m| eprintln!("{m}"))?;
            let v = json!({ "op": "sync", "meta": r.meta, "lists": r.lists, "out": r.out, "published": r.published });
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        }
        Cmd::Status => {
            let ds = store::load();
            let v = match ds {
                Ok(ds) => json!({ "op": "status", "home": store::home(), "dataset": ds.dir, "meta": ds.meta, "records": ds.records.len(), "user_agent": fw_crates::http::USER_AGENT }),
                Err(e) => json!({ "op": "status", "home": store::home(), "dataset": null, "error": e }),
            };
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fw crates: {e}");
            ExitCode::FAILURE
        }
    }
}
