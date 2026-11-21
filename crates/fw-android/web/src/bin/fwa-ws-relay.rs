//! WebSocket to TCP relay for the browser client: a page cannot open TCP, so this accepts WebSocket connections and
//! forwards them to the compositor's fwa-stream endpoint. Page to server: one binary frame is one message. Server to
//! page: the TCP byte stream is cut into messages (`u8 kind, u32 window, u32 payload_len` header) and each message
//! becomes one binary frame, the framing `fw-android-web` expects.
//!
//! `fwa-ws-relay <listen host:port> <compositor host:port> [--token <secret>]`; with a token the page must connect to
//! `ws://<listen>/?token=<secret>`, so another web page cannot reach the loopback compositor.

use std::process::ExitCode;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message as Frame;

const HEADER: usize = 9;
const MAX_PAYLOAD: usize = 256 * 1024 * 1024;

struct Options {
    listen: String,
    target: String,
    token: Option<String>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut positional = Vec::new();
    let mut token = None;
    let mut iter = args.iter();
    while let Some(argument) = iter.next() {
        match argument.as_str() {
            "--token" => token = Some(iter.next().ok_or("--token needs a value")?.clone()),
            other => positional.push(other.to_string()),
        }
    }
    let [listen, target] = <[String; 2]>::try_from(positional)
        .map_err(|_| "usage: fwa-ws-relay <listen host:port> <compositor host:port> [--token <secret>]".to_string())?;
    Ok(Options { listen, target, token })
}

fn token_of(request: &Request) -> Option<String> {
    let query = request.uri().query()?;
    query.split('&').find_map(|pair| pair.strip_prefix("token=")).map(str::to_string)
}

async fn serve(socket: TcpStream, target: &str, token: Option<&str>) -> Result<(), String> {
    let websocket = tokio_tungstenite::accept_hdr_async(socket, |request: &Request, response: Response| match token {
        Some(expected) if token_of(request).as_deref() != Some(expected) => {
            let mut denied = ErrorResponse::new(Some("token required".to_string()));
            *denied.status_mut() = StatusCode::UNAUTHORIZED;
            Err(denied)
        }
        _ => Ok(response),
    })
    .await
    .map_err(|error| format!("websocket handshake: {error}"))?;
    let upstream = TcpStream::connect(target).await.map_err(|error| format!("connect {target}: {error}"))?;
    upstream.set_nodelay(true).map_err(|error| error.to_string())?;
    let (mut tcp_read, mut tcp_write) = upstream.into_split();
    let (mut ws_write, mut ws_read) = websocket.split();

    let page_to_server = async {
        while let Some(frame) = ws_read.next().await {
            match frame.map_err(|error| error.to_string())? {
                Frame::Binary(bytes) => tcp_write.write_all(&bytes).await.map_err(|error| error.to_string())?,
                Frame::Close(_) => break,
                _ => {}
            }
        }
        tcp_write.shutdown().await.map_err(|error| error.to_string())
    };
    let server_to_page = async {
        let mut header = [0u8; HEADER];
        loop {
            match tcp_read.read_exact(&mut header).await {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(error) => return Err(error.to_string()),
            }
            let length = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
            if length > MAX_PAYLOAD {
                return Err(format!("payload of {length} bytes exceeds the limit"));
            }
            let mut message = vec![0u8; HEADER + length];
            message[..HEADER].copy_from_slice(&header);
            tcp_read.read_exact(&mut message[HEADER..]).await.map_err(|error| error.to_string())?;
            ws_write.send(Frame::Binary(message.into())).await.map_err(|error| error.to_string())?;
        }
        ws_write.close().await.map_err(|error| error.to_string())
    };
    tokio::select! {
        result = page_to_server => result,
        result = server_to_page => result,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse(&args) {
        Ok(options) => std::sync::Arc::new(options),
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let listener = match TcpListener::bind(&options.listen).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("fwa-ws-relay: bind {}: {error}", options.listen);
            return ExitCode::FAILURE;
        }
    };
    eprintln!("fwa-ws-relay: ws://{} -> tcp://{}", options.listen, options.target);
    loop {
        let (socket, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                eprintln!("fwa-ws-relay: accept: {error}");
                continue;
            }
        };
        let options = options.clone();
        tokio::spawn(async move {
            if let Err(error) = serve(socket, &options.target, options.token.as_deref()).await {
                eprintln!("fwa-ws-relay: {peer}: {error}");
            }
        });
    }
}
