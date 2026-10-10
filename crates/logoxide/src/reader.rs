/*!
What the package manager asks of a package's text, and nothing more: the facts a summary
records ([`TextFacts`]), each book's verse counts, and whether its media timings fit. It never
reads the text itself: a [`TextReader`] does, given by whoever runs it (lx, the bindings, the
language server), so the package manager builds without a parser (`--no-default-features`)
and is tested with facts made up. The `read` feature's [`DefaultReader`] is the reader built
in here for now (it reads through [`crate::source`]); it will move beside the binaries.

The summary file is the contract between the two sides: a reader makes the facts, and the
package manager diffs and versions them ([`crate::summary`]).
*/

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use topos_bible::data::books::BookId;

use crate::{
    error::Result,
    package::Package,
    references::Cited,
    summary::{HashRule, VerseHash},
};

/// What a package's text gives its summary: the half a reader makes (the package manager adds
/// checksums, sizes, media, and the version)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextFacts {
    /// Each book's verse counts, by USFM code, in the text's order
    pub books: IndexMap<String, Vec<u8>>,
    /// How many of each kind of element (`note`, `title`, `words-of-jesus`, ...), and `strongs`
    pub contents: BTreeMap<String, u64>,
    /// Every verse and the hash of its words, made by the rule asked for
    pub verses: Vec<VerseHash>,
    /// Each file's title, by path, for the files that have one
    pub titles: BTreeMap<String, String>,
    /// The files that should have a title and don't (a release refuses them)
    pub untitled: Vec<String>,
    /// The passages its books (EPUBs) cite, each once with how often
    pub cited: Vec<Cited>,
    /// How many Bible references its books have
    pub references: u64,
    /// For a versification map, how many verses it maps
    pub mappings: u64,
    /// Files, or books in them, that were passed over, and why
    pub skipped: Vec<Skipped>,
}

/// Reads a package's text for the package manager. Every method takes the package's files and
/// manifest ([`Package`]) and gives back facts; nothing parsed crosses the boundary
pub trait TextReader: Send + Sync {
    /// Each book's verse counts, in the text's order (cheaper than [`facts`](Self::facts): no
    /// hashing, no books read for references)
    fn counts(&self, package: &Package) -> Result<Vec<(BookId, Vec<u8>)>>;

    /// Everything a summary records of the text, its verses hashed by `rule`
    fn facts(&self, package: &Package, rule: HashRule) -> Result<TextFacts>;

    /// What's wrong with the package's media timings against its text, or against `text`, the
    /// package `[media] text` names, as installed (see
    /// [`crate::media::check_package_timings_with`]); empty when all is well
    fn timing_problems(&self, package: &Package, text: Option<&Package>) -> Result<Vec<String>>;
}

/// A reader that reads nothing: a package has no text to it. For tests of the package manager
/// with facts made up, and hosts that only install
#[derive(Clone, Copy, Debug, Default)]
pub struct NoReader;

impl TextReader for NoReader {
    fn counts(&self, _: &Package) -> Result<Vec<(BookId, Vec<u8>)>> {
        Ok(vec![])
    }

    fn facts(&self, _: &Package, _: HashRule) -> Result<TextFacts> {
        Ok(TextFacts::default())
    }

    fn timing_problems(&self, _: &Package, _: Option<&Package>) -> Result<Vec<String>> {
        Ok(vec![])
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
    /// Verses in a lettered chapter (Esther's Greek additions, `Esth.A.1`), which the format
    /// hasn't: chapters are numbers
    LetteredChapter,
}

/// A file, or a book in one, that a reader passed over
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
            SkipReason::LetteredChapter => write!(
                f,
                "{path}: {book} has lettered chapters (A to F), which logoxide doesn't read, passed over"
            ),
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

/// The reader built in with the `read` feature: it reads through [`crate::source`], hashes
/// verses as [`crate::summary`] says, titles files as [`crate::titles`] does, and finds an
/// EPUB's references with topos
#[cfg(feature = "read")]
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultReader;

#[cfg(feature = "read")]
impl TextReader for DefaultReader {
    fn counts(&self, package: &Package) -> Result<Vec<(BookId, Vec<u8>)>> {
        let source = package.source();
        (source.books().into_iter())
            .map(|id| Ok((id, source.chapter_verses(id)?)))
            .collect()
    }

    fn facts(&self, package: &Package, rule: HashRule) -> Result<TextFacts> {
        let source = package.source();
        let mut facts = TextFacts::default();
        for id in source.books() {
            let book = source.book(id)?;
            let code = crate::bible_data()
                .books()
                .get_usfm(id)
                .cloned()
                .unwrap_or_else(|| id.0.to_string());
            facts.books.insert(code, book.chapter_verses());
            crate::summary::count_elements(&book.nodes, &mut facts.contents);
            facts
                .verses
                .extend(crate::summary::book_hashes_by(&book, rule));
        }
        for titled in crate::titles::titles(package)? {
            match titled.title {
                Some(title) => {
                    facts.titles.insert(titled.path, title);
                }
                None => facts.untitled.push(titled.path),
            }
        }
        let found = crate::references::find(package)?;
        facts.references = found.len() as u64;
        facts.cited = crate::references::cited(&found);
        if package.info().kind == crate::manifest::PackageKind::VersificationMap {
            facts.mappings =
                crate::versification::VersificationMap::from_package(package)?.len() as u64;
        }
        facts.skipped = source.skipped().to_vec();
        Ok(facts)
    }

    fn timing_problems(&self, package: &Package, text: Option<&Package>) -> Result<Vec<String>> {
        crate::media::check_package_timings_with(package, text)
    }
}
