use std::path::PathBuf;

/// Everything `fw-theme` can fail on; every variant names the file or key involved.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{}: {source}", path.display())]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unknown theme `{name}`; available: {available}")]
    UnknownTheme { name: String, available: String },
    #[error("{0}")]
    Invalid(String),
    #[error("watch: {0}")]
    Watch(#[from] notify::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}
