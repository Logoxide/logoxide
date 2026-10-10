/*!
What goes wrong, as one error type with a [`kind`](LogoxideError::kind) for callers that act on
it rather than read it.

Each variant says what failed in its message; the kinds are what a front end tells apart:

- `NotFound`: a file, a package, or a version that isn't there
- `Io`: reading or writing failed, or the storage can't be written (its `source()` is the
  operating system's error where there is one)
- `Invalid`: a file that doesn't make sense (a manifest, a lock, a catalog, a cache, a map, a
  timings file, a book's text, a download that can't be made into a package)
- `Network`: a download couldn't be made or didn't answer (a `source()` where the client gives
  one)
- `Git`: the `git` command failed, or a repository's refs don't name what was asked for
- `Archive`: a `.tar.gz` that can't be unpacked or is too large
- `Integrity`: what was fetched or installed isn't what was listed or locked
- `Resolve`: no version satisfies a requirement, or a package asks for what it may not
- `Busy`: another process holds the library or the project; try again
- `Conflict`: a file changed since it was read, so writing it would lose someone's change

The enum is `#[non_exhaustive]`: matching it needs a wildcard arm, and the kinds are the stable
way to branch. The text crate's `TextError` maps into it (`reader.rs`): a book that can't be
read, a map, or a timings file is `Invalid`, a file that isn't there `NotFound`, and a file that
can't be read `Io`.
*/

use thiserror::Error;

pub type Result<T, E = LogoxideError> = std::result::Result<T, E>;

/// A boxed error kept as a variant's `source()`
pub type Source = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LogoxideError {
    /// A file a package needs isn't there
    #[error("{0}: not found")]
    NotFound(String),
    /// Reading or writing failed
    #[error("{path}: {message}")]
    Io {
        path: String,
        message: String,
        #[source]
        source: Option<Source>,
    },
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
    /// A download couldn't be made, or nothing answered
    #[error("{url}: {message}")]
    Network {
        url: String,
        message: String,
        #[source]
        source: Option<Source>,
    },
    /// The `git` command failed, or a repository's refs don't name what was asked for
    #[error("{url}: {message}")]
    Git { url: String, message: String },
    /// An archive can't be unpacked, or is larger than allowed
    #[error("{url}: {message}")]
    Archive { url: String, message: String },
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
    /// Another process holds what this one needs (the library, the project): try again
    #[error("{what}: {message}")]
    Busy { what: String, message: String },
    /// A file changed since it was read, so writing it would lose that change
    #[error("{path}: changed since it was read (open it again and redo the change)")]
    Conflict { path: String },
}

/// What kind of failure an error is, for callers that act on it (see the module's doc)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    NotFound,
    Io,
    Invalid,
    Network,
    Git,
    Archive,
    Integrity,
    Resolve,
    Busy,
    Conflict,
}

impl LogoxideError {
    pub fn kind(&self) -> ErrorKind {
        use LogoxideError as E;
        match self {
            E::NotFound(_) => ErrorKind::NotFound,
            E::Io { .. } | E::ReadOnly => ErrorKind::Io,
            E::BadPath(_)
            | E::Manifest(_)
            | E::Newer(_)
            | E::NoReadableFormat(_)
            | E::Book { .. }
            | E::Cache(_)
            | E::Lock(_)
            | E::Catalog(_)
            | E::Import(_)
            | E::Media(_)
            | E::Versification(_) => ErrorKind::Invalid,
            E::Network { .. } => ErrorKind::Network,
            E::Git { .. } => ErrorKind::Git,
            E::Archive { .. } => ErrorKind::Archive,
            E::Mismatch { .. } | E::Checksum { .. } | E::Changed { .. } => ErrorKind::Integrity,
            E::Resolve(_) => ErrorKind::Resolve,
            E::Busy { .. } => ErrorKind::Busy,
            E::Conflict { .. } => ErrorKind::Conflict,
        }
    }

    /// An `Io` error from the operating system's, kept as its source
    pub fn io(path: &str, error: std::io::Error) -> Self {
        LogoxideError::Io {
            path: path.to_string(),
            message: error.to_string(),
            source: Some(Box::new(error)),
        }
    }

    /// An `Io` error with only a message (a poisoned lock, text that isn't UTF-8)
    pub fn io_message(path: &str, message: impl std::fmt::Display) -> Self {
        LogoxideError::Io {
            path: path.to_string(),
            message: message.to_string(),
            source: None,
        }
    }

    /// A `Network` error without a client's error behind it
    pub fn network(url: &str, message: impl Into<String>) -> Self {
        LogoxideError::Network {
            url: url.to_string(),
            message: message.into(),
            source: None,
        }
    }

    pub fn git(url: &str, message: impl Into<String>) -> Self {
        LogoxideError::Git {
            url: url.to_string(),
            message: message.into(),
        }
    }

    pub fn archive(url: &str, message: impl Into<String>) -> Self {
        LogoxideError::Archive {
            url: url.to_string(),
            message: message.into(),
        }
    }

    /// Whether fetching failed: a `Network`, `Git`, or `Archive` error
    pub fn is_fetch(&self) -> bool {
        matches!(
            self.kind(),
            ErrorKind::Network | ErrorKind::Git | ErrorKind::Archive
        )
    }

    /// The same error with `note` added to its message, when it's a fetch error; others as they
    /// are
    pub fn with_fetch_note(self, note: &str) -> Self {
        match self {
            LogoxideError::Network {
                url,
                message,
                source,
            } => LogoxideError::Network {
                url,
                message: format!("{message} {note}"),
                source,
            },
            LogoxideError::Git { url, message } => LogoxideError::Git {
                url,
                message: format!("{message} {note}"),
            },
            LogoxideError::Archive { url, message } => LogoxideError::Archive {
                url,
                message: format!("{message} {note}"),
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_sources() {
        let io = LogoxideError::io("x", std::io::Error::other("gone"));
        assert_eq!(io.kind(), ErrorKind::Io);
        assert!(std::error::Error::source(&io).is_some());
        assert_eq!(io.to_string(), "x: gone");
        let network = LogoxideError::network("https://x", "no route");
        assert_eq!(network.kind(), ErrorKind::Network);
        assert!(std::error::Error::source(&network).is_none());
        assert!(network.is_fetch());
        assert_eq!(
            network.with_fetch_note("(try later)").to_string(),
            "https://x: no route (try later)"
        );
        assert_eq!(
            LogoxideError::Manifest(String::new()).kind(),
            ErrorKind::Invalid
        );
        assert_eq!(
            LogoxideError::Conflict {
                path: String::from("logoxide.toml")
            }
            .kind(),
            ErrorKind::Conflict
        );
        assert!(!LogoxideError::ReadOnly.is_fetch());
    }
}
