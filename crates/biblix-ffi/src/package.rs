//! A package opened from its files, with its cache: its information, verse counts, and books.

use std::sync::{Arc, Mutex};

use biblix_core::{
    Cache, CachedPackage, Features, MemoryStorage, Package, Storage,
    manifest::{self, Direction, PackageKind as CoreKind},
    topos_bible::data::{books::BookId, translations::Translations},
};
use boltffi::*;
use topos_bible::{ToposBook, ToposTranslations};

use crate::{BiblixError, PackageFeatures, PackageFile};

/// What a package is (`kind` in its `biblix.toml`)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    Bible,
    Commentary,
    Lexicon,
    CrossReferences,
    VersificationMap,
    Book,
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
    pub aliases: Vec<String>,
    pub version: String,
    pub kind: PackageKind,
    /// BCP 47: `en`, `grc`
    pub language: String,
    /// Whether the text is written right to left
    pub right_to_left: bool,
    /// An SPDX expression (`CC0-1.0`)
    pub license: Option<String>,
    /// A file in the package with the license's text
    pub license_file: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    /// Where the text was imported from, if anywhere
    pub upstream: Option<String>,
    /// `own` (the text's verse markers say), or a named system like `KJV`
    pub versification: String,
}

impl From<&manifest::Package> for PackageInfo {
    fn from(info: &manifest::Package) -> Self {
        PackageInfo {
            id: info.id.clone(),
            name: info.name.clone(),
            abbreviation: info.abbreviation.clone(),
            aliases: info.aliases.clone(),
            version: info.version.to_string(),
            kind: info.kind.into(),
            language: info.language.clone(),
            right_to_left: info.direction == Direction::Rtl,
            license: info.license.clone(),
            license_file: info.license_file.clone(),
            publisher: info.publisher.clone(),
            homepage: info.homepage.clone(),
            upstream: info.upstream.clone(),
            versification: info.versification.clone(),
        }
    }
}

/// An audio or video file a package lists
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaFile {
    /// Its name in the package: `JHN.mp3`
    pub path: String,
    pub url: String,
    pub size: u64,
    /// Lowercase hex
    pub sha256: String,
    /// The book it reads (a USFM code), if one
    pub book: Option<String>,
}

impl From<&biblix_core::media::MediaFile> for MediaFile {
    fn from(f: &biblix_core::media::MediaFile) -> Self {
        Self {
            path: f.path.clone(),
            url: f.url.clone(),
            size: f.size,
            sha256: f.sha256.clone(),
            book: f.book.clone(),
        }
    }
}

/// Where one verse (or a bridge: `verse` to `last`) is in a recording, from its timings
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseTiming {
    /// The book's USFM code: `JHN`
    pub book: String,
    /// Its number, as `BiblixPackage.books()` gives them (0 for a code topos doesn't know)
    pub book_id: u8,
    pub chapter: u16,
    pub verse: u16,
    pub last: u16,
    /// From the start of the recording, in milliseconds
    pub start_ms: u32,
    pub end_ms: u32,
}

impl From<biblix_core::media::VerseTiming> for VerseTiming {
    fn from(t: biblix_core::media::VerseTiming) -> Self {
        let books = biblix_core::bible_data().books();
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

impl From<MediaFile> for biblix_core::media::MediaFile {
    fn from(f: MediaFile) -> Self {
        Self {
            path: f.path,
            url: f.url,
            size: f.size,
            sha256: f.sha256,
            book: f.book,
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
}

/// A memory storage that remembers what was written to it, so the host can save it
struct Recording {
    files: MemoryStorage,
    written: Mutex<Vec<String>>,
}

impl Storage for Recording {
    fn read(&self, path: &str) -> biblix_core::Result<Vec<u8>> {
        self.files.read(path)
    }

    fn list(&self, dir: &str) -> biblix_core::Result<Vec<String>> {
        self.files.list(dir)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> biblix_core::Result<()> {
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
with `Biblix::open_package` (or `open_package_dir`)

```ts
// The host reads the files (every one under the package's folder) and saves what the cache wrote
const pkg = biblix.openPackage(files, entry.features, cacheFiles);
for (const file of pkg.cacheWrites()) await save(`${cacheDir}/${file.path}`, file.bytes);
pkg.info().abbreviation; // "BSB"
translations = pkg.addTo(translations); // references naming it use its verse counts
const john = pkg.book(43); // a ToposBook, for passages as HTML or text
```

- Opening counts every book's verses unless the cache has them, which for USFM means parsing it
  (seconds for a whole Bible); give the cache's files back next time to skip that
- Books are parsed when first asked for, and cached
*/
pub struct BiblixPackage {
    cached: CachedPackage,
    /// The cache, when the host keeps it (`Biblix::open_package`)
    cache: Option<Arc<Recording>>,
}

impl BiblixPackage {
    /// A package from its files (paths relative to its folder), and the cache files saved from
    /// `cache_writes` before (paths relative to the cache's folder)
    pub(crate) fn open(
        files: Vec<PackageFile>,
        features: PackageFeatures,
        cache: Vec<PackageFile>,
    ) -> Result<Self, BiblixError> {
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
    ) -> Result<Self, BiblixError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use biblix_core::FsStorage;
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
            Err(BiblixError::Files {
                message: String::from("no file access in WebAssembly: use openPackage"),
            })
        }
    }

    fn new(
        storage: Arc<dyn Storage>,
        features: &PackageFeatures,
        cache_storage: Arc<dyn Storage>,
        cache: Option<Arc<Recording>>,
    ) -> Result<Self, BiblixError> {
        let names: Vec<&str> = features.features.iter().map(String::as_str).collect();
        let package = Package::open(
            storage,
            Features {
                features: &names,
                default: features.default_features,
            },
        )?;
        let cached = Cache::new(cache_storage).open(package)?;
        Ok(Self { cached, cache })
    }
}

#[export]
impl BiblixPackage {
    pub fn info(&self) -> PackageInfo {
        PackageInfo::from(self.cached.package().info())
    }

    /// A hash of every file (`sha256:` and hex), to compare with the lock file's
    pub fn checksum(&self) -> Result<String, BiblixError> {
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

    /// The audio and video it lists (by URL and hash; download one when it's played, check it
    /// with `Biblix::verify_media`, and keep it at `Biblix::media_store_path`)
    pub fn media(&self) -> Vec<MediaFile> {
        (self.cached.package().manifest().media.iter())
            .flat_map(|m| m.files.iter())
            .map(MediaFile::from)
            .collect()
    }

    /// Where each verse is in a listed file (from the package's WebVTT timings, in their order),
    /// to play from a verse or follow along; empty if it has none
    pub fn timings(&self, file: MediaFile) -> Result<Vec<VerseTiming>, BiblixError> {
        let file = file.into();
        let timings = biblix_core::media::timings(self.cached.package(), &file)?;
        Ok(timings
            .into_iter()
            .flatten()
            .map(VerseTiming::from)
            .collect())
    }

    /// Each book's file, relative to the package's folder (`usx/JHN.usx`), for hosts that open
    /// book files themselves
    pub fn book_files(&self) -> Vec<BookFile> {
        let files = self.cached.package().source().files();
        (files.iter())
            .map(|(id, path)| BookFile {
                book_id: id.0,
                path: path.clone(),
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
    pub fn book(&self, book_id: u8) -> Result<ToposBook, BiblixError> {
        if !self.books().contains(&book_id) {
            let id = self.cached.package().info().id.clone();
            return Err(BiblixError::NotFound {
                message: format!("{id} has no book {book_id}"),
            });
        }
        let book = self.cached.book(BookId(book_id))?;
        Ok(ToposBook::from_tree(
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
    /// folder), for the host to save; empty for `Biblix::open_package_dir`, which writes its
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
