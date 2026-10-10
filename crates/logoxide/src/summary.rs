/*!
What a package version holds, published with it so it can be known without downloading the text:
`logoxide.summary.toml` (its books' verse counts, how big each format is, how many of each element),
`logoxide.verses.bin` (a short hash of every verse's text), and for books,
`logoxide.references.bin` (the passages they cite, [`crate::references`]). `lx release` writes them, `lx verify`
builds them again and compares, and two versions' summaries say what changed between them
([`diff`]), which says how the version number must change ([`Change`]).

`logoxide.verses.bin`: `LXV1`, then 12 bytes a verse (373 KB for the KJV): book (logoxide's
number, [`crate::books`]), chapter, verse, last verse (of a bridge like 4-5), and the first 8 bytes of the SHA-256 of its text (its words, with whitespace collapsed;
notes and headings left out, since changing them can't break a link).
*/

use std::collections::{BTreeMap, HashMap};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use topos_bible_markup::{Filter, Kind, Node, TextOptions, plain_text, verses::slice};

use crate::{
    books::Book,
    error::{LogoxideError, Result},
    package::Package,
    storage::Storage,
};

pub const SUMMARY: &str = "logoxide.summary.toml";
pub const VERSES: &str = "logoxide.verses.bin";

/// The summary format's version
const SUMMARY_VERSION: u32 = 1;
const MAGIC: &[u8; 4] = b"LXV1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Summary {
    pub summary: u32,
    pub id: String,
    pub version: semver::Version,
    /// The checksum of every file but the summary's own two
    pub text_checksum: String,
    /// The checksum of the formats' files alone (not `logoxide.toml`): for a package without
    /// verses (a book), what says whether its text changed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_checksum: Option<String>,
    /// How many verses (verse markers: a bridge like 4-5 is one)
    pub verses: u64,
    /// Bytes per format, so a phone can choose
    pub sizes: BTreeMap<String, u64>,
    /// Each book's verse counts, by USFM code, in order: its versification
    pub books: IndexMap<String, Vec<u8>>,
    /// How many of each kind of element (`note`, `title`, `words-of-jesus`, ...), and `strongs`,
    /// the words tagged with Strong's numbers
    pub contents: BTreeMap<String, u64>,
    /// The audio and video it lists (not stored in it): how many files, and their bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaSummary>,
    /// How many Bible references its books (EPUBs) have; the passages they cite are in
    /// `logoxide.references.bin`
    #[serde(default, skip_serializing_if = "is_zero")]
    pub references: u64,
    /// For a versification map: how many verses it maps ([`crate::versification`])
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mappings: u64,
    /// The passages they cite, and how often (read from or written to `logoxide.references.bin`)
    #[serde(skip)]
    pub cited: Vec<crate::references::Cited>,
    /// Each file's title, by path ([`crate::titles`]): hosts name what they show by it, so a
    /// changed title is a minor change
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub titles: BTreeMap<String, String>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// A package's media, in its summary
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSummary {
    pub files: u64,
    pub bytes: u64,
}

/// One verse and the hash of its text
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VerseHash {
    /// logoxide's number for the book ([`crate::books`]), not topos's
    pub book: Book,
    pub chapter: u8,
    pub verse: u8,
    pub last: u8,
    pub hash: [u8; 8],
}

impl VerseHash {
    pub fn key(&self) -> (Book, u8, u8) {
        (self.book, self.chapter, self.verse)
    }

    /// `JHN 3:16` (or `JHN 3:4-5` for a bridge)
    pub fn reference(&self) -> String {
        let code = self.book.name();
        match self.last > self.verse {
            true => format!("{code} {}:{}-{}", self.chapter, self.verse, self.last),
            false => format!("{code} {}:{}", self.chapter, self.verse),
        }
    }
}

/// A verse's text as hashed: its words, with whitespace collapsed
pub fn verse_text(nodes: &[Node]) -> String {
    let options = TextOptions {
        filter: Filter::text(),
        ..TextOptions::default()
    };
    plain_text(nodes, &options)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/**
Each verse of a book and the hash of its words (notes and headings left out), as
`logoxide.verses.bin` has them: what a link records, so it can find its verse again in a later
version (renumbered: the same words at another number). A book without a number in logoxide's
table has none (every book topos knows has one)
*/
pub fn book_hashes(book: &crate::source::Book) -> Vec<VerseHash> {
    let Some(number) = Book::from_book_id(book.id) else {
        return vec![];
    };
    (book.spans.iter())
        .map(|span| VerseHash {
            book: number,
            chapter: span.chapter,
            verse: span.verse,
            last: span.last,
            hash: hash8(&verse_text(&slice(
                &book.nodes,
                &span.start,
                Some(&span.end),
            ))),
        })
        .collect()
}

fn hash8(text: &str) -> [u8; 8] {
    let full = Sha256::digest(text.as_bytes());
    let mut hash = [0; 8];
    hash.copy_from_slice(&full[..8]);
    hash
}

/// `WordsOfJesus` → `words-of-jesus`
fn kebab(kind: Kind) -> String {
    let mut out = String::new();
    for (i, c) in format!("{kind:?}").chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Markers and containers, which say nothing about what a text holds
const STRUCTURE: [Kind; 8] = [
    Kind::Verse,
    Kind::Chapter,
    Kind::Metadata,
    Kind::Other,
    Kind::Paragraph,
    Kind::LineBreak,
    Kind::Milestone,
    Kind::Division,
];

fn count(nodes: &[Node], contents: &mut BTreeMap<String, u64>) {
    for node in nodes {
        if let Node::Element(e) = node {
            if !STRUCTURE.contains(&e.kind) {
                *contents.entry(kebab(e.kind)).or_default() += 1;
            }
            if e.kind == Kind::Word && !e.strongs().is_empty() {
                *contents.entry(String::from("strongs")).or_default() += 1;
            }
            count(&e.children, contents);
        }
    }
}

/// The files a summary leaves out of its checksum (itself)
pub fn is_summary_file(path: &str) -> bool {
    path == SUMMARY || path == VERSES || path == crate::references::REFERENCES
}

/// Reads every book of a package into its summary and verse hashes
pub fn summarize(package: &Package) -> Result<(Summary, Vec<VerseHash>)> {
    let info = package.info();
    let mut books = IndexMap::new();
    let mut contents = BTreeMap::new();
    let mut hashes = vec![];
    for id in package.books() {
        let book = package.source().book(id)?;
        let code = crate::bible_data()
            .books()
            .get_usfm(id)
            .cloned()
            .unwrap_or_else(|| id.0.to_string());
        books.insert(code, book.chapter_verses());
        count(&book.nodes, &mut contents);
        hashes.extend(book_hashes(&book));
    }
    let storage = package.storage();
    // The package's files (not the summary's own, nor a reader's annotations), and each format's
    let declared = crate::package::declared_files(storage.as_ref())?;
    let text_files: Vec<String> = (declared.into_iter())
        .filter(|p| !is_summary_file(p))
        .collect();
    let manifest = package.manifest();
    let format_of = |path: &str| {
        (manifest.formats.iter())
            .find(|(_, f)| {
                let dir = f.path.trim_end_matches('/');
                path == dir || path.starts_with(&format!("{dir}/"))
            })
            .map(|(name, _)| name.clone())
    };
    let mut sizes: BTreeMap<String, u64> = (manifest.formats.keys())
        .map(|name| (name.clone(), 0))
        .collect();
    let mut format_files = vec![];
    for path in &text_files {
        if let Some(name) = format_of(path) {
            *sizes.entry(name).or_default() += storage.read(path)?.len() as u64;
            format_files.push(path.clone());
        }
    }
    let found = crate::references::find(package)?;
    let mappings = match info.kind {
        crate::manifest::PackageKind::VersificationMap => {
            crate::versification::VersificationMap::from_package(package)?.len() as u64
        }
        _ => 0,
    };
    let summary = Summary {
        summary: SUMMARY_VERSION,
        id: info.id.clone(),
        version: info.version.clone(),
        text_checksum: crate::package::checksum_of(storage.as_ref(), &text_files)?,
        files_checksum: Some(crate::package::checksum_of(
            storage.as_ref(),
            &format_files,
        )?),
        verses: hashes.len() as u64,
        sizes,
        books,
        contents,
        media: (package.manifest().media.as_ref())
            .filter(|m| !m.files.is_empty())
            .map(|m| MediaSummary {
                files: m.files.len() as u64,
                bytes: m.files.iter().map(|f| f.size).sum(),
            }),
        references: found.len() as u64,
        mappings,
        cited: crate::references::cited(&found),
        titles: (crate::titles::titles(package)?.into_iter())
            .filter_map(|t| Some((t.path, t.title?)))
            .collect(),
    };
    Ok((summary, hashes))
}

impl Summary {
    pub fn parse(text: &str) -> Result<Summary> {
        let summary: Summary =
            toml::from_str(text).map_err(|e| LogoxideError::Manifest(format!("{SUMMARY}: {e}")))?;
        if summary.summary != SUMMARY_VERSION {
            return Err(LogoxideError::Manifest(format!(
                "{SUMMARY}: format {} (this logoxide reads {SUMMARY_VERSION})",
                summary.summary
            )));
        }
        Ok(summary)
    }

    pub fn to_toml(&self) -> Result<String> {
        // Arrays on one line: a book's verse counts per line
        let body = toml::to_string(self).map_err(|e| LogoxideError::Manifest(e.to_string()))?;
        Ok(format!(
            "# Written by `lx release`: what this version holds. `lx verify` checks it.\n{body}"
        ))
    }

    /// Reads a package's published summary and verse hashes
    pub fn read(storage: &dyn Storage) -> Result<(Summary, Vec<VerseHash>)> {
        let mut summary = Summary::parse(&storage.read_string(SUMMARY)?)?;
        let verses = decode_verses(&storage.read(VERSES)?)?;
        if summary.references > 0 {
            let bytes = storage.read(crate::references::REFERENCES)?;
            summary.cited = crate::references::decode(&bytes)?;
        }
        Ok((summary, verses))
    }

    /// Writes the summary and verse hashes into a package, and its references when it has any
    pub fn write(&self, verses: &[VerseHash], storage: &dyn Storage) -> Result<()> {
        storage.write(SUMMARY, self.to_toml()?.as_bytes())?;
        storage.write(VERSES, &encode_verses(verses))?;
        if !self.cited.is_empty() {
            let bytes = crate::references::encode(&self.cited)?;
            storage.write(crate::references::REFERENCES, &bytes)?;
        }
        Ok(())
    }
}

pub fn encode_verses(verses: &[VerseHash]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + verses.len() * 12);
    out.extend_from_slice(MAGIC);
    for v in verses {
        out.extend_from_slice(&[v.book.0, v.chapter, v.verse, v.last]);
        out.extend_from_slice(&v.hash);
    }
    out
}

pub fn decode_verses(bytes: &[u8]) -> Result<Vec<VerseHash>> {
    let body = bytes
        .strip_prefix(MAGIC.as_slice())
        .filter(|b| b.len() % 12 == 0)
        .ok_or_else(|| LogoxideError::Manifest(format!("{VERSES}: not a verse hash file")))?;
    Ok(body
        .chunks_exact(12)
        .map(|c| {
            let mut hash = [0; 8];
            hash.copy_from_slice(&c[4..]);
            VerseHash {
                book: Book(c[0]),
                chapter: c[1],
                verse: c[2],
                last: c[3],
                hash,
            }
        })
        .collect())
}

/// How much a change can break links, which says how a version must change
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    /// Markup, headings, notes, metadata: nothing breaks
    Patch,
    /// Wording changed, verses were added or a bridge split, or a title changed: exact-word links
    /// in those verses may not match
    Minor,
    /// Verses removed, renumbered, moved, or joined into another's bridge: verse links may point
    /// at other text
    Major,
}

/// What changed between two versions' verses
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diff {
    /// Verses (by number) the new version doesn't have
    pub removed: Vec<VerseHash>,
    pub added: Vec<VerseHash>,
    /// Verses whose text changed
    pub changed: Vec<VerseHash>,
    /// Verses whose old text is at another number now (renumbered)
    pub moved: Vec<(VerseHash, VerseHash)>,
    /// Verses now inside another verse's bridge (`5` joined into `4-5`): each old verse and the
    /// new one that covers it
    pub joined: Vec<(VerseHash, VerseHash)>,
    /// Verses whose bridge was split or narrowed, every number it covered still a verse (`4-5`
    /// became `4` and `5`): the new verse at its number
    pub split: Vec<VerseHash>,
    /// Verses whose bridge narrowed and lost a number (`4-5` became `4`, with no `5`)
    pub bridged: Vec<VerseHash>,
    /// For packages without verses (books): their files changed, in ways verses can't show
    pub files_changed: bool,
    /// Files whose title changed (hosts name what they show by it): `(path, old, new)`
    pub retitled: Vec<(String, String, String)>,
}

/// What changed between two summaries, with their verses: [`diff`], and for packages without
/// verses (books), whether their files did
pub fn diff_summaries(old: (&Summary, &[VerseHash]), new: (&Summary, &[VerseHash])) -> Diff {
    let mut d = diff(old.1, new.1);
    d.files_changed = old.1.is_empty()
        && new.1.is_empty()
        && (old.0.files_checksum.as_ref())
            .is_some_and(|c| Some(c) != new.0.files_checksum.as_ref());
    // Summaries from before titles have none: nothing to compare
    d.retitled = (old.0.titles.iter())
        .filter_map(|(path, old)| {
            let new = new.0.titles.get(path)?;
            (new != old).then(|| (path.clone(), old.clone(), new.clone()))
        })
        .collect();
    d
}

/// Each number inside a bridge (5 and 6 of `4-6`), and the verse whose bridge it is
fn bridged_numbers(verses: &[VerseHash]) -> HashMap<(Book, u8, u8), &VerseHash> {
    let mut covered = HashMap::new();
    for v in verses {
        for n in v.verse.saturating_add(1)..=v.last {
            covered.entry((v.book, v.chapter, n)).or_insert(v);
        }
    }
    covered
}

/**
What changed from `old` to `new`. The range a verse covers is compared before its words: a
number now inside another verse's bridge has been joined into it, a bridge split into verses
(each of its numbers still somewhere) is minor, and one that lost a number is major; a split
verse's words are taken to change with it. A verse whose text is gone from its number has moved
when that text is now at a number that held other text before: identical verses (Psalm 107's
refrain, or empty verses) reworded or removed aren't moves to their twins, and empty text never
moves
*/
pub fn diff(old: &[VerseHash], new: &[VerseHash]) -> Diff {
    let new_by_key: HashMap<_, _> = new.iter().map(|v| (v.key(), v)).collect();
    let old_by_key: HashMap<_, _> = old.iter().map(|v| (v.key(), v)).collect();
    let new_bridged = bridged_numbers(new);
    let old_bridged = bridged_numbers(old);
    let mut new_by_hash: HashMap<_, Vec<_>> = HashMap::new();
    for v in new {
        new_by_hash.entry(v.hash).or_default().push(v);
    }
    let empty = hash8("");
    // Where a verse's text went: a number that didn't hold that text before
    let moved_to = |v: &VerseHash| {
        if v.hash == empty {
            return None;
        }
        (new_by_hash.get(&v.hash)?.iter())
            .find(|to| {
                to.key() != v.key()
                    && (old_by_key.get(&to.key())).is_none_or(|before| before.hash != v.hash)
            })
            .copied()
    };
    // Whether a number of a verse's chapter is a verse, or inside a bridge, in `by_key`/`bridged`
    let present = |by_key: &HashMap<(Book, u8, u8), &VerseHash>,
                   bridged: &HashMap<(Book, u8, u8), &VerseHash>,
                   v: &VerseHash,
                   n: u8| {
        let key = (v.book, v.chapter, n);
        by_key.contains_key(&key) || bridged.contains_key(&key)
    };
    let mut diff = Diff::default();
    for v in old {
        match new_by_key.get(&v.key()) {
            None => match (new_bridged.get(&v.key()), moved_to(v)) {
                (Some(by), _) => diff.joined.push((*v, **by)),
                (None, Some(to)) => diff.moved.push((*v, *to)),
                (None, None) => diff.removed.push(*v),
            },
            // Narrowed: split when each number it no longer covers is still in the text
            Some(n) if n.last < v.last => {
                match (n.last + 1..=v.last).all(|k| present(&new_by_key, &new_bridged, v, k)) {
                    true => diff.split.push(**n),
                    false => diff.bridged.push(**n),
                }
            }
            // Widened: the verses it took in are joined (listed with them); over numbers no verse
            // had, only its words can have changed
            Some(n) if n.last > v.last => {
                let took_in =
                    (v.last + 1..=n.last).any(|k| present(&old_by_key, &old_bridged, v, k));
                if !took_in && n.hash != v.hash {
                    diff.changed.push(**n);
                }
            }
            Some(n) if n.hash != v.hash => match moved_to(v) {
                // Its text is now at another number, and something else is here
                Some(to) => diff.moved.push((*v, *to)),
                None => diff.changed.push(**n),
            },
            Some(_) => {}
        }
    }
    // A split bridge's later verses are part of the split, not added
    diff.added = new
        .iter()
        .filter(|v| !old_by_key.contains_key(&v.key()) && !old_bridged.contains_key(&v.key()))
        .copied()
        .collect();
    diff
}

impl Diff {
    pub fn change(&self) -> Change {
        if !self.removed.is_empty()
            || !self.moved.is_empty()
            || !self.joined.is_empty()
            || !self.bridged.is_empty()
        {
            Change::Major
        } else if !self.changed.is_empty()
            || !self.added.is_empty()
            || !self.split.is_empty()
            || !self.retitled.is_empty()
            || self.files_changed
        {
            Change::Minor
        } else {
            Change::Patch
        }
    }
}

/// How `new` differs from `old` as a version number; `None` if it isn't newer. Before 1.0, like
/// Cargo, the minor counts as major (and the patch as minor), and before 0.1 any new number
/// counts as major (Cargo takes 0.0.1 and 0.0.2 to be incompatible)
pub fn version_change(old: &semver::Version, new: &semver::Version) -> Option<Change> {
    if new <= old {
        return None;
    }
    let renumbered = (new.major, new.minor, new.patch) != (old.major, old.minor, old.patch);
    Some(
        if new.major != old.major
            || (old.major == 0 && new.minor != old.minor)
            || (old.major == 0 && old.minor == 0 && renumbered)
        {
            Change::Major
        } else if old.major == 0 || new.minor != old.minor {
            // Before 1.0, a patch is the smallest change there is, so it covers wording too
            Change::Minor
        } else {
            Change::Patch
        },
    )
}

/// The smallest version after `old` for a change (before 0.1, the next patch for any change)
pub fn next_version(old: &semver::Version, change: Change) -> semver::Version {
    let v = |major, minor, patch| semver::Version::new(major, minor, patch);
    match (change, old.major, old.minor) {
        (_, 0, 0) => v(0, 0, old.patch + 1),
        (Change::Major, 0, _) => v(0, old.minor + 1, 0),
        (Change::Major, _, _) => v(old.major + 1, 0, 0),
        (Change::Minor, 0, _) => v(0, old.minor, old.patch + 1),
        (Change::Minor, _, _) => v(old.major, old.minor + 1, 0),
        (Change::Patch, _, _) => v(old.major, old.minor, old.patch + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verse(chapter: u8, verse: u8, text: &str) -> VerseHash {
        VerseHash {
            book: Book(43),
            chapter,
            verse,
            last: verse,
            hash: hash8(text),
        }
    }

    fn bridge(chapter: u8, verse: u8, last: u8, text: &str) -> VerseHash {
        VerseHash {
            last,
            ..self::verse(chapter, verse, text)
        }
    }

    #[test]
    fn a_split_bridge_is_minor() {
        let old = [verse(3, 3, "c"), bridge(3, 4, 5, "d e"), verse(3, 6, "f")];
        let new = [
            verse(3, 3, "c"),
            verse(3, 4, "d"),
            verse(3, 5, "e"),
            verse(3, 6, "f"),
        ];
        let d = diff(&old, &new);
        assert_eq!(d.split, [verse(3, 4, "d")]);
        assert!(d.added.is_empty() && d.changed.is_empty() && d.removed.is_empty());
        assert_eq!(d.change(), Change::Minor);
        // Split and reworded: still minor, the words can't be compared across a split
        let reworded = [
            verse(3, 3, "c"),
            verse(3, 4, "d!"),
            verse(3, 5, "e!"),
            verse(3, 6, "f"),
        ];
        let d = diff(&old, &reworded);
        assert_eq!((d.split.len(), d.change()), (1, Change::Minor));
        // Narrowed into another bridge (4-6 to 4 and 5-6): each number still there
        let old = [bridge(3, 4, 6, "d e f")];
        let new = [verse(3, 4, "d"), bridge(3, 5, 6, "e f")];
        let d = diff(&old, &new);
        assert_eq!((d.split.len(), d.change()), (1, Change::Minor));
        // Narrowed with a number lost (4-5 to 4, and no 5): a link to 5 finds nothing
        let d = diff(&[bridge(3, 4, 5, "d e")], &[verse(3, 4, "d e")]);
        assert_eq!((d.bridged.len(), d.change()), (1, Change::Major));
    }

    #[test]
    fn joined_verses_are_major() {
        let old = [
            verse(3, 3, "c"),
            verse(3, 4, "d"),
            verse(3, 5, "e"),
            verse(3, 6, "f"),
        ];
        let new = [verse(3, 3, "c"), bridge(3, 4, 5, "d e"), verse(3, 6, "f")];
        let d = diff(&old, &new);
        // 5 is joined into 4-5, not removed; 4 isn't reworded for taking it in
        assert_eq!(d.joined, [(verse(3, 5, "e"), bridge(3, 4, 5, "d e"))]);
        assert!(d.removed.is_empty() && d.changed.is_empty() && d.moved.is_empty());
        assert_eq!(d.change(), Change::Major);
        assert_eq!(d.joined[0].1.reference(), "JHN 3:4-5");
        // Widened: 4-5 to 4-6 takes in 6
        let old = [bridge(3, 4, 5, "d e"), verse(3, 6, "f")];
        let new = [bridge(3, 4, 6, "d e f")];
        let d = diff(&old, &new);
        assert_eq!(d.joined, [(verse(3, 6, "f"), bridge(3, 4, 6, "d e f"))]);
        assert_eq!(d.change(), Change::Major);
        // Widened over a number no verse had (an omitted verse bridged in): its words changed,
        // and no link breaks
        let d = diff(&[verse(17, 20, "a")], &[bridge(17, 20, 21, "a b")]);
        assert_eq!((d.changed.len(), d.change()), (1, Change::Minor));
    }

    #[test]
    fn diffs_and_versions() {
        let old = [verse(3, 16, "a"), verse(3, 17, "b"), verse(3, 18, "c")];
        assert_eq!(diff(&old, &old).change(), Change::Patch);
        // Wording
        let worded = [verse(3, 16, "a!"), verse(3, 17, "b"), verse(3, 18, "c")];
        let d = diff(&old, &worded);
        assert_eq!((d.change(), d.changed.len()), (Change::Minor, 1));
        assert_eq!(d.changed[0].reference(), "JHN 3:16");
        // Added
        let added = [
            verse(3, 16, "a"),
            verse(3, 17, "b"),
            verse(3, 18, "c"),
            verse(3, 19, "d"),
        ];
        assert_eq!(diff(&old, &added).change(), Change::Minor);
        // Removed, and renumbered (b's text is 3:18 now)
        assert_eq!(diff(&old, &old[..2]).change(), Change::Major);
        let renumbered = [verse(3, 16, "a"), verse(3, 17, "x"), verse(3, 18, "b")];
        let d = diff(&old, &renumbered);
        assert_eq!((d.change(), d.moved.len()), (Change::Major, 1));

        let v = |s: &str| semver::Version::parse(s).unwrap();
        assert_eq!(
            version_change(&v("1.2.0"), &v("2.0.0")),
            Some(Change::Major)
        );
        assert_eq!(
            version_change(&v("1.2.0"), &v("1.3.0")),
            Some(Change::Minor)
        );
        assert_eq!(
            version_change(&v("1.2.0"), &v("1.2.1")),
            Some(Change::Patch)
        );
        assert_eq!(
            version_change(&v("0.2.0"), &v("0.3.0")),
            Some(Change::Major)
        );
        assert_eq!(
            version_change(&v("0.2.0"), &v("0.2.1")),
            Some(Change::Minor)
        );
        assert_eq!(version_change(&v("1.2.0"), &v("1.2.0")), None);
        assert_eq!(next_version(&v("1.2.3"), Change::Minor), v("1.3.0"));
        assert_eq!(next_version(&v("0.2.3"), Change::Major), v("0.3.0"));

        let bytes = encode_verses(&old);
        assert_eq!(bytes.len(), 4 + 3 * 12);
        assert_eq!(decode_verses(&bytes).unwrap(), old);
        assert!(decode_verses(b"nope").is_err());
    }

    #[test]
    fn identical_verses_reworded_are_not_moves() {
        // Psalm 107:8, 15, 21, and 31 are the same words
        let refrain = "Oh that men would praise the LORD";
        let psalm = |eight: &str| {
            [
                verse(107, 8, eight),
                verse(107, 9, "b"),
                verse(107, 15, refrain),
                verse(107, 21, refrain),
                verse(107, 31, refrain),
            ]
        };
        let old = psalm(refrain);
        let d = diff(&old, &psalm("Oh that men would praise the LORD!"));
        assert_eq!((d.change(), d.changed.len()), (Change::Minor, 1));
        assert!(d.moved.is_empty());
        // One of them removed: removed, not moved to its twin
        let d = diff(&old, &old[1..]);
        assert_eq!((d.removed.len(), d.moved.len()), (1, 0));
        // Empty verses share a hash: filling one in is wording, not a move
        let empty = [verse(3, 1, ""), verse(3, 2, ""), verse(3, 3, "c")];
        let filled = [verse(3, 1, "a"), verse(3, 2, ""), verse(3, 3, "c")];
        let d = diff(&empty, &filled);
        assert_eq!((d.change(), d.moved.len()), (Change::Minor, 0));
        // The refrain at a number that held other words has moved there
        let mut moved = psalm("x");
        moved[1] = verse(107, 9, refrain);
        let d = diff(&old, &moved);
        assert_eq!((d.change(), d.moved.len()), (Change::Major, 1));
        assert_eq!(d.moved[0].1.reference(), "JHN 107:9");
    }

    #[test]
    fn before_0_1_every_change_is_major() {
        let v = |s: &str| semver::Version::parse(s).unwrap();
        assert_eq!(
            version_change(&v("0.0.1"), &v("0.0.2")),
            Some(Change::Major)
        );
        assert_eq!(
            version_change(&v("0.0.1"), &v("0.1.0")),
            Some(Change::Major)
        );
        assert_eq!(version_change(&v("0.0.2"), &v("0.0.1")), None);
        // Each change's next version is a patch, and enough for it
        for change in [Change::Patch, Change::Minor, Change::Major] {
            let old = v("0.0.3");
            let next = next_version(&old, change);
            assert_eq!(next, v("0.0.4"));
            assert!(version_change(&old, &next) >= Some(change));
        }
    }

    #[test]
    fn a_changed_title_is_a_minor_change() {
        let summary = |title: &str| Summary {
            summary: SUMMARY_VERSION,
            id: "b".into(),
            version: "1.0.0".parse().unwrap(),
            text_checksum: "sha256:00".into(),
            files_checksum: Some("sha256:00".into()),
            verses: 0,
            sizes: BTreeMap::new(),
            books: IndexMap::new(),
            contents: BTreeMap::new(),
            media: None,
            references: 0,
            cited: vec![],
            mappings: 0,
            titles: [("epub/b.epub".to_string(), title.to_string())].into(),
        };
        let d = diff_summaries((&summary("Freely"), &[]), (&summary("Freely Given"), &[]));
        assert_eq!(
            d.retitled,
            [("epub/b.epub".into(), "Freely".into(), "Freely Given".into())]
        );
        assert_eq!(d.change(), Change::Minor);
        let same = diff_summaries((&summary("Freely"), &[]), (&summary("Freely"), &[]));
        assert_eq!(same.change(), Change::Patch);
    }
}
