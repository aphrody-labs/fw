#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

extern crate self as aphrody_auto;

#[cfg(feature = "runtime")]
pub mod balancer;
#[cfg(feature = "runtime")]
pub mod config;
#[cfg(feature = "runtime")]
pub mod mcp;
#[cfg(feature = "runtime")]
pub mod server_rt;
#[cfg(feature = "runtime")]
pub mod updater;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(feature = "runtime")]
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("network/proxy error: {0}")]
    Network(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("auth error: {0}")]
    Auth(#[from] crate::auth::AuthError),
    #[error("client error: {0}")]
    Client(#[from] crate::client::ClientError),
}

#[cfg(feature = "runtime")]
pub type SdkResult<T> = Result<T, SdkError>;

#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "docs")]
pub mod docs;
#[cfg(feature = "facturx")]
pub mod facturx;
#[cfg(feature = "files")]
pub mod files;
#[cfg(feature = "i18n")]
pub mod i18n;
#[cfg(feature = "knowledge")]
pub mod knowledge;
#[cfg(feature = "model")]
pub mod model;
#[cfg(feature = "sheets")]
pub mod sheets;
#[cfg(feature = "theme")]
pub mod theme;
