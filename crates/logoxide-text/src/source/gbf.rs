/*!
GBF (the General Bible Format, SWORD's older modules): `<XY…>` tokens logoxide-markup reads,
with sync marks for the structure: `<SBJohn>` starts a book, `<SC3>` a chapter, `<SV16>` a
verse (a mark without a number is one more than the last; `<SV16><SV17>` with nothing between
is a bridge). A file holds a whole Bible (its books the ones its `<SB>` marks name) or one book
(no `<SB>`: named by its file name). The sync marks become the chapter and verse markers every
other source has, each book wrapped in a division of its own, so verses hash and render as USX's
do.
*/

use std::{collections::BTreeMap, sync::Arc};

use logoxide_markup::{Element, Kind, Markup, Milestone, Node};
use topos_bible::data::books::BookId;

use super::{
    Book, Skipped, Source,
    whole::{Parsed, Scanned, marker_book, whole_files},
};
use crate::{
    error::{Result, TextError},
    files::Files as Storage,
    source::TextFormat,
};

pub struct GbfSource {
    storage: Arc<dyn Storage>,
    files: BTreeMap<BookId, String>,
    skipped: Vec<Skipped>,
    parsed: Parsed,
}

impl GbfSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let (files, skipped) = whole_files(storage.as_ref(), dir, &["gbf", "txt"], scan_books)?;
        Ok(Self {
            storage,
            files,
            skipped,
            parsed: Parsed::new(),
        })
    }
}

impl Source for GbfSource {
    fn format(&self) -> Option<TextFormat> {
        Some(TextFormat::Gbf)
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
        let name = osis_name(id).unwrap_or_else(|| id.0.to_string());
        (self.parsed).book(
            self.storage.as_ref(),
            path,
            id,
            Markup::Gbf,
            |nodes| mark_structure(nodes, &name),
            marker_book,
        )
    }
}

/// A book's OSIS name, as the markers name it
fn osis_name(id: BookId) -> Option<String> {
    crate::bible_data().books().get_osis(id).cloned()
}

/**
The book an `<SB…>` mark names: an OSIS id (`John`), a USFM code (`JHN`), an English name or
abbreviation (`Matthew`, `Mt`), or GBF's own number (1 to 39 the Old Testament in order, 64 to 90
the New; its apocrypha numbers, 40 to 58, aren't read)
*/
pub(crate) fn gbf_book(name: &str) -> Option<BookId> {
    let name = name.trim();
    let books = crate::bible_data().books();
    if let Ok(number) = name.parse::<u8>() {
        return match number {
            1..=39 => Some(BookId(number)),
            64..=90 => Some(BookId(number - 24)),
            _ => None,
        };
    }
    (books.search_osis(name))
        .or_else(|| books.search_usfm(&name.to_ascii_uppercase()))
        .or_else(|| books.search(name))
}

/// The value of each `<SB…>` mark in a file's text, in order
fn book_marks(text: &str) -> Vec<&str> {
    let mut marks = vec![];
    let mut rest = text;
    while let Some(i) = rest.find("<SB") {
        rest = &rest[i + 3..];
        let Some(end) = rest.find('>') else { break };
        let value = &rest[..end];
        if !value.contains(|c: char| c.is_whitespace() || c == '<') {
            marks.push(value);
        }
        rest = &rest[end + 1..];
    }
    marks
}

/// The books a file's `<SB>` marks name (none for a book a file)
fn scan_books(text: &str) -> Scanned {
    let mut scanned = Scanned::default();
    for mark in book_marks(text) {
        match gbf_book(mark) {
            Some(id) if !scanned.books.contains(&id) => scanned.books.push(id),
            Some(_) => {}
            None if !scanned.unknown.contains(&mark.to_string()) => {
                scanned.unknown.push(mark.to_string());
            }
            None => {}
        }
    }
    scanned
}

/// Where the marks are: the book (its OSIS name), chapter, and verse so far
struct Place {
    book: String,
    chapter: u8,
    verse: u8,
}

fn marker(kind: Kind, name: &str, attrs: Vec<(String, String)>) -> Element {
    Element {
        kind,
        name: name.to_string(),
        attrs,
        children: vec![],
        milestone: Some(Milestone::Start),
        unclosed: false,
    }
}

/// A sync mark's number: its value, else one more than `last`
fn mark_number(e: &Element, last: u8) -> u8 {
    (e.attr("value"))
        .and_then(|v| v.trim().parse::<u8>().ok())
        .unwrap_or_else(|| last.saturating_add(1))
}

/**
The tree with GBF's sync marks as chapter and verse markers (`osisID` and `n`, as OSIS's
have), each book's nodes in a division naming it (`default_book` for a file without `<SB>`
marks; what comes before the first mark, a file header, stays outside every book), and verse
marks with nothing between them made one bridge
*/
pub(crate) fn mark_structure(nodes: Vec<Node>, default_book: &str) -> Vec<Node> {
    let mut place = Place {
        book: default_book.to_string(),
        chapter: 0,
        verse: 0,
    };
    let has_books = nodes.iter().any(|n| is_mark(n, "SB"));
    let mut out = vec![];
    let mut current: Vec<Node> = vec![];
    let mut division: Option<Element> = (!has_books).then(|| book_division(default_book));
    for node in nodes {
        if is_mark(&node, "SB") {
            if let Some(mut d) = division.take() {
                d.children = bridge_marks(std::mem::take(&mut current));
                out.push(Node::Element(d));
            }
            // A book logoxide doesn't know keeps its mark's name, which names no book, so its
            // verses belong to none
            let mark = (node.as_element().and_then(|e| e.attr("value"))).unwrap_or_default();
            let name = (gbf_book(mark).and_then(osis_name)).unwrap_or_else(|| mark.to_string());
            place = Place {
                book: name.clone(),
                chapter: 0,
                verse: 0,
            };
            division = Some(book_division(&name));
            continue;
        }
        let marked = mark_node(node, &mut place);
        match &mut division {
            Some(_) => current.push(marked),
            None => out.push(marked),
        }
    }
    if let Some(mut d) = division {
        d.children = bridge_marks(current);
        out.push(Node::Element(d));
    }
    out
}

fn book_division(name: &str) -> Element {
    Element {
        kind: Kind::Division,
        name: String::from("book"),
        attrs: vec![
            (String::from("type"), String::from("book")),
            (String::from("osisID"), name.to_string()),
        ],
        children: vec![],
        milestone: None,
        unclosed: false,
    }
}

fn is_mark(node: &Node, code: &str) -> bool {
    node.as_element().is_some_and(|e| is_mark_element(e, code))
}

fn is_mark_element(e: &Element, code: &str) -> bool {
    e.name == code && e.children.is_empty()
}

/// A node with its sync marks made markers (recursing into elements, since a mark can fall
/// inside an unclosed one)
fn mark_node(node: Node, place: &mut Place) -> Node {
    let Node::Element(mut e) = node else {
        return node;
    };
    if is_mark_element(&e, "SC") {
        place.chapter = mark_number(&e, place.chapter);
        place.verse = 0;
        let id = format!("{}.{}", place.book, place.chapter);
        return Node::Element(marker(
            Kind::Chapter,
            "chapter",
            vec![
                (String::from("osisID"), id),
                (String::from("n"), place.chapter.to_string()),
            ],
        ));
    }
    if is_mark_element(&e, "SV") {
        let number = mark_number(&e, place.verse);
        place.verse = number;
        let id = format!("{}.{}.{number}", place.book, place.chapter.max(1));
        return Node::Element(marker(
            Kind::Verse,
            "verse",
            vec![
                (String::from("osisID"), id),
                (String::from("n"), number.to_string()),
            ],
        ));
    }
    e.children = (std::mem::take(&mut e.children).into_iter())
        .map(|child| mark_node(child, place))
        .collect();
    Node::Element(e)
}

/// Verse markers with nothing but whitespace between them, made one: the first widened into a
/// bridge to the last (`<SV16><SV17>` is `n="16-17"`)
fn bridge_marks(nodes: Vec<Node>) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Node::Element(mut e) = node else {
            out.push(node);
            continue;
        };
        if e.kind == Kind::Verse {
            // The node before, past any whitespace: a verse marker widens
            let blank = |n: &Node| n.as_text().is_some_and(|t| t.trim().is_empty());
            let last = out.iter().rposition(|n| !blank(n));
            if let Some(i) = last
                && let Some(Node::Element(before)) = out.get_mut(i)
                && before.kind == Kind::Verse
            {
                let first = (before.attr("n").and_then(|n| n.split('-').next()))
                    .unwrap_or("")
                    .to_string();
                let last = e.attr("n").unwrap_or("").to_string();
                before.attrs.retain(|(k, _)| k != "n");
                before
                    .attrs
                    .push((String::from("n"), format!("{first}-{last}")));
                continue;
            }
        } else {
            e.children = bridge_marks(std::mem::take(&mut e.children));
        }
        out.push(Node::Element(e));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::osis::tests::{book, package, verse_texts};

    const TWO_BOOKS: &str = "<H1>A Test Bible<SBJohn><TT>John<Tt><SC1><SV1>In the beginning was the Word.<SV2>The same was in the beginning.<CM><SC><SV1>And the third day.\n<SBActs><SC1><SV1>The former treatise.<SV2><SV3>Linked verses.<SV4>Last of all.<ZZ>";

    #[test]
    fn notes_stretch_over_the_verses_after_them_when_asked() {
        // Notes on 1:1, 1:4-5, and 2:3; nothing on 1:2-3, 1:6, or 2:1-2
        let text =
            "<SBJohn><SC1><SV1>On one.<SV4><SV5>On four and five.<CM><SC2><SV3>On two three.<ZZ>";
        let package = package("gbf", &[("john.gbf", text)]);
        let john = package.book(book("JHN")).unwrap();
        let notes = |reference: &str, stretch: bool| {
            let passage = crate::bible_data().books().parse_osis(reference).unwrap();
            let notes = super::super::book_notes(&john, &passage, stretch);
            (notes.iter())
                .map(|n| (n.chapter, n.verse, n.last))
                .collect::<Vec<_>>()
        };
        assert_eq!(notes("John.1.2", false), []);
        assert_eq!(notes("John.1.2", true), [(1, 1, 1)]);
        assert_eq!(notes("John.1.2-John.1.4", true), [(1, 1, 1), (1, 4, 5)]);
        assert_eq!(notes("John.1.4", true), [(1, 4, 5)]);
        assert_eq!(notes("John.1.6", true), [(1, 4, 5)]);
        // Never from the chapter before
        assert_eq!(notes("John.2.1", true), []);
        assert_eq!(notes("John.2.1-John.2.3", true), [(2, 3, 3)]);
    }

    #[test]
    fn a_bible_in_one_file() {
        let package = package("gbf", &[("bible.gbf", TWO_BOOKS)]);
        let (john, acts) = (book("JHN"), book("ACT"));
        assert_eq!(package.books(), [john, acts]);
        assert_eq!(package.chapter_verses(john).unwrap(), [2, 1]);
        assert_eq!(
            verse_texts(&*package, john),
            [
                (1, 1, "In the beginning was the Word.".into()),
                (1, 2, "The same was in the beginning.".into()),
                (2, 1, "And the third day.".into())
            ]
        );
        // The header stays out of every book; the title too (it's a heading)
        let first = package.book(john).unwrap();
        let all = crate::hash::verse_text(&first.nodes, crate::hash::HashRule::CURRENT);
        assert!(
            !all.contains("A Test Bible") && !all.contains("John"),
            "{all}"
        );
        let acts_book = package.book(acts).unwrap();
        assert_eq!(
            verse_texts(&*package, acts),
            [
                (1, 1, "The former treatise.".into()),
                (1, 2, "Linked verses.".into()),
                (1, 4, "Last of all.".into())
            ]
        );
        assert_eq!((acts_book.spans[1].verse, acts_book.spans[1].last), (2, 3));
    }

    #[test]
    fn a_book_a_file_and_books_by_number() {
        let obadiah = "<SC1><SV1>The vision of Obadiah. <FR>Thus<Fr><SV>Behold.";
        let numbered =
            "<SB1><SC1><SV1>In the beginning.<SB64><SC1><SV1>The book of the generation.";
        let package = package("gbf", &[("OBA.gbf", obadiah), ("two.txt", numbered)]);
        let (genesis, oba, mat) = (book("GEN"), book("OBA"), book("MAT"));
        assert_eq!(package.books(), [genesis, oba, mat]);
        assert_eq!(
            verse_texts(&*package, oba),
            [
                (1, 1, "The vision of Obadiah. Thus".into()),
                (1, 2, "Behold.".into())
            ]
        );
        assert_eq!(
            verse_texts(&*package, genesis),
            [(1, 1, "In the beginning.".into())]
        );
        assert_eq!(
            verse_texts(&*package, mat),
            [(1, 1, "The book of the generation.".into())]
        );
        assert_eq!(gbf_book("Mt"), Some(mat));
        assert_eq!(gbf_book("JHN"), gbf_book("John"));
        assert_eq!(gbf_book("45"), None);
    }

    #[test]
    fn unknown_books_are_passed_over() {
        let text = "<SBZzz><SC1><SV1>x<SBGen><SC1><SV1>In the beginning.";
        let package = package("gbf", &[("bible.gbf", text)]);
        assert_eq!(package.books(), [book("GEN")]);
        assert_eq!(package.skipped()[0].book, "Zzz");
        assert_eq!(
            verse_texts(&*package, book("GEN")),
            [(1, 1, "In the beginning.".into())]
        );
    }
}
