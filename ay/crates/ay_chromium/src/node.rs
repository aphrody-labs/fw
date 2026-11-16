//! Node-API addon (the form Bun documents as its stable native interface): the same engine as a `.node` file.
#![allow(unsafe_code)]
// napi-derive registers every item at load time; only the cdylib uses them, the rlib and the binary do not.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Task};
use napi_derive::napi;

use crate::{Chromium, Config, EvaluateOptions, ScreenshotOptions};

static CONFIG: OnceLock<Config> = OnceLock::new();

fn engine() -> napi::Result<&'static Chromium> {
    Chromium::global(CONFIG.get().cloned().unwrap_or_default()).map_err(to_napi)
}

fn to_napi(error: crate::Error) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

#[napi(object)]
pub struct NativeConfig {
    /// Directory with libcef, the .pak files and locales/.
    pub cef_dir: Option<String>,
    /// The ay-chromium executable CEF launches for child processes.
    pub helper: Option<String>,
    pub cache_dir: Option<String>,
}

/// Set where CEF and its helper live; call before the first screenshot or evaluate.
#[napi]
pub fn configure(config: NativeConfig) {
    let _ = CONFIG.set(Config {
        cef_dir: config.cef_dir.map(PathBuf::from),
        helper: config.helper.map(PathBuf::from),
        cache_dir: config.cache_dir.map(PathBuf::from),
    });
}

#[napi]
pub fn cef_version() -> String {
    crate::cef_version()
}

/// Shut CEF down; it cannot be started again in this process.
#[napi]
pub fn shutdown() {
    if let Ok(chromium) = engine() {
        chromium.shutdown();
    }
}

#[napi(object)]
pub struct ScreenshotResult {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

pub struct ScreenshotTask {
    url: String,
    out: String,
    options: ScreenshotOptions,
}

impl Task for ScreenshotTask {
    type Output = crate::Screenshot;
    type JsValue = ScreenshotResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        engine()?
            .screenshot(&self.url, &self.out, &self.options)
            .map_err(to_napi)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(ScreenshotResult {
            path: output.path.to_string_lossy().into_owned(),
            width: output.width,
            height: output.height,
        })
    }
}

#[napi]
pub fn screenshot(
    url: String,
    out: String,
    width: Option<u32>,
    height: Option<u32>,
    settle_ms: Option<u32>,
    timeout_ms: Option<u32>,
) -> AsyncTask<ScreenshotTask> {
    let mut options = ScreenshotOptions::default();
    options.width = width.unwrap_or(options.width);
    options.height = height.unwrap_or(options.height);
    options.settle = settle_ms.map_or(options.settle, |ms| Duration::from_millis(ms.into()));
    options.timeout = timeout_ms.map_or(options.timeout, |ms| Duration::from_millis(ms.into()));
    AsyncTask::new(ScreenshotTask { url, out, options })
}

pub struct EvaluateTask {
    url: String,
    expression: String,
    options: EvaluateOptions,
}

impl Task for EvaluateTask {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let value = engine()?
            .evaluate(&self.url, &self.expression, &self.options)
            .map_err(to_napi)?;
        Ok(value.to_string())
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

/// Evaluate `expression` in the page at `url`; resolves to the JSON text of the value.
#[napi]
pub fn evaluate(
    url: String,
    expression: String,
    settle_ms: Option<u32>,
    timeout_ms: Option<u32>,
) -> AsyncTask<EvaluateTask> {
    let mut options = EvaluateOptions::default();
    options.settle = settle_ms.map_or(options.settle, |ms| Duration::from_millis(ms.into()));
    options.timeout = timeout_ms.map_or(options.timeout, |ms| Duration::from_millis(ms.into()));
    AsyncTask::new(EvaluateTask {
        url,
        expression,
        options,
    })
}
