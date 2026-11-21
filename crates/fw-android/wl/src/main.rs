mod control;
mod render;
mod state;
mod stream;
mod surface;

use std::{
    io::{Read, Write},
    net::TcpListener,
    os::{fd::AsFd, unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}}},
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

use calloop::{
    EventLoop, Interest, LoopHandle, Mode, PostAction,
    generic::Generic,
    signals::{Signal, Signals},
};
use smithay::reexports::wayland_server::{Display, ListeningSocket};

use state::{ClientState, State};
use stream::Socket;

const IDLE_TIMEOUT: Duration = Duration::from_millis(250);

struct Options {
    socket: PathBuf,
    ctl: PathBuf,
    streams: Vec<String>,
    size: (i32, i32),
    scale: f64,
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("fw-android-wl: removing {} failed: {error}", self.0.display());
        }
    }
}

struct LoopData {
    state: State,
    display: Display<State>,
}

fn parse_options() -> Result<Options, String> {
    let mut socket = None;
    let mut ctl = None;
    let mut streams = Vec::new();
    let mut size = (1280, 720);
    let mut scale = 1.0;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--socket" => socket = Some(PathBuf::from(value)),
            "--ctl" => ctl = Some(PathBuf::from(value)),
            "--stream" => streams.push(value),
            "--size" => {
                let (width, height) = value.split_once('x').ok_or("--size expects WxH")?;
                size = (
                    width.parse().map_err(|_| "--size width is not a number")?,
                    height.parse().map_err(|_| "--size height is not a number")?,
                );
                if size.0 <= 0 || size.1 <= 0 {
                    return Err("--size must be positive".into());
                }
            }
            "--scale" => {
                scale = value.parse().map_err(|_| "--scale is not a number")?;
                if !(0.25..=16.0).contains(&scale) {
                    return Err("--scale must be within 0.25..16".into());
                }
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(Options {
        socket: socket.ok_or("--socket is required")?,
        ctl: ctl.ok_or("--ctl is required")?,
        streams,
        size,
        scale,
    })
}

fn serve_control_connection(stream: UnixStream) -> Generic<UnixStream> {
    Generic::new(stream, Interest::READ, Mode::Level)
}

fn handle_control(state: &mut State, stream: &UnixStream, pending: &mut Vec<u8>) -> std::io::Result<PostAction> {
    let mut chunk = [0u8; 4096];
    let mut reader = stream;
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => return Ok(PostAction::Remove),
            Ok(read) => pending.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error),
        }
    }
    while let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        let reply = control::execute(state, String::from_utf8_lossy(&line).trim());
        let mut writer = stream;
        writer.write_all(reply.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    Ok(PostAction::Continue)
}

fn attach_stream(handle: &LoopHandle<'static, LoopData>, data: &mut LoopData, socket: Socket) {
    if let Err(error) = socket.prepare() {
        eprintln!("fw-android-wl: stream connection setup failed: {error}");
        return;
    }
    let reader = match socket.try_clone() {
        Ok(reader) => reader,
        Err(error) => {
            eprintln!("fw-android-wl: stream connection clone failed: {error}");
            return;
        }
    };
    let id = data.state.stream_attach(socket);
    let result = handle.insert_source(Generic::new(reader, Interest::READ, Mode::Level), move |_, socket, data| {
        let mut chunk = vec![0u8; 64 * 1024];
        let mut reader: &Socket = socket;
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    if !data.state.stream_input(id, &chunk[..read]) {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(PostAction::Continue),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => {
                    eprintln!("fw-android-wl: stream client #{id} read failed: {error}");
                    break;
                }
            }
        }
        data.state.stream_detach(id);
        Ok(PostAction::Remove)
    });
    if let Err(error) = result {
        eprintln!("fw-android-wl: stream reader rejected: {error}");
        data.state.stream_detach(id);
    }
}

fn listen_stream(handle: &LoopHandle<'static, LoopData>, spec: &str) -> Result<Option<RemoveOnDrop>, String> {
    let accepting = handle.clone();
    if let Some(path) = spec.strip_prefix("unix:") {
        let path = PathBuf::from(path);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        let listener = UnixListener::bind(&path).map_err(|error| format!("stream {}: {error}", path.display()))?;
        let cleanup = RemoveOnDrop(path);
        listener.set_nonblocking(true).map_err(|error| error.to_string())?;
        handle
            .insert_source(Generic::new(listener, Interest::READ, Mode::Level), move |_, listener, data| {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => attach_stream(&accepting, data, Socket::Unix(stream)),
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error) => {
                            eprintln!("fw-android-wl: stream accept failed: {error}");
                            break;
                        }
                    }
                }
                Ok(PostAction::Continue)
            })
            .map_err(|error| error.to_string())?;
        eprintln!("fw-android-wl: stream listening on unix:{}", cleanup.0.display());
        return Ok(Some(cleanup));
    }
    let listener = TcpListener::bind(spec).map_err(|error| format!("stream {spec}: {error}"))?;
    listener.set_nonblocking(true).map_err(|error| error.to_string())?;
    handle
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), move |_, listener, data| {
            loop {
                match listener.accept() {
                    Ok((stream, peer)) => {
                        eprintln!("fw-android-wl: stream tcp connection from {peer}");
                        attach_stream(&accepting, data, Socket::Tcp(stream));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("fw-android-wl: stream accept failed: {error}");
                        break;
                    }
                }
            }
            Ok(PostAction::Continue)
        })
        .map_err(|error| error.to_string())?;
    eprintln!("fw-android-wl: stream listening on tcp:{spec}");
    Ok(None)
}

fn run(options: &Options) -> Result<(), String> {
    for path in [&options.socket, &options.ctl] {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
    }
    let display: Display<State> = Display::new().map_err(|error| error.to_string())?;
    let state = State::new(display.handle(), options.size, options.scale);
    eprintln!(
        "fw-android-wl: output {}x{} scale {} globals: {}",
        options.size.0,
        options.size.1,
        options.scale,
        state.describe_globals()
    );

    let mut socket_lock = options.socket.clone().into_os_string();
    socket_lock.push(".lock");
    let listener = ListeningSocket::bind_absolute(options.socket.clone())
        .map_err(|error| format!("wayland socket {}: {error}", options.socket.display()))?;
    let _socket_cleanup = [RemoveOnDrop(options.socket.clone()), RemoveOnDrop(PathBuf::from(socket_lock))];
    std::fs::set_permissions(&options.socket, std::fs::Permissions::from_mode(0o777))
        .map_err(|error| format!("chmod {}: {error}", options.socket.display()))?;

    if options.ctl.exists() {
        std::fs::remove_file(&options.ctl).map_err(|error| format!("{}: {error}", options.ctl.display()))?;
    }
    let control_listener = UnixListener::bind(&options.ctl).map_err(|error| format!("{}: {error}", options.ctl.display()))?;
    let _ctl_cleanup = RemoveOnDrop(options.ctl.clone());
    control_listener.set_nonblocking(true).map_err(|error| error.to_string())?;

    let mut event_loop: EventLoop<LoopData> = EventLoop::try_new().map_err(|error| error.to_string())?;
    let handle = event_loop.handle();

    let display_fd = display.as_fd().try_clone_to_owned().map_err(|error| error.to_string())?;
    handle
        .insert_source(Generic::new(display_fd, Interest::READ, Mode::Level), |_, _, _| Ok(PostAction::Continue))
        .map_err(|error| error.to_string())?;

    handle
        .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, listener, data| {
            loop {
                match listener.accept() {
                    Ok(Some(stream)) => match data.display.handle().insert_client(stream, Arc::new(ClientState::default())) {
                        Ok(client) => {
                            let pid = client
                                .get_credentials(&data.display.handle())
                                .map_or_else(|_| "unknown".into(), |credentials| credentials.pid.to_string());
                            eprintln!("fw-android-wl: wayland client connected pid={pid} id={:?}", client.id());
                        }
                        Err(error) => eprintln!("fw-android-wl: client rejected: {error}"),
                    },
                    Ok(None) => break,
                    Err(error) => {
                        eprintln!("fw-android-wl: accept failed: {error}");
                        break;
                    }
                }
            }
            Ok(PostAction::Continue)
        })
        .map_err(|error| error.to_string())?;

    let loop_handle = handle.clone();
    handle
        .insert_source(Generic::new(control_listener, Interest::READ, Mode::Level), move |_, listener, _| {
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Err(error) = stream.set_nonblocking(true) {
                            eprintln!("fw-android-wl: control connection setup failed: {error}");
                            continue;
                        }
                        let mut pending = Vec::new();
                        let source = serve_control_connection(stream);
                        if let Err(error) = loop_handle.insert_source(source, move |_, stream, data: &mut LoopData| {
                            handle_control(&mut data.state, stream, &mut pending)
                        }) {
                            eprintln!("fw-android-wl: control connection rejected: {error}");
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => {
                        eprintln!("fw-android-wl: control accept failed: {error}");
                        break;
                    }
                }
            }
            Ok(PostAction::Continue)
        })
        .map_err(|error| error.to_string())?;

    let signals = Signals::new(&[Signal::SIGTERM, Signal::SIGINT]).map_err(|error| error.to_string())?;
    handle
        .insert_source(signals, |event, _, data| {
            eprintln!("fw-android-wl: signal {:?}, shutting down", event.signal());
            data.state.quit = true;
        })
        .map_err(|error| error.to_string())?;

    let mut stream_cleanup = Vec::new();
    for spec in &options.streams {
        stream_cleanup.extend(listen_stream(&handle, spec)?);
    }

    eprintln!(
        "fw-android-wl: listening wayland={} ctl={}",
        options.socket.display(),
        options.ctl.display()
    );
    let mut data = LoopData { state, display };
    let mut timeout = IDLE_TIMEOUT;
    while !data.state.quit {
        event_loop.dispatch(Some(timeout), &mut data).map_err(|error| error.to_string())?;
        data.display.dispatch_clients(&mut data.state).map_err(|error| error.to_string())?;
        timeout = data.state.stream_tick().map_or(IDLE_TIMEOUT, |wake| wake.min(IDLE_TIMEOUT));
        data.display.flush_clients().map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("fw-android-wl: {error}");
            eprintln!("usage: fw-android-wl --socket <path> --ctl <path> [--size WxH] [--scale N] [--stream <host:port>|unix:<path>]...");
            return ExitCode::from(2);
        }
    };

    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fw-android-wl: {error}");
            ExitCode::FAILURE
        }
    }
}
