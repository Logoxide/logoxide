//! Completing references as they're typed, and the passage's text for the one chosen

use super::*;

impl Server {
    pub fn completion(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let position = params.text_document_position;
        let text = self.documents.get(&position.text_document.uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(
            position.position.line as usize,
            position.position.character as usize,
        )?;
        let options = CompleteOptions {
            format: self.format.clone(),
            limit: Some(100),
        };
        let translations = &self.bibles.translations;
        let items = self
            .matcher
            .complete_with_translations(text, cursor, &options, translations)
            .into_iter()
            .enumerate()
            .map(|(idx, completion)| {
                let range = completion.edit.range;
                CompletionItem {
                    label: completion.label.clone(),
                    kind: Some(match completion.kind {
                        CompletionKind::Book => CompletionItemKind::FOLDER,
                        CompletionKind::Chapter => CompletionItemKind::MODULE,
                        CompletionKind::Verse => CompletionItemKind::REFERENCE,
                        CompletionKind::Translation => CompletionItemKind::ENUM_MEMBER,
                    }),
                    /*
                    What was typed, then the label (`jn 3:1 John 3:16`):
                    - Editors that match from the start (Neovim's own completion) see what was
                      typed, so `jn 3:1` still shows `John 3:16`
                    - Editors that match the word before the cursor (blink.cmp) find it in the
                      label, even against a list made a keystroke ago: with only what was
                      typed, `1 C`'s list filtered `Co` to nothing, so `1 Co` showed none
                    */
                    filter_text: Some(format!("{} {}", &text[range.clone()], completion.label)),
                    // Keep the order: chapters and verses are in reading order
                    sort_text: Some(format!("{idx:05}")),
                    // For `completionItem/resolve` to add the passage's text (a translation's
                    // in that Bible)
                    data: completion
                        .passage
                        .as_ref()
                        .filter(|_| !self.bibles.is_empty())
                        .map(|p| match completion.kind {
                            CompletionKind::Translation => serde_json::json!({
                                "passage": self.parseable(p),
                                "bible": completion.label,
                            }),
                            _ => serde_json::json!({ "passage": self.parseable(p) }),
                        }),
                    text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit {
                        range: lsp_range(&index, range.start, range.end),
                        new_text: completion.edit.text,
                    })),
                    ..CompletionItem::default()
                }
            })
            .collect();
        // Incomplete: the editor asks again as you type, instead of filtering this list itself
        // (which filtered `1 Ti` against what was typed when it was made, `1 T`, and emptied it)
        Some(CompletionResponse::List(lsp_types::CompletionList {
            is_incomplete: true,
            items,
        }))
    }

    /// A completion with its passage's text as documentation (with a `bible`), and what to color
    /// in it as `data.tokens` (see [`bible::Text::fence`])
    pub fn resolve_completion(&self, mut item: CompletionItem) -> CompletionItem {
        let data = item.data.as_ref();
        let bible = data
            .and_then(|data| data.get("bible")?.as_str())
            .and_then(|name| self.bibles.get(name))
            .or_else(|| self.bibles.default_bible());
        let text = data
            .and_then(|data| data.get("passage")?.as_str())
            .and_then(|passage| self.matcher.data().parse(passage))
            .and_then(|passage| bible?.passage(&passage, None, Some(MAX_HOVER_TEXT)));
        if let Some(text) = text {
            // What to color, for editors that draw the documentation themselves
            if let Some(Value::Object(data)) = &mut item.data {
                data.insert("tokens".into(), Value::String(text.token_info()));
            }
            item.documentation = Some(lsp_types::Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text.fence(false),
            }));
        }
        item
    }
}
