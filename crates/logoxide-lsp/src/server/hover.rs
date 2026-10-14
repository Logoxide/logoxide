//! Hovering a reference (or a missing one, a Strong's number, or a verse in a whole book)

use logoxide_text::Note;

use super::*;

/// The most notes hover shows from one commentary (a chapter's verse-by-verse notes would fill
/// the screen); the rest are counted
const MAX_HOVER_NOTES: usize = 8;

/// A note's verses as a reference: `John 3:16`, `John 3:16-18`, `John 3:36-4:2`, `John 3:20a`
fn note_reference(book: &str, note: &Note) -> String {
    let part = match note.part {
        0 => String::new(),
        n => char::from(b'a' + (n - 1).min(25)).to_string(),
    };
    let first = format!("{book} {}:{}{part}", note.chapter, note.verse);
    if note.last_chapter != note.chapter {
        format!("{first}-{}:{}", note.last_chapter, note.last)
    } else if note.last != note.verse {
        format!("{first}-{}", note.last)
    } else {
        first
    }
}

impl Server {
    /// A text as hover (and completion documentation) shows it: without its leading whitespace
    /// with `hover-trim`
    pub(super) fn hover_shown(&self, text: bible::Text) -> bible::Text {
        match self.hover_trim {
            true => text.trim_start(),
            false => text,
        }
    }

    /// A `{translation}`'s Bible for a text from `bible`: its name when the reference names it, or
    /// when it isn't the default Bible (a template's from another Bible); else none
    pub(super) fn translation_of<'a>(
        &self,
        bible: &'a Bible,
        translation: Option<&Translation>,
    ) -> Option<&'a str> {
        let default = self.bibles.default_bible().map(|b| b.name.as_str());
        (translation.is_some() || default != Some(bible.name.as_str()))
            .then_some(bible.name.as_str())
    }

    /// The Bibles whose text hover shows for a reference: the one it names (or the default), then
    /// the `hover-bibles` setting's
    fn hover_texts(&self, translation: Option<&Translation>) -> Vec<&Bible> {
        let mut shown: Vec<&Bible> = self.bibles.named(translation).into_iter().collect();
        for bible in self
            .hover_bibles
            .iter()
            .filter_map(|name| self.bibles.get(name))
        {
            if !shown.iter().any(|b| b.name == bible.name) {
                shown.push(bible);
            }
        }
        shown
    }

    /// The reference under the cursor, normalized, with what the `hover` setting lists about it
    /// (its OSIS id, book, genres, ...); or, on a reference that doesn't exist, what does
    /// (`John 3 has 36 verses`)
    pub fn hover(&self, params: HoverParams) -> Option<Hover> {
        let position = params.text_document_position_params;
        let uri = &position.text_document.uri;
        if let Some(hover) = self.problem_hover(uri, position.position) {
            return Some(hover);
        }
        let Some((
            TranslatedMatch {
                reference: found,
                translation,
            },
            index,
        )) = self.reference_at(uri, position.position)
        else {
            return (self.strongs_hover(uri, position.position))
                .or_else(|| self.verse_hover(uri, position.position));
        };
        let bytes = found.location.bytes;
        let written = self
            .documents
            .get(uri)
            .and_then(|text| text.get(bytes.start..bytes.end))
            .unwrap_or_default();
        let title = self.reference(&found);
        let named = self.bibles.named(translation.as_ref());
        let named_name = named.and_then(|bible| self.translation_of(bible, translation.as_ref()));
        // The first line (`hover-title`), if any
        let mut value = settings::fill_translation(&self.hover_title, named_name)
            .replace("{reference}", &title)
            .replace("{bible}", named.map_or("", |bible| bible.name.as_str()));
        // Lists of lines, and the passage's text between them
        let mut sections: Vec<String> = vec![];
        let mut lines: Vec<String> = vec![];
        for &field in &self.hover {
            if field == HoverField::Translation {
                if let Some(name) = named_name {
                    lines.push(format!("- **Translation**: {name}"));
                }
                continue;
            }
            if field == HoverField::Text {
                let bibles = self.hover_texts(translation.as_ref());
                // Each Bible's name labels its text when several Bibles' texts are shown
                let shown: Vec<(String, String, bool)> = (bibles.into_iter())
                    .filter_map(|bible| self.bible_text(&found.psg, named, bible))
                    .collect();
                if !shown.is_empty() && !lines.is_empty() {
                    sections.push(lines.join("\n"));
                    lines.clear();
                }
                let several = shown.len() > 1;
                for (label, text, mapped) in shown {
                    sections.push(match several || mapped {
                        true => format!("{label}\n{text}"),
                        false => text,
                    });
                }
                // Then the commentaries' notes on it
                let notes = self.commentary_sections(&found.psg);
                if !notes.is_empty() && !lines.is_empty() {
                    sections.push(lines.join("\n"));
                    lines.clear();
                }
                sections.extend(notes);
                continue;
            }
            let Some((label, detail)) = self.hover_line(field, &found, written) else {
                continue;
            };
            // The full name or abbreviation is left out when it is the title already
            let repeats =
                matches!(field, HoverField::Name | HoverField::Abbreviation) && detail == title;
            if !repeats {
                lines.push(format!("- **{label}**: {detail}"));
            }
        }
        if !lines.is_empty() {
            sections.push(lines.join("\n"));
        }
        for section in sections {
            if !value.is_empty() {
                value.push_str("\n\n");
            }
            value.push_str(&section);
        }
        if value.is_empty() {
            return None;
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(lsp_range(&index, bytes.start, bytes.end)),
        })
    }

    /**
    A passage's text in `bible` as hover shows it, fenced, with its label (`*KJV*`) and whether
    it was mapped: a Bible numbered differently from `from` reads the passage as it numbers it,
    when the project's maps say how, and its label says where (`*LXX* (Psalm 9:22)`); none when
    `bible` has no text for it
    */
    fn bible_text(
        &self,
        passage: &Passage,
        from: Option<&Bible>,
        bible: &Bible,
    ) -> Option<(String, String, bool)> {
        let mapped = from
            .filter(|from| from.name != bible.name)
            .and_then(|from| self.bibles.mapped(passage, from, bible));
        let (passages, label) = match &mapped {
            Some(mapped) => {
                let mut label = format!("*{}* ({})", bible.name, mapped.text());
                if !mapped.missing.is_empty() {
                    label.push_str(&format!(" (not in it: {})", mapped.missing_text()));
                }
                (mapped.passages(), label)
            }
            None => (vec![passage.clone()], format!("*{}*", bible.name)),
        };
        let texts: Vec<String> = (passages.iter())
            .filter_map(|p| bible.passage(p, None, self.hover_limit))
            .map(|text| self.hover_shown(text).fence(true))
            .collect();
        (!texts.is_empty()).then(|| (label, texts.join("\n\n"), mapped.is_some()))
    }

    /**
    Each commentary's notes on a passage (the `commentaries` setting), a section a note,
    labeled with the commentary's name and the verses the note is on: `*MHCC* (John 3:16-18)`;
    after [`MAX_HOVER_NOTES`] of one commentary's, a line counting the rest
    */
    fn commentary_sections(&self, passage: &Passage) -> Vec<String> {
        let book = (self.matcher.data().books().get_name(passage.book))
            .cloned()
            .unwrap_or_default();
        let mut sections = vec![];
        for commentary in &self.commentaries {
            let notes: Vec<(Note, bible::Text)> = (commentary
                .notes(passage, self.stretch_notes)
                .into_iter())
            .filter_map(|note| {
                let text = bible::nodes_text(&note.nodes, &commentary.markup, self.hover_limit)?;
                Some((note, self.hover_shown(text)))
            })
            .collect();
            for (note, text) in notes.iter().take(MAX_HOVER_NOTES) {
                sections.push(format!(
                    "*{}* ({})\n{}",
                    commentary.name,
                    note_reference(&book, note),
                    text.fence(true)
                ));
            }
            if let Some(more) = notes.len().checked_sub(MAX_HOVER_NOTES).filter(|&n| n > 0) {
                let plural = if more == 1 { "" } else { "s" };
                sections.push(format!("*{}*: {more} more note{plural}", commentary.name));
            }
        }
        sections
    }

    /**
    Hovering a Strong's number (`G25`, `H430`, `G0025`) with the `strongs` setting: its entry in
    each of the project's Strong's dictionaries that has it, labeled with the dictionary's name
    when there are several, under the number and its headword: `**G25** ἀγαπάω`
    */
    fn strongs_hover(&self, uri: &Uri, position: Position) -> Option<Hover> {
        if self.dictionaries.is_empty() {
            return None;
        }
        let text = self.documents.get(uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(position.line as usize, position.character as usize)?;
        let (key, bytes) = dictionary::strongs_at(text, cursor)?;
        let entries: Vec<(&Dictionary, logoxide_text::Entry, bible::Text)> = (self.dictionaries)
            .iter()
            .filter(|d| d.covers(&key))
            .filter_map(|d| {
                let (entry, text) = d.entry(&key, self.hover_limit)?;
                Some((d, entry, self.hover_shown(text)))
            })
            .collect();
        let (_, first, _) = entries.first()?;
        let mut value = match &first.headword {
            Some(headword) => format!("**{key}** {headword}"),
            None => format!("**{key}**"),
        };
        let labeled = self.dictionaries.len() > 1;
        for (dictionary, _, text) in &entries {
            value.push_str("\n\n");
            if labeled {
                value.push_str(&format!("*{}*\n", dictionary.name));
            }
            value.push_str(&text.fence(true));
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(lsp_range(&index, bytes.start, bytes.end)),
        })
    }

    /// One hover line's label and text, or [`None`] when there is nothing to say
    fn hover_line(
        &self,
        field: HoverField,
        found: &BibleMatch,
        written: &str,
    ) -> Option<(&'static str, String)> {
        let data = self.matcher.data();
        let psg = &found.psg;
        let styled = |book| {
            FormatOptions {
                book,
                ..self.format.clone()
            }
            .passage(psg, data)
        };
        let versification = data.chapter_verses().get_chapter_verses(&psg.book);
        Some(match field {
            HoverField::Text | HoverField::Translation => return None,
            HoverField::Written => ("Written", format!("`{written}`")),
            HoverField::Name => ("Name", styled(BookStyle::Name)?),
            HoverField::Abbreviation => ("Abbreviation", styled(BookStyle::Abbreviation)?),
            HoverField::Osis => ("OSIS", format!("`{}`", psg.to_osis(data.books())?)),
            HoverField::Usfm => ("USFM", format!("`{}`", psg.to_usfm(data.books())?)),
            HoverField::Book => {
                let name = data.books().get_name(psg.book)?;
                let books = data.books().ids().count();
                let mut detail = format!("{name}, book {} of {books}", psg.book.0);
                if let Some(chapters) = versification.map(|v| v.get_chapter_count()) {
                    let plural = if chapters == 1 { "" } else { "s" };
                    detail.push_str(&format!(", {chapters} chapter{plural}"));
                }
                ("Book", detail)
            }
            HoverField::Testament => {
                let testament = if TestamentFilter::Old.contains(psg.book) {
                    "Old Testament"
                } else if TestamentFilter::New.contains(psg.book) {
                    "New Testament"
                } else {
                    return None;
                };
                ("Testament", testament.to_string())
            }
            HoverField::Genres => {
                let genres: Vec<&str> = data
                    .genres()
                    .iter()
                    .filter(|genre| genre.books().contains(&psg.book))
                    .map(|genre| genre.name())
                    .collect();
                let label = if genres.len() == 1 { "Genre" } else { "Genres" };
                (label, (!genres.is_empty()).then(|| genres.join(", "))?)
            }
            HoverField::Verses => {
                let mut verses = psg.verses(versification);
                verses.sort();
                verses.dedup();
                let ranges: Vec<String> = psg
                    .ranges(versification)
                    .iter()
                    .map(|range| {
                        let (start, end) = (range.start, range.end);
                        let cv = &self.format.chapter_verse;
                        let to = &self.format.range;
                        if end.verse == VerseRange::END_OF_CHAPTER {
                            start.chapter.to_string()
                        } else if start == end {
                            format!("{}{cv}{}", start.chapter, start.verse)
                        } else if start.chapter == end.chapter {
                            format!("{}{cv}{}{to}{}", start.chapter, start.verse, end.verse)
                        } else {
                            format!(
                                "{}{cv}{}{to}{}{cv}{}",
                                start.chapter, start.verse, end.chapter, end.verse
                            )
                        }
                    })
                    .collect();
                let count = if verses.is_empty() {
                    String::from("?")
                } else {
                    verses.len().to_string()
                };
                (
                    "Verses",
                    format!("{count} ({})", ranges.join(&self.format.chapter_separator)),
                )
            }
            HoverField::Bcv => {
                let keys: Vec<String> = psg
                    .bcv_ranges()
                    .into_iter()
                    .map(|range| match range.start() == range.end() {
                        true => format!("`{}`", range.start()),
                        false => format!("`{}-{}`", range.start(), range.end()),
                    })
                    .collect();
                ("BCV", keys.join(", "))
            }
            HoverField::Location => {
                let (start, end) = (found.location.start, found.location.end);
                // Columns are 1-based characters; the end is the last character's
                let last = end.char_column.saturating_sub(1).max(1);
                let detail = if start.line == end.line {
                    format!("line {}, columns {}-{last}", start.line, start.char_column)
                } else {
                    format!(
                        "line {}, column {} to line {}, column {last}",
                        start.line, start.char_column, end.line
                    )
                };
                ("Location", detail)
            }
        })
    }

    fn problem_hover(&self, uri: &Uri, position: Position) -> Option<Hover> {
        let text = self.documents.get(uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(position.line as usize, position.character as usize)?;
        let problem = self
            .matcher
            .problems_with_translations(text, &self.bibles.translations)
            .into_iter()
            .find(|p| p.bytes.start <= cursor && cursor <= p.bytes.end)?;
        let value = match &problem.detail {
            Some(detail) => format!("**{}**\n\n{detail}", problem.message),
            None => format!("**{}**", problem.message),
        };
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(lsp_range(&index, problem.bytes.start, problem.bytes.end)),
        })
    }

    /**
    Hovering a verse in a whole book (not on a reference): what `book-hover` lists, under the
    verse in bold
    - `references`: how many references name it, and in how many files, or `???` if searching
      takes longer than [`HOVER_SEARCH_TIME`] (files read until then are cached, so the next
      hover gets further)
    - `translations`: the verse in the other Bibles (`hover-bibles`', else every other), each
      labeled; a chapter label has none
    - `current`: the verse in the book's own Bible, labeled the same way
    */
    fn verse_hover(&self, uri: &Uri, position: Position) -> Option<Hover> {
        if self.book_hover.is_empty() {
            return None;
        }
        let (mode, passage) = self.verse_at(uri, position)?;
        let mut value = format!("**{passage}**");
        for field in &self.book_hover {
            let section = match field {
                BookHoverField::References => self.reference_count(mode, &passage),
                BookHoverField::Translations | BookHoverField::Current
                    if mode == SearchMode::ExplicitOverlap =>
                {
                    let texts = self.verse_texts(uri, &passage, *field == BookHoverField::Current);
                    if texts.is_empty() {
                        continue;
                    }
                    texts.join("\n\n")
                }
                BookHoverField::Translations | BookHoverField::Current => continue,
            };
            value.push_str("\n\n");
            value.push_str(&section);
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        })
    }

    /// How many references name `passage`, and in how many files: `4 references in 3 files`
    fn reference_count(&self, mode: SearchMode, passage: &str) -> String {
        let stop = workspace::Stop::at(Instant::now() + HOVER_SEARCH_TIME);
        match self.search_until(mode, passage, &stop) {
            Ok((locations, true)) => {
                let files: HashSet<&str> = locations.iter().map(|l| l.uri.as_str()).collect();
                let plural = |n: usize| if n == 1 { "" } else { "s" };
                format!(
                    "{} reference{} in {} file{}",
                    locations.len(),
                    plural(locations.len()),
                    files.len(),
                    plural(files.len())
                )
            }
            _ => String::from("??? references (searching took too long)"),
        }
    }

    /// `passage` (a verse of the whole book `uri`) in the book's own Bible (`current`), else in
    /// the other Bibles, `hover-bibles`' (else every other); each labeled with its name
    fn verse_texts(&self, uri: &Uri, passage: &str, current: bool) -> Vec<String> {
        let Some((own, _)) = self.book_document(uri) else {
            return vec![];
        };
        let Some(psg) = self.matcher.data().parse(passage) else {
            return vec![];
        };
        let others: Vec<&Bible> = match self.hover_bibles.is_empty() {
            _ if current => vec![own],
            true => self.bibles.iter().collect(),
            false => (self.hover_bibles.iter())
                .filter_map(|name| self.bibles.get(name))
                .collect(),
        };
        (others.into_iter())
            .filter(|bible| current || bible.name != own.name)
            .filter_map(|bible| self.bible_text(&psg, Some(own), bible))
            .map(|(label, text, _)| format!("{label}\n{text}"))
            .collect()
    }
}
