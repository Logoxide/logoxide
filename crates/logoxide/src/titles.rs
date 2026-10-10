/*!
Each file's title: what a reader sees for a Bible book or a book (`John`, `The Dorean
Principle`), apart from its file name (`usx/JHN.usx`). Hosts name what they show by it (the
Obsidian plugin's stubs, `Logoxide/bsb/John.bible`), so a package's titles are part of what it
publishes: its summary lists them, and a changed title is a major change.

A title is, in order:
- what `[titles]` in `logoxide.toml` gives the file (`"usx/JHN.usx" = "John"`)
- what the file says: a Bible book's short name (USFM's `\toc2`, else `\h`, else `\toc1`, in USX
  the paragraphs of those styles), an EPUB's `dc:title`
- for a Bible book, its name in English (`John`); a file holding many books (a whole OSIS or ThML
  Bible) is titled by the package's name

A file with none of these has no title, which `lx check` and `lx release` refuse.

```toml
[titles]
"usx/JHN.usx" = "Juan"
```
*/

use std::collections::BTreeMap;

use topos_bible::data::books::BookId;

use crate::{error::Result, package::Package};

/// A file a reader sees, and its title (none when nothing gives one)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Titled {
    /// Relative to the package's folder: `usx/JHN.usx`, `epub/book.epub`
    pub path: String,
    pub title: Option<String>,
}

/// The title of each Bible book's file and each EPUB, in path order
pub fn titles(package: &Package) -> Result<Vec<Titled>> {
    let given = &package.manifest().titles;
    let storage = package.storage();
    let mut found: BTreeMap<String, Option<String>> = BTreeMap::new();
    // Bible books: a file can hold one book or many
    let mut books_in: BTreeMap<&String, Vec<BookId>> = BTreeMap::new();
    for (id, path) in package.source().files() {
        books_in.entry(path).or_default().push(*id);
    }
    for (path, books) in books_in {
        let title = match books.as_slice() {
            [id] => {
                let text = String::from_utf8_lossy(&storage.read(path)?).into_owned();
                own_title(&text).or_else(|| crate::bible_data().books().get_name(*id).cloned())
            }
            _ => Some(package.info().name.clone()),
        };
        found.insert(path.to_string(), title);
    }
    for path in crate::references::epub_files(package)? {
        let title = crate::create::epub_metadata(&storage.read(&path)?)
            .ok()
            .and_then(|m| m.title)
            .filter(|t| !t.trim().is_empty());
        found.insert(path, title);
    }
    Ok(found
        .into_iter()
        .map(|(path, own)| Titled {
            title: given.get(&path).cloned().or(own),
            path,
        })
        .collect())
}

/// The files without a title (to refuse a release with)
pub fn untitled(package: &Package) -> Result<Vec<String>> {
    Ok(titles(package)?
        .into_iter()
        .filter(|t| t.title.is_none())
        .map(|t| t.path)
        .collect())
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
