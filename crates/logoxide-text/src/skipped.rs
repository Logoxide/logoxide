//! What a source passed over, and why: a file naming no book, a peripheral, a book topos
//! doesn't know, a lettered chapter, and among loose files, a file of no format or a book read
//! from another.

use serde::{Deserialize, Serialize};

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
    /// A file in no format read as a Bible's text, among loose files ([`crate::source::open_loose`])
    NotText,
    /// A file, or a book in one, that another file has (in a better format, among loose files):
    /// that one is read
    Duplicate,
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
        !matches!(self.reason, SkipReason::Peripheral | SkipReason::Duplicate)
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
            SkipReason::NotText => write!(
                f,
                "{path}: not a Bible's text (USX, USFM, USJ, OSIS, ThML, or GBF), passed over"
            ),
            SkipReason::Duplicate if book.is_empty() => {
                write!(
                    f,
                    "{path}: another file of the same name is read, passed over"
                )
            }
            SkipReason::Duplicate => {
                write!(f, "{path}: {book} is read from another file, passed over")
            }
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
