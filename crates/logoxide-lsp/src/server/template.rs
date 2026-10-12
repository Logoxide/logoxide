//! Templates for a reference: code actions, copying, and inlay hints

use super::*;
use crate::bible::Verses;

/// What a template fills its passage's text with, and how
pub(super) struct TextFill<'a> {
    /// The Bible the text comes from (none: a template that needs the text is [`None`])
    pub bible: Option<&'a Bible>,
    pub markup: Option<&'a settings::MarkupSettings>,
    /// Written before each line of the text, like `> `
    pub line_prefix: &'a str,
    /// The text's lines joined with this instead (inlay hints are one line)
    pub join: &'a str,
}

/// A placeholder for a passage's text: `{text}`, `{first:N}`, or `{last:N}`
fn text_placeholder(name: &str) -> Option<Verses> {
    let count = |n: &str| n.trim().parse().ok();
    match name.split_once(':') {
        None if name == "text" => Some(Verses::All),
        Some(("first", n)) => count(n).map(Verses::First),
        Some(("last", n)) => count(n).map(Verses::Last),
        _ => None,
    }
}

impl Server {
    /**
    A template filled for a reference, or [`None`] if it needs a passage's text that the Bible
    doesn't have:
    - `{reference}` (as formatted, like `John 3:16`), `{osis}` (`John.3.16`), `{usfm}`
      (`JHN 3:16`), `{bible}` (the Bible's name), `{translation}` (see
      [`settings::fill_translation`])
    - `{text}` (the passage), `{first:N}` and `{last:N}` (its first or last N verses)
    - Anything else in braces is left as it is
    */
    pub(super) fn fill_template(
        &self,
        template: &str,
        found: &BibleMatch,
        translation: Option<&Translation>,
        fill: &TextFill,
    ) -> Option<String> {
        let books = self.matcher.data().books();
        let named = fill.bible.and_then(|b| self.translation_of(b, translation));
        let template = settings::fill_translation(template, named);
        let mut filled = String::new();
        let mut rest = template.as_str();
        while let Some(open) = rest.find('{') {
            filled.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                filled.push_str(&rest[open..]);
                return Some(filled);
            };
            let name = &after[..close];
            let value = match name {
                "reference" => Some(self.reference(found)),
                "osis" => Some(found.psg.to_osis(books).unwrap_or_default()),
                "usfm" => Some(found.psg.to_usfm(books).unwrap_or_default()),
                "bible" => Some(fill.bible.map_or("", |b| b.name.as_str()).to_string()),
                _ => match text_placeholder(name) {
                    Some(verses) => Some(self.template_text(found, verses, fill)?),
                    None => None,
                },
            };
            match value {
                Some(value) => filled.push_str(&value),
                None => filled.push_str(&rest[open..open + close + 2]),
            }
            rest = &after[close + 1..];
        }
        filled.push_str(rest);
        Some(filled)
    }

    /// Some of a passage's verses as a template writes them: each line prefixed, or joined
    fn template_text(&self, found: &BibleMatch, verses: Verses, fill: &TextFill) -> Option<String> {
        let text = fill
            .bible?
            .passage_verses(&found.psg, fill.markup, verses)?
            .text;
        let lines: Vec<String> = text
            .lines()
            .map(|line| format!("{}{line}", fill.line_prefix).trim_end().to_string())
            .collect();
        match fill.join {
            "\n" => Some(lines.join("\n")),
            join => Some(
                (lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()))
                    .collect::<Vec<_>>()
                    .join(join),
            ),
        }
    }
}
