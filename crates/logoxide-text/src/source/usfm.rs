use std::{collections::BTreeMap, sync::Arc};

use logoxide_markup::{
    Markup, parse,
    verses::{usx_chapter_verses, verses},
};
use topos_bible::data::books::BookId;

use super::{Book, Skipped, Source, book_files};
use crate::{
    error::{Result, TextError},
    files::Files as Storage,
    source::TextFormat,
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
    skipped: Vec<Skipped>,
}

impl UsfmSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let (files, skipped) = book_files(storage.as_ref(), dir, &["usfm", "sfm"], |head| {
            // `\id JHN ...`
            let rest = &head[head.find("\\id ")? + 4..];
            Some(rest.split_whitespace().next()?.to_string())
        })?;
        Ok(Self {
            storage,
            files,
            skipped,
        })
    }

    /// A book as USX
    fn usx(&self, id: BookId) -> Result<String> {
        let path = self
            .files
            .get(&id)
            .ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
        usfm_to_usx(&self.storage.read_string(path)?).map_err(|e| match e {
            TextError::Book { message, .. } => TextError::Book {
                book: path.clone(),
                message,
            },
            e => e,
        })
    }
}

/// A USFM book as USX (through usfm3), for readers that take USX
pub fn usfm_to_usx(usfm: &str) -> Result<String> {
    usfm3::parse(usfm, usfm3::ParseOptions::default())
        .to_usx()
        .map_err(|e| TextError::Book {
            book: String::from("USFM"),
            message: format!("{e:?}"),
        })
}

impl Source for UsfmSource {
    fn format(&self) -> Option<TextFormat> {
        Some(TextFormat::Usfm)
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn skipped(&self) -> &[Skipped] {
        &self.skipped
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
