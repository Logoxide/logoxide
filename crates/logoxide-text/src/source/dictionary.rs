/*!
A dictionary's entries (`entries/` and its `.xml` files, `doc/entries.md`): a root `<entries markup="tei">`
holding `<entry key="G25" headword="ἀγαπάω">…</entry>` elements, each body the entry's markup
as its source gives it; a body that isn't XML (GBF, plain text) is inside CDATA sections, so
the file is well-formed XML whatever it holds. The files are indexed once when opened (each
entry's key, headword, link, and where its body is); a body is parsed when its entry is asked
for.
*/

use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::{Arc, Mutex},
};

use logoxide_markup::{Markup, Node, decode_entities, parse};
use topos_bible::data::books::BookId;

use super::{Entry, Source, TextFormat};
use crate::{
    error::{Result, TextError},
    files::{Files, join},
    keys::{fold, normalize_key},
};

/// A file of entries and the markup its root declares
struct EntryFile {
    path: String,
    markup: Markup,
}

/// Where an entry is, and what its tag says
struct Indexed {
    /// The key in normal form
    key: String,
    headword: Option<String>,
    /// For an alias, the key (in normal form) of the entry whose text it shares
    link: Option<String>,
    file: usize,
    /// The body's bytes in the file
    body: Range<usize>,
}

pub struct DictionarySource {
    storage: Arc<dyn Files>,
    files: Vec<EntryFile>,
    entries: Vec<Indexed>,
    /// Each entry by its normal key
    by_key: HashMap<String, usize>,
    /// The first entry by each folded key ([`fold`])
    by_fold: HashMap<String, usize>,
    /// The file read last, kept while its entries are read one by one
    last: Mutex<Option<(usize, String)>>,
    no_books: BTreeMap<BookId, String>,
}

impl DictionarySource {
    /// Indexes every `*.xml` file under `dir`, in path order
    pub fn open(storage: Arc<dyn Files>, dir: &str) -> Result<Self> {
        let mut names = storage.list(dir)?;
        names.sort();
        let mut source = Self {
            storage: storage.clone(),
            files: vec![],
            entries: vec![],
            by_key: HashMap::new(),
            by_fold: HashMap::new(),
            last: Mutex::new(None),
            no_books: BTreeMap::new(),
        };
        for name in names {
            let wanted =
                (name.rsplit_once('.')).is_some_and(|(_, e)| e.eq_ignore_ascii_case("xml"));
            if !wanted {
                continue;
            }
            let path = join(dir, &name);
            let text = storage.read_string(&path)?;
            source.index(&path, &text)?;
        }
        Ok(source)
    }

    /// Indexes one file's entries
    fn index(&mut self, path: &str, text: &str) -> Result<()> {
        let bad = |message: String| TextError::Entries(format!("{path}: {message}"));
        let root =
            find_tag(text, 0, "entries").ok_or_else(|| bad(String::from("no <entries> root")))?;
        let root_tag = &text[root.clone()];
        let markup = attribute(root_tag, "markup").ok_or_else(|| {
            bad(String::from(
                "<entries> doesn't say its markup (markup=\"tei\")",
            ))
        })?;
        let markup = Markup::from_name(&markup).ok_or_else(|| {
            bad(format!(
                "markup `{markup}` isn't one logoxide reads (tei, osis, thml, gbf, plain)"
            ))
        })?;
        let file = self.files.len();
        self.files.push(EntryFile {
            path: path.to_string(),
            markup,
        });
        let mut from = root.end;
        while let Some(tag) = find_tag(text, from, "entry") {
            let tag_text = &text[tag.clone()];
            let key = attribute(tag_text, "key")
                .ok_or_else(|| bad(String::from("an <entry> without a key")))?;
            let key = normalize_key(&key);
            if key.is_empty() {
                return Err(bad(String::from("an <entry> with an empty key")));
            }
            // `<entry key="…"/>` has no body; else the body runs to the first `</entry>`
            // outside a CDATA section
            let empty = tag_text.trim_end_matches('>').ends_with('/');
            let body = match empty {
                true => tag.end..tag.end,
                false => {
                    let end = entry_end(text, tag.end)
                        .ok_or_else(|| bad(format!("entry `{key}` has no </entry>")))?;
                    tag.end..end
                }
            };
            from = match empty {
                true => body.end,
                false => body.end + "</entry>".len(),
            };
            if let Some(&first) = self.by_key.get(&key) {
                let first = &self.files[self.entries[first].file].path;
                return Err(bad(format!("key `{key}` is also in {first}: keep one")));
            }
            let at = self.entries.len();
            self.by_key.insert(key.clone(), at);
            self.by_fold.entry(fold(&key)).or_insert(at);
            self.entries.push(Indexed {
                headword: attribute(tag_text, "headword").filter(|h| !h.trim().is_empty()),
                link: attribute(tag_text, "link").map(|l| normalize_key(&l)),
                key,
                file,
                body,
            });
        }
        Ok(())
    }

    /// The entry a key names: by its normal form, else loosely
    fn find(&self, key: &str) -> Option<&Indexed> {
        let normal = normalize_key(key);
        let at = (self.by_key.get(&normal)).or_else(|| self.by_fold.get(&fold(&normal)))?;
        self.entries.get(*at)
    }

    /// An entry's body, parsed by its file's markup
    fn body(&self, entry: &Indexed) -> Result<Vec<Node>> {
        let file = &self.files[entry.file];
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_ref().is_none_or(|(f, _)| *f != entry.file) {
            *last = Some((entry.file, self.storage.read_string(&file.path)?));
        }
        let body = (last.as_ref())
            .and_then(|(_, text)| text.get(entry.body.clone()))
            .ok_or_else(|| {
                TextError::Entries(format!("{}: changed since it was indexed", file.path))
            })?;
        Ok(match cdata_text(body) {
            Some(text) => parse(file.markup, &text),
            None => parse(file.markup, body),
        })
    }

    /// How many entries the files have
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Source for DictionarySource {
    fn format(&self) -> Option<TextFormat> {
        Some(TextFormat::Dictionary)
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.no_books
    }

    fn book(&self, id: BookId) -> Result<super::Book> {
        Err(TextError::NotFound(format!(
            "book {}: a dictionary has entries, not verses",
            id.0
        )))
    }

    fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.key.clone()).collect()
    }

    fn entry(&self, key: &str) -> Result<Option<Entry>> {
        let Some(found) = self.find(key) else {
            return Ok(None);
        };
        // An alias reads as its target: one step
        let target = match &found.link {
            Some(link) => match self.find(link) {
                Some(target) => target,
                None => return Ok(None),
            },
            None => found,
        };
        Ok(Some(Entry {
            key: found.key.clone(),
            headword: found.headword.clone().or_else(|| target.headword.clone()),
            nodes: self.body(target)?,
        }))
    }
}

/// The bytes of the next `<{name} …>` (or `<{name}/>`) tag at or after `from`, the `>`
/// included; a comment or a tag of another name is passed over
fn find_tag(text: &str, from: usize, name: &str) -> Option<Range<usize>> {
    let open = format!("<{name}");
    let mut at = from;
    while let Some(i) = text.get(at..)?.find('<') {
        let start = at + i;
        let rest = &text[start..];
        if rest.starts_with("<!--") {
            at = start + rest.find("-->").map_or(rest.len(), |e| e + 3);
            continue;
        }
        let end = start + rest.find('>')? + 1;
        let is = rest.starts_with(&open)
            && rest[open.len()..].starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>');
        if is {
            return Some(start..end);
        }
        at = end;
    }
    None
}

/// Where the body that starts at `from` ends: the first `</entry>` not inside a CDATA section
/// (`<![CDATA[ … ]]>`, which may hold one); `None` when there is none, or a section never ends
fn entry_end(text: &str, from: usize) -> Option<usize> {
    const OPEN: &str = "<![CDATA[";
    let mut at = from;
    loop {
        let rest = text.get(at..)?;
        let close = rest.find("</entry>")?;
        match rest.find(OPEN).filter(|&cdata| cdata < close) {
            Some(cdata) => {
                let inner = at + cdata + OPEN.len();
                at = inner + text.get(inner..)?.find("]]>")? + "]]>".len();
            }
            None => return Some(at + close),
        }
    }
}

/// A body that is CDATA sections and nothing else (whitespace around them allowed): their
/// text joined, so a GBF or plain body can hold `<`, `&`, and `</entry>` in a well-formed file
/// (`]]>` in the text is written as two sections, `]]]]><![CDATA[>`). `None` for any other body,
/// which is read as written
fn cdata_text(body: &str) -> Option<String> {
    let mut rest = body.trim();
    if rest.is_empty() {
        return None;
    }
    let mut out = String::new();
    while !rest.is_empty() {
        let inner = rest.strip_prefix("<![CDATA[")?;
        let end = inner.find("]]>")?;
        out.push_str(&inner[..end]);
        rest = &inner[end + "]]>".len()..];
    }
    Some(out)
}

/// An attribute's value in a tag, decoded (`&amp;`, `&#x3b1;`), in either quote
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = tag.get(from..)?.find(name) {
        let at = from + i;
        from = at + name.len();
        let preceded = tag[..at].ends_with(char::is_whitespace);
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        if !preceded {
            continue;
        }
        let rest = rest.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &rest[1..];
        let value = &value[..value.find(quote)?];
        return Some(decode_entities(value));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        files::MemoryFiles,
        hash::{HashRule, verse_text},
        source::open,
    };

    const GREEK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<entries keys="strongs-greek" markup="tei">
<!-- a comment -->
<entry key="G0025" headword="ἀγαπάω"><entryFree n="25"><orth>ἀγαπάω</orth> to love &amp; cherish<note>a note</note></entryFree></entry>
<entry key='G26' headword='ἀγάπη'>love</entry>
<entry key="G25a" link="G0025"/>
<entry key="G27" link="G9999"/>
</entries>
"#;

    #[test]
    fn entries_by_key() {
        let files = Arc::new(MemoryFiles::with([
            ("entries/greek.xml", GREEK),
            ("entries/notes.md", "not entries"),
        ]));
        let source = open(files, TextFormat::Dictionary, "entries/").unwrap();
        assert!(source.books().is_empty());
        assert_eq!(source.keys(), ["G25", "G26", "G25a", "G27"]);
        let text = |key: &str| {
            (source.entry(key).unwrap())
                .map(|e| (e.key, e.headword, verse_text(&e.nodes, HashRule::CURRENT)))
        };
        // Found by any form of the key; notes left out of the hashed text
        assert_eq!(
            text("g0025"),
            Some((
                "G25".into(),
                Some("ἀγαπάω".into()),
                "ἀγαπάω to love & cherish".into()
            ))
        );
        assert_eq!(text("G26").map(|t| t.2), Some("love".into()));
        // An alias reads as its target; one whose target is missing finds nothing
        assert_eq!(
            text("G25a"),
            Some((
                "G25a".into(),
                Some("ἀγαπάω".into()),
                "ἀγαπάω to love & cherish".into()
            ))
        );
        assert_eq!(text("G27"), None);
        assert_eq!(text("G28"), None);
        assert!(source.cross_references("John.3.16").unwrap().is_empty());
    }

    #[test]
    fn headwords_match_loosely() {
        let eastons = r#"<entries keys="headword" markup="thml">
<entry key="Aaron">brother of <i>Moses</i></entry>
<entry key="Ἰησοῦς">Jesus</entry>
<entry key="MARY">the mother</entry>
<entry key="Mary">of Bethany</entry>
</entries>"#;
        let plain =
            "<entries markup=\"plain\"><entry key=\"a\">one &amp; <b>two</b></entry></entries>";
        let files = Arc::new(MemoryFiles::with([
            ("d/a.xml", eastons),
            ("d/b.xml", plain),
        ]));
        let source = DictionarySource::open(files, "d").unwrap();
        assert_eq!(source.len(), 5);
        let text = |key: &str| {
            (source.entry(key).unwrap()).map(|e| verse_text(&e.nodes, HashRule::CURRENT))
        };
        assert_eq!(text("aaron"), Some("brother of Moses".into()));
        assert_eq!(text("ιησους"), Some("Jesus".into()));
        // Exact first, then the first loose match
        assert_eq!(text("Mary"), Some("of Bethany".into()));
        assert_eq!(text("MARY"), Some("the mother".into()));
        assert_eq!(text("mary"), Some("the mother".into()));
        // Plain text decodes nothing
        assert_eq!(text("a"), Some("one &amp; <b>two</b>".into()));
    }

    /// A body that isn't XML is inside CDATA, so the file is well-formed whatever it holds;
    /// what's read, and so hashed, is the text inside, the same as a bare body's
    #[test]
    fn cdata_bodies_read_as_their_text() {
        let bare = r#"<entries markup="plain">
<entry key="a">one &amp; <b>two</b></entry>
<entry key="b">ends here</entry>
<entry key="c">x</entry>
</entries>"#;
        let wrapped = r#"<entries markup="plain">
<entry key="a">
  <![CDATA[one &amp; <b>two</b>]]>
</entry>
<entry key="b"><![CDATA[ends ]]]]><![CDATA[> here </entry> too]]></entry>
<entry key="c"><![CDATA[x]]></entry>
<entry key="d"><![CDATA[not only]]> cdata</entry>
<entry key="e"><![CDATA[]]></entry>
</entries>"#;
        let gbf = r#"<entries markup="gbf">
<entry key="g"><![CDATA[<FI>words<Fi> & <RF>a note<Rf> more]]></entry>
</entries>"#;
        let files = Arc::new(MemoryFiles::with([
            ("d/a.xml", bare),
            ("e/a.xml", wrapped),
            ("f/a.xml", gbf),
        ]));
        let text = |dir: &str, key: &str| {
            let source = DictionarySource::open(files.clone(), dir).unwrap();
            (source.entry(key).unwrap()).map(|e| verse_text(&e.nodes, HashRule::CURRENT))
        };
        assert_eq!(text("e", "a"), text("d", "a"));
        assert_eq!(text("e", "a"), Some("one &amp; <b>two</b>".into()));
        // `]]>` split over two sections, and `</entry>` inside, are the text
        assert_eq!(text("e", "b"), Some("ends ]]> here </entry> too".into()));
        assert_eq!(text("e", "c"), text("d", "c"));
        // A body that isn't CDATA alone is read as written
        assert_eq!(text("e", "d"), Some("<![CDATA[not only]]> cdata".into()));
        assert_eq!(text("e", "e"), Some("".into()));
        assert_eq!(DictionarySource::open(files.clone(), "e").unwrap().len(), 5);
        assert_eq!(text("f", "g"), Some("words & more".into()));
        // A section that never ends leaves the entry without its end
        let unended = "<entries markup=\"plain\"><entry key=\"x\"><![CDATA[a</entry></entries>";
        let files = Arc::new(MemoryFiles::with([("d/a.xml", unended)]));
        let error = DictionarySource::open(files, "d")
            .err()
            .expect("refused")
            .to_string();
        assert!(error.contains("no </entry>"), "{error}");
        assert_eq!(cdata_text("  "), None);
        assert_eq!(entry_end("<![CDATA[</entry>]]></entry>", 0), Some(20));
    }

    #[test]
    fn mistakes_are_named() {
        let error = |files: &[(&str, &str)]| {
            let files = Arc::new(MemoryFiles::with(files.to_vec()));
            DictionarySource::open(files, "d")
                .err()
                .expect("refused")
                .to_string()
        };
        assert!(error(&[("d/a.xml", "<entry key=\"x\"/>")]).contains("no <entries> root"));
        assert!(error(&[("d/a.xml", "<entries><entry key=\"x\"/></entries>")]).contains("markup"));
        assert!(error(&[("d/a.xml", "<entries markup=\"docbook\"/>")]).contains("docbook"));
        assert!(
            error(&[(
                "d/a.xml",
                "<entries markup=\"tei\"><entry headword=\"x\"/></entries>"
            )])
            .contains("without a key")
        );
        assert!(
            error(&[(
                "d/a.xml",
                "<entries markup=\"tei\"><entry key=\"x\">no end</entries>"
            )])
            .contains("no </entry>")
        );
        let twice = error(&[
            (
                "d/a.xml",
                "<entries markup=\"tei\"><entry key=\"G0025\"/></entries>",
            ),
            (
                "d/b.xml",
                "<entries markup=\"tei\"><entry key=\"g25\"/></entries>",
            ),
        ]);
        assert!(
            twice.contains("d/b.xml: key `G25` is also in d/a.xml"),
            "{twice}"
        );
        assert_eq!(
            attribute("<entry key=\"a &amp; b\" x='y'>", "key").as_deref(),
            Some("a & b")
        );
        assert_eq!(attribute("<entry headword-key=\"z\">", "key"), None);
        assert_eq!(find_tag("<entryFree/><entry/>", 0, "entry"), Some(12..20));
    }
}
