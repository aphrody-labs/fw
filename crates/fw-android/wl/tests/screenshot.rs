use std::{
    io::{BufRead, BufReader, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use tempfile::TempDir;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface},
};
use wayland_protocols::{
    wp::viewporter::client::{wp_viewport, wp_viewporter},
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

struct Compositor {
    child: Child,
    dir: TempDir,
    socket: PathBuf,
    ctl: PathBuf,
}

impl Compositor {
    fn start(size: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("wayland-fw");
        let ctl = dir.path().join("ctl.sock");
        let child = Command::new(env!("CARGO_BIN_EXE_fw-android-wl"))
            .args(["--socket", socket.to_str().expect("utf8"), "--ctl", ctl.to_str().expect("utf8"), "--size", size])
            .stderr(Stdio::inherit())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn compositor");
        let compositor = Self { child, dir, socket, ctl };
        wait_for(&compositor.socket);
        wait_for(&compositor.ctl);
        compositor
    }

    fn control(&self, command: &str) -> String {
        let mut stream = UnixStream::connect(&self.ctl).expect("control connect");
        writeln!(stream, "{command}").expect("control write");
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).expect("control read");
        reply.trim().to_string()
    }

    fn screenshot(&self, expected_reply: &str) -> Screenshot {
        let path = self.dir.path().join("shot.png");
        assert_eq!(self.control(&format!("screenshot {}", path.display())), expected_reply);
        let decoder = png::Decoder::new(BufReader::new(std::fs::File::open(&path).expect("open png")));
        let mut reader = decoder.read_info().expect("png info");
        let mut pixels = vec![0; reader.output_buffer_size().expect("png size")];
        let info = reader.next_frame(&mut pixels).expect("png frame");
        Screenshot { width: info.width as usize, height: info.height as usize, pixels }
    }
}

impl Drop for Compositor {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            self.child.kill().ok();
        }
        self.child.wait().ok();
    }
}

struct Screenshot {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Screenshot {
    fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let at = (y * self.width + x) * 4;
        [self.pixels[at], self.pixels[at + 1], self.pixels[at + 2], self.pixels[at + 3]]
    }
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(Instant::now() < deadline, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Default)]
struct Client {
    configured: bool,
    configure_serial: u32,
    toplevel_size: (i32, i32),
    maximized: bool,
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
    fn event(state: &mut Self, _: &xdg_toplevel::XdgToplevel, event: xdg_toplevel::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let xdg_toplevel::Event::Configure { width, height, states } = event {
            state.toplevel_size = (width, height);
            state.maximized = states
                .as_chunks::<4>()
                .0
                .iter()
                .any(|chunk| u32::from_ne_bytes(*chunk) == xdg_toplevel::State::Maximized as u32);
        }
    }
}

delegate_noop!(Client: ignore wl_compositor::WlCompositor);
delegate_noop!(Client: ignore wl_surface::WlSurface);
delegate_noop!(Client: ignore wl_shm::WlShm);
delegate_noop!(Client: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Client: ignore wl_buffer::WlBuffer);
delegate_noop!(Client: ignore wp_viewporter::WpViewporter);
delegate_noop!(Client: ignore wp_viewport::WpViewport);

struct Window {
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
}

struct Session {
    client: Client,
    queue: EventQueue<Client>,
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    wm_base: xdg_wm_base::XdgWmBase,
    viewporter: wp_viewporter::WpViewporter,
    _connection: Connection,
}

impl Session {
    fn connect(compositor: &Compositor) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(&compositor.socket).expect("wayland connect")).expect("connection");
        let (globals, queue) = registry_queue_init::<Client>(&connection).expect("registry");
        let handle = queue.handle();
        for interface in ["wl_subcompositor", "wl_seat", "wl_output", "wp_presentation", "zwp_linux_dmabuf_v1", "wp_fractional_scale_manager_v1"] {
            assert!(
                globals.contents().with_list(|list| list.iter().any(|global| global.interface == interface)),
                "missing global {interface}"
            );
        }
        Self {
            compositor: globals.bind(&handle, 4..=5, ()).expect("wl_compositor"),
            shm: globals.bind(&handle, 1..=1, ()).expect("wl_shm"),
            wm_base: globals.bind(&handle, 1..=1, ()).expect("xdg_wm_base"),
            viewporter: globals.bind(&handle, 1..=1, ()).expect("wp_viewporter"),
            client: Client::default(),
            queue,
            _connection: connection,
        }
    }

    fn toplevel(&mut self, title: &str, app_id: &str, maximized: bool) -> Window {
        let handle = self.queue.handle();
        let surface = self.compositor.create_surface(&handle, ());
        let xdg_surface = self.wm_base.get_xdg_surface(&surface, &handle, ());
        let toplevel = xdg_surface.get_toplevel(&handle, ());
        toplevel.set_title(title.into());
        toplevel.set_app_id(app_id.into());
        if maximized {
            toplevel.set_maximized();
        }
        surface.commit();
        self.client.configured = false;
        while !self.client.configured {
            self.queue.blocking_dispatch(&mut self.client).expect("dispatch");
        }
        xdg_surface.ack_configure(self.client.configure_serial);
        Window { surface, xdg_surface, toplevel }
    }

    fn attach_solid(&mut self, window: &Window, width: i32, height: i32, argb: u32, destination: Option<(i32, i32)>) {
        let handle = self.queue.handle();
        let bytes = argb.to_le_bytes().repeat((width * height) as usize);
        let file = tempfile::tempfile().expect("tempfile");
        file.set_len(bytes.len() as u64).expect("set_len");
        (&file).write_all(&bytes).expect("fill");
        let pool = self.shm.create_pool(file.as_fd(), bytes.len() as i32, &handle, ());
        let buffer = pool.create_buffer(0, width, height, width * 4, wl_shm::Format::Argb8888, &handle, ());
        if let Some((dest_width, dest_height)) = destination {
            self.viewporter.get_viewport(&window.surface, &handle, ()).set_destination(dest_width, dest_height);
        }
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage(0, 0, width, height);
        window.surface.commit();
        self.queue.roundtrip(&mut self.client).expect("roundtrip");
    }
}

#[test]
fn toplevel_with_red_shm_buffer_shows_in_screenshot() {
    let compositor = Compositor::start("640x360");
    let mut session = Session::connect(&compositor);
    let window = session.toplevel("fw test", "fw.test", false);
    session.attach_solid(&window, 64, 64, 0xFFFF_0000, None);

    let windows: serde_json::Value = serde_json::from_str(&compositor.control("windows")).expect("windows json");
    assert_eq!(windows.as_array().map(Vec::len), Some(1), "{windows}");
    let entry = &windows[0];
    assert_eq!(entry["app_id"], "fw.test");
    assert_eq!(entry["title"], "fw test");
    assert_eq!(entry["w"], 64);
    assert_eq!(entry["h"], 64);
    assert_eq!(entry["mapped"], true);
    let (x, y) = (entry["x"].as_u64().expect("x") as usize, entry["y"].as_u64().expect("y") as usize);

    let shot = compositor.screenshot("ok 640x360 windows=1");
    assert_eq!((shot.width, shot.height), (640, 360));
    assert_eq!(shot.pixel(x + 32, y + 32), [255, 0, 0, 255]);
    assert_eq!(shot.pixel(x + 63, y + 63), [255, 0, 0, 255]);
    assert_ne!(shot.pixel(x + 100, y + 100), [255, 0, 0, 255]);
    assert_eq!(shot.pixel(639, 359), [0x20, 0x21, 0x24, 255]);

    assert_eq!(compositor.control("pointer 10 10 none"), "ok");
    assert_eq!(compositor.control(&format!("pointer {} {} left", x + 5, y + 5)), "ok");
    assert_eq!(compositor.control(&format!("pointer {} {} none", x + 5, y + 5)), "ok");
    assert_eq!(compositor.control("key 30 down"), "ok");
    assert_eq!(compositor.control("key 30 up"), "ok");
    assert!(compositor.control("bogus").starts_with("err"));
    drop(window.toplevel);
    drop(window.xdg_surface);
}

#[test]
fn maximized_toplevel_fills_canvas_through_viewport_and_sigterm_cleans_up() {
    let mut compositor = Compositor::start("640x360");
    let mut session = Session::connect(&compositor);
    let window = session.toplevel("full", "fw.full", true);
    assert!(session.client.maximized, "maximized state missing from configure");
    assert_eq!(session.client.toplevel_size, (640, 360));
    session.attach_solid(&window, 1, 1, 0xFF00_80FF, Some((640, 360)));

    let windows: serde_json::Value = serde_json::from_str(&compositor.control("windows")).expect("windows json");
    assert_eq!((windows[0]["x"].as_i64(), windows[0]["y"].as_i64()), (Some(0), Some(0)));
    assert_eq!((windows[0]["w"].as_i64(), windows[0]["h"].as_i64()), (Some(640), Some(360)));

    let shot = compositor.screenshot("ok 640x360 windows=1");
    assert_eq!(shot.pixel(5, 5), [0x00, 0x80, 0xFF, 255]);
    assert_eq!(shot.pixel(320, 180), [0x00, 0x80, 0xFF, 255]);
    assert_eq!(shot.pixel(639, 359), [0x00, 0x80, 0xFF, 255]);

    let status = Command::new("kill").args(["-TERM", &compositor.child.id().to_string()]).status().expect("kill");
    assert!(status.success());
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = compositor.child.try_wait().expect("try_wait") {
            break exit;
        }
        assert!(Instant::now() < deadline, "compositor ignored SIGTERM");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(exit.success(), "unclean exit {exit:?}");
    assert!(!compositor.socket.exists(), "wayland socket left behind");
    assert!(!compositor.ctl.exists(), "control socket left behind");
}
