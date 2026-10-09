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
    /// `biblix.lock` can't be read
    #[error("biblix.lock: {0}")]
    Lock(String),
    /// A catalog can't be read
    #[error("catalog: {0}")]
    Catalog(String),
    /// A download can't be made into a package
    #[error("import: {0}")]
    Import(String),
    /// A package's media list or timings are wrong
    #[error("media: {0}")]
    Media(String),
    /// Fetching a package failed
    #[error("{url}: {message}")]
    Fetch { url: String, message: String },
    /// No version satisfies a requirement, or two requirements conflict
    #[error("{0}")]
    Resolve(String),
    /// What was installed isn't what the lock file says
    #[error("{id}: expected {expected}, got {actual} (the source changed since it was locked)")]
    Checksum {
        id: String,
        expected: String,
        actual: String,
    },
    /// An installed version's files changed (edited in the library); repairing puts them back
    #[error(
        "{id}: its files in {dir} changed since it was locked (expected {expected}, got {actual}): `bx install --repair` puts them back"
    )]
    Changed {
        id: String,
        dir: String,
        expected: String,
        actual: String,
    },
}

impl BiblixError {
    pub(crate) fn io(path: &str, error: impl std::fmt::Display) -> Self {
        BiblixError::Io {
            path: path.to_string(),
            message: error.to_string(),
        }
    }
}
