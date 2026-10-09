use std::{collections::BTreeMap, sync::Arc};

use topos_bible::data::books::BookId;
use topos_bible_markup::Markup;

use super::{
    Book, Source,
    whole::{Parsed, marker_book, osis_book, scan_tags, tag_attr, whole_files},
};
use crate::{
    error::{BiblixError, Result},
    manifest::Format,
    storage::Storage,
};

/**
A folder of OSIS files: a whole Bible in one file (`<div type="book" osisID="Gen">` for each book,
as CrossWire and eBible.org publish them), or a book a file
- A file's books are the ones its verses' `osisID`s name (`Gen.1.1`)
- Verses are milestones (`<verse sID/>` … `<verse eID/>`) or elements holding their text
*/
pub struct OsisSource {
    storage: Arc<dyn Storage>,
    files: BTreeMap<BookId, String>,
    parsed: Parsed,
}

impl OsisSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let files = whole_files(storage.as_ref(), dir, &["xml", "osis"], |text| {
            scan_tags(text, "verse", |tag| {
                ["osisID", "sID"]
                    .iter()
                    .find_map(|name| tag_attr(tag, name))
                    .and_then(osis_book)
            })
        })?;
        Ok(Self {
            storage,
            files,
            parsed: Parsed::new(),
        })
    }
}

impl Source for OsisSource {
    fn format(&self) -> Option<Format> {
        Some(Format::Osis)
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn book(&self, id: BookId) -> Result<Book> {
        let path = self
            .files
            .get(&id)
            .ok_or_else(|| BiblixError::NotFound(format!("book {}", id.0)))?;
        (self.parsed).book(
            self.storage.as_ref(),
            path,
            id,
            Markup::Osis,
            |nodes| nodes,
            marker_book,
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        package::{Features, Package},
        storage::MemoryStorage,
        summary::verse_text,
    };
    use topos_bible_markup::verses::slice;

    /// A package of one format's files
    pub(crate) fn package(format: &str, files: &[(&str, &str)]) -> Package {
        let manifest = format!(
            "[package]\nid = \"t\"\nname = \"T\"\nabbreviation = \"T\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n\n[formats]\n{format} = {{ path = \"{format}/\" }}\n"
        );
        let mut all = vec![(String::from("biblix.toml"), manifest)];
        all.extend((files.iter()).map(|(p, t)| (format!("{format}/{p}"), t.to_string())));
        let storage = Arc::new(MemoryStorage::with(all));
        Package::open(
            storage,
            Features {
                features: &[],
                default: true,
            },
        )
        .unwrap()
    }

    /// Each verse's chapter, number, and words
    pub(crate) fn verse_texts(package: &Package, id: BookId) -> Vec<(u8, u8, String)> {
        let book = package.source().book(id).unwrap();
        (book.spans.iter())
            .map(|s| {
                let text = verse_text(&slice(&book.nodes, &s.start, Some(&s.end)));
                (s.chapter, s.verse, text)
            })
            .collect()
    }

    pub(crate) fn book(code: &str) -> BookId {
        crate::bible_data().books().search_usfm(code).unwrap()
    }

    const TWO_BOOKS: &str = r#"<?xml version="1.0"?>
<osis><osisText osisIDWork="KJV">
<header><work osisWork="KJV"><title>Test</title></work></header>
<div type="bookGroup">
<div type="book" osisID="Gen"><title>Genesis</title>
<chapter osisRef="Gen.1" sID="Gen.1.seID.1" n="1"/>
<p><verse osisID="Gen.1.1" sID="Gen.1.1.seID.2" n="1"/>In the beginning.<verse eID="Gen.1.1.seID.2"/>
<verse osisID="Gen.1.2" sID="Gen.1.2.seID.3" n="2"/>And the earth.<verse eID="Gen.1.2.seID.3"/></p>
<chapter eID="Gen.1.seID.1"/>
<chapter osisRef="Gen.2" sID="Gen.2.seID.4" n="2"/>
<p><verse osisID="Gen.2.1" sID="Gen.2.1.seID.5" n="1"/>Thus the heavens.<verse eID="Gen.2.1.seID.5"/></p>
<chapter eID="Gen.2.seID.4"/>
</div>
<div type="book" osisID="Exod"><title>Exodus</title>
<chapter sID="Exod.1" osisID="Exod.1"/><p><verse osisID="Exod.1.1">Now these are the names.</verse><verse osisID="Exod.1.2">Reuben.</verse></p><chapter eID="Exod.1"/>
</div>
</div>
</osisText></osis>"#;

    #[test]
    fn a_bible_in_one_file() {
        let package = package("osis", &[("kjv.osis.xml", TWO_BOOKS)]);
        let (genesis, exo) = (book("GEN"), book("EXO"));
        assert_eq!(package.books(), [genesis, exo]);
        assert_eq!(package.source().chapter_verses(genesis).unwrap(), [2, 1]);
        assert_eq!(
            verse_texts(&package, genesis),
            [
                (1, 1, "In the beginning.".into()),
                (1, 2, "And the earth.".into()),
                (2, 1, "Thus the heavens.".into())
            ]
        );
        // Verses as elements holding their text, and nothing of Genesis
        assert_eq!(
            verse_texts(&package, exo),
            [
                (1, 1, "Now these are the names.".into()),
                (1, 2, "Reuben.".into())
            ]
        );
        let exodus = package.source().book(exo).unwrap();
        assert!(!verse_text(&exodus.nodes).contains("beginning"));
    }

    #[test]
    fn books_as_milestones_and_a_book_a_file() {
        // Books marked by milestones, not elements: each is the run from its first marker to
        // its last
        let flat = r#"<osis><osisText>
<div type="book" sID="b1" osisID="Ruth"/><chapter sID="Ruth.1" osisID="Ruth.1" n="1"/><verse sID="Ruth.1.1" osisID="Ruth.1.1"/>Ruth one.<verse eID="Ruth.1.1"/><chapter eID="Ruth.1"/><div type="book" eID="b1"/>
<div type="book" sID="b2" osisID="Jonah"/><chapter sID="Jonah.1" osisID="Jonah.1" n="1"/><verse sID="Jonah.1.1" osisID="Jonah.1.1"/>Jonah one.<verse eID="Jonah.1.1"/><chapter eID="Jonah.1"/><div type="book" eID="b2"/>
</osisText></osis>"#;
        // A book a file whose verses name no book: its file's name says which
        let obadiah = r#"<osis><chapter n="1"/><verse n="1"/>The vision of Obadiah.<verse n="2"/>Behold.</osis>"#;
        let package = package("osis", &[("bible.xml", flat), ("OBA.xml", obadiah)]);
        let (ruth, jonah, oba) = (book("RUT"), book("JON"), book("OBA"));
        assert_eq!(package.books(), [ruth, oba, jonah]);
        assert_eq!(verse_texts(&package, ruth), [(1, 1, "Ruth one.".into())]);
        assert_eq!(verse_texts(&package, jonah), [(1, 1, "Jonah one.".into())]);
        assert_eq!(
            verse_texts(&package, oba),
            [
                (1, 1, "The vision of Obadiah.".into()),
                (1, 2, "Behold.".into())
            ]
        );
    }
}
