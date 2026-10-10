/*!
Parsed books and verse counts, kept per package version so a book loads without parsing its
source again. A cache is per device and never synced: it can always be built again.

```text
<cache>/<id>/<version>-<checksum>-<format>/
  manifest.postcard     the verse counts of every book, and what the cache was built from (and
                        by which logoxide: another's is built again)
  books/<CODE>.postcard each book's tree and verse spans, written when first read, after a
                        line naming the cache format and logoxide version (others are read again)
  references.postcard   the Bible references in its EPUBs, written when first found
```

The folder names the format read (`usx`, `usfm`, `none` for a package without text), so a package
read in one format and then another (its features changed) never takes one's books for the
other's: they can number verses differently.

References are found by reading every EPUB (a tenth of a second a book natively, a quarter in
WebAssembly), so they're kept like parsed books. They're kept per logoxide version too, since a
newer topos may find references an older one missed.
*/

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use topos_bible::data::{books::BookId, chapter_verses::ChapterVerses, translations::Translations};

use crate::{
    error::{LogoxideError, Result},
    package::Package,
    references::FoundReference,
    source::Book,
    storage::Storage,
};

/// Bumped when what the cache holds changes shape, so older caches are built again
pub const CACHE_FORMAT: u32 = 3;

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
}

/**
What a parsed book's file starts with: the cache format and the logoxide that wrote it. Postcard
isn't self-describing, so a book written by another logoxide (whose topos-bible-markup may lay
its trees out otherwise) could decode into the wrong tree; one without this header is parsed again
*/
fn book_header() -> String {
    format!(
        "logoxide-book {CACHE_FORMAT} {}\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// The format a package is read in, as the cache names it (`usx`, `usfm`; `none` without text)
fn format_name(package: &Package) -> String {
    (package.source().format()).map_or(String::from("none"), |f| format!("{f:?}").to_lowercase())
}

/// A package version's folder in the cache, for the format it's read in
fn version_dir(package: &Package, checksum: &str) -> String {
    let info = package.info();
    let short = checksum
        .trim_start_matches("sha256:")
        .get(..16)
        .unwrap_or_default();
    format!(
        "{}/{}-{short}-{}",
        info.id,
        info.version,
        format_name(package)
    )
}

/// The references in a package's EPUBs: from the cache if they were found in this exact text by
/// this logoxide, else found now and cached (a package without EPUBs has none, and writes nothing)
fn references(
    storage: &dyn Storage,
    package: &Package,
    dir: &str,
    checksum: &str,
) -> Result<Vec<FoundReference>> {
    if crate::references::epub_files(package)?.is_empty() {
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
        Self { storage }
    }

    /// A package's cache: read if it was built from this exact text, else built now (verse
    /// counts only; books are cached as they're read)
    pub fn open(&self, package: Package) -> Result<CachedPackage> {
        let info = package.info();
        let checksum = package.checksum()?;
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
                let source = package.source();
                let books = (source.books().into_iter())
                    .map(|id| Ok((id, source.chapter_verses(id)?)))
                    .collect::<Result<Vec<_>>>()?;
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
        Ok(CachedPackage {
            storage: self.storage.clone(),
            references: Mutex::default(),
            package,
            dir,
            manifest,
            books: Mutex::default(),
        })
    }

    /// The Bible references in a package's EPUBs, found once per version (see
    /// [`crate::references::find`]), without reading its verse counts as [`open`](Self::open)
    /// does
    pub fn references(&self, package: &Package) -> Result<Vec<FoundReference>> {
        let checksum = package.checksum()?;
        let dir = version_dir(package, &checksum);
        references(self.storage.as_ref(), package, &dir, &checksum)
    }
}

/// A package with its cache: verse counts at hand, and books parsed once
pub struct CachedPackage {
    storage: Arc<dyn Storage>,
    package: Package,
    dir: String,
    manifest: CachedManifest,
    books: Mutex<HashMap<BookId, Arc<Book>>>,
    references: Mutex<Option<Arc<Vec<FoundReference>>>>,
}

impl CachedPackage {
    pub fn package(&self) -> &Package {
        &self.package
    }

    pub fn manifest(&self) -> &CachedManifest {
        &self.manifest
    }

    /// A book: from memory, else the cache, else its source (then cached)
    pub fn book(&self, id: BookId) -> Result<Arc<Book>> {
        if let Some(book) = self.books.lock().ok().and_then(|b| b.get(&id).cloned()) {
            return Ok(book);
        }
        let code = crate::bible_data()
            .books()
            .get_usfm(id)
            .cloned()
            .unwrap_or_else(|| id.0.to_string());
        let path = format!("{}/books/{code}.postcard", self.dir);
        let header = book_header();
        let cached = (self.storage.read(&path).ok()).and_then(|bytes| {
            let body = bytes.strip_prefix(header.as_bytes())?;
            postcard::from_bytes::<Book>(body).ok()
        });
        let book = match cached {
            Some(book) => book,
            None => {
                let book = self.package.source().book(id)?;
                if let Ok(body) = postcard::to_stdvec(&book) {
                    let bytes = [header.as_bytes(), &body].concat();
                    let _ = self.storage.write(&path, &bytes);
                }
                book
            }
        };
        let book = Arc::new(book);
        if let Ok(mut books) = self.books.lock() {
            books.insert(id, book.clone());
        }
        Ok(book)
    }

    /// The Bible references in its EPUBs: from memory, else the cache, else found by reading
    /// them (then cached)
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
        self.add_as(translations, &info.abbreviation, &info.aliases)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Features, MemoryStorage};

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
        let book = (Cache::new(cache_storage.clone()).open(open()))
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
        let again = (Cache::new(cache_storage.clone()).open(open()))
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
        let usx = cache.open(usx).unwrap();
        usx.book(john).unwrap();
        let written = cache_storage.walk("").unwrap();
        let usfm = cache.open(usfm).unwrap();
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
        assert_eq!(cache.open(usx_again).unwrap().manifest(), usx.manifest());
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
        let manifest = cache.open(open()).unwrap().manifest().clone();
        let path = (cache_storage.walk("").unwrap().into_iter())
            .find(|f| f.ends_with("/manifest.postcard"))
            .unwrap();
        let stale = CachedManifest {
            engine: String::from("0.0.1"),
            books: vec![],
            ..manifest.clone()
        };
        (cache_storage.write(&path, &postcard::to_stdvec(&stale).unwrap())).unwrap();
        assert_eq!(cache.open(open()).unwrap().manifest(), &manifest);
    }
}
