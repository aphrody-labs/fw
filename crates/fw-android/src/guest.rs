//! `up` and `down`: from pinned images to a booted Android in a container, and back.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use libcontainer::container::ContainerStatus;

use crate::pins::Pins;
use crate::props::{self, Options};
use crate::spec::{self, Devices};
use crate::state::State;
use crate::{engine, host, image, session};

pub struct UpOptions {
    pub size: String,
    pub multi_windows: bool,
    pub density: Option<u32>,
    pub with_compositor: bool,
    pub render_node: Option<String>,
    pub boot_timeout: Duration,
}

impl Default for UpOptions {
    fn default() -> Self {
        Self { size: "1080x1920".into(), multi_windows: false, density: Some(240), with_compositor: true, render_node: None, boot_timeout: Duration::from_secs(240) }
    }
}

fn log(message: &str) {
    eprintln!("fw-android: {message}");
}

pub fn mount_tree(state: &State) -> Result<(), String> {
    let images = state.images();
    host::mount_image(&images.join("system.img"), &state.mounts("system"))?;
    host::mount_overlay(&state.mounts("system"), &state.overlay("system", "upper"), &state.overlay("system", "work"), &state.rootfs())?;
    host::mount_image(&images.join("vendor.img"), &state.mounts("vendor"))?;
    host::mount_overlay(&state.mounts("vendor"), &state.overlay("vendor", "upper"), &state.overlay("vendor", "work"), &state.rootfs().join("vendor"))
}

pub fn unmount_tree(state: &State) -> Result<(), String> {
    host::unmount(&state.rootfs().join("vendor"))?;
    host::unmount(&state.rootfs())?;
    host::unmount(&state.mounts("vendor"))?;
    host::unmount(&state.mounts("system"))?;
    host::unmount(&state.binderfs())
}

pub fn up(state: &State, pins: &Pins, options: &UpOptions) -> Result<(), String> {
    if nix::unistd::geteuid().as_raw() != 0 {
        return Err("fw android up needs root (mounts, binderfs, container): run it with sudo".into());
    }
    if engine::status(state) == ContainerStatus::Running {
        return Err("the guest is already running; `fw android down` first".into());
    }
    log("images");
    image::ensure(pins, state.root())?;
    log("mounts and binderfs");
    mount_tree(state)?;
    host::setup_binderfs(&state.binderfs())?;
    fs::create_dir_all(state.data()).map_err(|cause| cause.to_string())?;
    let properties = props::render(&Options {
        user: "fw".into(),
        uid: 1000,
        gid: 1000,
        data_path: state.data().display().to_string(),
        multi_windows: options.multi_windows,
        density: options.density,
        render_node: options.render_node.clone(),
    });
    fs::write(state.rootfs().join("vendor/waydroid.prop"), properties).map_err(|cause| format!("waydroid.prop: {cause}"))?;

    session::stop_compositor(state);
    fs::create_dir_all(state.run()).map_err(|cause| cause.to_string())?;
    if options.with_compositor {
        log(&format!("compositor {}", options.size));
        session::start_compositor(state, &options.size)?;
    } else {
        fs::write(state.wayland_socket(), b"").map_err(|cause| cause.to_string())?;
    }

    let devices = Devices { render_nodes: options.render_node.iter().cloned().collect() };
    let config = spec::build(state, &devices, None);
    log("container");
    if let Some(previous) = engine::load(state) {
        drop(previous);
        engine::stop(state)?;
    }
    engine::create_and_start(state, &config)?;
    log(&format!("init pid {}", engine::init_pid(state).unwrap_or(0)));
    wait_boot(state, options.boot_timeout)
}

fn wait_boot(state: &State, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let started = Instant::now();
    loop {
        if engine::status(state) != ContainerStatus::Running {
            return Err(format!("the guest stopped before it booted; see `{} logs`", env!("CARGO_PKG_NAME")));
        }
        if let Ok(output) = engine::exec(state, &["getprop".into(), "sys.boot_completed".into()], true) {
            if String::from_utf8_lossy(&output.stdout).trim() == "1" {
                log(&format!("booted in {:.1} s", started.elapsed().as_secs_f64()));
                return Ok(());
            }
        }
        if Instant::now() > deadline {
            return Err(format!("sys.boot_completed was not set after {} s", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

pub fn down(state: &State) -> Result<(), String> {
    engine::stop(state)?;
    session::stop_compositor(state);
    unmount_tree(state)
}

pub fn image_present(state: &State) -> bool {
    ["system.img", "vendor.img"].iter().all(|name| Path::new(&state.images().join(name)).is_file())
}
