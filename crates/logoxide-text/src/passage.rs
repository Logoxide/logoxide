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
    Filter, Flat, Flattener, Kind, Node, NodePath, TextOptions,
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
- `indents` are the spaces each kind of poetic line or indented paragraph starts with, by its
  USFM marker (`q1`, `q2`, `pi1`; see `TextOptions::indents`)
- `verse_lines` starts each verse on a line of its own, and `paragraph_gaps` puts a blank line
  between paragraphs (see `TextOptions::verse_lines` and `paragraph_gaps`)
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub verse_label: Option<String>,
    pub chapter_label: Option<String>,
    pub hidden: Vec<Kind>,
    pub templates: BTreeMap<Kind, String>,
    pub width: Option<usize>,
    pub indents: BTreeMap<String, usize>,
    pub verse_lines: bool,
    pub paragraph_gaps: bool,
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
            indents: BTreeMap::new(),
            verse_lines: false,
            paragraph_gaps: false,
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
            indents: self.indents.clone(),
            verse_lines: self.verse_lines,
            paragraph_gaps: self.paragraph_gaps,
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
    text_in(&book.nodes, &book.spans, passage, style, name, limit)
}

/// Which of a passage's verses [`text_verses`] writes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verses {
    All,
    /// Its first that many verses (`John 3:16-18` with 2 is `John 3:16-17`)
    First(usize),
    /// Its last that many verses
    Last(usize),
}

impl Verses {
    /// The verses kept of `ranges` (indexes into a book's verses), each part still its own
    fn select(self, ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
        let total: usize = ranges.iter().map(ExactSizeIterator::len).sum();
        let kept = match self {
            Verses::All => 0..total,
            Verses::First(n) => 0..n.min(total),
            Verses::Last(n) => total.saturating_sub(n)..total,
        };
        // Each range's place among all the verses, cut to the ones kept
        let mut before = 0;
        ranges
            .into_iter()
            .filter_map(|r| {
                let (from, to) = (before, before + r.len());
                before = to;
                let (start, end) = (from.max(kept.start), to.min(kept.end));
                (start < end).then(|| r.start + start - from..r.start + end - from)
            })
            .collect()
    }
}

/// [`text`], with only some of the passage's verses: its first or last few
pub fn text_verses(
    book: &Book,
    passage: &Passage,
    style: &Style,
    name: &str,
    verses: Verses,
) -> Option<Text> {
    if book.id != passage.book {
        return None;
    }
    let ranges = verses.select(passage_ranges(&book.spans, passage));
    text_ranges(&book.nodes, &book.spans, ranges, style, name, None)
}

/// [`text`] for a tree and its verses held apart from a [`Book`] (the bindings' books), with
/// no check that the passage is in this book
pub fn text_in(
    nodes: &[Node],
    spans: &[VerseSpan],
    passage: &Passage,
    style: &Style,
    name: &str,
    limit: Option<usize>,
) -> Option<Text> {
    text_ranges(
        nodes,
        spans,
        passage_ranges(spans, passage),
        style,
        name,
        limit,
    )
}

/// The text of these ranges of verses (indexes into `spans`)
fn text_ranges(
    nodes: &[Node],
    spans: &[VerseSpan],
    ranges: Vec<Range<usize>>,
    style: &Style,
    name: &str,
    limit: Option<usize>,
) -> Option<Text> {
    let mut flat = Flattener::new(style.options(name));
    for r in ranges {
        let (first, last) = (&spans[r.start], &spans[r.end - 1]);
        flat.push(&slice(nodes, &first.start, Some(&last.end)));
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

/**
One verse of a book as reading text ([`verse_texts`]): its words as [`crate::hash`]'s rule 1
gives them (notes and headings left out, NFC, whitespace collapsed), the same text its hash is
of, and where it stands in the book's layout
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseText {
    pub chapter: u8,
    pub verse: u8,
    /// The last verse of a bridge (else `verse`), in `last_chapter`
    pub last: u8,
    pub last_chapter: u8,
    /// Which part of `verse` this is: 0 for the whole verse, 1 for `20a`, 2 for `20b`
    pub part: u8,
    pub text: String,
    /// The headings just before it, a line each, when there are any
    pub heading: Option<String>,
    /// It starts a paragraph or a stanza (the book's first verse does)
    pub paragraph: bool,
    /// It starts a line of its own (a paragraph, or a line of poetry)
    pub line: bool,
}

/// Every verse of a book as reading text, in order ([`VerseText`])
pub fn verse_texts(book: &Book) -> Vec<VerseText> {
    verse_texts_in(&book.nodes, &book.spans)
}

/// [`verse_texts`] for a tree and its verses held apart from a [`Book`] (the bindings' books)
pub fn verse_texts_in(nodes: &[Node], spans: &[VerseSpan]) -> Vec<VerseText> {
    // The layout: paragraphs a blank line apart, lines of poetry a line, headings kept
    let style = Style {
        hidden: vec![Kind::Note],
        paragraph_gaps: true,
        ..Style::default()
    };
    let flat = flatten(nodes, &style.options(""));
    let markers: HashMap<&NodePath, usize> = spans
        .iter()
        .enumerate()
        .map(|(i, s)| (&s.marker, i))
        .collect();
    // Where each verse starts in the layout, and each heading
    let mut starts: Vec<Option<usize>> = vec![None; spans.len()];
    let mut headings: Vec<Range<usize>> = vec![];
    for span in &flat.spans {
        match span.kind {
            Kind::Verse => {
                if let Some(&i) = markers.get(&span.path) {
                    starts[i].get_or_insert(span.range.start);
                }
            }
            Kind::Title if !span.range.is_empty() => headings.push(span.range.clone()),
            _ => {}
        }
    }
    // A heading goes with the first verse starting where it starts or after, or with nothing but
    // whitespace between (a superscription's paragraph can hold its psalm's first marker: BSB's
    // Psalm 3)
    headings.sort_by_key(|h| h.start);
    let mut headings = headings.into_iter().peekable();
    let mut before = 0;
    (spans.iter().zip(starts))
        .map(|(span, start)| {
            let text = crate::hash::verse_text(
                &slice(nodes, &span.start, Some(&span.end)),
                crate::hash::HashRule::One,
            );
            let mut verse = VerseText {
                chapter: span.chapter,
                verse: span.verse,
                last: span.last,
                last_chapter: span.last_chapter,
                part: span.part,
                text,
                heading: None,
                paragraph: false,
                line: false,
            };
            if let Some(start) = start.filter(|&s| s >= before) {
                // The whitespace around its start (a marker can come before the break it's
                // after is written)
                let gap = &flat.text[before..start];
                let after = &flat.text[start..];
                let space = format!(
                    "{}{}",
                    &gap[gap.trim_end().len()..],
                    &after[..after.len() - after.trim_start().len()]
                );
                verse.paragraph = before == 0 && gap.trim().is_empty() || space.contains("\n\n");
                verse.line = verse.paragraph || space.contains('\n');
                let mut titles: Vec<&str> = vec![];
                let starts_it = |h: &Range<usize>| {
                    h.start <= start
                        || (flat.text.get(start..h.start)).is_some_and(|t| t.trim().is_empty())
                };
                while let Some(h) = headings.next_if(starts_it) {
                    titles.push(flat.text[h].trim());
                }
                if !titles.is_empty() {
                    verse.heading = Some(titles.join("\n"));
                    verse.paragraph = true;
                    verse.line = true;
                }
                before = start;
            }
            verse
        })
        .collect()
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

/**
Where `fragment` (a link's exact words, as a text fragment `:~:text=…` carries them) is in
`text` now, when the words changed a little since the link was made: the run of words that
matches it best, by its words in order (case, accents, and punctuation ignored), if at least
three in five of the fragment's words are found in order (two in three for a fragment of one
or two words). None when the words are gone. Byte offsets, from the first matched word to the
end of the last, so a host can highlight them
*/
pub fn find_fragment(text: &str, fragment: &str) -> Option<Range<usize>> {
    let wanted: Vec<String> = words(fragment).into_iter().map(|(_, w)| w).collect();
    if wanted.is_empty() {
        return None;
    }
    let have = words(text);
    let needed = match wanted.len() {
        1 | 2 => wanted.len(),
        n => (n * 3).div_ceil(5),
    };
    // Each start in the text: the longest run of the fragment's words in order from there,
    // within a window a little longer than the fragment (a few words added or removed)
    let window = wanted.len() + wanted.len() / 2 + 2;
    let mut best: Option<(usize, Range<usize>)> = None;
    for start in 0..have.len() {
        let end = (start + window).min(have.len());
        let (count, first, last) = in_order(&wanted, &have[start..end]);
        if count >= needed && best.as_ref().is_none_or(|(c, _)| count > *c) {
            best = Some((count, have[start + first].0.start..have[start + last].0.end));
        }
    }
    best.map(|(_, range)| range)
}

/// The words of `text` and where each is: runs of letters and digits, lowercased, their
/// accents dropped, so a changed accent or capital doesn't lose the word
fn words(text: &str) -> Vec<(Range<usize>, String)> {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    let mut out = vec![];
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                out.push((from..i, fold(&text[from..i])));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        out.push((from..text.len(), fold(&text[from..])));
    }
    fn fold(word: &str) -> String {
        word.to_lowercase()
            .nfd()
            .filter(|c| !is_combining_mark(*c))
            .collect()
    }
    out
}

/// How many of `wanted` appear in `have` in order (the longest common subsequence), and the
/// first and last positions in `have` that are part of it
fn in_order(wanted: &[String], have: &[(Range<usize>, String)]) -> (usize, usize, usize) {
    let (n, m) = (wanted.len(), have.len());
    // lcs[i][j]: the longest common subsequence of wanted[i..] and have[j..]
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = match wanted[i] == have[j].1 {
                true => lcs[i + 1][j + 1] + 1,
                false => lcs[i + 1][j].max(lcs[i][j + 1]),
            };
        }
    }
    // Walk one subsequence to find where it starts and ends in `have`
    let (mut i, mut j) = (0, 0);
    let (mut first, mut last) = (None, 0);
    while i < n && j < m {
        if wanted[i] == have[j].1 {
            first.get_or_insert(j);
            last = j;
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (lcs[0][0], first.unwrap_or(0), last)
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
    fn each_verse_as_reading_text() {
        let verses = verse_texts(&john());
        let found: Vec<_> = (verses.iter())
            .map(|v| {
                (
                    v.chapter,
                    v.verse,
                    v.text.as_str(),
                    v.heading.as_deref(),
                    v.paragraph,
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                (
                    3,
                    16,
                    "For God so loved the world, that he gave.",
                    Some("Nicodemus"),
                    true
                ),
                (3, 17, "For God sent not his Son.", None, false),
                (4, 1, "When therefore the Lord knew.", None, true),
            ]
        );
        // Lines of poetry start lines, a stanza a paragraph; the text is the hashed text
        let psalm = r#"<usx version="3.0"><book code="PSA" style="id"/><chapter number="1" style="c"/>
<para style="q1"><verse number="1" style="v"/>Blessed is the man</para><para style="q2">who walks not</para>
<para style="q1"><verse number="2" style="v"/>But his delight</para>
<para style="b"/>
<para style="q1"><verse number="3" style="v"/>He is like a tree</para></usx>"#;
        let files = Arc::new(MemoryFiles::with([("PSA.usx", psalm)]));
        let source = open(files, TextFormat::Usx, "").unwrap();
        let id = crate::bible_data().books().search_usfm("PSA").unwrap();
        let book = source.book(id).unwrap();
        let verses = verse_texts(&book);
        let layout: Vec<_> = (verses.iter())
            .map(|v| (v.verse, v.text.as_str(), v.paragraph, v.line))
            .collect();
        assert_eq!(
            layout,
            [
                (1, "Blessed is the man who walks not", true, true),
                (2, "But his delight", false, true),
                (3, "He is like a tree", true, true),
            ]
        );
        let hashed = crate::hash::book_hashes(&book, crate::hash::HashRule::One);
        assert_eq!(hashed[0].hash, crate::hash::verse_hash(&verses[0].text));
    }

    #[test]
    fn a_heading_holding_its_verses_marker_goes_with_that_verse() {
        // BSB's Psalm 3: the superscription's paragraph holds verse 1's marker
        let psalm = r#"<usx version="3.0"><book code="PSA" style="id"/><chapter number="3" style="c"/>
<para style="s1">Deliver Me, O LORD!</para>
<para style="d"><verse number="1" style="v" sid="PSA 3:1"/>A Psalm of David.</para>
<para style="q1" vid="PSA 3:1">O LORD, how my foes have increased!<verse eid="PSA 3:1"/></para>
<para style="q1"><verse number="2" style="v" sid="PSA 3:2"/>Many say of me,<verse eid="PSA 3:2"/></para>
<para style="s1">A Mid-Verse Heading</para>
<para style="q1"><verse number="3" style="v" sid="PSA 3:3"/>But You, O LORD<verse eid="PSA 3:3"/></para></usx>"#;
        let files = Arc::new(MemoryFiles::with([("PSA.usx", psalm)]));
        let source = open(files, TextFormat::Usx, "").unwrap();
        let id = crate::bible_data().books().search_usfm("PSA").unwrap();
        let verses = verse_texts(&source.book(id).unwrap());
        let headings: Vec<_> = (verses.iter())
            .map(|v| (v.verse, v.heading.as_deref()))
            .collect();
        assert_eq!(
            headings,
            [
                (1, Some("Deliver Me, O LORD!\nA Psalm of David.")),
                (2, None),
                (3, Some("A Mid-Verse Heading")),
            ]
        );
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
        // The first or last verses, each part still a blank line apart
        let first = |n| {
            let verses = Verses::First(n);
            text_verses(&book, &passage("John 3:16-17; 4:1"), &style, "John", verses)
        };
        assert_eq!(
            first(1).unwrap().text,
            "Nicodemus\n[16] For God so loved the world, that he gave."
        );
        assert!(
            first(3)
                .unwrap()
                .text
                .ends_with("Son.\n\n[1] When therefore the Lord knew.")
        );
        let last = Verses::Last(2);
        let last = text_verses(&book, &passage("John 3:16-17; 4:1"), &style, "John", last).unwrap();
        assert_eq!(
            last.text,
            "[17] For God sent not his Son.\n\n[1] When therefore the Lord knew."
        );
        assert!(first(0).is_none());
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
    fn a_links_words_are_found_again() {
        let text = "For God so loved the world, that he gave his only begotten Son.";
        let at = |fragment: &str| find_fragment(text, fragment).map(|r| &text[r]);
        // The same words, and the same words differently cased and punctuated
        assert_eq!(at("so loved the world"), Some("so loved the world"));
        assert_eq!(at("SO LOVED, the World"), Some("so loved the world"));
        // A word changed in the middle, or added
        assert_eq!(at("so loved this world"), Some("so loved the world"));
        assert_eq!(
            at("gave his one and only begotten Son"),
            Some("gave his only begotten Son")
        );
        // Too few of the words: nothing
        assert_eq!(at("gave them bread"), None);
        assert_eq!(at(""), None);
        // Accents don't lose a word
        assert_eq!(
            find_fragment("Dios amó al mundo", "dios amo al mundo").map(|r| r.start),
            Some(0)
        );
    }

    #[test]
    fn wraps_at_spaces() {
        assert_eq!(wrap("aa bb cc dd\nee ff", 5), "aa bb\ncc dd\nee ff");
        assert_eq!(wrap("a verylongword b", 4), "a\nverylongword\nb");
        assert_eq!(wrap("“é” ü ö", 3), "“é”\nü ö");
    }
}
