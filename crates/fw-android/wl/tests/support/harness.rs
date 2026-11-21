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

pub struct Compositor {
    pub child: Child,
    dir: TempDir,
    pub socket: PathBuf,
    pub ctl: PathBuf,
}

impl Compositor {
    pub fn start(size: &str, extra: &[String]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let socket = dir.path().join("wayland-fw");
        let ctl = dir.path().join("ctl.sock");
        let child = Command::new(env!("CARGO_BIN_EXE_fw-android-wl"))
            .args(["--socket", socket.to_str().expect("utf8"), "--ctl", ctl.to_str().expect("utf8"), "--size", size])
            .args(extra)
            .stderr(Stdio::inherit())
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn compositor");
        let compositor = Self { child, dir, socket, ctl };
        wait_for(&compositor.socket);
        wait_for(&compositor.ctl);
        compositor
    }

    pub fn control(&self, command: &str) -> String {
        let mut stream = UnixStream::connect(&self.ctl).expect("control connect");
        writeln!(stream, "{command}").expect("control write");
        let mut reply = String::new();
        BufReader::new(stream).read_line(&mut reply).expect("control read");
        reply.trim().to_string()
    }

    pub fn screenshot(&self, expected_reply: &str) -> Screenshot {
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

pub struct Screenshot {
    pub width: usize,
    pub height: usize,
    pixels: Vec<u8>,
}

impl Screenshot {
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
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
pub struct Client {
    configured: bool,
    configure_serial: u32,
    pub toplevel_size: (i32, i32),
    pub maximized: bool,
    pub closed: bool,
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
        match event {
            xdg_toplevel::Event::Configure { width, height, states } => {
                state.toplevel_size = (width, height);
                state.maximized = states
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|chunk| u32::from_ne_bytes(*chunk) == xdg_toplevel::State::Maximized as u32);
            }
            xdg_toplevel::Event::Close => state.closed = true,
            _ => {}
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

pub struct Window {
    pub surface: wl_surface::WlSurface,
    pub xdg_surface: xdg_surface::XdgSurface,
    pub toplevel: xdg_toplevel::XdgToplevel,
}

pub struct Session {
    pub client: Client,
    pub queue: EventQueue<Client>,
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    wm_base: xdg_wm_base::XdgWmBase,
    viewporter: wp_viewporter::WpViewporter,
    _connection: Connection,
}

impl Session {
    pub fn connect(compositor: &Compositor) -> Self {
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

    pub fn toplevel(&mut self, title: &str, app_id: &str, maximized: bool) -> Window {
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

    pub fn attach_solid(&mut self, window: &Window, width: i32, height: i32, argb: u32, destination: Option<(i32, i32)>) {
        self.attach_argb(window, width, height, &vec![argb; (width * height) as usize], destination);
    }

    pub fn attach_argb(&mut self, window: &Window, width: i32, height: i32, pixels: &[u32], destination: Option<(i32, i32)>) {
        let handle = self.queue.handle();
        let bytes: Vec<u8> = pixels.iter().flat_map(|pixel| pixel.to_le_bytes()).collect();
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

