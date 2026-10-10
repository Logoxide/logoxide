/*!
Each verse's words hashed, so a verse can be found again in a later version of its text
(renumbered: the same words at another number) and a change told from a renumbering. A rule
never changes once published: what changes how the text is rendered or normalized (a markup
upgrade that renders differently included, which `tests/verse_hash.rs` pins) is a new rule beside
the old, so two versions' hashes are compared only when made by one rule. The package manager
records the rule's number with the hashes.
*/

use std::collections::BTreeMap;

use logoxide_markup::{Filter, Kind, Node, TextOptions, plain_text, verses::slice};
use sha2::{Digest, Sha256};
use topos_bible::data::books::BookId;
use unicode_normalization::UnicodeNormalization;

use crate::source::Book;

/// Bytes of SHA-256 kept per verse
pub const HASH_BYTES: usize = 7;

/// How a verse's words become the text that's hashed
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HashRule {
    /**
    logoxide-markup's `plain_text` with `Filter::text()` (notes, headings, and metadata left
    out), in Unicode NFC (not NFKC, which would fold away distinctions a text makes), without
    the invisible U+200B (zero-width space), U+FEFF (byte-order mark), U+00AD (soft hyphen), and
    U+2060 (word joiner), split on Unicode whitespace (NBSP included) and joined with single
    spaces. Zero-width joiners and non-joiners (U+200D, U+200C) are kept: they change how a word
    is written in some scripts
    */
    One,
}

impl HashRule {
    /// The rule this crate writes
    pub const CURRENT: HashRule = HashRule::One;

    /// Its number, as files give it
    pub fn number(self) -> u16 {
        match self {
            HashRule::One => 1,
        }
    }

    /// The rule a number names, if this crate knows it
    pub fn from_number(number: u16) -> Option<HashRule> {
        match number {
            1 => Some(HashRule::One),
            _ => None,
        }
    }
}

/// One verse and the hash of its words
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VerseRecord {
    pub book: BookId,
    pub chapter: u8,
    pub verse: u8,
    /// The last verse of a bridge like 4-5 (`verse` when it's one)
    pub last: u8,
    /// Which part of the verse (1 for `20a`, 2 for `20b`), 0 for the whole verse
    pub part: u8,
    pub hash: [u8; HASH_BYTES],
}

/// The characters [`HashRule::One`] drops: zero-width space, byte-order mark, soft hyphen, and
/// word joiner
const INVISIBLE: [char; 4] = ['\u{200B}', '\u{FEFF}', '\u{00AD}', '\u{2060}'];

/// A verse's text as hashed by `rule`: its words, normalized, with whitespace collapsed
pub fn verse_text(nodes: &[Node], rule: HashRule) -> String {
    let options = TextOptions {
        filter: Filter::text(),
        ..TextOptions::default()
    };
    let text = plain_text(nodes, &options);
    match rule {
        HashRule::One => {
            let normalized: String = (text.nfc()).filter(|c| !INVISIBLE.contains(c)).collect();
            normalized.split_whitespace().collect::<Vec<_>>().join(" ")
        }
    }
}

/// The first [`HASH_BYTES`] bytes of a text's SHA-256
pub fn verse_hash(text: &str) -> [u8; HASH_BYTES] {
    let full = Sha256::digest(text.as_bytes());
    let mut hash = [0; HASH_BYTES];
    hash.copy_from_slice(&full[..HASH_BYTES]);
    hash
}

/**
Each verse of a book and the hash of its words (notes and headings left out) by `rule`. A verse
across chapters (a note on John 3:36 to 4:2) is two records with its words: its first verse, and
the verses it covers in its last chapter
*/
pub fn book_hashes(book: &Book, rule: HashRule) -> Vec<VerseRecord> {
    let mut out = Vec::with_capacity(book.spans.len());
    for span in &book.spans {
        let hash = verse_hash(&verse_text(
            &slice(&book.nodes, &span.start, Some(&span.end)),
            rule,
        ));
        let across = span.last_chapter > span.chapter;
        out.push(VerseRecord {
            book: book.id,
            chapter: span.chapter,
            verse: span.verse,
            last: if across { span.verse } else { span.last },
            part: span.part,
            hash,
        });
        if across && span.last >= 1 {
            out.push(VerseRecord {
                book: book.id,
                chapter: span.last_chapter,
                verse: 1,
                last: span.last,
                part: 0,
                hash,
            });
        }
    }
    out
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

/// Counts a book's elements into `contents` by kind in kebab case (`note`, `title`,
/// `words-of-jesus`, ...), and `strongs` for words tagged with Strong's numbers; markers and
/// containers are left out
pub fn count_elements(nodes: &[Node], contents: &mut BTreeMap<String, u64>) {
    for node in nodes {
        if let Node::Element(e) = node {
            if !STRUCTURE.contains(&e.kind) {
                *contents.entry(kebab(e.kind)).or_default() += 1;
            }
            if e.kind == Kind::Word && !e.strongs().is_empty() {
                *contents.entry(String::from("strongs")).or_default() += 1;
            }
            count_elements(&e.children, contents);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_one_normalizes() {
        let text = |s: &str| {
            let nodes = [Node::Text(s.to_string())];
            verse_text(&nodes, HashRule::One)
        };
        assert_eq!(
            text("  In the\u{a0}beginning\n God "),
            "In the beginning God"
        );
        assert_eq!(text("a\u{200b}b\u{feff}c\u{ad}d\u{2060}e"), "abcde");
        assert_eq!(text("\u{65}\u{301}"), "\u{e9}");
        assert_eq!(text("a\u{200d}b"), "a\u{200d}b");
        assert_eq!(HashRule::from_number(1), Some(HashRule::One));
        assert_eq!(HashRule::from_number(2), None);
    }
}
