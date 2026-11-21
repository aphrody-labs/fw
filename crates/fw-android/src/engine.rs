//! The container lifecycle through youki's libcontainer: create and start the guest's init, run commands inside it,
//! stop it. The runtime root and the bundle live in the state directory, nothing under /run/youki.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::os::fd::OwnedFd;
use std::path::PathBuf;

use libcontainer::container::builder::ContainerBuilder;
use libcontainer::container::tenant_builder::TenantContainerBuilder;
use libcontainer::container::{Container, ContainerStatus};
use libcontainer::signal::Signal;
use libcontainer::syscall::syscall::SyscallType;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::pipe;
use serde_json::Value;

use crate::state::{CONTAINER_ID, State};

const ANDROID_ENV: [(&str, &str); 9] = [
    ("PATH", "/product/bin:/apex/com.android.runtime/bin:/apex/com.android.art/bin:/system_ext/bin:/system/bin:/system/xbin:/odm/bin:/vendor/bin:/vendor/xbin"),
    ("ANDROID_ROOT", "/system"),
    ("ANDROID_DATA", "/data"),
    ("ANDROID_STORAGE", "/storage"),
    ("ANDROID_ART_ROOT", "/apex/com.android.art"),
    ("ANDROID_I18N_ROOT", "/apex/com.android.i18n"),
    ("ANDROID_TZDATA_ROOT", "/apex/com.android.tzdata"),
    ("ANDROID_RUNTIME_ROOT", "/apex/com.android.runtime"),
    ("BOOTCLASSPATH", "/apex/com.android.art/javalib/core-oj.jar:/apex/com.android.art/javalib/core-libart.jar:/apex/com.android.art/javalib/core-icu4j.jar:/apex/com.android.art/javalib/okhttp.jar:/apex/com.android.art/javalib/bouncycastle.jar:/apex/com.android.art/javalib/apache-xml.jar:/system/framework/framework.jar:/system/framework/ext.jar:/system/framework/telephony-common.jar:/system/framework/voip-common.jar:/system/framework/ims-common.jar:/system/framework/framework-atb-backward-compatibility.jar:/apex/com.android.conscrypt/javalib/conscrypt.jar:/apex/com.android.media/javalib/updatable-media.jar:/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar:/apex/com.android.os.statsd/javalib/framework-statsd.jar:/apex/com.android.permission/javalib/framework-permission.jar:/apex/com.android.sdkext/javalib/framework-sdkextensions.jar:/apex/com.android.wifi/javalib/framework-wifi.jar:/apex/com.android.tethering/javalib/framework-tethering.jar"),
];

/// Android keeps CLASSPATH and SYSTEMSERVERCLASSPATH in a file the init generates; `pm`, `am` and friends need them.
const WITH_CLASSPATH: &str = r#"if [ -r /data/system/environ/classpath ]; then while read -r _ name value; do case "$name" in CLASSPATH|SYSTEMSERVERCLASSPATH) export "$name=$value";; esac; done < /data/system/environ/classpath; fi; exec "$@""#;

fn error(context: &str, cause: impl std::fmt::Display) -> String {
    format!("{context}: {cause}")
}

fn builder(state: &State) -> Result<ContainerBuilder, String> {
    let root = state.container_root();
    fs::create_dir_all(&root).map_err(|cause| error(&root.display().to_string(), cause))?;
    ContainerBuilder::new(CONTAINER_ID.to_owned(), SyscallType::default())
        .with_root_path(root)
        .map_err(|cause| error("runtime root", cause))
}

pub fn load(state: &State) -> Option<Container> {
    Container::load(state.container_root().join(CONTAINER_ID)).ok()
}

pub fn status(state: &State) -> ContainerStatus {
    load(state)
        .map(|mut container| {
            if container.refresh_status().is_err() {
                return ContainerStatus::Stopped;
            }
            container.status()
        })
        .unwrap_or(ContainerStatus::Stopped)
}

pub fn init_pid(state: &State) -> Option<i32> {
    load(state).and_then(|container| container.pid()).map(|pid| pid.as_raw())
}

pub fn create_and_start(state: &State, spec: &Value) -> Result<(), String> {
    let bundle: PathBuf = state.bundle();
    fs::create_dir_all(&bundle).map_err(|cause| error(&bundle.display().to_string(), cause))?;
    let text = serde_json::to_string_pretty(spec).map_err(|cause| cause.to_string())?;
    fs::write(bundle.join("config.json"), text).map_err(|cause| error("config.json", cause))?;
    fs::create_dir_all(state.logs()).map_err(|cause| error("logs", cause))?;
    let log_path = state.logs().join("init.log");
    let log = fs::File::create(&log_path).map_err(|cause| error(&log_path.display().to_string(), cause))?;
    let log_errors = log.try_clone().map_err(|cause| error(&log_path.display().to_string(), cause))?;
    let mut container = builder(state)?
        .with_stdout(log)
        .with_stderr(log_errors)
        .as_init(&bundle)
        .with_systemd(false)
        .with_detach(true)
        .build()
        .map_err(|cause| error("create container", cause))?;
    container.start().map_err(|cause| error("start container", cause))
}

pub fn stop(state: &State) -> Result<(), String> {
    let Some(mut container) = load(state) else { return Ok(()) };
    container.refresh_status().map_err(|cause| error("refresh status", cause))?;
    if container.can_kill() {
        let signal = Signal::try_from("SIGKILL").map_err(|cause| error("SIGKILL", format!("{cause:?}")))?;
        container.kill(signal, true).map_err(|cause| error("kill", cause))?;
        for _ in 0..50 {
            container.refresh_status().map_err(|cause| error("refresh status", cause))?;
            if container.status() == ContainerStatus::Stopped {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    container.delete(true).map_err(|cause| error("delete container", cause))
}

pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
}

fn tenant(state: &State, command: &[String], stdout: Option<OwnedFd>) -> Result<TenantContainerBuilder, String> {
    let mut base = builder(state)?;
    if let Some(descriptor) = stdout {
        base = base.with_stdout(descriptor);
    }
    let mut args: Vec<String> = vec!["/system/bin/sh".into(), "-c".into(), WITH_CLASSPATH.into(), "sh".into()];
    args.extend(command.iter().cloned());
    let environment: HashMap<String, String> = ANDROID_ENV.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect();
    Ok(base.as_tenant().with_detach(false).with_env(environment).with_container_args(args))
}

/// Runs `command` inside the guest with Android's environment. With `capture` the standard output is returned,
/// otherwise it goes to this process's standard output.
pub fn exec(state: &State, command: &[String], capture: bool) -> Result<Output, String> {
    if command.is_empty() {
        return Err("empty command".into());
    }
    let (reader, writer) = if capture {
        let (read_end, write_end) = pipe().map_err(|cause| error("pipe", cause))?;
        (Some(read_end), Some(write_end))
    } else {
        (None, None)
    };
    let pid = tenant(state, command, writer)?.build().map_err(|cause| error("exec in container", cause))?;
    let mut stdout = Vec::new();
    if let Some(read_end) = reader {
        fs::File::from(read_end).read_to_end(&mut stdout).map_err(|cause| error("read output", cause))?;
    }
    let code = match waitpid(pid, None) {
        Ok(WaitStatus::Exited(_, code)) => code,
        Ok(WaitStatus::Signaled(_, signal, _)) => 128 + signal as i32,
        Ok(_) | Err(nix::errno::Errno::ECHILD) => 0,
        Err(cause) => return Err(error("wait", cause)),
    };
    Ok(Output { code, stdout })
}
