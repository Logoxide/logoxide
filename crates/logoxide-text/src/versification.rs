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
- Several lines may map onto one verse of `maps-to`, in any form: the BSB's own file has
  `ESG 5:10-11 = ESG 5:1-2`, `ESG 5:12 = ESG 5:2`, and `ESG 5:16-28 = ESG 5:2-14` (and
  `ESG 5:13 = ESG 5:2a`, …), lines ending in Greek Esther 5:2, whose words the BSB numbers six
  ways. Mapped back, such a verse is every verse mapped onto it (Original `ESG 5:2` is the BSB's
  5:11-16), as for `&ACT 19:40-41 = ACT 19:40`. On the left side, a verse on several lines is a mistake unless
  each names it alone (one verse written to each of many)
- Verse 0 is a psalm's title, which English Bibles don't number
- A verse may name a part (`ESG 1:1a`, Paratext's lettered parts): it's mapped as the whole
  verse, so `ESG 1:1 = ESG 1:1a` and `ESG 1:2 = ESG 1:1b` map both to 1:1; the part is kept in
  [`VerseRange`] to show. Lines naming parts of one verse don't overlap
- `#` starts a comment. A whole `.vrs` file also has lines of verse counts (`MAL 1:14 2:17 3:18
  4:6`: the last verse of each chapter) and of verses its versification leaves out (`-PSA 9:21`),
  both describing `versification` (the left side). A mapping naming a verse past its book's
  counts is a mistake ([`VersificationMap::problems`]); a verse left out has no counterpart, and
  nothing maps to it the other way. Those lines for books topos doesn't know are passed over

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

use crate::error::{Result, TextError};

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

/// Verses in one chapter: `PSA 51:1-19`, or part of a verse: `ESG 1:1a`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerseRange {
    pub book: BookId,
    pub chapter: u8,
    pub first: u8,
    pub last: u8,
    /// The part of the first verse named (`a` in `1:1a`), when only a part is: it's mapped as the
    /// whole verse, and the part kept to show
    pub first_part: Option<char>,
    /// The part of the last verse named (`b` in `22:43-44b`)
    pub last_part: Option<char>,
}

impl VerseRange {
    pub fn verses(&self) -> impl Iterator<Item = Verse> + '_ {
        (self.first..=self.last).map(|v| Verse::new(self.book, self.chapter, v))
    }

    /// How many verses (never none)
    pub fn count(&self) -> usize {
        usize::from(self.last - self.first) + 1
    }

    /// The part of a verse of it that's named, if only a part is
    pub fn part_of(&self, verse: u8) -> Option<char> {
        match verse {
            v if v == self.first && self.first_part.is_some() => self.first_part,
            v if v == self.last => self.last_part,
            _ => None,
        }
    }
}

impl std::fmt::Display for VerseRange {
    /// `PSA 51:1-19`, `ESG 1:1a`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let books = crate::bible_data().books();
        match books.get_usfm(self.book) {
            Some(code) => write!(f, "{code} {}:", self.chapter)?,
            None => write!(f, "{} {}:", self.book.0, self.chapter)?,
        }
        let part = |p: Option<char>| p.map(String::from).unwrap_or_default();
        write!(f, "{}{}", self.first, part(self.first_part))?;
        if self.last != self.first || self.last_part != self.first_part {
            write!(f, "-{}{}", self.last, part(self.last_part))?;
        }
        Ok(())
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
    /// Its verses, pair by pair: verse by verse, or one to each of many. Ranges of different
    /// lengths, neither one verse, are an error ([`VersificationMap::problems`] says so first)
    fn pairs(&self) -> Result<Vec<(Verse, Verse)>> {
        let (from, to) = (self.from, self.to);
        Ok(match (from.count(), to.count()) {
            (1, _) => (to.verses())
                .map(|to| (Verse::new(from.book, from.chapter, from.first), to))
                .collect(),
            (_, 1) => (from.verses())
                .map(|from| (from, Verse::new(to.book, to.chapter, to.first)))
                .collect(),
            (a, b) if a == b => from.verses().zip(to.verses()).collect(),
            (a, b) => {
                return Err(TextError::Versification(format!(
                    "{}: {a} verses map to {b}: give ranges of one length, or one verse",
                    self.at
                )));
            }
        })
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
    /// The last verse of each chapter of a book in `from`, as the `.vrs` file's count lines give
    /// them (books without a line aren't checked)
    from_counts: BTreeMap<BookId, Vec<u8>>,
    /// The same in `to` (only a reversed map has them)
    to_counts: BTreeMap<BookId, Vec<u8>>,
    /// Verses `from` leaves out (`-PSA 9:21`): none of them has a counterpart
    from_excluded: BTreeSet<Verse>,
    /// Verses `to` leaves out (only a reversed map has them): nothing maps to them
    to_excluded: BTreeSet<Verse>,
    /// Mapping lines passed over, each where it is and the book topos doesn't know
    passed_over: Vec<(String, String)>,
}

impl VersificationMap {
    /**
    A map from its `.vrs` files' text, each with its name (for errors): `(eng.vrs, text)`. Their
    verse-count lines and lines of verses left out describe `from`
    */
    pub fn parse<'a>(
        from: &str,
        to: &str,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<VersificationMap> {
        let mut mappings = vec![];
        let (mut counts, mut excluded) = (BTreeMap::new(), BTreeSet::new());
        let mut passed_over = vec![];
        for (file, text) in files {
            for (n, line) in text.lines().enumerate() {
                let at = format!("{file}:{}", n + 1);
                match parse_line(line, &at)? {
                    Line::Mapping(mapping) => mappings.push(mapping),
                    Line::Counts(book, chapters) => {
                        counts.insert(book, chapters);
                    }
                    Line::Excluded(range) => excluded.extend(range.verses()),
                    Line::Unknown(code) => passed_over.push((at, code)),
                    Line::Other => {}
                }
            }
        }
        let mut map = VersificationMap::build(from, to, mappings, counts, excluded)?;
        map.passed_over = passed_over;
        Ok(map)
    }

    /// The mapping lines passed over, each where it is (`eng.vrs:375`) and the book code topos
    /// doesn't know (`DAG`)
    pub fn passed_over(&self) -> &[(String, String)] {
        &self.passed_over
    }

    /// A map from its lines; an error if they overlap (see [`VersificationMap::problems`])
    pub fn new(from: &str, to: &str, mappings: Vec<Mapping>) -> Result<VersificationMap> {
        VersificationMap::build(from, to, mappings, BTreeMap::new(), BTreeSet::new())
    }

    fn build(
        from: &str,
        to: &str,
        mappings: Vec<Mapping>,
        from_counts: BTreeMap<BookId, Vec<u8>>,
        from_excluded: BTreeSet<Verse>,
    ) -> Result<VersificationMap> {
        let mut map = VersificationMap {
            from: from.to_string(),
            to: to.to_string(),
            mappings,
            forward: BTreeMap::new(),
            backward: BTreeMap::new(),
            from_counts,
            to_counts: BTreeMap::new(),
            from_excluded,
            to_excluded: BTreeSet::new(),
            passed_over: vec![],
        };
        let mistakes = map.mistakes();
        if !mistakes.is_empty() {
            return Err(TextError::Versification(mistakes.join("; ")));
        }
        for mapping in &map.mappings {
            for (from, to) in mapping.pairs()? {
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
            from_counts: self.to_counts.clone(),
            to_counts: self.from_counts.clone(),
            from_excluded: self.to_excluded.clone(),
            to_excluded: self.from_excluded.clone(),
            passed_over: self.passed_over.clone(),
        }
    }

    /// The verses `from` leaves out (`-PSA 9:21` lines): they have no counterpart
    pub fn excluded(&self) -> &BTreeSet<Verse> {
        &self.from_excluded
    }

    /// The last verse of each chapter of the books whose counts the `.vrs` files give, in `from`
    pub fn counts(&self) -> &BTreeMap<BookId, Vec<u8>> {
        &self.from_counts
    }

    /**
    What's wrong with its lines: its mistakes (which a map can't be made with: lines that overlap,
    ranges that can't be paired), then the verses its lines name that the `.vrs` files' counts say
    don't exist (verse 0, a psalm's title, always may). Those are only said: Paratext's own files
    have some (the BSB's maps Greek Esther 8:36-41, of a chapter of 39 verses)
    */
    pub fn problems(&self) -> Vec<String> {
        let mut problems = self.mistakes();
        problems.extend(self.past_counts());
        problems
    }

    /// The verses lines name that the counts say don't exist
    fn past_counts(&self) -> Vec<String> {
        let mut problems = vec![];
        for m in &self.mappings {
            for (range, counts, name) in [
                (m.from, &self.from_counts, &self.from),
                (m.to, &self.to_counts, &self.to),
            ] {
                let Some(chapters) = counts.get(&range.book) else {
                    continue;
                };
                let last = (usize::from(range.chapter).checked_sub(1))
                    .and_then(|i| chapters.get(i))
                    .copied();
                match last {
                    None => problems.push(format!(
                        "{}: {range}: {name} has no chapter {}",
                        m.at, range.chapter
                    )),
                    Some(last) if range.last > last => problems.push(format!(
                        "{}: {range}: chapter {} has {last} verses in {name}",
                        m.at, range.chapter
                    )),
                    Some(_) => {}
                }
            }
        }
        problems
    }

    /**
    The lines a map can't be made with: a verse on the left of several lines, unless each of them
    names it alone there (one verse written to each of many, line by line); ranges of different
    lengths where neither is one verse. A part of a verse (`ESG 1:1a`) is apart from the rest of
    it, so lines naming parts overlap nothing. The right side may overlap: several verses mapped
    onto one are many to one, which Paratext's own files have (the BSB's Greek Esther 5:2)
    */
    fn mistakes(&self) -> Vec<String> {
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
        // Each verse of the left side, with the lines naming it (not only a part of it) and
        // whether alone
        let mut seen: BTreeMap<Verse, Vec<(&str, bool)>> = BTreeMap::new();
        for m in &self.mappings {
            for verse in m.from.verses() {
                if m.from.part_of(verse.verse).is_none() {
                    seen.entry(verse)
                        .or_default()
                        .push((&m.at, m.from.count() == 1));
                }
            }
        }
        for (verse, lines) in seen {
            if lines.len() > 1 && lines.iter().any(|(_, alone)| !alone) {
                let at: Vec<&str> = lines.iter().map(|(at, _)| *at).collect();
                problems.push(format!(
                    "{verse} is on the left of overlapping lines ({})",
                    at.join(", ")
                ));
            }
        }
        problems
    }

    /// Where a verse of `from` is in `to`: its verses there (several, when it was split), or
    /// [`None`] if it has no counterpart (one of the two leaves it out)
    pub fn map_verse(&self, verse: Verse) -> Option<Vec<Verse>> {
        if self.from_excluded.contains(&verse) {
            return None;
        }
        let targets = match self.forward.get(&verse) {
            Some(targets) => targets.clone(),
            // Another verse has this number now
            None if self.backward.contains_key(&verse) => return None,
            None => vec![verse],
        };
        let targets: Vec<Verse> = (targets.into_iter())
            .filter(|v| !self.to_excluded.contains(v))
            .collect();
        (!targets.is_empty()).then_some(targets)
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

/// `PSA 51:1-19` (a verse, or a range within a chapter), either end perhaps a part of a verse
/// (`ESG 1:1a`, `1KI 22:43b-44`)
fn parse_range(text: &str, at: &str) -> Result<VerseRange> {
    let bad = |why: &str| TextError::Versification(format!("{at}: `{text}`: {why}"));
    let (code, numbers) = (text.trim().split_once(' '))
        .ok_or_else(|| bad("write a book's USFM code, then chapter:verse"))?;
    let book = (crate::bible_data().books())
        .search_usfm(code)
        .ok_or_else(|| bad("no book has this USFM code"))?;
    let (chapter, verses) =
        (numbers.trim().split_once(':')).ok_or_else(|| bad("write chapter:verse"))?;
    let number = |n: &str| n.trim().parse::<u8>().map_err(|_| bad("not a number"));
    // A verse, perhaps with a part's letter after it (`20a`)
    let verse = |n: &str| -> Result<(u8, Option<char>)> {
        let n = n.trim();
        match n.chars().last() {
            Some(part) if part.is_ascii_lowercase() => Ok((number(&n[..n.len() - 1])?, Some(part))),
            _ => Ok((number(n)?, None)),
        }
    };
    let chapter = number(chapter)?;
    let ((first, first_part), (last, last_part)) = match verses.split_once('-') {
        Some((_, last)) if last.contains(':') => {
            return Err(bad("a range can't span chapters"));
        }
        Some((first, last)) => (verse(first)?, verse(last)?),
        None => (verse(verses)?, verse(verses)?),
    };
    if last < first {
        return Err(bad("the range ends before it starts"));
    }
    Ok(VerseRange {
        book,
        chapter,
        first,
        last,
        first_part,
        last_part,
    })
}

/// A line of a `.vrs` file
enum Line {
    Mapping(Mapping),
    /// A book's verse counts (`MAL 1:14 2:17 3:18 4:6`): the last verse of each chapter
    Counts(BookId, Vec<u8>),
    /// Verses the versification leaves out (`-PSA 9:21`)
    Excluded(VerseRange),
    /// A mapping naming a book topos doesn't know (its USFM code)
    Unknown(String),
    /// A comment, a blank line, or counts or verses left out of a book topos doesn't know
    Other,
}

/// A line of a `.vrs` file: a mapping, a book's verse counts, or verses left out
fn parse_line(line: &str, at: &str) -> Result<Line> {
    let line = line.split('#').next().unwrap_or_default().trim();
    let known = |code: &str| crate::bible_data().books().search_usfm(code);
    if let Some(excluded) = line.strip_prefix('-') {
        let code = excluded.split_whitespace().next().unwrap_or_default();
        return match known(code) {
            Some(_) => Ok(Line::Excluded(parse_range(excluded, at)?)),
            None => Ok(Line::Other),
        };
    }
    let Some((from, to)) = line.split_once('=') else {
        // Verse counts (`MAL 1:14 2:17`), or nothing
        let number = |n: &str| n.parse::<u8>().ok();
        let mut words = line.split_whitespace();
        let code = words.next().unwrap_or_default();
        let pairs: Option<Vec<(u8, u8)>> = (words.map(|w| {
            let (c, v) = w.split_once(':')?;
            Some((number(c)?, number(v)?))
        }))
        .collect();
        return match (pairs, line.is_empty()) {
            (_, true) => Ok(Line::Other),
            (Some(pairs), false) if !pairs.is_empty() => {
                let Some(book) = known(code) else {
                    return Ok(Line::Other);
                };
                let mut counts: Vec<u8> = vec![];
                for (chapter, last) in pairs.into_iter().filter(|(c, _)| *c > 0) {
                    let index = usize::from(chapter) - 1;
                    if counts.len() <= index {
                        counts.resize(index + 1, 0);
                    }
                    counts[index] = last;
                }
                Ok(Line::Counts(book, counts))
            }
            _ => Err(TextError::Versification(format!(
                "{at}: `{line}` is no mapping (`BOOK C:V = BOOK C:V`)"
            ))),
        };
    };
    let from = from.trim().trim_start_matches('&');
    // A book with a USFM code topos doesn't know (yet): the line is passed over, and said
    let looks_like_code = |c: &str| {
        c.len() == 3
            && c.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    };
    for side in [from, to] {
        let code = side.split_whitespace().next().unwrap_or_default();
        if looks_like_code(code) && known(code).is_none() {
            return Ok(Line::Unknown(code.to_string()));
        }
    }
    Ok(Line::Mapping(Mapping {
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
        Err(TextError::Versification(format!(
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
        .ok_or_else(|| TextError::Versification(format!("`{reference}` is no reference")))?;
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
        assert!(bad("Genesis 1:1 = GEN 1:1").contains("m.vrs:1"));
        // A book topos doesn't know is passed over, and said
        let map = VersificationMap::parse(
            "A",
            "B",
            [("m.vrs", "GEN 1:1 = GEN 1:2\nZZZ 1:1 = ZZZ 1:2")],
        )
        .unwrap();
        assert_eq!(map.mappings().len(), 1);
        assert_eq!(
            map.passed_over(),
            [(String::from("m.vrs:2"), String::from("ZZZ"))]
        );
        assert!(bad("GEN 1:1-3 = GEN 1:2-3").contains("one length"));
        assert!(bad("GEN 1:1-3 = GEN 1:2-4\nGEN 1:3 = GEN 1:9").contains("GEN 1:3 is on the left"));
        assert!(bad("GEN 1:1-2 = GEN 1:5-6\nGEN 1:2 = GEN 1:9").contains("GEN 1:2 is on the left"));
        assert!(bad("GEN 4:10-5:11 = GEN 4:10-5:11").contains("span chapters"));
        assert!(bad("GEN 1:3-1 = GEN 1:1").contains("ends before"));
        assert!(bad("hello").contains("no mapping"));
        // A whole .vrs file's other lines are skipped
        let ok = "# Versification \"English\"\nGEN 1:31 2:25\n-PSA 9:21\n\nPSA 3:0-8 = PSA 3:1-9 # titles";
        let map = VersificationMap::parse("English", "Original", [("eng.vrs", ok)]).unwrap();
        assert_eq!((map.mappings().len(), map.len()), (1, 9));
        // Ranges of different lengths can't be paired, even made by hand
        let gen_ = crate::bible_data().books().search_usfm("GEN").unwrap();
        let range = |first, last| VerseRange {
            book: gen_,
            chapter: 1,
            first,
            last,
            first_part: None,
            last_part: None,
        };
        let uneven = Mapping {
            from: range(1, 3),
            to: range(1, 2),
            at: String::from("by hand"),
        };
        assert!(uneven.pairs().is_err());
        assert!(VersificationMap::new("A", "B", vec![uneven]).is_err());
    }

    #[test]
    fn parts_of_verses_map_as_whole_verses() {
        // As the BSB's versification.vrs has them: Greek Esther's verses in the parts of 1:1
        let text = "ESG 1:1 = ESG 1:1a\nESG 1:2 = ESG 1:1b\nESG 1:3 = ESG 1:1c\nGEN 35:20a = GEN 35:21\nGEN 35:20b-21 = GEN 35:22-23\n";
        let map = VersificationMap::parse("BSB", "Original", [("b.vrs", text)]).unwrap();
        let shown: Vec<String> = (map.mappings().iter())
            .map(|m| format!("{} = {}", m.from, m.to))
            .collect();
        assert_eq!(
            shown,
            [
                "ESG 1:1 = ESG 1:1a",
                "ESG 1:2 = ESG 1:1b",
                "ESG 1:3 = ESG 1:1c",
                "GEN 35:20a = GEN 35:21",
                "GEN 35:20b-21 = GEN 35:22-23",
            ]
        );
        let maps = Versifications::new(vec![map]);
        let to = |r: &str| {
            let m = maps.map_reference(r, "BSB", "Original", None).unwrap();
            m.text()
        };
        assert_eq!(to("Greek Esther 1:2"), "Greek Esther 1:1");
        assert_eq!(to("Genesis 35:20"), "Genesis 35:21-22");
        let back = maps
            .map_reference("Greek Esther 1:1", "Original", "BSB", None)
            .unwrap();
        assert_eq!(back.text(), "Greek Esther 1:1-3");
    }

    #[test]
    fn verses_left_out_and_counts() {
        let text = "PSA 9:20 10:18\n-PSA 9:21\nPSA 10:1 = PSA 9:22\n";
        let map = VersificationMap::parse("English", "Original", [("e.vrs", text)]).unwrap();
        let psa = crate::bible_data().books().search_usfm("PSA").unwrap();
        assert!(map.excluded().contains(&Verse::new(psa, 9, 21)));
        assert_eq!(
            map.counts().get(&psa),
            Some(&vec![0, 0, 0, 0, 0, 0, 0, 0, 20, 18])
        );
        let maps = Versifications::new(vec![map]);
        // Left out: no counterpart either way
        let m = maps
            .map_reference("Psalm 9:21", "English", "Original", None)
            .unwrap();
        assert_eq!(
            (m.text(), m.missing_text()),
            (String::new(), String::from("Psalm 9:21"))
        );
        let m = maps
            .map_reference("Psalm 9:21", "Original", "English", None)
            .unwrap();
        assert_eq!(m.missing_text(), "Psalm 9:21");
        // Past the counts: a verse English doesn't have, said but mapped
        let text = "PSA 9:20\nPSA 9:25 = PSA 9:26\nPSA 10:1 = PSA 10:1";
        let map = VersificationMap::parse("English", "O", [("e.vrs", text)]).unwrap();
        assert_eq!(
            map.problems(),
            [
                "e.vrs:2: PSA 9:25: chapter 9 has 20 verses in English",
                "e.vrs:3: PSA 10:1: English has no chapter 10",
            ]
        );
        assert_eq!(map.len(), 2);
        // A book topos doesn't know, in counts or left out, is passed over
        let map = VersificationMap::parse("A", "B", [("x.vrs", "ZZZ 1:5\n-ZZZ 1:3\n")]).unwrap();
        assert!(map.counts().is_empty() && map.excluded().is_empty());
    }

    #[test]
    fn several_lines_may_map_onto_one_verse() {
        // The BSB's versification.vrs: three lines end in Greek Esther 5:2
        let text = "ESG 5:10-11 = ESG 5:1-2\nESG 5:12 = ESG 5:2\nESG 5:13 = ESG 5:2a\nESG 5:16-28 = ESG 5:2-14\n";
        let map = VersificationMap::parse("BSB", "Original", [("b.vrs", text)]).unwrap();
        assert!(map.problems().is_empty(), "{:?}", map.problems());
        let maps = Versifications::new(vec![map]);
        let to = |r: &str, from: &str, to: &str| {
            let m = maps.map_reference(r, from, to, None).unwrap();
            (m.text(), m.missing_text())
        };
        let found = |there: &str| (String::from(there), String::new());
        // Forward, each verse has its one place
        assert_eq!(
            to("Greek Esther 5:11", "BSB", "Original"),
            found("Greek Esther 5:2")
        );
        assert_eq!(
            to("Greek Esther 5:12", "BSB", "Original"),
            found("Greek Esther 5:2")
        );
        assert_eq!(
            to("Greek Esther 5:17", "BSB", "Original"),
            found("Greek Esther 5:3")
        );
        // Back, every verse mapped onto it
        assert_eq!(
            to("Greek Esther 5:2", "Original", "BSB"),
            found("Greek Esther 5:11-13,16")
        );
    }
}
