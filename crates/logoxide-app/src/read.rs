/*!
Reading passages from a project's Bibles: [`read`] (`John 3:16 KJV`: the passage in the Bible it
names, else the project's first) and [`compare`] (one passage in several Bibles, each in its own
numbering through the project's versification maps, with what isn't in it). The text is
logoxide-text's [`passage`](logoxide_text::passage), in the [`Style`] given
*/

use logoxide::{
    CachedPackage, Library, LockedPackage, LogoxideError, Project, Result, TextReader, bible_data,
};
use logoxide_text::passage::{Style, Text};
use topos_bible::{
    data::chapter_verses::ChapterVerses,
    segments::{formatter::FormatOptions, passage::Passage},
};

/// A passage read from one Bible
#[derive(Clone, Debug)]
pub struct Read {
    /// What the manifest calls the Bible
    pub bible: String,
    /// What references name it by here (`KJV`)
    pub abbreviation: String,
    pub passage: Passage,
    /// The passage written out (`John 3:16`)
    pub reference: String,
    /// The passage as OSIS (`John.3.16`)
    pub osis: String,
    /// Its text, or none if the Bible has no text there
    pub text: Option<Text>,
    /// What the Bible numbers its verses by
    pub versification: String,
}

/// A passage in one of the Bibles compared
#[derive(Clone, Debug)]
pub struct Compared {
    pub bible: String,
    pub abbreviation: String,
    /// Where the passage is in this Bible's numbering (written out; empty if nothing of it is)
    pub reference: String,
    pub passages: Vec<Passage>,
    pub text: Option<Text>,
    /// The verses of the passage this Bible doesn't have, in the first Bible's numbering
    /// (written out; empty if none)
    pub missing: String,
    /// Whether it was mapped from another numbering
    pub mapped: bool,
    pub versification: String,
}

/// A passage written out (`John 3:16-18`)
pub fn write(passage: &Passage) -> String {
    (FormatOptions::default().passage(passage, bible_data())).unwrap_or_default()
}

/// A passage as OSIS (`John.3.16-John.3.18`)
fn osis(passage: &Passage) -> String {
    passage.to_osis(bible_data().books()).unwrap_or_default()
}

/// One of the project's Bibles, and its names
struct Bible<'a> {
    locked: &'a LockedPackage,
    cached: CachedPackage,
    abbreviation: String,
    aliases: Vec<String>,
}

impl Bible<'_> {
    /// Whether `name` names it: what the manifest calls it, its abbreviation, or an alias,
    /// ignoring case
    fn is(&self, name: &str) -> bool {
        let names = [self.locked.dependency_name(), self.abbreviation.as_str()];
        (names
            .into_iter()
            .chain(self.aliases.iter().map(String::as_str)))
        .any(|n| n.eq_ignore_ascii_case(name))
    }

    fn versification(&self) -> &str {
        &self.cached.package().info().versification
    }
}

/// The project's Bibles, each opened through the library's cache
fn bibles<'a>(
    project: &'a Project,
    library: &Library,
    reader: &dyn TextReader,
) -> Result<Vec<Bible<'a>>> {
    let found = project.bibles(library)?;
    if found.is_empty() {
        return Err(LogoxideError::Resolve(String::from(
            "this project has no Bibles (lx add, lx import)",
        )));
    }
    let cache = library.book_cache();
    let mut bibles = vec![];
    for (locked, package) in found {
        let (abbreviation, aliases) = project.names(locked, package.info());
        bibles.push(Bible {
            locked,
            cached: cache.open(package, reader)?,
            abbreviation,
            aliases,
        });
    }
    Ok(bibles)
}

/// The passages' text in a Bible, a blank line apart, each run's marks moved with it
fn text_of(bible: &Bible, passages: &[Passage], style: &Style) -> Result<Option<Text>> {
    let mut all: Option<Text> = None;
    for passage in passages {
        if !bible.cached.package().books().contains(&passage.book) {
            continue;
        }
        let book = bible.cached.book(passage.book)?;
        let name = (bible_data().books().get_name(passage.book).cloned()).unwrap_or_default();
        let Some(text) = logoxide_text::passage::text(&book, passage, style, &name, None) else {
            continue;
        };
        match &mut all {
            None => all = Some(text),
            Some(all) => {
                all.text.push_str("\n\n");
                let offset = all.text.len();
                all.text.push_str(&text.text);
                (all.marks).extend(
                    (text.marks.into_iter())
                        .map(|(range, kind)| (range.start + offset..range.end + offset, kind)),
                );
            }
        }
    }
    Ok(all)
}

/// A reference, and the Bible it names among the project's Bibles' names
fn parse(
    project: &Project,
    library: &Library,
    reader: &dyn TextReader,
    reference: &str,
) -> Result<(Passage, Option<String>)> {
    let translations = project.translations(library, reader)?;
    let found = (bible_data().parse_with_translation(reference, &translations))
        .ok_or_else(|| LogoxideError::Resolve(format!("`{reference}` isn't a Bible reference")))?;
    if let Some(translation) = found.translation {
        return Ok((found.passage, Some(translation.name)));
    }
    // A last word that isn't part of the passage names a Bible topos doesn't know by that name
    // (a package id like `ccel-kjv`, or one not in the project), which the caller looks for
    // among the Bibles rather than reading the first instead
    let trimmed = reference.trim_end();
    if let Some((before, last)) = trimmed.rsplit_once(char::is_whitespace) {
        let last = last.trim_matches(|c| c == '(' || c == ')');
        let without = bible_data().parse_with_translation(before, &translations);
        if last.chars().any(char::is_alphabetic)
            && without.is_some_and(|w| w.passage == found.passage)
        {
            return Ok((found.passage, Some(last.to_string())));
        }
    }
    Ok((found.passage, None))
}

/**
A passage from one of the project's Bibles: the one `reference` names after it (`John 3:16 KJV`,
`John 3:16 (kjv)`), else the first. A reference past the end of a chapter the Bible has
(`John 3:99 KJV`) is an error
*/
pub fn read(
    project: &Project,
    library: &Library,
    reader: &dyn TextReader,
    reference: &str,
    style: &Style,
) -> Result<Read> {
    let (passage, named) = parse(project, library, reader, reference)?;
    let mut bibles = bibles(project, library, reader)?;
    let index = match &named {
        Some(name) => (bibles.iter().position(|b| b.is(name))).ok_or_else(|| {
            LogoxideError::Resolve(format!("no Bible `{name}` in this project (lx list)"))
        })?,
        None => 0,
    };
    let bible = bibles.swap_remove(index);
    let text = text_of(&bible, std::slice::from_ref(&passage), style)?;
    Ok(Read {
        bible: bible.locked.dependency_name().to_string(),
        abbreviation: bible.abbreviation.clone(),
        reference: write(&passage),
        osis: osis(&passage),
        versification: bible.versification().to_string(),
        passage,
        text,
    })
}

/**
A passage in several of the project's Bibles (`bibles` by name; all of them when empty), in that
order. The passage is numbered by the Bible `reference` names, else the first of `bibles`; in
each Bible numbered otherwise it's mapped through the project's versification maps
([`Project::versifications`]), saying what of it that Bible doesn't have. A Bible numbered its own
way (`versification = "own"`) is read as written. A name that's none of the project's Bibles, or
two numberings no map joins, is an error
*/
pub fn compare(
    project: &Project,
    library: &Library,
    reader: &dyn TextReader,
    reference: &str,
    names: &[String],
    style: &Style,
) -> Result<Vec<Compared>> {
    let (passage, named) = parse(project, library, reader, reference)?;
    let all = bibles(project, library, reader)?;
    let mut chosen: Vec<&Bible> = vec![];
    for name in names {
        let bible = (all.iter().find(|b| b.is(name))).ok_or_else(|| {
            LogoxideError::Resolve(format!("no Bible `{name}` in this project (lx list)"))
        })?;
        chosen.push(bible);
    }
    if chosen.is_empty() {
        chosen = all.iter().collect();
    }
    let source = match &named {
        Some(name) => (all.iter().find(|b| b.is(name))).ok_or_else(|| {
            LogoxideError::Resolve(format!("no Bible `{name}` in this project (lx list)"))
        })?,
        None => chosen[0],
    };
    // The first Bible's verse counts, which a whole chapter needs to be mapped
    let counts: Option<ChapterVerses> = (source.cached.manifest().books.iter())
        .find(|(book, _)| *book == passage.book)
        .map(|(_, counts)| ChapterVerses::new(counts.clone()));
    let mut maps = None;
    let mut compared = vec![];
    for bible in chosen {
        let (from, to) = (source.versification(), bible.versification());
        let same = from.eq_ignore_ascii_case(to) || from == "own" || to == "own";
        let (passages, missing) = match same {
            true => (vec![passage.clone()], String::new()),
            false => {
                let maps = match &mut maps {
                    Some(maps) => maps,
                    None => maps.insert(project.versifications(library)?),
                };
                let mapped = maps.map(&passage, from, to, counts.as_ref())?;
                (mapped.passages(), mapped.missing_text())
            }
        };
        let text = text_of(bible, &passages, style)?;
        compared.push(Compared {
            bible: bible.locked.dependency_name().to_string(),
            abbreviation: bible.abbreviation.clone(),
            reference: (passages.iter().map(write)).collect::<Vec<_>>().join("; "),
            passages,
            text,
            missing,
            mapped: !same,
            versification: to.to_string(),
        });
    }
    Ok(compared)
}
