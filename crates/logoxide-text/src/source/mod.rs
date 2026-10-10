/*!
A package's text in one format, read a book at a time into logoxide-markup's tree, the same
whatever the format: USX and USFM (a book a file), OSIS, ThML, and GBF (a whole Bible in one
file, or a book a file). A commentary is read the same way, and its notes on a passage are the
verses covering it ([`Source::notes`]). Two formats hold no verses: a dictionary's entries by key
([`Source::entry`], [`DictionarySource`]) and cross-references by verse
([`Source::cross_references`], [`XrefSource`]); they open as sources too, with no books, so one
[`open`] gives one `Box<dyn Source>` for any format.
*/

mod dictionary;
mod gbf;
mod osis;
mod thml;
mod usfm;
mod usx;
mod whole;
mod xref;

use std::{collections::BTreeMap, sync::Arc};

use logoxide_markup::{
    Node,
    verses::{VerseSpan, chapter_verses, passage_ranges, slice},
};
use serde::{Deserialize, Serialize};
use topos_bible::{data::books::BookId, segments::Passage};

pub use dictionary::DictionarySource;
pub use gbf::GbfSource;
pub use osis::OsisSource;
pub use thml::ThmlSource;
pub use usfm::{UsfmSource, usfm_to_usx};
pub use usx::UsxSource;
pub use xref::XrefSource;

pub use crate::skipped::{PERIPHERALS, SkipReason, Skipped, is_peripheral};

use crate::{
    error::{Result, TextError},
    files::Files,
};

/// The formats read: those read as verses, best first (USX is the richest and the most exact),
/// then a dictionary's entries and cross-references, which hold no verses
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextFormat {
    Usx,
    Usfm,
    Osis,
    Thml,
    /// GBF, with its sync marks for books, chapters, and verses ([`GbfSource`])
    Gbf,
    /// A dictionary's entries by key (`entries/*.xml`, [`DictionarySource`])
    Dictionary,
    /// Cross-references by verse (`xref/*.tsv`, [`XrefSource`])
    Xref,
}

impl TextFormat {
    /// Its name as manifests give it (`usx`)
    pub fn name(self) -> &'static str {
        match self {
            TextFormat::Usx => "usx",
            TextFormat::Usfm => "usfm",
            TextFormat::Osis => "osis",
            TextFormat::Thml => "thml",
            TextFormat::Gbf => "gbf",
            TextFormat::Dictionary => "dictionary",
            TextFormat::Xref => "xref",
        }
    }

    pub fn from_name(name: &str) -> Option<TextFormat> {
        Some(match name.to_ascii_lowercase().as_str() {
            "usx" => TextFormat::Usx,
            "usfm" => TextFormat::Usfm,
            "osis" => TextFormat::Osis,
            "thml" => TextFormat::Thml,
            "gbf" => TextFormat::Gbf,
            "dictionary" => TextFormat::Dictionary,
            "xref" => TextFormat::Xref,
            _ => return None,
        })
    }

    /// Whether it's read as verses (a Bible's or a commentary's text), rather than as entries
    pub fn is_verse_text(self) -> bool {
        !matches!(self, TextFormat::Dictionary | TextFormat::Xref)
    }
}

/// The source reading `format`'s files in the folder `dir` of `files`
pub fn open(files: Arc<dyn Files>, format: TextFormat, dir: &str) -> Result<Box<dyn Source>> {
    let dir = dir.trim_end_matches('/');
    Ok(match format {
        TextFormat::Usx => Box::new(UsxSource::open(files, dir)?),
        TextFormat::Usfm => Box::new(UsfmSource::open(files, dir)?),
        TextFormat::Osis => Box::new(OsisSource::open(files, dir)?),
        TextFormat::Thml => Box::new(ThmlSource::open(files, dir)?),
        TextFormat::Gbf => Box::new(GbfSource::open(files, dir)?),
        TextFormat::Dictionary => Box::new(DictionarySource::open(files, dir)?),
        TextFormat::Xref => Box::new(XrefSource::open(files, dir)?),
    })
}

/// A note of a commentary: the verses one of its spans covers, and the tree of the note
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub chapter: u8,
    pub verse: u8,
    /// The chapter `last` is in: `chapter`, unless the note runs into a later one
    pub last_chapter: u8,
    /// The last verse the note covers (`verse` when it's on one)
    pub last: u8,
    /// Which part of the verse (1 for `20a`), 0 for the whole
    pub part: u8,
    pub nodes: Vec<Node>,
}

/// A dictionary's entry (or, for cross-references, a verse's targets as one text)
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its key in normal form (`G25`; [`crate::keys::normalize_key`])
    pub key: String,
    /// The word it's about, when the dictionary says (`ἀγαπάω`)
    pub headword: Option<String>,
    /// Its text, parsed by the markup its file declares (an alias's is its target's)
    pub nodes: Vec<Node>,
}

/// One cross-reference: from a passage to another, perhaps weighted
#[derive(Clone, Debug, PartialEq)]
pub struct CrossReference {
    /// The passage it's for, as an OSIS reference in normal form (`John.3.16`,
    /// `John.3.16-John.3.18`)
    pub from: String,
    /// The passage referred to, the same way
    pub to: String,
    /// How strong the link is, when the file says
    pub weight: Option<f64>,
}

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
    fn format(&self) -> Option<TextFormat>;

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

    /**
    A commentary's notes on a passage (in any verse source: a Bible's are its verses): each
    verse span covering any of the passage's verses, with its tree, in the book's order. A
    span across chapters is found by any verse it covers; a passage's verses the book lacks
    find nothing. [`book_notes`] with `stretch` off
    */
    fn notes(&self, passage: &Passage) -> Result<Vec<Note>> {
        Ok(book_notes(&self.book(passage.book)?, passage, false))
    }

    /// A dictionary's keys in normal form, in its files' order (for cross-references, the
    /// passages lines are for); empty for a verse source
    fn keys(&self) -> Vec<String> {
        vec![]
    }

    /// A dictionary's entry for a key (matched by its normal form, then loosely:
    /// [`crate::keys::fold`]); for cross-references, a passage's targets as one text (what a
    /// summary hashes); none in a verse source, or for a key it lacks
    fn entry(&self, _key: &str) -> Result<Option<Entry>> {
        Ok(None)
    }

    /// The cross-references for a verse (an OSIS reference, `John.3.16`): every line whose
    /// passage covers it, in file order; none in other sources
    fn cross_references(&self, _verse: &str) -> Result<Vec<CrossReference>> {
        Ok(vec![])
    }
}

/**
A commentary's notes on a passage in its book: each verse span covering any of the passage's
verses, in the book's order. With `stretch`, a note also covers the verses after it up to the
next note in its chapter: where no span covers the passage's first verse (or the first verse of
each of its ranges), the span before it in that chapter is taken too (a chapter's verse 0, its
introduction, is one). Off by default where a user can choose: a commentary that skips verses
on purpose would seem to speak of them
*/
pub fn book_notes(book: &Book, passage: &Passage, stretch: bool) -> Vec<Note> {
    let mut found: Vec<usize> = passage_ranges(&book.spans, passage)
        .into_iter()
        .flatten()
        .collect();
    if stretch {
        for range in passage.ranges(None) {
            let start = (range.start.chapter, range.start.verse);
            let covered = (book.spans.iter())
                .any(|s| (s.chapter, s.verse) <= start && (s.last_chapter, s.last) >= start);
            if covered {
                continue;
            }
            let before = (book.spans.iter().enumerate())
                .filter(|(_, s)| s.last_chapter == start.0 && (s.last_chapter, s.last) < start)
                .map(|(i, _)| i)
                .next_back();
            found.extend(before);
        }
        found.sort_unstable();
        found.dedup();
    }
    (found.into_iter())
        .filter_map(|i| book.spans.get(i))
        .map(|span| Note {
            chapter: span.chapter,
            verse: span.verse,
            last_chapter: span.last_chapter,
            last: span.last,
            part: span.part,
            nodes: slice(&book.nodes, &span.start, Some(&span.end)),
        })
        .collect()
}

/// A book found in a second file: an error naming both
pub(crate) fn duplicate(id: BookId, first: &str, second: &str) -> TextError {
    TextError::Book {
        book: id.0.to_string(),
        message: format!("in two files, {first} and {second}: keep one"),
    }
}

/**
No verse text to read: a book (an EPUB, whose references topos finds), or a collection (only
dependencies). It has no books
*/
#[derive(Default)]
pub struct NoText {
    files: BTreeMap<BookId, String>,
}

impl Source for NoText {
    fn format(&self) -> Option<TextFormat> {
        None
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn book(&self, id: BookId) -> Result<Book> {
        Err(TextError::NotFound(format!(
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
    storage: &dyn Files,
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
        let path = crate::files::join(dir, &name);
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
    use crate::files::MemoryFiles as MemoryStorage;

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

    /// Esther's lettered chapters (the Greek additions some Bibles number A to F) aren't read
    #[test]
    fn lettered_chapters_are_passed_over() {
        let osis = r#"<osis><osisText><div type="book" osisID="Esth"><chapter osisID="Esth.1"><verse osisID="Esth.1.1">In the days of Ahasuerus</verse><verse osisID="Esth.1.2">that</verse></chapter><chapter osisID="Esth.A"><verse osisID="Esth.A.1">In the second year</verse></chapter></div></osisText></osis>"#;
        let storage = MemoryStorage::with([("osis/esth.xml", osis)]);
        let source = OsisSource::open(Arc::new(storage), "osis").expect("opens");
        let esther = crate::bible_data()
            .books()
            .search_usfm("EST")
            .expect("Esther");
        assert_eq!(source.books(), [esther]);
        let skip = &source.skipped()[0];
        assert_eq!(
            (skip.path.as_str(), skip.book.as_str(), skip.reason),
            ("osis/esth.xml", "Esth", SkipReason::LetteredChapter)
        );
        assert!(skip.is_warning());
        let book = source.book(esther).expect("reads");
        let numbered: Vec<(u8, u8)> = (book.spans.iter()).map(|s| (s.chapter, s.verse)).collect();
        assert_eq!(numbered, [(1, 1), (1, 2)]);
        // In ThML, CCEL's markers
        let thml = r#"<ThML><ThML.body><div1><scripture parsed="|Esth|1|1|0|0"/>In the days <scripture parsed="|Esth|A|1|0|0"/>In the second year</div1></ThML.body></ThML>"#;
        let storage = MemoryStorage::with([("thml/esth.xml", thml)]);
        let source = ThmlSource::open(Arc::new(storage), "thml").expect("opens");
        assert_eq!(source.skipped()[0].reason, SkipReason::LetteredChapter);
        let package = osis::tests::package("thml", &[("esth.xml", thml)]);
        assert_eq!(
            osis::tests::verse_texts(&*package, esther),
            [(1, 1, String::from("In the days"))]
        );
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
