use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

struct Compositor {
    child: Child,
}

impl Drop for Compositor {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            if self.child.kill().is_err() {
                eprintln!("compositor already gone");
            }
        }
        if self.child.wait().is_err() {
            eprintln!("compositor wait failed");
        }
    }
}

#[derive(Default)]
struct Client {
    configured: bool,
    configure_serial: u32,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Client {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for Client {
    fn event(_: &mut Self, base: &xdg_wm_base::XdgWmBase, event: xdg_wm_base::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for Client {
    fn event(state: &mut Self, _: &xdg_surface::XdgSurface, event: xdg_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_surface::Event::Configure { serial } = event {
            state.configured = true;
            state.configure_serial = serial;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for Client {
    fn event(_: &mut Self, _: &xdg_toplevel::XdgToplevel, _: xdg_toplevel::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_buffer::WlBuffer);

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(Instant::now() < deadline, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn control(path: &Path, command: &str) -> String {
    let mut stream = UnixStream::connect(path).expect("control connect");
    writeln!(stream, "{command}").expect("control write");
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).expect("control read");
    reply.trim().to_string()
}

fn pixel(image: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width + x) * 4;
    [image[at], image[at + 1], image[at + 2], image[at + 3]]
}

#[test]
fn toplevel_with_red_shm_buffer_shows_in_screenshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("wayland-fw");
    let ctl = dir.path().join("ctl.sock");
    let compositor = Compositor {
        child: Command::new(env!("CARGO_BIN_EXE_fw-android-wl"))
            .args(["--socket", socket.to_str().expect("utf8"), "--ctl", ctl.to_str().expect("utf8"), "--size", "640x360"])
            .stderr(Stdio::inherit())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn compositor"),
    };
    wait_for(&socket);
    wait_for(&ctl);

    let connection = Connection::from_socket(UnixStream::connect(&socket).expect("wayland connect")).expect("connection");
    let (globals, mut queue) = registry_queue_init::<Client>(&connection).expect("registry");
    let handle = queue.handle();
    let mut client = Client::default();

    let wl_compositor: wl_compositor::WlCompositor = globals.bind(&handle, 4..=5, ()).expect("wl_compositor");
    let shm: wl_shm::WlShm = globals.bind(&handle, 1..=1, ()).expect("wl_shm");
    let wm_base: xdg_wm_base::XdgWmBase = globals.bind(&handle, 1..=1, ()).expect("xdg_wm_base");
    for interface in ["wl_subcompositor", "wl_seat", "wl_output", "wp_viewporter", "wp_presentation", "zwp_linux_dmabuf_v1"] {
        assert!(globals.contents().with_list(|list| list.iter().any(|global| global.interface == interface)), "missing global {interface}");
    }

    let surface = wl_compositor.create_surface(&handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
    let toplevel = xdg_surface.get_toplevel(&handle, ());
    toplevel.set_title("fw test".into());
    toplevel.set_app_id("fw.test".into());
    surface.commit();
    while !client.configured {
        queue.blocking_dispatch(&mut client).expect("dispatch");
    }
    xdg_surface.ack_configure(client.configure_serial);

    let file = tempfile::tempfile().expect("tempfile");
    file.set_len(64 * 64 * 4).expect("set_len");
    let red = [0x00u8, 0x00, 0xFF, 0xFF].repeat(64 * 64);
    (&file).write_all(&red).expect("fill");
    let pool = shm.create_pool(std::os::fd::AsFd::as_fd(&file), 64 * 64 * 4, &handle, ());
    let buffer = pool.create_buffer(0, 64, 64, 64 * 4, wl_shm::Format::Argb8888, &handle, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.damage(0, 0, 64, 64);
    surface.commit();
    queue.roundtrip(&mut client).expect("roundtrip");

    let windows: serde_json::Value = serde_json::from_str(&control(&ctl, "windows")).expect("windows json");
    let entry = &windows[0];
    assert_eq!(windows.as_array().map(Vec::len), Some(1), "{windows}");
    assert_eq!(entry["app_id"], "fw.test");
    assert_eq!(entry["title"], "fw test");
    assert_eq!(entry["w"], 64);
    assert_eq!(entry["h"], 64);
    assert_eq!(entry["mapped"], true);
    let (x, y) = (entry["x"].as_u64().expect("x") as usize, entry["y"].as_u64().expect("y") as usize);

    let shot = dir.path().join("shot.png");
    let reply = control(&ctl, &format!("screenshot {}", shot.display()));
    assert_eq!(reply, "ok 640x360 windows=1");

    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&shot).expect("open png")));
    let mut reader = decoder.read_info().expect("png info");
    let mut image = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut image).expect("png frame");
    assert_eq!((info.width, info.height), (640, 360));
    assert_eq!(pixel(&image, 640, x + 32, y + 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 640, x + 63, y + 63), [255, 0, 0, 255]);
    assert_ne!(pixel(&image, 640, x + 100, y + 100), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 640, 639, 359), [0x20, 0x21, 0x24, 255]);

    assert_eq!(control(&ctl, "pointer 10 10 none"), "ok");
    assert_eq!(control(&ctl, "key 30 down"), "ok");
    assert_eq!(control(&ctl, "key 30 up"), "ok");
    assert!(control(&ctl, "bogus").starts_with("err"));

    drop(compositor);
}
