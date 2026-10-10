//! Hovering a reference (or a missing one, or a verse in a whole book)

use super::*;

impl Server {
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
            return self.verse_hover(uri, position.position);
        };
        let bytes = found.location.bytes;
        let written = self
            .documents
            .get(uri)
            .and_then(|text| text.get(bytes.start..bytes.end))
            .unwrap_or_default();
        let title = self.reference(&found);
        let mut value = match &translation {
            Some(translation) => format!("**{title}** ({})", translation.name),
            None => format!("**{title}**"),
        };
        // Lists of lines, and the passage's text between them
        let mut sections: Vec<String> = vec![];
        let mut lines: Vec<String> = vec![];
        for &field in &self.hover {
            if field == HoverField::Text {
                let bibles = self.hover_texts(translation.as_ref());
                // Each Bible's name labels its text when there are several
                let labeled = self.bibles.len() > 1;
                for bible in bibles {
                    let Some(text) = bible.passage(&found.psg, None, Some(MAX_HOVER_TEXT)) else {
                        continue;
                    };
                    if !lines.is_empty() {
                        sections.push(lines.join("\n"));
                        lines.clear();
                    }
                    sections.push(match labeled {
                        true => format!("*{}*\n{}", bible.name, text.fence(true)),
                        false => text.fence(true),
                    });
                }
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
            value.push_str("\n\n");
            value.push_str(&section);
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
            HoverField::Text => return None,
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
    Hovering a verse in a whole book (not on a reference): how many references name it, and in
    how many files, or `???` if searching takes longer than [`HOVER_SEARCH_TIME`] (files read
    until then are cached, so the next hover gets further)
    */
    fn verse_hover(&self, uri: &Uri, position: Position) -> Option<Hover> {
        let (mode, passage) = self.verse_at(uri, position)?;
        let stop = workspace::Stop::at(Instant::now() + HOVER_SEARCH_TIME);
        let count = match self.search_until(mode, &passage, &stop) {
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
        };
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{passage}**\n\n{count}"),
            }),
            range: None,
        })
    }
}
