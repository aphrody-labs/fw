//! A blocking facade over [`crate::Client`] for synchronous code that has no async runtime of its own (command-line tools, trait
//! seams that are not async). Every call runs on one shared tokio runtime and blocks the calling thread until it finishes, so the
//! connection pools, h3 upgrade, Alt-Svc cache and retries of the async client apply unchanged.
//!
//! Do not call it from a thread that must stay responsive inside an async runtime: it parks the thread like any blocking call.

use std::future::Future;
use std::io::{self, Read};
use std::sync::OnceLock;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{IntoUrl, Method, Result, StatusCode, Url};

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ay-http-blocking")
            .enable_all()
            .build()
            .unwrap_or_else(|error| panic!("the blocking HTTP runtime cannot start: {error}"))
    })
}

/// Runs `future` on the shared runtime and waits for its output.
pub fn block_on<T: Send + 'static>(future: impl Future<Output = T> + Send + 'static) -> T {
    let (done, wait) = std::sync::mpsc::channel();
    runtime().spawn(async move {
        let _ = done.send(future.await);
    });
    wait.recv().unwrap_or_else(|_| panic!("the blocking HTTP runtime dropped a task"))
}

/// The blocking client; cloning shares the pools of the async client underneath.
#[derive(Clone, Debug, Default)]
pub struct Client {
    inner: crate::Client,
}

impl Client {
    /// A client with the defaults of the stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A client over a configured async builder (timeouts, user agent, ...).
    ///
    /// # Errors
    /// When the TLS backend or an option is invalid.
    pub fn from_builder(builder: crate::ClientBuilder) -> Result<Self> {
        let _guard = runtime().enter();
        Ok(Self { inner: builder.build()? })
    }

    /// A request builder.
    pub fn request<U: IntoUrl>(&self, method: Method, url: U) -> RequestBuilder {
        RequestBuilder { inner: self.inner.request(method, url) }
    }

    /// `GET`.
    pub fn get<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    /// `POST`.
    pub fn post<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::POST, url)
    }
}

/// A request under construction.
#[derive(Debug)]
#[must_use = "a RequestBuilder does nothing until it is sent"]
pub struct RequestBuilder {
    inner: crate::RequestBuilder,
}

impl RequestBuilder {
    /// Adds a header.
    pub fn header<K, V>(self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        Self { inner: self.inner.header(key, value) }
    }

    /// Bearer token.
    pub fn bearer_auth<T: std::fmt::Display>(self, token: T) -> Self {
        Self { inner: self.inner.bearer_auth(token) }
    }

    /// Raw body.
    pub fn body(self, body: impl Into<Vec<u8>>) -> Self {
        Self { inner: self.inner.body(body.into()) }
    }

    /// Per-request timeout.
    pub fn timeout(self, timeout: Duration) -> Self {
        Self { inner: self.inner.timeout(timeout) }
    }

    /// Sends the request and returns once the response head has arrived.
    ///
    /// # Errors
    /// Builder and transport errors.
    pub fn send(self) -> Result<Response> {
        let inner = self.inner;
        let response = block_on(async move { inner.send().await })?;
        Ok(Response { status: response.status(), headers: response.headers().clone(), url: response.url().clone(), body: Some(response), pending: Bytes::new() })
    }
}

/// A response whose body is read on demand; it also implements [`Read`].
#[derive(Debug)]
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    url: Url,
    body: Option<crate::Response>,
    pending: Bytes,
}

impl Response {
    /// The status code.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// The headers.
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The final URL after redirects.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The next chunk of the body, `None` at its end.
    ///
    /// # Errors
    /// Transport errors while reading.
    pub fn chunk(&mut self) -> Result<Option<Bytes>> {
        let Some(mut response) = self.body.take() else { return Ok(None) };
        let (response, chunk) = block_on(async move {
            let chunk = response.chunk().await;
            (response, chunk)
        });
        match chunk {
            Ok(Some(chunk)) => {
                self.body = Some(response);
                Ok(Some(chunk))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// The whole remaining body.
    ///
    /// # Errors
    /// Transport errors while reading.
    pub fn bytes(mut self) -> Result<Bytes> {
        let mut out = Vec::from(std::mem::take(&mut self.pending));
        while let Some(chunk) = self.chunk()? {
            out.extend_from_slice(&chunk);
        }
        Ok(Bytes::from(out))
    }

    /// The whole remaining body as JSON.
    ///
    /// # Errors
    /// Transport or decoding errors.
    pub fn json<T: serde::de::DeserializeOwned>(self) -> std::result::Result<T, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = self.bytes()?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

impl Read for Response {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            match self.chunk().map_err(io::Error::other)? {
                Some(chunk) => self.pending = chunk,
                None => return Ok(0),
            }
        }
        let n = out.len().min(self.pending.len());
        out[..n].copy_from_slice(&self.pending[..n]);
        bytes::Buf::advance(&mut self.pending, n);
        Ok(n)
    }
}
