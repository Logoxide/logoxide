/*!
A package's text in one format, read a book at a time into topos-bible-markup's tree, the same
whatever the format.
*/

mod usfm;
mod usx;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use topos_bible::data::books::BookId;
use topos_bible_markup::{
    Node,
    verses::{VerseSpan, chapter_verses},
};

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
    fn format(&self) -> Format;

    /// The books it has, in order, and the file each is in
    fn files(&self) -> &BTreeMap<BookId, String>;

    fn book(&self, id: BookId) -> Result<Book>;

    /// The number of verses in each chapter (sources can count without building the tree)
    fn chapter_verses(&self, id: BookId) -> Result<Vec<u8>> {
        Ok(self.book(id)?.chapter_verses())
    }

    fn books(&self) -> Vec<BookId> {
        self.files().keys().copied().collect()
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

/// Each book's file in a folder: files with `extension`, named for their book, else by the code
/// `code_in` finds at the start of the file
pub(crate) fn book_files(
    storage: &dyn crate::storage::Storage,
    dir: &str,
    extension: &str,
    code_in: impl Fn(&str) -> Option<String>,
) -> Result<BTreeMap<BookId, String>> {
    let mut files = BTreeMap::new();
    for name in storage.list(dir)? {
        let Some(stem) = name
            .rsplit_once('.')
            .filter(|(_, ext)| ext.eq_ignore_ascii_case(extension))
            .map(|(stem, _)| stem)
        else {
            continue;
        };
        let path = crate::storage::join(dir, &name);
        let id = code_from_name(stem).or_else(|| {
            let head = storage.read(&path).ok()?;
            let head = String::from_utf8_lossy(&head[..head.len().min(4096)]).into_owned();
            crate::bible_data()
                .books()
                .search_usfm(&code_in(&head)?.to_ascii_uppercase())
        });
        if let Some(id) = id {
            files.entry(id).or_insert(path);
        }
    }
    Ok(files)
}
