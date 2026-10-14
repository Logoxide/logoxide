/*!
Parsed books and verse counts, kept per package version so a book loads without parsing its
source again. A cache is per device and never synced: it can always be built again.

```text
<cache>/<id>/<version>-<checksum>-<format>/
  manifest.postcard     the verse counts of every book, and what the cache was built from (and
                        by which logoxide: another's is built again)
  books/<CODE>.postcard each book's tree and verse spans, written when first read: logoxide-text's
                        book cache (`logoxide_text::cache`), which starts each file with a line
                        naming its cache format and version (others are parsed again)
  references.postcard   the Bible references in its EPUBs, written when first found
```

The folder names the format read (`usx`, `usfm`, `none` for a package without text), so a package
read in one format and then another (its features changed) never takes one's books for the
other's: they can number verses differently. The books are logoxide-text's cache keyed by
`<checksum>-<format>` (the same facts this folder's name carries, so the key is dropped from the
path: `VersionBooks`), and whoever else keys a text that way (the language server, a host of the
bindings) reads the books this wrote.

References are found by reading every EPUB (a tenth of a second a book natively, a quarter in
WebAssembly), so they're kept like parsed books. They're kept per logoxide version too, since a
newer topos may find references an older one missed.
*/

use std::sync::Arc;
#[cfg(feature = "read")]
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use topos_bible::data::{books::BookId, chapter_verses::ChapterVerses, translations::Translations};

#[cfg(feature = "read")]
use crate::references::FoundReference;
use crate::{
    error::{LogoxideError, Result},
    package::Package,
    reader::TextReader,
    storage::Storage,
};
#[cfg(feature = "read")]
use logoxide_text::{Book, cache::BookCache};

/// Bumped when what the cache holds changes shape, so older caches are built again (the books'
/// own format is logoxide-text's `cache::CACHE_FORMAT`)
pub const CACHE_FORMAT: u32 = 4;

/// What a package version's cache was built from, and its verse counts
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CachedManifest {
    pub cache_format: u32,
    /// The logoxide that built it
    pub engine: String,
    pub id: String,
    pub version: String,
    pub checksum: String,
    /// The format the text was read from (`usx`, `usfm`)
    pub source_format: String,
    /// Each book and the number of verses in each of its chapters
    pub books: Vec<(BookId, Vec<u8>)>,
}

/// A package version's references, and what found them
#[cfg(feature = "read")]
#[derive(Serialize, Deserialize)]
struct CachedReferences {
    cache_format: u32,
    /// The logoxide that found them
    engine: String,
    checksum: String,
    references: Vec<FoundReference>,
}

pub struct Cache {
    storage: Arc<dyn Storage>,
    /// Where checksums are remembered by the files' stamps (`stamps.rs`); without one,
    /// every open hashes the package
    stamps: Option<Arc<dyn Storage>>,
}

/// The format a package is read in, as the cache names it (`usx`, `usfm`; `none` without text)
fn format_name(package: &Package) -> String {
    (package.text_format()).map_or(String::from("none"), |(f, _)| {
        format!("{f:?}").to_lowercase()
    })
}

/// The first 16 hex digits of a checksum
fn short_checksum(checksum: &str) -> &str {
    checksum
        .trim_start_matches("sha256:")
        .get(..16)
        .unwrap_or_default()
}

/// A package version's folder in the cache, for the format it's read in
fn version_dir(package: &Package, checksum: &str) -> String {
    let info = package.info();
    format!(
        "{}/{}-{}-{}",
        info.id,
        info.version,
        short_checksum(checksum),
        format_name(package)
    )
}

/// What logoxide-text's book cache keys the package's text by: `<checksum>-<format>`
#[cfg(feature = "read")]
fn book_key(package: &Package, checksum: &str) -> String {
    format!("{}-{}", short_checksum(checksum), format_name(package))
}

/**
A version's `books/` folder as the store logoxide-text's book cache writes to. The cache names
each file by the text's key (`<key>/JHN.postcard`); the folder's name already says what the
key does, so the key is dropped: `books/JHN.postcard` under the version's folder, where the
books have always been
*/
#[cfg(feature = "read")]
struct VersionBooks {
    storage: Arc<dyn Storage>,
    dir: String,
    key: String,
}

#[cfg(feature = "read")]
impl VersionBooks {
    fn path(&self, path: &str) -> String {
        let name = (path.strip_prefix(self.key.as_str()))
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(path);
        format!("{}/books/{name}", self.dir)
    }
}

#[cfg(feature = "read")]
impl logoxide_text::cache::BookStore for VersionBooks {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.storage.read(&self.path(path)).ok()
    }

    fn write(&self, path: &str, bytes: &[u8]) -> logoxide_text::Result<()> {
        (self.storage.write(&self.path(path), bytes))
            .map_err(|e| logoxide_text::TextError::Files(e.to_string()))
    }
}

/// The references in a package's EPUBs: from the cache if they were found in this exact text by
/// this logoxide, else found now and cached (a package without EPUBs has none, and writes nothing)
#[cfg(feature = "read")]
fn references(
    storage: &dyn Storage,
    package: &Package,
    dir: &str,
    checksum: &str,
) -> Result<Vec<FoundReference>> {
    if crate::references::book_files(package)?.is_empty() {
        return Ok(vec![]);
    }
    let engine = env!("CARGO_PKG_VERSION");
    let path = format!("{dir}/references.postcard");
    let cached = (storage.read(&path).ok())
        .and_then(|bytes| postcard::from_bytes::<CachedReferences>(&bytes).ok())
        .filter(|c| c.cache_format == CACHE_FORMAT && c.engine == engine && c.checksum == checksum);
    if let Some(cached) = cached {
        return Ok(cached.references);
    }
    let cached = CachedReferences {
        cache_format: CACHE_FORMAT,
        engine: engine.to_string(),
        checksum: checksum.to_string(),
        references: crate::references::find(package)?,
    };
    if let Ok(bytes) = postcard::to_stdvec(&cached) {
        let _ = storage.write(&path, &bytes);
    }
    Ok(cached.references)
}

impl Cache {
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            stamps: None,
        }
    }

    /// With a place to remember checksums by the files' sizes and times (`stamps.rs`), so
    /// opening a package that hasn't changed doesn't hash it again
    pub fn with_stamps(mut self, stamps: Arc<dyn Storage>) -> Self {
        self.stamps = Some(stamps);
        self
    }

    /// The package's checksum: remembered by its files' stamps when it can be, else hashed now
    fn checksum(&self, package: &Package) -> Result<String> {
        let storage = package.storage().as_ref();
        let declared = crate::package::declared_files(storage)?;
        let info = package.info();
        let path = storage
            .location()
            .map(|at| crate::stamps::stamps_path(&info.id, &info.version.to_string(), &at));
        match (&self.stamps, path) {
            (Some(store), Some(path)) => {
                crate::stamps::checksum(Some(store.as_ref()), storage, &declared, &path)
            }
            _ => crate::package::checksum_of(storage, &declared),
        }
    }

    /// A package's cache: read if it was built from this exact text, else built now by `reader`
    /// (verse counts only; books are cached as they're read)
    pub fn open(&self, package: Package, reader: &dyn TextReader) -> Result<CachedPackage> {
        let info = package.info();
        let checksum = self.checksum(&package)?;
        let dir = version_dir(&package, &checksum);
        let path = format!("{dir}/manifest.postcard");
        let engine = env!("CARGO_PKG_VERSION");
        let cached = self
            .storage
            .read(&path)
            .ok()
            .and_then(|bytes| postcard::from_bytes::<CachedManifest>(&bytes).ok())
            .filter(|m| {
                m.cache_format == CACHE_FORMAT && m.engine == engine && m.checksum == checksum
            });
        let manifest = match cached {
            Some(manifest) => manifest,
            None => {
                let books = reader.counts(&package)?;
                let manifest = CachedManifest {
                    cache_format: CACHE_FORMAT,
                    engine: engine.to_string(),
                    id: info.id.clone(),
                    version: info.version.to_string(),
                    checksum,
                    source_format: format_name(&package),
                    books,
                };
                let bytes = postcard::to_stdvec(&manifest)
                    .map_err(|e| LogoxideError::Cache(e.to_string()))?;
                // A cache that can't be written is still a cache for this run
                let _ = self.storage.write(&path, &bytes);
                manifest
            }
        };
        #[cfg(feature = "read")]
        let books = {
            let key = book_key(&package, &manifest.checksum);
            let store = VersionBooks {
                storage: self.storage.clone(),
                dir: dir.clone(),
                key: key.clone(),
            };
            BookCache::new(Arc::new(store), key)
        };
        Ok(CachedPackage {
            #[cfg(feature = "read")]
            storage: self.storage.clone(),
            #[cfg(feature = "read")]
            references: Mutex::default(),
            package,
            #[cfg(feature = "read")]
            dir,
            manifest,
            #[cfg(feature = "read")]
            books,
        })
    }

    /// The Bible references in a package's EPUBs, found once per version (see
    /// [`crate::references::find`]), without reading its verse counts as [`open`](Self::open)
    /// does
    #[cfg(feature = "read")]
    pub fn references(&self, package: &Package) -> Result<Vec<FoundReference>> {
        let checksum = package.checksum()?;
        let dir = version_dir(package, &checksum);
        references(self.storage.as_ref(), package, &dir, &checksum)
    }
}

/// A package with its cache: verse counts at hand, and books parsed once
pub struct CachedPackage {
    #[cfg(feature = "read")]
    storage: Arc<dyn Storage>,
    package: Package,
    #[cfg(feature = "read")]
    dir: String,
    manifest: CachedManifest,
    /// logoxide-text's cache of its parsed books, over `storage`
    #[cfg(feature = "read")]
    books: BookCache,
    #[cfg(feature = "read")]
    references: Mutex<Option<Arc<Vec<FoundReference>>>>,
}

impl CachedPackage {
    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn manifest(&self) -> &CachedManifest {
        &self.manifest
    }

    /// A book: from memory, else the cache, else its source (then cached), through
    /// logoxide-text's book cache
    #[cfg(feature = "read")]
    pub fn book(&self, id: BookId) -> Result<Arc<Book>> {
        Ok(self.books.book(self.package.source(), id)?)
    }

    /// The Bible references in its EPUBs: from memory, else the cache, else found by reading
    /// them (then cached)
    #[cfg(feature = "read")]
    pub fn references(&self) -> Result<Arc<Vec<FoundReference>>> {
        if let Some(found) = self.references.lock().ok().and_then(|r| r.clone()) {
            return Ok(found);
        }
        let found = Arc::new(references(
            self.storage.as_ref(),
            &self.package,
            &self.dir,
            &self.manifest.checksum,
        )?);
        if let Ok(mut references) = self.references.lock() {
            *references = Some(found.clone());
        }
        Ok(found)
    }

    /// `translations` with this package added: its abbreviation, its aliases, and each book's
    /// verse counts (references naming it are checked against them)
    pub fn add_to(&self, translations: Translations) -> Translations {
        let info = self.package.info();
        self.add_as(translations, info.short_name(), &info.aliases)
    }

    /// `translations` with this package added by another abbreviation and aliases (a project's
    /// own names for it, `abbreviation = "KJVB"` on its dependency)
    pub fn add_as(
        &self,
        translations: Translations,
        abbreviation: &str,
        aliases: &[String],
    ) -> Translations {
        let name = abbreviation.to_string();
        let mut translations = (aliases.iter()).fold(
            translations.alias(name.clone(), name.clone()),
            |t, alias| t.alias(alias.clone(), name.clone()),
        );
        for (book, counts) in &self.manifest.books {
            translations.set_chapter_verses(&name, *book, ChapterVerses::new(counts.clone()));
        }
        translations
    }
}

#[cfg(all(test, feature = "read"))]
mod tests {
    use super::*;
    use crate::reader::DefaultReader;
    use crate::{Features, MemoryStorage};
    use logoxide_text::cache::book_header;

    const MANIFEST: &str = r#"
[package]
id = "test"
name = "A Test Bible"
abbreviation = "TB"
version = "1.0.0"
language = "en"

[formats]
usfm = { path = "usfm/" }
"#;

    #[test]
    fn books_from_another_logoxide_are_parsed_again() {
        let files: Arc<dyn Storage> = Arc::new(MemoryStorage::with([
            ("logoxide.toml", MANIFEST),
            (
                "usfm/JHN.usfm",
                "\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n",
            ),
        ]));
        let john = BookId(43);
        let open = || Package::open(files.clone(), Features::default()).unwrap();
        let cache_storage = Arc::new(MemoryStorage::new());
        let book = (Cache::new(cache_storage.clone()).open(open(), &DefaultReader))
            .unwrap()
            .book(john)
            .unwrap();
        let path = (cache_storage.walk("").unwrap().into_iter())
            .find(|f| f.ends_with("/books/JHN.postcard"))
            .unwrap();
        let written = cache_storage.read(&path).unwrap();
        assert!(written.starts_with(book_header().as_bytes()));
        // A file without the header (an older logoxide's, perhaps laid out otherwise) isn't
        // trusted: the book is parsed again and written over it
        let stale = Book {
            id: book.id,
            nodes: book.nodes.clone(),
            spans: vec![],
        };
        (cache_storage.write(&path, &postcard::to_stdvec(&stale).unwrap())).unwrap();
        let again = (Cache::new(cache_storage.clone()).open(open(), &DefaultReader))
            .unwrap()
            .book(john)
            .unwrap();
        assert_eq!(*again, *book);
        assert_eq!(cache_storage.read(&path).unwrap(), written);
    }

    #[test]
    fn each_format_has_its_own_cache() {
        let manifest = r#"
[package]
id = "test"
name = "A Test Bible"
abbreviation = "TB"
version = "1.0.0"
language = "en"

[formats]
usx = { path = "usx/", feature = "usx" }
usfm = { path = "usfm/" }

[features]
default = ["usx"]
usx = []
"#;
        // The two formats number John 3 differently, as a careless conversion might
        let files: Arc<dyn Storage> = Arc::new(MemoryStorage::with([
            ("logoxide.toml", manifest),
            (
                "usx/JHN.usx",
                r#"<usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c" sid="JHN 3"/><para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world.<verse eid="JHN 3:16"/></para><chapter eid="JHN 3"/></usx>"#,
            ),
            (
                "usfm/JHN.usfm",
                "\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved\n\\v 17 the world.\n",
            ),
        ]));
        let john = BookId(43);
        let cache_storage = Arc::new(MemoryStorage::new());
        let cache = Cache::new(cache_storage.clone());
        let usx = Package::open(files.clone(), Features::default()).unwrap();
        let usfm = Package::open(
            files.clone(),
            Features {
                features: &[],
                default: false,
            },
        )
        .unwrap();
        let usx = cache.open(usx, &DefaultReader).unwrap();
        usx.book(john).unwrap();
        let written = cache_storage.walk("").unwrap();
        let usfm = cache.open(usfm, &DefaultReader).unwrap();
        usfm.book(john).unwrap();
        assert_eq!(usx.manifest().source_format, "usx");
        assert_eq!(usfm.manifest().source_format, "usfm");
        assert_ne!(usx.manifest().books, usfm.manifest().books);
        let usfm_files: Vec<String> = (cache_storage.walk("").unwrap().into_iter())
            .filter(|f| !written.contains(f))
            .collect();
        // Every file the second format wrote is its own, under a folder naming it
        assert_eq!(usfm_files.len(), 2);
        assert!(
            written
                .iter()
                .all(|f| f.starts_with("test/1.0.0-") && f.contains("-usx/"))
        );
        assert!(usfm_files.iter().all(|f| f.contains("-usfm/")));
        // Opened again, each reads its own counts back
        let usx_again = Package::open(files.clone(), Features::default()).unwrap();
        assert_eq!(
            cache.open(usx_again, &DefaultReader).unwrap().manifest(),
            usx.manifest()
        );
    }

    #[test]
    fn a_cache_from_another_logoxide_is_built_again() {
        let files: Arc<dyn Storage> = Arc::new(MemoryStorage::with([
            ("logoxide.toml", MANIFEST),
            (
                "usfm/JHN.usfm",
                "\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n",
            ),
        ]));
        let open = || Package::open(files.clone(), Features::default()).unwrap();
        let cache_storage = Arc::new(MemoryStorage::new());
        let cache = Cache::new(cache_storage.clone());
        let manifest = cache
            .open(open(), &DefaultReader)
            .unwrap()
            .manifest()
            .clone();
        let path = (cache_storage.walk("").unwrap().into_iter())
            .find(|f| f.ends_with("/manifest.postcard"))
            .unwrap();
        let stale = CachedManifest {
            engine: String::from("0.0.1"),
            books: vec![],
            ..manifest.clone()
        };
        (cache_storage.write(&path, &postcard::to_stdvec(&stale).unwrap())).unwrap();
        assert_eq!(
            cache.open(open(), &DefaultReader).unwrap().manifest(),
            &manifest
        );
    }
}
