//! The `fw-depot` command line: one subcommand per depot_tools tool.

use std::path::{Path, PathBuf};

use crate::deps::{Deps, Env, Platform, Value};
use crate::error::{Error, Result, fs};
use crate::cipd::{self, Client};
use crate::depedit;
use crate::gclient::{Config, Graph};
use crate::git::History;
use crate::sync::{self, Action};

const USAGE: &str = "usage: fw depot <command> [args]
  revinfo [--output-json FILE] [--filter X]   gclient revinfo: name: url@revision of every dependency
  sync [--nohooks] [--jobs N] [--no-history|--partial] [--revision path@rev] [--dry-run] [--no-cipd] [--no-gcs]   gclient sync
  runhooks   run the hooks of the DEPS files
  getdep (-r path | --var name) [--deps-file DEPS]   print a revision or variable
  setdep (-r path@rev | -r path:package@version | --var name=value) [--deps-file DEPS]   edit a DEPS file in place
  cipd (resolve PACKAGE VERSION | ensure --root DIR --ensure-file FILE | install --root DIR PACKAGE VERSION)   CIPD client
  fetch NAME [--nohooks] [--no-history] [--dry-run] [--prop=value ...]   write the .gclient of chromium, android, v8, webrtc, ... and sync
  autoninja [--print] [ninja args]   build with Siso or Ninja, chosen from args.gn and the checkout
  cl (issue|status|upload|land|patch|description|abandon) ...   git cl: Gerrit changes (--dry-run describes and sends nothing)
  new-branch NAME [--upstream REF] | map-branches   git new-branch, git map-branches
  presubmit (canned [--base REV] [--max-len N] | python [--base REV] [--depot-tools DIR])   PRESUBMIT checks
  repo (init -u URL -b REV [-m NAME] | sync | list | manifest) [-g groups] [-j N] [--partial|--shallow|--metadata-only]   AOSP repo
  lunch PRODUCT[-RELEASE-VARIANT] [--top DIR]   the environment `lunch` sets, as export lines
  skeleton   write the DEPS files of every recursed dependency into the checkout, without cloning (for offline revinfo)
  deps list DEPS [--host-os os] [--host-cpu cpu] [--os os] [--cpu cpu] [--var name=value]   JSON of a DEPS file for a target
  model --graph FILE --run1 FILE --run2 FILE ...   work/span model of a siso build
  help";

/// Options shared by the commands: flags with a value and bare flags, in order.
pub struct Options {
    pub positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Options {
    /// Splits `--name value`, `--name=value` and bare `--name` (those listed in `bare`).
    pub fn parse(arguments: &[String], bare: &[&str]) -> Options {
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut iterator = arguments.iter().peekable();
        while let Some(argument) = iterator.next() {
            let short = argument.len() == 2 && argument.starts_with('-') && !argument.starts_with("--");
            let Some(flag) = argument.strip_prefix("--").or(short.then(|| &argument[1..])) else {
                positional.push(argument.clone());
                continue;
            };
            if let Some((name, value)) = flag.split_once('=') {
                flags.push((name.to_owned(), Some(value.to_owned())));
            } else if bare.contains(&flag) {
                flags.push((flag.to_owned(), None));
            } else {
                flags.push((flag.to_owned(), iterator.next().cloned()));
            }
        }
        Options { positional, flags }
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(flag, _)| flag == name)
            .and_then(|(_, value)| value.as_deref())
    }

    pub fn values(&self, name: &str) -> Vec<&str> {
        self.flags
            .iter()
            .filter(|(flag, _)| flag == name)
            .filter_map(|(_, value)| value.as_deref())
            .collect()
    }

    pub fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(flag, _)| flag == name)
    }
}

pub fn run(arguments: &[String]) -> Result<u8> {
    let Some((command, rest)) = arguments.split_first() else {
        println!("{USAGE}");
        return Ok(0);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(0)
        }
        "revinfo" => revinfo(rest),
        "deps" => deps(rest),
        "sync" => sync(rest),
        "skeleton" => {
            let config = Config::find(&current_directory()?)?;
            let count = sync::skeleton(config, &Platform::detect())?;
            println!("wrote {count} files");
            Ok(0)
        }
        "runhooks" => runhooks(rest),
        "getdep" => getdep(rest),
        "setdep" => setdep(rest),
        "cipd" => cipd(rest),
        "fetch" => fetch(rest),
        "cl" => cl(rest),
        "new-branch" => new_branch(rest),
        "map-branches" => map_branches(),
        "presubmit" => presubmit(rest),
        "repo" => repo(rest),
        "lunch" => lunch(rest),
        "autoninja" => autoninja(rest),
        "model" => {
            crate::model::run(rest);
            Ok(0)
        }
        other => Err(Error::message(format!("unknown command {other}\n{USAGE}"))),
    }
}

fn current_directory() -> Result<PathBuf> {
    std::env::current_dir().map_err(|source| Error::io(Path::new("."), source))
}

fn revinfo(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(arguments, &[]);
    let config = Config::find(&current_directory()?)?;
    let mut graph = Graph::new(config, &Platform::detect());
    graph.load_local()?;
    let filters = options.values("filter");
    let lines: Vec<String> = graph
        .revinfo()
        .into_iter()
        .filter(|line| filters.is_empty() || filters.iter().any(|filter| line.contains(filter)))
        .collect();
    if let Some(output) = options.value("output-json") {
        let mut map = serde_json::Map::new();
        for line in &lines {
            let (name, url) = line.split_once(": ").unwrap_or((line, ""));
            let (base, revision) = match url.rsplit_once('@') {
                Some((base, revision)) if !revision.contains('/') && !revision.contains(':') => {
                    (Some(base), Some(revision))
                }
                _ => (Some(url), None),
            };
            map.insert(
                name.to_owned(),
                serde_json::json!({ "url": base, "rev": revision }),
            );
        }
        let text = serde_json::to_string_pretty(&map)?;
        if output == "-" {
            println!("{text}");
        } else {
            fs::write(Path::new(output), text)?;
        }
        return Ok(0);
    }
    for line in lines {
        println!("{line}");
    }
    Ok(0)
}

fn deps(arguments: &[String]) -> Result<u8> {
    let [subcommand, rest @ ..] = arguments else {
        return Err(Error::message("usage: fw depot deps list DEPS [flags]"));
    };
    if subcommand != "list" {
        return Err(Error::message(format!("unknown deps command {subcommand}")));
    }
    let options = Options::parse(rest, &[]);
    let path = options
        .positional
        .first()
        .ok_or_else(|| Error::message("deps list needs a DEPS file"))?;
    let platform = Platform::single(
        options.value("host-os").unwrap_or("linux"),
        options.value("host-cpu").unwrap_or("x64"),
        options.value("os").unwrap_or("linux"),
        options.value("cpu").unwrap_or("x64"),
    );
    let mut overrides = Vec::new();
    for assignment in options.values("var") {
        let (name, text) = assignment
            .split_once('=')
            .ok_or_else(|| Error::message("--var needs name=value"))?;
        overrides.push((
            name.to_owned(),
            match text {
                "True" => Value::Bool(true),
                "False" => Value::Bool(false),
                other => Value::Str(other.to_owned()),
            },
        ));
    }
    let source = fs::read_to_string(Path::new(path))?;
    let parsed = Deps::parse(&source)?;
    let env = Env::build(&parsed, &platform, None, &overrides)?;
    let resolved = parsed.resolve(&env)?;
    let hooks = parsed.hooks(&env)?;
    println!(
        "{}",
        serde_json::json!({ "deps": resolved, "hooks": hooks, "recursedeps": parsed.recursedeps })
    );
    Ok(0)
}

fn sync(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(
        arguments,
        &["nohooks", "no-history", "partial", "dry-run", "no-cipd", "no-gcs", "force", "reset", "delete_unversioned_trees"],
    );
    let config = Config::find(&current_directory()?)?;
    let mut settings = sync::Options::default();
    if let Some(jobs) = options.value("jobs") {
        settings.jobs = jobs
            .parse()
            .map_err(|_| Error::message("--jobs needs a number"))?;
    }
    settings.run_hooks = !options.has("nohooks");
    settings.dry_run = options.has("dry-run");
    settings.cipd = !options.has("no-cipd");
    settings.gcs = !options.has("no-gcs");
    settings.history = if options.has("no-history") {
        History::Shallow
    } else if options.has("partial") {
        History::BlobLess
    } else {
        History::Full
    };
    for revision in options.values("revision") {
        let (path, rev) = revision
            .split_once('@')
            .ok_or_else(|| Error::message("--revision needs path@rev"))?;
        settings.revisions.push((path.to_owned(), rev.to_owned()));
    }
    settings.only = options.values("only").iter().map(|only| (*only).to_owned()).collect();
    let (_, report) = sync::sync(config, &Platform::detect(), &settings)?;
    for result in &report.git {
        let action = match result.action {
            Action::Cloned => "cloned",
            Action::Updated => "updated",
            Action::Unchanged => "unchanged",
            Action::Planned => "planned",
        };
        println!(
            "git   {action:<9} {} {} {:.1}s",
            result.path,
            result.revision,
            result.elapsed.as_secs_f64()
        );
    }
    for (want, instance, changed) in &report.cipd {
        println!(
            "cipd  {:<9} {}:{} {}",
            if *changed { "installed" } else { "unchanged" },
            want.subdir,
            want.package,
            instance.id
        );
    }
    for (name, changed) in &report.gcs {
        println!("gcs   {:<9} {name}", if *changed { "fetched" } else { "unchanged" });
    }
    for (name, elapsed) in &report.hooks {
        println!("hook  ran       {name} {:.1}s", elapsed.as_secs_f64());
    }
    println!("sync took {:.1}s", report.elapsed.as_secs_f64());
    Ok(0)
}

fn runhooks(_arguments: &[String]) -> Result<u8> {
    let config = Config::find(&current_directory()?)?;
    let mut graph = Graph::new(config, &Platform::detect());
    graph.load_local()?;
    let hooks = graph.hooks()?;
    let mut report = sync::Report::default();
    sync::run_hooks(&graph, &hooks, &mut report)?;
    Ok(0)
}

fn deps_file(options: &Options) -> Result<PathBuf> {
    match options.value("deps-file") {
        Some(path) => Ok(PathBuf::from(path)),
        None => {
            let config = Config::find(&current_directory()?)?;
            let name = config
                .solutions
                .first()
                .map(|solution| solution.name.clone())
                .ok_or_else(|| Error::message("no solution in .gclient"))?;
            Ok(config.root.join(name).join("DEPS"))
        }
    }
}

fn getdep(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(arguments, &[]);
    let text = fs::read_to_string(&deps_file(&options)?)?;
    let mut missing = false;
    for variable in options.values("var") {
        match depedit::get_var(&text, variable)? {
            Some(value) => println!("{value}"),
            None => {
                eprintln!("Could not find any variable called {variable}.");
                missing = true;
            }
        }
    }
    for revision in options.values("r").into_iter().chain(options.values("revision")) {
        let path = revision.split(':').next().unwrap_or(revision);
        match depedit::get_revision(&text, path)? {
            Some(value) => println!("{value}"),
            None => {
                eprintln!("Could not find any dependency called {path}.");
                missing = true;
            }
        }
    }
    Ok(u8::from(missing))
}

fn setdep(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(arguments, &[]);
    let path = deps_file(&options)?;
    let mut text = fs::read_to_string(&path)?;
    for assignment in options.values("var") {
        let (name, value) = assignment
            .split_once('=')
            .ok_or_else(|| Error::message("--var needs name=value"))?;
        text = depedit::set_var(&text, name, value)?;
    }
    for revision in options.values("r").into_iter().chain(options.values("revision")) {
        let (target, value) = revision
            .rsplit_once('@')
            .ok_or_else(|| Error::message("-r needs path@revision"))?;
        text = match target.split_once(':') {
            Some((dep, package)) => depedit::set_cipd(&text, dep, package, value)?,
            None => depedit::set_revision(&text, target, value)?,
        };
    }
    fs::write(&path, text)
        .map(|()| 0)
}

fn cipd(arguments: &[String]) -> Result<u8> {
    let [subcommand, rest @ ..] = arguments else {
        return Err(Error::message("usage: fw depot cipd (resolve|ensure|install) ..."));
    };
    let options = Options::parse(rest, &[]);
    let service = options.value("service-url").unwrap_or(cipd::DEFAULT_SERVICE);
    let client = Client::new(service);
    let (os, arch) = cipd::host_platform();
    match subcommand.as_str() {
        "resolve" => {
            let [package, version] = options.positional.as_slice() else {
                return Err(Error::message("usage: cipd resolve PACKAGE VERSION"));
            };
            let instance = client.resolve(&cipd::expand(package, os, arch), version)?;
            println!("{}:{}", instance.package, instance.id);
        }
        "install" => {
            let [package, version] = options.positional.as_slice() else {
                return Err(Error::message("usage: cipd install --root DIR PACKAGE VERSION"));
            };
            let root = PathBuf::from(options.value("root").unwrap_or("."));
            let want = cipd::Want {
                subdir: String::new(),
                package: cipd::expand(package, os, arch),
                version: version.clone(),
            };
            for (want, instance, changed) in client.ensure(&root, &[want], 1)? {
                println!("{}:{} {}", want.package, instance.id, if changed { "installed" } else { "unchanged" });
            }
        }
        "ensure" => {
            let root = PathBuf::from(options.value("root").ok_or_else(|| Error::message("--root is required"))?);
            let file = options
                .value("ensure-file")
                .ok_or_else(|| Error::message("--ensure-file is required"))?;
            let (_, wants) = cipd::parse_ensure_file(&fs::read_to_string(Path::new(file))?, os, arch)?;
            let jobs = options.value("max-threads").and_then(|jobs| jobs.parse().ok()).unwrap_or(8);
            for (want, instance, changed) in client.ensure(&root, &wants, jobs)? {
                println!(
                    "{}/{}:{} {}",
                    want.subdir,
                    want.package,
                    instance.id,
                    if changed { "installed" } else { "unchanged" }
                );
            }
        }
        other => return Err(Error::message(format!("unknown cipd command {other}"))),
    }
    Ok(0)
}

fn fetch(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(arguments, &["nohooks", "no-history", "dry-run", "list"]);
    if options.has("list") || options.positional.is_empty() {
        for name in crate::fetch::names()? {
            println!("{name}");
        }
        return Ok(0);
    }
    let name = &options.positional[0];
    let mut properties = Vec::new();
    for argument in arguments.iter().filter(|argument| argument.starts_with("--")) {
        let flag = argument.trim_start_matches("--");
        if let Some((key, value)) = flag.split_once('=')
            && !matches!(key, "nohooks" | "no-history" | "dry-run")
        {
            properties.push((key.to_owned(), value.to_owned()));
        }
    }
    let spec = crate::fetch::resolve(name, &properties)?;
    let text = crate::fetch::format_spec(&spec);
    let directory = current_directory()?;
    if options.has("dry-run") {
        print!("{text}");
        return Ok(0);
    }
    if directory.join(".gclient").exists() {
        return Err(Error::message("a .gclient already exists here; remove it or fetch elsewhere"));
    }
    fs::write(&directory.join(".gclient"), &text)?;
    let config = Config::parse(&text, &directory)?;
    let mut settings = sync::Options {
        run_hooks: !options.has("nohooks"),
        history: if options.has("no-history") { History::Shallow } else { History::Full },
        ..sync::Options::default()
    };
    settings.only = Vec::new();
    let (_, report) = sync::sync(config, &Platform::detect(), &settings)?;
    println!("fetched {} git dependencies in {:.1}s", report.git.len(), report.elapsed.as_secs_f64());
    Ok(0)
}

fn autoninja(arguments: &[String]) -> Result<u8> {
    let print_only = arguments.first().is_some_and(|argument| argument == "--print");
    let arguments = if print_only { &arguments[1..] } else { arguments };
    let mut context = crate::autoninja::Context::detect();
    context.nofile_limit = nofile_limit();
    let mut notes = Vec::new();
    let outcome = crate::autoninja::plan(arguments, &context, &mut notes);
    for note in &notes {
        eprintln!("{note}");
    }
    match outcome {
        Ok(plan) => {
            if print_only {
                println!("{}", plan.program.display());
                for argument in &plan.args {
                    println!("{argument}");
                }
                for (key, value) in &plan.env {
                    println!("env {key}={value}");
                }
                return Ok(0);
            }
            let code = crate::autoninja::run(&plan).map_err(|source| Error::io(&plan.program, source))?;
            Ok(u8::try_from(code).unwrap_or(1))
        }
        Err(failure) => {
            eprintln!("depot_tools: {}", failure.message);
            Ok(failure.code)
        }
    }
}

#[cfg(unix)]
fn nofile_limit() -> u64 {
    std::fs::read_to_string("/proc/self/limits")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("Max open files"))
                .and_then(|line| line.split_whitespace().nth(4).and_then(|hard| hard.parse().ok()))
        })
        .unwrap_or(1024)
}

#[cfg(not(unix))]
fn nofile_limit() -> u64 {
    8192
}

fn cl(arguments: &[String]) -> Result<u8> {
    use crate::git_cl;
    let [subcommand, rest @ ..] = arguments else {
        return Err(Error::message("usage: fw depot cl (issue|status|upload|land|patch|description|abandon)"));
    };
    let options = Options::parse(rest, &["dry-run", "wip", "ready", "cq-dry-run", "json"]);
    let repo = current_directory()?;
    let branch = git_cl::current_branch(&repo).ok();
    let base = git_cl::server(&repo, branch.as_deref(), options.value("gerrit-server"))
        .or_else(|error| if subcommand == "issue" { Ok(String::new()) } else { Err(error) })?;
    let mut client = crate::gerrit::Client::new(&base);
    client.dry_run = options.has("dry-run");
    let need_branch = || branch.clone().ok_or_else(|| Error::message("not on a branch"));
    let need_issue = || -> Result<u64> {
        let name = need_branch()?;
        git_cl::association(&repo, &name)
            .map(|found| found.issue)
            .ok_or_else(|| Error::message(format!("branch {name} has no issue; run `fw depot cl issue N` or upload")))
    };
    match subcommand.as_str() {
        "issue" => {
            let name = need_branch()?;
            match options.positional.first() {
                Some(number) if number == "0" => {
                    git_cl::dissociate(&repo, &name);
                    println!("Issue number: None (None)");
                }
                Some(number) => {
                    let issue: u64 = number.parse().map_err(|_| Error::message("issue must be a number"))?;
                    git_cl::associate(&repo, &name, issue, &base, None)?;
                    println!("Issue number: {issue} ({base}/c/{issue})");
                }
                None => match git_cl::association(&repo, &name) {
                    Some(found) => println!("Issue number: {} ({}/c/{})", found.issue, found.server, found.issue),
                    None => println!("Issue number: None (None)"),
                },
            }
        }
        "status" => {
            let listing = crate::git::run(&repo, &["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;
            for name in listing.lines() {
                if let Some(found) = git_cl::association(&repo, name) {
                    let client = crate::gerrit::Client::new(&found.server);
                    println!("{}", git_cl::status(&repo, &client, name, &found)?);
                }
            }
        }
        "upload" => {
            let target = options.value("target-branch").or(options.value("b")).unwrap_or("main");
            let split = |name: &str| -> Vec<String> {
                options
                    .values(name)
                    .iter()
                    .flat_map(|value| value.split(','))
                    .map(str::to_owned)
                    .collect()
            };
            let upload = crate::gerrit::UploadOptions {
                reviewers: split("reviewers"),
                cc: split("cc"),
                topic: options.value("topic").map(str::to_owned),
                wip: options.has("wip"),
                ready: options.has("ready"),
                message: options.value("message").map(str::to_owned),
                hashtags: split("hashtags"),
                cq_dry_run: options.has("cq-dry-run"),
            };
            println!("{}", git_cl::upload(&repo, target, &upload, options.has("dry-run"))?);
        }
        "land" => {
            println!("{}", git_cl::land(&client, need_issue()?)?);
        }
        "patch" => {
            let spec = options
                .positional
                .first()
                .ok_or_else(|| Error::message("usage: cl patch N[/PATCHSET]"))?;
            println!("{}", git_cl::patch(&repo, &client, spec, options.value("branch"))?);
        }
        "description" => {
            let issue = need_issue()?;
            let change = client.change(&issue.to_string())?;
            let current = change["current_revision"].as_str().unwrap_or_default();
            println!("{}", change["revisions"][current]["commit"]["message"].as_str().unwrap_or_default());
        }
        "abandon" => {
            let issue = need_issue()?;
            match client.abandon(&issue.to_string(), options.value("message").unwrap_or(""))? {
                crate::gerrit::Outcome::Sent(value) => println!("{value}"),
                crate::gerrit::Outcome::Skipped(described) => println!("dry run: {} {}", described.method, described.url),
            }
        }
        other => return Err(Error::message(format!("unknown cl command {other}"))),
    }
    Ok(0)
}

fn new_branch(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(arguments, &[]);
    let name = options
        .positional
        .first()
        .ok_or_else(|| Error::message("usage: new-branch NAME [--upstream REF]"))?;
    crate::git_cl::new_branch(&current_directory()?, name, options.value("upstream"))?;
    Ok(0)
}

fn map_branches() -> Result<u8> {
    print!("{}", crate::git_cl::map_branches(&current_directory()?)?);
    Ok(0)
}

fn presubmit(arguments: &[String]) -> Result<u8> {
    use crate::presubmit;
    let [mode, rest @ ..] = arguments else {
        return Err(Error::message("usage: fw depot presubmit (canned|python) [--base REV]"));
    };
    let options = Options::parse(rest, &[]);
    let repo = current_directory()?;
    let base = options.value("base").unwrap_or("HEAD~1");
    match mode.as_str() {
        "canned" => {
            let changed = presubmit::change(&repo, base)?;
            let max_len = options.value("max-len").and_then(|value| value.parse().ok()).unwrap_or(80);
            let findings = presubmit::run_canned(&changed, max_len);
            print!("{}", presubmit::render(&findings));
            Ok(u8::from(!findings.is_empty()))
        }
        "python" => {
            let depot_tools = presubmit::find_depot_tools(&repo, options.value("depot-tools"))
                .ok_or_else(|| Error::message("no depot_tools with presubmit_support.py; pass --depot-tools DIR or set DEPOT_TOOLS"))?;
            let code = presubmit::run_python(&repo, &depot_tools, base, &options.positional)?;
            Ok(u8::try_from(code).unwrap_or(1))
        }
        other => Err(Error::message(format!("unknown presubmit mode {other}"))),
    }
}

fn repo(arguments: &[String]) -> Result<u8> {
    use crate::repo_manifest as manifest;
    let [subcommand, rest @ ..] = arguments else {
        return Err(Error::message("usage: fw depot repo (init|sync|list|manifest)"));
    };
    let options = Options::parse(rest, &["partial", "shallow", "metadata-only", "dry-run"]);
    let top = current_directory()?;
    let groups: Vec<String> = options
        .value("g")
        .or(options.value("groups"))
        .map(|text| text.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    match subcommand.as_str() {
        "init" => {
            let url = options.value("u").ok_or_else(|| Error::message("repo init needs -u URL"))?;
            manifest::init(&top, url, options.value("b").unwrap_or("main"), options.value("m").unwrap_or("default.xml"))?;
        }
        "list" => {
            let (loaded, _) = manifest::load_checkout(&top)?;
            for line in manifest::list(&loaded, &groups) {
                println!("{line}");
            }
        }
        "manifest" => {
            let (loaded, url) = manifest::load_checkout(&top)?;
            for project in loaded.select(&groups) {
                let resolved = loaded.resolve(project, &url)?;
                println!("{}\t{}\t{}\t{}", project.path, project.name, resolved.url, resolved.revision);
            }
        }
        "sync" => {
            let settings = manifest::SyncOptions {
                jobs: options.value("j").and_then(|jobs| jobs.parse().ok()).unwrap_or(8),
                groups,
                shallow: options.has("shallow"),
                partial: options.has("partial"),
                metadata_only: options.has("metadata-only"),
                dry_run: options.has("dry-run"),
                only: options.positional.clone(),
            };
            let started = std::time::Instant::now();
            let synced = manifest::sync(&top, &settings)?;
            for project in &synced {
                println!("{} {} {}", project.path, project.revision, project.commit);
            }
            eprintln!("synced {} projects in {:.1}s", synced.len(), started.elapsed().as_secs_f64());
        }
        other => return Err(Error::message(format!("unknown repo command {other}"))),
    }
    Ok(0)
}

fn lunch(arguments: &[String]) -> Result<u8> {
    let options = Options::parse(arguments, &[]);
    let start = match options.value("top") {
        Some(top) => PathBuf::from(top),
        None => current_directory()?,
    };
    let top = crate::envsetup::gettop(&start, std::env::var("TOP").ok().as_deref()).ok_or_else(|| {
        Error::message("Couldn't locate the top of the tree. Run it from the root of the tree or pass --top.")
    })?;
    let result = crate::envsetup::lunch(
        &options.positional,
        &top,
        std::env::var("OUT_DIR").ok().as_deref(),
        std::env::var("OUT_DIR_COMMON_BASE").ok().as_deref(),
    )?;
    print!("{}", result.export_script(&std::env::var("PATH").unwrap_or_default()));
    for note in &result.unresolved {
        eprintln!("not computed without Soong: {note}");
    }
    Ok(0)
}
