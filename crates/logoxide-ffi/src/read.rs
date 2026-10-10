//! Reading and comparing passages in a project's Bibles natively, through logoxide-app's `read`
//! (what `lx read` and `lx compare` print): `Logoxide.readPassage` and `comparePassage`. They
//! read the project's folder and the library's, so not in WebAssembly, where the host opens each
//! package (`openPackage`) and reads its books (`LogoxidePackage.book`, `styledText`)

use boltffi::*;
use logoxide_core::topos_bible::segments::passage::Passage as CorePassage;
use logoxide_text::passage::{Style, Text};
use topos_bible::{Passage, PassageSegment};

use crate::{BookTextStyle, LogoxideError, NodeKind, StyledText, TextMark};

/// A passage read from one of a project's Bibles
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadPassage {
    /// What the manifest calls the Bible
    pub bible: String,
    /// What references name it by here (`KJV`)
    pub abbreviation: String,
    pub passage: Passage,
    /// Its text in the style given, or none if the Bible has no text there
    pub text: Option<StyledText>,
    /// What the Bible numbers its verses by (`KJV`, `own`)
    pub versification: String,
}

/// A passage in one of the Bibles compared
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComparedPassage {
    pub bible: String,
    pub abbreviation: String,
    /// Where the passage is in this Bible's numbering, written out (`Psalm 51:1-2; 52:1`;
    /// empty if nothing of it is)
    pub reference: String,
    /// The same, a passage a book
    pub passages: Vec<Passage>,
    pub text: Option<StyledText>,
    /// The verses of the passage this Bible doesn't have, in the first Bible's numbering
    /// (written out; empty if none)
    pub missing: String,
    /// Whether it was mapped from another numbering
    pub mapped: bool,
    pub versification: String,
}

/// topos-ffi's record of a passage
fn passage(passage: &CorePassage) -> Passage {
    let books = logoxide_core::bible_data().books();
    Passage {
        book_id: passage.book.0,
        book: books.get_name(passage.book).cloned().unwrap_or_default(),
        reference: logoxide_app::read::write(passage),
        segments: passage.segments.iter().map(PassageSegment::from).collect(),
        osis: passage.to_osis(books).unwrap_or_default(),
        usfm: passage.to_usfm(books).unwrap_or_default(),
    }
}

/// The text with its marks, cut after `limit` bytes (at a character's end) with an ellipsis
fn styled(text: Text, limit: Option<u32>) -> StyledText {
    let mut text = text;
    if let Some(limit) = limit.map(|l| l as usize)
        && text.text.len() > limit
    {
        let mut end = limit;
        while !text.text.is_char_boundary(end) {
            end -= 1;
        }
        text.text.truncate(end);
        text.text.push_str(" …");
        text.marks.retain(|(range, _)| range.start < end);
        for (range, _) in &mut text.marks {
            range.end = range.end.min(end);
        }
    }
    StyledText {
        marks: (text.marks.iter())
            .map(|(r, kind)| TextMark {
                start: r.start as u32,
                end: r.end as u32,
                kind: NodeKind::from(*kind),
            })
            .collect(),
        text: text.text,
    }
}

/// The project in `project_dir` and the library at `library_dir` (its cache in `cache_dir`)
#[cfg(not(target_arch = "wasm32"))]
fn open(
    project_dir: String,
    library_dir: String,
    cache_dir: String,
) -> Result<(logoxide_core::Project, logoxide_core::Library), LogoxideError> {
    let project = logoxide_core::Project::open(project_dir)?;
    Ok((project, logoxide_core::Library::new(library_dir, cache_dir)))
}

#[cfg(target_arch = "wasm32")]
fn open(
    project_dir: String,
    library_dir: String,
    cache_dir: String,
) -> Result<(logoxide_core::Project, logoxide_core::Library), LogoxideError> {
    let _ = (project_dir, library_dir, cache_dir);
    Err(LogoxideError::Io {
        message: String::from("no file access in WebAssembly: open each package with openPackage"),
    })
}

pub(crate) fn read_passage(
    project_dir: String,
    library_dir: String,
    cache_dir: String,
    reference: String,
    style: BookTextStyle,
) -> Result<ReadPassage, LogoxideError> {
    let (project, library) = open(project_dir, library_dir, cache_dir)?;
    let reader = logoxide_core::reader::DefaultReader;
    let read = logoxide_app::read::read(
        &project,
        &library,
        &reader,
        &reference,
        &Style::from(&style),
    )?;
    Ok(ReadPassage {
        bible: read.bible,
        abbreviation: read.abbreviation,
        passage: passage(&read.passage),
        text: read.text.map(|t| styled(t, style.limit)),
        versification: read.versification,
    })
}

pub(crate) fn compare_passage(
    project_dir: String,
    library_dir: String,
    cache_dir: String,
    reference: String,
    bibles: Vec<String>,
    style: BookTextStyle,
) -> Result<Vec<ComparedPassage>, LogoxideError> {
    let (project, library) = open(project_dir, library_dir, cache_dir)?;
    let reader = logoxide_core::reader::DefaultReader;
    let compared = logoxide_app::read::compare(
        &project,
        &library,
        &reader,
        &reference,
        &bibles,
        &Style::from(&style),
    )?;
    Ok((compared.into_iter())
        .map(|c| ComparedPassage {
            bible: c.bible,
            abbreviation: c.abbreviation,
            reference: c.reference,
            passages: c.passages.iter().map(passage).collect(),
            text: c.text.map(|t| styled(t, style.limit)),
            missing: c.missing,
            mapped: c.mapped,
            versification: c.versification,
        })
        .collect())
}
