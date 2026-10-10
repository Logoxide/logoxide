use std::{collections::BTreeMap, sync::Arc};

use logoxide_markup::{
    Markup, parse,
    verses::{usx_chapter_verses, verses},
};
use topos_bible::data::books::BookId;

use super::{Book, Skipped, Source, book_files};
use crate::{
    error::{LogoxideError, Result},
    manifest::Format,
    storage::Storage,
};

/// A folder of USX files, one book each, as Paratext, the Digital Bible Library, eBible.org, and
/// fetch.bible publish them
pub struct UsxSource {
    storage: Arc<dyn Storage>,
    files: BTreeMap<BookId, String>,
    skipped: Vec<Skipped>,
}

impl UsxSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let (files, skipped) = book_files(storage.as_ref(), dir, &["usx"], |head| {
            // `<book code="JHN" ...>`
            let rest = &head[head.find("<book")?..];
            Some(rest.split("code=\"").nth(1)?.split('"').next()?.to_string())
        })?;
        Ok(Self {
            storage,
            files,
            skipped,
        })
    }
}

impl Source for UsxSource {
    fn format(&self) -> Option<Format> {
        Some(Format::Usx)
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }

    fn book(&self, id: BookId) -> Result<Book> {
        let path = self
            .files
            .get(&id)
            .ok_or_else(|| LogoxideError::NotFound(format!("book {}", id.0)))?;
        let text = self.storage.read_string(path)?;
        let nodes = parse(Markup::Usx, &text);
        let spans = verses(&nodes);
        Ok(Book { id, nodes, spans })
    }

    fn chapter_verses(&self, id: BookId) -> Result<Vec<u8>> {
        let path = self
            .files
            .get(&id)
            .ok_or_else(|| LogoxideError::NotFound(format!("book {}", id.0)))?;
        Ok(usx_chapter_verses(&self.storage.read_string(path)?))
    }
}
