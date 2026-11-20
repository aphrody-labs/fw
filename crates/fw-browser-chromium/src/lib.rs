//! Chromium through CEF (`cef` crate, tauri-apps/cef-rs) from Rust, the `browser-chromium` binary and Bun.
//!
//! One engine per process starts CEF with off-screen rendering; every page is one browser. Opening a page waits for
//! the main frame to load; a screenshot paints a frame, an evaluation runs `Runtime.evaluate` over the DevTools
//! protocol. [`Backend`] and [`Page`] are the engine-neutral shape; [`Chromium`] implements them.

mod cef_dir;
mod engine;
#[cfg(feature = "node")]
mod node;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

pub use cef_dir::{CefDir, install_cef};
pub use engine::subprocess_main;

/// The CEF build this crate is compiled against, for example `156.0.30+g6028375+chromium-156.0.8078.96`.
pub fn cef_version() -> String {
    let raw = cef::sys::CEF_VERSION;
    String::from_utf8_lossy(&raw[..raw.len() - 1]).into_owned()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("CEF binary distribution not found: {0}")]
    CefNotFound(String),
    #[error("CEF failed to start: {0}")]
    Start(String),
    #[error("page failed to load: {0}")]
    Load(String),
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("javascript error: {0}")]
    Script(String),
    #[error("unexpected DevTools reply: {0}")]
    Protocol(String),
    #[error("PNG encoding failed: {0}")]
    Png(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("the engine is shut down")]
    Closed,
}

pub type Result<T> = std::result::Result<T, Error>;

/// How the engine finds CEF and the helper executable that CEF re-launches for its renderer, GPU and utility
/// processes. In the `browser-chromium` binary the helper is the binary itself; in a host such as Bun it is the
/// `browser-chromium` binary shipped next to the addon.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Directory holding `libcef.so`, the `.pak` files and `locales/`; discovered when `None`.
    pub cef_dir: Option<PathBuf>,
    /// Executable CEF launches for child processes; the current executable when `None`.
    pub helper: Option<PathBuf>,
    /// Directory for CEF's logs and profile; a fresh temporary directory when `None`.
    pub cache_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ScreenshotOptions {
    pub width: u32,
    pub height: u32,
    /// Wait before painting, for late scripts and images.
    pub settle: Duration,
    pub timeout: Duration,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            settle: Duration::from_millis(500),
            timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EvaluateOptions {
    /// Wait before evaluating.
    pub settle: Duration,
    pub timeout: Duration,
}

impl Default for EvaluateOptions {
    fn default() -> Self {
        Self {
            settle: Duration::ZERO,
            timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Screenshot {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

/// A browser engine that opens pages: the shape the future `ay_browser` crate plugs backends into. [`Chromium`]
/// (CEF) is one; a CDP client over Chrome for Testing or a headless shell would be another.
pub trait Backend {
    type Page: Page;

    /// Open `url` and return once its main frame has loaded.
    fn open(&self, url: &str) -> Result<Self::Page>;
}

/// An open page. Dropping it closes it.
pub trait Page {
    /// Write a PNG of the viewport to `out`.
    fn screenshot(&self, out: &Path, options: &ScreenshotOptions) -> Result<Screenshot>;

    /// Evaluate a JavaScript expression in the page; promises are awaited and the value is returned by value as
    /// JSON (`undefined` is `null`).
    fn evaluate(&self, expression: &str, options: &EvaluateOptions) -> Result<serde_json::Value>;
}

/// A running CEF engine. CEF initializes once per process, so use [`Chromium::global`] unless you own the process.
pub struct Chromium {
    engine: engine::Engine,
}

/// One page of the CEF engine.
pub struct ChromiumPage {
    handle: engine::PageHandle,
}

static GLOBAL: OnceLock<std::result::Result<Chromium, String>> = OnceLock::new();

impl Chromium {
    pub fn start(config: Config) -> Result<Self> {
        Ok(Self {
            engine: engine::Engine::start(config)?,
        })
    }

    /// The process-wide engine, started on first use with `config` (later configs are ignored).
    pub fn global(config: Config) -> Result<&'static Chromium> {
        GLOBAL
            .get_or_init(|| Chromium::start(config).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| Error::Start(e.clone()))
    }

    /// Close every page and shut CEF down. CEF cannot be started again in the same process; drop your pages first.
    pub fn shutdown(&self) {
        self.engine.close();
    }

    /// Like [`Backend::open`], with a load timeout (default 30 s).
    pub fn open_with(&self, url: &str, timeout: Duration) -> Result<ChromiumPage> {
        Ok(ChromiumPage {
            handle: engine::PageHandle::open(url, timeout)?,
        })
    }

    /// Open `url`, write a PNG of the viewport to `out` and close the page.
    pub fn screenshot(
        &self,
        url: &str,
        out: impl AsRef<Path>,
        options: &ScreenshotOptions,
    ) -> Result<Screenshot> {
        self.open_with(url, options.timeout)?
            .screenshot(out.as_ref(), options)
    }

    /// Open `url`, evaluate the expression in the page and close the page.
    pub fn evaluate(
        &self,
        url: &str,
        expression: &str,
        options: &EvaluateOptions,
    ) -> Result<serde_json::Value> {
        self.open_with(url, options.timeout)?
            .evaluate(expression, options)
    }
}

impl Backend for Chromium {
    type Page = ChromiumPage;

    fn open(&self, url: &str) -> Result<ChromiumPage> {
        self.open_with(url, Duration::from_secs(30))
    }
}

impl Page for ChromiumPage {
    fn screenshot(&self, out: &Path, options: &ScreenshotOptions) -> Result<Screenshot> {
        std::thread::sleep(options.settle);
        let frame = self
            .handle
            .screenshot(options.width, options.height, options.timeout)?;
        write_png(out, frame.width, frame.height, &frame.bgra)?;
        Ok(Screenshot {
            path: out.to_path_buf(),
            width: frame.width,
            height: frame.height,
        })
    }

    fn evaluate(&self, expression: &str, options: &EvaluateOptions) -> Result<serde_json::Value> {
        std::thread::sleep(options.settle);
        self.handle.evaluate(expression, options.timeout)
    }
}

fn write_png(path: &Path, width: u32, height: u32, bgra: &[u8]) -> Result<()> {
    let mut rgba = bgra.to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
        px[3] = 255;
    }
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::Png(e.to_string()))?;
    writer
        .write_image_data(&rgba)
        .map_err(|e| Error::Png(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_frame_is_written_as_rgba_png() {
        let path =
            std::env::temp_dir().join(format!("browser-chromium-unit-{}.png", std::process::id()));
        write_png(&path, 2, 1, &[0, 0, 255, 0, 255, 0, 0, 0]).unwrap();
        let mut reader =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()))
                .read_info()
                .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(pixels, [255, 0, 0, 255, 0, 0, 255, 255]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn cef_version_names_the_chromium_build() {
        assert!(cef_version().contains("+chromium-"), "{}", cef_version());
    }

    #[test]
    fn locate_accepts_an_explicit_directory_with_the_runtime() {
        let dir = std::env::temp_dir().join(format!("browser-chromium-cef-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("locales")).unwrap();
        std::fs::write(
            dir.join(if cfg!(windows) {
                "libcef.dll"
            } else {
                "libcef.so"
            }),
            b"",
        )
        .unwrap();
        assert_eq!(CefDir::locate(Some(&dir), &[]).unwrap().path(), dir);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
