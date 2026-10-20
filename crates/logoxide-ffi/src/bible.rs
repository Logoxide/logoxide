//! A Bible from loose files, with no manifest: what a reader dropped in, or a publisher zipped
//! (logoxide-text's `source::open_loose`).

use boltffi::*;
use logoxide_core::topos_bible::data::books::BookId;
use logoxide_text::{
    MemoryFiles, Source,
    source::{LooseSource, open_loose},
};

use crate::{BookFile, LogoxideBook, LogoxideError, PackageFile, SkippedFile, skipped_file};

/**
A Bible read from loose files ([`crate::Logoxide::open_bible`]): each file's format found from
its name and how it starts, `.zip`s opened, whole-Bible OSIS and ThML files cut into their books,
and each book read from its best format (USX, USFM, USJ, OSIS, ThML, GBF)

```ts
const bible = logoxide.openBible([{ path: "JHN.xml", bytes }, { path: "kjv.zip", bytes: zip }]);
bible.books(); // [43, ...]
bible.book(43).verseTexts();
bible.skipped(); // the files passed over, and why
```
*/
pub struct LogoxideBible {
    source: LooseSource,
}

impl LogoxideBible {
    pub(crate) fn open(files: Vec<PackageFile>) -> Result<Self, LogoxideError> {
        let files = MemoryFiles::with(files.into_iter().map(|f| (f.path, f.bytes)));
        Ok(Self {
            source: open_loose(&files, "")?,
        })
    }
}

#[export]
impl LogoxideBible {
    /// Its books, in order
    pub fn books(&self) -> Vec<u8> {
        self.source.books().into_iter().map(|id| id.0).collect()
    }

    /// A book, parsed (a `NotFound` error for one it hasn't)
    pub fn book(&self, book_id: u8) -> Result<LogoxideBook, LogoxideError> {
        let book = self.source.book(BookId(book_id))?;
        Ok(LogoxideBook::from_tree(
            book.nodes,
            book.spans,
            Some(book_id),
        ))
    }

    /// Each book's file, as it was given (in a `.zip`, the zip's path and the file's in it:
    /// `kjv.zip/usfm/JHN.usfm`)
    pub fn book_files(&self) -> Vec<BookFile> {
        (self.source.files().iter())
            .map(|(id, path)| BookFile {
                book_id: id.0,
                path: self.source.given_path(path),
                title: None,
            })
            .collect()
    }

    /// The format a book is read from (`usx`, `usfm`, `usj`, `osis`, `thml`, `gbf`)
    pub fn book_format(&self, book_id: u8) -> Option<String> {
        (self.source.book_format(BookId(book_id))).map(|f| f.name().to_string())
    }

    /// The number of verses in each of a book's chapters, from chapter 1, as it numbers them
    pub fn chapter_verses(&self, book_id: u8) -> Result<Vec<u8>, LogoxideError> {
        Ok(self.source.chapter_verses(BookId(book_id))?)
    }

    /// The files, or books in them, passed over: files of no format read (`not-text`), books
    /// another file has in a better format (`duplicate`), peripherals, books logoxide doesn't know
    pub fn skipped(&self) -> Vec<SkippedFile> {
        self.source.skipped().iter().map(skipped_file).collect()
    }
}
