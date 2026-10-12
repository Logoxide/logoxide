//! Code actions: reformatting references, inserting a passage's text, and searching

use super::{template::TextFill, *};

impl Server {
    /**
    For the reference under the cursor:
    - Reformat it, if it isn't written the way completion would write it (and, when several
      references in the document are, reformat them all)
    - One search of the workspace per [`SearchMode`]
    */
    pub fn code_actions(&self, params: CodeActionParams) -> Option<Vec<CodeActionOrCommand>> {
        let uri = &params.text_document.uri;
        let (
            TranslatedMatch {
                reference: found,
                translation,
            },
            index,
        ) = self.reference_at(uri, params.range.start)?;
        let reference = self.reference(&found);
        let passage = self.parseable(&found.psg);
        let mut actions = self.reformat_actions(uri, &found, &index);
        actions.extend(self.insert_actions(uri, &found, translation.as_ref(), &index));
        let searches = SearchMode::ALL
            .into_iter()
            .map(|mode| {
                let title = mode.title(&reference);
                let arguments = SearchArguments {
                    mode,
                    passage: passage.clone(),
                };
                CodeActionOrCommand::CodeAction(CodeAction {
                    title: title.clone(),
                    command: Some(Command {
                        title,
                        command: SEARCH_COMMAND.into(),
                        arguments: Some(vec![serde_json::to_value(arguments).ok()?]),
                    }),
                    ..CodeAction::default()
                })
                .into()
            })
            .collect::<Option<Vec<CodeActionOrCommand>>>()?;
        actions.extend(searches);
        Some(actions)
    }

    /// `Reformat as "John 3:16"` for this reference, and for every reference in the document
    /// that is written differently, when there are several
    // `WorkspaceEdit::changes` is keyed by `Uri`, which clippy counts as a mutable key
    #[allow(clippy::mutable_key_type)]
    fn reformat_actions(
        &self,
        uri: &Uri,
        found: &BibleMatch,
        index: &LineIndex,
    ) -> Vec<CodeActionOrCommand> {
        let Some(text) = self.documents.get(uri) else {
            return vec![];
        };
        // Whole books are read-only
        if bible::is_book_uri(uri.as_str()) {
            return vec![];
        }
        // Each reference written differently from the configured format, with its rewrite
        let rewrite = |m: &BibleMatch| {
            let bytes = m.location.bytes;
            let formatted = self.reference(m);
            (text[bytes.start..bytes.end] != formatted).then(|| TextEdit {
                range: lsp_range(index, bytes.start, bytes.end),
                new_text: formatted,
            })
        };
        let action = |title: String, edits: Vec<TextEdit>| {
            edit_action(
                uri,
                title,
                edits,
                lsp_types::CodeActionKind::REFACTOR_REWRITE,
            )
        };
        let mut actions = vec![];
        if let Some(edit) = rewrite(found) {
            actions.push(action(
                format!("Reformat as \"{}\"", edit.new_text),
                vec![edit],
            ));
        }
        let all: Vec<TextEdit> = self
            .matcher
            .search(text)
            .iter()
            .filter_map(rewrite)
            .collect();
        if all.len() > 1 {
            let title = format!("Reformat all {} references in this file", all.len());
            actions.push(action(title, all));
        }
        actions
    }

    /**
    A code action for each template with an `action`: `Insert callout` (its text on the lines
    after the reference), `Replace line with callout`, or `Replace with callout` (the reference
    itself)
    - From the Bible the reference names; else one action per Bible, the default first
    - With several Bibles, titles name theirs: `Insert callout (KJV)`
    */
    fn insert_actions(
        &self,
        uri: &Uri,
        found: &BibleMatch,
        translation: Option<&Translation>,
        index: &LineIndex,
    ) -> Vec<CodeActionOrCommand> {
        let templates: Vec<(&str, &Template)> = (self.templates.iter())
            .filter(|(_, t)| t.action.is_some())
            .map(|(name, t)| (name.as_str(), t))
            .collect();
        (self
            .template_choices(uri, &templates, found, translation, index)
            .into_iter())
        .filter_map(|choice| {
            let kind = lsp_types::CodeActionKind::REFACTOR;
            Some(edit_action(uri, choice.title?, vec![choice.edit?], kind))
        })
        .collect()
    }

    /// [`TEMPLATES_REQUEST`]: each template filled for the reference at a position
    pub fn templates(&self, params: TextDocumentPositionParams) -> Vec<TemplateChoice> {
        let uri = &params.text_document.uri;
        let Some((found, index)) = self.reference_at(uri, params.position) else {
            return vec![];
        };
        let default = Template::default();
        let templates: Vec<(&str, &Template)> = match self.templates.is_empty() {
            true => vec![("text", &default)],
            false => (self.templates.iter())
                .map(|(name, t)| (name.as_str(), t))
                .collect(),
        };
        let translation = found.translation.as_ref();
        self.template_choices(uri, &templates, &found.reference, translation, &index)
    }

    /// Each template for each Bible (the one the reference names, else all, the default first),
    /// filled for a reference, with its code action's edit if it has an `action`
    fn template_choices(
        &self,
        uri: &Uri,
        templates: &[(&str, &Template)],
        found: &BibleMatch,
        translation: Option<&Translation>,
        index: &LineIndex,
    ) -> Vec<TemplateChoice> {
        let Some(text) = self.documents.get(uri) else {
            return vec![];
        };
        if bible::is_book_uri(uri.as_str()) {
            return vec![];
        }
        let bibles: Vec<&Bible> = match translation {
            Some(_) => self.bibles.named(translation).into_iter().collect(),
            None => self.bibles.iter().collect(),
        };
        // The reference, its line, and where the line ends
        let bytes = found.location.bytes;
        let line_start = text[..bytes.start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[bytes.end..]
            .find('\n')
            .map_or(text.len(), |i| bytes.end + i);
        // Labels name the Bible when there are several
        let several = self.bibles.len() > 1;
        (templates.iter())
            .flat_map(|&(name, template)| bibles.iter().map(move |bible| (name, template, *bible)))
            .filter_map(|(name, template, bible)| {
                let written = self.written(bible, template, found, translation)?;
                let label = match several {
                    true => format!("{name} ({})", bible.name),
                    false => name.to_string(),
                };
                let action = template.action.map(|action| {
                    let (range, new_text, title) = match action {
                        TemplateAction::Insert => {
                            (line_end..line_end, format!("\n{written}"), "Insert")
                        }
                        TemplateAction::ReplaceLine => {
                            (line_start..line_end, written.clone(), "Replace line with")
                        }
                        TemplateAction::Replace => {
                            (bytes.start..bytes.end, written.clone(), "Replace with")
                        }
                    };
                    let at = lsp_range(index, range.start, range.end);
                    (format!("{title} {label}"), TextEdit::new(at, new_text))
                });
                let (title, edit) = action.unzip();
                Some(TemplateChoice {
                    name: name.to_string(),
                    label,
                    bible: bible.name.clone(),
                    text: written,
                    title,
                    edit,
                })
            })
            .collect()
    }

    /// What a template writes for a reference, or [`None`] if the Bible doesn't have it
    fn written(
        &self,
        bible: &Bible,
        template: &Template,
        found: &BibleMatch,
        translation: Option<&Translation>,
    ) -> Option<String> {
        let markup = bible.markup.merged(&template.markup);
        let fill = TextFill {
            bible: Some(bible),
            markup: Some(&markup),
            line_prefix: &template.line_prefix,
            join: "\n",
        };
        self.fill_template(&template.display, found, translation, &fill)
    }
}

/**
A code action that makes edits in a document
- In unnamed documents, as the [`APPLY_EDITS_COMMAND`]: a workspace edit's URI (`file://`) would
  name no buffer
*/
// `WorkspaceEdit::changes` is keyed by `Uri`, which clippy counts as a mutable key
#[allow(clippy::mutable_key_type)]
fn edit_action(
    uri: &Uri,
    title: String,
    edits: Vec<TextEdit>,
    kind: lsp_types::CodeActionKind,
) -> CodeActionOrCommand {
    let (edit, command) = if is_unnamed(uri) {
        let arguments = serde_json::json!({ "edits": edits });
        let command = Command::new(
            title.clone(),
            APPLY_EDITS_COMMAND.into(),
            Some(vec![arguments]),
        );
        (None, Some(command))
    } else {
        let changes = HashMap::from([(uri.clone(), edits)]);
        (Some(lsp_types::WorkspaceEdit::new(changes)), None)
    };
    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(kind),
        edit,
        command,
        ..CodeAction::default()
    })
}
