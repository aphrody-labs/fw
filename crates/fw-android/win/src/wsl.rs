//! The `fw-android` WSL2 distro: the Hyper-V utility VM that carries the Android guest with GPU-PV (`/dev/dxg`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const DISTRO: &str = "fw-android";
// Placeholder rootfs until the fw Linux rootfs of the Linux side ships; pinned by URL and sha256, replaced with `--rootfs`.
const ROOTFS_URL: &str = "https://cloud-images.ubuntu.com/wsl/releases/24.04/current/ubuntu-noble-wsl-amd64-wsl.rootfs.tar.gz";
const ROOTFS_SHA256: &str = "8251e27ffff381a4af5f41dcb94d867de3e0d9774a9241908ab34555d99315ea";
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const PACKAGES: &str = "mesa-utils vulkan-tools mesa-vulkan-drivers libgl1-mesa-dri libegl1 binutils";

/// Environment that routes Mesa to the host GPU through Microsoft's d3d12 stack; without `GALLIUM_DRIVER` Mesa 25.2 falls
/// back to llvmpipe on WSLg because there is no DRI render node.
pub const GPU_ENVIRONMENT: [(&str, &str); 3] =
    [("LD_LIBRARY_PATH", "/usr/lib/wsl/lib"), ("GALLIUM_DRIVER", "d3d12"), ("MESA_D3D12_DEFAULT_ADAPTER_NAME", "NVIDIA")];

#[derive(Serialize)]
pub struct WslInfo {
    pub version_lines: Vec<String>,
    pub distros: Vec<Distro>,
    pub wslconfig: Option<WslConfig>,
}

#[derive(Serialize)]
pub struct Distro {
    pub name: String,
    pub state: String,
    pub version: String,
}

#[derive(Serialize)]
pub struct WslConfig {
    pub path: String,
    pub kernel: Option<String>,
    pub keys: BTreeMap<String, String>,
}

#[derive(Serialize)]
pub struct GuestInfo {
    pub distro: String,
    pub kernel_release: String,
    pub dev_dxg: bool,
    pub wsl_libs: Vec<String>,
    pub binderfs_registered: bool,
    pub config: BTreeMap<String, String>,
}

fn decode(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[1] == 0 {
        let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn wsl_command() -> Command {
    let mut command = Command::new("wsl.exe");
    command.env("WSL_UTF8", "1").creation_flags(CREATE_NO_WINDOW);
    command
}

pub fn run_wsl(args: &[&str]) -> io::Result<(bool, String, String)> {
    let output = wsl_command().args(args).stdin(Stdio::null()).output()?;
    Ok((output.status.success(), decode(&output.stdout), decode(&output.stderr)))
}

pub fn wslconfig_path() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".wslconfig"))
}

fn read_wslconfig() -> Option<WslConfig> {
    let path = wslconfig_path()?;
    let text = fs::read_to_string(&path).ok()?;
    let mut keys = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with(['#', ';', '['])) {
        if let Some((key, value)) = line.split_once('=') {
            keys.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Some(WslConfig { path: path.display().to_string(), kernel: keys.get("kernel").cloned(), keys })
}

pub fn distros() -> Vec<Distro> {
    let Ok((_, text, _)) = run_wsl(&["--list", "--verbose"]) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.trim().trim_start_matches('*').split_whitespace().collect();
            let [name, state, version] = fields[..] else { return None };
            version.chars().all(|digit| digit.is_ascii_digit()).then(|| Distro {
                name: name.into(),
                state: state.into(),
                version: version.into(),
            })
        })
        .collect()
}

pub fn info() -> WslInfo {
    let version_lines = run_wsl(&["--version"])
        .map(|(_, text, _)| text.lines().map(str::trim).filter(|line| !line.is_empty()).map(String::from).collect())
        .unwrap_or_default();
    WslInfo { version_lines, distros: distros(), wslconfig: read_wslconfig() }
}

pub fn host_nvidia_smi() -> Option<String> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name,driver_version", "--format=csv,noheader"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Runs `argv` in the guest through `wsl.exe --exec` (no login shell) as `user`, with the GPU environment applied.
pub fn guest_command(distro: &str, user: &str, argv: &[&str]) -> Command {
    let mut command = wsl_command();
    command.args(["-d", distro, "-u", user, "--exec", "env"]);
    for (key, value) in GPU_ENVIRONMENT {
        command.arg(format!("{key}={value}"));
    }
    command.args(argv);
    command
}

fn guest_capture(distro: &str, script: &str) -> Option<String> {
    let output = guest_command(distro, "root", &["sh", "-c", script]).stdin(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn guest_info(distro: &str, options: &[&str]) -> GuestInfo {
    let script = "echo kernel=$(uname -r); if [ -c /dev/dxg ]; then echo dxg=1; else echo dxg=0; fi; \
        echo libs=$(ls /usr/lib/wsl/lib 2>/dev/null | tr '\\n' ' '); \
        if grep -qw binder /proc/filesystems; then echo binderfs=1; else echo binderfs=0; fi; \
        echo ---config---; zcat /proc/config.gz 2>/dev/null";
    let text = guest_capture(distro, script).unwrap_or_default();
    let (head, config_text) = text.split_once("---config---\n").unwrap_or((&text, ""));
    let field =
        |name: &str| head.lines().find_map(|line| line.strip_prefix(name)?.strip_prefix('=')).unwrap_or_default().trim().to_string();
    let mut config = BTreeMap::new();
    for option in options {
        let value = config_text
            .lines()
            .find_map(|line| {
                if let Some(rest) = line.strip_prefix(&format!("CONFIG_{option}=")) {
                    return Some(rest.to_string());
                }
                (line == format!("# CONFIG_{option} is not set")).then(|| "not set".to_string())
            })
            .unwrap_or_else(|| "absent".into());
        config.insert((*option).to_string(), value);
    }
    GuestInfo {
        distro: distro.into(),
        kernel_release: field("kernel"),
        dev_dxg: field("dxg") == "1",
        wsl_libs: field("libs").split_whitespace().map(String::from).collect(),
        binderfs_registered: field("binderfs") == "1",
        config,
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("FW_ANDROID_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("fw").join("android")))
        .unwrap_or_else(|| PathBuf::from("fw-android"))
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

fn fetch_rootfs(destination: &Path) -> Result<(), String> {
    if destination.exists() && sha256_file(destination).is_ok_and(|digest| digest == ROOTFS_SHA256) {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let partial = destination.with_extension("part");
    let mut response = ureq::get(ROOTFS_URL).call().map_err(|error| format!("download {ROOTFS_URL}: {error}"))?;
    let mut file = fs::File::create(&partial).map_err(|error| format!("create {}: {error}", partial.display()))?;
    io::copy(&mut response.body_mut().as_reader(), &mut file).map_err(|error| format!("download {ROOTFS_URL}: {error}"))?;
    file.flush().map_err(|error| error.to_string())?;
    drop(file);
    let digest = sha256_file(&partial).map_err(|error| error.to_string())?;
    if digest != ROOTFS_SHA256 {
        fs::remove_file(&partial).ok();
        return Err(format!("rootfs sha256 {digest} does not match the pinned {ROOTFS_SHA256}"));
    }
    fs::rename(&partial, destination).map_err(|error| error.to_string())
}

fn provision(distro: &str) -> Result<(), String> {
    let environment: String = GPU_ENVIRONMENT.iter().map(|(key, value)| format!("export {key}={value}\n")).collect();
    let script = format!(
        "set -e; export DEBIAN_FRONTEND=noninteractive; apt-get update -qq; apt-get install -y -qq {PACKAGES}; \
         printf '%s' '{environment}' > /etc/profile.d/fw-android-gpu.sh"
    );
    let output = guest_command(distro, "root", &["sh", "-c", &script]).stdin(Stdio::null()).output().map_err(|error| error.to_string())?;
    if output.status.success() { Ok(()) } else { Err(format!("provisioning failed: {}", String::from_utf8_lossy(&output.stderr))) }
}

fn keepalive_file() -> PathBuf {
    home().join("keepalive.pid")
}

/// Imports the guest when missing, provisions it, and keeps its utility VM alive with a detached `sleep infinity`.
pub fn up(rootfs: Option<&Path>) -> Result<(), String> {
    if !distros().iter().any(|entry| entry.name == DISTRO) {
        let archive = match rootfs {
            Some(path) => path.to_path_buf(),
            None => {
                let path = home().join("cache").join("rootfs.tar.gz");
                fetch_rootfs(&path)?;
                path
            }
        };
        let location = home().join("distro");
        fs::create_dir_all(&location).map_err(|error| format!("create {}: {error}", location.display()))?;
        let (ok, _, error) = run_wsl(&["--import", DISTRO, &location.to_string_lossy(), &archive.to_string_lossy(), "--version", "2"])
            .map_err(|error| error.to_string())?;
        if !ok {
            return Err(format!("wsl --import failed: {error}"));
        }
        provision(DISTRO)?;
    }
    let pid_file = keepalive_file();
    if let Some(pid) = fs::read_to_string(&pid_file).ok().and_then(|text| text.trim().parse::<u32>().ok())
        && process_alive(pid)
    {
        return Ok(());
    }
    let child = guest_command(DISTRO, "root", &["sleep", "infinity"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| format!("start keepalive: {error}"))?;
    fs::create_dir_all(home()).map_err(|error| error.to_string())?;
    fs::write(&pid_file, child.id().to_string()).map_err(|error| error.to_string())
}

fn process_alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FI", "IMAGENAME eq wsl.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
}

pub fn down() -> Result<(), String> {
    if let Some(pid) = fs::read_to_string(keepalive_file()).ok().and_then(|text| text.trim().parse::<u32>().ok()) {
        Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).creation_flags(CREATE_NO_WINDOW).output().ok();
        fs::remove_file(keepalive_file()).ok();
    }
    let (ok, _, error) = run_wsl(&["--terminate", DISTRO]).map_err(|error| error.to_string())?;
    if ok { Ok(()) } else { Err(format!("wsl --terminate failed: {error}")) }
}

/// Points the global `kernel=` of `%UserProfile%\.wslconfig` at `bzimage` (or removes it with `None`), keeping every other
/// line and a one-time backup beside the file. WSL has no per-distro kernel, so this applies to every WSL2 distro.
pub fn set_kernel(bzimage: Option<&Path>) -> Result<String, String> {
    let path = wslconfig_path().ok_or("USERPROFILE is not set")?;
    let original = fs::read_to_string(&path).unwrap_or_default();
    let backup = path.with_file_name(".wslconfig.fw-android.bak");
    if !backup.exists() {
        fs::write(&backup, &original).map_err(|error| format!("write {}: {error}", backup.display()))?;
    }
    let is_kernel = |line: &str| line.trim_start().strip_prefix("kernel").is_some_and(|rest| rest.trim_start().starts_with('='));
    let mut lines: Vec<String> = original.lines().filter(|line| !is_kernel(line)).map(String::from).collect();
    if let Some(bzimage) = bzimage {
        let entry = format!("kernel={}", bzimage.display().to_string().replace('\\', "\\\\"));
        match lines.iter().position(|line| line.trim().eq_ignore_ascii_case("[wsl2]")) {
            Some(index) => lines.insert(index + 1, entry),
            None => {
                lines.push("[wsl2]".into());
                lines.push(entry);
            }
        }
    }
    fs::write(&path, lines.join("\r\n") + "\r\n").map_err(|error| format!("write {}: {error}", path.display()))?;
    run_wsl(&["--shutdown"]).map_err(|error| error.to_string())?;
    Ok(backup.display().to_string())
}

#[derive(Serialize)]
pub struct GpuReport {
    pub accelerated: bool,
    pub renderer: String,
    pub gl_version: String,
    pub nvidia_smi: String,
    pub renderer_without_environment: String,
}

fn field_after(text: &str, label: &str) -> String {
    text.lines().find_map(|line| line.trim().strip_prefix(label)).unwrap_or_default().trim().to_string()
}

/// Renders through the guest's Mesa and reports which device answered, with and without [`GPU_ENVIRONMENT`].
pub fn gpu_report(distro: &str) -> GpuReport {
    let tuned = guest_command(distro, "root", &["glxinfo", "-B"]).stdin(Stdio::null()).output().ok();
    let tuned = tuned.map(|output| String::from_utf8_lossy(&output.stdout).into_owned()).unwrap_or_default();
    let plain = wsl_command()
        .args(["-d", distro, "-u", "root", "--exec", "glxinfo", "-B"])
        .stdin(Stdio::null())
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    let nvidia_smi =
        guest_command(distro, "root", &["/usr/lib/wsl/lib/nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader"])
            .stdin(Stdio::null())
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default();
    let renderer = field_after(&tuned, "OpenGL renderer string:");
    GpuReport {
        accelerated: field_after(&tuned, "Accelerated:") == "yes" && renderer.contains("NVIDIA"),
        gl_version: field_after(&tuned, "OpenGL version string:"),
        renderer,
        nvidia_smi,
        renderer_without_environment: field_after(&plain, "OpenGL renderer string:"),
    }
}
