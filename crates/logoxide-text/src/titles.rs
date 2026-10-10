/*!
Each file's own title: a Bible book's short name (USFM's `\toc2`, else `\h`, else `\toc1`, in USX
the paragraphs of those styles), else its name in English; an EPUB's `dc:title`. The package
manager lays what a manifest says over these.
*/

use std::collections::BTreeMap;

use topos_bible::data::books::BookId;

use crate::{Files, Source, error::Result};

/// A file a reader sees, and what it says it's called
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTitle {
    /// Relative to the root: `usx/JHN.usx`, `epub/book.epub`
    pub path: String,
    /// Its own title, if it gives one (a Bible book without one is named in English)
    pub title: Option<String>,
    /// How many books of the Bible the file holds (0 for an EPUB): a file of many has no title
    /// of its own
    pub books: usize,
}

/// The title of each Bible book's file in `source`, in path order: its own, else the book's
/// English name; a file holding many books (a whole OSIS or ThML Bible) has none of its own
pub fn book_file_titles(source: &dyn Source, files: &dyn Files) -> Result<Vec<FileTitle>> {
    let mut books_in: BTreeMap<&String, Vec<BookId>> = BTreeMap::new();
    for (id, path) in source.files() {
        books_in.entry(path).or_default().push(*id);
    }
    let mut out = vec![];
    for (path, books) in books_in {
        let title = match books.as_slice() {
            [id] => {
                let text = files.read_string(path)?;
                own_title(&text).or_else(|| crate::bible_data().books().get_name(*id).cloned())
            }
            _ => None,
        };
        out.push(FileTitle {
            path: path.clone(),
            title,
            books: books.len(),
        });
    }
    Ok(out)
}

/// An EPUB's title (`dc:title`), if it has one
pub fn epub_title(bytes: &[u8]) -> Option<String> {
    (crate::metadata::epub_metadata(bytes).ok()?.title).filter(|t| !t.trim().is_empty())
}

/**
A Bible book's own short name, from USFM or USX: `\toc2`, else `\h`, else `\toc1` (the
paragraph of that style in USX)
*/
pub fn own_title(text: &str) -> Option<String> {
    ["toc2", "h", "toc1"].iter().find_map(|marker| {
        usfm_field(text, marker)
            .or_else(|| usx_para(text, marker))
            .filter(|t| !t.is_empty())
    })
}

/// A USFM line's text: `\h John` is `John`
fn usfm_field(text: &str, marker: &str) -> Option<String> {
    let start = format!("\\{marker} ");
    text.lines()
        .map(str::trim_start)
        .find_map(|line| line.strip_prefix(&start))
        .map(|t| t.trim().to_string())
}

/// A USX paragraph's text: `<para style="h">John</para>` is `John` (any tags inside dropped)
fn usx_para(text: &str, style: &str) -> Option<String> {
    let open = text.match_indices("<para").map(|(i, _)| i).find(|&i| {
        let tag = &text[i..text[i..].find('>').map_or(text.len(), |e| i + e)];
        tag.contains(&format!("style=\"{style}\"")) || tag.contains(&format!("style='{style}'"))
    })?;
    let body_start = open + text[open..].find('>')? + 1;
    let body = &text[body_start..body_start + text[body_start..].find("</para>")?];
    let mut plain = String::new();
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => plain.push(c),
            _ => {}
        }
    }
    Some(unescape(
        plain.split_whitespace().collect::<Vec<_>>().join(" "),
    ))
}

fn unescape(text: String) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_book_names_itself() {
        let usfm = "\\id JHN\n\\h John\n\\toc1 The Gospel According to John\n\\toc2 John\n\\c 1";
        assert_eq!(own_title(usfm).as_deref(), Some("John"));
        let header_only = "\\id JHN\n\\h Juan\n\\c 1";
        assert_eq!(own_title(header_only).as_deref(), Some("Juan"));
        let usx = r#"<usx><book code="JHN" style="id"/><para style="toc1">The Gospel of John</para><para style="h">John &amp; <char style="w">more</char></para></usx>"#;
        assert_eq!(own_title(usx).as_deref(), Some("John & more"));
        assert_eq!(own_title("<usx><book code=\"JHN\"/></usx>"), None);
    }
}
