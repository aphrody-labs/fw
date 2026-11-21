//! One error type for the crate.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{0}")]
    Message(String),
    #[error("{command}: {detail}")]
    Command { command: String, detail: String },
    #[error("http {url}: {detail}")]
    Http { url: String, detail: String },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{path}: {detail}")]
    Toml { path: PathBuf, detail: String },
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn message(text: impl Into<String>) -> Error {
        Error::Message(text.into())
    }
}

/// Reads a file, naming the path in the error.
pub fn read_to_string(path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Io { path: path.to_owned(), source })
}
