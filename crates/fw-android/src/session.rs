//! The host session around the container: the compositor process, its sockets and the commands that talk to it.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::state::State;

/// `$FW_ANDROID_WL`, else `fw-android-wl` next to this executable, else on PATH.
pub fn compositor_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("FW_ANDROID_WL") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(sibling) = exe.parent().map(|directory| directory.join("fw-android-wl")) {
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("fw-android-wl")
}

pub fn start_compositor(state: &State, size: &str) -> Result<u32, String> {
    fs::create_dir_all(state.run()).map_err(|cause| cause.to_string())?;
    fs::create_dir_all(state.logs()).map_err(|cause| cause.to_string())?;
    let log = fs::File::create(state.logs().join("compositor.log")).map_err(|cause| cause.to_string())?;
    let log_err = log.try_clone().map_err(|cause| cause.to_string())?;
    let mut command = Command::new(compositor_binary());
    command
        .arg("--socket")
        .arg(state.wayland_socket())
        .arg("--ctl")
        .arg(state.control_socket())
        .args(["--size", size])
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_err);
    // SAFETY: setsid is async-signal-safe and runs in the forked child before exec.
    unsafe {
        command.pre_exec(|| nix::unistd::setsid().map(|_| ()).map_err(std::io::Error::from));
    }
    let child = command.spawn().map_err(|cause| format!("spawn {}: {cause}", compositor_binary().display()))?;
    fs::write(state.compositor_pid(), child.id().to_string()).map_err(|cause| cause.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !state.wayland_socket().exists() {
        if Instant::now() > deadline {
            return Err(format!("compositor did not create {} (see {})", state.wayland_socket().display(), state.logs().join("compositor.log").display()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(child.id())
}

pub fn stop_compositor(state: &State) {
    if let Some(pid) = fs::read_to_string(state.compositor_pid()).ok().and_then(|text| text.trim().parse::<i32>().ok()) {
        let _signalled = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGTERM);
    }
    for path in [state.compositor_pid(), state.control_socket(), state.wayland_socket()] {
        let _removed = fs::remove_file(path);
    }
}

/// One command line to the compositor's control socket, the reply is one line.
pub fn control(socket: &Path, line: &str) -> Result<String, String> {
    let mut stream = UnixStream::connect(socket).map_err(|cause| format!("{}: {cause}", socket.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(20))).map_err(|cause| cause.to_string())?;
    stream.write_all(format!("{line}\n").as_bytes()).map_err(|cause| cause.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).map_err(|cause| cause.to_string())?;
    Ok(reply.trim().to_owned())
}
