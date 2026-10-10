/*!
Each file's title: what a reader sees for a Bible book or a book (`John`, `The Dorean
Principle`), apart from its file name (`usx/JHN.usx`). Hosts name what they show by it (the
Obsidian plugin's stubs, `Logoxide/bsb/John.bible`), so a package's titles are part of what it
publishes: its summary lists them, and a changed title is a minor change (a host may rename what
it shows, but nothing it points at is gone).

A title is, in order:
- what `[titles]` in `logoxide.toml` gives the file (`"usx/JHN.usx" = "John"`)
- what the file says (logoxide-text reads it): a Bible book's short name (USFM's `\toc2`, else
  `\h`, else `\toc1`, in USX the paragraphs of those styles), an EPUB's `dc:title`
- for a Bible book, its name in English (`John`); a file holding many books (a whole OSIS or ThML
  Bible) is titled by the package's name

A file with none of these has no title, which `lx check` and `lx release` refuse.

```toml
[titles]
"usx/JHN.usx" = "Juan"
```
*/

use std::collections::BTreeMap;

use crate::{error::Result, package::Package, reader::StorageFiles};

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
    let files = StorageFiles(storage.clone());
    let mut found: BTreeMap<String, Option<String>> = BTreeMap::new();
    for file in logoxide_text::titles::book_file_titles(package.source(), &files)? {
        // A file of many books is named for the package
        let title = match file.books > 1 {
            true => Some(package.info().name.clone()),
            false => file.title,
        };
        found.insert(file.path, title);
    }
    for path in crate::references::epub_files(package)? {
        let title = logoxide_text::titles::epub_title(&storage.read(&path)?);
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
