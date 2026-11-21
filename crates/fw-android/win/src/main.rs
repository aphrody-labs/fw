//! `fw-android-win`: Windows 11 host side of `fw android`. The guest is the `fw-android` WSL2 distro (a Hyper-V utility VM
//! with GPU-PV); its Wayland compositor streams windows over fwa-stream v1 and `bridge` shows them as Win32 windows.

#[cfg(windows)]
mod bridge;
#[cfg(windows)]
mod probe;
#[cfg(windows)]
mod testserver;
#[cfg(windows)]
mod wsl;

#[cfg(not(windows))]
fn main() {
    eprintln!("fw-android-win runs on Windows 11 only");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&arguments) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("fw-android-win: {message}");
            1
        }
    };
    std::process::exit(code);
}

#[cfg(windows)]
const USAGE: &str = "usage:
  fw-android-win probe [--json] [--distro NAME]
  fw-android-win guest up [--rootfs PATH] | exec [--user NAME] -- COMMAND... | down | status | gpu
  fw-android-win kernel install BZIMAGE | stock
  fw-android-win bridge --connect HOST:PORT
  fw-android-win test-server [--listen HOST:PORT] [--windows N]
  fw-android-win test-bridge [--shot DIR]";

#[cfg(windows)]
fn flag_value<'a>(arguments: &'a [String], name: &str) -> Option<&'a str> {
    let index = arguments.iter().position(|argument| argument == name)?;
    arguments.get(index + 1).map(String::as_str)
}

#[cfg(windows)]
fn run(arguments: &[String]) -> Result<i32, String> {
    let rest = || arguments.get(1..).unwrap_or_default();
    match arguments.first().map(String::as_str) {
        Some("probe") => {
            let distro = flag_value(rest(), "--distro").unwrap_or(wsl::DISTRO);
            let report = probe::run(distro);
            let text = if rest().iter().any(|argument| argument == "--json") {
                serde_json::to_string_pretty(&report)
            } else {
                serde_json::to_string(&report)
            };
            println!("{}", text.map_err(|error| error.to_string())?);
            Ok(0)
        }
        Some("guest") => guest(rest()),
        Some("kernel") => match rest().first().map(String::as_str) {
            Some("install") => {
                let bzimage = rest().get(1).ok_or("kernel install needs the path of a bzImage")?;
                let backup = wsl::set_kernel(Some(std::path::Path::new(bzimage)))?;
                println!("kernel set; WSL shut down; original .wslconfig saved as {backup}; undo with `fw-android-win kernel stock`");
                Ok(0)
            }
            Some("stock") => {
                wsl::set_kernel(None)?;
                println!("kernel= removed from .wslconfig; WSL shut down; stock kernel active at next start");
                Ok(0)
            }
            _ => Err(USAGE.into()),
        },
        Some("bridge") => {
            let address = flag_value(rest(), "--connect").ok_or("bridge needs --connect HOST:PORT")?;
            bridge::run(address)
        }
        Some("test-server") => {
            let listen = flag_value(rest(), "--listen").unwrap_or("127.0.0.1:7870");
            let windows = flag_value(rest(), "--windows").and_then(|text| text.parse().ok()).unwrap_or(2);
            testserver::serve(listen, windows)
        }
        Some("test-bridge") => testserver::self_test(flag_value(rest(), "--shot")),
        _ => Err(USAGE.into()),
    }
}

#[cfg(windows)]
fn guest(arguments: &[String]) -> Result<i32, String> {
    match arguments.first().map(String::as_str) {
        Some("up") => {
            let rootfs = flag_value(arguments, "--rootfs").map(std::path::PathBuf::from);
            wsl::up(rootfs.as_deref())?;
            println!("guest {} is up", wsl::DISTRO);
            Ok(0)
        }
        Some("down") => {
            wsl::down()?;
            println!("guest {} stopped", wsl::DISTRO);
            Ok(0)
        }
        Some("status") => {
            let entry = wsl::distros().into_iter().find(|entry| entry.name == wsl::DISTRO);
            let guest = entry
                .as_ref()
                .filter(|entry| entry.state == "Running")
                .map(|_| wsl::guest_info(wsl::DISTRO, &["ANDROID_BINDER_IPC", "ANDROID_BINDERFS"]));
            let status = serde_json::json!({ "distro": wsl::DISTRO, "wsl": entry, "guest": guest });
            println!("{}", serde_json::to_string_pretty(&status).map_err(|error| error.to_string())?);
            Ok(if entry.is_some() { 0 } else { 1 })
        }
        Some("exec") => {
            let user = flag_value(arguments, "--user").unwrap_or("root");
            let start =
                arguments.iter().position(|argument| argument == "--").map(|index| index + 1).ok_or("guest exec needs -- COMMAND")?;
            let argv: Vec<&str> = arguments[start..].iter().map(String::as_str).collect();
            if argv.is_empty() {
                return Err("guest exec needs a command after --".into());
            }
            let status = wsl::guest_command(wsl::DISTRO, user, &argv).status().map_err(|error| error.to_string())?;
            Ok(status.code().unwrap_or(1))
        }
        Some("gpu") => {
            let report = wsl::gpu_report(wsl::DISTRO);
            println!("{}", serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?);
            Ok(if report.accelerated { 0 } else { 1 })
        }
        _ => Err(USAGE.into()),
    }
}
