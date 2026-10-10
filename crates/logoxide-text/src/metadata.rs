/*!
What a file says about itself: an EPUB's title, subtitle, authors (`dc:creator`, leaving out
editors, illustrators, and other roles), language, publisher, and rights, from its package
document (the `.opf` its `META-INF/container.xml` names).
*/

use logoxide_markup::decode_entities;
use quick_xml::{Reader, events::Event};

use crate::error::{Result, TextError};

/// What a file's own metadata says (empty where it says nothing)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileMetadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    /// Its rights statement, if it has one (shown to the person making a package, who gives the
    /// license: nothing guesses one)
    pub rights: Option<String>,
}

/// An EPUB's metadata, from the package document its `META-INF/container.xml` names
pub fn epub_metadata(bytes: &[u8]) -> Result<FileMetadata> {
    let files = crate::archive::unzip(bytes)?;
    let file = |path: &str| {
        (files.iter())
            .find(|(p, _)| p == path)
            .map(|(_, b)| String::from_utf8_lossy(b).into_owned())
            .ok_or_else(|| TextError::Archive(format!("EPUB: no {path}")))
    };
    let container = file("META-INF/container.xml")?;
    let opf_path = (elements(&container).into_iter())
        .find(|e| e.name == "rootfile")
        .and_then(|e| e.attr("full-path").map(String::from))
        .ok_or_else(|| TextError::Archive(String::from("EPUB: no rootfile in its container")))?;
    Ok(opf_metadata(&file(&opf_path)?))
}

/// The metadata in an EPUB's package document (`.opf`), EPUB 2 or 3
pub fn opf_metadata(opf: &str) -> FileMetadata {
    let found = elements(opf);
    // EPUB 3 says what a title or a creator is with `<meta refines="#id" property="…">`
    let refined = |id: Option<&str>, property: &str| {
        let id = id?;
        (found.iter())
            .find(|m| {
                m.name == "meta"
                    && m.attr("refines") == Some(&format!("#{id}"))
                    && m.attr("property") == Some(property)
            })
            .map(|m| m.text.trim().to_string())
    };
    let text = |name: &str| {
        (found.iter())
            .filter(|e| e.name == name)
            .map(|e| e.text.trim().to_string())
            .find(|t| !t.is_empty())
    };
    let titles: Vec<&Element> = (found.iter())
        .filter(|e| e.name == "title" && !e.text.trim().is_empty())
        .collect();
    let title_type = |e: &Element| refined(e.attr("id"), "title-type");
    // The main title, else the first; the subtitle, else another one
    let main = (titles.iter())
        .position(|e| title_type(e).as_deref() == Some("main"))
        .unwrap_or(0);
    let title = titles.get(main);
    let subtitle = (titles.iter())
        .find(|e| title_type(e).as_deref() == Some("subtitle"))
        .or_else(|| (titles.iter().enumerate()).find_map(|(i, e)| (i != main).then_some(e)));
    let authors = (found.iter())
        .filter(|e| e.name == "creator" && !e.text.trim().is_empty())
        .filter(|e| {
            // Authors only: a role that says otherwise (editor, illustrator) leaves one out
            let role = (e.attr("role").map(String::from)).or_else(|| refined(e.attr("id"), "role"));
            role.is_none_or(|r| r == "aut")
        })
        .map(|e| e.text.trim().to_string())
        .collect();
    FileMetadata {
        title: title.map(|e| e.text.trim().to_string()),
        subtitle: subtitle.map(|e| e.text.trim().to_string()),
        authors,
        language: text("language"),
        publisher: text("publisher"),
        rights: text("rights"),
    }
}

/// An element of a small XML document: its local name (no prefix), attributes (local names too),
/// and its text
pub struct Element {
    pub name: String,
    attributes: Vec<(String, String)>,
    pub text: String,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        (self.attributes.iter())
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Every element in `xml`, in document order, each with the text directly inside it
pub fn elements(xml: &str) -> Vec<Element> {
    let local = |name: &str| name.rsplit(':').next().unwrap_or_default().to_string();
    let mut reader = Reader::from_str(xml);
    let mut found: Vec<Element> = vec![];
    let mut open: Vec<usize> = vec![];
    let text_in = |found: &mut Vec<Element>, open: &[usize], text: &str| {
        if let Some(&at) = open.last() {
            found[at].text.push_str(text);
        }
    };
    loop {
        let event = match reader.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(event) => event,
        };
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let attributes = (e.attributes().flatten())
                    .map(|a| (local(a.key.0), decode_entities(&a.value)))
                    .collect();
                found.push(Element {
                    name: local(e.name().0),
                    attributes,
                    text: String::new(),
                });
                if matches!(event, Event::Start(_)) {
                    open.push(found.len() - 1);
                }
            }
            Event::End(_) => {
                open.pop();
            }
            Event::Text(t) => text_in(&mut found, &open, &t),
            Event::GeneralRef(r) => {
                let reference: &str = &r;
                text_in(
                    &mut found,
                    &open,
                    &decode_entities(&format!("&{reference};")),
                );
            }
            Event::CData(c) => text_in(&mut found, &open, &c.into_inner()),
            _ => {}
        }
    }
    found
}
