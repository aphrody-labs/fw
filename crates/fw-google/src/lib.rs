//! Typed Google Workspace and YouTube client for `fw`, built on the generated `google-apis-rs` crates.
//!
//! * [`Credentials`]: explicit access token, then `gcloud auth print-access-token`, then Application
//!   Default Credentials; tokens are never logged.
//! * [`Google`]: Drive list/export, Sheets values get/update, Gmail list and draft creation (never
//!   sends), Calendar events, Docs text and YouTube channels/videos/playlists plus a video metadata
//!   update that needs a [`WriteEnabled`] proof.
//!
//! The generated crates are re-exported so callers can name the response types.

#![forbid(unsafe_code)]

pub mod auth;
mod calendar;
mod docs;
mod drive;
mod gmail;
mod sheets;
pub mod youtube;

#[cfg(test)]
mod mock;

pub use auth::{Credentials, Source};
pub use calendar::EventsQuery;
pub use docs::plain_text as docs_plain_text;
pub use drive::DriveListQuery;
pub use gmail::{DraftSpec, MessageSummary, rfc822_draft};
pub use google_apis_common::Connector;
pub use youtube::{ChannelSelector, PlaylistSelector, VideoPatch, apply_patch};
pub use {
    google_calendar3 as calendar_api, google_docs1 as docs_api, google_drive3 as drive_api,
    google_gmail1 as gmail_api, google_sheets4 as sheets_api, google_youtube3 as youtube_api,
};

use google_apis_common::Client;
use http_body_util::BodyExt;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

/// The connector used by [`Google::new`]: HTTPS (rustls, webpki roots) over TCP.
pub type DefaultConnector = hyper_rustls::HttpsConnector<HttpConnector>;

/// Everything this crate can fail on. Messages never contain tokens.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Authentication failed (no credentials, expired, rejected).
    #[error("auth: {0}")]
    Auth(String),
    /// The API answered with an error status.
    #[error("{service}: HTTP {status}: {body}")]
    Http {
        service: &'static str,
        status: u16,
        body: String,
    },
    /// The request could not be sent or the response could not be read.
    #[error("{service}: {message}")]
    Transport {
        service: &'static str,
        message: String,
    },
    /// The caller passed something the API cannot accept.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// A write was attempted without the explicit write flag.
    #[error("refusing to write: {0}; pass the explicit write flag (--apply)")]
    WriteNotEnabled(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Proof that the caller explicitly allowed a mutation. Mutating operations that are not draft-only
/// take one; build it from a CLI flag with [`WriteEnabled::from_flag`].
#[derive(Clone, Copy, Debug)]
pub struct WriteEnabled(());

impl WriteEnabled {
    /// `Ok` only when `flag` is set; `what` names the refused operation in the error.
    pub fn from_flag(flag: bool, what: &'static str) -> Result<Self> {
        if flag {
            Ok(Self(()))
        } else {
            Err(Error::WriteNotEnabled(what))
        }
    }
}

/// Installs the aws-lc-rs rustls provider as process default once (several providers can be linked
/// into the workspace, which makes rustls refuse to pick one on its own).
pub(crate) fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// A client for every supported service, generic over the connector so tests can run in memory.
#[derive(Clone)]
pub struct Google<C = DefaultConnector> {
    pub(crate) client: Client<C>,
    pub(crate) auth: Credentials,
}

impl Google<DefaultConnector> {
    /// A client over HTTPS with the given credentials.
    pub fn new(auth: Credentials) -> Self {
        install_crypto_provider();
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_webpki_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        Self::with_connector(https, auth)
    }
}

impl<C: Connector> Google<C> {
    /// A client over a caller-supplied connector (a mock in tests).
    pub fn with_connector(connector: C, auth: Credentials) -> Self {
        let client =
            hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(connector);
        Self { client, auth }
    }
}

/// Maps a generated-crate error to [`Error`], reading the body of failure responses.
pub(crate) async fn map_err(service: &'static str, e: google_apis_common::Error) -> Error {
    use google_apis_common::Error as E;
    match e {
        E::MissingToken(inner) => Error::Auth(inner.to_string()),
        E::Failure(resp) => {
            let status = resp.status().as_u16();
            let bytes = resp
                .into_body()
                .collect()
                .await
                .map(|c| c.to_bytes())
                .unwrap_or_default();
            let mut body = String::from_utf8_lossy(&bytes).trim().to_string();
            body.truncate(body.char_indices().nth(600).map_or(body.len(), |(i, _)| i));
            Error::Http {
                service,
                status,
                body,
            }
        }
        // the generated clients turn any JSON error body into `BadRequest`; the real status is in `error.code`
        E::BadRequest(v) => {
            let status = v
                .pointer("/error/code")
                .and_then(serde_json::Value::as_u64)
                .and_then(|c| u16::try_from(c).ok())
                .unwrap_or(400);
            Error::Http {
                service,
                status,
                body: v.to_string(),
            }
        }
        E::FieldClash(f) => Error::Invalid(format!("field clash: {f}")),
        other => Error::Transport {
            service,
            message: other.to_string(),
        },
    }
}

/// Awaits a generated call and maps its error.
pub(crate) async fn call<T>(service: &'static str, r: google_apis_common::Result<T>) -> Result<T> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => Err(map_err(service, e).await),
    }
}

/// Reads a whole response body.
pub(crate) async fn read_body(
    service: &'static str,
    resp: google_apis_common::Response,
) -> Result<Vec<u8>> {
    resp.into_body()
        .collect()
        .await
        .map(|c| c.to_bytes().to_vec())
        .map_err(|e| Error::Transport {
            service,
            message: e.to_string(),
        })
}

#[cfg(test)]
mod tests;
