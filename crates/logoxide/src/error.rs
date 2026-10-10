use thiserror::Error;

pub type Result<T, E = LogoxideError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum LogoxideError {
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
    /// `logoxide.toml` can't be read or doesn't make sense
    #[error("logoxide.toml: {0}")]
    Manifest(String),
    /// A file written by a newer logoxide, in a form this one doesn't know (a verse-hash rule)
    #[error("{0}: written by a newer lx (update lx to read it)")]
    Newer(String),
    /// None of the formats the enabled features turn on can be read
    #[error("no format logoxide reads is enabled (enabled: {0})")]
    NoReadableFormat(String),
    /// A book's text can't be read
    #[error("{book}: {message}")]
    Book { book: String, message: String },
    /// A cached file is damaged or from another version
    #[error("cache: {0}")]
    Cache(String),
    /// `logoxide.lock` can't be read
    #[error("logoxide.lock: {0}")]
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
    /// A versification map can't be read, or doesn't lead where asked
    #[error("versification: {0}")]
    Versification(String),
    /// Fetching a package failed
    #[error("{url}: {message}")]
    Fetch { url: String, message: String },
    /// No version satisfies a requirement, or two requirements conflict
    #[error("{0}")]
    Resolve(String),
    /// A download isn't what was listed for it (its size or hash differs)
    #[error("{url}: {message}")]
    Mismatch { url: String, message: String },
    /// What was installed isn't what the lock file says
    #[error("{id}: expected {expected}, got {actual} (the source changed since it was locked)")]
    Checksum {
        id: String,
        expected: String,
        actual: String,
    },
    /// An installed version's files changed (edited in the library); repairing puts them back
    #[error(
        "{id}: its files in {dir} changed since it was locked (expected {expected}, got {actual}): `lx install --repair` puts them back"
    )]
    Changed {
        id: String,
        dir: String,
        expected: String,
        actual: String,
    },
}

impl LogoxideError {
    pub(crate) fn io(path: &str, error: impl std::fmt::Display) -> Self {
        LogoxideError::Io {
            path: path.to_string(),
            message: error.to_string(),
        }
    }
}
