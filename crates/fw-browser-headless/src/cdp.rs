// SPDX-License-Identifier: Apache-2.0
//! A blocking Chrome DevTools Protocol client over one WebSocket. Every engine behind `fw browser` speaks it: the lite engine
//! (Obscura, V8 + wgpu) answers a subset, Blink the whole protocol.

use std::collections::VecDeque;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

#[derive(Debug, thiserror::Error)]
pub enum CdpError {
    #[error("websocket: {0}")]
    Socket(#[from] tungstenite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{method}: {message} (code {code})")]
    Protocol { method: String, code: i64, message: String },
    #[error("{method}: no answer within {seconds}s")]
    Timeout { method: String, seconds: u64 },
    #[error("invalid endpoint `{0}`")]
    Endpoint(String),
}

pub struct Cdp {
    socket: WebSocket<TcpStream>,
    next_id: u64,
    events: VecDeque<Value>,
}

impl Cdp {
    /// Connects to a `ws://host:port/path` DevTools endpoint.
    pub fn connect(endpoint: &str) -> Result<Self, CdpError> {
        let authority = endpoint
            .strip_prefix("ws://")
            .and_then(|rest| rest.split('/').next())
            .ok_or_else(|| CdpError::Endpoint(endpoint.to_owned()))?;
        let stream = TcpStream::connect(authority)?;
        stream.set_nodelay(true)?;
        let (socket, _) = tungstenite::client(endpoint, stream).map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => CdpError::Socket(error),
            tungstenite::HandshakeError::Interrupted(_) => CdpError::Endpoint(endpoint.to_owned()),
        })?;
        Ok(Self { socket, next_id: 1, events: VecDeque::new() })
    }

    fn read_message(&mut self, deadline: Instant) -> Result<Option<Value>, CdpError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(None);
        }
        self.socket.get_ref().set_read_timeout(Some(remaining))?;
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(Some(serde_json::from_str(text.as_str())?)),
            Ok(Message::Close(_)) => Err(CdpError::Socket(tungstenite::Error::ConnectionClosed)),
            Ok(_) => Ok(Some(Value::Null)),
            Err(tungstenite::Error::Io(error))
                if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
            {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Sends one command and returns its `result`; events that arrive meanwhile are kept for [`Cdp::wait_event`].
    pub fn call(&mut self, method: &str, params: Value, session: Option<&str>, timeout: Duration) -> Result<Value, CdpError> {
        let id = self.next_id;
        self.next_id += 1;
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = Value::String(session.to_owned());
        }
        self.socket.send(Message::text(message.to_string()))?;
        let deadline = Instant::now() + timeout;
        loop {
            let Some(reply) = self.read_message(deadline)? else {
                return Err(CdpError::Timeout { method: method.to_owned(), seconds: timeout.as_secs() });
            };
            if reply.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = reply.get("error") {
                    return Err(CdpError::Protocol {
                        method: method.to_owned(),
                        code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                        message: error.get("message").and_then(Value::as_str).unwrap_or_default().to_owned(),
                    });
                }
                return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
            }
            if reply.get("method").is_some() {
                self.events.push_back(reply);
            }
        }
    }

    /// Waits for an event of this session; returns `None` on timeout.
    pub fn wait_event(&mut self, method: &str, session: &str, timeout: Duration) -> Result<Option<Value>, CdpError> {
        let matches = |event: &Value| {
            event.get("method").and_then(Value::as_str) == Some(method)
                && event.get("sessionId").and_then(Value::as_str).is_none_or(|id| id == session)
        };
        if let Some(index) = self.events.iter().position(matches) {
            return Ok(self.events.remove(index));
        }
        let deadline = Instant::now() + timeout;
        while let Some(message) = self.read_message(deadline)? {
            if message.get("method").is_none() {
                continue;
            }
            if matches(&message) {
                return Ok(Some(message));
            }
            self.events.push_back(message);
        }
        Ok(None)
    }
}
