//! Versification maps: where a reference numbered one way (a Bible's `versification`) is in
//! another numbering, through the map packages a project has.

use boltffi::*;
use logoxide_core::topos_bible::data::chapter_verses::ChapterVerses;
use logoxide_text::versification::{Mapped, Verse, VersificationMap, Versifications};

use crate::{LogoxideError, LogoxidePackage};

/// One verse: its book (1 for Genesis), chapter, and number (0 for a psalm's title)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappedVerse {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
}

impl From<Verse> for MappedVerse {
    fn from(v: Verse) -> Self {
        MappedVerse {
            book: v.book.0,
            chapter: v.chapter,
            verse: v.verse,
        }
    }
}

/// A passage once mapped: its verses in the other numbering, and those with no counterpart there
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappedPassage {
    /// `Psalm 51:3-4` (empty when no verse has a counterpart)
    pub text: String,
    /// The same as OSIS references (`Ps.51.3-Ps.51.4`), books apart by spaces
    pub osis: String,
    /// In Bible order, each once
    pub verses: Vec<MappedVerse>,
    /// The passage's verses the other numbering doesn't have, written like `text`
    pub missing_text: String,
    pub missing: Vec<MappedVerse>,
}

impl From<Mapped> for MappedPassage {
    fn from(m: Mapped) -> Self {
        let books = logoxide_core::bible_data().books();
        let osis: Vec<String> = (m.passages().iter())
            .filter_map(|p| p.to_osis(books))
            .collect();
        MappedPassage {
            text: m.text(),
            osis: osis.join(" "),
            verses: m.verses.iter().copied().map(MappedVerse::from).collect(),
            missing_text: m.missing_text(),
            missing: m.missing.iter().copied().map(MappedVerse::from).collect(),
        }
    }
}

/**
The versification maps a host has opened, to map references between any two versifications they
connect (a Bible's is `info().versification`; `own` names none)

```ts
let maps = logoxide.versifications();
for (const pkg of mapPackages) maps = maps.withMap(pkg); // kind VersificationMap
const mapped = maps.map("Psalm 51:1", "English", "Original", null); // mapped.text: "Psalm 51:3"
```
*/
#[derive(Clone, Default)]
pub struct LogoxideVersifications {
    pub(crate) maps: Versifications,
}

#[export]
impl LogoxideVersifications {
    /// These maps and a map package's (`kind = "versification-map"`)
    pub fn with_map(
        &self,
        package: &LogoxidePackage,
    ) -> Result<LogoxideVersifications, LogoxideError> {
        let map = logoxide_core::reader::versification_map(package.package())?;
        Ok(self.with(map))
    }

    /// These maps and one from a `.vrs` file's text, mapping `from` to `to`
    pub fn with_vrs(
        &self,
        from: String,
        to: String,
        text: String,
    ) -> Result<LogoxideVersifications, LogoxideError> {
        let map = VersificationMap::parse(&from, &to, [("map.vrs", text.as_str())])?;
        Ok(self.with(map))
    }

    /// Every versification the maps name
    pub fn names(&self) -> Vec<String> {
        self.maps.names().into_iter().map(String::from).collect()
    }

    /// Where a reference (`Psalm 51:1`, `Joel 3`) numbered by `from` is in `to` (names ignore
    /// case). `counts` are the book's verse counts in `from` (a Bible's own,
    /// `LogoxidePackage.chapterVerses(book)`), for whole chapters; without them topos's are used
    pub fn map(
        &self,
        reference: String,
        from: String,
        to: String,
        counts: Option<Vec<u8>>,
    ) -> Result<MappedPassage, LogoxideError> {
        let counts = counts.map(ChapterVerses::new);
        let mapped = self
            .maps
            .map_reference(&reference, &from, &to, counts.as_ref())?;
        Ok(mapped.into())
    }
}

impl LogoxideVersifications {
    fn with(&self, map: VersificationMap) -> LogoxideVersifications {
        let mut maps = self.maps.clone();
        maps.add(map);
        LogoxideVersifications { maps }
    }
}
