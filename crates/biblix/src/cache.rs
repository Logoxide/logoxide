/*!
Parsed books and verse counts, kept per package version so a book loads without parsing its
source again. A cache is per device and never synced: it can always be built again.

```text
<cache>/<id>/<version>-<checksum>/
  manifest.postcard     the verse counts of every book, and what the cache was built from
  books/<CODE>.postcard each book's tree and verse spans, written when first read
```
*/

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use topos_bible::data::{books::BookId, chapter_verses::ChapterVerses, translations::Translations};

use crate::{
    error::{BiblixError, Result},
    package::Package,
    source::Book,
    storage::Storage,
};

/// Bumped when what the cache holds changes shape, so older caches are built again
pub const CACHE_FORMAT: u32 = 1;

/// What a package version's cache was built from, and its verse counts
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CachedManifest {
    pub cache_format: u32,
    pub id: String,
    pub version: String,
    pub checksum: String,
    /// The format the text was read from (`usx`, `usfm`)
    pub source_format: String,
    /// Each book and the number of verses in each of its chapters
    pub books: Vec<(BookId, Vec<u8>)>,
}

pub struct Cache {
    storage: Arc<dyn Storage>,
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
        let short = checksum
            .trim_start_matches("sha256:")
            .get(..16)
            .unwrap_or_default();
        let dir = format!("{}/{}-{short}", info.id, info.version);
        let path = format!("{dir}/manifest.postcard");
        let cached = self
            .storage
            .read(&path)
            .ok()
            .and_then(|bytes| postcard::from_bytes::<CachedManifest>(&bytes).ok())
            .filter(|m| m.cache_format == CACHE_FORMAT && m.checksum == checksum);
        let manifest = match cached {
            Some(manifest) => manifest,
            None => {
                let source = package.source();
                let books = (source.books().into_iter())
                    .map(|id| Ok((id, source.chapter_verses(id)?)))
                    .collect::<Result<Vec<_>>>()?;
                let manifest = CachedManifest {
                    cache_format: CACHE_FORMAT,
                    id: info.id.clone(),
                    version: info.version.to_string(),
                    checksum,
                    source_format: format!("{:?}", source.format()).to_lowercase(),
                    books,
                };
                let bytes = postcard::to_stdvec(&manifest)
                    .map_err(|e| BiblixError::Cache(e.to_string()))?;
                // A cache that can't be written is still a cache for this run
                let _ = self.storage.write(&path, &bytes);
                manifest
            }
        };
        Ok(CachedPackage {
            storage: self.storage.clone(),
            package,
            dir,
            manifest,
            books: Mutex::default(),
        })
    }
}

/// A package with its cache: verse counts at hand, and books parsed once
pub struct CachedPackage {
    storage: Arc<dyn Storage>,
    package: Package,
    dir: String,
    manifest: CachedManifest,
    books: Mutex<HashMap<BookId, Arc<Book>>>,
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
        let cached = (self.storage.read(&path).ok())
            .and_then(|bytes| postcard::from_bytes::<Book>(&bytes).ok());
        let book = match cached {
            Some(book) => book,
            None => {
                let book = self.package.source().book(id)?;
                if let Ok(bytes) = postcard::to_stdvec(&book) {
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

    /// `translations` with this package added: its abbreviation, its aliases, and each book's
    /// verse counts (references naming it are checked against them)
    pub fn add_to(&self, translations: Translations) -> Translations {
        let info = self.package.info();
        let name = info.abbreviation.clone();
        let mut translations = (info.aliases.iter()).fold(
            translations.alias(name.clone(), name.clone()),
            |t, alias| t.alias(alias.clone(), name.clone()),
        );
        for (book, counts) in &self.manifest.books {
            translations.set_chapter_verses(&name, *book, ChapterVerses::new(counts.clone()));
        }
        translations
    }
}
