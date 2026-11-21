#[path = "support/harness.rs"]
#[allow(dead_code)]
mod harness;

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use fw_android_stream::{DecodeError, Message};
use harness::{Compositor, Session};

struct StreamClient {
    socket: TcpStream,
    buffer: Vec<u8>,
}

impl StreamClient {
    fn connect(port: u16) -> Self {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let socket = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(socket) => break socket,
                Err(error) => {
                    assert!(std::time::Instant::now() < deadline, "stream port never opened: {error}");
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(10))).expect("timeout");
        Self { socket, buffer: Vec::new() }
    }

    fn next(&mut self) -> Message {
        loop {
            match Message::decode(&self.buffer) {
                Ok((message, used)) => {
                    self.buffer.drain(..used);
                    return message;
                }
                Err(DecodeError::Incomplete) => {
                    let mut chunk = [0u8; 65536];
                    let read = self.socket.read(&mut chunk).expect("stream read");
                    assert!(read > 0, "stream closed");
                    self.buffer.extend_from_slice(&chunk[..read]);
                }
                Err(error) => panic!("stream decode failed: {error}"),
            }
        }
    }

    fn send(&mut self, message: &Message) {
        self.socket.write_all(&message.encode()).expect("stream write");
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").port()
}

#[test]
fn tcp_stream_announces_window_sends_frames_and_applies_close_and_resize() {
    let port = free_port();
    let compositor = Compositor::start("640x360", &["--stream".into(), format!("127.0.0.1:{port}")]);
    let mut stream = StreamClient::connect(port);
    let mut session = Session::connect(&compositor);
    let window = session.toplevel("stream test", "fw.stream", false);
    session.attach_solid(&window, 64, 64, 0xFFFF_0000, None);

    let Message::WindowOpen { window: id, app_id, title, width, height } = stream.next() else {
        panic!("first message is not WindowOpen");
    };
    assert_eq!((app_id.as_str(), title.as_str(), width, height), ("fw.stream", "stream test", 64, 64));

    let Message::Frame { window: frame_window, x, y, width, height, pixels } = stream.next() else {
        panic!("second message is not Frame");
    };
    assert_eq!((frame_window, x, y, width, height), (id, 0, 0, 64, 64));
    assert_eq!(pixels.len(), 64 * 64 * 4);
    assert_eq!(&pixels[..4], &[0x00, 0x00, 0xFF, 0xFF], "BGRA red");
    let last = pixels.len() - 4;
    assert_eq!(&pixels[last..], &[0x00, 0x00, 0xFF, 0xFF]);

    let mut next = vec![0xFFFF_0000u32; 64 * 64];
    for row in 0..16 {
        next[row * 64..row * 64 + 16].fill(0xFF00_FF00);
    }
    session.attach_argb(&window, 64, 64, &next, None);
    let Message::Frame { x, y, width, height, pixels, .. } = stream.next() else {
        panic!("damage message is not Frame");
    };
    assert_eq!((x, y, width, height), (0, 0, 16, 16));
    assert_eq!(&pixels[..4], &[0x00, 0xFF, 0x00, 0xFF], "BGRA green");

    stream.send(&Message::ResizeRequest { window: id, width: 128, height: 96 });
    stream.send(&Message::CloseRequest { window: id });
    while !session.client.closed {
        session.queue.blocking_dispatch(&mut session.client).expect("dispatch");
    }
    assert_eq!(session.client.toplevel_size, (128, 96));

    window.toplevel.destroy();
    window.surface.destroy();
    session.queue.roundtrip(&mut session.client).expect("roundtrip");
    assert_eq!(stream.next(), Message::WindowClose { window: id });
}

#[test]
fn late_stream_client_receives_existing_windows() {
    let port = free_port();
    let compositor = Compositor::start("640x360", &["--stream".into(), format!("127.0.0.1:{port}")]);
    let mut session = Session::connect(&compositor);
    let window = session.toplevel("early", "fw.early", false);
    session.attach_solid(&window, 32, 16, 0xFF00_00FF, None);

    let mut stream = StreamClient::connect(port);
    let Message::WindowOpen { width, height, .. } = stream.next() else { panic!("expected WindowOpen") };
    assert_eq!((width, height), (32, 16));
    let Message::Frame { pixels, .. } = stream.next() else { panic!("expected Frame") };
    assert_eq!(&pixels[..4], &[0xFF, 0x00, 0x00, 0xFF], "BGRA blue");
}
