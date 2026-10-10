/*!
Making packages from other publishers' downloads, so a Bible that isn't a logoxide package can still
be installed: its files with a `logoxide.toml` around them, which a vault then uses as a path
package (or which can be pushed anywhere as a git repository).

- **eBible.org** ([`ebible`]): a translation's `<id>_usfm.zip`, one USFM file per book, with its
  copyright in `copr.htm` (the package's license file)
- **fetch.bible** ([`fetch_bible`]): a translation's USX, a file per book
  ([`fetch_bible_book_url`]), with its attribution in `COPYRIGHT.txt`
- **CCEL** ([`ccel`]): a Bible's ThML, the whole Bible in one file ([`ccel_url`]), described by
  its own head (title, language, rights, version), with its rights in `COPYRIGHT.txt`
- **SWORD** ([`sword`]): a module's zip from its repository ([`sword_zip_urls`]), read with
  sword-modules from memory: its entries in their own markup (OSIS or ThML), a file per book,
  with its conf's copyright and license in `COPYRIGHT.txt`

The catalog entry ([`crate::catalog`]) gives the package's name, language, license, and version.
*/

use std::io::Read;

use crate::{
    catalog::{CatalogEntry, CatalogSource},
    error::{LogoxideError, Result},
    manifest::{FormatEntry, MANIFEST, Manifest, Package as Info},
    package::{Features, Package},
    storage::{MemoryStorage, Storage},
};

/// Where eBible.org serves a translation's USFM
pub fn ebible_zip_url(id: &str) -> String {
    format!("https://ebible.org/Scriptures/{id}_usfm.zip")
}

/// Each file in a `.zip`, by path (folders left out)
pub fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let bad = |e: &dyn std::fmt::Display| LogoxideError::Import(format!("not a .zip: {e}"));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| bad(&e))?;
    let mut files = vec![];
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| bad(&e))?;
        if !file.is_file() {
            continue;
        }
        let Some(path) = file
            .enclosed_name()
            .and_then(|p| p.to_str().map(String::from))
        else {
            continue;
        };
        let mut data = vec![];
        file.read_to_end(&mut data).map_err(|e| bad(&e))?;
        files.push((path.replace('\\', "/"), data));
    }
    Ok(files)
}

/**
An eBible.org translation's `<id>_usfm.zip` (from [`ebible_zip_url`]) as a package in `dest`:
`logoxide.toml`, `usfm/` (every book's file), and `copr.htm` (its copyright, the license file). The
package's id, name, language, and license come from `entry`, its version from eBible.org's update
date. Checked by opening it: a zip without books is an error
*/
pub fn ebible(entry: &CatalogEntry, zip: &[u8], dest: &dyn Storage) -> Result<()> {
    let CatalogSource::Ebible { id } = &entry.source else {
        return Err(LogoxideError::Import(format!(
            "{} isn't from eBible.org",
            entry.id
        )));
    };
    let files = unzip(zip)?;
    let staged = MemoryStorage::new();
    let mut license_file = None;
    for (path, data) in files {
        let name = path.rsplit('/').next().unwrap_or(&path);
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".usfm") || lower.ends_with(".sfm") {
            staged.write(&format!("usfm/{name}"), &data)?;
        } else if lower == "copr.htm" {
            staged.write(name, &data)?;
            license_file = Some(name.to_string());
        }
    }
    let made = Made {
        format: "usfm",
        license_file,
        publisher: "eBible.org",
        homepage: format!("https://ebible.org/{id}/"),
        upstream: ebible_zip_url(id),
    };
    finish(entry, made, staged, dest)
}

/// Where fetch.bible serves a translation's book as USX (`code` as its catalog lists it: `jhn`)
pub fn fetch_bible_book_url(id: &str, code: &str) -> String {
    format!(
        "https://v1.fetch.bible/bibles/{id}/usx/{}.usx",
        code.to_ascii_lowercase()
    )
}

/**
A fetch.bible translation as a package in `dest`, from each of its books' USX (`books`: each
book's code and the bytes at [`fetch_bible_book_url`]): `logoxide.toml`, `usx/`, and
`COPYRIGHT.txt` (its attribution and license, the license file)
*/
pub fn fetch_bible(
    entry: &CatalogEntry,
    books: &[(String, Vec<u8>)],
    dest: &dyn Storage,
) -> Result<()> {
    let CatalogSource::FetchBible { id, .. } = &entry.source else {
        return Err(LogoxideError::Import(format!(
            "{} isn't from fetch.bible",
            entry.id
        )));
    };
    let staged = MemoryStorage::new();
    for (code, data) in books {
        if !data.starts_with(b"<?xml")
            && !data.starts_with(b"<usx")
            && !data.starts_with(b"\xef\xbb\xbf")
        {
            return Err(LogoxideError::Import(format!(
                "{id}: {code} isn't USX (was it downloaded from {}?)",
                fetch_bible_book_url(id, code)
            )));
        }
        staged.write(&format!("usx/{}.usx", code.to_ascii_uppercase()), data)?;
    }
    let mut license_file = None;
    let notice = [entry.copyright.as_deref(), entry.license.as_deref()];
    if notice.iter().any(Option::is_some) {
        let text: Vec<&str> = notice.into_iter().flatten().collect();
        staged.write("COPYRIGHT.txt", format!("{}\n", text.join("\n")).as_bytes())?;
        license_file = Some(String::from("COPYRIGHT.txt"));
    }
    let made = Made {
        format: "usx",
        license_file,
        publisher: "fetch.bible",
        homepage: String::from("https://fetch.bible/"),
        upstream: fetch_bible_url(id),
    };
    finish(entry, made, staged, dest)
}

/// Where CCEL serves a Bible's ThML (`kjv`, as its catalog lists it)
pub fn ccel_url(id: &str) -> String {
    format!("https://ccel.org/ccel/bible/{id}.xml")
}

/// What a ThML file's head (`<ThML.head>`) says about it
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThmlHead {
    /// `DC.Title` (the first, which isn't a short form)
    pub title: Option<String>,
    /// `DC.Language`, as ISO 639-3 (`eng`)
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
    let found = crate::create::elements(head);
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
    let text = topos_bible_markup::decode_entities(&text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/**
A Bible CCEL publishes, from its ThML (the bytes at [`ccel_url`]), as a package in `dest`:
`logoxide.toml`, `thml/<id>.xml`, and `COPYRIGHT.txt` (its rights, the license file). The file's
head names it and gives its language and rights (CCEL's list doesn't), and its version: the
edition's own (`1.0` is `1.0.0`), else its date. Public domain is `LicenseRef-PublicDomain`;
anything else is `LicenseRef-CCEL`, with the statement in the license file. A page in place of
the file (CCEL answers with one for works it restricts) is an error
*/
pub fn ccel(entry: &CatalogEntry, xml: &[u8], dest: &dyn Storage) -> Result<()> {
    let CatalogSource::Ccel { id } = &entry.source else {
        return Err(LogoxideError::Import(format!(
            "{} isn't from CCEL",
            entry.id
        )));
    };
    let text = String::from_utf8_lossy(xml);
    let start = text.get(..text.len().min(4096)).unwrap_or_default();
    if !start.contains("<ThML") {
        return Err(LogoxideError::Import(format!(
            "{id}: {} isn't ThML (CCEL may need a login for it)",
            ccel_url(id)
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
        entry.language = language;
    }
    if head.description.is_some() {
        entry.description = head.description;
    }
    let body = (text.find("</ThML.head>")).map_or(&*text, |at| &text[at..]);
    entry.direction = text_direction(body);
    let public =
        (head.rights.as_deref()).is_some_and(|r| r.to_ascii_lowercase().contains("public domain"));
    entry.license = Some(String::from(match public {
        true => "LicenseRef-PublicDomain",
        false => "LicenseRef-CCEL",
    }));
    entry.version = (head.version.as_deref())
        .and_then(crate::catalog::dotted_version)
        .or_else(|| head.date.as_deref().and_then(crate::catalog::date_version))
        .or(entry.version);
    let staged = MemoryStorage::new();
    staged.write(&format!("thml/{id}.xml"), xml)?;
    let rights = head
        .rights
        .as_deref()
        .unwrap_or("No rights statement given");
    let notice = format!(
        "{rights}\n\nFrom the Christian Classics Ethereal Library: {}\n",
        ccel_url(id)
    );
    staged.write("COPYRIGHT.txt", notice.as_bytes())?;
    let made = Made {
        format: "thml",
        license_file: Some(String::from("COPYRIGHT.txt")),
        publisher: "CCEL",
        homepage: format!("https://ccel.org/ccel/bible/{id}"),
        upstream: ccel_url(id),
    };
    finish(&entry, made, staged, dest)
}

/**
Which way a text's script runs, from its letters (outside tags): right to left when most are
Hebrew, Arabic, Syriac, or Thaana. CCEL doesn't say, and its Hebrew transliteration is tagged as
Hebrew too, so the language can't tell
*/
fn text_direction(text: &str) -> crate::manifest::Direction {
    let (mut rtl, mut ltr) = (0, 0);
    let mut in_tag = false;
    for c in text.chars().take(200_000) {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag || !c.is_alphabetic() => {}
            '\u{0590}'..='\u{08ff}' | '\u{fb1d}'..='\u{fdff}' | '\u{fe70}'..='\u{feff}' => rtl += 1,
            _ => ltr += 1,
        }
    }
    match rtl > ltr {
        true => crate::manifest::Direction::Rtl,
        false => crate::manifest::Direction::Ltr,
    }
}

/// Where a SWORD repository serves a module's zip: `packages/<module>.zip` at its root, else
/// with the name in lowercase (as some repositories name them)
pub fn sword_zip_urls(repository: &str, module: &str) -> Vec<String> {
    let root = repository.trim_end_matches('/');
    let mut urls = vec![format!("{root}/packages/{module}.zip")];
    if module.to_lowercase() != module {
        urls.push(format!("{root}/packages/{}.zip", module.to_lowercase()));
    }
    urls
}

/**
A SWORD module from its zip (as [`sword_zip_urls`] serve it), as a package in `dest`: each book
a file in the module's own markup (`osis/Gen.xml` for an OSIS module, `thml/Gen.xml` for ThML),
`logoxide.toml`, and `COPYRIGHT.txt` (the conf's copyright, license, and about text). The zip's
conf describes the package (its description, language, version, license, and direction); the
entry gives its id.

Each verse becomes a marker and its entry (`<verse osisID="John.3.16" sID="John.3.16"/>…<verse
eID="John.3.16"/>`, or ThML's `<scripture parsed="|John|3|16|0|0"/>`), in chapters, with a
book's and a chapter's introductions before their first verse. A verse whose entry is the one
before it again (SWORD links the verses of a range to one entry) makes that one a bridge
(`n="16-17"`); an introduction that is an earlier verse's entry is left out. A locked module, or
markup other than OSIS and ThML, is an error
*/
pub fn sword(entry: &CatalogEntry, zip: &[u8], dest: &dyn Storage) -> Result<()> {
    use sword_modules::{Markup, MemoryFiles, SwordLibrary};
    let CatalogSource::Sword {
        module, repository, ..
    } = &entry.source
    else {
        return Err(LogoxideError::Import(format!(
            "{} isn't from a SWORD repository",
            entry.id
        )));
    };
    let failed = |e: sword_modules::Error| LogoxideError::Import(format!("{module}: {e}"));
    let mut files = MemoryFiles::new();
    for (path, bytes) in unzip(zip)? {
        files.insert(path, bytes);
    }
    let library = SwordLibrary::open_files(std::sync::Arc::new(files)).map_err(failed)?;
    let conf = (library.conf(module).cloned())
        .ok_or_else(|| LogoxideError::Import(format!("the zip has no module {module}")))?;
    let opened = library.open_module(module).map_err(failed)?;
    let verses = opened.into_verse().map_err(failed)?;
    let (format, markup) = match verses.markup() {
        Markup::Osis => ("osis", Markup::Osis),
        Markup::Thml => ("thml", Markup::Thml),
        other => {
            return Err(LogoxideError::Import(format!(
                "{module}: its markup is {other:?}; logoxide reads OSIS and ThML"
            )));
        }
    };
    // The package as the zip's conf describes it (newer than the catalog's, perhaps)
    let described = crate::catalog::sword_entry(&conf, entry.kind, "", repository);
    let entry = CatalogEntry {
        id: entry.id.clone(),
        source: entry.source.clone(),
        path: entry.path.clone(),
        releases: vec![],
        keywords: vec![],
        categories: vec![],
        about_books: vec![],
        ..described
    };
    let mut books: Vec<(sword_modules::BookId, Vec<SwordVerse>)> = vec![];
    // Every verse's text so far: an introduction that is one of them points at that verse's
    // entry (the ISV's John 1:0 is Luke 23:56), and isn't an introduction
    let mut seen = std::collections::HashSet::new();
    for read in verses.iter() {
        let (key, text) = read.map_err(failed)?;
        let verse = SwordVerse {
            chapter: key.chapter,
            verse: key.verse,
            last: key.verse,
            text: match markup {
                Markup::Osis => without_structure(&text.raw),
                _ => text.raw,
            },
        };
        if verse.verse == 0 && seen.contains(&verse.text) {
            continue;
        }
        if verse.verse > 0 {
            seen.insert(verse.text.clone());
        }
        match books.last_mut() {
            Some((book, list)) if *book == key.book => {
                // A range's later verses link to its entry: one verse, bridged
                if let Some(before) = list.last_mut()
                    && verse.verse > 0
                    && before.verse > 0
                    && before.chapter == verse.chapter
                    && before.last + 1 == verse.verse
                    && before.text == verse.text
                    && !verse.text.trim().is_empty()
                {
                    before.last = verse.verse;
                    continue;
                }
                list.push(verse);
            }
            _ => books.push((key.book, vec![verse])),
        }
    }
    let staged = MemoryStorage::new();
    let language = conf.lang().unwrap_or("und");
    for (book, list) in &books {
        let Some(osis) = sword_modules::VerseKey::new(*book, 1, 1).osis_book() else {
            continue;
        };
        let text = match markup {
            Markup::Osis => osis_book_file(module, language, osis, list),
            _ => thml_book_file(osis, list),
        };
        staged.write(&format!("{format}/{osis}.xml"), text.as_bytes())?;
    }
    staged.write("COPYRIGHT.txt", sword_notice(&conf, repository).as_bytes())?;
    let made = Made {
        format,
        license_file: Some(String::from("COPYRIGHT.txt")),
        publisher: "SWORD",
        homepage: (conf.get("TextSource").filter(|t| t.contains("://")))
            .map_or_else(|| repository.clone(), String::from),
        upstream: sword_zip_urls(repository, module).swap_remove(0),
    };
    finish(&entry, made, staged, dest)
}

/**
An OSIS entry without the module's own book and chapter milestones (`<chapter sID="…"/>`,
`<div type="book" eID="…"/>`, and `bookGroup`s): the file marks its book and chapters itself.
Other milestones (a psalm's title before its first verse) stay
*/
fn without_structure(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let tag_end = rest[at..].find('>').map(|end| at + end + 1);
        let Some(end) = tag_end else {
            out.push_str(&rest[at..]);
            return out;
        };
        let tag = &rest[at..end];
        let empty = tag.ends_with("/>");
        let structural = tag.starts_with("<chapter ")
            || (tag.starts_with("<div ")
                && (tag.contains("type=\"book\"") || tag.contains("type=\"bookGroup\"")));
        if !(empty && structural) {
            out.push_str(tag);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// A verse of a SWORD module (verse 0 is a chapter's introduction; chapter 0 the book's)
struct SwordVerse {
    chapter: u8,
    verse: u8,
    /// The last verse a bridge covers (`verse` otherwise)
    last: u8,
    text: String,
}

/// A book of an OSIS module as an OSIS file: its entries between verse milestones, in chapters
fn osis_book_file(work: &str, language: &str, book: &str, verses: &[SwordVerse]) -> String {
    let attr = |text: &str| text.replace('&', "&amp;").replace('"', "&quot;");
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<osis xmlns=\"http://www.bibletechnologies.net/2003/OSIS/namespace\">\n<osisText osisIDWork=\"{}\" xml:lang=\"{}\">\n<div type=\"book\" osisID=\"{book}\">\n",
        attr(work),
        attr(language)
    );
    let mut chapter = 0;
    for v in verses {
        if v.chapter != chapter {
            if chapter > 0 {
                out.push_str(&format!("<chapter eID=\"{book}.{chapter}\"/>\n"));
            }
            chapter = v.chapter;
            out.push_str(&format!(
                "<chapter osisID=\"{book}.{chapter}\" sID=\"{book}.{chapter}\"/>\n"
            ));
        }
        if v.verse == 0 {
            out.push_str(&format!("{}\n", v.text));
            continue;
        }
        let id = format!("{book}.{}.{}", v.chapter, v.verse);
        let (ids, n) = match v.last > v.verse {
            true => (
                (v.verse..=v.last)
                    .map(|n| format!("{book}.{}.{n}", v.chapter))
                    .collect::<Vec<_>>()
                    .join(" "),
                format!("{}-{}", v.verse, v.last),
            ),
            false => (id.clone(), v.verse.to_string()),
        };
        out.push_str(&format!(
            "<verse osisID=\"{ids}\" sID=\"{id}\" n=\"{n}\"/>{}<verse eID=\"{id}\"/>\n",
            v.text
        ));
    }
    if chapter > 0 {
        out.push_str(&format!("<chapter eID=\"{book}.{chapter}\"/>\n"));
    }
    out.push_str("</div>\n</osisText>\n</osis>\n");
    out
}

/// A book of a ThML module as a ThML file: each verse marked as CCEL marks them
/// (`<scripture parsed="|John|3|16|0|0"/>`), in a `div2` for each chapter
fn thml_book_file(book: &str, verses: &[SwordVerse]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ThML>\n<ThML.head></ThML.head>\n<ThML.body>\n<div1 title=\"{book}\">\n"
    );
    let mut chapter = 0;
    for v in verses {
        if v.chapter != chapter {
            if chapter > 0 {
                out.push_str("</div2>\n");
            }
            chapter = v.chapter;
            out.push_str(&format!("<div2 title=\"{book} {chapter}\">\n"));
        }
        if v.verse == 0 {
            out.push_str(&format!("{}\n", v.text));
            continue;
        }
        let (c, first, last) = (v.chapter, v.verse, v.last);
        let (passage, parsed) = match last > first {
            true => (
                format!("{book} {c}.{first}-{last}"),
                format!("|{book}|{c}|{first}|{c}|{last}"),
            ),
            false => (
                format!("{book} {c}.{first}"),
                format!("|{book}|{c}|{first}|0|0"),
            ),
        };
        out.push_str(&format!(
            "<scripture passage=\"{passage}\" parsed=\"{parsed}\" osisRef=\"Bible:{book}.{c}.{first}\"/>{}\n",
            v.text
        ));
    }
    if chapter > 0 {
        out.push_str("</div2>\n");
    }
    out.push_str("</div1>\n</ThML.body>\n</ThML>\n");
    out
}

/// What a module's conf says of its copyright and license, for the package's license file
fn sword_notice(conf: &sword_modules::ModuleConf, repository: &str) -> String {
    let mut lines = vec![];
    for (key, label) in [
        ("ShortCopyright", "Copyright"),
        ("Copyright", "Copyright"),
        ("CopyrightHolder", "Copyright holder"),
        ("CopyrightDate", "Copyright date"),
        ("CopyrightNotes", "Notes"),
        ("DistributionLicense", "License"),
        ("DistributionNotes", "Distribution"),
        ("ShortPromo", "Promotion"),
    ] {
        if let Some(value) = conf.get(key).map(str::trim).filter(|v| !v.is_empty()) {
            let value = sword_modules::conf::rtf_to_text(value);
            if !lines.iter().any(|l: &String| l.ends_with(&value)) {
                lines.push(format!("{label}: {value}"));
            }
        }
    }
    if let Some(about) = conf.about_text().filter(|a| !a.trim().is_empty()) {
        lines.push(format!("\n{}", about.trim()));
    }
    lines.push(format!(
        "\nFrom the SWORD module {}, {repository}",
        conf.name()
    ));
    format!("{}\n", lines.join("\n"))
}

/// Where fetch.bible serves a translation (its books are below it)
fn fetch_bible_url(id: &str) -> String {
    format!("https://v1.fetch.bible/bibles/{id}/")
}

/**
What an import of `entry` writes as its package's `upstream` (the download it was made from), so
an imported package names the catalog entry it came from ([`Catalog::find_upstream`]); none for
packages that aren't imported

[`Catalog::find_upstream`]: crate::catalog::Catalog::find_upstream
*/
pub fn upstream(entry: &CatalogEntry) -> Option<String> {
    match &entry.source {
        CatalogSource::Ebible { id } => Some(ebible_zip_url(id)),
        CatalogSource::FetchBible { id, .. } => Some(fetch_bible_url(id)),
        CatalogSource::Ccel { id } => Some(ccel_url(id)),
        CatalogSource::Sword {
            repository, module, ..
        } => sword_zip_urls(repository, module).into_iter().next(),
        CatalogSource::Git { .. } | CatalogSource::Registry { .. } => None,
    }
}

/// What updating an imported package changes ([`compare_import`])
#[derive(Clone, Debug)]
pub struct ImportUpdate {
    pub from: semver::Version,
    pub to: semver::Version,
    /// Its verses' changes (reworded, added, removed, renumbered), and its titles'
    pub diff: crate::summary::Diff,
    /// The publisher's version isn't newer though the package changed: `to` is the next version
    /// the change calls for
    pub bumped: bool,
}

/**
How a package made again from its publisher's download (`fresh`, as [`ebible`] and the others
write it) differs from the one a project has (`current`): none when their declared files are the
same. The publisher's version is kept when it's newer. When it isn't (fetch.bible's and CCEL's
change without one), the fresh package's version is raised to the next one its change calls for
([`crate::summary::next_version`]), in its `logoxide.toml`, so one version never names two texts
(links record the version they were made in)
*/
pub fn compare_import(
    current: std::sync::Arc<dyn Storage>,
    fresh: &std::sync::Arc<MemoryStorage>,
) -> Result<Option<ImportUpdate>> {
    use crate::summary::{diff_summaries, next_version, summarize};
    let fresh_storage: std::sync::Arc<dyn Storage> = fresh.clone();
    // A version raised by an earlier update is newer than the publisher's: compared at it, so
    // the same text isn't raised again
    let version_of = |storage: &dyn Storage| -> Result<semver::Version> {
        let manifest = Manifest::parse(&storage.read_string(MANIFEST)?)?;
        (manifest.package.map(|p| p.version))
            .ok_or_else(|| LogoxideError::Import(String::from("not a package")))
    };
    let had = version_of(current.as_ref())?;
    if version_of(fresh.as_ref())? < had {
        set_version(fresh.as_ref(), &had)?;
    }
    if crate::package::checksum(current.as_ref())? == crate::package::checksum(fresh.as_ref())? {
        return Ok(None);
    }
    let old = Package::open(current, Features::default())?;
    let new = Package::open(fresh_storage, Features::default())?;
    let (old_summary, old_verses) = summarize(&old)?;
    let (new_summary, new_verses) = summarize(&new)?;
    let diff = diff_summaries((&old_summary, &old_verses), (&new_summary, &new_verses));
    let from = old.info().version.clone();
    let published = new.info().version.clone();
    if published > from {
        return Ok(Some(ImportUpdate {
            from,
            to: published,
            diff,
            bumped: false,
        }));
    }
    let to = next_version(&from, diff.change());
    set_version(fresh.as_ref(), &to)?;
    Ok(Some(ImportUpdate {
        from,
        to,
        diff,
        bumped: true,
    }))
}

/// Sets a made package's version in its `logoxide.toml`
fn set_version(storage: &dyn Storage, version: &semver::Version) -> Result<()> {
    let text = storage.read_string(MANIFEST)?;
    let mut doc: toml_edit::DocumentMut =
        (text.parse()).map_err(|e: toml_edit::TomlError| LogoxideError::Import(e.to_string()))?;
    doc["package"]["version"] = toml_edit::value(version.to_string());
    storage.write(MANIFEST, doc.to_string().as_bytes())
}

/**
Replaces an imported package's files (`current`) with a fresh import's: its declared files the
fresh one doesn't have are removed, the fresh one's written. Files beside them that aren't the
package's (a reader's annotations) stay
*/
pub fn apply_import(current: &dyn Storage, fresh: &dyn Storage) -> Result<()> {
    for path in import_removals(current, fresh)? {
        current.remove(&path)?;
    }
    for path in fresh.walk("")? {
        current.write(&path, &fresh.read(&path)?)?;
    }
    Ok(())
}

/// The imported package's files a fresh import replaces by nothing: its declared files the fresh
/// one lacks (what [`apply_import`] removes, for hosts that remove files themselves)
pub fn import_removals(current: &dyn Storage, fresh: &dyn Storage) -> Result<Vec<String>> {
    let new = fresh.walk("")?;
    Ok((crate::package::declared_files(current)?.into_iter())
        .filter(|path| !new.contains(path))
        .collect())
}

/// Whether an import at `installed` is as new as `entry` lists, so it needn't be downloaded again
/// to know (only where the publisher's versions follow its updates:
/// [`CatalogSource::versions_track_updates`])
pub fn import_is_current(entry: &CatalogEntry, installed: &semver::Version) -> bool {
    let listed = (entry.version.as_deref()).and_then(|v| semver::Version::parse(v).ok());
    entry.source.versions_track_updates() && listed.is_some_and(|v| v <= *installed)
}

/// How an import made its package
struct Made {
    /// The format its books are in, in a folder of that name
    format: &'static str,
    license_file: Option<String>,
    publisher: &'static str,
    homepage: String,
    upstream: String,
}

/// Writes the package's `logoxide.toml`, checks it opens with books, and copies it into `dest`
fn finish(
    entry: &CatalogEntry,
    made: Made,
    staged: MemoryStorage,
    dest: &dyn Storage,
) -> Result<()> {
    let version = (entry.version.as_deref())
        .and_then(|v| semver::Version::parse(v).ok())
        .unwrap_or_else(|| semver::Version::new(0, 0, 0));
    let info = Info {
        id: entry.id.clone(),
        name: entry.name.clone(),
        abbreviation: entry.abbreviation.clone(),
        authors: vec![],
        description: entry.description.clone(),
        aliases: entry.aliases.clone(),
        version,
        kind: entry.kind,
        language: entry.language.clone(),
        direction: entry.direction,
        license: entry.license.clone(),
        license_file: made.license_file,
        publisher: Some(made.publisher.to_string()),
        homepage: Some(made.homepage),
        repository: None,
        documentation: None,
        readme: None,
        keywords: entry.keywords.clone(),
        categories: entry.categories.clone(),
        about_books: entry.about_books.clone(),
        upstream: Some(made.upstream),
        versification: String::from("own"),
        maps_to: None,
        include: vec![],
    };
    let manifest = Manifest {
        package: Some(info),
        formats: [(
            made.format.to_string(),
            FormatEntry {
                path: format!("{}/", made.format),
                feature: None,
            },
        )]
        .into(),
        ..Manifest::default()
    };
    let text = toml::to_string(&manifest).map_err(|e| LogoxideError::Import(e.to_string()))?;
    let header = format!("# Imported from {} by logoxide\n", made.publisher);
    staged.write(MANIFEST, format!("{header}{text}").as_bytes())?;
    let staged = std::sync::Arc::new(staged);
    let package = Package::open(staged.clone(), Features::default())?;
    if package.books().is_empty() {
        return Err(LogoxideError::Import(format!("{}: no books", entry.id)));
    }
    for path in staged.walk("")? {
        dest.write(&path, &staged.read(&path)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::catalog::Catalog;

    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, text) in files {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn imports_an_ebible_zip() {
        let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
        let catalog = Catalog::parse_ebible(csv).unwrap();
        let entry = &catalog.entries[0];
        let bytes = zip(&[
            ("00-FRTeng-asv.usfm", "\\id FRT\n\\mt Front"),
            (
                "73-JHNeng-asv.usfm",
                "\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n",
            ),
            ("copr.htm", "<p>public domain</p>"),
            ("latin.css", "body {}"),
        ]);
        let dest = MemoryStorage::new();
        ebible(entry, &bytes, &dest).unwrap();
        assert_eq!(
            dest.walk("").unwrap(),
            [
                "copr.htm",
                "logoxide.toml",
                "usfm/00-FRTeng-asv.usfm",
                "usfm/73-JHNeng-asv.usfm"
            ]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(
            (
                info.id.as_str(),
                info.abbreviation.as_str(),
                info.version.to_string()
            ),
            ("eng-asv", "ASV", "2026.10.8".to_string())
        );
        assert_eq!(info.license_file.as_deref(), Some("copr.htm"));
        assert_eq!(package.books().len(), 1);
        assert!(ebible(entry, &zip(&[("copr.htm", "")]), &MemoryStorage::new()).is_err());

        // fetch.bible: a book's USX, and its attribution
        let mut from_fetch = entry.clone();
        from_fetch.source = CatalogSource::FetchBible {
            id: "eng_asv".into(),
            books: vec!["jhn".into()],
        };
        from_fetch.copyright = Some("Someone".into());
        let usx = r#"<?xml version="1.0" encoding="utf-8"?><usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c" sid="JHN 3"/><para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world.<verse eid="JHN 3:16"/></para></usx>"#;
        let dest = MemoryStorage::new();
        fetch_bible(
            &from_fetch,
            &[("jhn".into(), usx.as_bytes().to_vec())],
            &dest,
        )
        .unwrap();
        assert_eq!(
            dest.walk("").unwrap(),
            ["COPYRIGHT.txt", "logoxide.toml", "usx/JHN.usx"]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        assert_eq!(package.books().len(), 1);
        // A page in place of a book (fetch.bible answers unknown paths with its site)
        let page = [("jhn".to_string(), b"<!DOCTYPE html>".to_vec())];
        assert!(fetch_bible(&from_fetch, &page, &MemoryStorage::new()).is_err());
        assert!(ebible(entry, b"not a zip", &MemoryStorage::new()).is_err());
    }

    #[test]
    fn imports_a_ccel_bible() {
        let page = r#"<div id="worksBy_44" class="worksEntry">
        <h2>
            <a href="https://ccel.org/ccel/bible/heot/heot">
                Holy Bible: Hebrew &amp; Old Testament</a>
        </h2></div>
        <h2><a href="https://ccel.org/ccel/calvin/institutes">Not a Bible</a></h2>"#;
        let catalog = Catalog::parse(crate::catalog::CCEL_CATALOG, page).unwrap();
        assert_eq!(catalog.entries.len(), 1);
        let entry = catalog.find_import("ccel:heot").unwrap();
        assert_eq!(
            (entry.name.as_str(), entry.abbreviation.as_str()),
            ("Holy Bible: Hebrew & Old Testament", "HEOT")
        );
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
        ccel(entry, xml.as_bytes(), &dest).unwrap();
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
            ("heb", "1.1.0".to_string())
        );
        assert_eq!(info.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(
            info.description.as_deref(),
            Some("The Hebrew Old Testament (see ccel.org).")
        );
        assert_eq!(info.direction, crate::manifest::Direction::Rtl);
        assert_eq!(package.books().len(), 1);

        // Without an edition version, its date; a copyright isn't public domain
        let dated = xml
            .replace("<version>1.1</version>", "")
            .replace("Public Domain", "Copyright 2010 Someone");
        let dest = MemoryStorage::new();
        ccel(entry, dated.as_bytes(), &dest).unwrap();
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        assert_eq!(package.info().version.to_string(), "2002.12.31");
        assert_eq!(package.info().license.as_deref(), Some("LicenseRef-CCEL"));
        // CCEL's page for a work it restricts
        let page = b"<!DOCTYPE html><html><body>Log in</body></html>";
        let error = ccel(entry, page, &MemoryStorage::new()).unwrap_err();
        assert!(error.to_string().contains("isn't ThML"), "{error}");
    }

    /// A RawText module's zip, as a repository's `packages/<module>.zip` holds it: `conf`, and
    /// each verse's entry (verses sharing an entry are linked, as SWORD stores a range)
    fn sword_zip(conf: &str, verses: &[(&str, u8, u8, &str)]) -> Vec<u8> {
        use sword_modules::{Testament, VerseKey, Versification};
        let v11n = Versification::kjv();
        let slots = v11n.slot_count(Testament::New) as usize;
        let (mut index, mut data) = (vec![0u8; slots * 6], Vec::<u8>::new());
        let mut stored: Vec<(String, u32)> = vec![];
        for (book, chapter, verse, text) in verses {
            let key = VerseKey::from_osis(book, *chapter, *verse).unwrap();
            let at = v11n.index_of(&key).unwrap().slot as usize * 6;
            let offset = match stored.iter().find(|(t, _)| t == text) {
                Some((_, offset)) => *offset,
                None => {
                    let offset = data.len() as u32;
                    data.extend_from_slice(text.as_bytes());
                    stored.push((text.to_string(), offset));
                    offset
                }
            };
            index[at..at + 4].copy_from_slice(&offset.to_le_bytes());
            index[at + 4..at + 6].copy_from_slice(&(text.len() as u16).to_le_bytes());
        }
        let dir = "modules/texts/rawtext/test";
        zip_bytes(&[
            ("mods.d/test.conf", conf.as_bytes()),
            (&format!("{dir}/nt.vss"), &index),
            (&format!("{dir}/nt"), &data),
        ])
    }

    fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, bytes) in files {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const SWORD_CONF: &str = "[Test]\nDataPath=./modules/texts/rawtext/test/\nModDrv=RawText\nSourceType=OSIS\nEncoding=UTF-8\nLang=en\nVersion=1.2\nDescription=A  Test Bible\nDistributionLicense=Creative Commons: BY-SA 4.0\nShortCopyright=Someone, 2020\nAbout=A test.\\par Second paragraph.\n";

    fn sword_test_entry() -> CatalogEntry {
        let conf = sword_modules::ModuleConf::parse(SWORD_CONF).unwrap();
        let repository = "https://crosswire.org/ftpmirror/pub/sword/raw/";
        crate::catalog::sword_entry(
            &conf,
            crate::manifest::PackageKind::Bible,
            "crosswire",
            repository,
        )
    }

    #[test]
    fn imports_an_osis_sword_module() {
        let entry = sword_test_entry();
        assert_eq!(
            (
                entry.id.as_str(),
                entry.name.as_str(),
                entry.description.as_deref()
            ),
            ("sword-test", "A Test Bible", Some("A test."))
        );
        assert_eq!(
            entry.source.import_name().as_deref(),
            Some("sword:crosswire/Test")
        );
        let zip = sword_zip(
            SWORD_CONF,
            &[
                // The module's own chapter milestone, which the file's replaces
                (
                    "John",
                    1,
                    0,
                    r#"<chapter osisID="John.1" sID="c1"/> <title type="chapter">John 1</title>"#,
                ),
                ("John", 1, 1, "In the beginning was the Word."),
                // 3:16-17 is one entry
                (
                    "John",
                    3,
                    16,
                    "For God so loved the world, <w lemma=\"strong:G2889\">that</w> he gave.",
                ),
                (
                    "John",
                    3,
                    17,
                    "For God so loved the world, <w lemma=\"strong:G2889\">that</w> he gave.",
                ),
                ("John", 3, 18, "He that believeth.<chapter eID=\"c3\"/>"),
                // An introduction that is a verse's entry isn't one
                ("Rom", 1, 0, "He that believeth.<chapter eID=\"c3\"/>"),
                ("Rom", 1, 1, "Paul, a servant."),
            ],
        );
        let dest = MemoryStorage::new();
        sword(&entry, &zip, &dest).unwrap();
        assert_eq!(
            dest.walk("").unwrap(),
            [
                "COPYRIGHT.txt",
                "logoxide.toml",
                "osis/John.xml",
                "osis/Rom.xml"
            ]
        );
        let john = String::from_utf8(dest.read("osis/John.xml").unwrap()).unwrap();
        assert!(!john.contains("sID=\"c1\""), "{john}");
        assert!(
            john.contains(r#"<verse osisID="John.3.16 John.3.17" sID="John.3.16" n="16-17"/>"#)
        );
        let romans = String::from_utf8(dest.read("osis/Rom.xml").unwrap()).unwrap();
        assert!(!romans.contains("believeth"), "{romans}");
        let notice = String::from_utf8(dest.read("COPYRIGHT.txt").unwrap()).unwrap();
        assert!(
            notice.contains("Someone, 2020") && notice.contains("Second paragraph"),
            "{notice}"
        );

        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(
            (info.version.to_string(), info.license.as_deref()),
            ("1.2.0".to_string(), Some("CC-BY-SA-4.0"))
        );
        let john = crate::bible_data().books().search_usfm("JHN").unwrap();
        assert_eq!(package.source().chapter_verses(john).unwrap(), [1, 0, 18]);
        let book = package.source().book(john).unwrap();
        let span = &book.spans[1];
        let words = topos_bible_markup::verses::slice(&book.nodes, &span.start, Some(&span.end));
        assert_eq!(
            (span.verse, span.last, crate::summary::verse_text(&words)),
            (
                16,
                17,
                "For God so loved the world, that he gave.".to_string()
            )
        );

        // GBF isn't read; a zip without the module is an error
        let gbf = SWORD_CONF.replace("SourceType=OSIS", "SourceType=GBF");
        let error = sword(
            &entry,
            &sword_zip(&gbf, &[("John", 1, 1, "x")]),
            &MemoryStorage::new(),
        );
        assert!(error.unwrap_err().to_string().contains("OSIS and ThML"));
        let other = SWORD_CONF.replace("[Test]", "[Other]");
        assert!(
            sword(
                &entry,
                &sword_zip(&other, &[("John", 1, 1, "x")]),
                &MemoryStorage::new()
            )
            .is_err()
        );
    }

    #[test]
    fn imports_a_thml_sword_module() {
        let entry = sword_test_entry();
        let conf = SWORD_CONF.replace("SourceType=OSIS", "SourceType=ThML");
        let zip = sword_zip(
            &conf,
            &[("John", 3, 16, "For God <i>so</i> loved the world.")],
        );
        let dest = MemoryStorage::new();
        sword(&entry, &zip, &dest).unwrap();
        let file = String::from_utf8(dest.read("thml/John.xml").unwrap()).unwrap();
        assert!(
            file.contains(r#"<scripture passage="John 3.16" parsed="|John|3|16|0|0""#),
            "{file}"
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let john = crate::bible_data().books().search_usfm("JHN").unwrap();
        assert_eq!(package.source().chapter_verses(john).unwrap()[2], 16);
    }

    #[test]
    fn updates_an_import() {
        use std::sync::Arc;
        let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
        let catalog = Catalog::parse_ebible(csv).unwrap();
        let entry = &catalog.entries[0];
        let book = |text: &str| format!("\\id JHN\n\\c 3\n\\p\n\\v 16 {text}\n");
        let made = |files: &[(&str, &str)]| {
            let storage = Arc::new(MemoryStorage::new());
            ebible(entry, &zip(files), storage.as_ref()).unwrap();
            storage
        };
        let current = made(&[
            ("73-JHNeng-asv.usfm", &book("For God so loved the world.")),
            (
                "74-ACTeng-asv.usfm",
                "\\id ACT\n\\c 1\n\\p\n\\v 1 The former treatise.\n",
            ),
        ]);
        // An imported package names its catalog entry by its upstream
        let package = Package::open(current.clone(), Features::default()).unwrap();
        let upstream = package.info().upstream.clone().unwrap();
        assert_eq!(catalog.find_upstream(&upstream), Some(entry));
        assert_eq!(super::upstream(entry), Some(upstream));
        assert!(entry.source.versions_track_updates());

        // The same download: nothing to update
        let same = made(&[
            ("73-JHNeng-asv.usfm", &book("For God so loved the world.")),
            (
                "74-ACTeng-asv.usfm",
                "\\id ACT\n\\c 1\n\\p\n\\v 1 The former treatise.\n",
            ),
        ]);
        assert!(compare_import(current.clone(), &same).unwrap().is_none());

        // Reworded at the same version (as fetch.bible's change): the version is raised
        let fresh = made(&[(
            "73-JHNeng-asv.usfm",
            &book("For God so much loved the world."),
        )]);
        let update = compare_import(current.clone(), &fresh).unwrap().unwrap();
        assert_eq!(
            (
                update.from.to_string(),
                update.to.to_string(),
                update.bumped
            ),
            ("2026.10.8".into(), "2027.0.0".into(), true)
        );
        assert_eq!(
            (update.diff.changed.len(), update.diff.removed.len()),
            (1, 1)
        );
        // A reader's note beside the package stays; a book the publisher dropped goes
        current.write("JHN notes.md", b"mine").unwrap();
        apply_import(current.as_ref(), fresh.as_ref()).unwrap();
        assert_eq!(
            current.walk("").unwrap(),
            ["JHN notes.md", "logoxide.toml", "usfm/73-JHNeng-asv.usfm"]
        );
        let package = Package::open(current.clone(), Features::default()).unwrap();
        assert_eq!(package.info().version.to_string(), "2027.0.0");
        // Made again from the same download: current at its raised version, not raised again
        let again = made(&[(
            "73-JHNeng-asv.usfm",
            &book("For God so much loved the world."),
        )]);
        assert!(compare_import(current.clone(), &again).unwrap().is_none());
    }
}
