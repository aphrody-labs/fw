//! `fw-android` command line.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::image;
use crate::pins::Pins;
use crate::state::State;

const HELP: &str = "fw-android: Android apps without WSA (run by `fw android`)

usage: fw android <command>
  status [--json] [--offline]     pins against the OTA channel, host prerequisites, guest and compositor state
  fetch                           download, verify (sha256) and unpack the pinned images, write images/LICENSES.json
  up [--size WxH] [--multi-windows] [--density N] [--no-compositor] [--render-node PATH]
                                  mount the images and binderfs, start the compositor, boot the guest (Linux, root)
  down                            stop the guest and the compositor, unmount everything
  install <apk>                   pm install -r inside the guest
  run <package|package/activity>  launch an app
  shell [command...]              run a command in the guest (Android environment); default: sh
  logcat [args...]                logcat inside the guest
  screenshot <out.png>            capture the compositor canvas (the app windows) as PNG
  windows                         the compositor's window list as json
environment: FW_ANDROID_HOME (state directory, default /var/lib/fw-android), FW_ANDROID_PINS, FW_ANDROID_WL";

pub fn run(arguments: &[String]) -> Result<u8, String> {
    let Some((command, rest)) = arguments.split_first() else {
        println!("{HELP}");
        return Ok(0);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(0)
        }
        "status" => status(rest),
        "fetch" => fetch(),
        #[cfg(target_os = "linux")]
        "up" | "down" | "install" | "run" | "shell" | "logcat" | "screenshot" | "windows" => linux::dispatch(command, rest),
        #[cfg(not(target_os = "linux"))]
        "up" | "down" | "install" | "run" | "shell" | "logcat" | "screenshot" | "windows" => {
            Err(format!("`fw android {command}` runs the container on Linux; on Windows use fw-android-win (Hyper-V guest), in a browser use the WebGPU client"))
        }
        other => Err(format!("unknown command {other}; try `fw android help`")),
    }
}

fn fetch() -> Result<u8, String> {
    let pins = Pins::load()?;
    let state = State::discover();
    let paths = image::ensure(&pins, state.root())?;
    for path in paths {
        println!("{}", path.display());
    }
    Ok(0)
}

fn status(arguments: &[String]) -> Result<u8, String> {
    let json_output = arguments.iter().any(|argument| argument == "--json");
    let offline = arguments.iter().any(|argument| argument == "--offline");
    let pins = Pins::load()?;
    let state = State::discover();
    let mut images = Vec::new();
    for (role, channel) in [("system", &pins.channel.system), ("vendor", &pins.channel.vendor)] {
        let pinned = pins.image(role)?;
        let newest = if offline { None } else { Some(image::newest(channel)) };
        let (latest, behind) = match &newest {
            Some(Ok(entry)) => (Some(entry.filename.clone()), Some(entry.id != pinned.sha256)),
            _ => (None, None),
        };
        let present = state.images().join(format!("{role}.img")).is_file();
        images.push(json!({
            "role": role, "pinned": pinned.filename, "sha256": pinned.sha256, "size": pinned.size,
            "unpacked": present, "channel_newest": latest, "behind_channel": behind,
            "channel_error": newest.and_then(Result::err),
        }));
    }
    let mut report = json!({ "state_dir": state.root().display().to_string(), "arch": pins.arch, "images": images });
    #[cfg(target_os = "linux")]
    linux::describe(&state, &mut report);
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report).map_err(|cause| cause.to_string())?);
    } else {
        print_plain(&report);
    }
    Ok(0)
}

fn print_plain(report: &Value) {
    println!("state {}", report["state_dir"].as_str().unwrap_or(""));
    for image in report["images"].as_array().into_iter().flatten() {
        println!(
            "{:<7} {} unpacked={} newest={} behind={}",
            image["role"].as_str().unwrap_or(""), image["pinned"].as_str().unwrap_or(""), image["unpacked"], image["channel_newest"], image["behind_channel"]
        );
    }
    for check in report["host"].as_array().into_iter().flatten() {
        println!("{:<12} {} {}", check["name"].as_str().unwrap_or(""), if check["ok"] == true { "ok " } else { "NO " }, check["detail"].as_str().unwrap_or(""));
    }
    for key in ["guest", "boot_completed", "windows"] {
        if let Some(value) = report.get(key) {
            println!("{key:<12} {value}");
        }
    }
}

pub fn absolute(path: &str) -> Result<PathBuf, String> {
    std::path::absolute(Path::new(path)).map_err(|cause| format!("{path}: {cause}"))
}

#[cfg(target_os = "linux")]
mod linux {
    use std::time::Duration;

    use libcontainer::container::ContainerStatus;
    use serde_json::{Value, json};

    use super::absolute;
    use crate::guest::{self, UpOptions};
    use crate::pins::Pins;
    use crate::state::State;
    use crate::{engine, host, session};

    pub fn describe(state: &State, report: &mut Value) {
        let checks: Vec<Value> = host::prerequisites().into_iter().map(|(name, ok, detail)| json!({"name": name, "ok": ok, "detail": detail})).collect();
        report["host"] = Value::Array(checks);
        let status = engine::status(state);
        report["guest"] = json!(format!("{status:?}").to_lowercase());
        if status == ContainerStatus::Running {
            if let Ok(output) = engine::exec(state, &["getprop".into(), "sys.boot_completed".into()], true) {
                report["boot_completed"] = json!(String::from_utf8_lossy(&output.stdout).trim() == "1");
            }
            if let Ok(windows) = session::control(&state.control_socket(), "windows") {
                report["windows"] = serde_json::from_str(&windows).unwrap_or(json!(windows));
            }
        }
    }

    fn value_of(arguments: &[String], flag: &str) -> Option<String> {
        arguments.iter().position(|argument| argument == flag).and_then(|index| arguments.get(index + 1)).cloned()
    }

    pub fn dispatch(command: &str, rest: &[String]) -> Result<u8, String> {
        let state = State::discover();
        match command {
            "up" => {
                let mut options = UpOptions::default();
                if let Some(size) = value_of(rest, "--size") {
                    options.size = size;
                }
                options.multi_windows = rest.iter().any(|argument| argument == "--multi-windows");
                options.with_compositor = !rest.iter().any(|argument| argument == "--no-compositor");
                options.density = value_of(rest, "--density").and_then(|text| text.parse().ok()).or(options.density);
                options.render_node = value_of(rest, "--render-node");
                if let Some(seconds) = value_of(rest, "--boot-timeout").and_then(|text| text.parse().ok()) {
                    options.boot_timeout = Duration::from_secs(seconds);
                }
                guest::up(&state, &Pins::load()?, &options)?;
                Ok(0)
            }
            "down" => {
                guest::down(&state)?;
                Ok(0)
            }
            "shell" => {
                let command = if rest.is_empty() { vec!["/system/bin/sh".to_owned()] } else { rest.to_vec() };
                Ok(engine::exec(&state, &command, false)?.code as u8)
            }
            "logcat" => {
                let mut command = vec!["/system/bin/logcat".to_owned()];
                command.extend(rest.iter().cloned());
                Ok(engine::exec(&state, &command, false)?.code as u8)
            }
            "install" => {
                let apk = rest.first().ok_or("usage: fw android install <apk>")?;
                let source = absolute(apk)?;
                let name = source.file_name().and_then(|name| name.to_str()).ok_or("apk path has no file name")?;
                let staging = state.data().join("local/tmp/fw-android");
                std::fs::create_dir_all(&staging).map_err(|cause| cause.to_string())?;
                std::fs::copy(&source, staging.join(name)).map_err(|cause| format!("{}: {cause}", source.display()))?;
                let output = engine::exec(&state, &["pm".into(), "install".into(), "-r".into(), format!("/data/local/tmp/fw-android/{name}")], false)?;
                Ok(output.code as u8)
            }
            "run" => {
                let target = rest.first().ok_or("usage: fw android run <package|package/activity>")?;
                let command: Vec<String> = if target.contains('/') {
                    vec!["am".into(), "start".into(), "-n".into(), target.clone()]
                } else {
                    vec!["monkey".into(), "-p".into(), target.clone(), "-c".into(), "android.intent.category.LAUNCHER".into(), "1".into()]
                };
                Ok(engine::exec(&state, &command, false)?.code as u8)
            }
            "screenshot" => {
                let out = absolute(rest.first().ok_or("usage: fw android screenshot <out.png>")?)?;
                println!("{}", session::control(&state.control_socket(), &format!("screenshot {}", out.display()))?);
                Ok(0)
            }
            "windows" => {
                println!("{}", session::control(&state.control_socket(), "windows")?);
                Ok(0)
            }
            other => Err(format!("unknown command {other}")),
        }
    }
}
