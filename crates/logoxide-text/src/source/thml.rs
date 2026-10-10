use std::{collections::BTreeMap, sync::Arc};

use logoxide_markup::{Element, Kind, Markup, Milestone, Node};
use topos_bible::data::books::BookId;

use super::{
    Book, Skipped, Source,
    whole::{Parsed, marker_book, scan_tags, tag_attr, whole_files},
};
use crate::{
    error::{Result, TextError},
    files::Files as Storage,
    source::TextFormat,
};

/**
A folder of ThML files: a whole Bible in one file, as CCEL publishes them, or a book a file
- ThML has no verse or chapter elements: CCEL's Bibles mark each verse with an empty
  `<scripture passage="Gen 1.2" parsed="|Gen|1|2|0|0" osisRef="Bible:Gen.1.2"/>`, and number
  it for readers with `<sup class="verse">2</sup>`. Those markers become verse markers (with a
  chapter marker where the chapter changes), and the printed numbers are left out, so verses
  hash and render as USX's do
- CCEL's commentaries mark their sections the same way with empty `<scripCom type="Commentary"
  passage="Ge 1:1-2" parsed="|Gen|1|1|1|2"/>`, and a chapter's own section with
  `parsed="|Gen|1|0|0|0"`, read as its verse 0 (see [`mark_verses`])
- logoxide-markup reads `<scripture>` as an ordinary element, since in commentaries it holds
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
            // A Bible's markers, then a commentary's
            let mut scanned = scan_tags(text, "scripture", marker_reference);
            let commentary = scan_tags(text, "scripCom", marker_reference);
            for book in commentary.books {
                if !scanned.books.contains(&book) {
                    scanned.books.push(book);
                }
            }
            for (list, more) in [
                (&mut scanned.unknown, commentary.unknown),
                (&mut scanned.lettered, commentary.lettered),
            ] {
                for name in more {
                    if !list.contains(&name) {
                        list.push(name);
                    }
                }
            }
            scanned
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
    fn format(&self) -> Option<TextFormat> {
        Some(TextFormat::Thml)
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
            .ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
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

/// The OSIS reference an empty marker's tag names (`Gen.1.2`), for finding a file's books; a
/// lettered chapter (`|Esth|A|1|0|0`) isn't a place, but is named (`Esth.A`)
fn marker_reference(tag: &str) -> Option<String> {
    let tag = tag.trim_end();
    if !tag.ends_with('/') {
        return None;
    }
    match tag_place(|name| tag_attr(tag, name)) {
        Some(place) => Some(format!("{}.{}.{}", place.book, place.chapter, place.verse)),
        None => {
            let parsed = tag_attr(tag, "parsed")?.trim_matches('|');
            let mut parts = parsed.split('|');
            let (book, chapter) = (parts.next()?, parts.next()?);
            Some(format!("{book}.{chapter}"))
        }
    }
}

/// Where a `<scripture>` or `<scripCom>` marker is: an OSIS book name, chapter, verse, and the last verse of a
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

/**
A marker's place, from `parsed` (`|Gen|1|2|0|0`, `|Gen|1|2|1|3` for a bridge, `|John|3|36|4|2`
into the next chapter) or `osisRef` (`Bible:Gen.1.2`)
- A chapter's own marker (`|Gen|1|0|0|0`, a commentary's on the whole chapter) is its verse 0
*/
fn tag_place<'a>(attr: impl Fn(&str) -> Option<&'a str>) -> Option<Place> {
    let number = |s: &str| s.trim().parse::<u8>().ok().filter(|n| *n > 0);
    if let Some(parsed) = attr("parsed") {
        let parts: Vec<&str> = parsed.trim_matches('|').split('|').collect();
        if let [book, chapter, verse, rest @ ..] = parts.as_slice() {
            let (chapter, verse) = (number(chapter)?, verse.trim().parse::<u8>().ok()?);
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

/// Whether an element is an empty `<scripture>` or `<scripCom>`: a verse marker (one holding
/// text is a quotation)
fn is_marker(e: &Element) -> bool {
    e.children.is_empty()
        && (e.name.eq_ignore_ascii_case("scripture") || e.name.eq_ignore_ascii_case("scripcom"))
}

/// Whether an element is one of ThML's divisions (`div1` to `div6`)
fn is_division(e: &Element) -> bool {
    let name = e.name.to_ascii_lowercase();
    matches!(
        name.as_str(),
        "div1" | "div2" | "div3" | "div4" | "div5" | "div6"
    )
}

/// The place of the first marker in `nodes`, in document order
fn first_place(nodes: &[Node]) -> Option<Place> {
    for node in nodes {
        let Node::Element(e) = node else { continue };
        if is_marker(e) {
            if let Some(place) = tag_place(|n| e.attr(n)) {
                return Some(place);
            }
            continue;
        }
        if let Some(place) = first_place(&e.children) {
            return Some(place);
        }
    }
    None
}

/// A chapter marker for a place's chapter
fn chapter_marker(place: &Place) -> Node {
    Node::Element(Element {
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
    })
}

/// A marker ending the verse before, naming none (so it belongs to no book)
fn verse_end() -> Node {
    Node::Element(Element {
        kind: Kind::Verse,
        name: String::from("verse"),
        attrs: vec![],
        children: vec![],
        milestone: Some(Milestone::End),
        unclosed: false,
    })
}

/**
The tree with CCEL's verse markers as verse and chapter markers (see [`ThmlSource`]);
`chapter` is the book and chapter of the marker before
- A commentary's chapter marker (`|Gen|1|0|0|0`) is its verse 0, so what it says of the whole
  chapter is a note on verse 0. Only the first in a chapter counts: a later one would number the
  chapter's verses out of order
- A division (`div1` to `div6`) whose first marker is in another chapter ends the verse before
  it, so a chapter's introduction or a book's preface isn't read as the last note of the chapter
  before. When that marker is the chapter's verse 0, the chapter and its verse 0 start at the
  division's start instead (Matthew Henry puts the marker after a chapter's introduction,
  Calvin before), so the introduction is the note on verse 0
*/
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
        let marker = is_marker(&e);
        let place = marker.then(|| tag_place(|n| e.attr(n))).flatten();
        // A lettered chapter's marker (`|Esth|A|1|0|0`): a verse marker, so its verses are left
        // out with it (`without_lettered`), not run into the verse before
        let lettered = (marker && place.is_none())
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
            let mut children = vec![];
            if is_division(&e)
                && let Some(first) = first_place(&e.children)
            {
                let here = (first.book.clone(), first.chapter);
                if chapter.as_ref() != Some(&here) {
                    if first.verse == 0 {
                        children.push(chapter_marker(&first));
                        let empty = Element {
                            kind: Kind::Verse,
                            name: String::from("scripCom"),
                            attrs: vec![],
                            children: vec![],
                            milestone: None,
                            unclosed: false,
                        };
                        children.push(verse_marker(empty, &first));
                        *chapter = Some(here);
                    } else if chapter.is_some() {
                        out.push(verse_end());
                    }
                }
            }
            children.extend(mark_verses(std::mem::take(&mut e.children), chapter));
            e.children = children;
            out.push(Node::Element(e));
            continue;
        };
        let here = (place.book.clone(), place.chapter);
        if chapter.as_ref() != Some(&here) {
            out.push(chapter_marker(&place));
            *chapter = Some(here);
        } else if place.verse == 0 {
            // The chapter's verse 0 was marked already, at its division's start
            continue;
        }
        out.push(verse_marker(e, &place));
    }
    out
}

/// A marker element made a verse marker for its place
fn verse_marker(mut e: Element, place: &Place) -> Node {
    e.kind = Kind::Verse;
    e.milestone = Some(Milestone::Start);
    let id = format!("{}.{}.{}", place.book, place.chapter, place.verse);
    // A run into a later chapter is one id, which numbers it (`John.3.36-John.4.2`)
    if place.last_chapter > place.chapter {
        let end = format!("{}.{}.{}", place.book, place.last_chapter, place.last);
        e.attrs
            .push((String::from("osisID"), format!("{id}-{end}")));
        return Node::Element(e);
    }
    let number = match place.last > place.verse {
        true => format!("{}-{}", place.verse, place.last),
        false => place.verse.to_string(),
    };
    e.attrs.push((String::from("osisID"), id));
    e.attrs.push((String::from("n"), number));
    Node::Element(e)
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
        assert_eq!(package.chapter_verses(ruth).unwrap(), [2, 2]);
        // The printed verse numbers are left out, and the headings (as headings are)
        assert_eq!(
            verse_texts(&*package, ruth),
            [
                (1, 1, "Now it came to pass in the days.".into()),
                (1, 2, "And the name was Elimelech.".into()),
                (2, 1, "And Naomi had a kinsman.".into())
            ]
        );
        let bridge = &package.book(ruth).unwrap().spans[2];
        assert_eq!((bridge.verse, bridge.last), (1, 2));
        // A cross-reference names another book, and a `<scripture>` holding text is a quote:
        // neither is a verse of Obadiah's
        let texts = verse_texts(&*package, oba);
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

    /// Matthew Henry's Genesis 1–2 (CCEL's `henry/mhc1.xml`, its text cut short): sections
    /// marked with empty `<scripCom>`, each chapter's after the chapter's introduction
    const HENRY: &str = include_str!("../../tests/fixtures/ccel/mhc1-gen1-2.xml");
    /// Calvin's Genesis 1–2 (`calvin/calcom01.xml`, cut short): a chapter's marker before its text
    const CALVIN: &str = include_str!("../../tests/fixtures/ccel/calcom01-gen1-2.xml");

    fn spans(package: &dyn Source, id: BookId) -> Vec<(u8, u8, u8)> {
        let book = package.book(id).unwrap();
        (book.spans.iter())
            .map(|s| (s.chapter, s.verse, s.last))
            .collect()
    }

    #[test]
    fn a_commentarys_sections() {
        let package = package("thml", &[("mhc1.xml", HENRY)]);
        let genesis = book("GEN");
        assert_eq!(package.books(), [genesis]);
        let spans = spans(&*package, genesis);
        assert_eq!(
            spans[..4],
            [(1, 0, 0), (1, 1, 2), (1, 3, 5), (1, 6, 8)],
            "{spans:?}"
        );
        assert!(spans.contains(&(1, 31, 31)));
        assert!(spans.contains(&(2, 0, 0)) && spans.contains(&(2, 21, 25)));
        assert_eq!(spans.iter().filter(|s| s.1 == 0).count(), 2);
        let texts = verse_texts(&*package, genesis);
        // The chapter's introduction, before its marker, is the note on verse 0
        let intro = |chapter: u8| {
            let found = texts.iter().find(|(c, v, _)| (*c, *v) == (chapter, 0));
            found.map(|(_, _, t)| t.as_str()).unwrap_or_default()
        };
        assert!(intro(1).contains("The foundation of all"), "{}", intro(1));
        assert!(
            intro(2).starts_with("This chapter is an appendix"),
            "{}",
            intro(2)
        );
        // and isn't in the last note of the chapter before
        let last = texts.iter().find(|(c, v, _)| (*c, *v) == (1, 31));
        let last = last.map(|(_, _, t)| t.as_str()).unwrap_or_default();
        assert!(last.contains("We have here the approbation"), "{last}");
        assert!(!last.contains("appendix"), "{last}");
        let notes = package
            .notes(&crate::bible_data().books().parse_osis("Gen.1.4").unwrap())
            .unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!((notes[0].verse, notes[0].last), (3, 5));
    }

    #[test]
    fn a_commentary_marking_chapters_first() {
        let package = package("thml", &[("calcom01.xml", CALVIN)]);
        let genesis = book("GEN");
        let spans = spans(&*package, genesis);
        assert_eq!(spans[..3], [(1, 0, 0), (1, 1, 1), (1, 2, 2)], "{spans:?}");
        let at = spans.iter().position(|s| *s == (2, 0, 0)).unwrap();
        assert_eq!(spans[at - 1], (1, 31, 31));
        assert_eq!(spans[at + 1], (2, 1, 1));
        let texts = verse_texts(&*package, genesis);
        // The front matter (the Argument) is no verse's
        assert!(!texts.iter().any(|(_, _, t)| t.contains("hearken to Moses")));
        let last = &texts[at - 1].2;
        assert!(!last.contains("Genesis 2:1-25"), "{last}");
    }

    /// A division whose first marker is in another chapter ends the verse before
    #[test]
    fn divisions_end_the_chapter_before() {
        let thml = r#"<ThML><ThML.body><div1 title="Ruth">
<div2 title="Chapter 1"><p><scripCom parsed="|Ruth|1|1|1|2"/>On the first.</p></div2>
<div2 title="Chapter 2"><p>The second chapter's introduction.</p>
<p><scripCom parsed="|Ruth|2|1|0|0"/>On Ruth 2:1.</p></div2>
</div1></ThML.body></ThML>"#;
        let package = package("thml", &[("ruth.xml", thml)]);
        assert_eq!(
            verse_texts(&*package, book("RUT")),
            [
                (1, 1, String::from("On the first.")),
                (2, 1, String::from("On Ruth 2:1."))
            ]
        );
    }
}
