/*!
biblix's API for every language binding, with topos's (`Topos`, `ToposBook`, `ToposTranslations`,
...) in the same package, so an app needs only this one.

The host reads and writes files, as an app's file API is often asynchronous (Obsidian's vault)
and WebAssembly has none: a package is opened from its files' bytes ([`Biblix::open_package`]),
and the cache files it writes are handed back ([`BiblixPackage::cache_writes`]). Natively,
[`Biblix::open_package_dir`] reads a folder itself. [`BiblixProject`] says which packages a vault's
`biblix.toml` and `biblix.lock` ask for, and where each one's files are.
*/

mod installer;
mod package;
mod project;
#[cfg(test)]
mod tests;

pub use installer::*;
pub use package::*;
pub use project::*;
pub use topos_bible::*;

use boltffi::*;

#[error]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BiblixError {
    /// A file a package needs isn't there
    NotFound { message: String },
    /// `biblix.toml` can't be read or doesn't make sense
    Manifest { message: String },
    /// `biblix.lock` can't be read
    Lock { message: String },
    /// None of the package's enabled formats can be read, or a book's text can't be
    Text { message: String },
    /// Reading or writing files failed, or there's no file access (in WebAssembly)
    Files { message: String },
    /// A cached file is damaged or from another version
    Cache { message: String },
}

impl From<biblix_core::BiblixError> for BiblixError {
    fn from(error: biblix_core::BiblixError) -> Self {
        use biblix_core::BiblixError as E;
        let message = error.to_string();
        match error {
            E::NotFound(_) => BiblixError::NotFound { message },
            E::Manifest(_) => BiblixError::Manifest { message },
            E::Lock(_) => BiblixError::Lock { message },
            E::NoReadableFormat(_) | E::Book { .. } => BiblixError::Text { message },
            E::Cache(_) => BiblixError::Cache { message },
            E::Io { .. }
            | E::BadPath(_)
            | E::ReadOnly
            | E::Fetch { .. }
            | E::Resolve(_)
            | E::Checksum { .. } => BiblixError::Files { message },
        }
    }
}

/// A file and its bytes, relative to a package's folder (or to the cache's)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFile {
    /// `/`-separated, like `usx/JHN.usx`
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Which features to turn on, like Cargo's `--features` and `--no-default-features`
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFeatures {
    pub features: Vec<String>,
    /// Whether the package's `default` features are on too
    pub default_features: bool,
}

/**
Where packages and projects are opened (errors are thrown, with their message, in every language)

```ts
const biblix = Biblix.new();
const project = biblix.parseProject(manifestText, lockText);
const pkg = biblix.openPackage(files, entry.features, cacheFiles);
```
*/
#[derive(Default)]
pub struct Biblix;

#[export]
impl Biblix {
    pub fn new() -> Self {
        Self
    }

    /// biblix's version
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// A package from its files (paths relative to its folder, every file but git's), and the
    /// cache files saved from `BiblixPackage::cache_writes` before (paths relative to the
    /// cache's folder)
    pub fn open_package(
        &self,
        files: Vec<PackageFile>,
        features: PackageFeatures,
        cache: Vec<PackageFile>,
    ) -> Result<BiblixPackage, BiblixError> {
        BiblixPackage::open(files, features, cache)
    }

    /// A package in a folder, with its cache in `cache_dir` (written there as books are read).
    /// Not in WebAssembly, which has no file access
    pub fn open_package_dir(
        &self,
        dir: String,
        features: PackageFeatures,
        cache_dir: String,
    ) -> Result<BiblixPackage, BiblixError> {
        BiblixPackage::open_dir(dir, features, cache_dir)
    }

    /// A project from its `biblix.toml`'s text (a new, empty one without it) and its
    /// `biblix.lock`'s, if it has one
    pub fn parse_project(
        &self,
        manifest: Option<String>,
        lock: Option<String>,
    ) -> Result<BiblixProject, BiblixError> {
        BiblixProject::parse(manifest, lock)
    }

    /// Installs packages and inspects repositories, with the host downloading
    pub fn installer(&self) -> BiblixInstaller {
        BiblixInstaller::new()
    }
}
