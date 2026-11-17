//! `ay-chromium`: screenshot and evaluate from the command line, and the helper executable CEF re-launches.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use ay_chromium::{Chromium, Config, EvaluateOptions, ScreenshotOptions, install_cef};

const USAGE: &str = "usage:
  ay-chromium screenshot <url> <out.png> [--width N] [--height N] [--wait MS] [--timeout S]
  ay-chromium eval <url> <expression> [--wait MS] [--timeout S]
  ay-chromium install [dir]
  ay-chromium version";

fn main() -> ExitCode {
    if let Some(code) = ay_chromium::subprocess_main() {
        return ExitCode::from(code.clamp(0, 255) as u8);
    }
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ay-chromium: {message}");
            ExitCode::FAILURE
        }
    }
}

struct Parsed {
    positional: Vec<String>,
    width: Option<u32>,
    height: Option<u32>,
    wait: Option<Duration>,
    timeout: Option<Duration>,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut parsed = Parsed {
        positional: vec![],
        width: None,
        height: None,
        wait: None,
        timeout: None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut number = |name: &str| -> Result<u64, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| format!("{name} needs a number\n{USAGE}"))
        };
        match arg.as_str() {
            "--width" => parsed.width = Some(number("--width")? as u32),
            "--height" => parsed.height = Some(number("--height")? as u32),
            "--wait" => parsed.wait = Some(Duration::from_millis(number("--wait")?)),
            "--timeout" => parsed.timeout = Some(Duration::from_secs(number("--timeout")?)),
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option {flag}\n{USAGE}"));
            }
            _ => parsed.positional.push(arg.clone()),
        }
    }
    Ok(parsed)
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some((command, rest)) = args.split_first() else {
        return Err(USAGE.into());
    };
    let parsed = parse(rest)?;
    let err = |e: ay_chromium::Error| e.to_string();
    match (command.as_str(), parsed.positional.as_slice()) {
        ("screenshot", [url, out]) => {
            let mut options = ScreenshotOptions::default();
            options.width = parsed.width.unwrap_or(options.width);
            options.height = parsed.height.unwrap_or(options.height);
            options.settle = parsed.wait.unwrap_or(options.settle);
            options.timeout = parsed.timeout.unwrap_or(options.timeout);
            let chromium = Chromium::start(Config::default()).map_err(err)?;
            let shot = chromium.screenshot(url, out, &options).map_err(err)?;
            println!(
                "{}",
                serde_json::to_string(&shot).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        ("eval", [url, expression]) => {
            let mut options = EvaluateOptions::default();
            options.settle = parsed.wait.unwrap_or(options.settle);
            options.timeout = parsed.timeout.unwrap_or(options.timeout);
            let chromium = Chromium::start(Config::default()).map_err(err)?;
            let value = chromium.evaluate(url, expression, &options).map_err(err)?;
            println!("{value}");
            Ok(())
        }
        ("install", []) | ("install", [_]) => {
            let base = parsed
                .positional
                .first()
                .map(PathBuf::from)
                .unwrap_or_else(default_cef_base);
            let dir = install_cef(&base).map_err(err)?;
            println!("{}", dir.path().display());
            Ok(())
        }
        ("version", []) => {
            println!("{}", ay_chromium::cef_version());
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn default_cef_base() -> PathBuf {
    std::env::var_os("CEF_PATH")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share/cef")))
        .unwrap_or_else(|| PathBuf::from("cef"))
}
