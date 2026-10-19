/*!
USJ, USX as JSON: the same elements, an element's name a node's `type`, its `style` the node's
`marker`, its other attributes the node's own keys, and what it holds its `content` (text as
strings). So USJ is read by writing it as USX ([`usj_to_usx`]), and written from USX
([`usx_to_usj`]); USFM goes through USX both ways ([`convert`]).
- USX into USJ: text's whitespace as [`usx_to_usfm`] takes it (a run is one space; whitespace
  alone between blocks is the file's layout, dropped). End milestones (`<verse eid=…/>`,
  `<chapter eid=…/>`) are left out, as USJ has none; `<row>` and `<cell>` are `table:row` and
  `table:cell`.
- USJ into USX: a verse or chapter ends (`<verse eid=…/>`) where the next one starts, or the book
  ends, its `sid` the start's (else `JHN 3:16`, from the book's code and the numbers). Attributes
  in usfm3's shape (`"attributes": [{"key": …, "value": …}]`) are read as the node's own keys.
*/

use std::{collections::BTreeMap, sync::Arc};

use logoxide_markup::{
    Markup, decode_entities, parse,
    verses::{usx_chapter_verses, verses},
};
use quick_xml::{
    Reader,
    escape::{escape, partial_escape},
    events::{BytesStart, Event},
};
use serde_json::{Map, Value};
use topos_bible::data::books::BookId;

use super::{
    Book, Skipped, Source, book_files,
    to_usfm::{attributes, collapse, local},
    usfm_to_usx, usx_to_usfm,
};
use crate::{
    error::{Result, TextError},
    files::Files as Storage,
    source::TextFormat,
};

/// A folder of USJ files (`*.json`, `*.usj`), one book each
pub struct UsjSource {
    storage: Arc<dyn Storage>,
    files: BTreeMap<BookId, String>,
    skipped: Vec<Skipped>,
}

impl UsjSource {
    pub fn open(storage: Arc<dyn Storage>, dir: &str) -> Result<Self> {
        let (files, skipped) = book_files(storage.as_ref(), dir, &["json", "usj"], |head| {
            // `{"type": "book", "marker": "id", "code": "JHN", ...}`
            let rest = &head[head.find("\"book\"")?..];
            let rest = &rest[rest.find("\"code\"")? + 6..];
            let rest = rest.trim_start().strip_prefix(':')?.trim_start();
            Some(rest.strip_prefix('"')?.split('"').next()?.to_string())
        })?;
        Ok(Self {
            storage,
            files,
            skipped,
        })
    }

    /// A book as USX
    fn usx(&self, id: BookId) -> Result<String> {
        let path = self
            .files
            .get(&id)
            .ok_or_else(|| TextError::NotFound(format!("book {}", id.0)))?;
        usj_to_usx(&self.storage.read_string(path)?).map_err(|e| match e {
            TextError::Book { message, .. } => TextError::Book {
                book: path.clone(),
                message,
            },
            e => e,
        })
    }
}

impl Source for UsjSource {
    fn format(&self) -> Option<TextFormat> {
        Some(TextFormat::Usj)
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.files
    }

    fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }

    fn book(&self, id: BookId) -> Result<Book> {
        let nodes = parse(Markup::Usx, &self.usx(id)?);
        let spans = verses(&nodes);
        Ok(Book { id, nodes, spans })
    }

    fn chapter_verses(&self, id: BookId) -> Result<Vec<u8>> {
        Ok(usx_chapter_verses(&self.usx(id)?))
    }
}

/**
A book in USFM, USX, or USJ written in another of them, through USX: USFM read and written by
usfm3 ([`usfm_to_usx`], [`usx_to_usfm`]), USJ as above. Into its own format, it's normalized the
same way (USX through USJ, the others through USX). Other formats are an error
*/
pub fn convert(text: &str, from: TextFormat, to: TextFormat) -> Result<String> {
    for format in [from, to] {
        if !matches!(format, TextFormat::Usx | TextFormat::Usfm | TextFormat::Usj) {
            return Err(error(
                &format.name().to_ascii_uppercase(),
                "only USFM, USX, and USJ convert into each other",
            ));
        }
    }
    let usx = match from {
        TextFormat::Usfm => usfm_to_usx(text)?,
        TextFormat::Usj => usj_to_usx(text)?,
        _ if to == TextFormat::Usx => usj_to_usx(&usx_to_usj(text)?)?,
        _ => text.to_string(),
    };
    match to {
        TextFormat::Usfm => usx_to_usfm(&usx),
        TextFormat::Usj => usx_to_usj(&usx),
        _ => Ok(usx),
    }
}

fn error(format: &str, message: impl Into<String>) -> TextError {
    TextError::Book {
        book: format.to_string(),
        message: message.into(),
    }
}

/// A USX book as USJ (JSON, indented)
pub fn usx_to_usj(usx: &str) -> Result<String> {
    let usx = usx.trim_start_matches('\u{feff}');
    let mut reader = Reader::from_str(usx);
    let mut open = vec![Element::document()];
    loop {
        let event = reader
            .read_event()
            .map_err(|e| error("USX", format!("at byte {}: {e}", reader.error_position())))?;
        match event {
            Event::Eof => break,
            Event::Start(e) => open.push(Element::from(&e)),
            Event::Empty(e) => {
                let element = Element::from(&e);
                if !element.ends {
                    close(&mut open, element);
                }
            }
            Event::End(_) => {
                if open.len() > 1
                    && let Some(element) = open.pop()
                {
                    close(&mut open, element);
                }
            }
            Event::Text(t) => text(&mut open, &t),
            Event::GeneralRef(r) => {
                let reference: &str = &r;
                text(&mut open, &decode_entities(&format!("&{reference};")));
            }
            Event::CData(c) => text(&mut open, &c.into_inner()),
            _ => {}
        }
    }
    // A file cut short: what's open is closed where it ends
    while open.len() > 1 {
        if let Some(element) = open.pop() {
            close(&mut open, element);
        }
    }
    let content = open.pop().map(|e| e.content).unwrap_or_default();
    let mut document = Map::new();
    document.insert("type".into(), "USJ".into());
    document.insert("version".into(), "3.1".into());
    document.insert("content".into(), Value::Array(content));
    serde_json::to_string_pretty(&Value::Object(document))
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(|e| error("USJ", e.to_string()))
}

/// An element being read: its node's keys, and what's read inside it so far
struct Element {
    /// `None` for the document, or `<usx>`, whose content is the document's
    node: Option<Map<String, Value>>,
    content: Vec<Value>,
    /// Whitespace in it is layout, not text
    holds_blocks: bool,
    /// Whitespace at its start and end is layout (a paragraph's, a cell's)
    trims: bool,
    /// An end milestone, which USJ hasn't
    ends: bool,
}

impl Element {
    fn document() -> Self {
        Element {
            node: None,
            content: vec![],
            holds_blocks: true,
            trims: false,
            ends: false,
        }
    }

    fn from(e: &BytesStart) -> Self {
        let name = local(e.name().0);
        if name == "usx" {
            return Element::document();
        }
        let attributes = attributes(e);
        let has = |key: &str| attributes.iter().any(|(k, _)| k == key);
        let ends = matches!(name.as_str(), "verse" | "chapter") && !has("number") && has("eid");
        let kind = match name.as_str() {
            "row" => "table:row",
            "cell" => "table:cell",
            name => name,
        };
        let mut node = Map::new();
        node.insert("type".into(), kind.into());
        if let Some((_, style)) = attributes.iter().find(|(k, _)| k == "style") {
            node.insert("marker".into(), style.as_str().into());
        }
        for (key, value) in attributes.into_iter().filter(|(k, _)| k != "style") {
            node.insert(key, value.into());
        }
        Element {
            node: Some(node),
            content: vec![],
            holds_blocks: matches!(name.as_str(), "periph" | "table" | "row" | "sidebar"),
            trims: matches!(name.as_str(), "para" | "book" | "cell"),
            ends,
        }
    }
}

/// An element ended: its node goes into the one around it
fn close(open: &mut [Element], element: Element) {
    let Some(parent) = open.last_mut() else {
        return;
    };
    let mut content = element.content;
    if element.trims {
        if let Some(Value::String(first)) = content.first_mut() {
            *first = first.trim_start_matches(' ').to_string();
        }
        if let Some(Value::String(last)) = content.last_mut() {
            last.truncate(last.trim_end_matches(' ').len());
        }
        content.retain(|v| v.as_str() != Some(""));
    }
    match element.node {
        Some(mut node) => {
            if !content.is_empty() {
                node.insert("content".into(), Value::Array(content));
            }
            parent.content.push(Value::Object(node));
        }
        None => parent.content.extend(content),
    }
}

/// Text inside the element being read, its whitespace runs one space each
fn text(open: &mut [Element], raw: &str) {
    let Some(element) = open.last_mut() else {
        return;
    };
    let collapsed = collapse(raw);
    if collapsed.trim_matches(' ').is_empty() && element.holds_blocks {
        return;
    }
    match element.content.last_mut() {
        // Text cut by an entity is one text
        Some(Value::String(before)) => {
            let joined = match before.ends_with(' ') && collapsed.starts_with(' ') {
                true => &collapsed[1..],
                false => &collapsed[..],
            };
            before.push_str(joined);
        }
        _ => element.content.push(Value::String(collapsed)),
    }
}

/// A USJ book as USX
pub fn usj_to_usx(usj: &str) -> Result<String> {
    let usj: Value = serde_json::from_str(usj.trim_start_matches('\u{feff}'))
        .map_err(|e| error("USJ", e.to_string()))?;
    let content = (usj.get("content").and_then(Value::as_array))
        .ok_or_else(|| error("USJ", "not a USJ document (an object with `content`)"))?;
    let version = usj.get("version").and_then(Value::as_str).unwrap_or("3.1");
    let mut writer = UsxWriter::default();
    writer.out.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<usx version=\"{}\">",
        escape(version)
    ));
    for node in content {
        writer.node(node)?;
    }
    writer.end_verse();
    writer.end_chapter();
    writer.out.push_str("</usx>\n");
    Ok(writer.out)
}

/// USX being written, and the verse and chapter open (each ends where the next starts)
#[derive(Default)]
struct UsxWriter {
    out: String,
    book: Option<String>,
    chapter: Option<String>,
    /// The open verse's and chapter's `sid`
    verse: Option<String>,
    chapter_sid: Option<String>,
}

impl UsxWriter {
    fn node(&mut self, node: &Value) -> Result<()> {
        let node = match node {
            Value::String(text) => {
                self.out.push_str(&partial_escape(text.as_str()));
                return Ok(());
            }
            Value::Object(node) => node,
            other => return Err(error("USJ", format!("not a node: {other}"))),
        };
        let kind = (node.get("type").and_then(Value::as_str))
            .ok_or_else(|| error("USJ", "a node without a `type`"))?;
        let name = match kind {
            "table:row" => "row",
            "table:cell" => "cell",
            name => name,
        };
        let valid = |c: char| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':');
        if name.is_empty() || !name.chars().all(valid) {
            return Err(error("USJ", format!("`{kind}` is no USX element")));
        }
        let mut attributes: Vec<(String, String)> = vec![];
        for (key, value) in node {
            let key = match key.as_str() {
                "type" | "content" => continue,
                "marker" => "style",
                key => key,
            };
            match value {
                Value::String(s) => attributes.push((key.to_string(), s.clone())),
                Value::Number(_) | Value::Bool(_) => {
                    attributes.push((key.to_string(), value.to_string()));
                }
                // usfm3's `[{"key": "strong", "value": "H1"}]`
                Value::Array(pairs) if key == "attributes" => {
                    attributes.extend(pairs.iter().filter_map(|pair| {
                        let key = pair.get("key")?.as_str()?;
                        let value = pair.get("value")?.as_str()?;
                        Some((key.to_string(), value.to_string()))
                    }))
                }
                _ => {}
            }
        }
        let get = |key: &str| {
            (attributes.iter())
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
        };
        let (number, sid) = (get("number"), get("sid"));
        match (name, number) {
            ("book", _) => self.book = get("code"),
            ("chapter", Some(number)) => {
                self.end_verse();
                self.end_chapter();
                let made = (self.book.as_ref()).map(|book| format!("{book} {number}"));
                if sid.is_none()
                    && let Some(made) = &made
                {
                    attributes.push((String::from("sid"), made.clone()));
                }
                self.chapter_sid = sid.or(made);
                self.chapter = Some(number);
            }
            ("verse", Some(number)) => {
                self.end_verse();
                let made = match (&self.book, &self.chapter) {
                    (Some(book), Some(chapter)) => Some(format!("{book} {chapter}:{number}")),
                    _ => None,
                };
                if sid.is_none()
                    && let Some(made) = &made
                {
                    attributes.push((String::from("sid"), made.clone()));
                }
                self.verse = sid.or(made);
            }
            // An end milestone the USJ gives: the one written
            ("chapter", None) => {
                self.end_verse();
                self.chapter_sid = None;
            }
            ("verse", None) => self.verse = None,
            _ => {}
        }
        self.out.push('<');
        self.out.push_str(name);
        for (key, value) in &attributes {
            self.out
                .push_str(&format!(" {key}=\"{}\"", escape(value.as_str())));
        }
        let content = node.get("content").and_then(Value::as_array);
        match content.filter(|c| !c.is_empty()) {
            Some(content) => {
                self.out.push('>');
                for child in content {
                    self.node(child)?;
                }
                self.out.push_str(&format!("</{name}>"));
            }
            None => self.out.push_str("/>"),
        }
        Ok(())
    }

    fn end_verse(&mut self) {
        if let Some(sid) = self.verse.take() {
            self.out
                .push_str(&format!("<verse eid=\"{}\"/>", escape(sid.as_str())));
        }
    }

    fn end_chapter(&mut self) {
        if let Some(sid) = self.chapter_sid.take() {
            self.out
                .push_str(&format!("<chapter eid=\"{}\"/>", escape(sid.as_str())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USX: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<usx version=\"3.0\">\n  \
<book code=\"PSA\" style=\"id\">Psalms</book>\n  <chapter number=\"1\" style=\"c\" sid=\"PSA 1\" />\n  \
<para style=\"q1\">\n    <verse number=\"1\" style=\"v\" sid=\"PSA 1:1\" />The <char style=\"add\">way \
of the <char style=\"nd\">Lord</char></char> &amp; <char style=\"w\" strong=\"H1\">blessed</char>\
<note caller=\"+\" style=\"f\" category=\"x\"><char style=\"ft\">Or happy</char></note>.\
<verse eid=\"PSA 1:1\" /></para>\n  <table>\n    <row style=\"tr\"><cell style=\"tc1\" align=\"start\">\
a</cell></row>\n  </table>\n  <chapter eid=\"PSA 1\" />\n</usx>\n";

    #[test]
    fn usx_into_usj() {
        let usj: Value = serde_json::from_str(&usx_to_usj(USX).unwrap()).unwrap();
        assert_eq!(
            usj,
            serde_json::json!({
                "type": "USJ",
                "version": "3.1",
                "content": [
                    {"type": "book", "marker": "id", "code": "PSA", "content": ["Psalms"]},
                    {"type": "chapter", "marker": "c", "number": "1", "sid": "PSA 1"},
                    {"type": "para", "marker": "q1", "content": [
                        {"type": "verse", "marker": "v", "number": "1", "sid": "PSA 1:1"},
                        "The ",
                        {"type": "char", "marker": "add", "content": [
                            "way of the ",
                            {"type": "char", "marker": "nd", "content": ["Lord"]},
                        ]},
                        " & ",
                        {"type": "char", "marker": "w", "strong": "H1", "content": ["blessed"]},
                        {"type": "note", "marker": "f", "caller": "+", "category": "x", "content": [
                            {"type": "char", "marker": "ft", "content": ["Or happy"]},
                        ]},
                        ".",
                    ]},
                    {"type": "table", "content": [
                        {"type": "table:row", "marker": "tr", "content": [
                            {"type": "table:cell", "marker": "tc1", "align": "start", "content": ["a"]},
                        ]},
                    ]},
                ],
            })
        );
    }

    #[test]
    fn usj_into_usx_and_back() {
        let usj = usx_to_usj(USX).unwrap();
        let usx = usj_to_usx(&usj).unwrap();
        assert_eq!(
            usx,
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<usx version=\"3.1\"><book style=\"id\" \
code=\"PSA\">Psalms</book><chapter style=\"c\" number=\"1\" sid=\"PSA 1\"/><para style=\"q1\">\
<verse style=\"v\" number=\"1\" sid=\"PSA 1:1\"/>The <char style=\"add\">way of the <char \
style=\"nd\">Lord</char></char> &amp; <char style=\"w\" strong=\"H1\">blessed</char><note \
style=\"f\" caller=\"+\" category=\"x\"><char style=\"ft\">Or happy</char></note>.</para><table>\
<row style=\"tr\"><cell style=\"tc1\" align=\"start\">a</cell></row></table><verse eid=\"PSA 1:1\"/>\
<chapter eid=\"PSA 1\"/></usx>\n"
        );
        assert_eq!(usx_to_usj(&usx).unwrap(), usj);
    }

    #[test]
    fn sids_made_and_usfm3_attributes() {
        let usj = r#"{"type": "USJ", "version": "3.1", "content": [
            {"type": "book", "marker": "id", "code": "JHN"},
            {"type": "chapter", "marker": "c", "number": "3"},
            {"type": "para", "marker": "p", "content": [
                {"type": "verse", "marker": "v", "number": "16"}, "For God ",
                {"type": "char", "marker": "w", "content": ["loved"],
                 "attributes": [{"key": "lemma", "value": "agapao"}]}
            ]}
        ]}"#;
        assert_eq!(
            usj_to_usx(usj).unwrap(),
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<usx version=\"3.1\"><book style=\"id\" \
code=\"JHN\"/><chapter style=\"c\" number=\"3\" sid=\"JHN 3\"/><para style=\"p\"><verse \
style=\"v\" number=\"16\" sid=\"JHN 3:16\"/>For God <char style=\"w\" lemma=\"agapao\">loved</char>\
</para><verse eid=\"JHN 3:16\"/><chapter eid=\"JHN 3\"/></usx>\n"
        );
        assert_eq!(
            convert(usj, TextFormat::Usj, TextFormat::Usfm).unwrap(),
            "\\id JHN\n\\c 3\n\\p \\v 16 For God \\w loved|lemma=\"agapao\"\\w*\n"
        );
    }

    #[test]
    fn what_is_not_usj() {
        assert!(usj_to_usx("[1, 2]").is_err());
        assert!(usj_to_usx("{\"content\": [{\"marker\": \"p\"}]}").is_err());
        assert!(usj_to_usx("{\"content\": [{\"type\": \"a b\"}]}").is_err());
        assert!(usj_to_usx("{\"content\": [").is_err());
        assert!(convert("", TextFormat::Osis, TextFormat::Usj).is_err());
    }
}
