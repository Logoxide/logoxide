/*!
A package's text in one format, read a book at a time into topos-bible-markup's tree, the same
whatever the format: USX and USFM (a book a file), OSIS and ThML (a whole Bible in one file, or a
book a file).
*/

mod osis;
mod thml;
mod usfm;
mod usx;
mod whole;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use topos_bible::data::books::BookId;
use topos_bible_markup::{
    Node,
    verses::{VerseSpan, chapter_verses},
};

pub use osis::OsisSource;
pub use thml::ThmlSource;
pub use usfm::{UsfmSource, usfm_to_usx};
pub use usx::UsxSource;

use crate::{error::Result, manifest::Format};

/// A book's text: its tree, and where each verse is in it
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Book {
    pub id: BookId,
    pub nodes: Vec<Node>,
    pub spans: Vec<VerseSpan>,
}

impl Book {
    /// The number of verses in each chapter, as the book's markers number them
    pub fn chapter_verses(&self) -> Vec<u8> {
        chapter_verses(&self.spans)
    }
}

pub trait Source: Send + Sync {
    /// The format read, if any (a book or a collection has no verse text: [`NoText`])
    fn format(&self) -> Option<Format>;

    /// The books it has, in order, and the file each is in (several books can share one)
    fn files(&self) -> &BTreeMap<BookId, String>;

    fn book(&self, id: BookId) -> Result<Book>;

    /// The number of verses in each chapter (sources can count without building the tree)
    fn chapter_verses(&self, id: BookId) -> Result<Vec<u8>> {
        Ok(self.book(id)?.chapter_verses())
    }

    fn books(&self) -> Vec<BookId> {
        self.files().keys().copied().collect()
    }

    /// The files, or books in a file, passed over, and why (peripherals, books topos doesn't
    /// know)
    fn skipped(&self) -> &[Skipped] {
        &[]
    }
}

/// Why a file or a book in one was passed over
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// Front or back matter, a glossary, an index, and the like ([`PERIPHERALS`]): not a book of
    /// the Bible, so not read as one
    Peripheral,
    /// A book topos doesn't know (yet): warned of, since its verses are left out
    Unknown,
    /// A file with the format's extension that names no book, by its name or its text
    NoBook,
}

/// A file, or a book in one, that a source passed over
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    /// The book's code or name as the file gives it (`FRT`, `JUB`, OSIS's `1En`); empty for
    /// [`SkipReason::NoBook`]
    pub book: String,
    pub reason: SkipReason,
}

impl Skipped {
    /// Whether it's worth a warning (a book whose verses are left out, or a file naming none),
    /// not only a note
    pub fn is_warning(&self) -> bool {
        self.reason != SkipReason::Peripheral
    }
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (path, book) = (&self.path, &self.book);
        match self.reason {
            SkipReason::Peripheral => {
                write!(f, "{path}: {book} isn't a book of the Bible, passed over")
            }
            SkipReason::Unknown => {
                write!(
                    f,
                    "{path}: {book} is a book logoxide doesn't know, passed over"
                )
            }
            SkipReason::NoBook => write!(f, "{path}: names no book, passed over"),
        }
    }
}

/// Paratext's codes for books that aren't books of the Bible: front and back matter,
/// introductions, concordance, glossary, indexes, other matter, and the extra books `XXA` to
/// `XXG`
pub const PERIPHERALS: [&str; 15] = [
    "FRT", "INT", "BAK", "CNC", "GLO", "TDX", "NDX", "OTH", "XXA", "XXB", "XXC", "XXD", "XXE",
    "XXF", "XXG",
];

/// Whether a USFM code names a peripheral ([`PERIPHERALS`])
pub fn is_peripheral(code: &str) -> bool {
    PERIPHERALS.iter().any(|p| p.eq_ignore_ascii_case(code))
}

/// A book found in a second file: an error naming both
pub(crate) fn duplicate(id: BookId, first: &str, second: &str) -> crate::error::LogoxideError {
    crate::error::LogoxideError::Book {
        book: id.0.to_string(),
        message: format!("in two files, {first} and {second}: keep one"),
    }
}

/**
A package with no verse text to read: a book (an EPUB, whose references topos finds), or a
collection (only dependencies). It has no books, so its summary has sizes and a checksum but no
verses
*/
pub struct NoText {
    pub format: Option<Format>,
    files: BTreeMap<BookId, String>,
}

impl NoText {
    pub fn new(format: Option<Format>) -> NoText {
        NoText {
            format,
            files: BTreeMap::new(),
        }
    }
}

impl Source for NoText {
    fn format(&self) -> Option<Format> {
        self.format
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn book(&self, id: BookId) -> Result<Book> {
        Err(crate::error::LogoxideError::NotFound(format!(
            "book {}: this package has no verse text",
            id.0
        )))
    }
}

/// A book's USFM code from its file name, as publishers name them: `JHN.usx`, `jhn.usfm`,
/// `44JHNBSB.usx` (Paratext's), `43-JHN.usfm` and `73-JHNeng-asv.usfm` (eBible.org's), or a name with the code as a separate word
/// (`B04___01_John_JHN.usx`)
pub(crate) fn code_from_name(stem: &str) -> Option<BookId> {
    let books = crate::bible_data().books();
    let name = stem.to_ascii_uppercase();
    books
        .search_usfm(&name)
        .or_else(|| {
            // `44JHNBSB` (Paratext's), `73-JHNeng-asv` (eBible.org's)
            let digits = name.get(..2)?.bytes().all(|b| b.is_ascii_digit());
            let rest = name.get(2..)?;
            let rest = rest.strip_prefix('-').unwrap_or(rest);
            digits.then(|| books.search_usfm(rest.get(..3)?)).flatten()
        })
        .or_else(|| {
            name.split(|c: char| !c.is_ascii_alphanumeric())
                .find_map(|word| books.search_usfm(word))
        })
}

/// Each book's file in a folder, and the files passed over: files with one of the format's
/// `extensions`, named for their book, else by the code `code_in` finds at the start of the
/// file. Peripherals and books topos doesn't know are passed over; a book in two files is an
/// error naming both
pub(crate) fn book_files(
    storage: &dyn crate::storage::Storage,
    dir: &str,
    extensions: &[&str],
    code_in: impl Fn(&str) -> Option<String>,
) -> Result<(BTreeMap<BookId, String>, Vec<Skipped>)> {
    let mut files: BTreeMap<BookId, String> = BTreeMap::new();
    let mut skipped = vec![];
    let mut names = storage.list(dir)?;
    names.sort();
    for name in names {
        let Some(stem) = name
            .rsplit_once('.')
            .filter(|(_, ext)| extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)))
            .map(|(stem, _)| stem)
        else {
            continue;
        };
        let path = crate::storage::join(dir, &name);
        // Else the code the file gives (`\id FRT`), which may be one topos doesn't know
        let found = code_from_name(stem).ok_or_else(|| {
            let head = storage.read(&path).ok()?;
            let head = String::from_utf8_lossy(&head[..head.len().min(4096)]).into_owned();
            Some(code_in(&head)?.trim().to_ascii_uppercase())
        });
        let found = found.or_else(|code| {
            let id = code
                .as_deref()
                .and_then(|c| crate::bible_data().books().search_usfm(c));
            id.ok_or(code)
        });
        match found {
            Ok(id) => {
                if let Some(first) = files.get(&id) {
                    return Err(duplicate(id, first, &path));
                }
                files.insert(id, path);
            }
            Err(code) => {
                let reason = match code.as_deref() {
                    Some(c) if is_peripheral(c) => SkipReason::Peripheral,
                    Some(_) => SkipReason::Unknown,
                    None => SkipReason::NoBook,
                };
                let book = code.unwrap_or_default();
                skipped.push(Skipped { path, book, reason });
            }
        }
    }
    Ok((files, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;
    use std::sync::Arc;

    fn usfm(code: &str) -> String {
        format!("\\id {code}\n\\c 1\n\\p\n\\v 1 In the beginning.\n")
    }

    #[test]
    fn peripherals_and_unknown_books_are_passed_over() {
        let storage = MemoryStorage::with([
            ("usfm/01-GENeng.usfm", usfm("GEN")),
            ("usfm/00-FRTeng.usfm", usfm("FRT")),
            ("usfm/GLO.usfm", usfm("GLO")),
            ("usfm/XXB.usfm", usfm("XXB")),
            ("usfm/zzz.usfm", usfm("ZZZ")),
            ("usfm/notes.usfm", String::from("no id here")),
        ]);
        let source = UsfmSource::open(Arc::new(storage), "usfm").expect("opens");
        let genesis = crate::bible_data().books().search_usfm("GEN");
        assert_eq!(source.books(), genesis.into_iter().collect::<Vec<_>>());
        let skipped: Vec<(&str, &str, SkipReason)> = (source.skipped().iter())
            .map(|s| (s.path.as_str(), s.book.as_str(), s.reason))
            .collect();
        assert_eq!(
            skipped,
            [
                ("usfm/00-FRTeng.usfm", "FRT", SkipReason::Peripheral),
                ("usfm/GLO.usfm", "GLO", SkipReason::Peripheral),
                ("usfm/XXB.usfm", "XXB", SkipReason::Peripheral),
                ("usfm/notes.usfm", "", SkipReason::NoBook),
                ("usfm/zzz.usfm", "ZZZ", SkipReason::Unknown),
            ]
        );
        assert!(!source.skipped()[0].is_warning());
        assert!(source.skipped()[4].is_warning());
        assert_eq!(
            source.skipped()[4].to_string(),
            "usfm/zzz.usfm: ZZZ is a book logoxide doesn't know, passed over"
        );
    }

    /// Jubilees: topos doesn't know `JUB` yet, so it's passed over with a warning; once it does,
    /// it's read as a book. Either way the rest is read
    #[test]
    fn a_book_topos_may_not_know() {
        let storage = MemoryStorage::with([
            ("usfm/GEN.usfm", usfm("GEN")),
            ("usfm/JUB.usfm", usfm("JUB")),
        ]);
        let source = UsfmSource::open(Arc::new(storage), "usfm").expect("opens");
        match crate::bible_data().books().search_usfm("JUB") {
            Some(jub) => {
                assert!(source.books().contains(&jub));
                assert!(source.skipped().is_empty());
            }
            None => {
                assert_eq!(source.books().len(), 1);
                let skip = &source.skipped()[0];
                assert_eq!(
                    (skip.book.as_str(), skip.reason),
                    ("JUB", SkipReason::Unknown)
                );
                assert_eq!(skip.path, "usfm/JUB.usfm");
            }
        }
    }

    #[test]
    fn a_book_in_two_files_is_an_error() {
        let storage = MemoryStorage::with([
            ("usfm/GEN.usfm", usfm("GEN")),
            ("usfm/01-GENeng-kjv.usfm", usfm("GEN")),
        ]);
        let error = UsfmSource::open(Arc::new(storage), "usfm")
            .err()
            .expect("refused")
            .to_string();
        assert!(error.contains("usfm/01-GENeng-kjv.usfm"), "{error}");
        assert!(error.contains("usfm/GEN.usfm"), "{error}");
        let storage = MemoryStorage::with([
            ("usx/JHN.usx", "<usx><book code=\"JHN\"/></usx>"),
            ("usx/other.usx", "<usx><book code=\"JHN\"/></usx>"),
        ]);
        assert!(UsxSource::open(Arc::new(storage), "usx").is_err());
    }

    #[test]
    fn whole_files_pass_over_books_topos_does_not_know() {
        let osis = |books: &[&str]| {
            let verses: String = (books.iter())
                .map(|b| format!("<verse osisID=\"{b}.1.1\">x</verse>"))
                .collect();
            format!("<osis><osisText>{verses}</osisText></osis>")
        };
        let storage = MemoryStorage::with([
            ("osis/a.xml", osis(&["Gen", "Zzz"])),
            ("osis/b.xml", osis(&["Exod"])),
        ]);
        let source = OsisSource::open(Arc::new(storage), "osis").expect("opens");
        assert_eq!(source.books().len(), 2);
        let skip = &source.skipped()[0];
        assert_eq!(
            (skip.path.as_str(), skip.book.as_str(), skip.reason),
            ("osis/a.xml", "Zzz", SkipReason::Unknown)
        );
        // Genesis in both files
        let storage = MemoryStorage::with([
            ("osis/a.xml", osis(&["Gen"])),
            ("osis/b.xml", osis(&["Gen", "Exod"])),
        ]);
        let error = OsisSource::open(Arc::new(storage), "osis")
            .err()
            .expect("refused")
            .to_string();
        assert!(error.contains("osis/a.xml and osis/b.xml"), "{error}");
    }
}
