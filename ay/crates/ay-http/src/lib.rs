//! One HTTP stack for the SDK.
//!
//! * [`Client`] (feature `client`): an async client over `reqwest` with rustls, HTTP/1.1, h2 and h3. A response that carries
//!   `Alt-Svc: h3=":443"` makes the client send later requests to that origin over QUIC (connection reuse comes from reqwest's pool);
//!   a failed QUIC attempt falls back to TCP and the origin is not retried over h3 for a while.
//! * [`server`] (feature `server`): `axum` on `hyper` (h1, h2, optionally TLS) plus `h3` on `quinn`, serving the same
//!   [`axum::Router`] and advertising `Alt-Svc`.
//! * [`error`] (feature `server`): the JSON error envelope of first-party HTTP surfaces.
//!
//! Every other crate of the workspace goes through this crate; none depends on `reqwest`, `hyper` or `axum-server` directly for
//! plain HTTP work.

#[cfg(feature = "client")]
pub mod altsvc;
#[cfg(all(feature = "blocking", not(target_family = "wasm")))]
pub mod blocking;
#[cfg(feature = "client")]
mod client;
#[cfg(feature = "server")]
pub mod error;
#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
pub use client::{Client, ClientBuilder, RequestBuilder};

/// `GET` with a fresh default client (the shape of `reqwest::get`).
///
/// # Errors
/// Any transport error.
#[cfg(feature = "client")]
pub async fn get<U: IntoUrl>(url: U) -> Result<Response> {
    Client::new().get(url).send().await
}

/// The `reqwest` types of the client surface, so dependents never name `reqwest` themselves.
#[cfg(feature = "client")]
pub use reqwest;
#[cfg(feature = "client")]
pub use reqwest::{Body, Error, IntoUrl, Method, Request, Response, Result, StatusCode, Url, Version, header, multipart};
/// Native-only parts of the surface (the browser backend has no TLS configuration, proxies or redirect policies).
#[cfg(all(feature = "client", not(target_family = "wasm")))]
pub use reqwest::{Certificate, Proxy, redirect};
