/*!
A Bible from loose files, with no manifest to say their format: whatever a reader dropped in or
a publisher zipped, as apps take them. Each file's format is found by [`TextFormat::detect`], a
`.zip`'s files are read as if they were loose, and each format's files are read as its source
reads them (a whole-Bible OSIS or ThML file cut into its books). A book in files of several
formats is read from the best one (USX, USFM, USJ, OSIS, ThML, GBF); everything else is passed
over and said ([`Source::skipped`]).
*/

use std::{collections::BTreeMap, sync::Arc};

use topos_bible::data::books::BookId;

use super::{Book, SkipReason, Skipped, Source, TextFormat, open};
use crate::{
    archive::unzip,
    error::{Result, TextError},
    files::{Files, MemoryFiles},
};

/// A Bible read from loose files ([`open_loose`])
pub struct LooseSource {
    /// Each format's source, best first
    sources: Vec<Box<dyn Source>>,
    /// Each book's file, as its source names it (`usx/JHN.usx`)
    files: BTreeMap<BookId, String>,
    /// Which of `sources` reads each book
    reader: BTreeMap<BookId, usize>,
    skipped: Vec<Skipped>,
    /// The path each file was given as, by the path its source names it by
    given: BTreeMap<String, String>,
}

/**
The Bible in `files` under `dir` (every file at any depth): each file's format found from its
name and how it starts, `.zip`s opened, and each book read from its best format. Files that are
no format read, and books another file has in a better format, are passed over and said. An
error only when nothing can be read, or a format's files have a book twice
*/
pub fn open_loose(files: &dyn Files, dir: &str) -> Result<LooseSource> {
    let mut by_format: BTreeMap<TextFormat, BTreeMap<String, Vec<u8>>> = BTreeMap::new();
    let mut given = BTreeMap::new();
    let mut skipped = vec![];
    let mut pending: Vec<(String, Vec<u8>)> = vec![];
    for path in files.walk(dir)? {
        let bytes = files.read(&path)?;
        if path.to_ascii_lowercase().ends_with(".zip") {
            let inside = unzip(&bytes)?;
            pending.extend(inside.into_iter().map(|(p, b)| (format!("{path}/{p}"), b)));
        } else {
            pending.push((path, bytes));
        }
    }
    for (path, bytes) in pending {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        // macOS's resource forks and hidden files
        if name.starts_with('.') || path.split('/').any(|part| part == "__MACOSX") {
            continue;
        }
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
        let Some(format) = TextFormat::detect(&name, &head).filter(|f| f.is_verse_text()) else {
            skipped.push(Skipped {
                path,
                book: String::new(),
                reason: SkipReason::NotText,
            });
            continue;
        };
        // Its name in its format's folder, with the extension the format's source reads
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem);
        let inner = format!("{}/{stem}.{}", format.name(), format.name());
        let folder = by_format.entry(format).or_default();
        if folder.contains_key(&inner) {
            skipped.push(Skipped {
                path,
                book: String::new(),
                reason: SkipReason::Duplicate,
            });
            continue;
        }
        folder.insert(inner.clone(), bytes);
        given.insert(inner, path);
    }
    let mut source = LooseSource {
        sources: vec![],
        files: BTreeMap::new(),
        reader: BTreeMap::new(),
        skipped: vec![],
        given,
    };
    for (format, folder) in by_format {
        let read = open(Arc::new(MemoryFiles::with(folder)), format, format.name())
            .map_err(|e| source.named(e))?;
        let index = source.sources.len();
        for s in read.skipped() {
            let path = source.given_path(&s.path);
            skipped.push(Skipped { path, ..s.clone() });
        }
        for (&id, file) in read.files() {
            if source.files.contains_key(&id) {
                // Read from a better format's file
                let book = crate::bible_data().books().get_usfm(id).cloned();
                skipped.push(Skipped {
                    path: source.given_path(file),
                    book: book.unwrap_or_else(|| id.0.to_string()),
                    reason: SkipReason::Duplicate,
                });
                continue;
            }
            source.files.insert(id, file.clone());
            source.reader.insert(id, index);
        }
        source.sources.push(read);
    }
    source.skipped = skipped;
    if source.files.is_empty() {
        return Err(TextError::NotFound(String::from(
            "no book of the Bible in these files (USX, USFM, USJ, OSIS, ThML, or GBF)",
        )));
    }
    Ok(source)
}

impl LooseSource {
    /// The path a file was given as, from the path its source names it by
    pub fn given_path(&self, path: &str) -> String {
        self.given
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.to_string())
    }

    /// The format a book is read from
    pub fn book_format(&self, id: BookId) -> Option<TextFormat> {
        let index = *self.reader.get(&id)?;
        self.sources.get(index)?.format()
    }

    /// An error naming files as they were given
    fn named(&self, error: TextError) -> TextError {
        let rename = |mut message: String| {
            for (inner, path) in &self.given {
                message = message.replace(inner.as_str(), path);
            }
            message
        };
        match error {
            TextError::Book { book, message } => TextError::Book {
                book,
                message: rename(message),
            },
            TextError::NotFound(message) => TextError::NotFound(rename(message)),
            TextError::Files(message) => TextError::Files(rename(message)),
            error => error,
        }
    }
}

impl Source for LooseSource {
    /// The format most of its books are read from
    fn format(&self) -> Option<TextFormat> {
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for index in self.reader.values() {
            *counts.entry(*index).or_default() += 1;
        }
        let (&index, _) = counts.iter().max_by_key(|(_, n)| **n)?;
        self.sources.get(index)?.format()
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn book(&self, id: BookId) -> Result<Book> {
        let index =
            (self.reader.get(&id)).ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
        let source = (self.sources.get(*index))
            .ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
        source.book(id).map_err(|e| self.named(e))
    }

    fn chapter_verses(&self, id: BookId) -> Result<Vec<u8>> {
        let index =
            (self.reader.get(&id)).ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
        let source = (self.sources.get(*index))
            .ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
        source.chapter_verses(id)
    }

    fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn usx(code: &str, words: &str) -> String {
        format!(
            r#"<usx version="3.0"><book code="{code}" style="id"/><chapter number="1" style="c"/><para style="p"><verse number="1" style="v"/>{words}</para></usx>"#
        )
    }

    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, text) in files {
            let options = zip::write::SimpleFileOptions::default();
            out.start_file(*name, options).unwrap();
            out.write_all(text.as_bytes()).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    fn id(code: &str) -> BookId {
        crate::bible_data().books().search_usfm(code).unwrap()
    }

    #[test]
    fn loose_files_of_several_formats() {
        let osis = r#"<osis><osisText><div type="book" osisID="Phlm"><chapter osisID="Phlm.1"><verse osisID="Phlm.1.1">Paul, a prisoner</verse></chapter></div><div type="book" osisID="Jude"><chapter osisID="Jude.1"><verse osisID="Jude.1.1">Jude, a servant</verse></chapter></div></osisText></osis>"#;
        let files = MemoryFiles::with([
            ("drop/John.xml", usx("JHN", "In the beginning").into_bytes()),
            // John again in a worse format: passed over for the USX
            (
                "drop/43-JHN.usfm",
                b"\\id JHN\n\\c 1\n\\p\n\\v 1 In the start".to_vec(),
            ),
            ("drop/bible.xml", osis.as_bytes().to_vec()),
            (
                "drop/romans.zip",
                zip(&[("usfm/ROM.SFM", "\\id ROM\n\\c 1\n\\p\n\\v 1 Paul")]),
            ),
            ("drop/notes.md", b"# Notes".to_vec()),
            ("drop/__MACOSX/._John.xml", b"junk".to_vec()),
            ("drop/.DS_Store", b"junk".to_vec()),
        ]);
        let source = open_loose(&files, "drop").expect("opens");
        assert_eq!(source.books(), [id("JHN"), id("ROM"), id("PHM"), id("JUD")]);
        assert_eq!(source.book_format(id("JHN")), Some(TextFormat::Usx));
        assert_eq!(source.book_format(id("ROM")), Some(TextFormat::Usfm));
        // The whole-Bible file's books apart
        let jude = source.book(id("JUD")).unwrap();
        assert_eq!(
            crate::hash::verse_text(&jude.nodes, crate::hash::HashRule::One),
            "Jude, a servant"
        );
        assert_eq!(source.book(id("JHN")).unwrap().spans.len(), 1);
        let skipped: Vec<String> = source.skipped().iter().map(|s| s.to_string()).collect();
        assert_eq!(
            skipped,
            [
                "drop/notes.md: not a Bible's text (USX, USFM, USJ, OSIS, ThML, or GBF), passed over",
                "drop/43-JHN.usfm: JHN is read from another file, passed over",
            ]
        );
        assert!(source.skipped()[0].is_warning() && !source.skipped()[1].is_warning());
    }

    #[test]
    fn nothing_to_read() {
        let files = MemoryFiles::with([("a/notes.md", "# Notes")]);
        assert!(matches!(
            open_loose(&files, "a"),
            Err(TextError::NotFound(_))
        ));
    }
}
