//! Whole books: their text, symbols, semantic tokens, and going to a reference's verses; and
//! symbols in other documents

use super::*;

impl Server {
    /// Every reference in the document, for outlines and pickers
    pub fn document_symbols(&self, params: DocumentSymbolParams) -> Option<DocumentSymbolResponse> {
        if let Some((bible, book)) = self.book_document(&params.text_document.uri) {
            return book_symbols(bible, book);
        }
        let text = self.documents.get(&params.text_document.uri)?;
        let index = LineIndex::new(text);
        let symbols = self
            .matcher
            .search(text)
            .into_iter()
            .map(|m| {
                let bytes = m.location.bytes;
                let range = lsp_range(&index, bytes.start, bytes.end);
                #[allow(deprecated)]
                DocumentSymbol {
                    name: self.reference(&m),
                    detail: m.psg.to_osis(self.matcher.data().books()),
                    kind: SymbolKind::KEY,
                    tags: None,
                    deprecated: None,
                    range,
                    selection_range: range,
                    children: None,
                }
            })
            .collect();
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    /// Go to definition (and declaration): the reference's text in its whole book (one location
    /// per part of the reference), in the Bible it names or the default
    pub fn definition(&self, params: GotoDefinitionParams) -> Option<GotoDefinitionResponse> {
        let position = params.text_document_position_params;
        let (
            TranslatedMatch {
                reference: found,
                translation,
            },
            _,
        ) = self.reference_at(&position.text_document.uri, position.position)?;
        let bible = self.bibles.named(translation.as_ref())?;
        let rendered = bible.render(found.psg.book)?;
        let uri = Uri::from_str(&bible.uri(found.psg.book)?).ok()?;
        let index = LineIndex::new(&rendered.text);
        let mut locations: Vec<Location> = rendered
            .passage_ranges(&found.psg)
            .into_iter()
            .map(|r| Location::new(uri.clone(), lsp_range(&index, r.start, r.end)))
            .collect();
        match locations.len() {
            0 => None,
            1 => locations.pop().map(GotoDefinitionResponse::Scalar),
            _ => Some(GotoDefinitionResponse::Array(locations)),
        }
    }

    /// [`BOOK_REQUEST`]: a whole book's text
    pub fn book(&self, params: BookParams) -> Result<BookText, String> {
        if self.bibles.is_empty() {
            return Err("no Bible is set (the `bible` or `bibles` setting)".into());
        }
        let (bible, book) = self.bibles.book_of(&params.uri).ok_or_else(|| {
            let names: Vec<&str> = self.bibles.iter().map(|b| b.name.as_str()).collect();
            format!("{} is not a book of {}", params.uri, names.join(", "))
        })?;
        let rendered = bible
            .render(book)
            .ok_or_else(|| format!("{} can't be read", params.uri))?;
        Ok(BookText {
            title: bible.book_name(book).unwrap_or_default().to_string(),
            text: rendered.text.clone(),
        })
    }

    /// [`FILE_TEXT_REQUEST`]: a book's text (an EPUB or a PDF), where workspace searches with
    /// the `epub` and `pdf` settings put its references
    pub fn file_text(&self, params: BookParams) -> Result<BookText, String> {
        let path = workspace::book_path(&params.uri)
            .ok_or_else(|| format!("{} isn't a {} URI", params.uri, workspace::BOOK_SCHEME))?;
        Ok(BookText {
            title: (path.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            text: workspace::book_text(&path)?,
        })
    }

    /// [`SAME_VERSE_REQUEST`]: where the verse at a position in a whole book is in another
    /// Bible's book (its verse with that number, else its chapter, else the next chapter it has,
    /// else the book's start)
    pub fn same_verse(&self, params: SameVerseParams) -> Result<Option<Location>, String> {
        let (from, book) = self
            .bibles
            .book_of(&params.uri)
            .ok_or_else(|| format!("{} is not a whole book", params.uri))?;
        let target = self
            .bibles
            .get(&params.bible)
            .ok_or_else(|| format!("no Bible is named `{}`", params.bible))?;
        let (Some(rendered), Some(other)) = (from.render(book), target.render(book)) else {
            return Ok(None);
        };
        let index = LineIndex::new(&rendered.text);
        let position = params.position;
        let cursor = index
            .offset_of_utf16(position.line as usize, position.character as usize)
            .unwrap_or(rendered.text.len());
        let (chapter, verse) = match rendered.part_at(cursor) {
            Some(Part::Verse { chapter, verse, .. }) => (chapter, Some(verse)),
            Some(Part::Chapter(chapter)) => (chapter, None),
            None => (0, None),
        };
        let chapter_of = |part: &Part| match *part {
            Part::Verse { chapter, .. } | Part::Chapter(chapter) => chapter,
        };
        let parts = &other.parts;
        let found = verse
            .and_then(|verse| {
                parts.iter().find(|(_, part)| {
                    matches!(*part, Part::Verse { chapter: c, verse: v, last }
                        if c == chapter && v <= verse && verse <= last)
                })
            })
            .or_else(|| (parts.iter()).find(|(_, part)| *part == Part::Chapter(chapter)))
            .or_else(|| (parts.iter()).find(|(_, part)| chapter_of(part) >= chapter))
            .map_or(0, |(range, _)| range.start);
        let uri = Uri::from_str(&target.uri(book).ok_or("the book has no USFM code")?)
            .map_err(|e| e.to_string())?;
        let index = LineIndex::new(&other.text);
        Ok(Some(Location::new(uri, lsp_range(&index, found, found))))
    }

    /// [`BIBLES_REQUEST`]: the Bibles, the default first
    pub fn bibles(&self) -> BiblesList {
        let bibles = self
            .bibles
            .iter()
            .map(|bible| BibleInfo {
                name: bible.name.clone(),
                aliases: bible.aliases.clone(),
                books: bible.book_codes(),
            })
            .collect();
        BiblesList { bibles }
    }

    /// Whole books' verse numbers, chapter labels, words of Jesus, headings, and notes (split at
    /// line breaks); other documents have none
    pub fn semantic_tokens(&self, params: SemanticTokensParams) -> SemanticTokens {
        let Some(rendered) = self
            .book_document(&params.text_document.uri)
            .and_then(|(bible, book)| bible.render(book))
        else {
            return SemanticTokens::default();
        };
        let text = &rendered.text;
        let index = LineIndex::new(text);
        let mut data = vec![];
        let (mut line, mut character) = (0, 0);
        for (range, token) in &bible::tokens(&rendered.marks) {
            let mut start = range.start;
            for piece in text[range.clone()].split('\n') {
                let end = start + piece.len();
                if !piece.trim().is_empty() {
                    let position = lsp_position(&index, start);
                    let delta_line = position.line - line;
                    let delta_start = match delta_line {
                        0 => position.character - character,
                        _ => position.character,
                    };
                    data.push(SemanticToken {
                        delta_line,
                        delta_start,
                        length: piece.encode_utf16().count() as u32,
                        token_type: *token as u32,
                        token_modifiers_bitset: 0,
                    });
                    (line, character) = (position.line, position.character);
                }
                start = end + 1;
            }
        }
        SemanticTokens {
            result_id: None,
            data,
        }
    }
}

/**
A whole book's chapters (`John 3`), each with its verses (`John 3:16`, or `John 3:16-17` for a
bridge), for an outline and for moving between them
- A chapter runs from its label (or its first verse, if labels are hidden) to its last verse's
  end; its selection is the label (or the first verse)
*/
fn book_symbols(bible: &Bible, book: BookId) -> Option<DocumentSymbolResponse> {
    let rendered = bible.render(book)?;
    let index = LineIndex::new(&rendered.text);
    let name = bible.book_name(book).unwrap_or_default();
    #[allow(deprecated)]
    let symbol = |name: String, kind, range| DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: None,
    };
    let mut chapters: Vec<(u8, DocumentSymbol)> = vec![];
    for (range, part) in &rendered.parts {
        let (chapter, verse) = match *part {
            Part::Chapter(chapter) => (chapter, None),
            Part::Verse {
                chapter,
                verse,
                last,
            } => (chapter, Some((verse, last))),
        };
        if chapter == 0 {
            continue;
        }
        let range = lsp_range(&index, range.start, range.end);
        if chapters.last().is_none_or(|(c, _)| *c != chapter) {
            let label = symbol(format!("{name} {chapter}"), SymbolKind::NAMESPACE, range);
            chapters.push((chapter, label));
        }
        let (_, current) = chapters.last_mut()?;
        current.range.end = range.end;
        if let Some((verse, last)) = verse {
            let numbers = match last > verse {
                true => format!("{verse}-{last}"),
                false => verse.to_string(),
            };
            let verse = symbol(
                format!("{name} {chapter}:{numbers}"),
                SymbolKind::KEY,
                range,
            );
            current.children.get_or_insert_default().push(verse);
        }
    }
    let chapters = chapters.into_iter().map(|(_, symbol)| symbol).collect();
    Some(DocumentSymbolResponse::Nested(chapters))
}
