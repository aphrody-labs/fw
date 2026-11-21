//! The OCI runtime config of the guest. Android's init is pid 1 of a privileged container with a private binderfs,
//! the host render and input nodes it needs, a tmpfs /dev and the compositor's Wayland socket.

use std::path::Path;

use serde_json::{Value, json};

use crate::state::State;

const CAPABILITIES: [&str; 24] = [
    "CAP_AUDIT_CONTROL", "CAP_SYS_NICE", "CAP_WAKE_ALARM", "CAP_SETPCAP", "CAP_SETGID", "CAP_SETUID", "CAP_SYS_PTRACE", "CAP_SYS_ADMIN",
    "CAP_BLOCK_SUSPEND", "CAP_SYS_TIME", "CAP_NET_ADMIN", "CAP_NET_RAW", "CAP_NET_BIND_SERVICE", "CAP_KILL", "CAP_DAC_OVERRIDE",
    "CAP_DAC_READ_SEARCH", "CAP_FSETID", "CAP_MKNOD", "CAP_SYSLOG", "CAP_CHOWN", "CAP_SYS_RESOURCE", "CAP_FOWNER", "CAP_IPC_LOCK", "CAP_SYS_CHROOT",
];

fn mount_entry(kind: &str, source: &str, destination: &str, options: &[&str]) -> Value {
    json!({ "destination": destination, "type": kind, "source": source, "options": options })
}

fn bind(source: &Path, destination: &str, options: &[&str]) -> Value {
    mount_entry("bind", &source.display().to_string(), destination, options)
}

pub struct Devices {
    pub render_nodes: Vec<String>,
}

pub fn build(state: &State, devices: &Devices, network_namespace: Option<&Path>) -> Value {
    let mut mounts = vec![
        mount_entry("proc", "proc", "/proc", &["nosuid", "noexec", "nodev"]),
        mount_entry("sysfs", "sysfs", "/sys", &["nosuid", "noexec", "nodev", "ro"]),
        mount_entry("cgroup2", "cgroup2", "/sys/fs/cgroup", &["nosuid", "noexec", "nodev", "ro"]),
        mount_entry("tmpfs", "tmpfs", "/dev", &["nosuid", "mode=755"]),
        mount_entry("devpts", "devpts", "/dev/pts", &["nosuid", "noexec", "newinstance", "ptmxmode=0666", "mode=0644"]),
        mount_entry("tmpfs", "tmpfs", "/tmp", &["nodev"]),
        mount_entry("tmpfs", "tmpfs", "/var", &["nodev"]),
        mount_entry("tmpfs", "tmpfs", "/run", &["nodev"]),
        mount_entry("tmpfs", "tmpfs", "/mnt_extra", &["nodev"]),
        bind(&state.data(), "/data", &["rbind"]),
        bind(&state.binderfs().join("binder"), "/dev/binder", &["bind"]),
        bind(&state.binderfs().join("vndbinder"), "/dev/vndbinder", &["bind"]),
        bind(&state.binderfs().join("hwbinder"), "/dev/hwbinder", &["bind"]),
        bind(&state.wayland_socket(), "/run/xdg/wayland-0", &["bind"]),
    ];
    // null, zero, full, tty, random and urandom are created by the runtime itself; binding them as well makes mknod fail.
    for node in ["/dev/fuse", "/dev/uhid", "/dev/ashmem", "/dev/ion", "/dev/sw_sync"] {
        if Path::new(node).exists() {
            mounts.push(bind(Path::new(node), node, &["bind"]));
        }
    }
    if Path::new("/dev/net/tun").exists() {
        mounts.push(bind(Path::new("/dev/net/tun"), "/dev/tun", &["bind"]));
    }
    for node in std::fs::read_dir("/dev/dma_heap").into_iter().flatten().flatten() {
        let path = node.path();
        mounts.push(bind(&path, &path.display().to_string(), &["bind"]));
    }
    for node in &devices.render_nodes {
        mounts.push(bind(Path::new(node), node, &["bind"]));
    }
    if Path::new("/sys/kernel/debug/sync").exists() {
        mounts.push(bind(Path::new("/sys/kernel/debug"), "/sys/kernel/debug", &["rbind"]));
    }

    let mut namespaces = vec![json!({"type": "pid"}), json!({"type": "ipc"}), json!({"type": "uts"}), json!({"type": "mount"}), json!({"type": "cgroup"})];
    namespaces.push(match network_namespace {
        Some(path) => json!({"type": "network", "path": path.display().to_string()}),
        None => json!({"type": "network"}),
    });

    json!({
        "ociVersion": "1.0.2-dev",
        "process": {
            "terminal": false,
            "user": {"uid": 0, "gid": 0},
            "args": ["/init"],
            "env": [],
            "cwd": "/",
            "capabilities": {
                "bounding": CAPABILITIES, "effective": CAPABILITIES, "permitted": CAPABILITIES, "inheritable": CAPABILITIES, "ambient": CAPABILITIES,
            },
            "noNewPrivileges": true
        },
        "root": {"path": state.rootfs().display().to_string(), "readonly": false},
        "hostname": "fw-android",
        "mounts": mounts,
        "linux": {
            "cgroupsPath": "fw-android/guest",
            "namespaces": namespaces
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_runs_init_with_binder_devices_and_a_private_pid_namespace() {
        let state = State::at("/var/lib/fw-android");
        let spec = build(&state, &Devices { render_nodes: vec![] }, None);
        assert_eq!(spec["process"]["args"][0], "/init");
        let destinations: Vec<&str> = spec["mounts"].as_array().expect("mounts").iter().filter_map(|mount| mount["destination"].as_str()).collect();
        for expected in ["/dev/binder", "/dev/vndbinder", "/dev/hwbinder", "/data", "/run/xdg/wayland-0"] {
            assert!(destinations.contains(&expected), "{expected} missing");
        }
        let dev = destinations.iter().position(|destination| *destination == "/dev").expect("dev");
        let binder = destinations.iter().position(|destination| *destination == "/dev/binder").expect("binder");
        assert!(dev < binder, "tmpfs /dev must be mounted before the nodes bound into it");
        let types: Vec<&str> = spec["linux"]["namespaces"].as_array().expect("namespaces").iter().filter_map(|namespace| namespace["type"].as_str()).collect();
        assert!(types.contains(&"pid") && types.contains(&"network") && types.contains(&"cgroup"));
    }
}
