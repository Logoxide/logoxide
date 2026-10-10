/*!
Where each verse is in a recording: WebVTT, each cue named by its verse (`JHN 3:16`, or
`JHN 3:17-18` for a bridge), its time range the verse's. Cues not named by a verse
(introductions, music) are skipped, so any subtitle player shows the file as it is.
*/

use std::collections::BTreeMap;

use topos_bible::data::books::BookId;

use crate::{
    Source,
    error::{Result, TextError},
};

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
    let bad = |line: usize, what: &str| TextError::Timings(format!("line {line}: {what}"));
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
What's wrong with a file's timings, against the verses its text has (`verses(book, chapter)`:
the chapter's verse numbers, a bridge's each, with 0 where the text numbers a title so; or
`None` for a chapter it lacks): verses it doesn't have, another book than `file_book`, the one
the file reads (if known), and verses that overlap or go backwards
*/
pub fn check_timings(
    file_book: Option<&str>,
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
        if let Some(reads) = file_book
            && reads != t.book
        {
            problems.push(format!("{name}: the file reads {reads}"));
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

/// Each chapter's verse numbers in a book of `source` (a bridge's each, 0 for a title numbered
/// so), by chapter: what timings are checked against
pub fn book_verses(source: &dyn Source, id: BookId) -> BTreeMap<u16, Vec<u16>> {
    let mut chapters: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
    for span in source.book(id).map(|b| b.spans).unwrap_or_default() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_webvtt_cues_named_by_verse() {
        let vtt = "WEBVTT\n\nNOTE a note\n\nintro\n00:00.000 --> 00:04.500\nmusic\n\nJHN 3:16\n00:04.500 --> 00:09.200\nFor God so loved\n\nJHN 3:17-18\n00:09.200 --> 01:02:03.004\n";
        let timings = parse_timings(vtt).unwrap();
        assert_eq!(timings.len(), 2);
        assert_eq!(
            (timings[0].verse, timings[0].start_ms, timings[0].end_ms),
            (16, 4500, 9200)
        );
        assert_eq!(
            (timings[1].verse, timings[1].last, timings[1].end_ms),
            (17, 18, 3_723_004)
        );
        assert!(parse_timings("not vtt").is_err());
        assert!(parse_timings("WEBVTT\n\nJHN 1:1\n00:05.000 --> 00:04.000\n").is_err());
    }

    #[test]
    fn checks_timings_against_verses() {
        let t = |chapter, verse, last, start, end| VerseTiming {
            book: String::from("JHN"),
            chapter,
            verse,
            last,
            start_ms: start,
            end_ms: end,
        };
        let verses = |_: &str, chapter: u16| (chapter == 3).then(|| (1..=36).collect());
        let problems = check_timings(
            Some("MRK"),
            &[
                t(3, 16, 16, 0, 10),
                t(3, 15, 15, 5, 12),
                t(4, 1, 1, 12, 13),
                t(3, 37, 37, 20, 21),
            ],
            verses,
        );
        assert_eq!(problems.len(), 9, "{problems:?}");
        assert!(problems[0].contains("the file reads MRK"));
        assert!(
            problems
                .iter()
                .any(|p| p.contains("out of the text's order"))
        );
        assert!(problems.iter().any(|p| p.contains("starts before")));
        assert!(problems.iter().any(|p| p.contains("no such chapter")));
        assert!(problems.iter().any(|p| p.contains("no verse 37")));
    }
}
