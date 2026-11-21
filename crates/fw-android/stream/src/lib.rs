//! fwa-stream v1. Every message is `u8 kind, u32 window, u32 payload_len` (little endian) followed by the payload;
//! the same bytes travel over TCP, Hyper-V sockets, a unix socket or one WebSocket binary frame per message.
//! Server (compositor) to client: `WindowOpen`, `WindowTitle`, `WindowSize`, `Frame`, `WindowClose`.
//! Client to server: `Pointer`, `Key`, `ResizeRequest`, `CloseRequest`.

use std::fmt;

pub const VERSION: u8 = 1;
const HEADER: usize = 9;
const MAX_PAYLOAD: u32 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    WindowOpen { window: u32, app_id: String, title: String, width: u32, height: u32 },
    WindowTitle { window: u32, title: String },
    WindowSize { window: u32, width: u32, height: u32 },
    /// Damage rectangle of a window, `pixels` is `width * height * 4` bytes of BGRA (XRGB8888 little endian).
    Frame { window: u32, x: u32, y: u32, width: u32, height: u32, pixels: Vec<u8> },
    WindowClose { window: u32 },
    Pointer { window: u32, x: i32, y: i32, buttons: u8 },
    Key { window: u32, code: u32, down: bool },
    ResizeRequest { window: u32, width: u32, height: u32 },
    CloseRequest { window: u32 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecodeError {
    Incomplete,
    UnknownKind(u8),
    Malformed(&'static str),
    TooLarge(u32),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => formatter.write_str("incomplete message"),
            Self::UnknownKind(kind) => write!(formatter, "unknown message kind {kind}"),
            Self::Malformed(why) => write!(formatter, "malformed message: {why}"),
            Self::TooLarge(length) => write!(formatter, "payload of {length} bytes exceeds the limit"),
        }
    }
}

impl std::error::Error for DecodeError {}

fn put_string(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    let length = bytes.len().min(u16::MAX as usize);
    out.extend_from_slice(&(length as u16).to_le_bytes());
    out.extend_from_slice(&bytes[..length]);
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], DecodeError> {
        if self.0.len() < count {
            return Err(DecodeError::Malformed("payload shorter than its fields"));
        }
        let (head, tail) = self.0.split_at(count);
        self.0 = tail;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
    fn i32(&mut self) -> Result<i32, DecodeError> {
        Ok(self.u32()? as i32)
    }
    fn string(&mut self) -> Result<String, DecodeError> {
        let length = self.u16()? as usize;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|_| DecodeError::Malformed("string is not utf-8"))
    }
}

impl Message {
    pub fn window(&self) -> u32 {
        match self {
            Self::WindowOpen { window, .. }
            | Self::WindowTitle { window, .. }
            | Self::WindowSize { window, .. }
            | Self::Frame { window, .. }
            | Self::WindowClose { window }
            | Self::Pointer { window, .. }
            | Self::Key { window, .. }
            | Self::ResizeRequest { window, .. }
            | Self::CloseRequest { window } => *window,
        }
    }

    fn kind(&self) -> u8 {
        match self {
            Self::WindowOpen { .. } => 1,
            Self::WindowTitle { .. } => 2,
            Self::WindowSize { .. } => 3,
            Self::Frame { .. } => 4,
            Self::WindowClose { .. } => 5,
            Self::Pointer { .. } => 16,
            Self::Key { .. } => 17,
            Self::ResizeRequest { .. } => 18,
            Self::CloseRequest { .. } => 19,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        match self {
            Self::WindowOpen { app_id, title, width, height, .. } => {
                put_string(&mut payload, app_id);
                put_string(&mut payload, title);
                payload.extend_from_slice(&width.to_le_bytes());
                payload.extend_from_slice(&height.to_le_bytes());
            }
            Self::WindowTitle { title, .. } => put_string(&mut payload, title),
            Self::WindowSize { width, height, .. } | Self::ResizeRequest { width, height, .. } => {
                payload.extend_from_slice(&width.to_le_bytes());
                payload.extend_from_slice(&height.to_le_bytes());
            }
            Self::Frame { x, y, width, height, pixels, .. } => {
                for value in [x, y, width, height] {
                    payload.extend_from_slice(&value.to_le_bytes());
                }
                payload.extend_from_slice(pixels);
            }
            Self::WindowClose { .. } | Self::CloseRequest { .. } => {}
            Self::Pointer { x, y, buttons, .. } => {
                payload.extend_from_slice(&x.to_le_bytes());
                payload.extend_from_slice(&y.to_le_bytes());
                payload.push(*buttons);
            }
            Self::Key { code, down, .. } => {
                payload.extend_from_slice(&code.to_le_bytes());
                payload.push(u8::from(*down));
            }
        }
        let mut out = Vec::with_capacity(HEADER + payload.len());
        out.push(self.kind());
        out.extend_from_slice(&self.window().to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out
    }

    /// Decodes one message from the front of `bytes` and returns it with the number of bytes consumed.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize), DecodeError> {
        if bytes.len() < HEADER {
            return Err(DecodeError::Incomplete);
        }
        let kind = bytes[0];
        let window = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        let length = u32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        if length > MAX_PAYLOAD {
            return Err(DecodeError::TooLarge(length));
        }
        let end = HEADER + length as usize;
        if bytes.len() < end {
            return Err(DecodeError::Incomplete);
        }
        let mut reader = Reader(&bytes[HEADER..end]);
        let message = match kind {
            1 => {
                let app_id = reader.string()?;
                let title = reader.string()?;
                Self::WindowOpen { window, app_id, title, width: reader.u32()?, height: reader.u32()? }
            }
            2 => Self::WindowTitle { window, title: reader.string()? },
            3 => Self::WindowSize { window, width: reader.u32()?, height: reader.u32()? },
            4 => {
                let (x, y, width, height) = (reader.u32()?, reader.u32()?, reader.u32()?, reader.u32()?);
                let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
                if reader.0.len() != expected {
                    return Err(DecodeError::Malformed("frame pixel count does not match its rectangle"));
                }
                Self::Frame { window, x, y, width, height, pixels: reader.0.to_vec() }
            }
            5 => Self::WindowClose { window },
            16 => Self::Pointer { window, x: reader.i32()?, y: reader.i32()?, buttons: reader.u8()? },
            17 => Self::Key { window, code: reader.u32()?, down: reader.u8()? != 0 },
            18 => Self::ResizeRequest { window, width: reader.u32()?, height: reader.u32()? },
            19 => Self::CloseRequest { window },
            other => return Err(DecodeError::UnknownKind(other)),
        };
        Ok((message, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<Message> {
        vec![
            Message::WindowOpen { window: 7, app_id: "org.example.app".into(), title: "Titre éàü".into(), width: 1080, height: 1920 },
            Message::WindowTitle { window: 7, title: "x".into() },
            Message::WindowSize { window: 7, width: 640, height: 480 },
            Message::Frame { window: 7, x: 1, y: 2, width: 2, height: 2, pixels: (0..16).collect() },
            Message::WindowClose { window: 7 },
            Message::Pointer { window: 7, x: -3, y: 40, buttons: 1 },
            Message::Key { window: 7, code: 30, down: true },
            Message::ResizeRequest { window: 7, width: 800, height: 600 },
            Message::CloseRequest { window: 7 },
        ]
    }

    #[test]
    fn every_message_round_trips_through_a_concatenated_stream() {
        let messages = samples();
        let stream: Vec<u8> = messages.iter().flat_map(Message::encode).collect();
        let mut offset = 0;
        let mut decoded = Vec::new();
        while offset < stream.len() {
            let (message, used) = Message::decode(&stream[offset..]).expect("decode");
            decoded.push(message);
            offset += used;
        }
        assert_eq!(decoded, messages);
    }

    #[test]
    fn truncated_and_corrupt_input_is_rejected() {
        let bytes = samples()[3].encode();
        assert_eq!(Message::decode(&bytes[..bytes.len() - 1]), Err(DecodeError::Incomplete));
        assert_eq!(Message::decode(&bytes[..4]), Err(DecodeError::Incomplete));
        let mut unknown = bytes.clone();
        unknown[0] = 99;
        assert_eq!(Message::decode(&unknown), Err(DecodeError::UnknownKind(99)));
        let mut shortened = Message::Frame { window: 1, x: 0, y: 0, width: 2, height: 2, pixels: vec![0; 16] }.encode();
        shortened[5] = 12;
        shortened.truncate(HEADER + 12);
        assert!(matches!(Message::decode(&shortened), Err(DecodeError::Malformed(_))));
    }
}
