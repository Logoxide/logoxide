/*!
A Bible's text: a logoxide package (a folder with a `logoxide.toml`: USX, USFM), or a folder of USX
files (one book each, like `JHN.usx`), the same as the Obsidian plugin reads. Passages render as plain text for hover, and whole books as read-only documents
(`logoxide://BSB/JHN`) that go-to-definition opens.

Several Bibles ([`Bibles`], from the `bible` and `bibles` settings) are translations: a reference
followed by one's name or alias (`John 3:16 KJV`) shows that Bible's text, and is checked against
its own verse counts (read from its USX markers when it loads).

A rendered book knows which verse each part of its text is, so references from it can find
every reference to that verse, and where its verse numbers, chapter labels, words of Jesus, and
so on are, so the editor can color them (semantic tokens, and in hover the code block's info
string, see [`Text::fence`]). The text itself is logoxide-text's (`passage::text` and
`passage::render`, in the `Style` the markup settings make); this only keeps the books, maps the
markup's kinds to the editor's tokens, and names the documents.
*/

use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use logoxide::{
    Cache, CachedPackage, Features, FsStorage, MemoryStorage, Package, Storage,
    manifest::MANIFEST,
    reader::{DefaultReader, StorageBooks, StorageFiles},
};
use logoxide_markup::{
    Flattener, Kind, Node,
    verses::{passage_ranges, slice},
};
pub use logoxide_text::passage::{Part, Rendered};
use logoxide_text::{
    Book, Note, Source,
    cache::BookCache,
    passage,
    source::UsxSource,
    versification::{Mapped, Versifications},
};
use topos_bible::{
    data::{
        bible_data::BibleData,
        books::BookId,
        chapter_verses::ChapterVerses,
        translations::{Translation, Translations},
    },
    segments::Passage,
};

use crate::settings::MarkupSettings;

/// The scheme of whole-book documents: `logoxide://BSB/JHN`
pub const SCHEME: &str = "logoxide";

/// Hover text is cut after this many bytes (a whole psalm is plenty)
pub const MAX_HOVER_TEXT: usize = 2000;

/// What colors a part of a rendered book (the semantic token types, in the legend's order)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    VerseNumber,
    ChapterNumber,
    WordsOfJesus,
    Heading,
    Note,
    /// Translators' additions (KJV italics)
    Added,
    /// LORD in small capitals
    DivineName,
    /// Quotations other than words of Jesus
    Quote,
}

impl Token {
    /// The semantic token types, indexed by `Token as u32`
    pub const LEGEND: [&str; 8] = [
        "verseNumber",
        "chapterNumber",
        "wordsOfJesus",
        "heading",
        "note",
        "added",
        "divineName",
        "quote",
    ];

    fn of(kind: Kind) -> Option<Token> {
        Some(match kind {
            Kind::Verse => Token::VerseNumber,
            Kind::Chapter => Token::ChapterNumber,
            Kind::WordsOfJesus => Token::WordsOfJesus,
            Kind::Title => Token::Heading,
            Kind::Note => Token::Note,
            Kind::Added => Token::Added,
            Kind::DivineName => Token::DivineName,
            Kind::Quote => Token::Quote,
            _ => return None,
        })
    }
}

/// What to color in a text: each of its marks that has a token, in order
pub fn tokens(marks: &[(Range<usize>, Kind)]) -> Vec<(Range<usize>, Token)> {
    (marks.iter())
        .filter_map(|(range, kind)| Some((range.clone(), Token::of(*kind)?)))
        .collect()
}

/// A passage's text, and what to color in it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub tokens: Vec<(Range<usize>, Token)>,
}

impl Text {
    /**
    A Markdown code block of the text, whose info string after the language says what to color:
    ` ```logoxide 0:0:4:0 0:33:4:2` (line, byte column, byte length, and token type, each
    token within one line)
    - Neovim's hover hides the info string, and the plugin reads it to color the block
    - Without `tokens`, just the language: for completion documentation, which completion
      plugins (blink.cmp) show with the info string (the tokens go in the item's `data`)
    */
    pub fn fence(&self, tokens: bool) -> String {
        let info = match tokens {
            true => self.token_info(),
            false => String::new(),
        };
        // Backticks in the text would end a shorter fence
        let mut fence = String::from("```");
        while self.text.contains(&fence) {
            fence.push('`');
        }
        let info = match info.is_empty() {
            true => info,
            false => format!(" {info}"),
        };
        format!("{fence}{SCHEME}{info}\n{}\n{fence}", self.text)
    }

    /// What to color, as in [`Text::fence`]'s info string: `0:0:4:0 0:33:4:2`
    pub fn token_info(&self) -> String {
        let mut info = vec![];
        let starts: Vec<usize> = std::iter::once(0)
            .chain(self.text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        for (range, token) in &self.tokens {
            let mut start = range.start;
            while start < range.end {
                let line = starts.partition_point(|&s| s <= start) - 1;
                let line_end = starts.get(line + 1).map_or(self.text.len(), |&s| s - 1);
                let end = range.end.min(line_end);
                if end > start {
                    let column = start - starts[line];
                    info.push(format!("{line}:{column}:{}:{}", end - start, *token as u32));
                }
                start = end + 1;
            }
        }
        info.join(" ")
    }
}

/// A Bible: a logoxide package (a folder with a `logoxide.toml`, in any format logoxide reads) or a
/// plain folder of USX files, read a book at a time
pub struct Bible {
    /// Its name in URIs and after references: the `bibles` setting's key, the package's
    /// abbreviation, or the folder's name (`BSB`)
    pub name: String,
    /// Other ways to write its name after a reference (`AV` for `KJV`)
    pub aliases: Vec<String>,
    /// How its verses are numbered: the package's `versification` (`English`, `KJV`), or `own`
    /// for a plain folder, which maps to nothing
    pub versification: String,
    /// The folder (canonical), whose files workspace searches leave out
    pub folder: PathBuf,
    books: Books,
    data: BibleData,
    pub markup: MarkupSettings,
    rendered: Mutex<HashMap<BookId, Arc<Rendered>>>,
}

/**
Where a Bible's books come from, each parsed once and kept in logoxide-text's book cache
- A package: logoxide's cache of it (`<cache>/books/<id>/<version>-<checksum>-<format>/`), the
  one `lx` fills, so a book either parsed is found by the other, with its verse counts too
- A folder of USX files: logoxide-text's cache keyed by the files' checksum
  (`<cache>/folders/<checksum>-usx/JHN.postcard`), so an edited file is parsed again (its old
  books stay until the folder is cleared: nothing removes them)

Without a cache folder (the tests; a server with no machine library), both are in memory for the
process
*/
enum Books {
    Package(Box<CachedPackage>),
    Folder {
        source: Box<dyn Source>,
        books: BookCache,
    },
}

impl Bible {
    /**
    The Bible in a folder, named `name` (else the package's abbreviation, else the folder's name),
    its parsed books kept in logoxide's `cache` folder (the config's; see [`Books`])
    - With a `logoxide.toml`: the package, in the format its default features turn on (USX, USFM)
    - Without: its USX files (not its subfolders'), named for their books or by their `<book code>`
    */
    pub fn load(
        folder: &Path,
        name: Option<&str>,
        data: BibleData,
        markup: MarkupSettings,
        cache: Option<&Path>,
    ) -> Result<Bible, String> {
        if !folder.is_dir() {
            return Err(format!("bible: {}: not a folder", folder.display()));
        }
        let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(folder));
        if folder.join(MANIFEST).is_file() {
            let package = Package::open(storage, Features::default())
                .map_err(|e| format!("bible: {}: {e}", folder.display()))?;
            return Bible::from_package(package, folder, name, data, markup, cache);
        }
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Bible");
        let source = UsxSource::open(Arc::new(StorageFiles(storage.clone())), "")
            .map_err(|e| format!("bible: {}: {e}", folder.display()))?;
        let key = folder_key(storage.as_ref(), &source)
            .map_err(|e| format!("bible: {}: {e}", folder.display()))?;
        let books = BookCache::new(Arc::new(StorageBooks(store(cache, "folders"))), key);
        let books = Books::Folder {
            source: Box::new(source),
            books,
        };
        Bible::new(
            books,
            name.map_or_else(|| folder_name.to_string(), String::from),
            vec![],
            String::from("own"),
            folder,
            data,
            markup,
        )
    }

    /**
    A package already opened (from `folder`), named `name` or by its abbreviation: a Bible, or a
    commentary, whose notes are its verses ([`Bible::notes`])
    */
    pub fn from_package(
        package: Package,
        folder: &Path,
        name: Option<&str>,
        data: BibleData,
        markup: MarkupSettings,
        cache: Option<&Path>,
    ) -> Result<Bible, String> {
        let info = package.info();
        let (abbreviation, aliases) = (info.abbreviation.clone(), info.aliases.clone());
        let versification = info.versification.clone();
        let cached = Cache::new(store(cache, "books"))
            .open(package, &DefaultReader)
            .map_err(|e| format!("bible: {}: {e}", folder.display()))?;
        Bible::new(
            Books::Package(Box::new(cached)),
            name.map_or(abbreviation, String::from),
            aliases,
            versification,
            folder,
            data,
            markup,
        )
    }

    fn new(
        books: Books,
        name: String,
        aliases: Vec<String>,
        versification: String,
        folder: &Path,
        data: BibleData,
        markup: MarkupSettings,
    ) -> Result<Bible, String> {
        let bible = Bible {
            name,
            aliases,
            versification,
            folder: folder
                .canonicalize()
                .unwrap_or_else(|_| folder.to_path_buf()),
            books,
            data,
            markup,
            rendered: Mutex::default(),
        };
        if bible.source().books().is_empty() {
            return Err(format!("bible: no books in {}", folder.display()));
        }
        Ok(bible)
    }

    fn source(&self) -> &dyn Source {
        match &self.books {
            Books::Package(cached) => cached.package().source(),
            Books::Folder { source, .. } => source.as_ref(),
        }
    }

    /// Each book's verse counts: a package's from its cache, a folder's from its verse markers
    /// (USX is scanned without being parsed, so a whole Bible takes milliseconds)
    fn chapter_verses(&self) -> Vec<(BookId, ChapterVerses)> {
        let counts: Vec<(BookId, Vec<u8>)> = match &self.books {
            Books::Package(cached) => cached.manifest().books.clone(),
            Books::Folder { source, .. } => (source.books().into_iter())
                .filter_map(|id| Some((id, source.chapter_verses(id).ok()?)))
                .collect(),
        };
        (counts.into_iter())
            .filter(|(_, counts)| !counts.is_empty())
            .map(|(id, counts)| (id, ChapterVerses::new(counts)))
            .collect()
    }

    /// The books it has, in order, as USFM codes (`JHN`)
    pub fn book_codes(&self) -> Vec<String> {
        let books = self.data.books();
        (self.source().books().into_iter())
            .filter_map(|id| books.get_usfm(id).cloned())
            .collect()
    }

    /// A book from the cache (memory, then disk), else parsed and cached
    fn book(&self, id: BookId) -> Option<Arc<Book>> {
        match &self.books {
            Books::Package(cached) => cached.book(id).ok(),
            Books::Folder { source, books } => books.book(source.as_ref(), id).ok(),
        }
    }

    /**
    A passage as text with `markup` (the settings' by default), its parts separated by blank
    lines, or [`None`] if the Bible doesn't have it
    - With a `limit`, text after that many bytes is cut, ending with ` …`
    */
    pub fn passage(
        &self,
        passage: &Passage,
        markup: Option<&MarkupSettings>,
        limit: Option<usize>,
    ) -> Option<Text> {
        let book = self.book(passage.book)?;
        let style = markup.unwrap_or(&self.markup).style();
        let name = self.book_name(passage.book).unwrap_or("");
        let text = passage::text(&book, passage, &style, name, limit)?;
        Some(Text {
            tokens: tokens(&text.marks),
            text: text.text,
        })
    }

    /**
    A commentary's notes on a passage (its verses are notes): each verse span covering any of
    the passage's verses, with its tree, in the book's order; none when it lacks the book. The
    book is parsed once and kept, as a Bible's are
    */
    pub fn notes(&self, passage: &Passage) -> Vec<Note> {
        if !self.source().files().contains_key(&passage.book) {
            return vec![];
        }
        let Some(book) = self.book(passage.book) else {
            return vec![];
        };
        let mut notes = vec![];
        for range in passage_ranges(&book.spans, passage) {
            for span in book.spans.get(range).into_iter().flatten() {
                notes.push(Note {
                    chapter: span.chapter,
                    verse: span.verse,
                    last_chapter: span.last_chapter,
                    last: span.last,
                    part: span.part,
                    nodes: slice(&book.nodes, &span.start, Some(&span.end)),
                });
            }
        }
        notes
    }

    /// The whole book as text, rendered once (until the settings change)
    pub fn render(&self, id: BookId) -> Option<Arc<Rendered>> {
        if let Some(rendered) = self.rendered.lock().ok()?.get(&id) {
            return Some(rendered.clone());
        }
        let book = self.book(id)?;
        let name = self.book_name(id).unwrap_or("");
        let rendered = Arc::new(passage::render(&book, &self.markup.style(), name));
        self.rendered.lock().ok()?.insert(id, rendered.clone());
        Some(rendered)
    }

    /// A book's document: `logoxide://BSB/JHN`
    pub fn uri(&self, book: BookId) -> Option<String> {
        let code = self.data.books().get_usfm(book)?;
        Some(format!("{SCHEME}://{}/{code}", encode(&self.name)))
    }

    /// The book a document is, if it is one of this Bible's
    pub fn book_of(&self, uri: &str) -> Option<BookId> {
        let rest = uri.strip_prefix(SCHEME)?.strip_prefix("://")?;
        let (name, code) = rest.split_once('/')?;
        let code = code.split(['#', '?']).next()?;
        (decode(name) == self.name)
            .then(|| self.data.books().search_usfm(&code.to_ascii_uppercase()))
            .flatten()
    }

    /// A book's name: `John`
    pub fn book_name(&self, book: BookId) -> Option<&str> {
        self.data.books().get_name(book).map(String::as_str)
    }
}

/**
The Bibles from the `bible` and `bibles` settings, the default first, and the translations their
names and aliases make (with each one's verse counts)
*/
#[derive(Default)]
pub struct Bibles {
    list: Vec<Bible>,
    pub translations: Translations,
    /// The project's versification maps, which carry a reference from one Bible's numbering
    /// to another's ([`Bibles::mapped`]); none without a project or its map packages
    maps: Versifications,
}

impl Bibles {
    /// `list` with its default first
    pub fn new(list: Vec<Bible>) -> Bibles {
        let mut translations = Translations::default();
        for bible in &list {
            translations = translations.alias(bible.name.clone(), bible.name.clone());
            for alias in &bible.aliases {
                translations = translations.alias(alias.clone(), bible.name.clone());
            }
            for (book, counts) in bible.chapter_verses() {
                translations.set_chapter_verses(&bible.name, book, counts);
            }
        }
        Bibles {
            list,
            translations,
            maps: Versifications::default(),
        }
    }

    /// These Bibles with the project's versification maps
    pub fn with_maps(mut self, maps: Versifications) -> Bibles {
        self.maps = maps;
        self
    }

    /**
    A passage numbered as in `from` (the Bible a reference names), as `to` numbers it, when the
    two are numbered differently and the project's maps lead from one to the other (`Psalm
    51:1` in an English Bible is `51:3` in a Hebrew one); none when they're numbered the same,
    either's numbering is its `own`, or no map connects them, in which case the passage is
    read as it is
    */
    pub fn mapped(&self, passage: &Passage, from: &Bible, to: &Bible) -> Option<Mapped> {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        let own = |v: &str| v.is_empty() || same(v, "own");
        if same(&from.versification, &to.versification)
            || own(&from.versification)
            || own(&to.versification)
        {
            return None;
        }
        let route = (self.maps)
            .route(&from.versification, &to.versification)
            .ok()?;
        if route.is_empty() {
            return None;
        }
        let counts: BTreeMap<BookId, ChapterVerses> = from.chapter_verses().into_iter().collect();
        (self.maps)
            .map(
                passage,
                &from.versification,
                &to.versification,
                counts.get(&passage.book),
            )
            .ok()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Bible> {
        self.list.iter()
    }

    /// The Bible references open in when they name none
    pub fn default_bible(&self) -> Option<&Bible> {
        self.list.first()
    }

    /// A Bible by its name or an alias, ignoring case
    pub fn get(&self, name: &str) -> Option<&Bible> {
        let name = self.translations.get(name)?;
        self.list.iter().find(|bible| bible.name == name)
    }

    /// The Bible a reference names (`John 3:16 KJV`), else the default
    pub fn named(&self, translation: Option<&Translation>) -> Option<&Bible> {
        translation
            .and_then(|t| self.get(&t.name))
            .or_else(|| self.default_bible())
    }

    /// The Bible and book of a whole-book document (`logoxide://KJV/JHN`)
    pub fn book_of(&self, uri: &str) -> Option<(&Bible, BookId)> {
        self.list
            .iter()
            .find_map(|bible| Some((bible, bible.book_of(uri)?)))
    }

    /// The Bibles' folders, which workspace searches leave out
    pub fn folders(&self) -> Vec<&Path> {
        self.list.iter().map(|b| b.folder.as_path()).collect()
    }
}

/// Where parsed books are kept: `name` in the `cache` folder, else memory
fn store(cache: Option<&Path>, name: &str) -> Arc<dyn Storage> {
    match cache {
        Some(cache) => Arc::new(FsStorage::new(cache.join(name))),
        None => Arc::new(MemoryStorage::new()),
    }
}

/**
A tree of its own (a commentary's note, a dictionary's entry) as text, in `markup`'s style
without verse or chapter labels (whoever shows it labels it), cut after `limit` bytes like a
passage ([`Bible::passage`]); none when it has no text
*/
pub fn nodes_text(nodes: &[Node], markup: &MarkupSettings, limit: Option<usize>) -> Option<Text> {
    let style = passage::Style {
        verse_label: None,
        chapter_label: None,
        ..markup.style()
    };
    let mut flat = Flattener::new(style.options(""));
    flat.push(nodes);
    let flat = flat.finish();
    let entry = flat.entries.first()?.clone();
    let mut text = flat.text.get(entry.clone())?.to_string();
    let mut marks: Vec<(Range<usize>, Kind)> = (flat.spans.iter())
        .filter(|s| !s.range.is_empty())
        .filter(|s| entry.start <= s.range.start && s.range.end <= entry.end)
        .map(|s| {
            (
                s.range.start - entry.start..s.range.end - entry.start,
                s.kind,
            )
        })
        .collect();
    marks.sort_by_key(|(range, _)| (range.start, range.end));
    if text.is_empty() {
        return None;
    }
    if let Some(limit) = limit.filter(|&limit| text.len() > limit) {
        let mut cut = limit;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        marks.retain(|(r, _)| r.start < cut);
        for (r, _) in &mut marks {
            r.end = r.end.min(cut);
        }
        text.push_str(" …");
    }
    // Wrapping turns spaces into line breaks, so the marks stay where they are
    if let Some(width) = style.width.filter(|&w| w > 0) {
        text = passage::wrap(&text, width);
    }
    Some(Text {
        tokens: tokens(&marks),
        text,
    })
}

/// What a folder of USX files is keyed by in the book cache: the checksum of the files its
/// books are in (as a package's is made), and the format: `0123456789abcdef-usx`
fn folder_key(storage: &dyn Storage, source: &UsxSource) -> logoxide::Result<String> {
    let mut paths: Vec<String> = source.files().values().cloned().collect();
    paths.sort();
    paths.dedup();
    let checksum = logoxide::package::checksum_of(storage, &paths)?;
    let short = (checksum.trim_start_matches("sha256:").get(..16)).unwrap_or_default();
    Ok(format!("{short}-usx"))
}

/// Whether a URI is a whole-book document
pub fn is_book_uri(uri: &str) -> bool {
    uri.starts_with(SCHEME) && uri[SCHEME.len()..].starts_with("://")
}

fn encode(name: &str) -> String {
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use logoxide_text::versification::VersificationMap;

    const USX: &str = r#"<usx version="3.0"><book code="JHN" style="id">x</book>
<chapter number="3" style="c" sid="JHN 3"/><para style="p"><verse number="16" style="v" sid="JHN 3:16"/>sixteen<verse eid="JHN 3:16"/>
<verse number="17" style="v" sid="JHN 3:17"/>seventeen<verse eid="JHN 3:17"/></para><chapter eid="JHN 3"/></usx>"#;

    #[test]
    fn a_reference_is_mapped_between_numberings() {
        let dir = std::env::temp_dir().join(format!("logoxide-lsp-mapped-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = BibleData::with_deuterocanon();
        let mut bibles = vec![];
        for name in ["A", "B"] {
            let folder = dir.join(name);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("JHN.usx"), USX).unwrap();
            let mut bible = Bible::load(
                &folder,
                Some(name),
                data.clone(),
                MarkupSettings::default(),
                None,
            )
            .unwrap();
            bible.versification = String::from(match name {
                "A" => "First",
                _ => "Second",
            });
            bibles.push(bible);
        }
        // Second's 3:17 is First's 3:16
        let map =
            VersificationMap::parse("First", "Second", [("test.vrs", "JHN 3:16 = JHN 3:17\n")])
                .unwrap();
        let bibles = Bibles::new(bibles).with_maps(Versifications::new(vec![map]));
        let (a, b) = (bibles.get("A").unwrap(), bibles.get("B").unwrap());
        let passage = data.parse("John 3:16").unwrap();
        let mapped = bibles.mapped(&passage, a, b).unwrap();
        assert_eq!(mapped.text(), "John 3:17");
        // And back; the same numbering maps to nothing
        let back = bibles
            .mapped(&data.parse("John 3:17").unwrap(), b, a)
            .unwrap();
        assert_eq!(back.text(), "John 3:16");
        assert!(bibles.mapped(&passage, a, a).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
