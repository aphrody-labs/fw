//! Where the guest lives on disk. One directory, every object a file: images, mounts, data, sockets, logs.

use std::path::{Path, PathBuf};

pub const CONTAINER_ID: &str = "fw-android";

#[derive(Debug, Clone)]
pub struct State {
    root: PathBuf,
}

impl State {
    /// `$FW_ANDROID_HOME`, else `/var/lib/fw-android`.
    pub fn discover() -> Self {
        let root = std::env::var_os("FW_ANDROID_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/var/lib/fw-android"));
        Self { root }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn images(&self) -> PathBuf {
        self.root.join("images")
    }
    pub fn mounts(&self, role: &str) -> PathBuf {
        self.root.join("mnt").join(role)
    }
    pub fn overlay(&self, role: &str, part: &str) -> PathBuf {
        self.root.join("overlay").join(format!("{role}-{part}"))
    }
    pub fn rootfs(&self) -> PathBuf {
        self.root.join("rootfs")
    }
    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }
    pub fn binderfs(&self) -> PathBuf {
        self.root.join("binderfs")
    }
    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }
    pub fn wayland_socket(&self) -> PathBuf {
        self.run().join("wayland-0")
    }
    pub fn control_socket(&self) -> PathBuf {
        self.run().join("compositor.sock")
    }
    pub fn container_root(&self) -> PathBuf {
        self.root.join("container")
    }
    pub fn bundle(&self) -> PathBuf {
        self.root.join("bundle")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn compositor_pid(&self) -> PathBuf {
        self.run().join("compositor.pid")
    }
}
