// SPDX-License-Identifier: Apache-2.0
//! Engine binaries, their launch, and the page operations of `fw browser` (open, screenshot, eval, pdf) over CDP.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use fw_browser_engine::process::command;
use serde_json::{Value, json};
use url::Url;

use crate::cdp::{Cdp, CdpError};

const STEP: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Cdp(#[from] CdpError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("no `{0}` engine binary found: {1}")]
    NoBinary(&'static str, String),
    #[error("the {0} engine did not publish a DevTools endpoint within 20 s")]
    NoEndpoint(&'static str),
    #[error("invalid URL `{0}`")]
    Url(String),
    #[error("{0}")]
    Page(String),
}

/// The two engines `fw browser` drives. `Lite` is Obscura (V8, wgpu, tiny-skia); `Blink` is Chromium headless.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Lite,
    Blink,
}

impl Engine {
    pub fn id(self) -> &'static str {
        match self {
            Self::Lite => "lite",
            Self::Blink => "blink",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "lite" | "obscura" => Some(Self::Lite),
            "blink" | "chrome" | "chromium" => Some(Self::Blink),
            _ => None,
        }
    }

    /// The environment variable that pins the binary, then the file names searched next to `fw` and on `PATH`.
    fn candidates(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::Lite => ("FW_BROWSER_LITE", &["fw-browser-lite", "obscura"]),
            Self::Blink => (
                "FW_BROWSER_BLINK",
                &["chrome-headless-shell", "google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome"],
            ),
        }
    }

    /// Resolves the engine binary: the environment variable, then siblings of the running executable, then `PATH`.
    pub fn locate(self) -> Result<PathBuf, SessionError> {
        let (variable, names) = self.candidates();
        if let Some(path) = std::env::var_os(variable).map(PathBuf::from).filter(|path| path.is_file()) {
            return Ok(path);
        }
        let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        let search: Vec<PathBuf> = beside
            .into_iter()
            .chain(std::env::var_os("PATH").iter().flat_map(std::env::split_paths))
            .collect();
        for name in names {
            for dir in &search {
                let candidate = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
        #[cfg(windows)]
        if self == Self::Blink {
            for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
                if let Some(base) = std::env::var_os(root) {
                    let candidate = Path::new(&base).join("Google/Chrome/Application/chrome.exe");
                    if candidate.is_file() {
                        return Ok(candidate);
                    }
                }
            }
        }
        Err(SessionError::NoBinary(self.id(), format!("set {variable}, or put one of {names:?} next to `fw` or on PATH")))
    }
}

fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| status.lines().find_map(|line| line.strip_prefix("Uid:").map(str::to_owned)))
        .is_some_and(|uids| uids.split_whitespace().next() == Some("0"))
}

fn is_private_host(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(domain)) => domain == "localhost" || domain.ends_with(".localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

/// Arguments that start `engine` as a DevTools server on `port`. `private` lets the lite engine fetch loopback and LAN hosts.
pub fn serve_arguments(engine: Engine, port: u16, profile: &Path, private: bool) -> Vec<String> {
    let mut args = Vec::new();
    match engine {
        Engine::Lite => {
            args.extend(["serve".to_owned(), "--port".to_owned(), port.to_string()]);
            if private {
                args.push("--allow-private-network".to_owned());
            }
        }
        Engine::Blink => {
            args.extend([
                "--headless=new".to_owned(),
                format!("--remote-debugging-port={port}"),
                format!("--user-data-dir={}", profile.display()),
                "--no-first-run".to_owned(),
                "--enable-unsafe-webgpu".to_owned(),
                "--enable-features=Vulkan".to_owned(),
            ]);
            if is_root() {
                args.push("--no-sandbox".to_owned());
            }
            if let Ok(extra) = std::env::var("FW_BROWSER_BLINK_ARGS") {
                args.extend(extra.split_whitespace().map(str::to_owned));
            }
            args.push("about:blank".to_owned());
        }
    }
    args
}

/// A running engine process with its DevTools endpoint; the process is killed when this is dropped.
pub struct Server {
    child: Child,
    profile: PathBuf,
    pub engine: Engine,
    pub port: u16,
    pub endpoint: String,
}

impl Server {
    pub fn start(engine: Engine, private: bool) -> Result<Self, SessionError> {
        let binary = engine.locate()?;
        let port = free_port()?;
        let profile = std::env::temp_dir().join(format!("fw-browser-{}-{port}", std::process::id()));
        let child = command(&binary)
            .args(serve_arguments(engine, port, &profile, private))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let mut server = Self { child, profile, engine, port, endpoint: String::new() };
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(endpoint) = fetch_endpoint(port) {
                server.endpoint = endpoint;
                return Ok(server);
            }
            if server.child.try_wait()?.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(SessionError::NoEndpoint(engine.id()))
    }
}

fn fetch_endpoint(port: u16) -> Option<String> {
    let body = ureq::get(format!("http://127.0.0.1:{port}/json/version")).call().ok()?.body_mut().read_to_string().ok()?;
    let version: Value = serde_json::from_str(&body).ok()?;
    version.get("webSocketDebuggerUrl").and_then(Value::as_str).map(str::to_owned)
}

impl Drop for Server {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
        std::fs::remove_dir_all(&self.profile).ok();
    }
}

/// One attached page of an engine, after navigation.
pub struct Page {
    cdp: Cdp,
    session: String,
    pub final_url: String,
    _server: Server,
}

#[derive(Clone, Debug)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

impl Page {
    /// Starts `engine`, opens `raw_url` in a fresh target at `viewport` and waits for the load event.
    pub fn open(engine: Engine, raw_url: &str, viewport: Option<Viewport>) -> Result<Self, SessionError> {
        let url = Url::parse(raw_url).map_err(|_| SessionError::Url(raw_url.to_owned()))?;
        let server = Server::start(engine, is_private_host(&url))?;
        let mut cdp = Cdp::connect(&server.endpoint)?;
        let target = cdp.call("Target.createTarget", json!({ "url": "about:blank" }), None, STEP)?;
        let target_id = target.get("targetId").and_then(Value::as_str).unwrap_or_default().to_owned();
        let attached = cdp.call("Target.attachToTarget", json!({ "targetId": target_id, "flatten": true }), None, STEP)?;
        let session = attached
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| SessionError::Page("Target.attachToTarget returned no sessionId".to_owned()))?
            .to_owned();
        cdp.call("Page.enable", json!({}), Some(&session), STEP)?;
        if let Some(Viewport { width, height }) = viewport {
            cdp.call(
                "Emulation.setDeviceMetricsOverride",
                json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }),
                Some(&session),
                STEP,
            )?;
        }
        let navigated = cdp.call("Page.navigate", json!({ "url": url.as_str() }), Some(&session), STEP)?;
        if let Some(error) = navigated.get("errorText").and_then(Value::as_str) {
            return Err(SessionError::Page(format!("navigation to {url} failed: {error}")));
        }
        if cdp.wait_event("Page.loadEventFired", &session, STEP)?.is_none() {
            wait_ready(&mut cdp, &session)?;
        }
        let mut page = Self { cdp, session, final_url: url.to_string(), _server: server };
        if let Some(href) = page.eval("location.href")?.as_str() {
            page.final_url = href.to_owned();
        }
        Ok(page)
    }

    /// Evaluates an expression in the page (promises are awaited) and returns its JSON value.
    pub fn eval(&mut self, expression: &str) -> Result<Value, SessionError> {
        let answer = self.cdp.call(
            "Runtime.evaluate",
            json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }),
            Some(&self.session),
            STEP,
        )?;
        if let Some(details) = answer.get("exceptionDetails") {
            let text = details
                .pointer("/exception/description")
                .or_else(|| details.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("exception");
            return Err(SessionError::Page(text.to_owned()));
        }
        Ok(answer.pointer("/result/value").cloned().unwrap_or(Value::Null))
    }

    /// Title, final URL and the visible text (at most `limit` characters).
    pub fn summary(&mut self, limit: usize) -> Result<Value, SessionError> {
        let expression = format!(
            "({{ title: document.title, url: location.href, text: (document.body ? document.body.innerText : '').slice(0, {limit}) }})"
        );
        self.eval(&expression)
    }

    fn data(&mut self, method: &str, params: Value) -> Result<Vec<u8>, SessionError> {
        let answer = self.cdp.call(method, params, Some(&self.session), STEP * 2)?;
        let data = answer
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| SessionError::Page(format!("{method} returned no data")))?;
        STANDARD.decode(data).map_err(|error| SessionError::Page(format!("{method}: {error}")))
    }

    pub fn screenshot(&mut self, full_page: bool) -> Result<Vec<u8>, SessionError> {
        self.data("Page.captureScreenshot", json!({ "format": "png", "captureBeyondViewport": full_page }))
    }

    pub fn pdf(&mut self) -> Result<Vec<u8>, SessionError> {
        let answer = self.cdp.call("Page.printToPDF", json!({ "printBackground": true }), Some(&self.session), STEP * 2)?;
        let data = answer
            .get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| SessionError::Page("Page.printToPDF returned no data".to_owned()))?;
        STANDARD.decode(data).map_err(|error| SessionError::Page(format!("Page.printToPDF: {error}")))
    }
}

fn wait_ready(cdp: &mut Cdp, session: &str) -> Result<(), SessionError> {
    let deadline = Instant::now() + STEP;
    while Instant::now() < deadline {
        let state = cdp.call(
            "Runtime.evaluate",
            json!({ "expression": "document.readyState", "returnByValue": true }),
            Some(session),
            STEP,
        )?;
        if state.pointer("/result/value").and_then(Value::as_str) == Some("complete") {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(SessionError::Page("the page did not reach readyState complete within 30 s".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lite_serves_with_private_flag_only_for_private_hosts() {
        let profile = Path::new("/tmp/x");
        assert_eq!(serve_arguments(Engine::Lite, 9, profile, false), ["serve", "--port", "9"]);
        assert!(serve_arguments(Engine::Lite, 9, profile, true).contains(&"--allow-private-network".to_owned()));
    }

    #[test]
    fn blink_serves_headless_with_a_profile_and_webgpu() {
        let args = serve_arguments(Engine::Blink, 9222, Path::new("/tmp/p"), false);
        assert!(args.contains(&"--headless=new".to_owned()));
        assert!(args.contains(&"--remote-debugging-port=9222".to_owned()));
        assert!(args.contains(&"--user-data-dir=/tmp/p".to_owned()));
        assert!(args.contains(&"--enable-unsafe-webgpu".to_owned()));
    }

    #[test]
    fn private_hosts_are_loopback_and_lan() {
        for yes in ["http://127.0.0.1/", "http://localhost:8080/", "http://10.0.0.2/", "http://[::1]/"] {
            assert!(is_private_host(&Url::parse(yes).unwrap()), "{yes}");
        }
        assert!(!is_private_host(&Url::parse("https://example.com/").unwrap()));
    }

    #[test]
    fn engine_names_parse() {
        assert_eq!(Engine::parse("obscura"), Some(Engine::Lite));
        assert_eq!(Engine::parse("chrome"), Some(Engine::Blink));
        assert_eq!(Engine::parse("servo"), None);
    }
}
