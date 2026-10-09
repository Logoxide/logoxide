/*!
biblix's API for every language binding, with topos's (`Topos`, `ToposBook`, `ToposTranslations`,
...) in the same package, so an app needs only this one.

The host reads and writes files, as an app's file API is often asynchronous (Obsidian's vault)
and WebAssembly has none: a package is opened from its files' bytes ([`Biblix::open_package`]),
and the cache files it writes are handed back ([`BiblixPackage::cache_writes`]). Natively,
[`Biblix::open_package_dir`] reads a folder itself. [`BiblixProject`] says which packages a vault's
`biblix.toml` and `biblix.lock` ask for, and where each one's files are.
*/

mod catalog;
mod installer;
mod package;
mod project;
#[cfg(test)]
mod tests;

pub use catalog::*;
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
    /// A catalog can't be read
    Catalog { message: String },
    /// A download can't be made into a package
    Import { message: String },
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
            E::Catalog(_) => BiblixError::Catalog { message },
            E::Import(_) => BiblixError::Import { message },
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

    /// A USFM book as USX, for `ToposBook.parse(topos, "usx", ...)` (a reader of book files)
    pub fn usfm_to_usx(&self, usfm: String) -> Result<String, BiblixError> {
        Ok(biblix_core::source::usfm_to_usx(&usfm)?)
    }

    /// eBible.org's list of the translations it publishes (a catalog: `parse_catalog`; a
    /// project's `catalogs()` has it and fetch.bible's unless it names others)
    pub fn ebible_catalog_url(&self) -> String {
        biblix_core::catalog::EBIBLE_CATALOG.to_string()
    }

    /// A catalog from its file's text: eBible.org's CSV when `location` ends in `.csv`, else a
    /// `catalog.toml`
    pub fn parse_catalog(
        &self,
        location: String,
        text: String,
    ) -> Result<BiblixCatalog, BiblixError> {
        Ok(BiblixCatalog {
            catalog: biblix_core::catalog::Catalog::parse(&location, &text)?,
        })
    }

    /// A package made from eBible.org's download of a catalog entry (`zipUrl`'s bytes): its
    /// files, relative to the folder to write them in (`imports/<id>/` beside the vault's
    /// `biblix.toml`, then `addDependency(id, { path: "imports/<id>" })`)
    pub fn import_ebible(
        &self,
        entry: CatalogEntry,
        zip: Vec<u8>,
    ) -> Result<Vec<PackageFile>, BiblixError> {
        let files = biblix_core::MemoryStorage::new();
        biblix_core::import::ebible(&entry.into(), &zip, &files)?;
        let mut out = vec![];
        for path in biblix_core::Storage::walk(&files, "")? {
            out.push(PackageFile {
                bytes: biblix_core::Storage::read(&files, &path)?,
                path,
            });
        }
        Ok(out)
    }

    /// A package made from fetch.bible's books (each `FetchBook`'s bytes, its `code` as the
    /// path): its files, relative to the folder to write them in, like `import_ebible`
    pub fn import_fetch_bible(
        &self,
        entry: CatalogEntry,
        books: Vec<PackageFile>,
    ) -> Result<Vec<PackageFile>, BiblixError> {
        let books: Vec<(String, Vec<u8>)> = books.into_iter().map(|b| (b.path, b.bytes)).collect();
        let files = biblix_core::MemoryStorage::new();
        biblix_core::import::fetch_bible(&entry.into(), &books, &files)?;
        let mut out = vec![];
        for path in biblix_core::Storage::walk(&files, "")? {
            out.push(PackageFile {
                bytes: biblix_core::Storage::read(&files, &path)?,
                path,
            });
        }
        Ok(out)
    }

    /// Whether downloaded bytes are a listed media file (its size and SHA-256); an error says
    /// what differs
    pub fn verify_media(&self, file: MediaFile, bytes: Vec<u8>) -> Result<(), BiblixError> {
        Ok(biblix_core::media::verify(&file.into(), &bytes)?)
    }

    /// Where to keep a media file once checked, by its hash (`media/9f/9f86…`, relative to a
    /// cache), so versions that list the same file share it
    pub fn media_store_path(&self, file: MediaFile) -> String {
        biblix_core::media::store_path(&file.into())
    }

    /// Installs packages and inspects repositories, with the host downloading
    pub fn installer(&self) -> BiblixInstaller {
        BiblixInstaller::new()
    }
}
