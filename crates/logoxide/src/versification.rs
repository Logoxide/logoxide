/*!
Versification maps: where a verse numbered one way (a versification) is in Bibles numbered
another. `Psalm 51:1` in an English Bible is `Psalm 51:3` in the Hebrew, Malachi 4:1 is 3:19, and
Joel 2:28 is 3:1.

A Bible names its versification with `[package] versification = "English"`. A map package
(`kind = "versification-map"`) maps its `versification` to another, `maps-to`, in Paratext's
`.vrs` files (a `vrs` format), the form Paratext, libpalaso, and the Copenhagen Alliance's
mappings use, so Paratext's own `eng.vrs` (English to Original) can be a map package as it is:

```toml
[package]
id = "english-original"
kind = "versification-map"
versification = "English"
maps-to = "Original"

[formats]
vrs = { path = "eng.vrs" }
```

Each mapping line is `BOOK C:V = BOOK C:V`, either side a verse or a range within a chapter
(`PSA 51:1-19 = PSA 51:3-21`), the left side in `versification`, the right in `maps-to`, books by
their USFM codes:
- Two ranges of the same length map verse by verse
- A verse and a range map one to many (`3JN 1:14 = 3JN 1:14-15`), or many to one
  (`ACT 19:40-41 = ACT 19:40`; Paratext marks these with `&`, which is allowed and ignored).
  Writing the same verse on several lines does the same: `PSA 51:0 = PSA 51:1` and
  `PSA 51:0 = PSA 51:2`
- Verse 0 is a psalm's title, which English Bibles don't number
- `#` starts a comment. Lines of verse counts (`MAL 1:14 2:17 3:18 4:6`) and of verses a
  versification leaves out (`-PSA 9:21`), which a whole `.vrs` file has, are skipped

A verse no line names keeps its number, unless another verse was mapped to that number: then it
has no counterpart (English `Psalm 51:20` is no verse, and Hebrew `Malachi 4:1` neither). Maps
work both ways, and through each other: English to Original, then Original to Vulgate.
*/

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use topos_bible::{
    data::{books::BookId, chapter_verses::ChapterVerses},
    segments::{
        formatter::FormatOptions,
        passage::{Passage, Segments},
        segment::Segment,
        verse_bounds::VerseBounds,
    },
};

use crate::{
    error::{LogoxideError, Result},
    manifest::{Format, PackageKind},
    package::Package,
};

/// One verse: its book, chapter, and number (0 for a psalm's title)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Verse {
    pub book: BookId,
    pub chapter: u8,
    pub verse: u8,
}

impl Verse {
    pub fn new(book: BookId, chapter: u8, verse: u8) -> Verse {
        Verse {
            book,
            chapter,
            verse,
        }
    }
}

impl std::fmt::Display for Verse {
    /// `PSA 51:3`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let books = crate::bible_data().books();
        match books.get_usfm(self.book) {
            Some(code) => write!(f, "{code} {}:{}", self.chapter, self.verse),
            None => write!(f, "{} {}:{}", self.book.0, self.chapter, self.verse),
        }
    }
}

/// Verses in one chapter: `PSA 51:1-19`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerseRange {
    pub book: BookId,
    pub chapter: u8,
    pub first: u8,
    pub last: u8,
}

impl VerseRange {
    pub fn verses(&self) -> impl Iterator<Item = Verse> + '_ {
        (self.first..=self.last).map(|v| Verse::new(self.book, self.chapter, v))
    }

    /// How many verses (never none)
    pub fn count(&self) -> usize {
        usize::from(self.last - self.first) + 1
    }
}

/// One line of a map: its verses in one versification, and where they are in the other
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub from: VerseRange,
    pub to: VerseRange,
    /// The file and line it's on (`eng.vrs:12`)
    pub at: String,
}

impl Mapping {
    /// Its verses, pair by pair: verse by verse, or one to each of many
    fn pairs(&self) -> Vec<(Verse, Verse)> {
        match (self.from.count(), self.to.count()) {
            (1, _) => (self.to.verses())
                .map(|to| {
                    (
                        Verse::new(self.from.book, self.from.chapter, self.from.first),
                        to,
                    )
                })
                .collect(),
            (_, 1) => (self.from.verses())
                .map(|from| {
                    (
                        from,
                        Verse::new(self.to.book, self.to.chapter, self.to.first),
                    )
                })
                .collect(),
            _ => self.from.verses().zip(self.to.verses()).collect(),
        }
    }
}

/// A map from one versification to another
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersificationMap {
    /// The versification its left side numbers verses by (`English`)
    pub from: String,
    /// The one its right side does (`Original`)
    pub to: String,
    mappings: Vec<Mapping>,
    forward: BTreeMap<Verse, Vec<Verse>>,
    /// Each verse of `to` that verses of `from` map to
    backward: BTreeMap<Verse, Vec<Verse>>,
}

impl VersificationMap {
    /// A map from its `.vrs` files' text, each with its name (for errors): `(eng.vrs, text)`
    pub fn parse<'a>(
        from: &str,
        to: &str,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<VersificationMap> {
        let mut mappings = vec![];
        for (file, text) in files {
            for (n, line) in text.lines().enumerate() {
                let at = format!("{file}:{}", n + 1);
                if let Some(mapping) = parse_line(line, &at)? {
                    mappings.push(mapping);
                }
            }
        }
        VersificationMap::new(from, to, mappings)
    }

    /// A map from its lines; an error if they overlap (see [`VersificationMap::problems`])
    pub fn new(from: &str, to: &str, mappings: Vec<Mapping>) -> Result<VersificationMap> {
        let mut map = VersificationMap {
            from: from.to_string(),
            to: to.to_string(),
            mappings,
            forward: BTreeMap::new(),
            backward: BTreeMap::new(),
        };
        let problems = map.problems();
        if !problems.is_empty() {
            return Err(LogoxideError::Versification(problems.join("; ")));
        }
        for mapping in &map.mappings {
            for (from, to) in mapping.pairs() {
                let targets = map.forward.entry(from).or_default();
                if !targets.contains(&to) {
                    targets.push(to);
                }
                let sources = map.backward.entry(to).or_default();
                if !sources.contains(&from) {
                    sources.push(from);
                }
            }
        }
        Ok(map)
    }

    /// The map in a package (`kind = "versification-map"`): every `.vrs` file of its enabled
    /// formats, in path order
    pub fn from_package(package: &Package) -> Result<VersificationMap> {
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
        VersificationMap::parse(&info.versification, to, files)
    }

    pub fn mappings(&self) -> &[Mapping] {
        &self.mappings
    }

    /// How many verses it maps (pairs of a verse and where it is)
    pub fn len(&self) -> usize {
        self.forward.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    /// The same map the other way, from `to` to `from`
    pub fn reversed(&self) -> VersificationMap {
        VersificationMap {
            from: self.to.clone(),
            to: self.from.clone(),
            mappings: (self.mappings.iter())
                .map(|m| Mapping {
                    from: m.to,
                    to: m.from,
                    at: m.at.clone(),
                })
                .collect(),
            forward: self.backward.clone(),
            backward: self.forward.clone(),
        }
    }

    /**
    What's wrong with its lines: a verse on one side of several lines, unless each of them names
    it alone on that side (one verse written to each of many, line by line); ranges of different
    lengths where neither is one verse
    */
    pub fn problems(&self) -> Vec<String> {
        let mut problems = vec![];
        for m in &self.mappings {
            if m.from.count() > 1 && m.to.count() > 1 && m.from.count() != m.to.count() {
                problems.push(format!(
                    "{}: {} verses map to {}: give ranges of one length, or one verse",
                    m.at,
                    m.from.count(),
                    m.to.count()
                ));
            }
        }
        for (side, range) in [
            ("left", (|m: &Mapping| m.from) as fn(&Mapping) -> VerseRange),
            ("right", |m: &Mapping| m.to),
        ] {
            // Each verse, with the lines naming it and whether alone
            let mut seen: BTreeMap<Verse, Vec<(&str, bool)>> = BTreeMap::new();
            for m in &self.mappings {
                let r = range(m);
                for verse in r.verses() {
                    seen.entry(verse).or_default().push((&m.at, r.count() == 1));
                }
            }
            for (verse, lines) in seen {
                if lines.len() > 1 && lines.iter().any(|(_, alone)| !alone) {
                    let at: Vec<&str> = lines.iter().map(|(at, _)| *at).collect();
                    problems.push(format!(
                        "{verse} is on the {side} of overlapping lines ({})",
                        at.join(", ")
                    ));
                }
            }
        }
        problems
    }

    /// Where a verse of `from` is in `to`: its verses there (several, when it was split), or
    /// [`None`] if it has no counterpart
    pub fn map_verse(&self, verse: Verse) -> Option<Vec<Verse>> {
        match self.forward.get(&verse) {
            Some(targets) => Some(targets.clone()),
            // Another verse has this number now
            None if self.backward.contains_key(&verse) => None,
            None => Some(vec![verse]),
        }
    }

    /// The last verse its left side names in each chapter of a book (verse 0 counts), by chapter
    fn source_extent(&self, book: BookId) -> BTreeMap<u8, u8> {
        let mut extent: BTreeMap<u8, u8> = BTreeMap::new();
        for verse in self.forward.keys().filter(|v| v.book == book) {
            let last = extent.entry(verse.chapter).or_default();
            *last = (*last).max(verse.verse);
        }
        extent
    }
}

/// The verses of a passage once mapped, and those with no counterpart
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mapped {
    /// In Bible order, each once
    pub verses: Vec<Verse>,
    /// The verses of the passage that aren't in the other versification
    pub missing: Vec<Verse>,
}

impl Mapped {
    /// The verses as passages, one per book, consecutive verses of a chapter as one range
    pub fn passages(&self) -> Vec<Passage> {
        passages(&self.verses)
    }

    /// `Psalms 51:3-4`, books apart by `; ` (empty if no verse has a counterpart)
    pub fn text(&self) -> String {
        write(&self.verses)
    }

    /// The verses with no counterpart, written like [`Mapped::text`]
    pub fn missing_text(&self) -> String {
        write(&self.missing)
    }
}

fn passages(verses: &[Verse]) -> Vec<Passage> {
    let mut out: Vec<Passage> = vec![];
    let mut verses = verses.to_vec();
    verses.sort();
    verses.dedup();
    let mut i = 0;
    while i < verses.len() {
        let start = verses[i];
        let mut end = start;
        while let Some(next) = verses.get(i + 1)
            && next.book == start.book
            && next.chapter == start.chapter
            && u16::from(next.verse) == u16::from(end.verse) + 1
        {
            end = *next;
            i += 1;
        }
        i += 1;
        let segment = match end.verse == start.verse {
            true => Segment::chapter_verse(start.chapter, start.verse),
            false => Segment::chapter_verse_range(start.chapter, start.verse, end.verse),
        };
        match out.last_mut() {
            Some(p) if p.book == start.book => p.segments.push(segment),
            _ => out.push(Passage {
                book: start.book,
                segments: Segments(vec![segment].into()),
            }),
        }
    }
    out
}

fn write(verses: &[Verse]) -> String {
    let data = crate::bible_data();
    let options = FormatOptions::default();
    (passages(verses).iter())
        .filter_map(|p| options.passage(p, data))
        .collect::<Vec<_>>()
        .join("; ")
}

/// `PSA 51:1-19` (a verse, or a range within a chapter)
fn parse_range(text: &str, at: &str) -> Result<VerseRange> {
    let bad = |why: &str| LogoxideError::Versification(format!("{at}: `{text}`: {why}"));
    let (code, numbers) = (text.trim().split_once(' '))
        .ok_or_else(|| bad("write a book's USFM code, then chapter:verse"))?;
    let book = (crate::bible_data().books())
        .search_usfm(code)
        .ok_or_else(|| bad("no book has this USFM code"))?;
    let (chapter, verses) =
        (numbers.trim().split_once(':')).ok_or_else(|| bad("write chapter:verse"))?;
    let number = |n: &str| n.trim().parse::<u8>().map_err(|_| bad("not a number"));
    let chapter = number(chapter)?;
    let (first, last) = match verses.split_once('-') {
        Some((_, last)) if last.contains(':') => {
            return Err(bad("a range can't span chapters"));
        }
        Some((first, last)) => (number(first)?, number(last)?),
        None => (number(verses)?, number(verses)?),
    };
    if last < first {
        return Err(bad("the range ends before it starts"));
    }
    Ok(VerseRange {
        book,
        chapter,
        first,
        last,
    })
}

/// A mapping line, or [`None`] for a comment, a blank line, or a `.vrs` file's other lines
fn parse_line(line: &str, at: &str) -> Result<Option<Mapping>> {
    let line = line.split('#').next().unwrap_or_default().trim();
    let Some((from, to)) = line.split_once('=') else {
        // Verse counts (`MAL 1:14 2:17`), verses left out (`-PSA 9:21`), or nothing
        let number = |n: &str| n.parse::<u8>().is_ok();
        let mut words = line.split_whitespace().skip(1).peekable();
        let counts = words.peek().is_some()
            && words.all(|n| {
                n.split_once(':')
                    .is_some_and(|(c, v)| number(c) && number(v))
            });
        return match line.is_empty() || line.starts_with('-') || counts {
            true => Ok(None),
            false => Err(LogoxideError::Versification(format!(
                "{at}: `{line}` is no mapping (`BOOK C:V = BOOK C:V`)"
            ))),
        };
    };
    let from = from.trim().trim_start_matches('&');
    Ok(Some(Mapping {
        from: parse_range(from, at)?,
        to: parse_range(to, at)?,
        at: at.to_string(),
    }))
}

/// The maps a project has, to map between any two versifications they connect
#[derive(Clone, Debug, Default)]
pub struct Versifications {
    maps: Vec<VersificationMap>,
}

impl Versifications {
    pub fn new(maps: Vec<VersificationMap>) -> Versifications {
        Versifications { maps }
    }

    pub fn add(&mut self, map: VersificationMap) {
        self.maps.push(map);
    }

    pub fn maps(&self) -> &[VersificationMap] {
        &self.maps
    }

    /// Every versification the maps name, in order
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = vec![];
        for map in &self.maps {
            for name in [map.from.as_str(), map.to.as_str()] {
                if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                    names.push(name);
                }
            }
        }
        names
    }

    /// The maps that lead from one versification to another (names ignore case), each turned to
    /// face the way it's followed: none if they're the same, the fewest maps otherwise
    pub fn route(&self, from: &str, to: &str) -> Result<Vec<VersificationMap>> {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        if same(from, to) {
            return Ok(vec![]);
        }
        // Breadth first, each step a map in either direction (`true`: reversed)
        let mut came: BTreeMap<String, (usize, bool, String)> = BTreeMap::new();
        let mut todo = VecDeque::from([from.to_ascii_lowercase()]);
        let mut seen = BTreeSet::from([from.to_ascii_lowercase()]);
        while let Some(at) = todo.pop_front() {
            if same(&at, to) {
                let mut route = vec![];
                let mut here = at;
                while let Some((i, reversed, before)) = came.get(&here) {
                    let map = &self.maps[*i];
                    route.push(if *reversed {
                        map.reversed()
                    } else {
                        map.clone()
                    });
                    here = before.clone();
                }
                route.reverse();
                return Ok(route);
            }
            for (i, map) in self.maps.iter().enumerate() {
                for (start, end, reversed) in
                    [(&map.from, &map.to, false), (&map.to, &map.from, true)]
                {
                    let next = end.to_ascii_lowercase();
                    if same(start, &at) && seen.insert(next.clone()) {
                        came.insert(next.clone(), (i, reversed, at.clone()));
                        todo.push_back(next);
                    }
                }
            }
        }
        let known = self.names().join(", ");
        Err(LogoxideError::Versification(format!(
            "no map leads from {from} to {to} (the maps know: {})",
            if known.is_empty() { "none" } else { &known }
        )))
    }

    /**
    Where a passage numbered by `from` is in `to`. `counts` are the book's verse counts in
    `from` (a Bible's own), which a whole chapter needs; without them topos's are used, and
    either way a chapter reaches the last verse a map names in it. A whole chapter takes in its
    verse 0 (a psalm's title) when a map names it
    */
    pub fn map(
        &self,
        passage: &Passage,
        from: &str,
        to: &str,
        counts: Option<&ChapterVerses>,
    ) -> Result<Mapped> {
        let route = self.route(from, to)?;
        let first = route.first();
        let extent = first
            .map(|m| m.source_extent(passage.book))
            .unwrap_or_default();
        let given = counts
            .or_else(|| (crate::bible_data().chapter_verses()).get_chapter_verses(&passage.book));
        let mut counts: Vec<u8> = given.map(|c| c.verse_counts().to_vec()).unwrap_or_default();
        for (&chapter, &last) in &extent {
            let index = usize::from(chapter.max(1)) - 1;
            if counts.len() <= index {
                counts.resize(index + 1, 0);
            }
            counts[index] = counts[index].max(last);
        }
        let counts = ChapterVerses::new(counts);
        let mut verses: Vec<Verse> = vec![];
        for segment in passage.segments.iter() {
            let whole = matches!(
                segment,
                Segment::FullChapter(_) | Segment::FullChapterRange(_)
            );
            let one = Passage {
                book: passage.book,
                segments: Segments(vec![*segment].into()),
            };
            let named = one.verses(Some(&counts));
            for chapter in segment.starting_chapter()..=segment.ending_chapter() {
                // A whole chapter's title (verse 0), when a map has one
                let title = Verse::new(passage.book, chapter, 0);
                if whole && first.is_some_and(|m| m.forward.contains_key(&title)) {
                    verses.push(title);
                }
                (named.iter())
                    .filter(|cv| cv.chapter == chapter)
                    .for_each(|cv| verses.push(Verse::new(passage.book, chapter, cv.verse)));
            }
        }
        let mut mapped = Mapped::default();
        for verse in verses {
            let mut here = vec![verse];
            for map in &route {
                here = (here.into_iter())
                    .filter_map(|v| map.map_verse(v))
                    .flatten()
                    .collect();
            }
            if here.is_empty() {
                mapped.missing.push(verse);
            }
            mapped.verses.extend(here);
        }
        mapped.verses.sort();
        mapped.verses.dedup();
        mapped.missing.dedup();
        Ok(mapped)
    }

    /// [`Versifications::map`] for a written reference (`Psalm 51:1`, `Joel 4`), parsed by topos.
    /// A reference past topos's own counts is still read (`Malachi 3:24`, `Joel 4:1` in Hebrew
    /// numbering)
    pub fn map_reference(
        &self,
        reference: &str,
        from: &str,
        to: &str,
        counts: Option<&ChapterVerses>,
    ) -> Result<Mapped> {
        let passage = parse_reference(reference)?;
        self.map(&passage, from, to, counts)
    }
}

/// A written reference, checked against topos's counts where it can be (so `Jude 5` is a verse),
/// else read as written
pub fn parse_reference(reference: &str) -> Result<Passage> {
    let data = crate::bible_data();
    if let Some(passage) = data.parse(reference) {
        return Ok(passage);
    }
    let mut passage = (data.books().parse(reference))
        .ok_or_else(|| LogoxideError::Versification(format!("`{reference}` is no reference")))?;
    // In a book of one chapter, a number alone is a verse (`3 John 15`)
    let single = (data.chapter_verses().get_chapter_verses(&passage.book))
        .is_some_and(|c| c.has_one_chapter());
    if single {
        for segment in passage.segments.iter_mut() {
            *segment = match *segment {
                Segment::FullChapter(_) => Segment::chapter_verse(1, segment.starting_chapter()),
                Segment::FullChapterRange(_) => Segment::chapter_verse_range(
                    1,
                    segment.starting_chapter(),
                    segment.ending_chapter(),
                ),
                other => other,
            };
        }
    }
    Ok(passage)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = "\
# English = Original
PSA 51:0 = PSA 51:1
PSA 51:0 = PSA 51:2
PSA 51:1-19 = PSA 51:3-21
MAL 1:14 2:17 3:18 4:6
MAL 4:1-6 = MAL 3:19-24
3JN 1:14 = 3JN 1:14-15
&ACT 19:40-41 = ACT 19:40
";

    fn map() -> Versifications {
        let map = VersificationMap::parse("English", "Original", [("eng.vrs", MAP)]).unwrap();
        Versifications::new(vec![map])
    }

    fn mapped(reference: &str, from: &str, to: &str) -> (String, String) {
        let m = map().map_reference(reference, from, to, None).unwrap();
        (m.text(), m.missing_text())
    }

    #[test]
    fn maps_both_ways() {
        let both = |there: &str| (String::from(there), String::new());
        assert_eq!(
            mapped("Psalm 51:1", "English", "Original"),
            both("Psalm 51:3")
        );
        assert_eq!(
            mapped("Ps 51:1-2", "english", "original"),
            both("Psalm 51:3-4")
        );
        assert_eq!(
            mapped("Psalm 51:3", "Original", "English"),
            both("Psalm 51:1")
        );
        // The title: one verse in English, two in Hebrew
        assert_eq!(
            mapped("Psalm 51:0", "English", "Original"),
            both("Psalm 51:1-2")
        );
        assert_eq!(
            mapped("Psalm 51:2", "Original", "English"),
            both("Psalm 51:0")
        );
        // A whole chapter takes in its title
        assert_eq!(
            mapped("Psalm 51", "English", "Original"),
            both("Psalm 51:1-21")
        );
        assert_eq!(
            mapped("Malachi 4:5", "English", "Original"),
            both("Malachi 3:23")
        );
        assert_eq!(
            mapped("Mal 3:24", "Original", "English"),
            both("Malachi 4:6")
        );
        // Verses no line names keep their numbers
        assert_eq!(
            mapped("Psalm 23:1", "English", "Original"),
            both("Psalm 23:1")
        );
        assert_eq!(
            mapped("John 3:16", "Original", "English"),
            both("John 3:16")
        );
        // Split and joined verses
        assert_eq!(
            mapped("3 John 14", "English", "Original"),
            both("3 John 1:14-15")
        );
        assert_eq!(
            mapped("3 John 15", "Original", "English"),
            both("3 John 1:14")
        );
        assert_eq!(
            mapped("Acts 19:41", "English", "Original"),
            both("Acts 19:40")
        );
        assert_eq!(
            mapped("Acts 19:40", "Original", "English"),
            both("Acts 19:40-41")
        );
        // The same versification
        assert_eq!(
            mapped("Psalm 51:1", "English", "ENGLISH"),
            both("Psalm 51:1")
        );
    }

    #[test]
    fn says_what_has_no_counterpart() {
        // Hebrew Malachi has three chapters; English Psalm 51 has 19 verses
        let (text, missing) = mapped("Malachi 4:1", "Original", "English");
        assert_eq!((text.as_str(), missing.as_str()), ("", "Malachi 4:1"));
        let (text, missing) = mapped("Psalm 51:19-21", "Original", "English");
        assert_eq!((text.as_str(), missing.as_str()), ("Psalm 51:17-19", ""));
        let m = map()
            .map_reference("Psalm 51:18-19", "English", "Original", None)
            .unwrap();
        assert_eq!(m.text(), "Psalm 51:20-21");
        assert!(
            map()
                .map_reference("Psalm 51:1", "English", "Vulgate", None)
                .is_err()
        );
    }

    #[test]
    fn follows_maps_through_each_other() {
        let mut maps = map();
        let vulgate = "PSA 50:1-21 = PSA 51:1-21\n";
        maps.add(VersificationMap::parse("Vulgate", "Original", [("vul.vrs", vulgate)]).unwrap());
        let m = maps
            .map_reference("Psalm 51:1", "English", "Vulgate", None)
            .unwrap();
        assert_eq!(m.text(), "Psalm 50:3");
        assert_eq!(maps.route("English", "Vulgate").unwrap().len(), 2);
        assert_eq!(maps.names(), ["English", "Original", "Vulgate"]);
    }

    #[test]
    fn mistakes_are_explained() {
        let bad = |text: &str| {
            VersificationMap::parse("A", "B", [("m.vrs", text)])
                .unwrap_err()
                .to_string()
        };
        assert!(bad("XYZ 1:1 = GEN 1:1").contains("m.vrs:1"));
        assert!(bad("GEN 1:1-3 = GEN 1:2-3").contains("one length"));
        assert!(bad("GEN 1:1-3 = GEN 1:2-4\nGEN 1:3 = GEN 1:9").contains("GEN 1:3 is on the left"));
        assert!(
            bad("GEN 1:1 = GEN 1:5\nGEN 1:2-3 = GEN 1:5-6").contains("GEN 1:5 is on the right")
        );
        assert!(bad("GEN 4:10-5:11 = GEN 4:10-5:11").contains("span chapters"));
        assert!(bad("GEN 1:3-1 = GEN 1:1").contains("ends before"));
        assert!(bad("hello").contains("no mapping"));
        // A whole .vrs file's other lines are skipped
        let ok = "# Versification \"English\"\nGEN 1:31 2:25\n-PSA 9:21\n\nPSA 3:0-8 = PSA 3:1-9 # titles";
        let map = VersificationMap::parse("English", "Original", [("eng.vrs", ok)]).unwrap();
        assert_eq!((map.mappings().len(), map.len()), (1, 9));
    }
}
