//! End to end: fake compositor on TCP, the real `fwa-ws-relay` binary, a WebSocket client playing the page.

#![cfg(feature = "host")]

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use fw_android_stream::Message;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as Frame;

struct Relay(Child);

impl Drop for Relay {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

async fn free_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    listener.local_addr().expect("address").to_string()
}

async fn start_relay(target: &str, token: Option<&str>) -> (Relay, String) {
    let listen = free_address().await;
    let mut command = Command::new(env!("CARGO_BIN_EXE_fwa-ws-relay"));
    command.args([listen.as_str(), target]).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(token) = token {
        command.args(["--token", token]);
    }
    let relay = Relay(command.spawn().expect("spawn relay"));
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(&listen).await.is_ok() {
            return (relay, listen);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("relay did not start listening on {listen}");
}

#[tokio::test]
async fn each_tcp_message_becomes_one_binary_frame_and_page_messages_reach_the_compositor() {
    let compositor = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let target = compositor.local_addr().expect("address").to_string();
    let (_relay, listen) = start_relay(&target, None).await;

    let first = Message::Frame { window: 3, x: 1, y: 2, width: 2, height: 2, pixels: (0..16).collect() };
    let second = Message::WindowClose { window: 3 };
    let pointer = Message::Pointer { window: 3, x: 10, y: 20, buttons: 1 };
    let expected_pointer = pointer.encode();
    let server = tokio::spawn({
        let (first, second) = (first.encode(), second.encode());
        async move {
            let (mut socket, _) = compositor.accept().await.expect("accept");
            // Two messages in one TCP write, the second split across two writes: the relay must cut them apart.
            let mut burst = first;
            burst.extend_from_slice(&second[..4]);
            socket.write_all(&burst).await.expect("write");
            tokio::time::sleep(Duration::from_millis(50)).await;
            socket.write_all(&second[4..]).await.expect("write");
            let mut received = vec![0u8; expected_pointer.len()];
            socket.read_exact(&mut received).await.expect("read pointer");
            received
        }
    });

    let (mut page, _) = tokio_tungstenite::connect_async(format!("ws://{listen}/")).await.expect("connect");
    let mut decoded = Vec::new();
    while decoded.len() < 2 {
        let Frame::Binary(bytes) = page.next().await.expect("frame").expect("frame ok") else { continue };
        let (message, used) = Message::decode(&bytes).expect("decode");
        assert_eq!(used, bytes.len(), "one message per binary frame");
        decoded.push(message);
    }
    assert_eq!(decoded, vec![first, second]);
    page.send(Frame::Binary(pointer.encode().into())).await.expect("send");
    assert_eq!(server.await.expect("server"), pointer.encode());
}

#[tokio::test]
async fn a_wrong_or_missing_token_is_refused_and_the_right_one_is_accepted() {
    let compositor = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let target = compositor.local_addr().expect("address").to_string();
    let (_relay, listen) = start_relay(&target, Some("pairing")).await;
    assert!(tokio_tungstenite::connect_async(format!("ws://{listen}/")).await.is_err());
    assert!(tokio_tungstenite::connect_async(format!("ws://{listen}/?token=wrong")).await.is_err());
    assert!(tokio_tungstenite::connect_async(format!("ws://{listen}/?token=pairing")).await.is_ok());
}
