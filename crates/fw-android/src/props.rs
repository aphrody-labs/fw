//! The guest property file (`/vendor/waydroid.prop`): what the host tells Android about itself and its graphics.

pub struct Options {
    pub user: String,
    pub uid: u32,
    pub gid: u32,
    pub data_path: String,
    pub multi_windows: bool,
    pub density: Option<u32>,
    /// A host render node the guest may use; without one the guest renders in software (swiftshader).
    pub render_node: Option<String>,
}

pub fn render(options: &Options) -> String {
    let mut props = vec![
        "sys.use_memfd=true".to_owned(),
        "ro.adb.secure=1".to_owned(),
        "ro.debuggable=0".to_owned(),
        "ro.opengles.version=196610".to_owned(),
        "waydroid.updater.disabled=true".to_owned(),
        format!("waydroid.tools_version=fw-android-{}", env!("CARGO_PKG_VERSION")),
    ];
    match &options.render_node {
        Some(node) => {
            props.push("ro.hardware.gralloc=gbm".into());
            props.push("ro.hardware.egl=mesa".into());
            props.push(format!("gralloc.gbm.device={node}"));
        }
        None => {
            props.push("ro.hardware.gralloc=default".into());
            props.push("ro.hardware.egl=swiftshader".into());
        }
    }
    props.push("debug.stagefright.ccodec=0".into());
    props.push(format!("waydroid.host.user={}", options.user));
    props.push(format!("waydroid.host.uid={}", options.uid));
    props.push(format!("waydroid.host.gid={}", options.gid));
    props.push(format!("waydroid.host_data_path={}", options.data_path.replace("/mnt/", "/mnt_extra/")));
    props.push("waydroid.background_start=true".into());
    props.push("waydroid.xdg_runtime_dir=/run/xdg".into());
    props.push("waydroid.pulse_runtime_path=/run/xdg/pulse".into());
    props.push("waydroid.wayland_display=wayland-0".into());
    props.push("waydroid.stub_sensors_hal=1".into());
    if let Some(density) = options.density {
        props.push(format!("ro.sf.lcd_density={density}"));
    }
    if options.multi_windows {
        props.push("persist.waydroid.multi_windows=true".into());
    }
    let mut text = props.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(render_node: Option<&str>) -> Options {
        Options { user: "fw".into(), uid: 1000, gid: 1000, data_path: "/var/lib/fw-android/data".into(), multi_windows: true, density: Some(240), render_node: render_node.map(str::to_owned) }
    }

    #[test]
    fn software_guest_uses_swiftshader_and_memfd() {
        let text = render(&options(None));
        assert!(text.contains("sys.use_memfd=true\n"));
        assert!(text.contains("ro.hardware.gralloc=default\n") && text.contains("ro.hardware.egl=swiftshader\n"));
        assert!(text.contains("persist.waydroid.multi_windows=true\n") && text.contains("ro.sf.lcd_density=240\n"));
    }

    #[test]
    fn render_node_switches_the_guest_to_gbm() {
        let text = render(&options(Some("/dev/dri/renderD128")));
        assert!(text.contains("ro.hardware.gralloc=gbm\n") && text.contains("gralloc.gbm.device=/dev/dri/renderD128\n"));
        assert!(!text.contains("swiftshader"));
    }
}
