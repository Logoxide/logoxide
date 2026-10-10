//! A package opened from its files, with its cache: its information, verse counts, and books.

use std::sync::{Arc, Mutex};

use boltffi::*;
use logoxide_core::{
    Cache, CachedPackage, Features, MemoryStorage, Package, Storage, license,
    manifest::{self, Direction, PackageKind as CoreKind},
    source::SkipReason,
    topos_bible::data::{books::BookId, translations::Translations},
};
use topos_bible::ToposTranslations;

use crate::{LogoxideBook, LogoxideError, PackageFeatures, PackageFile};

/// What a package is (`kind` in its `logoxide.toml`)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    Bible,
    Commentary,
    Lexicon,
    CrossReferences,
    VersificationMap,
    Book,
    /// Other packages (its dependencies), with no text of its own
    Collection,
}

impl From<CoreKind> for PackageKind {
    fn from(kind: CoreKind) -> Self {
        match kind {
            CoreKind::Bible => PackageKind::Bible,
            CoreKind::Commentary => PackageKind::Commentary,
            CoreKind::Lexicon => PackageKind::Lexicon,
            CoreKind::CrossReferences => PackageKind::CrossReferences,
            CoreKind::VersificationMap => PackageKind::VersificationMap,
            CoreKind::Book => PackageKind::Book,
            CoreKind::Collection => PackageKind::Collection,
        }
    }
}

impl From<PackageKind> for CoreKind {
    fn from(kind: PackageKind) -> Self {
        match kind {
            PackageKind::Bible => CoreKind::Bible,
            PackageKind::Commentary => CoreKind::Commentary,
            PackageKind::Lexicon => CoreKind::Lexicon,
            PackageKind::CrossReferences => CoreKind::CrossReferences,
            PackageKind::VersificationMap => CoreKind::VersificationMap,
            PackageKind::Book => CoreKind::Book,
            PackageKind::Collection => CoreKind::Collection,
        }
    }
}

/// A package's `[package]`: what it is, and its license (shown, never enforced)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageInfo {
    /// What dependencies call it (`bsb`)
    pub id: String,
    pub name: String,
    /// What references name it by (`John 3:16 BSB`)
    pub abbreviation: String,
    /// Who wrote it, in order
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub aliases: Vec<String>,
    pub version: String,
    pub kind: PackageKind,
    /// BCP 47: `en`, `grc`
    pub language: String,
    /// The language's name, when the package gives it (`English`)
    pub language_name: Option<String>,
    /// Its script, ISO 15924 (`Latn`, `Hebr`): as said, else the language tag's
    pub script: Option<String>,
    /// Whether the text is written right to left (as said, else as its script runs)
    pub right_to_left: bool,
    /// An SPDX expression (`CC0-1.0`)
    pub license: Option<String>,
    /// What the license allows: `[package.rights]`, with the rest derived from `license`
    pub rights: Rights,
    /// A file in the package with the license's text
    pub license_file: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    /// Its repository's page
    pub repository: Option<String>,
    pub documentation: Option<String>,
    /// Its README's path in the package
    pub readme: Option<String>,
    /// Words a search should find it by
    pub keywords: Vec<String>,
    /// What it's about, as slugs (`systematic-theology`)
    pub categories: Vec<String>,
    /// The books of the Bible it's mainly about, as USFM codes (`ROM`), as its author says
    pub about_books: Vec<String>,
    /// The work it's an edition of (`kjv`), and which edition (`1769-oxford`) of what year
    pub work: Option<String>,
    pub edition: Option<String>,
    pub edition_year: Option<i32>,
    /// Where the text was imported from, if anywhere (`[package.upstream]`)
    pub upstream: Option<Upstream>,
    /// `own` (the text's verse markers say), or a named system like `English`
    pub versification: String,
    /// For a versification map: the versification it maps `versification` to
    pub maps_to: Option<String>,
}

impl From<&manifest::Package> for PackageInfo {
    fn from(info: &manifest::Package) -> Self {
        PackageInfo {
            id: info.id.clone(),
            name: info.name.clone(),
            abbreviation: info.abbreviation.clone(),
            authors: info.authors.clone(),
            description: info.description.clone(),
            aliases: info.aliases.clone(),
            version: info.version.to_string(),
            kind: info.kind.into(),
            language: info.language.clone(),
            language_name: info.language_name.clone(),
            script: info.text_script(),
            right_to_left: info.text_direction() == Direction::Rtl,
            license: info.license.clone(),
            rights: info.text_rights().into(),
            license_file: info.license_file.clone(),
            publisher: info.publisher.clone(),
            homepage: info.homepage.clone(),
            repository: info.repository.clone(),
            documentation: info.documentation.clone(),
            readme: info.readme.clone(),
            keywords: info.keywords.clone(),
            categories: info.categories.clone(),
            about_books: info.about_books.clone(),
            work: info.work.clone(),
            edition: info.edition.clone(),
            edition_year: info.edition_year,
            upstream: info.upstream.as_ref().map(Upstream::from),
            versification: info.versification.clone(),
            maps_to: info.maps_to.clone(),
        }
    }
}

/**
Where an imported package's text came from (`[package.upstream]`): the download, the import's
name (`ebible:eng-asv`), and the publisher's version as it gives it (`2026-10-08`). The package's
own `version` is logoxide's, raised by each update
*/
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    /// The download it was made from; none for a package made by hand from a source without one
    /// (then `note` says where)
    pub url: Option<String>,
    pub import: Option<String>,
    pub version: Option<String>,
    /// The import rewrote the publisher's markup (a SWORD module's entries as OSIS files), its
    /// words unchanged: not a derivative to logoxide, but worth saying where the license allows
    /// none
    pub converted: bool,
    pub note: Option<String>,
}

impl From<&manifest::Upstream> for Upstream {
    fn from(u: &manifest::Upstream) -> Self {
        Self {
            url: u.url.clone(),
            import: u.import.clone(),
            version: u.version.clone(),
            converted: u.converted,
            note: u.note.clone(),
        }
    }
}

impl From<Upstream> for manifest::Upstream {
    fn from(u: Upstream) -> Self {
        Self {
            url: u.url,
            import: u.import,
            version: u.version,
            converted: u.converted,
            note: u.note,
        }
    }
}

/// Whether a license allows derivatives (translations, revisions, adaptations)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Derivatives {
    Allowed,
    /// Under the same terms (Creative Commons' ShareAlike, the GPL)
    ShareAlike,
    Forbidden,
}

/**
What a license allows (`[package.rights]`, and what's derived from a license logoxide knows),
each unknown when neither says. Shown, never enforced
*/
#[data]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rights {
    /// May be shared as it is
    pub redistributable: Option<bool>,
    pub derivatives: Option<Derivatives>,
    /// May be sold or used commercially
    pub commercial: Option<bool>,
    /// Sharing it must name its source
    pub attribution: Option<bool>,
}

impl From<license::Rights> for Rights {
    fn from(r: license::Rights) -> Self {
        Self {
            redistributable: r.redistributable,
            derivatives: r.derivatives.map(|d| match d {
                license::Derivatives::Allowed => Derivatives::Allowed,
                license::Derivatives::ShareAlike => Derivatives::ShareAlike,
                license::Derivatives::None => Derivatives::Forbidden,
            }),
            commercial: r.commercial,
            attribution: r.attribution,
        }
    }
}

impl From<Rights> for license::Rights {
    fn from(r: Rights) -> Self {
        Self {
            redistributable: r.redistributable,
            derivatives: r.derivatives.map(|d| match d {
                Derivatives::Allowed => license::Derivatives::Allowed,
                Derivatives::ShareAlike => license::Derivatives::ShareAlike,
                Derivatives::Forbidden => license::Derivatives::None,
            }),
            commercial: r.commercial,
            attribution: r.attribution,
        }
    }
}

/// What a license concern is about (`LicenseConcern`)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConcernKind {
    /// No SPDX license given
    NoLicense,
    /// A license logoxide doesn't know to be free or open
    NotFree,
    /// Its rights say it isn't redistributable
    NotRedistributable,
    /// Its license allows no derivatives, and its import converted the publisher's markup
    Converted,
}

/// Something to know before redistributing a package (shown, never enforced)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenseConcern {
    pub kind: ConcernKind,
    pub message: String,
    /// To be confirmed before publishing, not only said (a package that isn't redistributable)
    pub serious: bool,
}

impl From<license::Concern> for LicenseConcern {
    fn from(c: license::Concern) -> Self {
        Self {
            serious: c.is_serious(),
            kind: match c.kind {
                license::ConcernKind::NoLicense => ConcernKind::NoLicense,
                license::ConcernKind::NotFree => ConcernKind::NotFree,
                license::ConcernKind::NotRedistributable => ConcernKind::NotRedistributable,
                license::ConcernKind::Converted => ConcernKind::Converted,
            },
            message: c.message,
        }
    }
}

/// One of the categories a package can be in
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Category {
    /// As `[package] categories` names it (`church-history`)
    pub name: String,
    /// What it's for
    pub description: String,
}

/// An audio or video file a package lists
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaFile {
    /// Its name in the package: `JHN.mp3`
    pub path: String,
    pub url: String,
    /// Other places serving the same bytes, to try in order when `url` fails
    pub mirrors: Vec<String>,
    pub size: u64,
    /// Lowercase hex
    pub sha256: String,
    /// The book it reads (a USFM code), if one
    pub book: Option<String>,
    /// Its timings' path in the package, if not where the timings folder puts them
    pub timings: Option<String>,
}

impl From<&logoxide_core::media::MediaFile> for MediaFile {
    fn from(f: &logoxide_core::media::MediaFile) -> Self {
        Self {
            path: f.path.clone(),
            url: f.url.clone(),
            mirrors: f.mirrors.clone(),
            size: f.size,
            sha256: f.sha256.clone(),
            book: f.book.clone(),
            timings: f.timings.clone(),
        }
    }
}

/// Where one verse (or a bridge: `verse` to `last`) is in a recording, from its timings
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseTiming {
    /// The book's USFM code: `JHN`
    pub book: String,
    /// Its number, as `LogoxidePackage.books()` gives them (0 for a code topos doesn't know)
    pub book_id: u8,
    pub chapter: u16,
    pub verse: u16,
    pub last: u16,
    /// From the start of the recording, in milliseconds
    pub start_ms: u32,
    pub end_ms: u32,
}

impl From<logoxide_core::media::VerseTiming> for VerseTiming {
    fn from(t: logoxide_core::media::VerseTiming) -> Self {
        let books = logoxide_core::bible_data().books();
        Self {
            book_id: books.search_usfm(&t.book).map_or(0, |id| id.0),
            book: t.book,
            chapter: t.chapter,
            verse: t.verse,
            last: t.last,
            start_ms: t.start_ms,
            end_ms: t.end_ms,
        }
    }
}

impl From<MediaFile> for logoxide_core::media::MediaFile {
    fn from(f: MediaFile) -> Self {
        Self {
            path: f.path,
            url: f.url,
            mirrors: f.mirrors,
            size: f.size,
            sha256: f.sha256,
            book: f.book,
            timings: f.timings,
        }
    }
}

/// A book and its file in a package
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookFile {
    pub book_id: u8,
    /// Relative to the package's folder: `usx/JHN.usx`
    pub path: String,
    /// What readers see it as (`John`): its title in `[titles]`, else the file's, else the book's
    /// name in English
    pub title: Option<String>,
}

/// A verse of a book (a bridge, `4-5`, is one) and the hash of its words, as hex
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseHashEntry {
    pub chapter: u8,
    pub verse: u8,
    /// The last verse a bridge covers (`verse` otherwise)
    pub last: u8,
    /// Which part of the verse (1 for `20a`, 2 for `20b`), 0 for the whole verse
    pub part: u8,
    /// The first 7 bytes of the SHA-256 of its words (verse-hash rule 1), as 14 hex digits; links
    /// keep the first 8 digits
    pub hash: String,
}

/// A file a reader sees (a Bible book's, or an EPUB) and its title
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TitledFile {
    pub path: String,
    /// None when nothing gives one (`lx check` refuses that)
    pub title: Option<String>,
}

/// A file, or a book in one, that the package's format passed over
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedFile {
    pub path: String,
    /// Its book's code or name as the file gives it (`FRT`, `JUB`); empty when it names none
    pub book: String,
    /// `peripheral` (front matter, a glossary, …), `unknown` (a book logoxide doesn't know),
    /// `no-book`, or `lettered-chapter` (Esther's chapters A to F)
    pub reason: String,
    /// What `lx check` says of it (`usx/FRT.usx: FRT isn't a book of the Bible, passed over`)
    pub message: String,
    /// Worth a warning (its verses are left out), not only a note
    pub warning: bool,
}

/// A memory storage that remembers what was written to it, so the host can save it
struct Recording {
    files: MemoryStorage,
    written: Mutex<Vec<String>>,
}

impl Storage for Recording {
    fn read(&self, path: &str) -> logoxide_core::Result<Vec<u8>> {
        self.files.read(path)
    }

    fn list(&self, dir: &str) -> logoxide_core::Result<Vec<String>> {
        self.files.list(dir)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> logoxide_core::Result<()> {
        self.files.write(path, bytes)?;
        if let Ok(mut written) = self.written.lock() {
            written.push(path.to_owned());
        }
        Ok(())
    }
}

fn memory(files: Vec<PackageFile>) -> MemoryStorage {
    MemoryStorage::with(files.into_iter().map(|f| (f.path, f.bytes)))
}

/**
A package and its cache: its information, verse counts, and books, read once and kept. Opened
with `Logoxide::open_package` (or `open_package_dir`)

```ts
// The host reads the files (every one under the package's folder) and saves what the cache wrote
const pkg = logoxide.openPackage(files, entry.features, cacheFiles);
for (const file of pkg.cacheWrites()) await save(`${cacheDir}/${file.path}`, file.bytes);
pkg.info().abbreviation; // "BSB"
translations = pkg.addTo(translations); // references naming it use its verse counts
const john = pkg.book(43); // a LogoxideBook, for passages as HTML or text
```

- Opening counts every book's verses unless the cache has them, which for USFM means parsing it
  (seconds for a whole Bible); give the cache's files back next time to skip that
- Books are parsed when first asked for, and cached
*/
pub struct LogoxidePackage {
    cached: CachedPackage,
    /// The cache, when the host keeps it (`Logoxide::open_package`)
    cache: Option<Arc<Recording>>,
}

impl LogoxidePackage {
    /// A package from its files (paths relative to its folder), and the cache files saved from
    /// `cache_writes` before (paths relative to the cache's folder)
    pub(crate) fn open(
        files: Vec<PackageFile>,
        features: PackageFeatures,
        cache: Vec<PackageFile>,
    ) -> Result<Self, LogoxideError> {
        let recording = Arc::new(Recording {
            files: memory(cache),
            written: Mutex::default(),
        });
        Self::new(
            Arc::new(memory(files)),
            &features,
            recording.clone(),
            Some(recording),
        )
    }

    /// A package in a folder, with its cache in `cache_dir` (written there as books are read).
    /// Not in WebAssembly, which has no file access
    pub(crate) fn open_dir(
        dir: String,
        features: PackageFeatures,
        cache_dir: String,
    ) -> Result<Self, LogoxideError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use logoxide_core::FsStorage;
            Self::new(
                Arc::new(FsStorage::new(dir)),
                &features,
                Arc::new(FsStorage::new(cache_dir)),
                None,
            )
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (dir, features, cache_dir);
            Err(LogoxideError::Files {
                message: String::from("no file access in WebAssembly: use openPackage"),
            })
        }
    }

    fn new(
        storage: Arc<dyn Storage>,
        features: &PackageFeatures,
        cache_storage: Arc<dyn Storage>,
        cache: Option<Arc<Recording>>,
    ) -> Result<Self, LogoxideError> {
        let names: Vec<&str> = features.features.iter().map(String::as_str).collect();
        let package = Package::open(
            storage,
            Features {
                features: &names,
                default: features.default_features,
            },
        )?;
        let cached =
            Cache::new(cache_storage).open(package, &logoxide_core::reader::DefaultReader)?;
        Ok(Self { cached, cache })
    }

    pub(crate) fn package(&self) -> &Package {
        self.cached.package()
    }
}

#[export]
impl LogoxidePackage {
    pub fn info(&self) -> PackageInfo {
        PackageInfo::from(self.cached.package().info())
    }

    /// What to know before redistributing it, from its license and rights (shown, never
    /// enforced): no license, one not known to be free, rights saying it isn't redistributable
    /// (`serious`: ask before publishing), markup converted where derivatives aren't allowed
    pub fn license_concerns(&self) -> Vec<LicenseConcern> {
        (license::assess(self.cached.package().info()).into_iter())
            .map(LicenseConcern::from)
            .collect()
    }

    /// A hash of every file (`sha256:` and hex), to compare with the lock file's
    pub fn checksum(&self) -> Result<String, LogoxideError> {
        Ok(self.cached.package().checksum()?)
    }

    /// The format the text is read from (`usx`, `usfm`)
    pub fn format(&self) -> String {
        self.cached.manifest().source_format.clone()
    }

    /// The books it has, in order (1 for Genesis through 66 for Revelation, then the
    /// deuterocanon)
    pub fn books(&self) -> Vec<u8> {
        let books = &self.cached.manifest().books;
        books.iter().map(|(id, _)| id.0).collect()
    }

    /// The audio and video it lists (by URL and hash; download one when it's played, from its
    /// `url` or else each of its `mirrors` in turn, check it with `Logoxide::verify_media`, and
    /// keep it at `Logoxide::media_store_path`)
    pub fn media(&self) -> Vec<MediaFile> {
        (self.cached.package().manifest().media.iter())
            .flat_map(|m| m.files.iter())
            .map(MediaFile::from)
            .collect()
    }

    /// For a recording without a text of its own, the dependency whose text it reads (`[media]
    /// text`: show its verses with the timings)
    pub fn media_text(&self) -> Option<String> {
        (self.cached.package().manifest().media.as_ref()).and_then(|m| m.text.clone())
    }

    /// The versification a recording is numbered by (`[media] versification`), if it says
    pub fn media_versification(&self) -> Option<String> {
        (self.cached.package().manifest().media.as_ref()).and_then(|m| m.versification.clone())
    }

    /// The Bible references in its books (EPUBs), in book order, each with its chapter and CFI;
    /// empty for a package without books. Found by reading them (well under a second a book) the
    /// first time, then kept in the cache (see `cacheWrites`)
    pub fn references(&self) -> Result<Vec<crate::FoundReference>, LogoxideError> {
        let found = self.cached.references()?;
        Ok(found.iter().cloned().map(Into::into).collect())
    }

    /// Its books' EPUBs, relative to its folder (`epub/the-dorean-principle.epub`), for hosts
    /// that show them as files of their own; empty for a package without books
    pub fn epub_files(&self) -> Result<Vec<String>, LogoxideError> {
        Ok(logoxide_core::references::epub_files(
            self.cached.package(),
        )?)
    }

    /// Where each verse is in a listed file (from the package's WebVTT timings, in their order),
    /// to play from a verse or follow along; empty if it has none
    pub fn timings(&self, file: MediaFile) -> Result<Vec<VerseTiming>, LogoxideError> {
        let file = file.into();
        let timings = logoxide_core::media::timings(self.cached.package(), &file)?;
        Ok(timings
            .into_iter()
            .flatten()
            .map(VerseTiming::from)
            .collect())
    }

    /// Each book's file, relative to the package's folder (`usx/JHN.usx`), for hosts that open
    /// book files themselves
    pub fn book_files(&self) -> Vec<BookFile> {
        let titles = self.titles();
        let files = self.cached.package().source().files();
        (files.iter())
            .map(|(id, path)| BookFile {
                book_id: id.0,
                path: path.clone(),
                title: (titles.iter())
                    .find(|t| t.path == *path)
                    .and_then(|t| t.title.clone()),
            })
            .collect()
    }

    /// The files, or books in them, its format passed over: peripherals, and books logoxide
    /// doesn't know (whose verses are left out)
    pub fn skipped(&self) -> Vec<SkippedFile> {
        let reason = |r: SkipReason| match r {
            SkipReason::Peripheral => "peripheral",
            SkipReason::Unknown => "unknown",
            SkipReason::NoBook => "no-book",
            SkipReason::LetteredChapter => "lettered-chapter",
        };
        (self.cached.package().source().skipped().iter())
            .map(|s| SkippedFile {
                path: s.path.clone(),
                book: s.book.clone(),
                reason: reason(s.reason).to_string(),
                message: s.to_string(),
                warning: s.is_warning(),
            })
            .collect()
    }

    /**
    Each verse of a book, with the hash of its words (notes and headings left out, normalized as
    verse-hash rule 1 says; the first 7 bytes of their SHA-256, as hex): a link made to a verse records it, and finds the verse again
    in a later version by it (the same words renumbered), or knows they changed
    */
    pub fn verse_hashes(&self, book_id: u8) -> Result<Vec<VerseHashEntry>, LogoxideError> {
        let book = self.cached.book(BookId(book_id))?;
        Ok((logoxide_core::summary::book_hashes(&book).into_iter())
            .map(|v| VerseHashEntry {
                chapter: v.chapter,
                verse: v.verse,
                last: v.last,
                part: v.part,
                hash: v.hash.iter().map(|b| format!("{b:02x}")).collect(),
            })
            .collect())
    }

    /// Each Bible book's file and each EPUB, with its title, in path order: what hosts name the
    /// files they show by (a title changes only in a minor or major release)
    pub fn titles(&self) -> Vec<TitledFile> {
        (logoxide_core::titles::titles(self.cached.package()).unwrap_or_default())
            .into_iter()
            .map(|t| TitledFile {
                path: t.path,
                title: t.title,
            })
            .collect()
    }

    /// A book's verse counts, one per chapter (empty if the package doesn't have it)
    pub fn chapter_verses(&self, book_id: u8) -> Vec<u8> {
        let books = &self.cached.manifest().books;
        (books.iter().find(|(id, _)| id.0 == book_id))
            .map(|(_, counts)| counts.clone())
            .unwrap_or_default()
    }

    /// A book's text, for passages as HTML or plain text (`NotFound` if the package doesn't have
    /// it: see [`books`](Self::books))
    pub fn book(&self, book_id: u8) -> Result<LogoxideBook, LogoxideError> {
        if !self.books().contains(&book_id) {
            let id = self.cached.package().info().id.clone();
            return Err(LogoxideError::NotFound {
                message: format!("{id} has no book {book_id}"),
            });
        }
        let book = self.cached.book(BookId(book_id))?;
        Ok(LogoxideBook::from_tree(
            book.nodes.clone(),
            book.spans.clone(),
            Some(book.id.0),
        ))
    }

    /// `translations` with this package added: its abbreviation, aliases, and verse counts, so
    /// references naming it are checked against them
    pub fn add_to(&self, translations: &ToposTranslations) -> ToposTranslations {
        let translations: Translations = translations.core().clone();
        self.cached.add_to(translations).into()
    }

    /// `translations` with this package added by the project's own names for it
    /// (`LockedEntry.abbreviation` and `aliases`, when the manifest gives them)
    pub fn add_to_as(
        &self,
        translations: &ToposTranslations,
        abbreviation: String,
        aliases: Vec<String>,
    ) -> ToposTranslations {
        let translations: Translations = translations.core().clone();
        (self.cached)
            .add_as(translations, &abbreviation, &aliases)
            .into()
    }

    /// The cache files written since this was last asked (paths relative to the cache's
    /// folder), for the host to save; empty for `Logoxide::open_package_dir`, which writes its
    /// own
    pub fn cache_writes(&self) -> Vec<PackageFile> {
        let Some(cache) = &self.cache else {
            return Vec::new();
        };
        let paths = match cache.written.lock() {
            Ok(mut written) => std::mem::take(&mut *written),
            Err(_) => return Vec::new(),
        };
        let mut seen = std::collections::BTreeSet::new();
        (paths.into_iter())
            .filter(|path| seen.insert(path.clone()))
            .filter_map(|path| {
                let bytes = cache.files.read(&path).ok()?;
                Some(PackageFile { path, bytes })
            })
            .collect()
    }
}
