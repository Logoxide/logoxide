/*!
USX into USFM, the other way from [`usfm_to_usx`](super::usfm_to_usx): the USX is read into
[`usfm3`]'s document (a `style` is a USFM marker's name, so each element is the node usfm3 would
have read from that marker) and written by usfm3 as normalized USFM, not the publisher's layout.
- A run of spaces, tabs, and line breaks is one space, as USX's own readers take it; whitespace
  alone between paragraphs, rows, and other blocks is the file's layout and is dropped.
- End milestones (`<verse eid=…/>`, `<chapter eid=…/>`) are left out: USFM has none.
- A character style inside another is written `\+nd …\+nd*` (usfm3 writes them bare, which a
  USFM reader takes as closing the outer one), a `<ref>` inside one `\+ref`.
- A note's `category` is a `\cat` at the note's start; `closed="false"` is dropped (usfm3 closes
  every marker it writes). A `<periph>` keeps only its content, and a figure's `file` is `src`.
*/

use std::borrow::Cow;

use logoxide_markup::decode_entities;
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use usfm3::ast::{Attribute, ChapterData, CharData, Document, Node, VerseData};

use crate::error::{Result, TextError};

/// A USX book as USFM (through usfm3), for tools that take USFM
pub fn usx_to_usfm(usx: &str) -> Result<String> {
    let usx = usx.trim_start_matches('\u{feff}');
    let mut reader = Reader::from_str(usx);
    let mut open = vec![Frame::new(Open::Root)];
    loop {
        let event = reader.read_event().map_err(|e| TextError::Book {
            book: String::from("USX"),
            message: format!("at byte {}: {e}", reader.error_position()),
        })?;
        match event {
            Event::Eof => break,
            Event::Start(e) => open.push(Frame::new(Open::from(&e))),
            Event::Empty(e) => match leaf(&e) {
                Some(node) => push(&mut open, node),
                None => {
                    let frame = Frame::new(Open::from(&e));
                    close(&mut open, frame);
                }
            },
            Event::End(_) => {
                if open.len() > 1
                    && let Some(frame) = open.pop()
                {
                    close(&mut open, frame);
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
        if let Some(frame) = open.pop() {
            close(&mut open, frame);
        }
    }
    let content = open.pop().map(|f| f.content).unwrap_or_default();
    Ok(usfm3::usfm::to_usfm_string(&Document { content }))
}

/// An element being read, and the nodes read inside it so far
struct Frame {
    open: Open,
    content: Vec<Node<'static>>,
}

impl Frame {
    fn new(open: Open) -> Self {
        Self {
            open,
            content: vec![],
        }
    }
}

/// The elements that hold others, with what their start tag says
enum Open {
    /// `<usx>`, or the document around it
    Root,
    Book {
        code: String,
    },
    Para {
        style: String,
    },
    Char {
        style: String,
        attributes: Vec<Attribute<'static>>,
    },
    Note {
        style: String,
        caller: String,
        category: Option<String>,
    },
    Figure {
        style: String,
        attributes: Vec<Attribute<'static>>,
    },
    Sidebar {
        style: String,
        category: Option<String>,
    },
    Periph {
        alt: Option<String>,
    },
    Table,
    Row {
        style: String,
    },
    Cell {
        style: String,
        align: String,
    },
    Ref {
        attributes: Vec<Attribute<'static>>,
    },
    /// An element USX doesn't have: its content goes on as if it weren't there
    Other,
}

impl Open {
    fn from(e: &BytesStart) -> Self {
        let attributes = attributes(e);
        let get = |name: &str| {
            (attributes.iter())
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        };
        let style = |default: &str| get("style").unwrap_or_else(|| default.to_string());
        // Every attribute but those USX uses for itself, as USFM's `|key="value"`
        let rest = |drop: &[&str]| {
            (attributes.iter())
                .filter(|(k, _)| !drop.contains(&k.as_str()))
                .map(|(k, v)| Attribute {
                    key: Cow::Owned(k.clone()),
                    value: Cow::Owned(v.clone()),
                })
                .collect()
        };
        match local(e.name().0).as_str() {
            "usx" => Open::Root,
            "book" => Open::Book {
                code: get("code").unwrap_or_default(),
            },
            "para" => Open::Para { style: style("p") },
            "char" => Open::Char {
                style: style("nd"),
                attributes: rest(&["style", "closed"]),
            },
            "note" => Open::Note {
                style: style("f"),
                caller: get("caller").unwrap_or_else(|| String::from("+")),
                category: get("category"),
            },
            "figure" => Open::Figure {
                style: style("fig"),
                attributes: (rest(&["style"]).into_iter())
                    .map(|mut a: Attribute| {
                        if a.key == "file" {
                            a.key = Cow::Borrowed("src");
                        }
                        a
                    })
                    .collect(),
            },
            "sidebar" => Open::Sidebar {
                style: style("esb"),
                category: get("category"),
            },
            "periph" => Open::Periph { alt: get("alt") },
            "table" => Open::Table,
            "row" => Open::Row { style: style("tr") },
            "cell" => Open::Cell {
                style: style("tc1"),
                align: get("align").unwrap_or_else(|| String::from("start")),
            },
            "ref" => Open::Ref {
                attributes: rest(&[]),
            },
            _ => Open::Other,
        }
    }

    /// Whether whitespace in it is layout, not text
    fn holds_blocks(&self) -> bool {
        matches!(
            self,
            Open::Root
                | Open::Periph { .. }
                | Open::Table
                | Open::Row { .. }
                | Open::Sidebar { .. }
        )
    }
}

/// An element that holds nothing in USX: a chapter, a verse, a milestone, an optional break (an
/// end milestone is nothing in USFM)
fn leaf(e: &BytesStart) -> Option<Option<Node<'static>>> {
    let attributes = attributes(e);
    let get = |name: &str| {
        (attributes.iter())
            .find(|(k, _)| k == name)
            .map(|(_, v)| Cow::Owned(v.clone()))
    };
    let node = match local(e.name().0).as_str() {
        "chapter" => get("number").map(|number| {
            Node::Chapter(Box::new(ChapterData {
                marker: "c".into(),
                number,
                sid: get("sid"),
                altnumber: get("altnumber"),
                pubnumber: get("pubnumber"),
            }))
        }),
        "verse" => get("number").map(|number| {
            Node::Verse(Box::new(VerseData {
                marker: "v".into(),
                number,
                sid: get("sid"),
                altnumber: get("altnumber"),
                pubnumber: get("pubnumber"),
            }))
        }),
        "ms" => Some(Node::Milestone {
            marker: get("style").unwrap_or_default().as_ref().into(),
            attributes: (attributes.iter())
                .filter(|(k, _)| k != "style")
                .map(|(k, v)| Attribute {
                    key: Cow::Owned(k.clone()),
                    value: Cow::Owned(v.clone()),
                })
                .collect(),
        }),
        "optbreak" => Some(Node::OptBreak),
        _ => return None,
    };
    Some(node)
}

/// A leaf's node into the element around it (an end milestone has none)
fn push(open: &mut [Frame], node: Option<Node<'static>>) {
    if let (Some(node), Some(frame)) = (node, open.last_mut()) {
        frame.content.push(node);
    }
}

/// An element ended: its node goes into the one around it
fn close(open: &mut [Frame], frame: Frame) {
    let Some(parent) = open.last_mut() else {
        return;
    };
    let in_char = matches!(parent.open, Open::Char { .. } | Open::Ref { .. });
    // `\+nd` inside another character style
    let nested = |style: String| match in_char {
        true => format!("+{style}"),
        false => style,
    };
    let content = frame.content;
    let node = match frame.open {
        Open::Root | Open::Other => {
            parent.content.extend(content);
            return;
        }
        Open::Book { code } => Node::Book {
            marker: "id".into(),
            code: Cow::Owned(code),
            content,
        },
        Open::Para { style } => Node::Para {
            marker: style.into(),
            content,
        },
        Open::Char { style, attributes } => Node::Char(Box::new(CharData {
            marker: nested(style).into(),
            content,
            attributes,
        })),
        Open::Note {
            style,
            caller,
            category,
        } => {
            let mut all = vec![];
            if let Some(category) = category {
                all.push(Node::Char(Box::new(CharData {
                    marker: "cat".into(),
                    content: vec![Node::Text(Cow::Owned(category))],
                    attributes: vec![],
                })));
            }
            all.extend(content);
            Node::Note {
                marker: style.into(),
                caller: Cow::Owned(caller),
                category: None,
                content: all,
            }
        }
        Open::Figure { style, attributes } => Node::Figure {
            marker: style.into(),
            content,
            attributes,
        },
        Open::Sidebar { style, category } => Node::Sidebar {
            marker: style.into(),
            category: category.map(Cow::Owned),
            content,
        },
        Open::Periph { alt } => Node::Periph {
            alt: alt.map(Cow::Owned),
            content,
            attributes: vec![],
        },
        Open::Table => Node::Table { content },
        Open::Row { style } => Node::TableRow {
            marker: style.into(),
            content,
        },
        Open::Cell { style, align } => Node::TableCell {
            marker: style.into(),
            align: Cow::Owned(align),
            content,
        },
        // usfm3 writes a `Ref` bare, so one inside a character style is written as `\+ref`
        Open::Ref { attributes } if in_char => Node::Char(Box::new(CharData {
            marker: "+ref".into(),
            content,
            attributes,
        })),
        Open::Ref { attributes } => Node::Ref {
            content,
            attributes,
        },
    };
    parent.content.push(node);
}

/// Text inside the element being read, its whitespace runs one space each
fn text(open: &mut [Frame], raw: &str) {
    let Some(frame) = open.last_mut() else {
        return;
    };
    let mut collapsed = String::with_capacity(raw.len());
    let mut space = false;
    for c in raw.chars() {
        match c {
            ' ' | '\t' | '\n' | '\r' => space = true,
            c => {
                if space {
                    collapsed.push(' ');
                    space = false;
                }
                collapsed.push(c);
            }
        }
    }
    if space {
        collapsed.push(' ');
    }
    if collapsed.trim_matches(' ').is_empty() && frame.open.holds_blocks() {
        return;
    }
    match frame.content.last_mut() {
        // Text cut by an entity is one text
        Some(Node::Text(before)) => {
            let joined = match before.ends_with(' ') && collapsed.starts_with(' ') {
                true => &collapsed[1..],
                false => &collapsed[..],
            };
            before.to_mut().push_str(joined);
        }
        _ => frame.content.push(Node::Text(Cow::Owned(collapsed))),
    }
}

/// An element's attributes by local name, their entities decoded
fn attributes(e: &BytesStart) -> Vec<(String, String)> {
    (e.attributes().flatten())
        .map(|a| (local(a.key.0), decode_entities(&a.value)))
        .collect()
}

fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verses_paragraphs_and_end_milestones() {
        let usx = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<usx version=\"3.0\">\n  \
<book code=\"JHN\" style=\"id\">John</book>\n  <chapter number=\"3\" style=\"c\" sid=\"JHN 3\" />\n  \
<para style=\"p\">\n    <verse number=\"16\" style=\"v\" sid=\"JHN 3:16\" />For God so loved \
the world. <verse eid=\"JHN 3:16\" /><verse number=\"17\" style=\"v\" sid=\"JHN 3:17\" />For God \
&amp; his Son.<verse eid=\"JHN 3:17\" /></para>\n  <para style=\"b\" />\n  <chapter eid=\"JHN 3\" />\n</usx>\n";
        assert_eq!(
            usx_to_usfm(usx).unwrap(),
            "\\id JHN John\n\\c 3\n\\p \\v 16 For God so loved the world. \\v 17 For God & his Son.\n\\b\n"
        );
    }

    #[test]
    fn nested_character_styles_and_notes() {
        let usx = "<usx version=\"3.0\"><book code=\"PSA\" style=\"id\"/><chapter number=\"1\" style=\"c\"/>\
<para style=\"q1\"><verse number=\"1\" style=\"v\"/>The <char style=\"add\">way of the \
<char style=\"nd\">Lord</char></char>, <char style=\"w\" strong=\"H1\">blessed</char>\
<note caller=\"+\" style=\"f\" category=\"x\"><char style=\"fr\" closed=\"false\">1:1 </char>\
<char style=\"ft\" closed=\"false\">Or <char style=\"bk\">happy</char></char></note>.</para></usx>";
        assert_eq!(
            usx_to_usfm(usx).unwrap(),
            "\\id PSA\n\\c 1\n\\q1 \\v 1 The \\add way of the \\+nd Lord\\+nd*\\add*, \
\\w blessed|strong=\"H1\"\\w*\\f + \\cat x\\cat*\\fr 1:1 \\fr*\\ft Or \\+bk happy\\+bk*\\ft*\\f*.\n"
        );
    }

    #[test]
    fn broken_xml_is_an_error() {
        assert!(usx_to_usfm("<usx><para style=\"p\">text</char></usx>").is_err());
    }
}
