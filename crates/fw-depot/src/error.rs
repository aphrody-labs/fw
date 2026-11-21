//! The error of every tool: a message that names what failed and where.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Deps(#[from] crate::deps::Error),
    #[error("{url}: {message}")]
    Http { url: String, message: String },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{command} exited with {status}: {stderr}")]
    Command {
        command: String,
        status: String,
        stderr: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn message(text: impl Into<String>) -> Error {
        Error::Message(text.into())
    }

    pub fn io(path: &Path, source: std::io::Error) -> Error {
        Error::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// `std::fs` calls that report the path.
pub mod fs {
    use super::{Error, Result};
    use std::path::Path;

    pub fn read(path: &Path) -> Result<Vec<u8>> {
        std::fs::read(path).map_err(|source| Error::io(path, source))
    }

    pub fn read_to_string(path: &Path) -> Result<String> {
        std::fs::read_to_string(path).map_err(|source| Error::io(path, source))
    }

    pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        std::fs::write(path, contents).map_err(|source| Error::io(path, source))
    }

    pub fn create_dir_all(path: &Path) -> Result<()> {
        std::fs::create_dir_all(path).map_err(|source| Error::io(path, source))
    }

    pub fn remove_dir_all(path: &Path) -> Result<()> {
        std::fs::remove_dir_all(path).map_err(|source| Error::io(path, source))
    }
}
