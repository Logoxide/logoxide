//! Diagnostics (references that don't exist, and how each is written, or a template) and inlay
//! hints

use super::{template::TextFill, *};

impl Server {
    /**
    The document's diagnostics:
    - Warnings for references that do not exist, like `John 3:99` (code `missing`)
    - An information diagnostic on each reference: how it is written in the configured format,
      like `John 3:16` (code `reference`; a hint, or none, with `reference-diagnostics`), or its
      `reference-diagnostics-template` (the reference when the Bible lacks its text)
    */
    pub fn diagnostics(&self, uri: &Uri) -> PublishDiagnosticsParams {
        // Whole books are Bible text, not notes
        let text = (!bible::is_book_uri(uri.as_str()))
            .then(|| self.documents.get(uri))
            .flatten();
        let diagnostics = text.map_or_else(Vec::new, |text| {
            let index = LineIndex::new(text);
            let diagnostic = |range, severity, code: &str, message| Diagnostic {
                range,
                severity: Some(severity),
                code: Some(lsp_types::NumberOrString::String(code.into())),
                source: Some(String::from("logoxide")),
                message,
                ..Diagnostic::default()
            };
            let mut diagnostics: Vec<Diagnostic> = self
                .matcher
                .problems_with_translations(text, &self.bibles.translations)
                .into_iter()
                .map(|problem| {
                    let range = lsp_range(&index, problem.bytes.start, problem.bytes.end);
                    diagnostic(
                        range,
                        DiagnosticSeverity::WARNING,
                        "missing",
                        problem.message,
                    )
                })
                .collect();
            let severity = match self.reference_diagnostics {
                ReferenceDiagnostics::Info => Some(DiagnosticSeverity::INFORMATION),
                ReferenceDiagnostics::Hint => Some(DiagnosticSeverity::HINT),
                ReferenceDiagnostics::Never => None,
            };
            if let Some(severity) = severity {
                let found =
                    (self.matcher).search_with_translations(text, &self.bibles.translations);
                for TranslatedMatch {
                    reference: m,
                    translation,
                } in found
                {
                    let bytes = m.location.bytes;
                    let range = lsp_range(&index, bytes.start, bytes.end);
                    let message = (self.reference_diagnostics_template.as_ref())
                        .and_then(|t| self.one_line(t, &m, translation.as_ref()))
                        .filter(|message| !message.trim().is_empty())
                        .unwrap_or_else(|| self.reference(&m));
                    diagnostics.push(diagnostic(range, severity, "reference", message));
                }
                // In document order, so they read top to bottom
                diagnostics.sort_by_key(|d| (d.range.start.line, d.range.start.character));
            }
            diagnostics
        });
        PublishDiagnosticsParams::new(uri.clone(), diagnostics, None)
    }

    /// The document's diagnostics when the editor asks for them (pull diagnostics)
    pub fn pull_diagnostics(
        &self,
        params: lsp_types::DocumentDiagnosticParams,
    ) -> lsp_types::DocumentDiagnosticReportResult {
        let items = self.diagnostics(&params.text_document.uri).diagnostics;
        lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(
                lsp_types::RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: lsp_types::FullDocumentDiagnosticReport {
                        result_id: None,
                        items,
                    },
                },
            ),
        )
    }

    /// A hint after each reference in the range: how it is written in the configured format
    /// (where that differs, by default), its OSIS id, or a template (after the Bible the
    /// reference names, if any)
    pub fn inlay_hints(&self, params: InlayHintParams) -> Option<Vec<InlayHint>> {
        if self.inlay_hints == InlayHints::Never
            || bible::is_book_uri(params.text_document.uri.as_str())
        {
            return Some(vec![]);
        }
        let text = self.documents.get(&params.text_document.uri)?;
        let index = LineIndex::new(text);
        let hints = self
            .matcher
            .search_with_translations(text, &self.bibles.translations)
            .into_iter()
            .filter_map(
                |TranslatedMatch {
                     reference: m,
                     translation,
                 }| {
                    let bytes = m.location.bytes;
                    let after = match &self.inlay_hints {
                        InlayHints::Template(_) => {
                            translation.as_ref().map_or(bytes.end, |t| t.span.end)
                        }
                        _ => bytes.end,
                    };
                    let end = lsp_position(&index, after);
                    let start = lsp_position(&index, bytes.start);
                    if end < params.range.start || start > params.range.end {
                        return None;
                    }
                    let label = match &self.inlay_hints {
                        InlayHints::Osis => m.psg.to_osis(self.matcher.data().books())?,
                        InlayHints::Usfm => m.psg.to_usfm(self.matcher.data().books())?,
                        InlayHints::Changed
                            if self.reference(&m) == text[bytes.start..bytes.end] =>
                        {
                            return None;
                        }
                        InlayHints::Template(template) => {
                            let label = self.one_line(template, &m, translation.as_ref())?;
                            (!label.trim().is_empty()).then_some(label)?
                        }
                        _ => self.reference(&m),
                    };
                    Some(InlayHint {
                        position: end,
                        label: InlayHintLabel::String(label),
                        kind: None,
                        text_edits: None,
                        tooltip: None,
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    })
                },
            )
            .collect();
        Some(hints)
    }

    /// A template filled for a reference, on one line (inlay hints and diagnostics): one written
    /// in the setting, or a named one with its markup
    fn one_line(
        &self,
        template: &TemplateRef,
        found: &BibleMatch,
        translation: Option<&Translation>,
    ) -> Option<String> {
        let bible = self.bibles.named(translation);
        let (display, markup) = match template {
            TemplateRef::Inline(display) => (display, None),
            TemplateRef::Named(name) => {
                let template = self.templates.get(name)?;
                let merged = bible.map(|b| b.markup.merged(&template.markup));
                (&template.display, merged)
            }
        };
        let fill = TextFill {
            bible,
            markup: markup.as_ref(),
            line_prefix: "",
            join: " ",
        };
        self.fill_template(display, found, translation, &fill)
    }
}
