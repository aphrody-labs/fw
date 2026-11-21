//! Self-hosted mode of the Cloudflare services: one wrangler config in, one workerd config out, plus the units and the parity suite.

pub mod bridge;
pub mod compile;
pub mod parity;
pub mod probe;
pub mod topology;
pub mod unit;
pub mod wrangler;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("{0}")]
    Invalid(String),
    #[error("http: {0}")]
    Http(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn read_text(path: &std::path::Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| Error::Io { path: path.to_path_buf(), source })
}

pub fn write_text(path: &std::path::Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
    }
    std::fs::write(path, text).map_err(|source| Error::Io { path: path.to_path_buf(), source })
}
