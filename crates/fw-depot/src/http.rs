//! HTTP for the tools: one agent, retries on transient failures, bodies of any size.

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result, fs};

const USER_AGENT: &str = concat!("fw-depot/", env!("CARGO_PKG_VERSION"));

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(900)))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn failure(url: &str, message: impl std::fmt::Display) -> Error {
    Error::Http {
        url: url.to_owned(),
        message: message.to_string(),
    }
}

fn transient(status: u16) -> bool {
    status == 429 || status >= 500
}

/// A request with its status and body, retried on a connection error, 429 and 5xx.
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub fn request(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: Option<&[u8]>,
) -> Result<Response> {
    let agent = agent();
    let mut last = String::new();
    for attempt in 0..4u64 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
        let outcome = match (method, body) {
            ("POST", Some(payload)) => {
                let mut request = agent.post(url);
                for (name, value) in headers {
                    request = request.header(*name, value);
                }
                request.send(payload)
            }
            ("PUT", Some(payload)) => {
                let mut request = agent.put(url);
                for (name, value) in headers {
                    request = request.header(*name, value);
                }
                request.send(payload)
            }
            (_, _) => {
                let mut request = agent.get(url);
                for (name, value) in headers {
                    request = request.header(*name, value);
                }
                request.call()
            }
        };
        match outcome {
            Ok(mut response) => {
                let status = response.status().as_u16();
                if transient(status) {
                    last = format!("status {status}");
                    continue;
                }
                let bytes = response
                    .body_mut()
                    .with_config()
                    .limit(u64::MAX)
                    .read_to_vec()
                    .map_err(|error| failure(url, error))?;
                return Ok(Response { status, body: bytes });
            }
            Err(error) => last = error.to_string(),
        }
    }
    Err(failure(url, last))
}

pub fn get(url: &str, headers: &[(&str, String)]) -> Result<Response> {
    request("GET", url, headers, None)
}

/// A 2xx response body or an error carrying the status and the start of the body.
pub fn get_ok(url: &str, headers: &[(&str, String)]) -> Result<Vec<u8>> {
    let response = get(url, headers)?;
    if (200..300).contains(&response.status) {
        return Ok(response.body);
    }
    Err(failure(
        url,
        format!(
            "status {}: {}",
            response.status,
            response.text().chars().take(200).collect::<String>()
        ),
    ))
}

/// Streams a GET into `path` and returns the SHA-256 of what was written.
pub fn download(url: &str, headers: &[(&str, String)], path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let agent = agent();
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, value);
    }
    let mut response = request.call().map_err(|error| failure(url, error))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(failure(url, format!("status {status}")));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = path.with_extension("part");
    let mut file = std::fs::File::create(&partial).map_err(|source| Error::io(&partial, source))?;
    let mut reader = response.body_mut().as_reader();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| failure(url, error))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|source| Error::io(&partial, source))?;
    }
    file.flush().map_err(|source| Error::io(&partial, source))?;
    drop(file);
    std::fs::rename(&partial, path).map_err(|source| Error::io(path, source))?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn post_json(url: &str, headers: &[(&str, String)], body: &serde_json::Value) -> Result<serde_json::Value> {
    send_json("POST", url, headers, body)
}

/// JSON request with a body (`POST` or `PUT`); the XSSI prefix `)]}'` of pRPC and Gerrit is stripped from the reply.
pub fn send_json(
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let mut all = vec![
        ("Content-Type", "application/json".to_owned()),
        ("Accept", "application/json".to_owned()),
    ];
    all.extend(headers.iter().map(|(name, value)| (*name, value.clone())));
    let payload = serde_json::to_vec(body)?;
    let response = request(method, url, &all, Some(&payload))?;
    parse_json(url, &response)
}

pub fn parse_json(url: &str, response: &Response) -> Result<serde_json::Value> {
    let text = response.text();
    if !(200..300).contains(&response.status) {
        return Err(failure(
            url,
            format!("status {}: {}", response.status, text.chars().take(300).collect::<String>()),
        ));
    }
    let trimmed = text.strip_prefix(")]}'").unwrap_or(&text).trim_start();
    serde_json::from_str(trimmed).map_err(Error::from)
}
