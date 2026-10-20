/*!
Parsed books kept between runs, so a book loads without parsing its source again. A cache is
per device and never synced: it can always be built again from the text.

A [`BookCache`] is for one text in one format, named by a key the caller makes from the files'
checksum and the format read (`<checksum>-usfm`), so a changed file, or the same files read in
another format (which can number verses differently), never gives these books. Under its
[`BookStore`] it writes one file a book:

```text
<key>/<CODE>.postcard   the book's tree and verse spans, after a line naming the cache format
                        and the logoxide-text that wrote it
```

Postcard isn't self-describing, so a book written by another version (whose logoxide-markup may
lay its trees out otherwise) could decode into the wrong tree: a file whose first line isn't
this version's [`book_header`] is parsed again and written over, never misread. Each cache also
keeps the books it has given in memory, so one is parsed once per process whichever caller asks.

The store is the caller's: the package manager adapts its `Storage`, a host collects the writes
to save them, tests use [`MemoryBookStore`]. Nothing here touches `std::fs`.
*/

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use topos_bible::data::books::BookId;

use crate::{
    Book, Source,
    error::{Result, TextError},
};

/// Bumped when a cached book's layout changes, so older caches are parsed again
pub const CACHE_FORMAT: u32 = 4;

/// Where cached books are kept: a folder, memory, or a host's files, written as books are read
pub trait BookStore: Send + Sync {
    /// A file's bytes, if it's there (anything that can't be read is parsed again)
    fn read(&self, path: &str) -> Option<Vec<u8>>;

    /// Keep a file's bytes (a store that can't is still a cache for this run)
    fn write(&self, path: &str, bytes: &[u8]) -> Result<()>;
}

/// A store in memory: a test's, or a host's that reads what was written to save it
#[derive(Debug, Default)]
pub struct MemoryBookStore {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemoryBookStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every file written, by path
    pub fn paths(&self) -> Vec<String> {
        (self.files.lock().ok()).map_or_else(Vec::new, |files| files.keys().cloned().collect())
    }
}

impl BookStore for MemoryBookStore {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.lock().ok()?.get(path).cloned()
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let mut files = (self.files.lock())
            .map_err(|_| TextError::Files(String::from("the book store's lock is poisoned")))?;
        files.insert(path.to_string(), bytes.to_vec());
        Ok(())
    }
}

/// What a cached book's file starts with: the cache format and the logoxide-text that wrote it
pub fn book_header() -> String {
    format!("logoxide-book {}\n", text_version())
}

/**
What text read from a file depends on: the cache format and this crate's version (logoxide-markup
is released with it). A host keeping what it read (a book's verses, its parsed tree) keys it by
this, so a release that reads files differently is read again
*/
pub fn text_version() -> String {
    format!("{CACHE_FORMAT} {}", env!("CARGO_PKG_VERSION"))
}

/// A book's file under its key: `<key>/JHN.postcard` (a book without a USFM code, by its number)
pub fn book_path(key: &str, id: BookId) -> String {
    let code =
        (crate::bible_data().books().get_usfm(id).cloned()).unwrap_or_else(|| id.0.to_string());
    format!("{key}/{code}.postcard")
}

/// A book as its file holds it: the header, then postcard
pub fn encode(book: &Book) -> Option<Vec<u8>> {
    let body = postcard::to_stdvec(book).ok()?;
    Some([book_header().as_bytes(), &body].concat())
}

/// The book in a file, if this version wrote it and it's the book asked for
pub fn decode(bytes: &[u8], id: BookId) -> Option<Book> {
    let body = bytes.strip_prefix(book_header().as_bytes())?;
    let book = postcard::from_bytes::<Book>(body).ok()?;
    (book.id == id).then_some(book)
}

/// One text's parsed books: from memory, else the store, else parsed and written
pub struct BookCache {
    store: Arc<dyn BookStore>,
    key: String,
    books: Mutex<HashMap<BookId, Arc<Book>>>,
}

impl BookCache {
    /// The books of the text `key` names (`<checksum>-<format>`: see the module's doc), under
    /// `store`
    pub fn new(store: Arc<dyn BookStore>, key: impl Into<String>) -> Self {
        Self {
            store,
            key: key.into(),
            books: Mutex::default(),
        }
    }

    /// What the text is keyed by
    pub fn key(&self) -> &str {
        &self.key
    }

    /// A book: from memory, else the store, else parsed by `source` (then written)
    pub fn book(&self, source: &dyn Source, id: BookId) -> Result<Arc<Book>> {
        if let Some(book) = self.books.lock().ok().and_then(|b| b.get(&id).cloned()) {
            return Ok(book);
        }
        let path = book_path(&self.key, id);
        let book = match self.store.read(&path).and_then(|bytes| decode(&bytes, id)) {
            Some(book) => book,
            None => {
                let book = source.book(id)?;
                if let Some(bytes) = encode(&book) {
                    let _ = self.store.write(&path, &bytes);
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
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::{MemoryFiles, source::UsfmSource};

    /// A source that counts how often it parses
    struct Counting {
        source: UsfmSource,
        parses: AtomicUsize,
    }

    impl Source for Counting {
        fn format(&self) -> Option<crate::TextFormat> {
            self.source.format()
        }

        fn files(&self) -> &BTreeMap<BookId, String> {
            self.source.files()
        }

        fn book(&self, id: BookId) -> Result<Book> {
            self.parses.fetch_add(1, Ordering::SeqCst);
            self.source.book(id)
        }
    }

    fn john() -> Counting {
        let files = MemoryFiles::with([(
            "usfm/JHN.usfm",
            "\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n",
        )]);
        Counting {
            source: UsfmSource::open(Arc::new(files), "usfm").unwrap(),
            parses: AtomicUsize::new(0),
        }
    }

    const JOHN: BookId = BookId(43);

    #[test]
    fn a_book_is_parsed_once_and_read_from_the_store_next_time() {
        let store = Arc::new(MemoryBookStore::new());
        let source = john();
        let cache = BookCache::new(store.clone(), "abc-usfm");
        let book = cache.book(&source, JOHN).unwrap();
        assert_eq!(book.spans.len(), 1);
        cache.book(&source, JOHN).unwrap();
        assert_eq!(source.parses.load(Ordering::SeqCst), 1, "kept in memory");
        assert_eq!(store.paths(), vec!["abc-usfm/JHN.postcard"]);
        assert!(
            store
                .read("abc-usfm/JHN.postcard")
                .unwrap()
                .starts_with(book_header().as_bytes())
        );
        // Another cache over the same store (another process) reads the file, parsing nothing
        let again = BookCache::new(store.clone(), "abc-usfm");
        assert_eq!(*again.book(&source, JOHN).unwrap(), *book);
        assert_eq!(source.parses.load(Ordering::SeqCst), 1);
        // Another key is another text
        BookCache::new(store.clone(), "def-usfm")
            .book(&source, JOHN)
            .unwrap();
        assert_eq!(source.parses.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_file_from_another_version_is_parsed_again_and_written_over() {
        let store = Arc::new(MemoryBookStore::new());
        let source = john();
        let path = "k/JHN.postcard";
        let book = BookCache::new(store.clone(), "k")
            .book(&source, JOHN)
            .unwrap();
        let written = store.read(path).unwrap();
        // No header (an older writer's, perhaps laid out otherwise)
        let stale = Book {
            id: book.id,
            nodes: book.nodes.clone(),
            spans: vec![],
        };
        store
            .write(path, &postcard::to_stdvec(&stale).unwrap())
            .unwrap();
        let again = BookCache::new(store.clone(), "k")
            .book(&source, JOHN)
            .unwrap();
        assert_eq!(*again, *book);
        assert_eq!(store.read(path).unwrap(), written);
        assert_eq!(source.parses.load(Ordering::SeqCst), 2);
        // Another version's header
        let other = [
            b"logoxide-book 4 0.0.1\n".as_slice(),
            &written[book_header().len()..],
        ]
        .concat();
        store.write(path, &other).unwrap();
        BookCache::new(store.clone(), "k")
            .book(&source, JOHN)
            .unwrap();
        assert_eq!(source.parses.load(Ordering::SeqCst), 3);
        assert_eq!(store.read(path).unwrap(), written);
        // The wrong book under the name
        let wrong = Book {
            id: BookId(42),
            ..(*book).clone()
        };
        store.write(path, &encode(&wrong).unwrap()).unwrap();
        assert_eq!(
            *BookCache::new(store.clone(), "k")
                .book(&source, JOHN)
                .unwrap(),
            *book
        );
        assert_eq!(source.parses.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn books_are_named_by_their_codes() {
        assert_eq!(book_path("k", JOHN), "k/JHN.postcard");
        assert_eq!(book_path("k", BookId(250)), "k/250.postcard");
    }
}
