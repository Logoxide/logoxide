/*!
Audio and video: listed in a package's `logoxide.toml` by URL, size, and SHA-256, never stored in
it, so a package stays a small git repository and the media stays wherever its owner serves it.
A host downloads a file when it's first played, from `url` or else each of its `mirrors` in
turn ([`MediaFile::urls`]), checks it ([`verify`]), and keeps it by its hash ([`store_path`]), so
versions that list the same file share it. The hash makes every copy as good as the first, so
anyone may serve one.

```toml
[media]
timings = "timings/"             # each verse's start and end in each file: timings/audio/JHN.vtt

[[media.files]]
path = "audio/JHN.mp3"           # what timings name it by
url = "https://example.org/audio/JHN.mp3"
mirrors = ["https://archive.org/download/bsb-audio/JHN.mp3"]  # tried in order when it fails
size = 31457280
sha256 = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
book = "JHN"                     # the book it reads, if one
# timings = "subtitles/john.vtt" # its timings, if not in the timings folder
```

The transcript is the package's text (`[formats]`, read like any other), which topos searches; a
recording names the edition it reads as a dependency. Releases are judged by the text, so a new
hash for the same words is a patch.

**Audio only**: a recording need not carry a text of its own. `[media] text` names the
dependency whose text it reads, and `versification` the versification it's numbered by (SWORD's
names, whose verse counts logoxide has: `KJV`, `NRSV`, …); a package with either may have no
`[formats]`. Its timings are checked against that text, else that versification's counts:

```toml
[dependencies]
bsb = { git = "https://example.org/bsb", version = "1" }

[media]
text = "bsb"
versification = "KJV"
timings = "timings/"
```

**Timings** say where each verse is in a recording, so an audio Bible can play from a verse and
show the verse being read. Each file has a WebVTT file in the timings folder, at its own path with `.vtt` for its extension
(`audio/JHN.mp3` → `timings/audio/JHN.vtt`, so `video/JHN.mp4`'s are apart), or wherever its own
`timings` says, whose cues are named by the verse they read (a USFM reference;
`JHN 3:16-17` for a bridge). The cue's text may be the verse's, or empty; cues not named by a
verse (an introduction, music) are left out. Verse 0 (a psalm's title) is a verse where the text has one. Any player that reads subtitles shows it:

```text
WEBVTT

JHN 3:16
00:04:31.200 --> 00:04:38.950
For God so loved the world…
```
*/

use std::{cell::RefCell, collections::BTreeMap};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use topos_bible::data::books::BookId;

use crate::{
    error::{LogoxideError, Result},
    package::Package,
};

/// A package's `[media]`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    /// The dependency whose text the recording reads (`bsb`), for a recording with no text of
    /// its own: its timings are checked against it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The versification the recording is numbered by (`KJV`), whose verse counts its timings
    /// are checked against when there's no text to check them by
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub versification: Option<String>,
    /// Where each file's verse timings are (a folder in the package): the file's path there, with
    /// `.vtt` for its extension
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<MediaFile>,
}

/// One audio or video file, by where it's served and what it hashes to
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFile {
    /// Its name in the package (what timings name it by): `JHN.mp3`
    pub path: String,
    pub url: String,
    /// Other places that serve the same bytes, tried in order after `url`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrors: Vec<String>,
    /// In bytes
    pub size: u64,
    /// 64 hex digits (lowercase as written; either case is read)
    #[serde(deserialize_with = "sha256_field")]
    pub sha256: String,
    /// The book it reads (a USFM code), if one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<String>,
    /// Its timings' path in the package, if not where the timings folder puts them
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<String>,
}

impl Media {
    /// Whether it says what text it reads apart from the package's own (`text`, or only a
    /// `versification`), so the package may have no `[formats]`
    pub fn names_its_text(&self) -> bool {
        self.text.is_some() || self.versification.is_some()
    }
}

impl MediaFile {
    /// An entry for a file's bytes (`lx media add`)
    pub fn of(path: &str, url: &str, bytes: &[u8]) -> MediaFile {
        MediaFile {
            path: path.to_string(),
            url: url.to_string(),
            mirrors: vec![],
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
            book: None,
            timings: None,
        }
    }

    /// Where it can be downloaded, in the order to try: `url`, then each mirror
    pub fn urls(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.url.as_str()).chain(self.mirrors.iter().map(String::as_str))
    }
}

/// Whether a listed SHA-256 is one: 64 hex digits. It names where the file is kept
/// ([`store_path`]), so nothing else is allowed
pub fn check_sha256(sha256: &str) -> Result<()> {
    match sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        true => Ok(()),
        false => Err(LogoxideError::Media(format!(
            "sha256 `{sha256}` isn't a SHA-256 (64 hex digits)"
        ))),
    }
}

fn sha256_field<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    let sha256 = String::deserialize(d)?;
    check_sha256(&sha256).map_err(serde::de::Error::custom)?;
    Ok(sha256)
}

/// A file's SHA-256, as lowercase hex
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether downloaded bytes are the file listed: its size, then its hash
pub fn verify(file: &MediaFile, bytes: &[u8]) -> Result<()> {
    let fail = |message: String| LogoxideError::Mismatch {
        url: file.url.clone(),
        message,
    };
    if bytes.len() as u64 != file.size {
        return Err(fail(format!(
            "{} bytes, not the {} listed",
            bytes.len(),
            file.size
        )));
    }
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(&file.sha256) {
        return Err(fail(format!(
            "its SHA-256 is {actual}, not the {} listed",
            file.sha256
        )));
    }
    Ok(())
}

/// One verse's place in a recording (or a bridge's: `verse` to `last`)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerseTiming {
    /// The book's USFM code: `JHN`
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    /// The bridge's last verse (`verse` when it's one)
    pub last: u16,
    /// From the start of the recording, in milliseconds
    pub start_ms: u32,
    pub end_ms: u32,
}

/// Where a file's timings are in its package: its own `timings`, else its path in the timings
/// folder with `.vtt` for its extension (`timings/audio/JHN.vtt` for `audio/JHN.mp3`); `None`
/// when it has neither
pub fn timings_path(media: &Media, file: &MediaFile) -> Option<String> {
    if let Some(path) = &file.timings {
        return Some(path.clone());
    }
    let folder = media.timings.as_deref()?.trim_end_matches('/');
    let path = file.path.trim_start_matches("./");
    let stem = match path.rsplit_once('.') {
        Some((stem, extension)) if !extension.contains('/') => stem,
        _ => path,
    };
    Some(crate::storage::join(folder, &format!("{stem}.vtt")))
}

/// `00:04:31.200` or `04:31.200` (WebVTT's timestamps), in milliseconds
fn timestamp(text: &str) -> Option<u32> {
    let (clock, millis) = text.trim().split_once('.')?;
    let parts: Vec<u32> = clock
        .split(':')
        .map(|n| n.parse().ok())
        .collect::<Option<_>>()?;
    let seconds = match parts[..] {
        [h, m, s] if m < 60 && s < 60 => h.checked_mul(3600)?.checked_add(m * 60 + s)?,
        [m, s] if s < 60 => m.checked_mul(60)?.checked_add(s)?,
        _ => return None,
    };
    if millis.len() != 3 {
        return None;
    }
    seconds.checked_mul(1000)?.checked_add(millis.parse().ok()?)
}

/// A cue's name as a verse: `JHN 3:16` or `JHN 3:16-17`
fn verse_name(name: &str) -> Option<(String, u16, u16, u16)> {
    let (book, place) = name.trim().split_once(' ')?;
    let valid = book.len() == 3
        && book
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    let (chapter, verses) = place.split_once(':')?;
    let (verse, last) = verses.split_once('-').unwrap_or((verses, verses));
    let (chapter, verse, last) = (
        chapter.parse().ok()?,
        verse.parse().ok()?,
        last.parse().ok()?,
    );
    (valid && verse <= last).then(|| (book.to_string(), chapter, verse, last))
}

/// A WebVTT timings file's verses, in its order. Cues not named by a verse are left out; a
/// malformed time is an error
pub fn parse_timings(text: &str) -> Result<Vec<VerseTiming>> {
    let bad = |line: usize, what: &str| LogoxideError::Media(format!("line {line}: {what}"));
    let text = text.trim_start_matches('\u{feff}');
    let lines: Vec<&str> = text.lines().collect();
    if !lines.first().is_some_and(|l| l.starts_with("WEBVTT")) {
        return Err(bad(1, "a WebVTT file starts with WEBVTT"));
    }
    let mut timings = vec![];
    // Each block: an optional name, then the time line, then text
    let mut i = 1;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() {
            i += 1;
        }
        let block = &lines[start..i];
        let Some(arrow) = block.iter().position(|l| l.contains("-->")) else {
            continue; // NOTE, STYLE, REGION
        };
        let name = block[..arrow].first().filter(|_| arrow == 1);
        let (from, to) = block[arrow].split_once("-->").unwrap_or_default();
        let to = to.split_whitespace().next().unwrap_or_default();
        let line = start + arrow + 1;
        let (Some(start_ms), Some(end_ms)) = (timestamp(from), timestamp(to)) else {
            return Err(bad(line, &format!("`{}` isn't a time range", block[arrow])));
        };
        if end_ms < start_ms {
            return Err(bad(line, "it ends before it starts"));
        }
        if let Some((book, chapter, verse, last)) = name.and_then(|n| verse_name(n)) {
            timings.push(VerseTiming {
                book,
                chapter,
                verse,
                last,
                start_ms,
                end_ms,
            });
        }
    }
    Ok(timings)
}

/**
What's wrong with a file's timings, against the verses its package's text has (`verses(book,
chapter)`: the chapter's verse numbers, a bridge's each, with 0 where the text numbers a title so;
or `None` for a chapter it lacks): verses it doesn't have, another book than the file's, and
verses that overlap or go backwards
*/
pub fn check_timings(
    file: &MediaFile,
    timings: &[VerseTiming],
    verses: impl Fn(&str, u16) -> Option<Vec<u16>>,
) -> Vec<String> {
    let mut problems = vec![];
    let mut previous: Option<&VerseTiming> = None;
    for t in timings {
        let name = match t.last == t.verse {
            true => format!("{} {}:{}", t.book, t.chapter, t.verse),
            false => format!("{} {}:{}-{}", t.book, t.chapter, t.verse, t.last),
        };
        if file.book.as_deref().is_some_and(|b| b != t.book) {
            problems.push(format!(
                "{name}: the file reads {}",
                file.book.as_deref().unwrap_or("")
            ));
        }
        match verses(&t.book, t.chapter) {
            None => problems.push(format!("{name}: the text has no such chapter")),
            Some(numbers) => {
                let lacking: Vec<String> = (t.verse..=t.last)
                    .filter(|v| !numbers.contains(v))
                    .map(|v| v.to_string())
                    .collect();
                let lacking = match lacking.split_last() {
                    Some((last, [])) => last.clone(),
                    Some((last, others)) => format!("{} or {last}", others.join(", ")),
                    None => String::new(),
                };
                if !lacking.is_empty() {
                    problems.push(format!("{name}: the text has no verse {lacking}"));
                }
            }
        }
        if let Some(p) = previous
            && p.book == t.book
        {
            if (t.chapter, t.verse) <= (p.chapter, p.last) {
                problems.push(format!("{name}: out of the text's order"));
            }
            if t.start_ms < p.end_ms {
                problems.push(format!("{name}: starts before the verse before it ends"));
            }
        }
        previous = Some(t);
    }
    problems
}

/// A listed file's timings from its package, if it has any
pub fn timings(package: &Package, file: &MediaFile) -> Result<Option<Vec<VerseTiming>>> {
    let Some(media) = package.manifest().media.as_ref() else {
        return Ok(None);
    };
    let Some(path) = timings_path(media, file) else {
        return Ok(None);
    };
    if !package.storage().exists(&path) {
        return Ok(None);
    }
    let text = package.storage().read_string(&path)?;
    parse_timings(&text)
        .map(Some)
        .map_err(|e| LogoxideError::Media(format!("{path}: {e}")))
}

/// Each chapter's verse numbers in a book (a bridge's each, 0 for a title numbered so), by
/// chapter
fn book_verses(package: &Package, id: BookId) -> BTreeMap<u16, Vec<u16>> {
    let mut chapters: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    for span in package
        .source()
        .book(id)
        .map(|b| b.spans)
        .unwrap_or_default()
    {
        if span.chapter >= 1 {
            let numbers = chapters.entry(u16::from(span.chapter)).or_default();
            numbers.extend(u16::from(span.verse)..=u16::from(span.last.max(span.verse)));
        }
    }
    for numbers in chapters.values_mut() {
        numbers.sort_unstable();
        numbers.dedup();
    }
    chapters
}

/**
Every listed file's timings checked against the package's text ([`check_timings`]), with a
problem for a file whose timings aren't where they should be (in the timings folder, or at its
own `timings`), and for a timings file anywhere under the folder that no listed file has. Empty
when all is well. For a recording whose `[media] text` names a dependency, see
[`check_package_timings_with`]: here its verses aren't checked
*/
pub fn check_package_timings(package: &Package) -> Result<Vec<String>> {
    check_package_timings_with(package, None)
}

/**
[`check_package_timings`], with the package `[media] text` names (`text`, as installed): the
verses are checked against it. Without one, the package's own text; with none, the counts of
`[media] versification` (SWORD's systems; another name is a problem); with neither, a problem.
When `[media] text` names a dependency that isn't given, verses aren't checked (only their order
and the files)
*/
pub fn check_package_timings_with(
    package: &Package,
    text: Option<&Package>,
) -> Result<Vec<String>> {
    let Some(media) = package.manifest().media.as_ref() else {
        return Ok(vec![]);
    };
    let mut problems = vec![];
    let own = !package.source().books().is_empty();
    let source = match (text, media.text.is_some(), own) {
        (Some(text), _, _) => Verses::Text(text),
        (None, true, _) => Verses::Unchecked,
        (None, false, true) => Verses::Text(package),
        (None, false, false) => match media.versification.as_deref() {
            Some(name) => match sword_modules::Versification::get(name) {
                Some(system) => Verses::Counts(system),
                None => {
                    problems.push(format!(
                        "[media] versification `{name}`: logoxide has no verse counts for it \
                         (SWORD's systems: KJV, NRSV, …); its timings' verses aren't checked"
                    ));
                    Verses::Unchecked
                }
            },
            None => {
                if !media.files.is_empty() {
                    problems.push(String::from(
                        "no text to check timings against: give [formats], or [media] text or \
                         versification",
                    ));
                }
                Verses::Unchecked
            }
        },
    };
    // Each book's verse numbers, read when a timings file first names it
    let books = RefCell::new(BTreeMap::new());
    let verses = |book: &str, chapter: u16| -> Option<Vec<u16>> {
        let id = crate::bible_data().books().search_usfm(book)?;
        match &source {
            Verses::Text(text) => {
                let mut books = books.borrow_mut();
                let chapters = (books.entry(id)).or_insert_with(|| book_verses(text, id));
                chapters.get(&chapter).cloned()
            }
            Verses::Counts(system) => {
                let count = system.verse_count(id, u8::try_from(chapter).ok()?)?;
                Some((1..=u16::from(count)).collect())
            }
            Verses::Unchecked => Some((0..=u16::from(u8::MAX)).collect()),
        }
    };
    let mut expected = vec![];
    for file in &media.files {
        let Some(path) = timings_path(media, file) else {
            continue;
        };
        match timings(package, file) {
            Ok(Some(found)) => {
                for problem in check_timings(file, &found, verses) {
                    problems.push(format!("{path}: {problem}"));
                }
            }
            Ok(None) => problems.push(format!("{}: no timings ({path})", file.path)),
            Err(e) => problems.push(e.to_string()),
        }
        expected.push(path);
    }
    if let Some(folder) = media.timings.as_deref() {
        let folder = folder.trim_end_matches('/');
        for path in package.storage().walk(folder).unwrap_or_default() {
            if path.ends_with(".vtt") && !expected.contains(&path) {
                problems.push(format!("{path}: no listed file has these timings"));
            }
        }
    }
    Ok(problems)
}

/// What a recording's verses are checked against
enum Verses<'a> {
    /// A text: its own, or the one `[media] text` names
    Text(&'a Package),
    /// A versification's verse counts (`[media] versification`)
    Counts(&'static sword_modules::Versification),
    /// Nothing: only the timings' order and files are checked
    Unchecked,
}

/// Where a host keeps a media file, by its hash: `media/9f/9f86…` (relative to its cache), so
/// every version that lists it shares one copy
pub fn store_path(file: &MediaFile) -> String {
    // Only hex digits, even from a file listed without reading a manifest
    let hash: String = (file.sha256.chars())
        .filter(char::is_ascii_hexdigit)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    format!("media/{}/{hash}", hash.get(..2).unwrap_or("00"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_checks_media() {
        let manifest = crate::Manifest::parse(
            r#"
[package]
id = "bsb-audio"
name = "BSB audio"
abbreviation = "BSBA"
version = "1.0.0"
language = "en"

[formats]
usx = { path = "usx/" }

[media]
timings = "timings/"

[[media.files]]
path = "JHN.mp3"
url = "https://example.org/JHN.mp3"
mirrors = ["https://mirror.example.org/JHN.mp3"]
size = 5
sha256 = "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"
book = "JHN"
"#,
        )
        .unwrap();
        let media = manifest.media.unwrap();
        let file = &media.files[0];
        assert_eq!(
            file.urls().collect::<Vec<_>>(),
            [
                "https://example.org/JHN.mp3",
                "https://mirror.example.org/JHN.mp3"
            ]
        );
        assert!(verify(file, b"hello").is_ok());
        assert!(verify(file, b"hellO").is_err());
        assert!(verify(file, b"hello!").is_err());
        assert_eq!(
            store_path(file),
            "media/2c/2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        // A hash that isn't one is refused: it names a folder
        let bad = |sha256: &str| {
            let text = format!(
                "[package]\nid = \"a\"\nname = \"A\"\nabbreviation = \"A\"\nversion = \"1.0.0\"\n\
                 language = \"en\"\n[formats]\nusx = {{ path = \"usx/\" }}\n[[media.files]]\n\
                 path = \"a.mp3\"\nurl = \"u\"\nsize = 0\nsha256 = \"{sha256}\"\n"
            );
            crate::Manifest::parse(&text).is_err()
        };
        assert!(bad("../../etc"));
        assert!(bad("e3b0"));
        assert!(bad(&"g".repeat(64)));
        assert!(!bad(&"e".repeat(64)));
        assert_eq!(
            MediaFile::of("a.mp3", "u", b"hello").sha256,
            file.sha256.to_lowercase()
        );
        assert_eq!(
            timings_path(&media, file).as_deref(),
            Some("timings/JHN.vtt")
        );
    }

    const TIMINGS: &str = "WEBVTT\n\nNOTE made by hand\n\n00:00.000 --> 00:04.000\nThe Gospel of John, chapter 3\n\nJHN 3:16\n00:00:04.000 --> 00:00:09.500 align:start\nFor God so loved the world…\n\nJHN 3:17-18\n00:09.500 --> 01:00:20.250\n";

    #[test]
    fn reads_timings_named_by_verse() {
        let timings = parse_timings(TIMINGS).unwrap();
        let places: Vec<_> = (timings.iter())
            .map(|t| {
                (
                    t.book.as_str(),
                    t.chapter,
                    t.verse,
                    t.last,
                    t.start_ms,
                    t.end_ms,
                )
            })
            .collect();
        assert_eq!(
            places,
            [
                ("JHN", 3, 16, 16, 4_000, 9_500),
                ("JHN", 3, 17, 18, 9_500, 3_620_250)
            ]
        );
        assert!(parse_timings("1\n00:00:01,000 --> 00:00:02,000\nSubRip").is_err());
        let bad = parse_timings("WEBVTT\n\nJHN 3:16\n00:00:09.000 --> 00:00:04.000\n");
        assert!(bad.unwrap_err().to_string().contains("line 4"));
        assert!(parse_timings("WEBVTT\n\nJHN 3:16\n00:61.000 --> 00:62.000\n").is_err());
    }

    #[test]
    fn checks_timings_against_the_text() {
        let file = MediaFile {
            book: Some("JHN".into()),
            ..MediaFile::of("JHN.mp3", "u", b"")
        };
        // John 3 has 36 verses here, and no chapter 30; Psalm 3 numbers its title 0
        let verses = |book: &str, chapter: u16| match book {
            "JHN" if chapter < 22 => Some((1..=36).collect()),
            "PSA" if chapter == 3 => Some((0..=8).collect()),
            _ => None,
        };
        let mut timings = parse_timings(TIMINGS).unwrap();
        assert!(check_timings(&file, &timings, verses).is_empty());
        let mut wrong = timings[0].clone();
        (wrong.verse, wrong.last, wrong.start_ms) = (40, 40, 9_000);
        timings.push(wrong);
        let mut other = timings[0].clone();
        (other.book, other.chapter, other.start_ms) = ("ACT".into(), 30, 4_000_000);
        timings.push(other);
        let mut bridge = timings[0].clone();
        (bridge.verse, bridge.last, bridge.start_ms) = (36, 38, 5_000_000);
        timings.push(bridge);
        let mut title = timings[0].clone();
        (title.chapter, title.verse, title.last, title.start_ms) = (4, 0, 0, 6_000_000);
        timings.push(title);
        assert_eq!(
            check_timings(&file, &timings, verses),
            [
                "JHN 3:40: the text has no verse 40",
                "JHN 3:40: starts before the verse before it ends",
                "ACT 30:16: the file reads JHN",
                "ACT 30:16: the text has no such chapter",
                "JHN 3:36-38: the text has no verse 37 or 38",
                "JHN 4:0: the text has no verse 0",
            ]
        );
        // Where the text numbers a psalm's title 0, it's a verse like any other
        let psalm = MediaFile::of("PSA.mp3", "u", b"");
        let titled = parse_timings(
            "WEBVTT\n\nPSA 3:0\n00:00.000 --> 00:04.000\n\nPSA 3:1\n00:04.000 --> 00:08.000\n",
        )
        .unwrap();
        assert!(check_timings(&psalm, &titled, verses).is_empty());
    }

    #[test]
    fn timings_keep_the_files_folder() {
        let media = Media {
            timings: Some("timings/".into()),
            ..Media::default()
        };
        let path = |file: &str| timings_path(&media, &MediaFile::of(file, "u", b""));
        assert_eq!(path("JHN.mp3").as_deref(), Some("timings/JHN.vtt"));
        assert_eq!(
            path("audio/JHN.mp3").as_deref(),
            Some("timings/audio/JHN.vtt")
        );
        assert_eq!(
            path("video/JHN.mp4").as_deref(),
            Some("timings/video/JHN.vtt")
        );
        assert_eq!(path("v1.0/JHN").as_deref(), Some("timings/v1.0/JHN.vtt"));
        let own = MediaFile {
            timings: Some("subtitles/john.vtt".into()),
            ..MediaFile::of("audio/JHN.mp3", "u", b"")
        };
        assert_eq!(
            timings_path(&media, &own).as_deref(),
            Some("subtitles/john.vtt")
        );
        // Without a timings folder, only a file's own timings
        let none = Media::default();
        assert_eq!(
            timings_path(&none, &MediaFile::of("JHN.mp3", "u", b"")),
            None
        );
        assert_eq!(
            timings_path(&none, &own).as_deref(),
            Some("subtitles/john.vtt")
        );
    }

    #[test]
    fn checks_a_packages_timings() {
        use crate::{
            package::Features,
            storage::{MemoryStorage, Storage},
        };
        let usx = r#"<usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c"/><para style="p"><verse number="16" style="v"/>For God so loved the world. <verse number="17" style="v"/>For God did not send. <verse number="18" style="v"/>Whoever believes.</para></usx>"#;
        let sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let manifest = format!(
            "[package]\nid = \"a\"\nname = \"A\"\nabbreviation = \"A\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n\n[media]\ntimings = \"timings/\"\n\n[[media.files]]\npath = \"audio/JHN.mp3\"\nurl = \"u\"\nsize = 0\nsha256 = \"{sha256}\"\nbook = \"JHN\"\n\n[[media.files]]\npath = \"video/JHN.mp4\"\nurl = \"u\"\nsize = 0\nsha256 = \"{sha256}\"\nbook = \"JHN\"\n\n[[media.files]]\npath = \"ACT.mp3\"\nurl = \"u\"\nsize = 0\nsha256 = \"{sha256}\"\ntimings = \"subtitles/acts.vtt\"\n"
        );
        let open = |extra: &[(&str, &str)]| {
            let storage = MemoryStorage::with([
                (crate::manifest::MANIFEST, manifest.as_str()),
                ("usx/JHN.usx", usx),
                ("timings/audio/JHN.vtt", TIMINGS),
            ]);
            for (path, text) in extra {
                storage.write(path, text.as_bytes()).unwrap();
            }
            Package::open(std::sync::Arc::new(storage), Features::default()).unwrap()
        };
        let package = open(&[]);
        let jhn = &package.manifest().media.as_ref().unwrap().files[0];
        assert_eq!(timings(&package, jhn).unwrap().unwrap().len(), 2);
        assert_eq!(
            check_package_timings(&package).unwrap(),
            [
                "video/JHN.mp4: no timings (timings/video/JHN.vtt)",
                "ACT.mp3: no timings (subtitles/acts.vtt)"
            ]
        );
        let video =
            "WEBVTT\n\nJHN 3:16\n00:00.000 --> 00:04.000\n\nJHN 3:19\n00:04.000 --> 00:08.000\n";
        let package = open(&[
            ("timings/video/JHN.vtt", video),
            ("subtitles/acts.vtt", "WEBVTT\n"),
            ("timings/audio/MRK.vtt", "WEBVTT\n"),
        ]);
        assert_eq!(
            check_package_timings(&package).unwrap(),
            [
                "timings/video/JHN.vtt: JHN 3:19: the text has no verse 19",
                "timings/audio/MRK.vtt: no listed file has these timings"
            ]
        );
        let declared = crate::package::declared_files(package.storage().as_ref()).unwrap();
        assert!(declared.contains(&"subtitles/acts.vtt".to_string()));
        assert!(declared.contains(&"timings/audio/MRK.vtt".to_string()));
    }

    #[test]
    fn a_recording_reads_another_text_or_a_versification() {
        use crate::{package::Features, storage::MemoryStorage};
        let sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let manifest = |media: &str| {
            format!(
                "[package]\nid = \"bsb-audio\"\nname = \"BSB audio\"\nabbreviation = \"BSBA\"\n\
                 version = \"1.0.0\"\nlanguage = \"en\"\n\n[dependencies]\n\
                 bsb = {{ git = \"https://example.org/bsb\", version = \"1\" }}\n\n\
                 [media]\n{media}\ntimings = \"timings/\"\n\n[[media.files]]\npath = \"JHN.mp3\"\n\
                 url = \"u\"\nsize = 0\nsha256 = \"{sha256}\"\nbook = \"JHN\"\n"
            )
        };
        // John 3 has 36 verses in the KJV's numbering
        let vtt =
            "WEBVTT\n\nJHN 3:36\n00:00.000 --> 00:04.000\n\nJHN 3:37\n00:04.000 --> 00:08.000\n";
        let open = |media: &str| {
            let storage = MemoryStorage::with([
                (crate::manifest::MANIFEST, manifest(media).as_str()),
                ("timings/JHN.vtt", vtt),
            ]);
            Package::open(std::sync::Arc::new(storage), Features::default()).unwrap()
        };
        // No [formats]: its versification's counts
        let counted = open("versification = \"KJV\"");
        assert!(counted.source().books().is_empty());
        assert_eq!(
            check_package_timings(&counted).unwrap(),
            ["timings/JHN.vtt: JHN 3:37: the text has no verse 37"]
        );
        // The text it reads, when given; without it, verses aren't checked
        let reading = open("text = \"bsb\"");
        assert_eq!(
            check_package_timings(&reading).unwrap(),
            Vec::<String>::new()
        );
        let usx = r#"<usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c"/><para style="p"><verse number="36" style="v"/>He that believeth.</para></usx>"#;
        let bsb = MemoryStorage::with([
            (
                crate::manifest::MANIFEST,
                "[package]\nid = \"bsb\"\nname = \"BSB\"\nabbreviation = \"BSB\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n[formats]\nusx = { path = \"usx/\" }\n",
            ),
            ("usx/JHN.usx", usx),
        ]);
        let bsb = Package::open(std::sync::Arc::new(bsb), Features::default()).unwrap();
        assert_eq!(
            check_package_timings_with(&reading, Some(&bsb)).unwrap(),
            ["timings/JHN.vtt: JHN 3:37: the text has no verse 37"]
        );
        // A versification logoxide has no counts for is said
        let unknown = open("versification = \"Mine\"");
        assert!(check_package_timings(&unknown).unwrap()[0].contains("no verse counts"));

        // The manifest: text names a dependency; without one of them, [formats] is needed
        let error = |media: &str| {
            crate::Manifest::parse(&manifest(media))
                .unwrap_err()
                .to_string()
        };
        assert!(error("text = \"kjv\"").contains("names no dependency"));
        assert!(error("").contains("a package needs [formats]"));
    }

    #[test]
    fn verse_0_is_a_verse_where_the_text_has_one() {
        use crate::{package::Features, storage::MemoryStorage};
        let usx = r#"<usx version="3.0"><book code="PSA" style="id"/><chapter number="3" style="c"/><para style="d"><verse number="0" style="v"/>A Psalm of David.</para><para style="q1"><verse number="1" style="v"/>Lord, how many are my foes!</para></usx>"#;
        let manifest = "[package]\nid = \"a\"\nname = \"A\"\nabbreviation = \"A\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n\n[formats]\nusx = { path = \"usx/\" }\n\n[media]\ntimings = \"timings/\"\n\n[[media.files]]\npath = \"PSA.mp3\"\nurl = \"u\"\nsize = 0\nsha256 = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"\n";
        let vtt =
            "WEBVTT\n\nPSA 3:0\n00:00.000 --> 00:04.000\n\nPSA 3:1\n00:04.000 --> 00:08.000\n";
        let storage = MemoryStorage::with([
            (crate::manifest::MANIFEST, manifest),
            ("usx/PSA.usx", usx),
            ("timings/PSA.vtt", vtt),
        ]);
        let package = Package::open(std::sync::Arc::new(storage), Features::default()).unwrap();
        assert_eq!(
            check_package_timings(&package).unwrap(),
            Vec::<String>::new()
        );
    }
}
