/*!
What the package manager asks of a package's text, and nothing more: the facts a summary
records ([`TextFacts`]), each book's verse counts, and whether its media timings fit. It never
reads the text itself: a [`TextReader`] does, given by whoever runs it (lx, the bindings, the
language server), so the package manager builds without a parser (`--no-default-features`)
and is tested with facts made up. The `read` feature's [`DefaultReader`] reads through
logoxide-text, the crate that knows what's in a Bible or a book and nothing of packages.

The summary file is the contract between the two sides: a reader makes the facts, and the
package manager diffs and versions them ([`crate::summary`]).
*/

use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use topos_bible::data::books::BookId;

use crate::{
    error::Result,
    package::Package,
    references::Cited,
    summary::{HashRule, VerseHash},
};

/// What a package's text gives its summary: the half a reader makes (the package manager adds
/// checksums, sizes, media, and the version)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextFacts {
    /// Each book's verse counts, by USFM code, in the text's order
    pub books: IndexMap<String, Vec<u8>>,
    /// How many of each kind of element (`note`, `title`, `words-of-jesus`, ...), and `strongs`
    pub contents: BTreeMap<String, u64>,
    /// Every verse and the hash of its words, made by the rule asked for
    pub verses: Vec<VerseHash>,
    /// Each file's title, by path, for the files that have one
    pub titles: BTreeMap<String, String>,
    /// The files that should have a title and don't (a release refuses them)
    pub untitled: Vec<String>,
    /// The passages its books (EPUBs) cite, each once with how often
    pub cited: Vec<Cited>,
    /// How many Bible references its books have
    pub references: u64,
    /// For a versification map, how many verses it maps
    pub mappings: u64,
    /// Files, or books in them, that were passed over, and why
    pub skipped: Vec<Skipped>,
}

/// Reads a package's text for the package manager. Every method takes the package's files and
/// manifest ([`Package`]) and gives back facts; nothing parsed crosses the boundary
pub trait TextReader: Send + Sync {
    /// Each book's verse counts, in the text's order (cheaper than [`facts`](Self::facts): no
    /// hashing, no books read for references)
    fn counts(&self, package: &Package) -> Result<Vec<(BookId, Vec<u8>)>>;

    /// Everything a summary records of the text, its verses hashed by `rule`
    fn facts(&self, package: &Package, rule: HashRule) -> Result<TextFacts>;

    /// What's wrong with the package's media timings against its text, or against `text`, the
    /// package `[media] text` names, as installed (see
    /// [`crate::media::check_package_timings_with`]); empty when all is well
    fn timing_problems(&self, package: &Package, text: Option<&Package>) -> Result<Vec<String>>;
}

/// A reader that reads nothing: a package has no text to it. For tests of the package manager
/// with facts made up, and hosts that only install
#[derive(Clone, Copy, Debug, Default)]
pub struct NoReader;

impl TextReader for NoReader {
    fn counts(&self, _: &Package) -> Result<Vec<(BookId, Vec<u8>)>> {
        Ok(vec![])
    }

    fn facts(&self, _: &Package, _: HashRule) -> Result<TextFacts> {
        Ok(TextFacts::default())
    }

    fn timing_problems(&self, _: &Package, _: Option<&Package>) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

/// Why a file or a book in one was passed over
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// Front or back matter, a glossary, an index, and the like: not a book of the Bible, so
    /// not read as one
    Peripheral,
    /// A book topos doesn't know (yet): warned of, since its verses are left out
    Unknown,
    /// A file with the format's extension that names no book, by its name or its text
    NoBook,
    /// Verses in a lettered chapter (Esther's Greek additions, `Esth.A.1`), which the format
    /// hasn't: chapters are numbers
    LetteredChapter,
}

/// A file, or a book in one, that a reader passed over
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    /// The book's code or name as the file gives it (`FRT`, `JUB`, OSIS's `1En`); empty for
    /// [`SkipReason::NoBook`]
    pub book: String,
    pub reason: SkipReason,
}

impl Skipped {
    /// Whether it's worth a warning (a book whose verses are left out, or a file naming none),
    /// not only a note
    pub fn is_warning(&self) -> bool {
        self.reason != SkipReason::Peripheral
    }
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (path, book) = (&self.path, &self.book);
        match self.reason {
            SkipReason::Peripheral => {
                write!(f, "{path}: {book} isn't a book of the Bible, passed over")
            }
            SkipReason::Unknown => {
                write!(
                    f,
                    "{path}: {book} is a book logoxide doesn't know, passed over"
                )
            }
            SkipReason::NoBook => write!(f, "{path}: names no book, passed over"),
            SkipReason::LetteredChapter => write!(
                f,
                "{path}: {book} has lettered chapters (A to F), which logoxide doesn't read, passed over"
            ),
        }
    }
}

#[cfg(feature = "read")]
pub(crate) use with_text::verse_hashes;
#[cfg(feature = "read")]
pub use with_text::{DefaultReader, StorageBooks, StorageFiles, versification_map};

/// The reader built on logoxide-text, and the bridges from a package's [`Storage`] to its
/// [`Files`](logoxide_text::Files) and from a cache's to its book store
#[cfg(feature = "read")]
mod with_text {
    use std::sync::Arc;

    use logoxide_text::{Files, TextError, hash, source::TextFormat};
    use topos_bible::data::books::BookId;

    use super::{SkipReason, Skipped, TextFacts, TextReader};
    use crate::{
        books::Book,
        error::{LogoxideError, Result},
        manifest::{Format, PackageKind},
        package::Package,
        storage::Storage,
        summary::{HashRule, VerseHash},
    };

    /// A package's storage as the files logoxide-text reads
    pub struct StorageFiles(pub Arc<dyn Storage>);

    impl Files for StorageFiles {
        fn read(&self, path: &str) -> logoxide_text::Result<Vec<u8>> {
            self.0.read(path).map_err(files_error)
        }

        fn list(&self, dir: &str) -> logoxide_text::Result<Vec<String>> {
            self.0.list(dir).map_err(files_error)
        }

        fn walk(&self, dir: &str) -> logoxide_text::Result<Vec<String>> {
            self.0.walk(dir).map_err(files_error)
        }

        fn exists(&self, path: &str) -> bool {
            self.0.exists(path)
        }

        fn read_string(&self, path: &str) -> logoxide_text::Result<String> {
            self.0.read_string(path).map_err(files_error)
        }
    }

    /// A cache's storage as the store logoxide-text's book cache writes to
    /// ([`BookStore`](logoxide_text::cache::BookStore)), paths as the cache names them
    /// (`<key>/JHN.postcard` under the storage's root)
    pub struct StorageBooks(pub Arc<dyn Storage>);

    impl logoxide_text::cache::BookStore for StorageBooks {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.0.read(path).ok()
        }

        fn write(&self, path: &str, bytes: &[u8]) -> logoxide_text::Result<()> {
            self.0.write(path, bytes).map_err(files_error)
        }
    }

    fn files_error(e: LogoxideError) -> TextError {
        match e {
            LogoxideError::NotFound(what) => TextError::NotFound(what),
            other => TextError::Files(other.to_string()),
        }
    }

    impl From<TextError> for LogoxideError {
        fn from(e: TextError) -> Self {
            match e {
                TextError::Book { book, message } => LogoxideError::Book { book, message },
                TextError::NotFound(what) => LogoxideError::NotFound(what),
                TextError::Files(what) => LogoxideError::Io {
                    path: String::new(),
                    message: what,
                },
                TextError::Versification(what) => LogoxideError::Versification(what),
                TextError::Timings(what) => LogoxideError::Media(what),
                TextError::Archive(what) => LogoxideError::Import(what),
            }
        }
    }

    impl From<logoxide_text::Skipped> for Skipped {
        fn from(s: logoxide_text::Skipped) -> Self {
            Skipped {
                path: s.path,
                book: s.book,
                reason: match s.reason {
                    logoxide_text::SkipReason::Peripheral => SkipReason::Peripheral,
                    logoxide_text::SkipReason::Unknown => SkipReason::Unknown,
                    logoxide_text::SkipReason::NoBook => SkipReason::NoBook,
                    logoxide_text::SkipReason::LetteredChapter => SkipReason::LetteredChapter,
                },
            }
        }
    }

    impl Format {
        /// The format as logoxide-text reads it, if it reads it
        pub fn text_format(self) -> Option<TextFormat> {
            Some(match self {
                Format::Usx => TextFormat::Usx,
                Format::Usfm => TextFormat::Usfm,
                Format::Osis => TextFormat::Osis,
                Format::Thml => TextFormat::Thml,
                _ => return None,
            })
        }
    }

    /// The rule as logoxide-text knows it (every rule the summary format names, it knows)
    fn text_rule(rule: HashRule) -> Result<hash::HashRule> {
        hash::HashRule::from_number(rule.number()).ok_or_else(|| {
            LogoxideError::Newer(format!(
                "verse-hash rule {}: logoxide-text doesn't know it",
                rule.number()
            ))
        })
    }

    /// Each verse of a book and the hash of its words by `rule`, as `logoxide.verses.bin` has
    /// them; a book without a number in logoxide's table has none (every book topos knows has
    /// one)
    pub(crate) fn verse_hashes(
        book: &logoxide_text::Book,
        rule: HashRule,
    ) -> Result<Vec<VerseHash>> {
        let rule = text_rule(rule)?;
        Ok((hash::book_hashes(book, rule).into_iter())
            .filter_map(|r| {
                Some(VerseHash {
                    book: Book::from_book_id(r.book)?,
                    chapter: r.chapter,
                    verse: r.verse,
                    last: r.last,
                    part: r.part,
                    hash: r.hash,
                })
            })
            .collect())
    }

    /// The map in a package (`kind = "versification-map"`): every `.vrs` file of its enabled
    /// formats, in path order
    pub fn versification_map(
        package: &Package,
    ) -> Result<logoxide_text::versification::VersificationMap> {
        let info = package.info();
        let (Some(to), PackageKind::VersificationMap) = (&info.maps_to, info.kind) else {
            return Err(LogoxideError::Versification(format!(
                "{} isn't a versification map (kind = \"versification-map\", with maps-to)",
                info.id
            )));
        };
        let storage = package.storage();
        let dirs: Vec<&str> = (package.manifest().formats(package.enabled()).into_iter())
            .filter(|(name, _)| Format::from_name(name) == Some(Format::Vrs))
            .map(|(_, entry)| entry.path.trim_end_matches('/'))
            .collect();
        let mut files = vec![];
        for path in crate::package::declared_files(storage.as_ref())? {
            let inside = (dirs.iter()).any(|d| path == *d || path.starts_with(&format!("{d}/")));
            if inside && path.to_ascii_lowercase().ends_with(".vrs") {
                let text = storage.read_string(&path)?;
                files.push((path, text));
            }
        }
        if files.is_empty() {
            return Err(LogoxideError::Versification(format!(
                "{}: no .vrs file in its formats",
                info.id
            )));
        }
        let files = files.iter().map(|(p, t)| (p.as_str(), t.as_str()));
        Ok(logoxide_text::versification::VersificationMap::parse(
            &info.versification,
            to,
            files,
        )?)
    }

    /// The reader built in with the `read` feature: logoxide-text's sources, hashes, titles,
    /// and citations
    #[derive(Clone, Copy, Debug, Default)]
    pub struct DefaultReader;

    impl TextReader for DefaultReader {
        fn counts(&self, package: &Package) -> Result<Vec<(BookId, Vec<u8>)>> {
            let source = package.source();
            (source.books().into_iter())
                .map(|id| Ok((id, source.chapter_verses(id)?)))
                .collect()
        }

        fn facts(&self, package: &Package, rule: HashRule) -> Result<TextFacts> {
            let source = package.source();
            let mut facts = TextFacts::default();
            for id in source.books() {
                let book = source.book(id)?;
                let code = crate::bible_data()
                    .books()
                    .get_usfm(id)
                    .cloned()
                    .unwrap_or_else(|| id.0.to_string());
                facts.books.insert(code, book.chapter_verses());
                hash::count_elements(&book.nodes, &mut facts.contents);
                facts.verses.extend(verse_hashes(&book, rule)?);
            }
            for titled in crate::titles::titles(package)? {
                match titled.title {
                    Some(title) => {
                        facts.titles.insert(titled.path, title);
                    }
                    None => facts.untitled.push(titled.path),
                }
            }
            let found = crate::references::find(package)?;
            facts.references = found.len() as u64;
            facts.cited = crate::references::cited(&found);
            if package.info().kind == PackageKind::VersificationMap {
                facts.mappings = versification_map(package)?.len() as u64;
            }
            facts.skipped = (source.skipped().iter()).cloned().map(Into::into).collect();
            Ok(facts)
        }

        fn timing_problems(
            &self,
            package: &Package,
            text: Option<&Package>,
        ) -> Result<Vec<String>> {
            crate::media::check_package_timings_with(package, text)
        }
    }
}
