use std::{collections::BTreeMap, sync::Arc};

use topos_bible::data::books::BookId;
use topos_bible_markup::{Element, Kind, Markup, Milestone, Node};

use super::{
    Book, Skipped, Source,
    whole::{Parsed, marker_book, scan_tags, tag_attr, whole_files},
};
use crate::{
    error::{LogoxideError, Result},
    manifest::Format,
    storage::Storage,
};

/**
A folder of ThML files: a whole Bible in one file, as CCEL publishes them, or a book a file
- ThML has no verse or chapter elements: CCEL's Bibles mark each verse with an empty
  `<scripture passage="Gen 1.2" parsed="|Gen|1|2|0|0" osisRef="Bible:Gen.1.2"/>`, and number
  it for readers with `<sup class="verse">2</sup>`. Those markers become verse markers (with a
  chapter marker where the chapter changes), and the printed numbers are left out, so verses
  hash and render as USX's do
- topos-bible-markup reads `<scripture>` as an ordinary element, since in commentaries it holds
  quoted text; only empty ones are taken as verses
*/
pub struct ThmlSource {
    storage: Arc<dyn Storage>,
    files: BTreeMap<BookId, String>,
    skipped: Vec<Skipped>,
    parsed: Parsed,
}

impl ThmlSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let (files, skipped) = whole_files(storage.as_ref(), dir, &["xml", "thml"], |text| {
            scan_tags(text, "scripture", |tag| {
                let tag = tag.trim_end();
                if !tag.ends_with('/') {
                    return None;
                }
                match tag_place(|name| tag_attr(tag, name)) {
                    Some(place) => {
                        Some(format!("{}.{}.{}", place.book, place.chapter, place.verse))
                    }
                    // A lettered chapter (`|Esth|A|1|0|0`) isn't a place, but is said
                    None => {
                        let parsed = tag_attr(tag, "parsed")?.trim_matches('|');
                        let mut parts = parsed.split('|');
                        let (book, chapter) = (parts.next()?, parts.next()?);
                        Some(format!("{book}.{chapter}"))
                    }
                }
            })
        })?;
        Ok(Self {
            storage,
            files,
            skipped,
            parsed: Parsed::new(),
        })
    }
}

impl Source for ThmlSource {
    fn format(&self) -> Option<Format> {
        Some(Format::Thml)
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
        (self.parsed).book(
            self.storage.as_ref(),
            path,
            id,
            Markup::Thml,
            |nodes| mark_verses(nodes, &mut None),
            marker_book,
        )
    }
}

/// Where a `<scripture>` marker is: an OSIS book name, chapter, verse, and the last verse of a
/// bridge
#[derive(Debug, PartialEq)]
struct Place {
    book: String,
    chapter: u8,
    verse: u8,
    /// The chapter of `last`: `chapter`, unless the marker runs into a later one
    last_chapter: u8,
    last: u8,
}

/// A marker's place, from `parsed` (`|Gen|1|2|0|0`, `|Gen|1|2|1|3` for a bridge, `|John|3|36|4|2`
/// into the next chapter) or `osisRef` (`Bible:Gen.1.2`)
fn tag_place<'a>(attr: impl Fn(&str) -> Option<&'a str>) -> Option<Place> {
    let number = |s: &str| s.trim().parse::<u8>().ok().filter(|n| *n > 0);
    if let Some(parsed) = attr("parsed") {
        let parts: Vec<&str> = parsed.trim_matches('|').split('|').collect();
        if let [book, chapter, verse, rest @ ..] = parts.as_slice() {
            let (chapter, verse) = (number(chapter)?, number(verse)?);
            let to = |s: &str| s.trim().parse::<u8>().ok();
            let (last_chapter, last) = match rest {
                [to_chapter, to_verse, ..] if number(to_chapter) == Some(chapter) => {
                    (chapter, number(to_verse).unwrap_or(verse).max(verse))
                }
                [to_chapter, to_verse, ..] => match (number(to_chapter), to(to_verse)) {
                    (Some(c), Some(v)) if c > chapter => (c, v),
                    _ => (chapter, verse),
                },
                _ => (chapter, verse),
            };
            return Some(Place {
                book: book.to_string(),
                chapter,
                verse,
                last_chapter,
                last,
            });
        }
    }
    let reference = attr("osisRef")?;
    let reference = reference.rsplit_once(':').map_or(reference, |(_, r)| r);
    let mut parts = reference.split('-').next()?.split('.');
    let book = parts.next()?.to_string();
    let chapter = number(parts.next()?)?;
    let verse = number(parts.next()?)?;
    Some(Place {
        book,
        chapter,
        verse,
        last_chapter: chapter,
        last: verse,
    })
}

/// The tree with CCEL's verse markers as verse and chapter markers (see [`ThmlSource`]);
/// `chapter` is the book and chapter of the marker before
fn mark_verses(nodes: Vec<Node>, chapter: &mut Option<(String, u8)>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Node::Element(mut e) = node else {
            out.push(node);
            continue;
        };
        let name = e.name.to_ascii_lowercase();
        if name == "sup" && e.attr("class").is_some_and(|c| c == "verse") {
            continue;
        }
        let place = (name == "scripture" && e.children.is_empty())
            .then(|| tag_place(|n| e.attr(n)))
            .flatten();
        // A lettered chapter's marker (`|Esth|A|1|0|0`): a verse marker, so its verses are left
        // out with it (`without_lettered`), not run into the verse before
        let lettered = (name == "scripture" && e.children.is_empty() && place.is_none())
            .then(|| e.attr("parsed"))
            .flatten()
            .map(|parsed| parsed.trim_matches('|').replace('|', "."))
            .filter(|reference| super::whole::lettered_chapter(reference));
        if let Some(reference) = lettered {
            e.kind = Kind::Verse;
            e.milestone = Some(Milestone::Start);
            e.attrs.push((String::from("osisID"), reference));
            out.push(Node::Element(e));
            continue;
        }
        let Some(place) = place else {
            e.children = mark_verses(std::mem::take(&mut e.children), chapter);
            out.push(Node::Element(e));
            continue;
        };
        let here = (place.book.clone(), place.chapter);
        if chapter.as_ref() != Some(&here) {
            out.push(Node::Element(Element {
                kind: Kind::Chapter,
                name: String::from("chapter"),
                attrs: vec![
                    (
                        String::from("osisID"),
                        format!("{}.{}", place.book, place.chapter),
                    ),
                    (String::from("n"), place.chapter.to_string()),
                ],
                children: vec![],
                milestone: Some(Milestone::Start),
                unclosed: false,
            }));
            *chapter = Some(here);
        }
        e.kind = Kind::Verse;
        e.milestone = Some(Milestone::Start);
        let id = format!("{}.{}.{}", place.book, place.chapter, place.verse);
        // A run into a later chapter is one id, which numbers it (`John.3.36-John.4.2`)
        if place.last_chapter > place.chapter {
            let end = format!("{}.{}.{}", place.book, place.last_chapter, place.last);
            e.attrs
                .push((String::from("osisID"), format!("{id}-{end}")));
            out.push(Node::Element(e));
            continue;
        }
        let number = match place.last > place.verse {
            true => format!("{}-{}", place.verse, place.last),
            false => place.verse.to_string(),
        };
        e.attrs.push((String::from("osisID"), id));
        e.attrs.push((String::from("n"), number));
        out.push(Node::Element(e));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::osis::tests::{book, package, verse_texts};

    const CCEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ThML><ThML.head><generalInfo><description>A test</description></generalInfo></ThML.head>
<ThML.body>
<div1 title="Old Testament" id="OT">
<div2 title="Ruth" id="Ruth">
<div3 title="Ruth 1" id="Ruth.1">
<h4>Chapter 1</h4>
<scripture passage="Ruth 1.1" parsed="|Ruth|1|1|0|0" osisRef="Bible:Ruth.1.1" />
<p>Now it came to pass in the days.
<scripture passage="Ruth 1.2" parsed="|Ruth|1|2|0|0" osisRef="Bible:Ruth.1.2" />
<sup class="verse">2</sup>And the name <i>was</i> Elimelech.</p>
</div3>
<div3 title="Ruth 2" id="Ruth.2">
<h4>Chapter 2</h4>
<p><scripture passage="Ruth 2.1-2" parsed="|Ruth|2|1|2|2" osisRef="Bible:Ruth.2.1-Ruth.2.2" />
<sup class="verse">1</sup>And Naomi had a kinsman.</p>
</div3>
</div2>
<div2 title="Obadiah" id="Obad">
<div3 title="Obadiah 1" id="Obad.1">
<p><scripture passage="Obad 1.1" parsed="|Obad|1|1|0|0" osisRef="Bible:Obad.1.1" />
The vision of Obadiah. <scripRef passage="Ruth 1:1">Ruth 1:1</scripRef></p>
<p><scripture passage="Obad 1.2" parsed="|Obad|1|2|0|0"><p>Quoted, not a verse</p></scripture></p>
</div3>
</div2>
</div1>
</ThML.body></ThML>"#;

    #[test]
    fn a_ccel_bible() {
        let package = package("thml", &[("bible.xml", CCEL)]);
        let (ruth, oba) = (book("RUT"), book("OBA"));
        assert_eq!(package.books(), [ruth, oba]);
        assert_eq!(package.source().chapter_verses(ruth).unwrap(), [2, 2]);
        // The printed verse numbers are left out, and the headings (as headings are)
        assert_eq!(
            verse_texts(&package, ruth),
            [
                (1, 1, "Now it came to pass in the days.".into()),
                (1, 2, "And the name was Elimelech.".into()),
                (2, 1, "And Naomi had a kinsman.".into())
            ]
        );
        let bridge = &package.source().book(ruth).unwrap().spans[2];
        assert_eq!((bridge.verse, bridge.last), (1, 2));
        // A cross-reference names another book, and a `<scripture>` holding text is a quote:
        // neither is a verse of Obadiah's
        let texts = verse_texts(&package, oba);
        assert_eq!(texts.len(), 1);
        assert!(texts[0].2.starts_with("The vision of Obadiah. Ruth 1:1"));
    }

    #[test]
    fn places_from_parsed_and_osis_refs() {
        let attrs = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
        };
        assert_eq!(
            tag_place(attrs(&[("parsed", "|Gen|1|2|0|0")])),
            Some(Place {
                book: "Gen".into(),
                chapter: 1,
                verse: 2,
                last_chapter: 1,
                last: 2
            })
        );
        assert_eq!(
            tag_place(attrs(&[("parsed", "|John|3|16|3|17")])).map(|p| p.last),
            Some(17)
        );
        assert_eq!(
            tag_place(attrs(&[("osisRef", "Bible:1Sam.2.3")])).map(|p| (p.book, p.verse)),
            Some(("1Sam".into(), 3))
        );
        assert_eq!(
            tag_place(attrs(&[("parsed", "|John|3|36|4|2")])).map(|p| (p.last_chapter, p.last)),
            Some((4, 2))
        );
        assert_eq!(tag_place(attrs(&[("passage", "Gen 1")])), None);
    }
}
