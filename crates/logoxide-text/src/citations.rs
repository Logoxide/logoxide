/*!
The Bible references in a book, found by topos in each EPUB: every reference, with where it is
(an EPUB CFI readers open straight to, its place in the book's text, its chapter, its paragraph).
About a tenth of a second a book natively, a quarter in WebAssembly.

A book in ThML (CCEL's) is searched the same way: its text nodes joined with `\n`, as topos
searches XML, each reference with its place in that text and the title of the division it's in
for its chapter (no CFI: there's no EPUB to open).
*/

use logoxide_markup::{Markup, Node};
use serde::{Deserialize, Serialize};
use topos_bible::matcher::{BibleMatcher, matches::ComplexFilter};
use topos_bible_formats::epub::{CfiOptions, search_epub_reader};

use crate::{
    Files,
    error::{Result, TextError},
};

/// A reference found in a book
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// The passage, as an OSIS reference (`John.3.16`, `John.3.16-John.3.18`)
    pub passage: String,
    /// The file it's in (`epub/the-dorean-principle.epub`)
    pub file: String,
    /// Where it is in the file: an EPUB CFI range (empty in ThML)
    pub cfi: String,
    /// The chapter it's in, when the book's table of contents names one (in ThML, the title of
    /// the division it's in)
    pub chapter: Option<String>,
    /// Its UTF-16 range in the book's text (each content document's text, a blank line apart,
    /// as topos's index and EPUB++ count it; in ThML, its text nodes a line apart)
    pub start: u32,
    pub end: u32,
    /// The 1-based line of the book's text it starts on, and its 1-based UTF-16 column there
    pub line: u32,
    pub column: u32,
    /// Its content document's place in the spine (0 in ThML)
    pub spine: u32,
    /// The line (paragraph) it starts in
    pub line_text: String,
}

/// Whether a book's file is ThML (`.xml`, `.thml`), else an EPUB
pub fn is_thml(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.ends_with(".thml")
}

/// Every reference in the books at `books` (EPUBs, and ThML files: [`is_thml`]), file by file in
/// the order given, each file's in the order they're in it
pub fn find(files: &dyn Files, books: &[String]) -> Result<Vec<Citation>> {
    if books.is_empty() {
        return Ok(vec![]);
    }
    let data = crate::bible_data();
    let matcher = BibleMatcher::new(data.clone(), ComplexFilter::default());
    let mut found = vec![];
    for file in books {
        let bytes = files.read(file)?;
        if is_thml(file) {
            found.extend(thml(&matcher, file, &String::from_utf8_lossy(&bytes)));
            continue;
        }
        let matches =
            search_epub_reader(&matcher, std::io::Cursor::new(bytes), CfiOptions::default())
                .map_err(|e| TextError::Book {
                    book: file.clone(),
                    message: e.to_string(),
                })?;
        for m in matches {
            let Some(passage) = m.psg.to_osis(data.books()) else {
                continue;
            };
            let at = m.location;
            let whole = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            found.push(Citation {
                passage,
                file: file.clone(),
                cfi: at.cfi,
                chapter: at.chapter,
                start: whole(at.text_utf16.start),
                end: whole(at.text_utf16.end),
                line: whole(at.line),
                column: whole(at.utf16_column),
                spine: whole(at.spine_index),
                line_text: at.line_text,
            });
        }
    }
    Ok(found)
}

/// A ThML book's text: its text nodes joined with `\n` (the head left out), where each starts,
/// and the division it's in, by its title's index in `titles`
#[derive(Default)]
struct Joined {
    text: String,
    nodes: Vec<(usize, Option<usize>)>,
    titles: Vec<String>,
}

impl Joined {
    fn add(&mut self, nodes: &[Node], division: Option<usize>) {
        for node in nodes {
            match node {
                Node::Text(text) => {
                    if !self.text.is_empty() {
                        self.text.push('\n');
                    }
                    self.nodes.push((self.text.len(), division));
                    self.text.push_str(text);
                }
                Node::Element(e) => {
                    let name = e.name.to_ascii_lowercase();
                    if name == "thml.head" {
                        continue;
                    }
                    let divides = name.len() == 4
                        && name.starts_with("div")
                        && name.as_bytes()[3].is_ascii_digit();
                    let title =
                        (e.attr("title").map(str::trim)).filter(|t| divides && !t.is_empty());
                    let inside = match title {
                        Some(title) => {
                            self.titles.push(title.to_string());
                            Some(self.titles.len() - 1)
                        }
                        None => division,
                    };
                    self.add(&e.children, inside);
                }
            }
        }
    }

    /// The title of the division the text at `offset` is in
    fn chapter_at(&self, offset: usize) -> Option<String> {
        let at = self.nodes.partition_point(|(start, _)| *start <= offset);
        let (_, division) = self.nodes.get(at.checked_sub(1)?)?;
        self.titles.get((*division)?).cloned()
    }
}

/// Every reference in a ThML book's text ([`Joined`]), in order
fn thml(matcher: &BibleMatcher, file: &str, xml: &str) -> Vec<Citation> {
    let mut joined = Joined::default();
    joined.add(&logoxide_markup::parse(Markup::Thml, xml), None);
    let text = joined.text.as_str();
    let data = crate::bible_data();
    let whole = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut matches = matcher.search(text);
    matches.sort_by_key(|m| m.location.bytes.start);
    // Counted as it goes, the matches being in order: the UTF-16 length of the text before a
    // place, and of its line before it
    let (mut at, mut utf16, mut line, mut line_start) = (0, 0, 1, 0);
    let mut advance = |to: usize| {
        let mut to = to.clamp(at, text.len());
        while !text.is_char_boundary(to) {
            to -= 1;
        }
        for c in text[at..to].chars() {
            utf16 += c.len_utf16();
            if c == '\n' {
                line += 1;
                line_start = utf16;
            }
        }
        at = to;
        (utf16, line, line_start, at)
    };
    let mut found = vec![];
    for m in matches {
        let Some(passage) = m.psg.to_osis(data.books()) else {
            continue;
        };
        let bytes = m.location.bytes;
        let (start, line, line_start, at) = advance(bytes.start);
        let length: usize =
            (text.get(at..bytes.end.max(at))).map_or(0, |t| t.chars().map(char::len_utf16).sum());
        let line_begins = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_ends = text[at..].find('\n').map_or(text.len(), |i| at + i);
        found.push(Citation {
            passage,
            file: file.to_string(),
            cfi: String::new(),
            chapter: joined.chapter_at(at),
            start: whole(start),
            end: whole(start + length),
            line: whole(line),
            column: whole(start - line_start + 1),
            spine: 0,
            line_text: text[line_begins..line_ends].trim().to_string(),
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemoryFiles;

    #[test]
    fn references_in_a_thml_book() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ThML><ThML.head><DC><DC.Title>A Book on John 1:1</DC.Title></DC></ThML.head>
<ThML.body>
<div1 title="Book First">
<div2 title="Chapter 1. Of God">
<p>As it is written (<scripRef passage="Ps 8:4" parsed="|Ps|8|4|0|0">Ps. 8:4</scripRef>), and
in Romans 12:6–8 we read.</p>
</div2>
</div1>
<div1 title="Book Second"><p>See é John 3:16.</p></div1>
</ThML.body></ThML>"#;
        let files = MemoryFiles::with([("thml/book.xml", xml)]);
        let found = find(&files, &[String::from("thml/book.xml")]).unwrap();
        let passages: Vec<&str> = found.iter().map(|c| c.passage.as_str()).collect();
        assert_eq!(passages, ["Ps.8.4", "Rom.12.6-Rom.12.8", "John.3.16"]);
        let first = &found[0];
        assert_eq!(first.chapter.as_deref(), Some("Chapter 1. Of God"));
        assert_eq!((first.cfi.as_str(), first.spine), ("", 0));
        assert_eq!(first.line_text, "Ps. 8:4");
        let last = &found[2];
        assert_eq!(last.chapter.as_deref(), Some("Book Second"));
        assert_eq!(last.line_text, "See é John 3:16.");
        // Its UTF-16 place in the joined text, and its column on its line
        assert_eq!(last.column, 7);
        assert_eq!(last.end - last.start, "John 3:16".len() as u32);
        assert!(is_thml("thml/BOOK.XML") && !is_thml("epub/book.epub"));
    }
}
