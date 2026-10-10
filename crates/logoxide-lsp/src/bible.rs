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
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use logoxide::{Features, FsStorage, Package, Storage, manifest::MANIFEST, reader::StorageFiles};
use logoxide_markup::Kind;
pub use logoxide_text::passage::{Part, Rendered};
use logoxide_text::{Book, Source, passage, source::UsxSource};
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
    /// The folder (canonical), whose files workspace searches leave out
    pub folder: PathBuf,
    source: Box<dyn Source>,
    data: BibleData,
    pub markup: MarkupSettings,
    books: Mutex<HashMap<BookId, Arc<Book>>>,
    rendered: Mutex<HashMap<BookId, Arc<Rendered>>>,
}

impl Bible {
    /**
    The Bible in a folder, named `name` (else the package's abbreviation, else the folder's name)
    - With a `logoxide.toml`: the package, in the format its default features turn on (USX, USFM)
    - Without: its USX files (not its subfolders'), named for their books or by their `<book code>`
    */
    pub fn load(
        folder: &Path,
        name: Option<&str>,
        data: BibleData,
        markup: MarkupSettings,
    ) -> Result<Bible, String> {
        if !folder.is_dir() {
            return Err(format!("bible: {}: not a folder", folder.display()));
        }
        let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(folder));
        let folder_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Bible");
        let (source, default_name, aliases): (Box<dyn Source>, String, Vec<String>) =
            if folder.join(MANIFEST).is_file() {
                let package = Package::open(storage, Features::default())
                    .map_err(|e| format!("bible: {}: {e}", folder.display()))?;
                let info = package.info();
                let (abbreviation, aliases) = (info.abbreviation.clone(), info.aliases.clone());
                (package.into_source(), abbreviation, aliases)
            } else {
                let source = UsxSource::open(Arc::new(StorageFiles(storage)), "")
                    .map_err(|e| format!("bible: {}: {e}", folder.display()))?;
                (Box::new(source), folder_name.to_string(), vec![])
            };
        if source.books().is_empty() {
            return Err(format!("bible: no books in {}", folder.display()));
        }
        Ok(Bible {
            name: name.map_or(default_name, String::from),
            aliases,
            folder: folder
                .canonicalize()
                .unwrap_or_else(|_| folder.to_path_buf()),
            source,
            data,
            markup,
            books: Mutex::default(),
            rendered: Mutex::default(),
        })
    }

    /// Each book's verse counts, from its verse markers (USX is scanned without being parsed, so
    /// a whole Bible takes milliseconds)
    fn chapter_verses(&self) -> Vec<(BookId, ChapterVerses)> {
        self.source
            .books()
            .into_iter()
            .filter_map(|id| {
                let counts = self.source.chapter_verses(id).ok()?;
                (!counts.is_empty()).then(|| (id, ChapterVerses::new(counts)))
            })
            .collect()
    }

    /// The books it has, in order, as USFM codes (`JHN`)
    pub fn book_codes(&self) -> Vec<String> {
        let books = self.data.books();
        (self.source.books().into_iter())
            .filter_map(|id| books.get_usfm(id).cloned())
            .collect()
    }

    fn book(&self, id: BookId) -> Option<Arc<Book>> {
        if let Some(book) = self.books.lock().ok()?.get(&id) {
            return Some(book.clone());
        }
        let book = Arc::new(self.source.book(id).ok()?);
        self.books.lock().ok()?.insert(id, book.clone());
        Some(book)
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
        Bibles { list, translations }
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
