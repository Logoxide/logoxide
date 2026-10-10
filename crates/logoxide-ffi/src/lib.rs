/*!
logoxide's API for every language binding, with topos's (`Topos`, `ToposTranslations`, ...) in the
same package, so an app needs only this one. A book's text is logoxide's own [`LogoxideBook`].

The host reads and writes files, as an app's file API is often asynchronous (Obsidian's vault)
and WebAssembly has none: a package is opened from its files' bytes ([`Logoxide::open_package`]),
and the cache files it writes are handed back ([`LogoxidePackage::cache_writes`]). Natively,
[`Logoxide::open_package_dir`] reads a folder itself. [`LogoxideProject`] says which packages a vault's
`logoxide.toml` and `logoxide.lock` ask for, and where each one's files are.
*/

mod book;
mod catalog;
mod create;
mod installer;
mod package;
mod project;
#[cfg(test)]
mod tests;
mod versification;

pub use book::*;
pub use catalog::*;
pub use create::*;
pub use installer::*;
pub use package::*;
pub use project::*;
pub use topos_bible::*;
pub use versification::*;

use boltffi::*;

#[error]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogoxideError {
    /// A file a package needs isn't there
    NotFound { message: String },
    /// `logoxide.toml` can't be read or doesn't make sense
    Manifest { message: String },
    /// `logoxide.lock` can't be read
    Lock { message: String },
    /// None of the package's enabled formats can be read, or a book's text can't be
    Text { message: String },
    /// Markup in a format logoxide-markup doesn't read (see [`LogoxideBook::parse`])
    InvalidMarkup { message: String },
    /// Reading or writing files failed, or there's no file access (in WebAssembly)
    Files { message: String },
    /// A cached file is damaged or from another version
    Cache { message: String },
    /// A catalog can't be read
    Catalog { message: String },
    /// A download can't be made into a package
    Import { message: String },
    /// A package's media list or timings are wrong
    Media { message: String },
    /// A versification map can't be read, or none leads where asked
    Versification { message: String },
    /// A download or a git repository couldn't be reached
    Network { message: String },
    /// What was fetched isn't what was listed or locked (its hash or checksum differs), or a
    /// release changed under its version
    Integrity { message: String },
    /// No versions satisfy what's asked, or a package names what it may not
    Resolve { message: String },
}

impl From<logoxide_core::LogoxideError> for LogoxideError {
    fn from(error: logoxide_core::LogoxideError) -> Self {
        use logoxide_core::LogoxideError as E;
        let message = error.to_string();
        match error {
            E::NotFound(_) => LogoxideError::NotFound { message },
            E::Manifest(_) | E::Newer(_) => LogoxideError::Manifest { message },
            E::Lock(_) => LogoxideError::Lock { message },
            E::NoReadableFormat(_) | E::Book { .. } => LogoxideError::Text { message },
            E::Cache(_) => LogoxideError::Cache { message },
            E::Catalog(_) => LogoxideError::Catalog { message },
            E::Import(_) => LogoxideError::Import { message },
            E::Media(_) => LogoxideError::Media { message },
            E::Versification(_) => LogoxideError::Versification { message },
            E::Fetch { .. } => LogoxideError::Network { message },
            E::Mismatch { .. } | E::Checksum { .. } | E::Changed { .. } => {
                LogoxideError::Integrity { message }
            }
            E::Resolve(_) => LogoxideError::Resolve { message },
            E::Io { .. } | E::BadPath(_) | E::ReadOnly => LogoxideError::Files { message },
        }
    }
}

/// A file an import needs (`Logoxide.importDownloads`): `name` is what to give its bytes as
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDownload {
    pub name: String,
    /// Tried in order: the first that answers has it
    pub urls: Vec<String>,
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
const logoxide = Logoxide.new();
const project = logoxide.parseProject(manifestText, lockText);
const pkg = logoxide.openPackage(files, entry.features, cacheFiles);
```
*/
#[derive(Default)]
pub struct Logoxide;

#[export]
impl Logoxide {
    pub fn new() -> Self {
        Self
    }

    /// logoxide's version
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// A package from its files (paths relative to its folder, every file but git's), and the
    /// cache files saved from `LogoxidePackage::cache_writes` before (paths relative to the
    /// cache's folder)
    pub fn open_package(
        &self,
        files: Vec<PackageFile>,
        features: PackageFeatures,
        cache: Vec<PackageFile>,
    ) -> Result<LogoxidePackage, LogoxideError> {
        LogoxidePackage::open(files, features, cache)
    }

    /// A package in a folder, with its cache in `cache_dir` (written there as books are read).
    /// Not in WebAssembly, which has no file access
    pub fn open_package_dir(
        &self,
        dir: String,
        features: PackageFeatures,
        cache_dir: String,
    ) -> Result<LogoxidePackage, LogoxideError> {
        LogoxidePackage::open_dir(dir, features, cache_dir)
    }

    /// What a package is, from its `logoxide.toml`'s text alone (without reading its text): to name
    /// one that's turned off. An error for a manifest without `[package]` (a vault's)
    pub fn read_package_info(&self, manifest: String) -> Result<PackageInfo, LogoxideError> {
        let parsed = logoxide_core::manifest::Manifest::parse(&manifest)?;
        let info = parsed
            .package
            .as_ref()
            .ok_or_else(|| LogoxideError::Manifest {
                message: String::from("no [package]: a vault's logoxide.toml, not a package's"),
            })?;
        Ok(PackageInfo::from(info))
    }

    /// A project from its `logoxide.toml`'s text (a new, empty one without it) and its
    /// `logoxide.lock`'s, if it has one
    pub fn parse_project(
        &self,
        manifest: Option<String>,
        lock: Option<String>,
    ) -> Result<LogoxideProject, LogoxideError> {
        LogoxideProject::parse(manifest, lock)
    }

    /// The categories a package can be in (`[package] categories`, at most
    /// `max_categories()`), in order, each with what it's for
    pub fn categories(&self) -> Vec<Category> {
        (logoxide_core::manifest::CATEGORIES.iter())
            .map(|(name, description)| Category {
                name: name.to_string(),
                description: description.to_string(),
            })
            .collect()
    }

    /// How many categories a package can be in
    pub fn max_categories(&self) -> u32 {
        logoxide_core::manifest::MAX_CATEGORIES as u32
    }

    /// A USFM book as USX, for `LogoxideBook.parse(topos, "usx", ...)` (a reader of book files)
    pub fn usfm_to_usx(&self, usfm: String) -> Result<String, LogoxideError> {
        Ok(logoxide_core::source::usfm_to_usx(&usfm)?)
    }

    /// eBible.org's list of the translations it publishes (a catalog: `parse_catalog`; one of
    /// `default_catalog_urls`)
    pub fn ebible_catalog_url(&self) -> String {
        logoxide_core::catalog::EBIBLE_CATALOG.to_string()
    }

    /// The catalogs a project searches unless it says otherwise (`default-catalogs = false`):
    /// eBible.org, fetch.bible, CCEL, and CrossWire's SWORD repository, in order
    pub fn default_catalog_urls(&self) -> Vec<String> {
        (logoxide_core::catalog::DEFAULT_CATALOGS.iter())
            .map(|c| c.to_string())
            .collect()
    }

    /// Whether `url` is fetched without encryption (`http://`, `git://`, `ftp://`), so what it
    /// sends can't be trusted: worth a warning
    pub fn is_insecure_url(&self, url: String) -> bool {
        logoxide_core::git::is_insecure(&url)
    }

    /// What importing `entry` downloads (each from the first of its `urls` that answers), the
    /// same for every source: give their bytes, by `name`, to `import_downloaded`
    pub fn import_downloads(
        &self,
        entry: CatalogEntry,
    ) -> Result<Vec<ImportDownload>, LogoxideError> {
        Ok(logoxide_core::import::downloads(&entry.into())?
            .into_iter()
            .map(|d| ImportDownload {
                name: d.name,
                urls: d.urls,
            })
            .collect())
    }

    /// The package `entry`'s import makes from its downloads (`import_downloads`; each file's
    /// `path` its `name`): its files, relative to the folder to write them in, like
    /// `import_ebible`
    pub fn import_downloaded(
        &self,
        entry: CatalogEntry,
        downloads: Vec<PackageFile>,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        let downloads: Vec<(String, Vec<u8>)> =
            downloads.into_iter().map(|d| (d.path, d.bytes)).collect();
        let files = logoxide_core::MemoryStorage::new();
        logoxide_core::import::build(&entry.into(), &downloads, &files)?;
        create::files_of(&files)
    }

    /// No versification maps yet: add map packages with `withMap` (or `.vrs` text with
    /// `withVrs`), then `map` references between the versifications they connect
    pub fn versifications(&self) -> LogoxideVersifications {
        LogoxideVersifications::default()
    }

    /// A catalog from its file's text: eBible.org's CSV when `location` ends in `.csv`,
    /// fetch.bible's manifest when it ends in `.json`, CCEL's page when it's on ccel.org, else a
    /// `catalog.toml`. A SWORD repository's (`.tar.gz`) is bytes: `parse_catalog_bytes`
    pub fn parse_catalog(
        &self,
        location: String,
        text: String,
    ) -> Result<LogoxideCatalog, LogoxideError> {
        Ok(LogoxideCatalog {
            catalog: logoxide_core::catalog::Catalog::parse(&location, &text)?,
        })
    }

    /// A catalog from its file's bytes: a SWORD repository's `mods.d.tar.gz` when `location` ends
    /// in `.tar.gz`, else as `parse_catalog` reads its text (so any catalog can be read this way)
    pub fn parse_catalog_bytes(
        &self,
        location: String,
        bytes: Vec<u8>,
    ) -> Result<LogoxideCatalog, LogoxideError> {
        Ok(LogoxideCatalog {
            catalog: logoxide_core::catalog::Catalog::parse_bytes(&location, &bytes)?,
        })
    }

    /// A package made from a SWORD module's zip (the first of its entry's `zipUrls` that
    /// answers): its files, relative to the folder to write them in, like `import_ebible`. Each
    /// book is a file in the module's markup (OSIS or ThML); its conf gives the version, license,
    /// and language. A Bible or a commentary
    pub fn import_sword(
        &self,
        entry: CatalogEntry,
        zip: Vec<u8>,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        let files = logoxide_core::MemoryStorage::new();
        logoxide_core::import::sword(&entry.into(), &zip, &files)?;
        create::files_of(&files)
    }

    /// A package made from eBible.org's download of a catalog entry (`zipUrl`'s bytes): its
    /// files, relative to the folder to write them in (`imports/<id>/` beside the vault's
    /// `logoxide.toml`, then `addDependency(id, { path: "imports/<id>" })`)
    pub fn import_ebible(
        &self,
        entry: CatalogEntry,
        zip: Vec<u8>,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        let files = logoxide_core::MemoryStorage::new();
        logoxide_core::import::ebible(&entry.into(), &zip, &files)?;
        create::files_of(&files)
    }

    /// A package made from fetch.bible's books (each `FetchBook`'s bytes, its `code` as the
    /// path): its files, relative to the folder to write them in, like `import_ebible`
    pub fn import_fetch_bible(
        &self,
        entry: CatalogEntry,
        books: Vec<PackageFile>,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        let books: Vec<(String, Vec<u8>)> = books.into_iter().map(|b| (b.path, b.bytes)).collect();
        let files = logoxide_core::MemoryStorage::new();
        logoxide_core::import::fetch_bible(&entry.into(), &books, &files)?;
        create::files_of(&files)
    }

    /// A package made from a Bible CCEL publishes (the ThML at its entry's `url`): its files,
    /// relative to the folder to write them in, like `import_ebible`. The file's head names it
    /// and gives its language, rights, and version; a page in its place (a work CCEL restricts)
    /// is an error
    pub fn import_ccel(
        &self,
        entry: CatalogEntry,
        xml: Vec<u8>,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        let files = logoxide_core::MemoryStorage::new();
        logoxide_core::import::ccel(&entry.into(), &xml, &files)?;
        create::files_of(&files)
    }

    /**
    Whether an import made from the publisher's `upstream_version` (its `Upstream.version`, never
    the package's own version) is as new as its catalog entry lists, so it needn't be downloaded
    to know: eBible.org's and SWORD's versions follow their updates; fetch.bible's and CCEL's
    don't, so those are always `false` (download, then `update_import`)
    */
    pub fn import_is_current(&self, entry: CatalogEntry, upstream_version: Option<String>) -> bool {
        logoxide_core::import::import_is_current(&entry.into(), upstream_version.as_deref())
    }

    /**
    How an imported package (`current`: its folder's files) differs from the same import made
    again (`fresh`: what `import_ebible` and the others return for its entry, from
    `LogoxideCatalog.find_upstream`); none when nothing changed. Write `files` and delete
    `removed`, then install with `UpdateMode.Only` for it. The version is raised by what changed,
    whatever the publisher numbers it, so one version never names two texts
    */
    pub fn update_import(
        &self,
        current: Vec<PackageFile>,
        fresh: Vec<PackageFile>,
    ) -> Result<Option<ImportUpdate>, LogoxideError> {
        let memory = |files: Vec<PackageFile>| {
            logoxide_core::MemoryStorage::with(files.into_iter().map(|f| (f.path, f.bytes)))
        };
        let current = std::sync::Arc::new(memory(current));
        let fresh = std::sync::Arc::new(memory(fresh));
        let Some(update) = logoxide_core::import::compare_import(
            current.clone(),
            &fresh,
            &logoxide_core::reader::DefaultReader,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(ImportUpdate {
            from: update.from.to_string(),
            to: update.to.to_string(),
            change: update.diff.change().into(),
            description: logoxide_core::release::describe(&update.diff),
            upstream_from: update.upstream_from,
            upstream_to: update.upstream_to,
            removed: logoxide_core::import::import_removals(current.as_ref(), fresh.as_ref())?,
            files: create::files_of(&fresh)?,
        }))
    }

    /// What a file says about itself, to fill in a new package's form: an EPUB's title,
    /// subtitle, authors, language, publisher, and rights; nothing for other formats
    pub fn file_metadata(
        &self,
        file_name: String,
        bytes: Vec<u8>,
    ) -> Result<FileMetadata, LogoxideError> {
        create::file_metadata(&file_name, &bytes)
    }

    /// A new package made from one file (`book.epub`, `JHN.usx`): its files, relative to its
    /// folder (`logoxide.toml`, and the file in its format's folder: `epub/book.epub`). What's
    /// given wins over the file's metadata; an EPUB fills in the rest, other formats need a name
    /// and a language. Make the folder a git repository to release it
    pub fn new_package(
        &self,
        file_name: String,
        bytes: Vec<u8>,
        given: NewPackage,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        create::new_package(&file_name, &bytes, given)
    }

    /// A new collection (a package of packages, with no text of its own; it needs a name and a
    /// language): its `logoxide.toml`. Add its packages with `LogoxideProject.addDependency` on it
    pub fn new_collection(&self, given: NewPackage) -> Result<Vec<PackageFile>, LogoxideError> {
        create::new_collection(given)
    }

    /// The passages a release's books cite, from its `logoxide.references.bin` (read without the
    /// books; with them, `LogoxidePackage.references()` gives where each reference is)
    pub fn parse_references(&self, bytes: Vec<u8>) -> Result<Vec<CitedPassage>, LogoxideError> {
        let cited = logoxide_core::references::decode(&bytes)?;
        Ok(cited.into_iter().map(CitedPassage::from).collect())
    }

    /// Whether downloaded bytes are a listed media file (its size and SHA-256); an error says
    /// what differs
    pub fn verify_media(&self, file: MediaFile, bytes: Vec<u8>) -> Result<(), LogoxideError> {
        Ok(logoxide_core::media::verify(&file.into(), &bytes)?)
    }

    /// A WebVTT timings file's verses (cues named `JHN 3:16`), for timings that aren't in a
    /// package
    pub fn parse_timings(&self, text: String) -> Result<Vec<VerseTiming>, LogoxideError> {
        let timings = logoxide_core::media::parse_timings(&text)?;
        Ok(timings.into_iter().map(VerseTiming::from).collect())
    }

    /// Where to keep a media file once checked, by its hash (`media/9f/9f86…`, relative to a
    /// cache), so versions that list the same file share it
    pub fn media_store_path(&self, file: MediaFile) -> String {
        logoxide_core::media::store_path(&file.into())
    }

    /// Installs packages and inspects repositories, with the host downloading
    pub fn installer(&self) -> LogoxideInstaller {
        LogoxideInstaller::new()
    }
}
