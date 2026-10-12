//! A Bible book's text in markup (logoxide-markup): its verses, and passages as HTML or plain
//! text. topos-ffi's `LogoxideBook` until October 2026: books are logoxide's, topos only finds
//! references.

use std::collections::HashMap;

use boltffi::*;
use logoxide_core::topos_bible::segments::Passage as CorePassage;
use logoxide_markup::{
    Filter, HtmlOptions, HtmlRenderer, Kind, Markup, Milestone, Node, NodePath, TextOptions,
    find_all, parse, plain_text,
    verses::{VerseSpan, chapter_verses, passage_ranges, slice, usx_chapter_verses, verses},
};
use logoxide_text::passage::{Style, text_in};
use topos_bible::{Passage, Topos};

use crate::LogoxideError;

/// What a node of a book's tree is: logoxide-markup's kinds, and `Text` for a run of text
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Text,
    Word,
    Note,
    Reference,
    Title,
    WordsOfJesus,
    Quote,
    Added,
    DivineName,
    Highlight,
    Foreign,
    Paragraph,
    LineGroup,
    Line,
    LineBreak,
    Division,
    Chapter,
    Verse,
    Milestone,
    Image,
    Entry,
    Metadata,
    Other,
}

impl From<Kind> for NodeKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Word => NodeKind::Word,
            Kind::Note => NodeKind::Note,
            Kind::Reference => NodeKind::Reference,
            Kind::Title => NodeKind::Title,
            Kind::WordsOfJesus => NodeKind::WordsOfJesus,
            Kind::Quote => NodeKind::Quote,
            Kind::Added => NodeKind::Added,
            Kind::DivineName => NodeKind::DivineName,
            Kind::Highlight => NodeKind::Highlight,
            Kind::Foreign => NodeKind::Foreign,
            Kind::Paragraph => NodeKind::Paragraph,
            Kind::LineGroup => NodeKind::LineGroup,
            Kind::Line => NodeKind::Line,
            Kind::LineBreak => NodeKind::LineBreak,
            Kind::Division => NodeKind::Division,
            Kind::Chapter => NodeKind::Chapter,
            Kind::Verse => NodeKind::Verse,
            Kind::Milestone => NodeKind::Milestone,
            Kind::Image => NodeKind::Image,
            Kind::Entry => NodeKind::Entry,
            Kind::Metadata => NodeKind::Metadata,
            _ => NodeKind::Other,
        }
    }
}

/// An attribute of an element, as the markup wrote it (`osisID`, `style`, `lemma`)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeAttribute {
    pub name: String,
    pub value: String,
}

/// Which milestone an element is: its start (`sID`) or its end (`eID`, or an end tag whose
/// start was in an earlier entry)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MilestoneKind {
    Start,
    End,
}

/**
One node of a book's tree ([`LogoxideBook::nodes`]), in document order, each naming its parent
by index (`null` at the top level), so a host can rebuild the tree or walk the list as it is

- A text node has `kind` `Text` and its words in `text`; an element has its kind, its tag
  `name` as the markup wrote it (USX `para`'s `style` attribute says which paragraph), and its
  `attributes`
- `milestone` says an element is a start or end marker rather than a container (a verse in
  USX, an `sID`/`eID` pair in OSIS); `unclosed` that it was opened but not closed in its entry
*/
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookNode {
    /// Its place in the list, which children and verses name
    pub index: u32,
    pub parent: Option<u32>,
    pub kind: NodeKind,
    pub name: String,
    pub text: String,
    pub attributes: Vec<NodeAttribute>,
    pub milestone: Option<MilestoneKind>,
    pub unclosed: bool,
}

/// Where a verse is in the book's nodes ([`LogoxideBook::verse_nodes`]): its marker, and the
/// nodes it runs over, from `start` to `end` (both indices into [`LogoxideBook::nodes`]; a
/// verse runs from the end of the one before it to its own end, so a heading belongs to the
/// verse after it)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerseNodes {
    pub chapter: u8,
    pub verse: u8,
    pub last: u8,
    pub last_chapter: u8,
    pub part: u8,
    pub marker: u32,
    pub start: u32,
    pub end: u32,
}

/// How [`LogoxideBook::styled_text`] writes a passage: logoxide-text's `passage::Style`, as the
/// language server writes hover text and whole books
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookTextStyle {
    /// Written where each verse starts (`[{verse}]`; `{chapter}`, `{book}`, `{name}` too), or
    /// none for no verse numbers
    pub verse_label: Option<String>,
    /// Written on its own line where each chapter starts (`{name} {chapter}`), or none
    pub chapter_label: Option<String>,
    /// Footnotes and cross references, in brackets when shown
    pub notes: bool,
    pub headings: bool,
    /// Words of Jesus, translators' additions, the divine name, and quotations written in a
    /// template with `{text}` (`“{text}”`), by kind; without one they're plain text
    pub templates: Vec<KindTemplate>,
    /// Lines longer than this many characters break at a space; none keeps them whole
    pub width: Option<u32>,
    /// Text after this many bytes is cut, ending with an ellipsis; none keeps it whole
    pub limit: Option<u32>,
}

/// A template for one kind of element in a [`BookTextStyle`]
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindTemplate {
    pub kind: NodeKind,
    pub template: String,
}

/// A run of a passage's text and what it is (a verse number, a heading, words of Jesus), by
/// byte offsets into [`StyledText::text`], for a host that colors it
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextMark {
    pub start: u32,
    pub end: u32,
    pub kind: NodeKind,
}

/// A run of text, by byte offsets
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

/// A passage as text in a [`BookTextStyle`], its parts a blank line apart, and its marks
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledText {
    pub text: String,
    pub marks: Vec<TextMark>,
}

fn template_kind(kind: NodeKind) -> Option<Kind> {
    Some(match kind {
        NodeKind::Title => Kind::Title,
        NodeKind::Note => Kind::Note,
        NodeKind::WordsOfJesus => Kind::WordsOfJesus,
        NodeKind::Added => Kind::Added,
        NodeKind::DivineName => Kind::DivineName,
        NodeKind::Quote => Kind::Quote,
        _ => return None,
    })
}

impl From<&BookTextStyle> for Style {
    fn from(style: &BookTextStyle) -> Self {
        let mut hidden = vec![];
        if !style.notes {
            hidden.push(Kind::Note);
        }
        if !style.headings {
            hidden.push(Kind::Title);
        }
        Style {
            verse_label: style.verse_label.clone(),
            chapter_label: style.chapter_label.clone(),
            hidden,
            templates: (style.templates.iter())
                .filter_map(|t| Some((template_kind(t.kind)?, t.template.clone())))
                .collect(),
            width: style.width.map(|w| w as usize),
            ..Style::default()
        }
    }
}

/// One verse of a [`LogoxideBook`]
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BookVerse {
    pub chapter: u8,
    pub verse: u8,
    /// The last verse of a bridge like `4-5` (else the same as `verse`), in `last_chapter`
    pub last: u8,
    /// The chapter the verse ends in: `chapter`, unless it's a range into a later chapter
    /// (`John.3.36-John.4.2`, a commentary's note)
    pub last_chapter: u8,
    /// Which part of `verse` this is: 0 for the whole verse, 1 for `20a`, 2 for `20b`, and so on
    pub part: u8,
}

/// How [`LogoxideBook::html`] and [`LogoxideBook::text`] write a passage
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookTextOptions {
    /// Footnotes and cross references
    pub notes: bool,
    /// Section headings
    pub headings: bool,
    /// Prefix of every HTML class (`topos-` gives `topos-verse`); data attributes are never
    /// prefixed
    pub class_prefix: String,
}

/**
A Bible book's text, parsed once and read a passage at a time

- `markup` names the format: `usx`, or `osis` for a book with verse milestones or elements
- Each verse runs from the end of the one before it to its own end, so a heading belongs to the
  verse after it, and a passage keeps its paragraphs and poetry lines
- HTML uses logoxide-markup's classes: verse and chapter markers are empty
  `<span class="topos-verse" data-n="16" data-osis-id="JHN 3:16">`s, which a stylesheet can number

```ts
const book = LogoxideBook.parse(topos, "usx", await read("JHN.usx")); // or LogoxidePackage.book(43)
book.bookId(); // 43
const options = { notes: false, headings: true, classPrefix: "topos-" };
book.html(topos.parse("John 3:16-18")!, options); // one HTML string for each part of the reference
```
*/
pub struct LogoxideBook {
    nodes: Vec<Node>,
    spans: Vec<VerseSpan>,
    book_id: Option<u8>,
}

impl LogoxideBook {
    /// A book parsed elsewhere, like a package's (`LogoxidePackage::book`) or the cache's (for
    /// Rust only; other languages use `parse`)
    pub fn from_tree(nodes: Vec<Node>, spans: Vec<VerseSpan>, book_id: Option<u8>) -> Self {
        Self {
            nodes,
            spans,
            book_id,
        }
    }

    /// The book's tree for each part of a passage in this book
    fn parts(&self, passage: &Passage) -> Vec<Vec<Node>> {
        if self.book_id != Some(passage.book_id) {
            return Vec::new();
        }
        let passage = CorePassage::from(passage);
        passage_ranges(&self.spans, &passage)
            .into_iter()
            .map(|r| {
                let (first, last) = (&self.spans[r.start], &self.spans[r.end - 1]);
                slice(&self.nodes, &first.start, Some(&last.end))
            })
            .collect()
    }
}

/// Nodes (a commentary's note, a dictionary's entry) as plain text, as `LogoxideBook::text`
/// writes a passage
pub(crate) fn nodes_text(nodes: &[Node], options: &BookTextOptions) -> String {
    let text = TextOptions {
        filter: filter(options),
        ..TextOptions::default()
    };
    plain_text(nodes, &text).trim().to_string()
}

/// Nodes as HTML, as `LogoxideBook::html` writes a passage
pub(crate) fn nodes_html(nodes: &[Node], options: &BookTextOptions) -> String {
    HtmlRenderer::with_options(HtmlOptions {
        class_prefix: options.class_prefix.clone().into(),
        filter: filter(options),
    })
    .render(nodes)
}

fn filter(options: &BookTextOptions) -> Filter {
    let mut filter = Filter::all().drop(Kind::Metadata);
    if !options.notes {
        filter = filter.drop(Kind::Note);
    }
    if !options.headings {
        filter = filter.drop(Kind::Title);
    }
    filter
}

#[export]
impl LogoxideBook {
    /// Parses a whole book (this never fails on odd markup; only an unknown `markup` is an error,
    /// which is `null` in JavaScript)
    pub fn parse(topos: &Topos, markup: String, text: String) -> Result<Self, LogoxideError> {
        let format = Markup::from_name(&markup).ok_or_else(|| LogoxideError::InvalidMarkup {
            message: format!("unknown markup {markup:?} (usx, osis)"),
        })?;
        let nodes = parse(format, &text);
        let books = topos.books();
        // USX `<book code="JHN">` (ignoring case), OSIS `<div type="book" osisID="John">`
        let book_id = find_all(&nodes, Kind::Metadata)
            .into_iter()
            .find_map(|e| e.attr("code"))
            .and_then(|code| {
                books
                    .iter()
                    .find(|b| !code.is_empty() && b.usfm.eq_ignore_ascii_case(code))
            })
            .or_else(|| {
                find_all(&nodes, Kind::Division)
                    .into_iter()
                    .filter(|e| e.attr("type") == Some("book"))
                    .find_map(|e| {
                        let osis = e.attr("osisID")?;
                        books.iter().find(|b| !osis.is_empty() && b.osis == osis)
                    })
            })
            .map(|book| book.id);
        let spans = verses(&nodes);
        Ok(Self {
            nodes,
            spans,
            book_id,
        })
    }

    /// A USX book's verse counts per chapter (like `chapterVerses`), from its chapter and verse
    /// markers alone: fast enough to count a whole Bible without parsing it
    pub fn usx_chapter_verses(usx: String) -> Vec<u8> {
        usx_chapter_verses(&usx)
    }

    /// Where a link's exact words (`:~:text=…`) are in a verse's text now, when the words
    /// changed a little since the link was made: byte offsets of the run of words that matches
    /// best, or none when the words are gone (logoxide-text's `passage::find_fragment`)
    pub fn find_fragment(text: String, fragment: String) -> Option<TextRange> {
        logoxide_text::passage::find_fragment(&text, &fragment).map(|r| TextRange {
            start: r.start as u32,
            end: r.end as u32,
        })
    }

    /// The book (1 for Genesis through 66 for Revelation with the default data), if the markup
    /// names one the data knows
    pub fn book_id(&self) -> Option<u8> {
        self.book_id
    }

    /// Every verse, in order (verses the translation leaves out are missing)
    pub fn verses(&self) -> Vec<BookVerse> {
        self.spans
            .iter()
            .map(|s| BookVerse {
                chapter: s.chapter,
                verse: s.verse,
                last: s.last,
                last_chapter: s.last_chapter,
                part: s.part,
            })
            .collect()
    }

    /// The number of verses in each chapter, from chapter 1, as the book numbers them: its
    /// versification, for `ToposTranslations.chapterVerses`
    pub fn chapter_verses(&self) -> Vec<u8> {
        chapter_verses(&self.spans)
    }

    /// The chapters with verses, in order
    pub fn chapters(&self) -> Vec<u8> {
        let mut chapters: Vec<u8> = self.spans.iter().map(|s| s.chapter).collect();
        chapters.dedup();
        chapters
    }

    /**
    The book's whole tree as a list in document order, each node naming its parent, so a host
    renders with its own engine without parsing USX, USFM, OSIS, or ThML itself; the verses'
    places in it are `verseNodes`
    */
    pub fn nodes(&self) -> Vec<BookNode> {
        let mut out = vec![];
        flatten_nodes(&self.nodes, None, &mut out);
        out
    }

    /// Where each verse is in `nodes`: its marker, and the first and last node it runs over
    pub fn verse_nodes(&self) -> Vec<VerseNodes> {
        let mut indices = HashMap::new();
        index_paths(&self.nodes, &mut NodePath::default(), &mut indices);
        let at = |path: &NodePath| indices.get(path).copied().unwrap_or(0);
        (self.spans.iter())
            .map(|s| VerseNodes {
                chapter: s.chapter,
                verse: s.verse,
                last: s.last,
                last_chapter: s.last_chapter,
                part: s.part,
                marker: at(&s.marker),
                start: at(&s.start),
                end: at(&s.end),
            })
            .collect()
    }

    /// A passage as text in `style` (`name` is the book's name for the labels, like `John`),
    /// its parts a blank line apart, with what marks each run of it; nothing for a passage the
    /// book doesn't have. The same text the language server shows
    pub fn styled_text(
        &self,
        passage: Passage,
        style: BookTextStyle,
        name: String,
    ) -> Option<StyledText> {
        if self.book_id != Some(passage.book_id) {
            return None;
        }
        let passage = CorePassage::from(&passage);
        let limit = style.limit.map(|l| l as usize);
        let text = text_in(
            &self.nodes,
            &self.spans,
            &passage,
            &Style::from(&style),
            &name,
            limit,
        )?;
        Some(StyledText {
            marks: (text.marks.iter())
                .map(|(r, kind)| TextMark {
                    start: r.start as u32,
                    end: r.end as u32,
                    kind: NodeKind::from(*kind),
                })
                .collect(),
            text: text.text,
        })
    }

    /// Each part of the passage (`John 3:16; 4` has two) as HTML, leaving out parts the book
    /// doesn't have; nothing for a passage in another book
    pub fn html(&self, passage: Passage, options: BookTextOptions) -> Vec<String> {
        // One renderer for the passage, so words of Jesus across verses stay marked
        let mut html = HtmlRenderer::with_options(HtmlOptions {
            class_prefix: options.class_prefix.clone().into(),
            filter: filter(&options),
        });
        self.parts(&passage)
            .iter()
            .map(|nodes| html.render(nodes))
            .collect()
    }

    /// Each part of the passage as plain text (notes in brackets when kept), like
    /// [`LogoxideBook::html`]
    pub fn text(&self, passage: Passage, options: BookTextOptions) -> Vec<String> {
        let text = TextOptions {
            filter: filter(&options),
            ..TextOptions::default()
        };
        self.parts(&passage)
            .iter()
            .map(|nodes| plain_text(nodes, &text).trim().to_string())
            .collect()
    }

    /// A whole chapter as HTML (its headings, and the verses the book has in it); empty if it has
    /// none
    pub fn chapter_html(&self, chapter: u8, options: BookTextOptions) -> String {
        let first = self.spans.iter().position(|s| s.chapter == chapter);
        let last = self.spans.iter().rposition(|s| s.chapter == chapter);
        let (Some(first), Some(last)) = (first, last) else {
            return String::new();
        };
        let nodes = slice(
            &self.nodes,
            &self.spans[first].start,
            Some(&self.spans[last].end),
        );
        HtmlRenderer::with_options(HtmlOptions {
            class_prefix: options.class_prefix.clone().into(),
            filter: filter(&options),
        })
        .render(&nodes)
    }
}

/// Each node and its descendants, in document order, naming their parents
fn flatten_nodes(nodes: &[Node], parent: Option<u32>, out: &mut Vec<BookNode>) {
    for node in nodes {
        let index = out.len() as u32;
        match node {
            Node::Text(text) => out.push(BookNode {
                index,
                parent,
                kind: NodeKind::Text,
                name: String::new(),
                text: text.clone(),
                attributes: vec![],
                milestone: None,
                unclosed: false,
            }),
            Node::Element(element) => {
                out.push(BookNode {
                    index,
                    parent,
                    kind: NodeKind::from(element.kind),
                    name: element.name.clone(),
                    text: String::new(),
                    attributes: (element.attrs.iter())
                        .map(|(name, value)| NodeAttribute {
                            name: name.clone(),
                            value: value.clone(),
                        })
                        .collect(),
                    milestone: element.milestone.map(|m| match m {
                        Milestone::Start => MilestoneKind::Start,
                        Milestone::End => MilestoneKind::End,
                    }),
                    unclosed: element.unclosed,
                });
                flatten_nodes(&element.children, Some(index), out);
            }
        }
    }
}

/// Each node's path and its index in the flattened list (the same order as `flatten_nodes`)
fn index_paths(nodes: &[Node], path: &mut NodePath, indices: &mut HashMap<NodePath, u32>) {
    for (i, node) in nodes.iter().enumerate() {
        path.0.push(i);
        indices.insert(path.clone(), indices.len() as u32);
        if let Node::Element(element) = node {
            index_paths(&element.children, path, indices);
        }
        path.0.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use topos_bible::BookStyle;

    const USX: &str = r#"<usx version="3.0"><book code="JHN" style="id">x</book>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="s1">Jesus and Nicodemus</para>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world<note caller="+" style="f"><char style="ft">Or so</char></note>.<verse eid="JHN 3:16"/>
<verse number="17" style="v" sid="JHN 3:17"/>For God did not send His Son.<verse eid="JHN 3:17"/></para>
<chapter eid="JHN 3"/>
<chapter number="4" style="c" sid="JHN 4"/>
<para style="p"><verse number="1" style="v" sid="JHN 4:1"/>Now Jesus learned<verse eid="JHN 4:1"/></para>
<chapter eid="JHN 4"/></usx>"#;

    fn options(notes: bool, headings: bool) -> BookTextOptions {
        BookTextOptions {
            notes,
            headings,
            class_prefix: "topos-".into(),
        }
    }

    #[test]
    fn partial_verses() {
        let topos = Topos::new();
        let usx = r#"<usx><book code="GEN"/><chapter number="1" sid="GEN 1"/><para style="p"><verse number="20a" sid="GEN 1:20a"/>first<verse eid="GEN 1:20a"/><verse number="20b" sid="GEN 1:20b"/>second<verse eid="GEN 1:20b"/></para><chapter eid="GEN 1"/></usx>"#;
        let book = LogoxideBook::parse(&topos, "usx".into(), usx.into()).unwrap();
        let parts: Vec<_> = book.verses().iter().map(|v| (v.verse, v.part)).collect();
        assert_eq!(parts, [(20, 1), (20, 2)]);
        let passage = topos.parse("Genesis 1:20".into(), BookStyle::Name).unwrap();
        assert_eq!(book.text(passage, options(false, false)), ["firstsecond"]);
    }

    #[test]
    fn passages_as_html_and_text() {
        let topos = Topos::new();
        let book = LogoxideBook::parse(&topos, "usx".into(), USX.into()).unwrap();
        assert_eq!(book.book_id(), Some(43));
        assert_eq!(book.chapters(), [3, 4]);
        assert_eq!(book.chapter_verses(), [0, 0, 17, 1]);
        assert_eq!(LogoxideBook::usx_chapter_verses(USX.into()), [0, 0, 17, 1]);
        assert_eq!(book.verses().len(), 3);
        let parse = |r: &str| topos.parse(r.into(), BookStyle::Name).unwrap();

        assert_eq!(
            book.text(parse("John 3:16; 4"), options(false, true)),
            [
                "Jesus and Nicodemus\nFor God so loved the world.",
                "Now Jesus learned"
            ]
        );
        assert_eq!(
            book.text(parse("John 3:16-17"), options(true, false)),
            ["For God so loved the world [Or so]. For God did not send His Son."]
        );
        let html = book.html(parse("John 3:17"), options(false, true));
        assert_eq!(
            html,
            [concat!(
                r#"<p class="topos-p" data-style="p">"#,
                "\n", // the line break before the verse in the USX
                r#"<span class="topos-verse" data-osis-id="JHN 3:17" data-n="17"></span>For God did not send His Son.</p>"#
            )]
        );
        // Other books and missing chapters give nothing
        assert!(
            book.html(parse("Romans 8"), options(false, true))
                .is_empty()
        );
        assert!(book.html(parse("John 9"), options(false, true)).is_empty());
        assert!(
            book.chapter_html(3, options(false, false))
                .contains("did not send")
        );
        assert!(
            !book
                .chapter_html(3, options(false, false))
                .contains("Nicodemus")
        );
        assert!(LogoxideBook::parse(&topos, "rtf".into(), String::new()).is_err());
    }

    #[test]
    fn the_tree_and_styled_text() {
        let topos = Topos::new();
        let book = LogoxideBook::parse(&topos, "usx".into(), USX.into()).unwrap();
        let nodes = book.nodes();
        // The list is in document order, each node naming its parent: the `usx` element at
        // the top, the `book` line inside it
        assert_eq!((nodes[0].name.as_str(), nodes[0].parent), ("usx", None));
        let meta = nodes.iter().find(|n| n.kind == NodeKind::Metadata).unwrap();
        assert_eq!((meta.name.as_str(), meta.parent), ("book", Some(0)));
        let code = meta.attributes.iter().find(|a| a.name == "code").unwrap();
        assert_eq!(code.value, "JHN");
        let heading = nodes.iter().find(|n| n.kind == NodeKind::Title).unwrap();
        let words = nodes
            .iter()
            .find(|n| n.parent == Some(heading.index))
            .unwrap();
        assert_eq!(
            (words.kind, words.text.as_str()),
            (NodeKind::Text, "Jesus and Nicodemus")
        );
        let verse = nodes.iter().find(|n| n.kind == NodeKind::Verse).unwrap();
        assert_eq!(verse.milestone, Some(MilestoneKind::Start));
        assert!(nodes.iter().all(|n| n.parent.is_none_or(|p| p < n.index)));
        // Each verse names its marker and the nodes it runs over
        let verses = book.verse_nodes();
        assert_eq!(verses.len(), 3);
        assert_eq!((verses[0].chapter, verses[0].verse), (3, 16));
        assert_eq!(nodes[verses[0].marker as usize].kind, NodeKind::Verse);
        assert!(verses[0].start <= verses[0].marker && verses[0].marker <= verses[0].end);
        assert!(verses[0].end <= verses[1].end);
        // Styled text, as the language server writes it
        let parse = |r: &str| topos.parse(r.into(), BookStyle::Name).unwrap();
        let style = BookTextStyle {
            verse_label: Some("[{verse}]".into()),
            chapter_label: None,
            notes: false,
            headings: true,
            templates: vec![],
            width: None,
            limit: None,
        };
        let text = book
            .styled_text(parse("John 3:16-17"), style.clone(), "John".into())
            .unwrap();
        assert_eq!(
            text.text,
            "Jesus and Nicodemus\n[16] For God so loved the world. [17] For God did not send His Son."
        );
        let label = text
            .marks
            .iter()
            .find(|m| m.kind == NodeKind::Verse)
            .unwrap();
        assert_eq!(&text.text[label.start as usize..label.end as usize], "[16]");
        assert!(text.marks.iter().any(|m| m.kind == NodeKind::Title));
        let cut = BookTextStyle {
            limit: Some(25),
            ..style
        };
        let short = book
            .styled_text(parse("John 3:16-17"), cut, "John".into())
            .unwrap();
        assert!(short.text.ends_with(" …"), "{}", short.text);
        assert_eq!(
            LogoxideBook::find_fragment(
                "For God so loved the world.".into(),
                "so loved this world".into()
            ),
            Some(TextRange { start: 8, end: 26 })
        );
        assert!(
            book.styled_text(
                parse("Romans 8"),
                BookTextStyle {
                    verse_label: None,
                    chapter_label: None,
                    notes: false,
                    headings: false,
                    templates: vec![],
                    width: None,
                    limit: None,
                },
                "Romans".into()
            )
            .is_none()
        );
    }
}
