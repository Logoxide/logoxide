/*!
CCEL (the Christian Classics Ethereal Library): its page of Bibles ([`catalog`]), its genre
indexes ([`genre_catalog`]: commentaries, Bibles, and any other genre's books), and a work's ThML
([`import`]), the whole work in one file ([`url`]), described by its own head (title, language,
rights, version, and whether it's a Bible, a commentary, or a book: [`thml_head`]), with its
rights in `COPYRIGHT.txt`. Its lists give no versions: an update is found by downloading.

A Bible is imported as `ccel:<id>` (`ccel:kjv`, its file `ccel/bible/kjv.xml`); any other work as
`ccel:<author>/<work>` (`ccel:henry/mhc1`, `ccel/henry/mhc1.xml`), which needn't be in a catalog
([`named_catalog`]): its head says what it is
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};

use crate::{Made, external, finish, import_id, package_id};

/// Where CCEL serves a work's ThML: a Bible's by its id (`kjv`), another work's by its author and
/// name (`henry/mhc1`), as its import names it
pub fn url(id: &str) -> String {
    match id.contains('/') {
        true => format!("https://ccel.org/ccel/{id}.xml"),
        false => format!("https://ccel.org/ccel/bible/{id}.xml"),
    }
}

/// A work's page on CCEL, from its import id (as [`url`])
fn homepage(id: &str) -> String {
    match id.contains('/') {
        true => format!("https://ccel.org/ccel/{id}"),
        false => format!("https://ccel.org/ccel/bible/{id}"),
    }
}

/// Whether a name is one CCEL's paths use for an author or a work (`henry`, `mhc1`,
/// `daily_light`)
fn is_path_name(name: &str) -> bool {
    !name.is_empty()
        && (name.bytes())
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b))
        && !name.starts_with('.')
}

/**
The entry for a work's import: `id` is a Bible's (`kjv`, CCEL's `bible` author left out) or
`<author>/<work>`; the package's id is `ccel-<id>` with `/` as `-` (`ccel-henry-mhc1`), prefixed
because CCEL's ids (`kjv`, `web`) are other catalogs' too, and the first of an id wins when
catalogs merge. Its language is `und` and its license unknown until its file's head says
([`import`])
*/
fn entry(id: &str, title: String, kind: PackageKind, description: Option<String>) -> CatalogEntry {
    let work = id.rsplit('/').next().unwrap_or(id);
    let downloads = vec![Download {
        name: String::from("thml"),
        urls: vec![url(id)],
    }];
    CatalogEntry {
        id: format!("ccel-{}", package_id(&id.replace('/', "-"))),
        name: title,
        abbreviation: work.to_uppercase(),
        aliases: vec![],
        language: String::from("und"),
        language_name: None,
        script: None,
        description,
        license: None,
        rights: Default::default(),
        copyright: None,
        version: None,
        kind,
        direction: Direction::Ltr,
        source: external("ccel", id, false, downloads),
        path: None,
        releases: vec![],
        keywords: vec![],
        categories: vec![],
        about_books: vec![],
        work: None,
        edition: None,
        edition_year: None,
        keys: None,
    }
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
        let title = link_text(heading).unwrap_or_else(|| work.to_string());
        if (entries.iter()).any(|e| e.source.import_id() == Some(work)) {
            continue;
        }
        entries.push(entry(work, title, PackageKind::Bible, None));
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

/// The text of the first link in `html` (`<a href="…">Title</a>`), its entities decoded and its
/// whitespace collapsed
fn link_text(html: &str) -> Option<String> {
    let (_, rest) = html.split_once("<a")?;
    let (_, rest) = rest.split_once('>')?;
    let (title, _) = rest.split_once("</a>")?;
    let title = decode_html_entities(title);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (!title.is_empty()).then_some(title)
}

/// The kind of package a CCEL genre's works are (`gen-COM` commentaries, `gen-BIB` Bibles), by
/// its code; every other genre's are books
pub fn genre_kind(genre: &str) -> PackageKind {
    match genre.to_ascii_uppercase().as_str() {
        "GEN-COM" => PackageKind::Commentary,
        "GEN-BIB" => PackageKind::Bible,
        _ => PackageKind::Book,
    }
}

/// The genre a CCEL genre index's URL names (`gen-COM` of `https://ccel.org/index/genre/gen-COM`)
pub fn genre_of(location: &str) -> Option<&str> {
    let path = location.split(['?', '#']).next().unwrap_or(location);
    let lower = path.to_ascii_lowercase();
    let at = lower.find("ccel.org/index/genre/")? + "ccel.org/index/genre/".len();
    let genre = path.get(at..)?.trim_end_matches('/');
    (!genre.is_empty() && !genre.contains('/')).then_some(genre)
}

/**
A CCEL genre index (`https://ccel.org/index/genre/gen-COM`, "All Works in Genre
'Commentaries'"): each `<p class="List2"><a href="/ccel/<author>/<work>.html">Title</a>` a
`ccel:<author>/<work>` import (a Bible's, `/ccel/bible/<id>.html`, is `ccel:<id>`, as
[`catalog`] lists it) of the kind the genre's works are ([`genre_kind`]); the author's name, when
the page gives it, is its description. Links of other shapes (a work's folder, a page of its
own) are passed over
*/
pub fn genre_catalog(text: &str, genre: &str) -> Result<Catalog> {
    let kind = genre_kind(genre);
    let mut entries: Vec<CatalogEntry> = vec![];
    for item in text.split("<p class=\"List2\">").skip(1) {
        let item = item.split_once("</p>").map_or(item, |(item, _)| item);
        let Some(href) = (item.split_once("href=\""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(href, _)| href)
        else {
            continue;
        };
        let Some(path) = (href.strip_prefix("https://ccel.org").or(Some(href)))
            .and_then(|h| h.strip_prefix("/ccel/"))
            .and_then(|h| h.strip_suffix(".html"))
        else {
            continue;
        };
        let Some((author, work)) = path.split_once('/') else {
            continue;
        };
        if !is_path_name(author) || !is_path_name(work) {
            continue;
        }
        let id = match author {
            "bible" => work.to_string(),
            _ => format!("{author}/{work}"),
        };
        if (entries.iter()).any(|e| e.source.import_id() == Some(id.as_str())) {
            continue;
        }
        let title = link_text(item).unwrap_or_else(|| work.to_string());
        let author =
            (item.split_once("class=\"authorName\"")).and_then(|(_, rest)| link_text(rest));
        entries.push(entry(&id, title, kind, author));
    }
    if entries.is_empty() {
        return Err(LogoxideError::Catalog(format!(
            "CCEL's genre {genre} lists no works (has its page changed?)"
        )));
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    logoxide::catalog::known_works(&mut entries);
    Ok(Catalog {
        name: format!("CCEL {genre}"),
        entries,
        warnings: vec![],
    })
}

/**
A catalog of the one work a `ccel:<author>/<work>` import names (`ccel:henry/mhc1`), for one no
catalog lists: CCEL serves every work's ThML the same way, so the name says where it is. It's a
book until its file's head says otherwise ([`import`]); `None` for any other name
*/
pub fn named_catalog(import: &str) -> Option<Catalog> {
    let id = import.strip_prefix("ccel:")?;
    let (author, work) = id.split_once('/')?;
    if !is_path_name(author) || !is_path_name(work) {
        return None;
    }
    let entry = match author {
        "bible" => entry(work, work.to_string(), PackageKind::Bible, None),
        _ => entry(id, work.to_string(), PackageKind::Book, None),
    };
    Some(Catalog {
        name: String::from("CCEL"),
        entries: vec![entry],
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
    /// `DC.Type`: what the work is (`Text.Bible`, `Text.Commentary`, `Text.Monograph`)
    pub dc_type: Option<String>,
}

impl ThmlHead {
    /// The kind of package its `DC.Type` says it is: a Bible, a commentary, or (any other type) a
    /// book; none when the head doesn't say
    pub fn kind(&self) -> Option<PackageKind> {
        let dc_type = self.dc_type.as_deref()?.to_ascii_lowercase();
        Some(match dc_type.as_str() {
            "text.bible" => PackageKind::Bible,
            "text.commentary" => PackageKind::Commentary,
            _ => PackageKind::Book,
        })
    }
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
        dc_type: text("DC.Type", false),
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
A work CCEL publishes, from its ThML (the bytes at [`url`]), as a package in `dest`:
`logoxide.toml`, `thml/<work>.xml`, and `COPYRIGHT.txt` (its rights, the license file). The file's
head names it and gives its language and rights (CCEL's list doesn't), its kind (`DC.Type`: a
Bible, a commentary, else a book; else the entry's), and the upstream version: the edition's own
(`1.0`), else its date (`2002-11-30`). Public domain is `LicenseRef-PublicDomain`; anything else
is `LicenseRef-CCEL`, with the statement in the license file. A page in place of the file (CCEL
answers with one for works it restricts) is an error, and so is a Bible or commentary with no
verse markers (CCEL's scans, "Images Only", have none)
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
    if let Some(kind) = head.kind() {
        entry.kind = kind;
    }
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
    let work = id.rsplit('/').next().unwrap_or(id);
    staged.write(&format!("thml/{work}.xml"), xml)?;
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
        homepage: homepage(id),
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
        crate::tests::build_twice(entry, &[("thml".into(), xml.as_bytes().to_vec())], &dest);
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

    /// CCEL's genre indexes, saved 2026-10-10 (`tests/fixtures/ccel/`)
    const GEN_COM: &str = include_str!("../tests/fixtures/ccel/gen-COM.html");
    const GEN_BIB: &str = include_str!("../tests/fixtures/ccel/gen-BIB.html");
    /// Matthew Henry's Genesis 1–2, cut short (logoxide-text's fixture)
    const HENRY: &str = include_str!("../../logoxide-text/tests/fixtures/ccel/mhc1-gen1-2.xml");

    #[test]
    fn genre_indexes() {
        let location = "https://ccel.org/index/genre/gen-COM";
        let catalog = crate::catalog::parse_text(location, GEN_COM).unwrap();
        assert_eq!(catalog.name, "CCEL gen-COM");
        // 213 links; a work's folder (`hebrewot/home.html`) and one with no `.html` aren't
        // imports
        assert!(catalog.entries.len() > 200, "{}", catalog.entries.len());
        assert!(
            catalog
                .entries
                .iter()
                .all(|e| e.kind == PackageKind::Commentary)
        );
        let henry = catalog.find_import("ccel:henry/mhc1").unwrap();
        assert_eq!(henry.id, "ccel-henry-mhc1");
        assert_eq!(henry.abbreviation, "MHC1");
        assert_eq!(henry.description.as_deref(), Some("Matthew Henry"));
        assert_eq!(
            henry.source.downloads()[0].urls,
            ["https://ccel.org/ccel/henry/mhc1.xml"]
        );
        let barnes = catalog.find_import("ccel:barnes/ntnotes").unwrap();
        assert_eq!(barnes.name, "Barnes' New Testament Notes");
        assert!(catalog.find_import("ccel:brenton/lxx").is_none());
        assert!(catalog.find_import("ccel:anonymous/hebrewot").is_none());
        // A Bible keeps its import name in the genre of Bibles
        let catalog =
            crate::catalog::parse_text("https://ccel.org/index/genre/gen-BIB", GEN_BIB).unwrap();
        let kjv = catalog.find_import("ccel:kjv").unwrap();
        assert_eq!(
            (kjv.id.as_str(), kjv.kind),
            ("ccel-kjv", PackageKind::Bible)
        );
        assert_eq!(
            kjv.source.downloads()[0].urls,
            ["https://ccel.org/ccel/bible/kjv.xml"]
        );
        assert_eq!(genre_kind("gen-THE"), PackageKind::Book);
    }

    #[test]
    fn imports_a_commentary_by_name() {
        let catalog = named_catalog("ccel:henry/mhc1").unwrap();
        let entry = catalog.find_import("ccel:henry/mhc1").unwrap();
        assert_eq!(entry.kind, PackageKind::Book);
        assert!(named_catalog("ccel:kjv").is_none());
        assert!(named_catalog("ccel:../x").is_none());
        let dest = MemoryStorage::new();
        crate::tests::build_twice(entry, &[("thml".into(), HENRY.as_bytes().to_vec())], &dest);
        assert_eq!(
            dest.walk("").unwrap(),
            ["COPYRIGHT.txt", "logoxide.toml", "thml/mhc1.xml"]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        // Its head says it's a commentary
        assert_eq!(info.kind, PackageKind::Commentary);
        assert_eq!(info.id, "ccel-henry-mhc1");
        assert_eq!(
            info.homepage.as_deref(),
            Some("https://ccel.org/ccel/henry/mhc1")
        );
        let genesis = logoxide_text::bible_data()
            .books()
            .search_usfm("GEN")
            .unwrap();
        assert_eq!(package.books(), [genesis]);
        let passage = (logoxide_text::bible_data().books())
            .parse_osis("Gen.1.1")
            .unwrap();
        let notes = package.source().notes(&passage).unwrap();
        assert_eq!(
            notes.iter().map(|n| (n.verse, n.last)).collect::<Vec<_>>(),
            [(1, 2)]
        );
        // A scan (no verse markers) isn't a commentary
        let scan = HENRY.replace("scripCom", "span");
        let error = import(entry, scan.as_bytes(), &MemoryStorage::new()).unwrap_err();
        assert!(error.to_string().contains("no books"), "{error}");
    }

    #[test]
    fn imports_a_book() {
        let catalog = named_catalog("ccel:calvin/institutes").unwrap();
        let entry = catalog.find_import("ccel:calvin/institutes").unwrap();
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ThML><ThML.head><electronicEdInfo><DC>
<DC.Title>The Institutes of the Christian Religion</DC.Title>
<DC.Type>Text.Monograph</DC.Type><DC.Language scheme="ISO639-3">eng</DC.Language>
<DC.Rights>Public Domain</DC.Rights></DC></electronicEdInfo></ThML.head>
<ThML.body><div1 title="Book First"><div2 title="Chapter 1.">
<p>Without vision the people perish (<scripRef passage="Prov. 29:18" parsed="|Prov|29|18|0|0">Prov. 29:18</scripRef>).</p>
</div2></div1></ThML.body></ThML>"#;
        let dest = MemoryStorage::new();
        crate::tests::build_twice(entry, &[("thml".into(), xml.as_bytes().to_vec())], &dest);
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        assert_eq!(package.info().kind, PackageKind::Book);
        assert_eq!(
            package.info().name,
            "The Institutes of the Christian Religion"
        );
        // Not read as verses: its references are found in it, as an EPUB's are
        assert!(package.books().is_empty() && package.text_format().is_none());
        assert_eq!(
            logoxide::references::book_files(&package).unwrap(),
            ["thml/institutes.xml"]
        );
        let found = logoxide::references::find(&package).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].passage.as_str(), found[0].chapter.as_deref()),
            ("Prov.29.18", Some("Chapter 1."))
        );
        let titles = logoxide::titles::titles(&package).unwrap();
        assert_eq!(
            titles[0].title.as_deref(),
            Some("The Institutes of the Christian Religion")
        );
    }
}
