#![forbid(unsafe_code)]

use fw_site::{Config, check, generate, write_llms};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: fw site <gen|check|llms> [--root <repo>] [--out <dir>] [--fw <binary>] [--bun <binary>] [--m3 <binary>] [--site <url>] [--live]

gen    write the MDX pages, sidebar, theme colors, llms.txt, robots.txt and the page markdown under apps/docs
check  regenerate in memory and fail on any difference with the files on disk (CI drift gate)
llms   write only llms.txt and llms-full.txt
--live fetch the release manifest and store the snapshot; without it the stored snapshot is read";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("fw site: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run(args: &[String]) -> anyhow::Result<ExitCode> {
    let Some(command) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    if matches!(command, "-h" | "--help" | "help") {
        println!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let flag = |name: &str| {
        args.iter()
            .position(|argument| argument == name)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    let root = match flag("--root") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir()?,
    };
    let mut config = Config::new(root);
    if let Some(out) = flag("--out") {
        config.out = PathBuf::from(out);
    }
    if let Some(fw) = flag("--fw") {
        config.fw = fw;
    }
    if let Some(bun) = flag("--bun") {
        config.bun = bun;
    }
    if let Some(m3) = flag("--m3") {
        config.m3 = m3;
    }
    if let Some(site) = flag("--site").or_else(|| std::env::var("FW_SITE_URL").ok()) {
        config.site = Some(site);
    }
    config.live = args.iter().any(|argument| argument == "--live");
    match command {
        "gen" => {
            let summary = generate(&config)?;
            println!("{summary}");
            Ok(ExitCode::SUCCESS)
        }
        "check" => {
            let problems = check(&config)?;
            if problems.is_empty() {
                println!("fw site check: apps/docs matches the code");
                Ok(ExitCode::SUCCESS)
            } else {
                for problem in &problems {
                    eprintln!("drift: {problem}");
                }
                eprintln!(
                    "fw site check: {} file(s) differ; run `fw site gen`",
                    problems.len()
                );
                Ok(ExitCode::from(1))
            }
        }
        "llms" => {
            println!("{}", write_llms(&config)?);
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("unknown command `{other}`\n{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}
