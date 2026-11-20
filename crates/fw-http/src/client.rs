//! The async client: reqwest underneath, with the Alt-Svc driven h3 upgrade on top.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{IntoUrl, Method, Request, Response, Result, StatusCode};

use crate::altsvc::{AltSvcCache, Origin};

type Op = Arc<dyn Fn(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send + Sync>;

/// Builds a [`Client`]. Every option is applied to both the TCP (h1, h2) and the QUIC (h3) transport.
#[derive(Clone)]
pub struct ClientBuilder {
    ops: Vec<Op>,
    h3: bool,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self { ops: Vec::new(), h3: true }
    }
}

#[cfg_attr(target_family = "wasm", allow(unused_macros))]
macro_rules! option {
    ($(#[$doc:meta])* $name:ident($($arg:ident: $ty:ty),*)) => {
        $(#[$doc])*
        #[must_use]
        pub fn $name(mut self, $($arg: $ty),*) -> Self {
            $(let $arg = $arg.clone();)*
            self.ops.push(Arc::new(move |b| b.$name($($arg.clone()),*)));
            self
        }
    };
}

impl ClientBuilder {
    /// A builder with the defaults of the stack: TCP keepalive, `TCP_NODELAY`, a 90 s idle pool, h3 upgrade on.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the `User-Agent`.
    #[must_use]
    pub fn user_agent<V>(mut self, value: V) -> Self
    where
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        if let Ok(value) = value.try_into() {
            self.ops.push(Arc::new(move |b| b.user_agent(value.clone())));
        }
        self
    }

    /// Headers sent with every request.
    #[must_use]
    pub fn default_headers(mut self, headers: HeaderMap) -> Self {
        self.ops.push(Arc::new(move |b| b.default_headers(headers.clone())));
        self
    }

    /// Any other `reqwest` option; the closure runs once per transport.
    #[must_use]
    pub fn with(mut self, f: impl Fn(reqwest::ClientBuilder) -> reqwest::ClientBuilder + Send + Sync + 'static) -> Self {
        self.ops.push(Arc::new(f));
        self
    }

    fn base(&self) -> reqwest::ClientBuilder {
        #[cfg(not(target_family = "wasm"))]
        let mut b = reqwest::Client::builder().tcp_nodelay(true).pool_idle_timeout(Duration::from_secs(90));
        #[cfg(target_family = "wasm")]
        let mut b = reqwest::Client::builder();
        for op in &self.ops {
            b = op(b);
        }
        b
    }
    /// Builds the client (the h3 transports are built on first use).
    ///
    /// # Errors
    /// Fails when the TLS backend or a configured option is invalid.
    pub fn build(self) -> Result<Client> {
        let base = self.base().build()?;
        Ok(Client {
            retries: 0,
            inner: Arc::new(Inner {
                base,
                #[cfg(not(target_family = "wasm"))]
                h3: self.h3.then(|| H3Transports { builder: self, plain: OnceLock::new(), early: OnceLock::new() }),
                alt: AltSvcCache::default(),
            }),
        })
    }
}

#[cfg(not(target_family = "wasm"))]
impl ClientBuilder {
    option!(/// Total request timeout.
        timeout(value: Duration));
    option!(/// Connect timeout.
        connect_timeout(value: Duration));
    option!(/// Timeout between two reads of a response body.
        read_timeout(value: Duration));
    /// Idle time before a pooled connection is closed.
    #[must_use]
    pub fn pool_idle_timeout(mut self, value: impl Into<Option<Duration>>) -> Self {
        let value = value.into();
        self.ops.push(Arc::new(move |b| b.pool_idle_timeout(value)));
        self
    }

    option!(/// Idle connections kept per host.
        pool_max_idle_per_host(value: usize));
    option!(/// Cookie jar on or off.
        cookie_store(enable: bool));
    option!(/// Accept invalid certificates (development only).
        danger_accept_invalid_certs(accept: bool));
    option!(/// Restrict to https.
        https_only(enable: bool));
    option!(/// Add a proxy.
        proxy(proxy: reqwest::Proxy));
    option!(/// Add a trusted root certificate.
        add_root_certificate(cert: reqwest::Certificate));
    /// TCP keepalive interval.
    #[must_use]
    pub fn tcp_keepalive(mut self, value: impl Into<Option<Duration>>) -> Self {
        let value = value.into();
        self.ops.push(Arc::new(move |b| b.tcp_keepalive(value)));
        self
    }

    /// A redirect policy built per transport (a `Policy` itself cannot be cloned).
    #[must_use]
    pub fn redirect(mut self, policy: impl Fn() -> reqwest::redirect::Policy + Send + Sync + 'static) -> Self {
        self.ops.push(Arc::new(move |b| b.redirect(policy())));
        self
    }

    /// HTTP/1.1 only (no h2 negotiation, no h3 upgrade of this client's TCP transport).
    #[must_use]
    pub fn http1_only(mut self) -> Self {
        self.ops.push(Arc::new(reqwest::ClientBuilder::http1_only));
        self
    }

    /// Cleartext and TLS connections speak h2 without negotiation.
    #[must_use]
    pub fn http2_prior_knowledge(mut self) -> Self {
        self.ops.push(Arc::new(reqwest::ClientBuilder::http2_prior_knowledge));
        self
    }

    option!(/// Adaptive h2 flow-control windows.
        http2_adaptive_window(enable: bool));
    option!(/// `TCP_NODELAY` on or off.
        tcp_nodelay(enable: bool));
    option!(/// Transparent gzip decoding.
        gzip(enable: bool));
    option!(/// Transparent brotli decoding.
        brotli(enable: bool));
    option!(/// Transparent deflate decoding.
        deflate(enable: bool));

    /// Bypass every proxy, system ones included.
    #[must_use]
    pub fn no_proxy(mut self) -> Self {
        self.ops.push(Arc::new(reqwest::ClientBuilder::no_proxy));
        self
    }

    /// Pin a domain to an address (the port of the address overrides the URL port when non-zero).
    #[must_use]
    pub fn resolve(mut self, domain: &str, addr: std::net::SocketAddr) -> Self {
        let domain = domain.to_owned();
        self.ops.push(Arc::new(move |b| b.resolve(&domain, addr)));
        self
    }

    /// Follow no redirect.
    #[must_use]
    pub fn redirect_none(mut self) -> Self {
        self.ops.push(Arc::new(|b| b.redirect(reqwest::redirect::Policy::none())));
        self
    }

    /// Follow at most `max` redirects.
    #[must_use]
    pub fn redirect_limited(mut self, max: usize) -> Self {
        self.ops.push(Arc::new(move |b| b.redirect(reqwest::redirect::Policy::limited(max))));
        self
    }

    /// Never use h3 for this client (no Alt-Svc upgrade).
    #[must_use]
    pub fn http3(mut self, enable: bool) -> Self {
        self.h3 = enable;
        self
    }

}

/// The QUIC transports: reqwest sends a request over h3 only when its version is HTTP/3 and the client was built with h3 prior knowledge.
/// `early` additionally enables TLS 1.3 early data in the rustls config and is used for safe methods only (RFC 8470); reqwest 0.13.5 does
/// not yet send 0-RTT data on a resumed connection (seanmonstar/reqwest#3148), so today the request travels as 1-RTT data.
#[cfg(not(target_family = "wasm"))]
struct H3Transports {
    builder: ClientBuilder,
    plain: OnceLock<Option<reqwest::Client>>,
    early: OnceLock<Option<reqwest::Client>>,
}

#[cfg(not(target_family = "wasm"))]
impl H3Transports {
    fn get(&self, safe_method: bool) -> Option<&reqwest::Client> {
        let (cell, early) = if safe_method { (&self.early, true) } else { (&self.plain, false) };
        cell.get_or_init(|| self.builder.base().http3_prior_knowledge().tls_early_data(early).build().ok()).as_ref()
    }
}

struct Inner {
    base: reqwest::Client,
    #[cfg(not(target_family = "wasm"))]
    h3: Option<H3Transports>,
    alt: AltSvcCache,
}

/// The HTTP client of the SDK. Cloning is cheap and shares the connection pools and the Alt-Svc cache.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    retries: u32,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// A client with the defaults of the stack.
    ///
    /// # Panics
    /// When the TLS backend cannot initialise, like `reqwest::Client::new`.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn new() -> Self {
        ClientBuilder::new().build().expect("TLS backend initialises")
    }

    /// A builder.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// A handle sharing this client's pools that retries transient failures (connect errors, timeouts, 408, 429 and 5xx other than 501)
    /// up to `max_retries` times with exponential backoff from 500 ms to 30 s. Requests whose body cannot be cloned are not retried.
    #[must_use]
    pub fn with_retries(&self, max_retries: u32) -> Client {
        Client { inner: Arc::clone(&self.inner), retries: max_retries }
    }

    /// The h1/h2 transport, for APIs that need a raw `reqwest::Client` (WebSocket upgrades, `reqwest-middleware`).
    #[must_use]
    pub fn reqwest(&self) -> &reqwest::Client {
        &self.inner.base
    }

    /// The Alt-Svc cache, to seed or inspect origins.
    #[must_use]
    pub fn alt_svc(&self) -> &AltSvcCache {
        &self.inner.alt
    }

    /// A request builder.
    pub fn request<U: IntoUrl>(&self, method: Method, url: U) -> RequestBuilder {
        RequestBuilder { client: self.clone(), inner: self.inner.base.request(method, url) }
    }

    /// `GET`.
    pub fn get<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::GET, url)
    }
    /// `POST`.
    pub fn post<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::POST, url)
    }
    /// `PUT`.
    pub fn put<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::PUT, url)
    }
    /// `PATCH`.
    pub fn patch<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::PATCH, url)
    }
    /// `DELETE`.
    pub fn delete<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }
    /// `HEAD`.
    pub fn head<U: IntoUrl>(&self, url: U) -> RequestBuilder {
        self.request(Method::HEAD, url)
    }

    /// Sends a request: over h3 when the origin advertised it, otherwise over TCP (h2 or h1 by ALPN).
    ///
    /// # Errors
    /// Any transport, TLS, timeout or redirect error of the chosen path.
    pub async fn execute(&self, request: Request) -> Result<Response> {
        #[cfg(not(target_family = "wasm"))]
        if self.retries > 0 {
            return self.execute_with_retries(request).await;
        }
        self.execute_once(request).await
    }

    #[cfg(not(target_family = "wasm"))]
    async fn execute_with_retries(&self, mut request: Request) -> Result<Response> {
        let mut attempt = 0u32;
        loop {
            let spare = request.try_clone();
            let result = self.execute_once(request).await;
            let transient = match &result {
                Ok(response) => {
                    let status = response.status();
                    status == StatusCode::REQUEST_TIMEOUT
                        || status == StatusCode::TOO_MANY_REQUESTS
                        || (status.is_server_error() && status != StatusCode::NOT_IMPLEMENTED)
                }
                Err(error) => error.is_connect() || error.is_timeout() || error.is_request(),
            };
            let Some(next) = spare.filter(|_| transient && attempt < self.retries) else {
                return result;
            };
            let backoff = Duration::from_millis(500u64.saturating_mul(1u64 << attempt.min(16))).min(Duration::from_secs(30));
            tokio::time::sleep(backoff).await;
            attempt += 1;
            request = next;
        }
    }

    async fn execute_once(&self, request: Request) -> Result<Response> {
        let origin = Origin::of_https(request.url());
        #[cfg(not(target_family = "wasm"))]
        if let (Some(origin), Some(h3)) = (&origin, &self.inner.h3)
            && self.inner.alt.h3_usable(origin)
            && let Some(transport) = h3.get(is_safe(request.method()))
        {
            let retry = request.try_clone();
            let mut over_quic = request;
            *over_quic.version_mut() = reqwest::Version::HTTP_3;
            match transport.execute(over_quic).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    // Nothing was received over QUIC: a connect failure, a timeout, or a stream reset by the peer (a server that stops
                    // reading with a non-H3_NO_ERROR code resets the client's request stream). The origin goes back to TCP for the
                    // freshness of its Alt-Svc, and the request is replayed once over TCP when it is safe to do so. A reset that
                    // arrives after the response head is a body error the caller sees: the response cannot be replayed.
                    let reset = is_stream_reset(&error) || error.is_request();
                    let connect = error.is_connect();
                    if connect || reset || error.is_timeout() {
                        self.inner.alt.mark_broken(origin);
                    }
                    return match retry {
                        Some(retry) if connect || ((reset || error.is_timeout()) && is_idempotent(retry.method())) => {
                            self.over_tcp(retry, Some(origin)).await
                        }
                        _ => Err(error),
                    };
                }
            }
        }
        self.over_tcp(request, origin.as_ref()).await
    }

    async fn over_tcp(&self, request: Request, origin: Option<&Origin>) -> Result<Response> {
        let response = self.inner.base.execute(request).await?;
        if let Some(origin) = origin {
            self.inner.alt.observe(origin, response.headers());
        }
        Ok(response)
    }
}

/// A request under construction; same surface as `reqwest::RequestBuilder` for the methods the workspace uses.
#[derive(Debug)]
#[must_use = "a RequestBuilder does nothing until it is sent"]
pub struct RequestBuilder {
    client: Client,
    inner: reqwest::RequestBuilder,
}

impl RequestBuilder {
    fn map(mut self, f: impl FnOnce(reqwest::RequestBuilder) -> reqwest::RequestBuilder) -> Self {
        self.inner = f(self.inner);
        self
    }

    /// Adds a header.
    pub fn header<K, V>(self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.map(|b| b.header(key, value))
    }
    /// Merges headers.
    pub fn headers(self, headers: HeaderMap) -> Self {
        self.map(|b| b.headers(headers))
    }
    /// Bearer token.
    pub fn bearer_auth<T: std::fmt::Display>(self, token: T) -> Self {
        self.map(|b| b.bearer_auth(token))
    }
    /// Basic authentication.
    pub fn basic_auth<U: std::fmt::Display, P: std::fmt::Display>(self, username: U, password: Option<P>) -> Self {
        self.map(|b| b.basic_auth(username, password))
    }
    /// JSON body.
    pub fn json<T: serde::Serialize + ?Sized>(self, json: &T) -> Self {
        self.map(|b| b.json(json))
    }
    /// Form body.
    pub fn form<T: serde::Serialize + ?Sized>(self, form: &T) -> Self {
        self.map(|b| b.form(form))
    }
    /// Query string.
    pub fn query<T: serde::Serialize + ?Sized>(self, query: &T) -> Self {
        self.map(|b| b.query(query))
    }
    /// Raw body.
    pub fn body<T: Into<reqwest::Body>>(self, body: T) -> Self {
        self.map(|b| b.body(body))
    }
    /// Per-request timeout.
    pub fn timeout(self, timeout: Duration) -> Self {
        self.map(|b| b.timeout(timeout))
    }
    /// Multipart body.
    pub fn multipart(self, form: reqwest::multipart::Form) -> Self {
        self.map(|b| b.multipart(form))
    }
    /// Pins the HTTP version of this request.
    #[cfg(not(target_family = "wasm"))]
    pub fn version(self, version: reqwest::Version) -> Self {
        self.map(|b| b.version(version))
    }

    /// A copy of the builder, when the body can be cloned.
    #[must_use]
    pub fn try_clone(&self) -> Option<Self> {
        Some(Self { client: self.client.clone(), inner: self.inner.try_clone()? })
    }

    /// The finished request.
    ///
    /// # Errors
    /// When a previous builder step failed.
    pub fn build(self) -> Result<Request> {
        self.inner.build()
    }

    /// Sends the request through [`Client::execute`].
    ///
    /// # Errors
    /// Builder errors and any transport error.
    pub async fn send(self) -> Result<Response> {
        let Self { client, inner } = self;
        let request = inner.build()?;
        client.execute(request).await
    }
}

impl From<reqwest::Client> for Client {
    /// Wraps an existing reqwest client as the TCP transport (no h3 upgrade).
    fn from(base: reqwest::Client) -> Self {
        Self {
            retries: 0,
            inner: Arc::new(Inner {
                base,
                #[cfg(not(target_family = "wasm"))]
                h3: None,
                alt: AltSvcCache::default(),
            }),
        }
    }
}

/// Methods that can be replayed without changing the outcome (RFC 9110 section 9.2.2).
#[cfg(not(target_family = "wasm"))]
fn is_idempotent(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE | Method::TRACE)
}

/// Whether the error chain holds an h3 stream reset or stop-sending (`RemoteTerminate`), however reqwest wrapped it. Request-phase
/// errors (`is_request`) cover the other ways a misbehaving peer ends an exchange before the response head, such as an h3 connection
/// error raised by the reset of a request stream.
#[cfg(not(target_family = "wasm"))]
fn is_stream_reset(error: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(current) = source {
        if format!("{current:?}").contains("RemoteTerminate") {
            return true;
        }
        source = current.source();
    }
    false
}

/// Methods that may travel in TLS early data: safe and idempotent by definition (RFC 9110 section 9.2.1).
#[cfg(not(target_family = "wasm"))]
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}
