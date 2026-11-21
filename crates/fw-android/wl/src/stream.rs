//! fwa-stream v1 server side: window lifecycle, damage frames and input for remote clients.

use std::{
    io::{self, Read, Write},
    net::TcpStream,
    os::{fd::{AsFd, BorrowedFd}, unix::net::UnixStream},
    time::{Duration, Instant},
};

use fw_android_stream::{DecodeError, Message};
use smithay::utils::Point;

use crate::{
    render::{self, Canvas},
    state::State,
};

pub const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
const CONGESTED_BYTES: usize = 8 * 1024 * 1024;
const MAX_OUTBOX_BYTES: usize = 256 * 1024 * 1024;

pub enum Socket {
    Tcp(TcpStream),
    Unix(UnixStream),
}

impl Socket {
    pub fn try_clone(&self) -> io::Result<Self> {
        match self {
            Self::Tcp(socket) => socket.try_clone().map(Self::Tcp),
            Self::Unix(socket) => socket.try_clone().map(Self::Unix),
        }
    }

    pub fn prepare(&self) -> io::Result<()> {
        match self {
            Self::Tcp(socket) => {
                socket.set_nonblocking(true)?;
                socket.set_nodelay(true)
            }
            Self::Unix(socket) => socket.set_nonblocking(true),
        }
    }
}

impl AsFd for Socket {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Tcp(socket) => socket.as_fd(),
            Self::Unix(socket) => socket.as_fd(),
        }
    }
}

impl Read for &Socket {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Socket::Tcp(socket) => Read::read(&mut &*socket, buffer),
            Socket::Unix(socket) => Read::read(&mut &*socket, buffer),
        }
    }
}

impl Write for &Socket {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Socket::Tcp(socket) => Write::write(&mut &*socket, buffer),
            Socket::Unix(socket) => Write::write(&mut &*socket, buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct StreamClient {
    id: u64,
    writer: Socket,
    inbox: Vec<u8>,
    outbox: Vec<u8>,
    sent: usize,
}

impl StreamClient {
    fn pending(&self) -> usize {
        self.outbox.len() - self.sent
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut writer = &self.writer;
        while self.sent < self.outbox.len() {
            match writer.write(&self.outbox[self.sent..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(written) => self.sent += written,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        self.outbox.clear();
        self.sent = 0;
        Ok(())
    }
}

#[derive(Default)]
pub struct StreamHub {
    clients: Vec<StreamClient>,
    next_id: u64,
}

pub struct SentFrame {
    width: usize,
    height: usize,
    pixels: Vec<u32>,
}

#[derive(Default)]
pub struct WindowFeed {
    pub dirty: bool,
    last_sent: Option<Instant>,
    last: Option<SentFrame>,
}

fn bgra(pixels: &[u32]) -> Vec<u8> {
    pixels.iter().flat_map(|pixel| pixel.to_le_bytes()).collect()
}

fn full_frame(window: u32, canvas: &Canvas) -> Message {
    Message::Frame {
        window,
        x: 0,
        y: 0,
        width: canvas.width as u32,
        height: canvas.height as u32,
        pixels: bgra(&canvas.pixels),
    }
}

fn damage(previous: &SentFrame, canvas: &Canvas) -> Option<[usize; 4]> {
    let (width, height) = (canvas.width as usize, canvas.height as usize);
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for row in 0..height {
        let current = &canvas.pixels[row * width..(row + 1) * width];
        let before = &previous.pixels[row * width..(row + 1) * width];
        if current == before {
            continue;
        }
        let first = current.iter().zip(before).position(|(a, b)| a != b).unwrap_or(0);
        let last = current.iter().zip(before).rposition(|(a, b)| a != b).unwrap_or(first);
        left = left.min(first);
        right = right.max(last + 1);
        top = top.min(row);
        bottom = row + 1;
    }
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

fn sub_frame(window: u32, canvas: &Canvas, [x, y, width, height]: [usize; 4]) -> Message {
    let stride = canvas.width as usize;
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in y..y + height {
        pixels.extend(canvas.pixels[row * stride + x..row * stride + x + width].iter().flat_map(|pixel| pixel.to_le_bytes()));
    }
    Message::Frame { window, x: x as u32, y: y as u32, width: width as u32, height: height as u32, pixels }
}

impl State {
    pub fn stream_attach(&mut self, writer: Socket) -> u64 {
        let id = self.stream.next_id;
        self.stream.next_id += 1;
        self.stream.clients.push(StreamClient { id, writer, inbox: Vec::new(), outbox: Vec::new(), sent: 0 });
        let mut greeting = Vec::new();
        for window in self.windows.iter().filter(|window| window.announced) {
            let Some(canvas) = render::render_window(window, self.scale) else { continue };
            let (app_id, title) = window.labels();
            greeting.push(Message::WindowOpen {
                window: window.id,
                app_id: app_id.unwrap_or_default(),
                title: title.unwrap_or_default(),
                width: canvas.width as u32,
                height: canvas.height as u32,
            });
            greeting.push(full_frame(window.id, &canvas));
        }
        for message in &greeting {
            self.stream_queue(Some(id), message);
        }
        eprintln!("fw-android-wl: stream client #{id} attached, {} window(s) announced", greeting.len() / 2);
        id
    }

    pub fn stream_detach(&mut self, id: u64) {
        let before = self.stream.clients.len();
        self.stream.clients.retain(|client| client.id != id);
        if self.stream.clients.len() != before {
            eprintln!("fw-android-wl: stream client #{id} detached");
        }
    }

    fn stream_queue(&mut self, only: Option<u64>, message: &Message) {
        let bytes = message.encode();
        let mut dropped = Vec::new();
        for client in self.stream.clients.iter_mut().filter(|client| only.is_none_or(|id| id == client.id)) {
            client.outbox.extend_from_slice(&bytes);
            let failure = match client.flush() {
                Err(error) => Some(error.to_string()),
                Ok(()) if client.pending() > MAX_OUTBOX_BYTES => Some("backlog too large".into()),
                Ok(()) => None,
            };
            if let Some(why) = failure {
                eprintln!("fw-android-wl: stream client #{} dropped: {why}", client.id);
                dropped.push(client.id);
            }
        }
        self.stream.clients.retain(|client| !dropped.contains(&client.id));
    }

    pub fn stream_window_opened(&mut self, index: usize) {
        let Some(window) = self.windows.get(index) else { return };
        let Some((width, height)) = self.window_pixels(window) else { return };
        let (app_id, title) = window.labels();
        let message = Message::WindowOpen {
            window: window.id,
            app_id: app_id.unwrap_or_default(),
            title: title.unwrap_or_default(),
            width,
            height,
        };
        self.stream_queue(None, &message);
        self.windows[index].feed.dirty = true;
    }

    pub fn stream_window_closed(&mut self, window: u32) {
        self.stream_queue(None, &Message::WindowClose { window });
    }

    pub fn stream_title(&mut self, index: usize) {
        let Some(window) = self.windows.get(index).filter(|window| window.announced) else { return };
        let message = Message::WindowTitle { window: window.id, title: window.labels().1.unwrap_or_default() };
        self.stream_queue(None, &message);
    }

    /// Emits due frames and flushes backlogs; returns when the loop must wake up again.
    pub fn stream_tick(&mut self) -> Option<Duration> {
        if self.stream.clients.is_empty() {
            return None;
        }
        let mut failed = Vec::new();
        for client in &mut self.stream.clients {
            if let Err(error) = client.flush() {
                eprintln!("fw-android-wl: stream client #{} dropped: {error}", client.id);
                failed.push(client.id);
            }
        }
        self.stream.clients.retain(|client| !failed.contains(&client.id));
        let mut wake = self.stream.clients.iter().any(|client| client.pending() > 0).then_some(Duration::from_millis(5));
        if self.stream.clients.iter().any(|client| client.pending() > CONGESTED_BYTES) {
            return wake;
        }
        let now = Instant::now();
        for index in 0..self.windows.len() {
            let window = &self.windows[index];
            if !window.announced || !window.feed.dirty {
                continue;
            }
            if let Some(remaining) = window.feed.last_sent.and_then(|sent| FRAME_INTERVAL.checked_sub(now.duration_since(sent))) {
                wake = Some(wake.map_or(remaining, |current| current.min(remaining)));
                continue;
            }
            let id = window.id;
            let Some(canvas) = render::render_window(window, self.scale) else {
                self.windows[index].feed.dirty = false;
                continue;
            };
            let (width, height) = (canvas.width as usize, canvas.height as usize);
            let mut messages = Vec::new();
            match self.windows[index].feed.last.as_ref() {
                Some(previous) if previous.width == width && previous.height == height => {
                    if let Some(rect) = damage(previous, &canvas) {
                        messages.push(sub_frame(id, &canvas, rect));
                    }
                }
                previous => {
                    if previous.is_some() {
                        messages.push(Message::WindowSize { window: id, width: width as u32, height: height as u32 });
                    }
                    messages.push(full_frame(id, &canvas));
                }
            }
            let feed = &mut self.windows[index].feed;
            feed.dirty = false;
            if !messages.is_empty() {
                feed.last_sent = Some(now);
            }
            feed.last = Some(SentFrame { width, height, pixels: canvas.pixels });
            for message in &messages {
                self.stream_queue(None, message);
            }
        }
        wake
    }

    /// Returns false when the client violated the protocol and must be dropped.
    pub fn stream_input(&mut self, id: u64, bytes: &[u8]) -> bool {
        let mut messages = Vec::new();
        {
            let Some(client) = self.stream.clients.iter_mut().find(|client| client.id == id) else { return false };
            client.inbox.extend_from_slice(bytes);
            loop {
                match Message::decode(&client.inbox) {
                    Ok((message, used)) => {
                        client.inbox.drain(..used);
                        messages.push(message);
                    }
                    Err(DecodeError::Incomplete) => break,
                    Err(error) => {
                        eprintln!("fw-android-wl: stream client #{id} protocol error: {error}");
                        return false;
                    }
                }
            }
        }
        for message in messages {
            self.apply_stream_message(message);
        }
        true
    }

    fn apply_stream_message(&mut self, message: Message) {
        let Some(index) = self.windows.iter().position(|window| window.id == message.window() && window.announced) else {
            eprintln!("fw-android-wl: stream message for unknown window {}", message.window());
            return;
        };
        match message {
            Message::Pointer { x, y, buttons, .. } => {
                let index = self.ensure_top(index);
                let position = self.windows[index].position;
                let local = Point::from((f64::from(x) / self.scale, f64::from(y) / self.scale));
                self.pointer_motion(position.to_f64() + local);
                self.pointer_buttons(buttons);
            }
            Message::Key { code, down, .. } => {
                self.ensure_top(index);
                if let Err(why) = self.key_input(code, down) {
                    eprintln!("fw-android-wl: stream key dropped: {why}");
                }
            }
            Message::ResizeRequest { width, height, .. } => self.resize_window(index, width, height),
            Message::CloseRequest { .. } => self.windows[index].toplevel.send_close(),
            other => eprintln!("fw-android-wl: stream client sent server-side message {other:?}"),
        }
    }

    fn ensure_top(&mut self, index: usize) -> usize {
        if index + 1 != self.windows.len() {
            self.raise_and_focus(index);
        }
        self.windows.len().saturating_sub(1)
    }
}
