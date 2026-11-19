// SPDX-License-Identifier: Apache-2.0
// Copyright AGNTCY Contributors (https://github.com/agntcy)

//! Serving axum routers over HTTPS (h1, h2 and h3 with `Alt-Svc`) through the workspace HTTP stack.
//!
//! Available with the `rustls` feature. Re-exports the server of [`ay_http`] and the matching [`rustls`].
//!
//! # Example
//!
//! ```no_run
//! use ay_a2a::server::tls::{ServeConfig, Tls, serve};
//!
//! # async fn example(cert_pem: Vec<u8>, key_pem: Vec<u8>) -> std::io::Result<()> {
//! let tls = Tls::from_pem(&cert_pem, &key_pem)?;
//! let server = serve(axum::Router::new(), ServeConfig::secure("0.0.0.0:3443".parse().unwrap(), tls)).await?;
//! server.wait().await;
//! # Ok(())
//! # }
//! ```

pub use ay_http::server::{ServeConfig, Server, Tls, serve, serve_listener, serve_on};
pub use rustls;
