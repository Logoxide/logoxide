//! Searching the whole workspace: every text file under the workspace folders (respecting
//! `.gitignore`, `.ignore`, and `.toposignore`, and skipping hidden and binary files, like the
//! CLI), with open documents' unsaved text instead of what is on disk, and a cache of each
//! file's references.
//!
//! With the `epub` setting, EPUBs are searched too (and with `pdf`, PDFs, in a build with the
//! `pdf` feature), through
//! topos's cache on disk (the CLI's, so a book is read once whichever searched it first). Their
//! references are in `topos-file://` documents: the book's text ([`book_text`]), where each
//! reference is at its line and column.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime},
};

use ignore::WalkState;
use lsp_types::{Position, Range, Uri};
use topos_bible::{
    matcher::{BibleMatcher, LineIndex},
    segments::Passage,
};
use topos_bible_index::files::{self, CfiOptions, Hit, WalkOptions};

/// The scheme of a book's text: `topos-file:///home/me/Books/Gill.epub`
pub const BOOK_SCHEME: &str = "topos-file";

/// Searching books (the `epub` and `pdf` settings), with topos's cache on disk if it has one
pub struct BookSearch {
    pub disk: Option<files::Cache>,
    pub epub: bool,
    /// Only in a build with the `pdf` feature
    pub pdf: bool,
}

impl BookSearch {
    /// Searching EPUBs, PDFs, or both (`None` for neither), with topos's cache in its usual
    /// folder, for this `fingerprint` (see [`files::fingerprint`])
    pub fn new(epub: bool, pdf: bool, fingerprint: &str) -> Option<Self> {
        let pdf = pdf && cfg!(feature = "pdf");
        (epub || pdf).then(|| Self {
            disk: files::Cache::open(fingerprint),
            epub,
            pdf,
        })
    }

    /// Whether it searches this file
    pub fn searches(&self, path: &Path) -> bool {
        let extension = path.extension().and_then(|e| e.to_str());
        let is = |name: &str| extension.is_some_and(|e| e.eq_ignore_ascii_case(name));
        (self.epub && is("epub")) || (self.pdf && is("pdf"))
    }
}

/// Which files a workspace search reads
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    /// Text files with one of these extensions (any if empty)
    pub extensions: &'a [String],
    /// EPUBs or PDFs too, whatever `extensions` says (the `epub` and `pdf` settings)
    pub books: Option<&'a BookSearch>,
}

/// A book's document: `topos-file:///home/me/Books/Gill.epub`
pub fn book_uri(path: &Path) -> Option<Uri> {
    let file = path_to_uri(path)?;
    let rest = file.as_str().strip_prefix("file")?;
    Uri::from_str(&format!("{BOOK_SCHEME}{rest}")).ok()
}

/// The path of a book's document
pub fn book_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix(BOOK_SCHEME)?;
    uri_to_path(&Uri::from_str(&format!("file{rest}")).ok()?)
}

/// A book's text, as its references' lines and columns count it
pub fn book_text(path: &Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("epub") {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return topos_bible_formats::epub::epub_text_reader(std::io::BufReader::new(file))
            .map_err(|e| format!("{}: {e}", path.display()));
    }
    #[cfg(feature = "pdf")]
    if extension.eq_ignore_ascii_case("pdf") {
        return topos_bible_formats::pdf::pdf_file_text(path)
            .map_err(|e| format!("{}: {e}", path.display()));
    }
    Err(format!("{} isn't a book (an EPUB or PDF)", path.display()))
}

/// Where a book's reference is in its text, in LSP positions (UTF-16 columns, as topos counts
/// them)
fn book_found(hit: &Hit) -> Option<Found> {
    let (line, column, len) = match (&hit.epub, &hit.pdf) {
        (Some(e), _) => (e.line, e.utf16_column, e.text_utf16.len()),
        (_, Some(p)) => (p.line, p.utf16_column, p.text_utf16.len()),
        _ => return None,
    };
    let start = Position::new(
        line.saturating_sub(1) as u32,
        column.saturating_sub(1) as u32,
    );
    let end = Position::new(start.line, start.character + len as u32);
    Some(Found {
        range: Range::new(start, end),
        passage: hit.passage.clone(),
    })
}

/**
A book's references: from topos's cache on disk while the book is unchanged, else read (and
cached). A book that can't be read has none, and isn't cached, so it's tried again
*/
fn book_references(matcher: &BibleMatcher, disk: Option<&files::Cache>, path: &Path) -> Vec<Found> {
    let hits = match disk.and_then(|cache| cache.get(path, None)) {
        Some(files::Cached::Hits(hits)) => hits,
        _ => match files::search_file(matcher, path, false, CfiOptions::default()) {
            Ok(found) => {
                let hits = found.map(|f| f.hits).unwrap_or_default();
                if let Some(cache) = disk {
                    cache.insert(path, &hits);
                }
                hits
            }
            Err(_) => vec![],
        },
    };
    hits.iter().filter_map(book_found).collect()
}

/// Larger files are skipped (a book-length text is a few MB)
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// A `file://` URI for a path, percent-encoding what URIs can't hold (like spaces)
pub fn path_to_uri(path: &Path) -> Option<Uri> {
    let path = path.to_str()?;
    let mut encoded = String::from("file://");
    if !path.starts_with('/') {
        // Windows: file:///C:/...
        encoded.push('/');
    }
    for byte in path.replace('\\', "/").bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Uri::from_str(&encoded).ok()
}

/// The path of a `file://` URI
pub fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let text = uri.as_str().strip_prefix("file://")?;
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8(decoded).ok()?;
    // Windows: /C:/... is C:/...
    let path = match path.as_bytes() {
        [b'/', _, b':', ..] => path[1..].to_string(),
        _ => path,
    };
    Some(PathBuf::from(path))
}

/// A reference found in a file: where it is (in LSP positions) and what it is
#[derive(Clone, Debug)]
pub struct Found {
    pub range: Range,
    pub passage: Passage,
}

/// Each file's references, found once and reused while the file is unchanged
struct Entry {
    size: u64,
    modified: Option<SystemTime>,
    found: Arc<Vec<Found>>,
}

/**
A cache of every workspace file's references, before any filter (searches filter them), so a
search reads and parses only the files that changed since the last one
- Files are matched by size and modification time, like the CLI's `--cache`
- [`Cache::clear`] when the data changes, since that changes what is found
*/
#[derive(Default)]
pub struct Cache {
    files: Mutex<HashMap<PathBuf, Entry>>,
}

impl Cache {
    pub fn clear(&self) {
        if let Ok(mut files) = self.files.lock() {
            files.clear();
        }
    }

    fn get(&self, path: &Path, size: u64, modified: Option<SystemTime>) -> Option<Arc<Vec<Found>>> {
        let files = self.files.lock().ok()?;
        let entry = files.get(path)?;
        (entry.size == size && entry.modified == modified).then(|| entry.found.clone())
    }

    fn insert(
        &self,
        path: PathBuf,
        size: u64,
        modified: Option<SystemTime>,
        found: Arc<Vec<Found>>,
    ) {
        if let Ok(mut files) = self.files.lock() {
            files.insert(
                path,
                Entry {
                    size,
                    modified,
                    found,
                },
            );
        }
    }

    /// Forgets files that are gone (or no longer searched)
    fn retain(&self, seen: &HashSet<PathBuf>) {
        if let Ok(mut files) = self.files.lock() {
            files.retain(|path, _| seen.contains(path));
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.files.lock().map_or(0, |files| files.len())
    }
}

/// A file and its references
pub type FileReferences = (Uri, Arc<Vec<Found>>);

/// The references in a text, with LSP ranges
pub fn find(matcher: &BibleMatcher, text: &str) -> Vec<Found> {
    let index = LineIndex::new(text);
    matcher
        .search(text)
        .into_iter()
        .map(|m| {
            let bytes = m.location.bytes;
            Found {
                range: crate::server::lsp_range(&index, bytes.start, bytes.end),
                passage: m.psg,
            }
        })
        .collect()
}

/// Whether a file is one of the Bible's books (by its canonical folder, since either path may go
/// through a symlink)
fn in_bible(path: &Path, folder: &Path) -> bool {
    let usx = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("usx"));
    usx && path
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .is_some_and(|parent| parent == folder)
}

/**
Every reference in the workspace, by file: open documents as they are in the editor (unsaved
text included), then the files under `roots` that `scope` takes, from the cache when unchanged.
Files are walked in parallel
- With [`Scope::books`], EPUBs and PDFs too, as `topos-file://` documents
- The USX files in `bible` (the Bible's canonical folder) are left out
- At the `deadline`, it stops walking; the `bool` is whether it searched every file
*/
// `Uri` caches its parsed parts, which clippy counts as a mutable key; its text never changes
#[allow(clippy::mutable_key_type)]
pub fn references(
    roots: &[PathBuf],
    documents: &HashMap<Uri, String>,
    scope: Scope,
    matcher: &BibleMatcher,
    cache: &Cache,
    bibles: &[&Path],
    deadline: Option<Instant>,
) -> (Vec<FileReferences>, bool) {
    // Open documents by path: editors may percent-encode their URIs differently
    let open: HashSet<PathBuf> = documents.keys().filter_map(uri_to_path).collect();
    // Unnamed documents (`file://`) are left out: there is no file to go to
    let results: Mutex<Vec<(Uri, Arc<Vec<Found>>)>> = Mutex::new(
        documents
            .iter()
            .filter(|(uri, _)| !crate::server::is_unnamed(uri))
            .map(|(uri, text)| (uri.clone(), Arc::new(find(matcher, text))))
            .collect(),
    );
    let seen: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
    // The CLI's walking rules (ignore files, `.toposignore`, hidden files)
    let walk = WalkOptions {
        max_filesize: Some(MAX_FILE_SIZE),
        ..WalkOptions::default()
    };
    let Ok(builder) = walk.builder(roots) else {
        return (results.into_inner().unwrap_or_default(), true);
    };
    let Scope { extensions, books } = scope;
    let stopped = AtomicBool::new(false);
    builder.build_parallel().run(|| {
        Box::new(|entry| {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                stopped.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            let path = entry.path();
            let book = books.is_some_and(|b| b.searches(path));
            let wanted = entry.file_type().is_some_and(|t| t.is_file())
                && !open.contains(path)
                && !bibles.iter().any(|folder| in_bible(path, folder))
                && (book
                    || extensions.is_empty()
                    || path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e))));
            if !wanted {
                return WalkState::Continue;
            }
            let uri = match book {
                true => book_uri(path),
                false => path_to_uri(path),
            };
            let (Some(uri), Ok(meta)) = (uri, entry.metadata()) else {
                return WalkState::Continue;
            };
            if let Ok(mut seen) = seen.lock() {
                seen.insert(path.to_path_buf());
            }
            let (size, modified) = (meta.len(), meta.modified().ok());
            let found = match cache.get(path, size, modified) {
                Some(found) => found,
                None if book => {
                    let disk = books.and_then(|b| b.disk.as_ref());
                    let found = Arc::new(book_references(matcher, disk, path));
                    cache.insert(path.to_path_buf(), size, modified, found.clone());
                    found
                }
                None => {
                    let Ok(bytes) = std::fs::read(path) else {
                        return WalkState::Continue;
                    };
                    // Like ripgrep, skip files that look binary (cached as having none)
                    let found = if bytes[..bytes.len().min(8192)].contains(&0) {
                        Arc::new(vec![])
                    } else {
                        Arc::new(find(matcher, &String::from_utf8_lossy(&bytes)))
                    };
                    cache.insert(path.to_path_buf(), size, modified, found.clone());
                    found
                }
            };
            if !found.is_empty()
                && let Ok(mut results) = results.lock()
            {
                results.push((uri, found));
            }
            WalkState::Continue
        })
    });
    let complete = !stopped.into_inner();
    // Files not reached yet may still be there
    if complete {
        cache.retain(&seen.into_inner().unwrap_or_default());
        if let Some(disk) = books.and_then(|b| b.disk.as_ref()) {
            // Now and then, entries for books that are gone
            disk.finish();
        }
    }
    (results.into_inner().unwrap_or_default(), complete)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        let path = Path::new("/home/me/My Notes/Romans 8 (2025)#1.md");
        let uri = path_to_uri(path).unwrap();
        assert_eq!(
            uri.as_str(),
            "file:///home/me/My%20Notes/Romans%208%20%282025%29%231.md"
        );
        assert_eq!(uri_to_path(&uri).unwrap(), path);
        let book = Path::new("/home/me/My Books/Gill.epub");
        let uri = book_uri(book).unwrap();
        assert_eq!(uri.as_str(), "topos-file:///home/me/My%20Books/Gill.epub");
        assert_eq!(book_path(uri.as_str()).unwrap(), book);
        assert!(book_path("file:///home/me/a.epub").is_none());
    }
}
