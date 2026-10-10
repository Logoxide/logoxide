/*!
CCEL (the Christian Classics Ethereal Library): its page of Bibles ([`catalog`]) and a Bible's
ThML ([`import`]), the whole Bible in one file ([`url`]), described by its own head (title,
language, rights, version: [`thml_head`]), with its rights in `COPYRIGHT.txt`. Its list gives no
versions: an update is found by downloading
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};

use crate::{Made, external, finish, import_id, package_id};

/// Where CCEL serves a Bible's ThML (`kjv`, as its catalog lists it)
pub fn url(id: &str) -> String {
    format!("https://ccel.org/ccel/bible/{id}.xml")
}

/**
CCEL's page of its Bibles ([`logoxide::catalog::CCEL_CATALOG`]): each work's id and title, from
the links in its headings (`<h2><a href="https://ccel.org/ccel/bible/kjv/kjv">King James Version
of the Holy Bible</a></h2>`), each a `ccel:<id>` import of its ThML. The page doesn't say a
work's language or rights (its file's head does, [`import`]): the language is `und` until then,
and the license unknown
*/
pub fn catalog(text: &str) -> Result<Catalog> {
    let mut entries: Vec<CatalogEntry> = vec![];
    for heading in text.split("<h2").skip(1) {
        let Some((heading, _)) = heading.split_once("</h2>") else {
            continue;
        };
        let Some(href) = (heading.split_once("href=\""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(href, _)| href)
        else {
            continue;
        };
        // `…/ccel/bible/<work>/<work>`: a work of the author `bible`
        let mut parts = href.trim_end_matches('/').rsplit('/');
        let (Some(_), Some(work), Some("bible")) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let title = (heading.split_once('>'))
            .and_then(|(_, rest)| rest.split_once("<a"))
            .map(|(_, rest)| rest)
            .and_then(|rest| rest.split_once('>'))
            .and_then(|(_, rest)| rest.split_once("</a>"))
            .map(|(title, _)| decode_html_entities(title))
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| work.to_string());
        if (entries.iter()).any(|e| e.source.import_id() == Some(work)) {
            continue;
        }
        let downloads = vec![Download {
            name: String::from("thml"),
            urls: vec![url(work)],
        }];
        entries.push(CatalogEntry {
            // Prefixed: CCEL's ids (`kjv`, `web`) are other catalogs' too, and the first of
            // an id wins when catalogs merge
            id: format!("ccel-{}", package_id(work)),
            name: title,
            abbreviation: work.to_uppercase(),
            aliases: vec![],
            language: String::from("und"),
            language_name: None,
            script: None,
            description: None,
            license: None,
            rights: Default::default(),
            copyright: None,
            version: None,
            kind: PackageKind::Bible,
            direction: Direction::Ltr,
            source: external("ccel", work, false, downloads),
            path: None,
            releases: vec![],
            keywords: vec![],
            categories: vec![],
            about_books: vec![],
            work: None,
            edition: None,
            edition_year: None,
            keys: None,
        });
    }
    if entries.is_empty() {
        return Err(LogoxideError::Catalog(String::from(
            "CCEL's page lists no Bibles (has it changed?)",
        )));
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    logoxide::catalog::known_works(&mut entries);
    Ok(Catalog {
        name: String::from("CCEL"),
        entries,
        warnings: vec![],
    })
}

/// The entities a title on CCEL's page may hold (the named ones HTML gives a Bible's name, and
/// numeric ones), decoded
fn decode_html_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let Some(semi) = after.find(';').filter(|&i| i <= 10) else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let name = &after[1..semi];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ => (name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&after[..=semi]),
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

/// What a ThML file's head (`<ThML.head>`) says about it
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThmlHead {
    /// `DC.Title` (the first, which isn't a short form)
    pub title: Option<String>,
    /// `DC.Language`, as ISO 639-3 (`eng`; the package says `en`)
    pub language: Option<String>,
    /// `DC.Rights`: `Public Domain`, or a copyright statement
    pub rights: Option<String>,
    /// The edition's own version (`<version>1.0</version>`)
    pub version: Option<String>,
    /// `DC.Date`, when it was made (`2002-11-30`)
    pub date: Option<String>,
    /// `<description>`, its lines joined
    pub description: Option<String>,
}

/// The head of a ThML file (only the part before `</ThML.head>` is read)
pub fn thml_head(xml: &str) -> ThmlHead {
    let end = xml.find("</ThML.head>").unwrap_or(xml.len().min(64 * 1024));
    let head = xml.get(..end).unwrap_or_default();
    let found = logoxide_text::metadata::elements(head);
    let text = |name: &str, plain: bool| {
        (found.iter())
            .filter(|e| e.name == name && (!plain || e.attr("sub").is_none()))
            .map(|e| e.text.split_whitespace().collect::<Vec<_>>().join(" "))
            .find(|t| !t.is_empty())
    };
    ThmlHead {
        title: text("DC.Title", true),
        language: text("DC.Language", false),
        rights: text("DC.Rights", false),
        version: text("version", false),
        date: text("DC.Date", false),
        description: inner_text(head, "description"),
    }
}

/// The text inside the first `<name>…</name>` of `xml`, with any elements in it left out (a
/// description can hold links), its whitespace collapsed
fn inner_text(xml: &str, name: &str) -> Option<String> {
    let (_, rest) = xml.split_once(&format!("<{name}>"))?;
    let (inside, _) = rest.split_once(&format!("</{name}>"))?;
    let mut text = String::new();
    let mut in_tag = false;
    for c in inside.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = logoxide_text::markup::decode_entities(&text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/**
A Bible CCEL publishes, from its ThML (the bytes at [`url`]), as a package in `dest`:
`logoxide.toml`, `thml/<id>.xml`, and `COPYRIGHT.txt` (its rights, the license file). The file's
head names it and gives its language and rights (CCEL's list doesn't), and the upstream version:
the edition's own (`1.0`), else its date (`2002-11-30`). Public domain is `LicenseRef-PublicDomain`;
anything else is `LicenseRef-CCEL`, with the statement in the license file. A page in place of
the file (CCEL answers with one for works it restricts) is an error
*/
pub fn import(entry: &CatalogEntry, xml: &[u8], dest: &dyn Storage) -> Result<()> {
    let id = import_id(entry, "ccel", "CCEL")?;
    let text = String::from_utf8_lossy(xml);
    let start = text.get(..text.len().min(4096)).unwrap_or_default();
    if !start.contains("<ThML") {
        return Err(LogoxideError::Import(format!(
            "{id}: {} isn't ThML (CCEL may need a login for it)",
            url(id)
        )));
    }
    let head = thml_head(&text);
    let mut entry = entry.clone();
    if let Some(title) = head.title
        && title != entry.name
    {
        entry
            .aliases
            .push(std::mem::replace(&mut entry.name, title));
    }
    if let Some(language) = head.language {
        let (tag, raw) = language::normalize_language(&language);
        entry.language = tag;
        entry.language_name = raw.or(entry.language_name);
    }
    if head.description.is_some() {
        entry.description = head.description;
    }
    // CCEL doesn't say the script, and its Hebrew transliteration is tagged as Hebrew too, so
    // the language can't tell: the letters do
    let body = (text.find("</ThML.head>")).map_or(&*text, |at| &text[at..]);
    let script = logoxide_text::script::dominant_script(body);
    entry.script = script.map(String::from);
    entry.direction = language::script_direction(script);
    let public = (head.rights.as_deref()).is_some_and(crate::license::says_public_domain);
    entry.license = Some(String::from(match public {
        true => "LicenseRef-PublicDomain",
        false => "LicenseRef-CCEL",
    }));
    entry.version = head.version.or(head.date).or(entry.version);
    let staged = MemoryStorage::new();
    staged.write(&format!("thml/{id}.xml"), xml)?;
    let rights = head
        .rights
        .as_deref()
        .unwrap_or("No rights statement given");
    let notice = format!(
        "{rights}\n\nFrom the Christian Classics Ethereal Library: {}\n",
        url(id)
    );
    staged.write("COPYRIGHT.txt", notice.as_bytes())?;
    let made = Made {
        format: "thml",
        license_file: Some(String::from("COPYRIGHT.txt")),
        publisher: "CCEL",
        homepage: format!("https://ccel.org/ccel/bible/{id}"),
        versification: None,
        keys: None,
    };
    finish(&entry, made, staged, dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use logoxide::{
        catalog::CCEL_CATALOG,
        package::{Features, Package},
    };

    #[test]
    fn imports_a_ccel_bible() {
        let page = r#"<div id="worksBy_44" class="worksEntry">
        <h2>
            <a href="https://ccel.org/ccel/bible/heot/heot">
                Holy Bible: Hebrew &amp; Old Testament</a>
        </h2></div>
        <h2><a href="https://ccel.org/ccel/calvin/institutes">Not a Bible</a></h2>"#;
        let catalog = crate::catalog::parse_text(CCEL_CATALOG, page).unwrap();
        assert_eq!(catalog.entries.len(), 1);
        let entry = catalog.find_import("ccel:heot").unwrap();
        assert_eq!(
            (entry.name.as_str(), entry.abbreviation.as_str()),
            ("Holy Bible: Hebrew & Old Testament", "HEOT")
        );
        assert_eq!(
            entry.source.downloads(),
            [Download {
                name: "thml".into(),
                urls: vec!["https://ccel.org/ccel/bible/heot.xml".into()]
            }]
        );
        assert!(!entry.source.versions_track_updates());
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE ThML PUBLIC "-//CCEL/DTD Theological Markup Language//EN" "http://www.ccel.org/dtd/ThML10.dtd">
<ThML><ThML.head><generalInfo><description>
  The Hebrew
  Old Testament (see <a href="https://ccel.org">ccel.org</a>).</description></generalInfo>
<electronicEdInfo><version>1.1</version><DC>
<DC.Title>The Holy Bible: Hebrew Old Testament</DC.Title><DC.Title sub="short">Hebrew Bible</DC.Title>
<DC.Date sub="Created">2002-12-31</DC.Date><DC.Language scheme="ISO639-3">heb</DC.Language>
<DC.Rights>Public Domain</DC.Rights></DC></electronicEdInfo></ThML.head>
<ThML.body><div1 title="Obadiah"><p><scripture passage="Obad 1.1" parsed="|Obad|1|1|0|0" osisRef="Bible:Obad.1.1" />
חֲזוֹן עֹבַדְיָה כֹּה־אָמַר אֲדֹנָי יְהוִה לֶאֱדוֹם</p></div1></ThML.body></ThML>"#;
        let dest = MemoryStorage::new();
        crate::build(entry, &[("thml".into(), xml.as_bytes().to_vec())], &dest).unwrap();
        assert_eq!(
            dest.walk("").unwrap(),
            ["COPYRIGHT.txt", "logoxide.toml", "thml/heot.xml"]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(info.name, "The Holy Bible: Hebrew Old Testament");
        assert_eq!(info.aliases, ["Holy Bible: Hebrew & Old Testament"]);
        assert_eq!(
            (info.language.as_str(), info.version.to_string()),
            ("he", "1.0.0".to_string())
        );
        let upstream = info.upstream.as_ref().unwrap();
        assert_eq!(
            (
                upstream.import.as_deref(),
                upstream.url.as_deref(),
                upstream.version.as_deref()
            ),
            (
                Some("ccel:heot"),
                Some("https://ccel.org/ccel/bible/heot.xml"),
                Some("1.1")
            )
        );
        assert_eq!(info.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(
            info.description.as_deref(),
            Some("The Hebrew Old Testament (see ccel.org).")
        );
        // Its letters are Hebrew: the script says it runs right to left, and only it is written
        assert_eq!(info.script.as_deref(), Some("Hebr"));
        assert_eq!(info.direction, None);
        assert_eq!(info.text_direction(), Direction::Rtl);
        assert_eq!(package.books().len(), 1);

        // Without an edition version, its date; a copyright isn't public domain
        let dated = xml
            .replace("<version>1.1</version>", "")
            .replace("Public Domain", "Copyright 2010 Someone");
        let dest = MemoryStorage::new();
        import(entry, dated.as_bytes(), &dest).unwrap();
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let upstream = package.info().upstream.clone().unwrap();
        assert_eq!(upstream.version.as_deref(), Some("2002-12-31"));
        assert_eq!(package.info().license.as_deref(), Some("LicenseRef-CCEL"));
        // CCEL's page for a work it restricts
        let page = b"<!DOCTYPE html><html><body>Log in</body></html>";
        let error = import(entry, page, &MemoryStorage::new()).unwrap_err();
        assert!(error.to_string().contains("isn't ThML"), "{error}");
    }

    #[test]
    fn titles_entities_are_decoded() {
        assert_eq!(
            decode_html_entities("A &amp; B &#x26; C &#38; &nbsp;x"),
            "A & B & C & \u{a0}x"
        );
        assert_eq!(
            decode_html_entities("A & B &unknown; C"),
            "A & B &unknown; C"
        );
    }
}
