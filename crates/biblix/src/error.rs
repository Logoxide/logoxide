use thiserror::Error;

pub type Result<T, E = BiblixError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum BiblixError {
    /// A file a package needs isn't there
    #[error("{0}: not found")]
    NotFound(String),
    /// Reading or writing failed
    #[error("{path}: {message}")]
    Io { path: String, message: String },
    /// A path outside the package, like `../x` or `/etc`
    #[error("{0}: not a path inside the package")]
    BadPath(String),
    /// This storage can't be written to
    #[error("this storage is read-only")]
    ReadOnly,
    /// `biblix.toml` can't be read or doesn't make sense
    #[error("biblix.toml: {0}")]
    Manifest(String),
    /// None of the formats the enabled features turn on can be read
    #[error("no format biblix reads is enabled (enabled: {0})")]
    NoReadableFormat(String),
    /// A book's text can't be read
    #[error("{book}: {message}")]
    Book { book: String, message: String },
    /// A cached file is damaged or from another version
    #[error("cache: {0}")]
    Cache(String),
}

impl BiblixError {
    pub(crate) fn io(path: &str, error: impl std::fmt::Display) -> Self {
        BiblixError::Io {
            path: path.to_string(),
            message: error.to_string(),
        }
    }
}
