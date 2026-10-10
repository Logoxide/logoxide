/*!
The Bible references in a book, found by topos in each EPUB: every reference, with where it is
(an EPUB CFI readers open straight to, its place in the book's text, its chapter, its paragraph).
About a tenth of a second a book natively, a quarter in WebAssembly.
*/

use serde::{Deserialize, Serialize};
use topos_bible::matcher::{BibleMatcher, matches::ComplexFilter};
use topos_bible_formats::epub::{CfiOptions, search_epub_reader};

use crate::{
    Files,
    error::{Result, TextError},
};

/// A reference found in a book
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// The passage, as an OSIS reference (`John.3.16`, `John.3.16-John.3.18`)
    pub passage: String,
    /// The file it's in (`epub/the-dorean-principle.epub`)
    pub file: String,
    /// Where it is in the file: an EPUB CFI range
    pub cfi: String,
    /// The chapter it's in, when the book's table of contents names one
    pub chapter: Option<String>,
    /// Its UTF-16 range in the book's text (each content document's text, a blank line apart,
    /// as topos's index and EPUB++ count it)
    pub start: u32,
    pub end: u32,
    /// The 1-based line of the book's text it starts on, and its 1-based UTF-16 column there
    pub line: u32,
    pub column: u32,
    /// Its content document's place in the spine
    pub spine: u32,
    /// The line (paragraph) it starts in
    pub line_text: String,
}

/// Every reference in the EPUBs at `epubs`, file by file in the order given, each file's in the
/// order they're in it
pub fn find(files: &dyn Files, epubs: &[String]) -> Result<Vec<Citation>> {
    if epubs.is_empty() {
        return Ok(vec![]);
    }
    let data = crate::bible_data();
    let matcher = BibleMatcher::new(data.clone(), ComplexFilter::default());
    let mut found = vec![];
    for file in epubs {
        let bytes = files.read(file)?;
        let matches =
            search_epub_reader(&matcher, std::io::Cursor::new(bytes), CfiOptions::default())
                .map_err(|e| TextError::Book {
                    book: file.clone(),
                    message: e.to_string(),
                })?;
        for m in matches {
            let Some(passage) = m.psg.to_osis(data.books()) else {
                continue;
            };
            let at = m.location;
            let whole = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            found.push(Citation {
                passage,
                file: file.clone(),
                cfi: at.cfi,
                chapter: at.chapter,
                start: whole(at.text_utf16.start),
                end: whole(at.text_utf16.end),
                line: whole(at.line),
                column: whole(at.utf16_column),
                spine: whole(at.spine_index),
                line_text: at.line_text,
            });
        }
    }
    Ok(found)
}
