/*!
Passages and whole books as plain text, with where each verse is and what each run of the text
is, for anything that shows a Bible as text: a hover, a read-only document, a note's quotation.

- [`text`] writes a passage (`John 3:16-18; 4:1`): its parts a blank line apart, cut at a limit
- [`render`] writes a whole book, with each verse's and chapter label's range ([`Rendered`]),
  so a place in the text is a verse again ([`Rendered::part_at`]) and a passage is found in it
  ([`Rendered::passage_ranges`])
- [`Style`] says how: which labels, which elements are hidden, templates for the rest, and a
  width to wrap at. The markup's kinds mark the text ([`Text::marks`], [`Rendered::marks`]),
  for whoever colors it

The tree and the verses come from a [`Book`]; this knows no packages, and no editor.
*/

use std::{collections::BTreeMap, collections::HashMap, ops::Range};

use logoxide_markup::{
    Filter, Flat, Flattener, Kind, NodePath, TextOptions,
    flatten::flatten,
    verses::{VerseSpan, passage_ranges, slice},
};
use topos_bible::segments::Passage;

use crate::source::Book;

/**
How a passage is written as text

- `verse_label` and `chapter_label` are written where each verse and chapter starts, with
  `{verse}`, `{chapter}`, `{book}` (the USX code, `JHN`), and `{name}` (`John`); none writes no
  label
- `hidden` elements are left out with their text (titles and notes), or unwrapped to plain text
  (words of Jesus, additions, the divine name, quotations: no template, so nothing marks them)
- `templates` write a shown element's text in a form: `## {text}` for a title, `“{text}”` for
  words of Jesus, with `{name}` as in the labels
- `width` breaks lines longer than that many characters at a space ([`wrap`])
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub verse_label: Option<String>,
    pub chapter_label: Option<String>,
    pub hidden: Vec<Kind>,
    pub templates: BTreeMap<Kind, String>,
    pub width: Option<usize>,
}

impl Style {
    /// The style a reader usually wants: verse numbers as `[{verse}]`, chapters as
    /// `{name} {chapter}`, headings shown, notes hidden
    pub fn reading() -> Style {
        Style {
            verse_label: Some(String::from("[{verse}]")),
            chapter_label: Some(String::from("{name} {chapter}")),
            hidden: vec![Kind::Note],
            templates: BTreeMap::new(),
            width: None,
        }
    }

    /// The markup's options for this style, with `{name}` as the book's name
    pub fn options(&self, name: &str) -> TextOptions {
        let mut filter = Filter::all().drop(Kind::Metadata);
        for &kind in &self.hidden {
            filter = match kind {
                Kind::Title | Kind::Note => filter.drop(kind),
                _ => filter.unwrap(kind),
            };
        }
        let with_name = |template: &String| template.replace("{name}", name);
        TextOptions {
            filter,
            verse_label: self.verse_label.as_ref().map(with_name),
            chapter_label: self.chapter_label.as_ref().map(with_name),
            templates: (self.templates.iter())
                .filter(|(kind, _)| !self.hidden.contains(kind))
                .map(|(kind, template)| (*kind, with_name(template)))
                .collect(),
            ..TextOptions::default()
        }
    }
}

/// What marks a run of text: each kept element's span that has text, in order (verse and
/// chapter labels, titles, notes, words of Jesus, additions, the divine name, quotations, and
/// the rest of the markup's kinds)
fn marks(flat: &Flat) -> Vec<(Range<usize>, Kind)> {
    let mut marks: Vec<(Range<usize>, Kind)> = (flat.spans.iter())
        .filter(|s| !s.range.is_empty())
        .map(|s| (s.range.clone(), s.kind))
        .collect();
    marks.sort_by_key(|(range, _)| (range.start, range.end));
    marks
}

/// A passage's text, and what marks each run of it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub marks: Vec<(Range<usize>, Kind)>,
}

/**
A passage as text in `style`, its parts (`John 3:16; 4` has two) a blank line apart, or none if
the book doesn't have it. `name` is the book's name for the labels (`John`). With a `limit`,
text after that many bytes is cut, ending with ` …`
*/
pub fn text(
    book: &Book,
    passage: &Passage,
    style: &Style,
    name: &str,
    limit: Option<usize>,
) -> Option<Text> {
    if book.id != passage.book {
        return None;
    }
    let mut flat = Flattener::new(style.options(name));
    for r in passage_ranges(&book.spans, passage) {
        let (first, last) = (&book.spans[r.start], &book.spans[r.end - 1]);
        flat.push(&slice(&book.nodes, &first.start, Some(&last.end)));
    }
    let flat = flat.finish();
    let all = marks(&flat);
    // The parts joined, moving each mark with its part
    let mut text = Text {
        text: String::new(),
        marks: vec![],
    };
    for entry in flat.entries.iter().filter(|r| !r.is_empty()) {
        if !text.text.is_empty() {
            text.text.push_str("\n\n");
        }
        let offset = text.text.len();
        text.text.push_str(&flat.text[entry.clone()]);
        text.marks.extend(
            all.iter()
                .filter(|(r, _)| entry.start <= r.start && r.end <= entry.end)
                .map(|(r, kind)| {
                    (
                        r.start - entry.start + offset..r.end - entry.start + offset,
                        *kind,
                    )
                }),
        );
    }
    if text.text.is_empty() {
        return None;
    }
    if let Some(limit) = limit.filter(|&limit| text.text.len() > limit) {
        let mut cut = limit;
        while !text.text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.text.truncate(cut);
        text.marks.retain(|(r, _)| r.start < cut);
        for (r, _) in &mut text.marks {
            r.end = r.end.min(cut);
        }
        text.text.push_str(" …");
    }
    if let Some(width) = style.width.filter(|&w| w > 0) {
        text.text = wrap(&text.text, width);
    }
    Some(text)
}

/// What a part of a rendered book is
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Verse {
        chapter: u8,
        verse: u8,
        /// The last verse of a bridge (else `verse`)
        last: u8,
    },
    /// A chapter's label
    Chapter(u8),
}

/// A whole book as text
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    /**
    Each verse and chapter label in order: from its number or label to the next one (or a
    heading), without the whitespace after it
    - Headings and the book's title are between parts
    */
    pub parts: Vec<(Range<usize>, Part)>,
    /// What marks each run of the text, in order
    pub marks: Vec<(Range<usize>, Kind)>,
}

impl Rendered {
    /// The part at a byte, or the next one after it (a heading goes with the verse after it)
    pub fn part_at(&self, byte: usize) -> Option<Part> {
        let i = self.parts.partition_point(|(range, _)| range.start <= byte);
        if let Some((range, part)) = i.checked_sub(1).and_then(|i| self.parts.get(i))
            && byte <= range.end
        {
            return Some(*part);
        }
        self.parts.get(i).map(|(_, part)| *part)
    }

    /// Where each part of a passage is: from its first verse's number to its last verse's end
    pub fn passage_ranges(&self, passage: &Passage) -> Vec<Range<usize>> {
        // Each verse's range, first (chapter, verse), and last
        type Verse<'a> = (&'a Range<usize>, (u8, u8), (u8, u8));
        let verses: Vec<Verse> = self
            .parts
            .iter()
            .filter_map(|(range, part)| match *part {
                Part::Verse {
                    chapter,
                    verse,
                    last,
                } => Some((range, (chapter, verse), (chapter, last))),
                Part::Chapter(_) => None,
            })
            .collect();
        passage
            .ranges(None)
            .into_iter()
            .filter_map(|r| {
                let (start, end) = (
                    (r.start.chapter, r.start.verse),
                    (r.end.chapter, r.end.verse),
                );
                let within = |v: &&Verse| v.2 >= start && v.1 <= end;
                let first = verses.iter().find(within)?;
                let last = verses.iter().rfind(within)?;
                Some(first.0.start..last.0.end)
            })
            .collect()
    }
}

/// The whole book as text in `style` (`name` is the book's name for the labels): the whole
/// tree at once (a verse can start in one paragraph and end in the next), with each verse found
/// by its marker
pub fn render(book: &Book, style: &Style, name: &str) -> Rendered {
    let flat = flatten(&book.nodes, &style.options(name));
    let verses: HashMap<&NodePath, &VerseSpan> =
        book.spans.iter().map(|s| (&s.marker, s)).collect();
    // Each verse's and chapter's start, and where headings start (which end the part before)
    let mut starts: Vec<(usize, Option<Part>)> = vec![];
    for span in &flat.spans {
        let start = span.range.start;
        match span.kind {
            Kind::Verse => {
                if let Some(v) = verses.get(&span.path) {
                    let part = Part::Verse {
                        chapter: v.chapter,
                        verse: v.verse,
                        last: v.last,
                    };
                    starts.push((start, Some(part)));
                }
            }
            // Its number is the next verse's chapter
            Kind::Chapter if !span.range.is_empty() => starts.push((start, Some(Part::Chapter(0)))),
            Kind::Title => starts.push((start, None)),
            _ => {}
        }
    }
    starts.sort_by_key(|(start, _)| *start);
    let mut parts: Vec<(Range<usize>, Part)> = vec![];
    for (i, &(start, part)) in starts.iter().enumerate() {
        let Some(part) = part else { continue };
        let end = starts.get(i + 1).map_or(flat.text.len(), |(next, _)| *next);
        let end = start + flat.text[start..end].trim_end().len();
        parts.push((start..end, part));
    }
    let mut chapter = None;
    for (_, part) in parts.iter_mut().rev() {
        match part {
            Part::Verse { chapter: c, .. } => chapter = Some(*c),
            Part::Chapter(c) => *c = chapter.unwrap_or_default(),
        }
    }
    let text = match style.width.filter(|&w| w > 0) {
        Some(width) => wrap(&flat.text, width),
        None => flat.text.clone(),
    };
    Rendered {
        marks: marks(&flat),
        text,
        parts,
    }
}

/**
Breaks lines longer than `width` characters at their last space that fits (a word longer than
the line overflows it)
- Each break replaces a space, so byte offsets (verses, marks) stay the same
*/
pub fn wrap(text: &str, width: usize) -> String {
    let mut bytes = text.as_bytes().to_vec();
    // The characters on the line so far, and the last space's byte and column
    let mut column = 0;
    let mut space: Option<(usize, usize)> = None;
    for (i, c) in text.char_indices() {
        if c == '\n' {
            (column, space) = (0, None);
            continue;
        }
        if c == ' ' {
            space = Some((i, column));
        }
        column += 1;
        if column > width
            && let Some((at, at_column)) = space.take()
        {
            bytes[at] = b'\n';
            column -= at_column + 1;
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| text.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{MemoryFiles, TextFormat, source::open};

    const USX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0"><book code="JHN" style="id">John</book>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="s">Nicodemus</para>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world<note caller="+" style="f">Or: loved so much</note>, that he gave.<verse eid="JHN 3:16"/>
<verse number="17" style="v" sid="JHN 3:17"/><char style="wj">For God sent not his Son.</char><verse eid="JHN 3:17"/></para>
<chapter eid="JHN 3"/>
<chapter number="4" style="c" sid="JHN 4"/>
<para style="p"><verse number="1" style="v" sid="JHN 4:1"/>When therefore the Lord knew.<verse eid="JHN 4:1"/></para>
<chapter eid="JHN 4"/>
</usx>"#;

    fn john() -> Book {
        let files = Arc::new(MemoryFiles::with([("JHN.usx", USX)]));
        let source = open(files, TextFormat::Usx, "").unwrap();
        let id = crate::bible_data().books().search_usfm("JHN").unwrap();
        source.book(id).unwrap()
    }

    fn passage(reference: &str) -> Passage {
        crate::bible_data().parse(reference).unwrap()
    }

    #[test]
    fn a_passage_as_text() {
        let book = john();
        let style = Style::reading();
        let read = text(&book, &passage("John 3:16-17"), &style, "John", None).unwrap();
        // The note is hidden; the labels are written and marked; the heading before verse 16
        // is the verse's (a verse runs from the end of the one before it)
        assert_eq!(
            read.text,
            "Nicodemus\n[16] For God so loved the world, that he gave. [17] For God sent not his Son."
        );
        let kinds: Vec<(&str, Kind)> = (read.marks.iter())
            .map(|(r, k)| (&read.text[r.clone()], *k))
            .collect();
        assert!(kinds.contains(&("[16]", Kind::Verse)), "{kinds:?}");
        assert!(
            kinds.contains(&("For God sent not his Son.", Kind::WordsOfJesus)),
            "{kinds:?}"
        );
        // Two parts, a blank line apart; a limit cuts with an ellipsis
        let two = text(&book, &passage("John 3:16; 4:1"), &style, "John", None).unwrap();
        assert!(two.text.contains("\n\n[1] When"), "{}", two.text);
        let cut = text(&book, &passage("John 3:16"), &style, "John", Some(20)).unwrap();
        assert!(
            cut.text.ends_with(" …") && cut.text.len() < 30,
            "{}",
            cut.text
        );
        assert!(cut.marks.iter().all(|(r, _)| r.end <= 20));
        // Another book, or a verse the book lacks, is nothing
        assert!(text(&book, &passage("Romans 1:1"), &style, "Romans", None).is_none());
        assert!(text(&book, &passage("John 5:1"), &style, "John", None).is_none());
        // A template, and the note shown in brackets
        let styled = Style {
            templates: [(Kind::WordsOfJesus, String::from("“{text}”"))].into(),
            hidden: vec![],
            verse_label: None,
            ..Style::reading()
        };
        let shown = text(&book, &passage("John 3:16-17"), &styled, "John", None).unwrap();
        assert_eq!(
            shown.text,
            "Nicodemus\nFor God so loved the world [Or: loved so much], that he gave. “For God sent not his Son.”"
        );
    }

    #[test]
    fn a_whole_book_knows_its_verses() {
        let book = john();
        let style = Style {
            width: Some(30),
            ..Style::reading()
        };
        let rendered = render(&book, &style, "John");
        assert!(rendered.text.starts_with("John 3\n"), "{}", rendered.text);
        assert!(
            rendered
                .text
                .lines()
                .all(|l| l.chars().count() <= 30 || !l.contains(' '))
        );
        let parts: Vec<Part> = rendered.parts.iter().map(|(_, p)| *p).collect();
        assert_eq!(
            parts,
            [
                Part::Chapter(3),
                Part::Verse {
                    chapter: 3,
                    verse: 16,
                    last: 16
                },
                Part::Verse {
                    chapter: 3,
                    verse: 17,
                    last: 17
                },
                Part::Chapter(4),
                Part::Verse {
                    chapter: 4,
                    verse: 1,
                    last: 1
                },
            ]
        );
        // The heading goes with the verse after it
        let heading = rendered.text.find("Nicodemus").unwrap();
        assert_eq!(
            rendered.part_at(heading),
            Some(Part::Verse {
                chapter: 3,
                verse: 16,
                last: 16
            })
        );
        let ranges = rendered.passage_ranges(&passage("John 3:17-4:1"));
        assert_eq!(ranges.len(), 1);
        assert!(rendered.text[ranges[0].clone()].starts_with("[17]"));
        assert!(rendered.text[ranges[0].clone()].ends_with("knew."));
        assert!(rendered.marks.iter().any(|(_, k)| *k == Kind::Title));
    }

    #[test]
    fn wraps_at_spaces() {
        assert_eq!(wrap("aa bb cc dd\nee ff", 5), "aa bb\ncc dd\nee ff");
        assert_eq!(wrap("a verylongword b", 4), "a\nverylongword\nb");
        assert_eq!(wrap("“é” ü ö", 3), "“é”\nü ö");
    }
}
