use std::{collections::BTreeMap, sync::Arc};

use topos_bible::data::books::BookId;
use topos_bible_markup::{
    Markup, parse,
    verses::{usx_chapter_verses, verses},
};

use super::{Book, Source, book_files};
use crate::{
    error::{BiblixError, Result},
    manifest::Format,
    storage::Storage,
};

/**
A folder of USFM files, one book each (eBible.org's and fetch.bible's downloads), read through
[`usfm3`] into USX, then into the same tree as USX
- usfm3 keeps the space between adjacent `\w` words, which usfm-onion's USX loses (checked on the
  whole KJV: both give its 31,102 verses)
*/
pub struct UsfmSource {
    storage: Arc<dyn Storage>,
    files: BTreeMap<BookId, String>,
}

impl UsfmSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let files = book_files(storage.as_ref(), dir, "usfm", |head| {
            // `\id JHN ...`
            let rest = &head[head.find("\\id ")? + 4..];
            Some(rest.split_whitespace().next()?.to_string())
        })?;
        Ok(Self { storage, files })
    }

    /// A book as USX
    fn usx(&self, id: BookId) -> Result<String> {
        let path = self
            .files
            .get(&id)
            .ok_or_else(|| BiblixError::NotFound(format!("book {}", id.0)))?;
        let text = self.storage.read_string(path)?;
        usfm3::parse(&text, usfm3::ParseOptions::default())
            .to_usx()
            .map_err(|e| BiblixError::Book {
                book: path.clone(),
                message: format!("{e:?}"),
            })
    }
}

impl Source for UsfmSource {
    fn format(&self) -> Format {
        Format::Usfm
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn book(&self, id: BookId) -> Result<Book> {
        let nodes = parse(Markup::Usx, &self.usx(id)?);
        let spans = verses(&nodes);
        Ok(Book { id, nodes, spans })
    }

    fn chapter_verses(&self, id: BookId) -> Result<Vec<u8>> {
        Ok(usx_chapter_verses(&self.usx(id)?))
    }
}
