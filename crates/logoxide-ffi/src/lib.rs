/*!
logoxide's API for every language binding, with topos's (`Topos`, `ToposTranslations`, ...) in the
same package, so an app needs only this one. A book's text is logoxide's own [`LogoxideBook`].

The host reads and writes files, as an app's file API is often asynchronous (Obsidian's vault)
and WebAssembly has none: a package is opened from its files' bytes ([`Logoxide::open_package`]),
and the cache files it writes are handed back ([`LogoxidePackage::cache_writes`]). Natively,
[`Logoxide::open_package_dir`] reads a folder itself, and [`Logoxide::read_passage`] and
[`Logoxide::compare_passage`] read a project's Bibles as `lx read` and `lx compare` do. [`LogoxideProject`] says which packages a vault's
`logoxide.toml` and `logoxide.lock` ask for, and where each one's files are.
*/

mod bible;
mod book;
mod catalog;
mod create;
mod installer;
mod package;
mod project;
mod read;
#[cfg(test)]
mod tests;
mod versification;

pub use bible::*;
pub use book::*;
pub use catalog::*;
pub use create::*;
pub use installer::*;
pub use package::*;
pub use project::*;
pub use read::*;
pub use topos_bible::*;
pub use versification::*;

use boltffi::*;

/// The formats a book converts between (`Logoxide.convertBook`)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BookFormat {
    Usfm,
    Usx,
    /// USX as JSON
    Usj,
}

impl From<BookFormat> for logoxide_text::source::TextFormat {
    fn from(format: BookFormat) -> Self {
        match format {
            BookFormat::Usfm => Self::Usfm,
            BookFormat::Usx => Self::Usx,
            BookFormat::Usj => Self::Usj,
        }
    }
}

#[error]
#[derive(Clone, Debug, PartialEq, Eq)]
/// What went wrong, by the core's kinds (`logoxide::error::ErrorKind`): hosts branch on the
/// tag and show the message
pub enum LogoxideError {
    /// A file, a package, or a version that isn't there
    NotFound { message: String },
    /// Reading or writing files failed, or there's no file access (in WebAssembly)
    Io { message: String },
    /// A file that doesn't make sense: a manifest, a lock, a catalog, a cache, a map, a timings
    /// file, a book's text, or a download that can't be made into a package
    Invalid { message: String },
    /// Markup in a format logoxide-markup doesn't read (see [`LogoxideBook::parse`])
    InvalidMarkup { message: String },
    /// A download couldn't be made or didn't answer
    Network { message: String },
    /// The `git` command failed, or a repository's refs don't name what was asked for
    Git { message: String },
    /// An archive can't be unpacked, or is larger than allowed
    Archive { message: String },
    /// What was fetched isn't what was listed or locked (its hash or checksum differs), or a
    /// release changed under its version
    Integrity { message: String },
    /// No versions satisfy what's asked, or a package names what it may not
    Resolve { message: String },
    /// Another process holds the library or the project: try again
    Busy { message: String },
    /// A file changed since it was read, so writing it would lose that change
    Conflict { message: String },
}

/// logoxide-text's errors, as the core maps them
impl From<logoxide_text::TextError> for LogoxideError {
    fn from(error: logoxide_text::TextError) -> Self {
        logoxide_core::LogoxideError::from(error).into()
    }
}

impl From<logoxide_core::LogoxideError> for LogoxideError {
    fn from(error: logoxide_core::LogoxideError) -> Self {
        use logoxide_core::error::ErrorKind as K;
        let message = error.to_string();
        match error.kind() {
            K::NotFound => LogoxideError::NotFound { message },
            K::Io => LogoxideError::Io { message },
            K::Network => LogoxideError::Network { message },
            K::Git => LogoxideError::Git { message },
            K::Archive => LogoxideError::Archive { message },
            K::Integrity => LogoxideError::Integrity { message },
            K::Resolve => LogoxideError::Resolve { message },
            K::Busy => LogoxideError::Busy { message },
            K::Conflict => LogoxideError::Conflict { message },
            // `Invalid`, and any kind a newer core adds
            _ => LogoxideError::Invalid { message },
        }
    }
}

/// A book an import downloads a file of its own for (`Logoxide.bookDownloads`): its file's
/// `name` (fetch.bible's lowercase code, `jhn`) and where it is
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookDownload {
    pub book_id: u8,
    pub name: String,
    /// Tried in order: the first that answers has it
    pub urls: Vec<String>,
}

/// A file an import needs (`CatalogSource::External`'s `downloads`, `Logoxide.importDownloads`):
/// `name` is what to give its bytes as
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDownload {
    pub name: String,
    /// Tried in order: the first that answers has it
    pub urls: Vec<String>,
}

impl From<&logoxide_core::catalog::Download> for ImportDownload {
    fn from(d: &logoxide_core::catalog::Download) -> Self {
        Self {
            name: d.name.clone(),
            urls: d.urls.clone(),
        }
    }
}

impl From<ImportDownload> for logoxide_core::catalog::Download {
    fn from(d: ImportDownload) -> Self {
        Self {
            name: d.name,
            urls: d.urls,
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

    /// A Bible from loose files, with no manifest: each file's format found from its name and
    /// how it starts, `.zip`s opened, whole-Bible OSIS and ThML files cut into books, each book
    /// read from its best format, and what was passed over said (`LogoxideBible.skipped`). A
    /// `NotFound` error when no file is a book of the Bible
    pub fn open_bible(&self, files: Vec<PackageFile>) -> Result<LogoxideBible, LogoxideError> {
        LogoxideBible::open(files)
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
            .ok_or_else(|| LogoxideError::Invalid {
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
        Ok(logoxide_text::source::usfm_to_usx(&usfm)?)
    }

    /// A USX book as USFM (normalized: usfm3's layout, not the publisher's), for tools that take
    /// USFM
    pub fn usx_to_usfm(&self, usx: String) -> Result<String, LogoxideError> {
        Ok(logoxide_text::source::usx_to_usfm(&usx)?)
    }

    /// A book in USFM, USX, or USJ written in another of them (or its own, normalized), through
    /// USX: `convertBook(usfm, BookFormat.Usfm, BookFormat.Usj)`
    pub fn convert_book(
        &self,
        text: String,
        from_format: BookFormat,
        to_format: BookFormat,
    ) -> Result<String, LogoxideError> {
        Ok(logoxide_text::source::convert(
            &text,
            from_format.into(),
            to_format.into(),
        )?)
    }

    /// A book's or a Bible's file's format (`usx`, `usfm`, `usj`, `osis`, `thml`, `gbf`), from its
    /// name's extension, else from how its text starts (`head`, its first few thousand
    /// characters): a `.xml`, `.json`, or `.txt` file is found by what it holds. None for
    /// anything else
    pub fn detect_format(&self, name: String, head: String) -> Option<String> {
        logoxide_text::TextFormat::detect(&name, &head).map(|f| f.name().to_string())
    }

    /// A USX book as USJ (USX as JSON, indented; end milestones left out, as USJ has none)
    pub fn usx_to_usj(&self, usx: String) -> Result<String, LogoxideError> {
        Ok(logoxide_text::source::usx_to_usj(&usx)?)
    }

    /// A USJ book as USX, for `LogoxideBook.parse(topos, "usx", ...)`
    pub fn usj_to_usx(&self, usj: String) -> Result<String, LogoxideError> {
        Ok(logoxide_text::source::usj_to_usx(&usj)?)
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

    /// The publishers packages can be imported from, as import names prefix them (`ebible`,
    /// `fetch`, `ccel`, `sword`, `door43`: `ebible:eng-asv`; `burrito` and `dbl`, a bundle by
    /// where it is: `importBundleEntry`)
    pub fn import_prefixes(&self) -> Vec<String> {
        logoxide_import::PREFIXES
            .iter()
            .map(|p| p.to_string())
            .collect()
    }

    /// The catalog (`mods.d.tar.gz`, bytes: `parse_catalog_bytes`) of a SWORD repository
    /// CrossWire lists, by its name (`crosswire`, `crosswire-beta`, `ebible`, …), so
    /// `sword:<repository>/<module>` can be found when no catalog of the project's lists it;
    /// none for a name it doesn't know
    pub fn sword_catalog_url(&self, name: String) -> Option<String> {
        logoxide_import::sword::catalog_url(&name)
    }

    /// What importing `entry` (an `External` source) downloads (each from the first of its
    /// `urls` that answers), the same for every publisher: give their bytes, by `name`, to
    /// `import_downloaded`
    pub fn import_downloads(
        &self,
        entry: CatalogEntry,
    ) -> Result<Vec<ImportDownload>, LogoxideError> {
        Ok(logoxide_import::downloads(&entry.into())?
            .iter()
            .map(ImportDownload::from)
            .collect())
    }

    /**
    The books `entry` downloads one a file (fetch.bible's USX, a book each), by book: so a
    reader fetches only the books it shows, parsing each with `LogoxideBook.parse`. Empty for an
    import downloaded whole (eBible.org's `.zip`, a SWORD module), which only `import_downloaded`
    reads
    */
    pub fn book_downloads(&self, entry: CatalogEntry) -> Result<Vec<BookDownload>, LogoxideError> {
        let books = logoxide_text::bible_data().books();
        Ok((logoxide_import::downloads(&entry.into())?.iter())
            .filter_map(|d| {
                let id = books.search_usfm(&d.name.to_ascii_uppercase())?;
                Some(BookDownload {
                    book_id: id.0,
                    name: d.name.clone(),
                    urls: d.urls.clone(),
                })
            })
            .collect())
    }

    /**
    The entry for a Scripture Burrito or DBL bundle at `location` (`https://…`, or a `file://` URL
    for one the host read where it is), from its bytes (`downloads`: its `.zip` or `.tar.gz` as
    `bundle`, or a folder's files as `files/<path>`): `burrito:<location>` or `dbl:<location>` by
    the metadata it has, named, languaged, and licensed as that says. Give it and the same
    downloads to `import_downloaded`. A bundle that isn't a Bible's text, or whose files aren't as
    its metadata lists them, is an error
    */
    pub fn import_bundle_entry(
        &self,
        location: String,
        downloads: Vec<PackageFile>,
    ) -> Result<CatalogEntry, LogoxideError> {
        let downloads: Vec<(String, Vec<u8>)> =
            downloads.into_iter().map(|d| (d.path, d.bytes)).collect();
        let entry = logoxide_import::burrito::bundle_entry(&location, &downloads)?;
        Ok(CatalogEntry::from(&entry))
    }

    /// The package `entry`'s import makes from its downloads (`import_downloads`; each file's
    /// `path` its `name`): its files, relative to the folder to write them in (`imports/<id>/`
    /// beside the vault's `logoxide.toml`, then `addDependency(id, { path: "imports/<id>" })`).
    /// Each publisher's download becomes its format's files (eBible.org's zip the USFM books,
    /// fetch.bible's books USX, CCEL's file ThML, a SWORD module's zip a file a book in the
    /// module's markup, OSIS or ThML) with `logoxide.toml` and the license file; a page in a
    /// download's place (a work CCEL restricts) is an error
    pub fn import_downloaded(
        &self,
        entry: CatalogEntry,
        downloads: Vec<PackageFile>,
    ) -> Result<Vec<PackageFile>, LogoxideError> {
        let downloads: Vec<(String, Vec<u8>)> =
            downloads.into_iter().map(|d| (d.path, d.bytes)).collect();
        let files = logoxide_core::MemoryStorage::new();
        logoxide_import::build(&entry.into(), &downloads, &files)?;
        create::files_of(&files)
    }

    /// Each file in a `.zip` (folders left out), by its `/`-separated path, so a host can read a
    /// zip of book files without a zip library of its own. Bytes that aren't a zip are an error,
    /// and so is one past the limits: 200,000 entries, 1 GiB a file or in all
    pub fn unzip(&self, bytes: Vec<u8>) -> Result<Vec<PackageFile>, LogoxideError> {
        let files = logoxide_text::archive::unzip(&bytes)?;
        Ok(files
            .into_iter()
            .map(|(path, bytes)| PackageFile { path, bytes })
            .collect())
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
            catalog: logoxide_import::catalog::parse_text(&location, &text)?,
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
            catalog: logoxide_import::catalog::parse(&location, &bytes)?,
        })
    }

    /**
    Whether an import made from the publisher's `upstream_version` (its `Upstream.version`, never
    the package's own version) is as new as its catalog entry lists, so it needn't be downloaded
    to know: eBible.org's and SWORD's versions follow their updates; fetch.bible's and CCEL's
    don't, so those are always `false` (download, then `update_import`)
    */
    pub fn import_is_current(&self, entry: CatalogEntry, upstream_version: Option<String>) -> bool {
        logoxide_import::import_is_current(&entry.into(), upstream_version.as_deref())
    }

    /**
    How an imported package (`current`: its folder's files) differs from the same import made
    again (`fresh`: what `import_downloaded` returns for its entry, from
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
        let Some(update) = logoxide_import::compare_import(
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
            removed: logoxide_import::import_removals(current.as_ref(), fresh.as_ref())?,
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
        let timings = logoxide_text::timings::parse_timings(&text)?;
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

    /**
    A passage from one of the project's Bibles in `style` (`John 3:16 KJV`: the Bible it names,
    else the project's first), as `lx read` gives it: the project in `project_dir`, its packages
    installed in the library at `library_dir`, parsed books cached in `cache_dir`. Not in
    WebAssembly, which has no file access (open the package and read its book there)
    */
    pub fn read_passage(
        &self,
        project_dir: String,
        library_dir: String,
        cache_dir: String,
        reference: String,
        style: BookTextStyle,
    ) -> Result<ReadPassage, LogoxideError> {
        read::read_passage(project_dir, library_dir, cache_dir, reference, style)
    }

    /**
    A passage in several of the project's Bibles (`bibles` by name, in that order; all of them
    when empty), as `lx compare` gives it: numbered by the Bible `reference` names, else the
    first, and mapped through the project's versification maps into each Bible numbered
    otherwise, with what of it that Bible lacks. Folders as `readPassage`'s; not in WebAssembly
    */
    pub fn compare_passage(
        &self,
        project_dir: String,
        library_dir: String,
        cache_dir: String,
        reference: String,
        bibles: Vec<String>,
        style: BookTextStyle,
    ) -> Result<Vec<ComparedPassage>, LogoxideError> {
        read::compare_passage(
            project_dir,
            library_dir,
            cache_dir,
            reference,
            bibles,
            style,
        )
    }
}
