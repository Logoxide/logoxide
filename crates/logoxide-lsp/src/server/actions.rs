//! Code actions: reformatting references, inserting a passage's text, and searching

use super::*;

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
    `Insert Callout` for each insertion: the passage's text on the lines after the reference, or
    in place of its line or of the reference itself (`replace`)
    - From the Bible the reference names; else one action per Bible, the default first
    - With several Bibles, titles name theirs: `Insert Callout (KJV)`
    */
    fn insert_actions(
        &self,
        uri: &Uri,
        found: &BibleMatch,
        translation: Option<&Translation>,
        index: &LineIndex,
    ) -> Vec<CodeActionOrCommand> {
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
        // Titles name the Bible when there are several
        let several = self.bibles.len() > 1;
        self.insertions
            .iter()
            .flat_map(|insertion| bibles.iter().map(move |bible| (insertion, *bible)))
            .filter_map(|(insertion, bible)| {
                let inserted = self.insertion_text(bible, insertion, found)?;
                let (range, new_text) = match insertion.replace {
                    None => (line_end..line_end, format!("\n{inserted}")),
                    Some(Replace::Line) => (line_start..line_end, inserted),
                    Some(Replace::Inline) => (bytes.start..bytes.end, inserted),
                };
                let at = lsp_range(index, range.start, range.end);
                let title = match several {
                    true => format!("Insert {} ({})", insertion.name, bible.name),
                    false => format!("Insert {}", insertion.name),
                };
                let edits = vec![TextEdit::new(at, new_text)];
                Some(edit_action(
                    uri,
                    title,
                    edits,
                    lsp_types::CodeActionKind::REFACTOR,
                ))
            })
            .collect()
    }

    /// What an insertion writes for a reference, or [`None`] if the Bible doesn't have it
    fn insertion_text(
        &self,
        bible: &Bible,
        insertion: &Insertion,
        found: &BibleMatch,
    ) -> Option<String> {
        let markup = bible.markup.merged(&insertion.markup);
        let text = bible.passage(&found.psg, Some(&markup), None)?.text;
        let text: Vec<String> = text
            .lines()
            .map(|line| {
                format!("{}{line}", insertion.line_prefix)
                    .trim_end()
                    .to_string()
            })
            .collect();
        let osis = found
            .psg
            .to_osis(self.matcher.data().books())
            .unwrap_or_default();
        Some(
            insertion
                .display
                .replace("{reference}", &self.reference(found))
                .replace("{osis}", &osis)
                .replace("{bible}", &bible.name)
                .replace("{text}", &text.join("\n")),
        )
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
