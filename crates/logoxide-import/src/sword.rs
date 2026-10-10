/*!
SWORD repositories: a repository's list of modules ([`catalog`], its `mods.d.tar.gz`) and a
module's zip ([`import`], from [`zip_urls`]), read with sword-modules from memory: its entries
in their own markup (OSIS or ThML), a file per book, with its conf's copyright and license in
`COPYRIGHT.txt`. A module's `Version` follows its updates.

Repositories are named as an import names them (`sword:crosswire/KJV`): CrossWire's list
([`REPOSITORIES`]), else by host ([`repository_name`])
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    license::{Derivatives, Rights},
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};
use sword_modules::{Markup, ModuleConf, ModuleKind};

use crate::{Made, external, finish, import_id, package_id, unzip};

/**
The SWORD repositories in CrossWire's list of them (`masterRepoList.conf`) served over HTTP(S), by
the name an import gives each (`sword:crosswire/KJV`). A repository's modules are at its root:
its catalog is `mods.d.tar.gz` there, a module's zip `packages/<module>.zip`. Others are named by
their host ([`repository_name`])
*/
pub const REPOSITORIES: [(&str, &str); 9] = [
    (
        "crosswire",
        "https://crosswire.org/ftpmirror/pub/sword/raw/",
    ),
    (
        "crosswire-beta",
        "https://crosswire.org/ftpmirror/pub/sword/betaraw/",
    ),
    (
        "crosswire-attic",
        "https://crosswire.org/ftpmirror/pub/sword/atticraw/",
    ),
    (
        "crosswire-wycliffe",
        "https://crosswire.org/ftpmirror/pub/sword/wyclifferaw/",
    ),
    (
        "bible-org",
        "https://crosswire.org/ftpmirror/pub/bible.org/sword/",
    ),
    ("ebible", "https://ebible.org/sword/"),
    ("dbg", "https://crosswire.org/ftpmirror/pub/sword/dbgraw/"),
    (
        "lockman",
        "https://crosswire.org/ftpmirror/pub/sword/lockmanraw/",
    ),
    // Over plain HTTP: ftp.xiphos.org serves no HTTPS (October 2026), so what it gives could be
    // changed on the way (`logoxide::git::is_insecure` says so, for a warning)
    ("xiphos", "http://ftp.xiphos.org/pub/xiphos/"),
];

/// What an import names a SWORD repository by (its root URL): its name in [`REPOSITORIES`],
/// else its host's (`sword.example.org` is `sword-example-org`)
pub fn repository_name(repository: &str) -> String {
    let root = repository.trim_end_matches('/');
    if let Some((name, _)) =
        (REPOSITORIES.iter()).find(|(_, url)| url.trim_end_matches('/') == root)
    {
        return name.to_string();
    }
    let host = root.split("://").nth(1).unwrap_or(root);
    let host = host.split('/').next().unwrap_or(host);
    package_id(host.trim_start_matches("www.")).replace('.', "-")
}

/// A SWORD repository's catalog, by the name an import gives it (`crosswire`)
pub fn catalog_url(name: &str) -> Option<String> {
    (REPOSITORIES.iter())
        .find(|(n, _)| *n == name)
        .map(|(_, url)| format!("{url}mods.d.tar.gz"))
}

/// Where a SWORD repository serves a module's zip: `packages/<module>.zip` at its root, else
/// with the name in lowercase (as some repositories name them)
pub fn zip_urls(repository: &str, module: &str) -> Vec<String> {
    let root = repository.trim_end_matches('/');
    let mut urls = vec![format!("{root}/packages/{module}.zip")];
    if module.to_lowercase() != module {
        urls.push(format!("{root}/packages/{}.zip", module.to_lowercase()));
    }
    urls
}

/**
A SWORD repository's catalog: its `mods.d.tar.gz` (at `location`, its root's `mods.d.tar.gz`),
every module's conf. Listed are the Bibles and commentaries that aren't locked (enciphered
without a key) and whose markup is OSIS or ThML (what logoxide reads), each with its conf's
description, language, version, and license, a `sword:<repository>/<module>` import of its zip
*/
pub fn catalog(location: &str, bytes: &[u8]) -> Result<Catalog> {
    use std::io::Read;
    let bad = |e: std::io::Error| LogoxideError::Catalog(format!("mods.d.tar.gz: {e}"));
    let repository = location.strip_suffix("mods.d.tar.gz").unwrap_or(location);
    let short = repository_name(repository);
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut entries = vec![];
    for file in archive.entries().map_err(bad)? {
        let mut file = file.map_err(bad)?;
        let is_conf = (file.path().ok())
            .is_some_and(|p| p.to_string_lossy().to_ascii_lowercase().ends_with(".conf"));
        if !file.header().entry_type().is_file() || !is_conf {
            continue;
        }
        let mut data = vec![];
        file.by_ref()
            .take(1 << 20)
            .read_to_end(&mut data)
            .map_err(bad)?;
        let Ok(confs) = sword_modules::conf::parse_conf(&conf_text(&data)) else {
            continue;
        };
        for conf in confs {
            let kind = match conf.module_kind() {
                Ok(ModuleKind::Bible) => PackageKind::Bible,
                Ok(ModuleKind::Commentary) => PackageKind::Commentary,
                _ => continue,
            };
            let readable = matches!(conf.source_type(), Markup::Osis | Markup::Thml);
            if conf.is_locked() || !readable || conf.get("ModDrv") == Some("RawFiles") {
                continue;
            }
            entries.push(entry(&conf, kind, &short, repository));
        }
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Catalog {
        name: format!("SWORD: {short}"),
        entries,
        warnings: vec![],
    })
}

/// A conf file's text: UTF-8, else Latin-1 (as older confs are)
fn conf_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// A SWORD module's catalog entry, from its conf: a repository other than CrossWire's main one
/// (`repository_name`, as [`repository_name`] gives it) adds its name to the id, so its module's
/// id is its own; `repository` is the root URL its zip is below
pub fn entry(
    conf: &ModuleConf,
    kind: PackageKind,
    repository_name: &str,
    repository: &str,
) -> CatalogEntry {
    let module = conf.name().to_string();
    let text = |key: &str| {
        (conf.get(key))
            .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|v| !v.is_empty())
    };
    let id = match repository_name {
        "crosswire" => format!("sword-{}", package_id(&module)),
        other => format!("sword-{other}-{}", package_id(&module)),
    };
    let copyright = text("ShortCopyright").or_else(|| text("Copyright"));
    // SWORD's `Lang` (`en_US`) as a tag; what isn't one is `und`, the conf's text kept
    let (language, raw) = language::normalize_language(conf.lang().unwrap_or("und"));
    let downloads = vec![Download {
        name: String::from("zip"),
        urls: zip_urls(repository, &module),
    }];
    let source = external(
        "sword",
        &format!("{repository_name}/{module}"),
        true,
        downloads,
    );
    CatalogEntry {
        id,
        name: text("Description").unwrap_or_else(|| module.clone()),
        abbreviation: text("Abbreviation").unwrap_or_else(|| module.clone()),
        aliases: vec![],
        script: language::script_of_tag(&language),
        language,
        language_name: raw,
        // Its first paragraph (the whole is in the package's license file)
        description: (conf.about_text()).and_then(|a| {
            (a.split('\n'))
                .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                .find(|p| !p.is_empty())
        }),
        license: Some(license(conf.get("DistributionLicense"))),
        rights: rights(conf.get("DistributionLicense")),
        copyright,
        version: (conf.version().map(str::trim))
            .filter(|v| !v.is_empty())
            .map(String::from),
        kind,
        direction: match conf.direction() {
            sword_modules::conf::Direction::RightToLeft => Direction::Rtl,
            _ => Direction::Ltr,
        },
        path: None,
        releases: vec![],
        keywords: vec![],
        categories: vec![],
        about_books: vec![],
        work: logoxide::works::of_source(&source),
        edition: None,
        edition_year: None,
        source,
    }
}

/**
A SWORD module's `DistributionLicense` as SPDX: public domain (when it doesn't deny it), a
Creative Commons license (`Creative Commons: BY-SA 4.0` is `CC-BY-SA-4.0`), the GPL; anything
else (`Copyrighted; Free non-commercial distribution`) is `LicenseRef-SWORD`, its statement in the
package's license file, with what it allows in [`rights`]
*/
pub fn license(license: Option<&str>) -> String {
    let Some(license) = license.map(str::trim) else {
        return String::from("LicenseRef-SWORD");
    };
    let lower = license.to_ascii_lowercase();
    if crate::license::says_public_domain(&lower) {
        return String::from("LicenseRef-PublicDomain");
    }
    if let Some(cc) = lower.strip_prefix("creative commons:") {
        let mut parts = cc.split_whitespace();
        let (Some(terms), version) = (parts.next(), parts.next()) else {
            return String::from("LicenseRef-SWORD");
        };
        if terms.eq_ignore_ascii_case("zero") || terms.eq_ignore_ascii_case("cc0") {
            return String::from("CC0-1.0");
        }
        return format!("CC-{}-{}", terms.to_uppercase(), version.unwrap_or("4.0"));
    }
    match lower.as_str() {
        "gpl" | "gplv2" | "gpl v2" => String::from("GPL-2.0-only"),
        "gplv3" | "gpl v3" => String::from("GPL-3.0-only"),
        _ => String::from("LicenseRef-SWORD"),
    }
}

/**
What a SWORD module's `DistributionLicense` allows, where it's a statement rather than a license
SPDX names (whose rights are derived from it): permission given to CrossWire alone, or only in
SWORD's format (`Copyrighted; Permission to distribute granted to CrossWire`, `…to distribute
non-commercially in SWORD format`) isn't anyone's to redistribute, nor to convert; `Free
non-commercial distribution` and `Freely distributable` are redistributable, without
derivatives (the statement grants distribution and nothing more, and not commercially, for the
first). Others say nothing
*/
pub fn rights(license: Option<&str>) -> Rights {
    let Some(license) = license.map(|l| l.trim().to_lowercase()) else {
        return Rights::default();
    };
    let commercial = !(license.contains("non-commercial")
        || license.contains("noncommercial")
        || license.contains("non commercial"));
    let to_crosswire = license.contains("crosswire")
        || license.contains("sword format")
        || license.contains("no other distribution");
    if to_crosswire {
        return Rights {
            redistributable: Some(false),
            derivatives: Some(Derivatives::None),
            commercial: (!commercial).then_some(false),
            attribution: None,
        };
    }
    let free = license.contains("free non-commercial distribution")
        || license.contains("freely distributable");
    match free {
        true => Rights {
            redistributable: Some(true),
            derivatives: Some(Derivatives::None),
            commercial: (!commercial).then_some(false),
            attribution: None,
        },
        false => Rights::default(),
    }
}

/// The repository a SWORD entry's module is served from (its root URL), as its first download
/// says: `<root>/packages/<module>.zip`
fn repository_of(entry: &CatalogEntry, module: &str) -> Option<String> {
    let url = entry.source.downloads().first()?.urls.first()?;
    let root = url.strip_suffix(&format!("packages/{module}.zip"))?;
    Some(root.to_string())
}

/**
A SWORD module from its zip (as [`zip_urls`] serve it), as a package in `dest`: each book a
file in the module's own markup (`osis/Gen.xml` for an OSIS module, `thml/Gen.xml` for ThML),
`logoxide.toml`, and `COPYRIGHT.txt` (the conf's copyright, license, and about text). The zip's
conf describes the package (its description, language, version, license, and direction); the
entry gives its id.

Each verse becomes a marker and its entry (`<verse osisID="John.3.16" sID="John.3.16"/>…<verse
eID="John.3.16"/>`, or ThML's `<scripture parsed="|John|3|16|0|0"/>`), in chapters, with a
book's and a chapter's introductions before their first verse. Verses SWORD links to one entry
(the verses of a range, [`VerseModule::iter_spans`]) are one verse, bridged (`n="16-17"`), or
for a run into the next chapter one id (`osisID="John.3.36-John.4.2"`, ThML's
`parsed="|John|3|36|4|2"`); verses that merely read the same stay apart. An introduction stored
where an earlier verse's entry is (`EntryLocation`) is left out. A locked module, or markup other
than OSIS and ThML, is an error

[`VerseModule::iter_spans`]: sword_modules::VerseModule::iter_spans
*/
pub fn import(entry: &CatalogEntry, zip: &[u8], dest: &dyn Storage) -> Result<()> {
    use sword_modules::{MemoryFiles, SwordLibrary};
    let id = import_id(entry, "sword", "a SWORD repository")?;
    let module = id.rsplit('/').next().unwrap_or(id).to_string();
    let repository = repository_of(entry, &module).ok_or_else(|| {
        LogoxideError::Import(format!(
            "{}: no repository to say where it's from",
            entry.id
        ))
    })?;
    let failed = |e: sword_modules::Error| LogoxideError::Import(format!("{module}: {e}"));
    let mut files = MemoryFiles::new();
    for (path, bytes) in unzip(zip)? {
        files.insert(path, bytes);
    }
    let library = SwordLibrary::open_files(std::sync::Arc::new(files)).map_err(failed)?;
    let conf = (library.conf(&module).cloned())
        .ok_or_else(|| LogoxideError::Import(format!("the zip has no module {module}")))?;
    let opened = library.open_module(&module).map_err(failed)?;
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
    let described = self::entry(&conf, entry.kind, "", &repository);
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
    // Where every verse's entry so far is stored: an introduction stored there points at that
    // verse's entry (the ISV's John 1:0 is Luke 23:56), and isn't an introduction
    let mut seen = std::collections::HashSet::new();
    for read in verses.iter_spans() {
        let span = read.map_err(failed)?;
        let (first, last) = (span.first, span.last);
        if first.verse == 0 && first == last && seen.contains(&span.location) {
            continue;
        }
        if last.verse > 0 {
            seen.insert(span.location);
        }
        // A run of linked verses is one verse, bridged (into the next chapter, when SWORD links
        // John 3:36 and 4:1-2); one starting at an introduction linked to the verse after it is
        // that verse's
        let (chapter, verse) = match first.verse == 0 && last.verse > 0 {
            true => (first.chapter.max(1), 1),
            false => (first.chapter, first.verse),
        };
        let verse = SwordVerse {
            chapter,
            verse,
            last_chapter: last.chapter,
            last: last.verse,
            text: match markup {
                Markup::Osis => without_structure(&span.entry.raw),
                _ => span.entry.raw,
            },
        };
        match books.last_mut() {
            Some((book, list)) if *book == first.book => list.push(verse),
            _ => books.push((first.book, vec![verse])),
        }
    }
    let staged = MemoryStorage::new();
    let language = entry.language.clone();
    for (book, list) in &books {
        let Some(osis) = sword_modules::VerseKey::new(*book, 1, 1).osis_book() else {
            continue;
        };
        let text = match markup {
            Markup::Osis => osis_book_file(&module, &language, osis, list),
            _ => thml_book_file(osis, list),
        };
        staged.write(&format!("{format}/{osis}.xml"), text.as_bytes())?;
    }
    staged.write("COPYRIGHT.txt", notice(&conf, &repository).as_bytes())?;
    let made = Made {
        format,
        license_file: Some(String::from("COPYRIGHT.txt")),
        publisher: "SWORD",
        homepage: (conf.get("TextSource").filter(|t| t.contains("://")))
            .map_or_else(|| repository.clone(), String::from),
        versification: Some(versification(conf.get("Versification"))),
    };
    finish(&entry, made, staged, dest)
}

/**
A SWORD module's versification, by SWORD's name for it (`KJV`, `KJVA`, `NRSV`, `NRSVA`, `MT`,
`Leningrad`, `Synodal`, `SynodalProt`, `Vulg`, `LXX`, `Orthodox`, `German`, `Luther`, `Catholic`,
`Catholic2`, `Segond`, `Calvin`, `DarbyFr`), from its conf's `Versification` (none means `KJV`,
as SWORD reads it). A name SWORD knows is written as SWORD spells it, whatever the conf's case;
another is kept as the conf gives it
*/
pub fn versification(conf: Option<&str>) -> String {
    match conf.map(str::trim).filter(|v| !v.is_empty()) {
        None => sword_modules::Versification::kjv().name().to_string(),
        Some(name) => sword_modules::Versification::get(name)
            .map_or_else(|| name.to_string(), |v| v.name().to_string()),
    }
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
    /// The chapter of `last`: `chapter`, unless linked verses run into the next
    last_chapter: u8,
    /// The last verse a bridge covers (`verse` otherwise)
    last: u8,
    text: String,
}

impl SwordVerse {
    fn across_chapters(&self) -> bool {
        self.last_chapter > self.chapter
    }

    fn is_bridge(&self) -> bool {
        self.across_chapters() || self.last > self.verse
    }
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
        // A range into the next chapter is one id (`John.3.36-John.4.2`), numbered by it
        let (ids, n) = match (v.across_chapters(), v.last > v.verse) {
            (true, _) => (format!("{id}-{book}.{}.{}", v.last_chapter, v.last), None),
            (false, true) => (
                (v.verse..=v.last)
                    .map(|n| format!("{book}.{}.{n}", v.chapter))
                    .collect::<Vec<_>>()
                    .join(" "),
                Some(format!("{}-{}", v.verse, v.last)),
            ),
            (false, false) => (id.clone(), Some(v.verse.to_string())),
        };
        let n = n.map_or_else(String::new, |n| format!(" n=\"{n}\""));
        out.push_str(&format!(
            "<verse osisID=\"{ids}\" sID=\"{id}\"{n}/>{}<verse eID=\"{id}\"/>\n",
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
        let (c, first, last, lc) = (v.chapter, v.verse, v.last, v.last_chapter);
        let (passage, parsed) = match v.is_bridge() {
            true if v.across_chapters() => (
                format!("{book} {c}.{first}-{lc}.{last}"),
                format!("|{book}|{c}|{first}|{lc}|{last}"),
            ),
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
fn notice(conf: &ModuleConf, repository: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::zip_bytes;
    use logoxide::{
        catalog::SWORD_CATALOG,
        manifest::MANIFEST,
        package::{Features, Package},
    };

    #[test]
    fn reads_a_sword_repositorys_modules() {
        let conf = |name: &str, extra: &str| {
            format!(
                "[{name}]\nDataPath=./modules/texts/ztext/{name}/\nDescription={name} Bible\nLang=de\n{extra}\n"
            )
        };
        let files = [
            (
                "mods.d/kjv.conf",
                conf(
                    "KJV",
                    "ModDrv=zText\nSourceType=OSIS\nVersion=3.1\nDistributionLicense=GPL",
                ),
            ),
            (
                "mods.d/isv.conf",
                conf("ISV", "ModDrv=zText\nSourceType=ThML\nDirection=RtoL"),
            ),
            (
                "mods.d/mhc.conf",
                conf("MHC", "ModDrv=zCom\nSourceType=ThML"),
            ),
            // GBF (the default), locked, a dictionary, personal notes: left out
            ("mods.d/old.conf", conf("Old", "ModDrv=zText")),
            (
                "mods.d/nasb.conf",
                conf("NASB", "ModDrv=zText\nSourceType=OSIS\nCipherKey="),
            ),
            (
                "mods.d/easton.conf",
                conf("Easton", "ModDrv=RawLD\nSourceType=ThML"),
            ),
            (
                "mods.d/notes.conf",
                conf("Notes", "ModDrv=RawFiles\nSourceType=ThML"),
            ),
        ];
        let mut tar = tar::Builder::new(Vec::new());
        for (path, text) in &files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, text.as_bytes()).unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar.into_inner().unwrap()).unwrap();
        let bytes = gz.finish().unwrap();

        // CrossWire's catalog is the one the package manager lists by default
        assert_eq!(catalog_url("crosswire").as_deref(), Some(SWORD_CATALOG));
        let catalog = crate::catalog::parse(SWORD_CATALOG, &bytes).unwrap();
        assert_eq!(catalog.name, "SWORD: crosswire");
        let ids: Vec<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["sword-isv", "sword-kjv", "sword-mhc"]);
        let kjv = catalog.find_import("sword:crosswire/KJV").unwrap();
        assert_eq!(
            (
                kjv.version.as_deref(),
                kjv.license.as_deref(),
                kjv.language.as_str()
            ),
            (Some("3.1"), Some("GPL-2.0-only"), "de")
        );
        assert_eq!(
            kjv.source.downloads(),
            [Download {
                name: "zip".into(),
                urls: vec![
                    "https://crosswire.org/ftpmirror/pub/sword/raw/packages/KJV.zip".into(),
                    "https://crosswire.org/ftpmirror/pub/sword/raw/packages/kjv.zip".into()
                ]
            }]
        );
        assert_eq!(
            kjv.source.upstream_url(),
            Some("https://crosswire.org/ftpmirror/pub/sword/raw/packages/KJV.zip")
        );
        assert!(kjv.source.versions_track_updates());
        assert_eq!(kjv.work.as_deref(), Some("kjv"));
        assert_eq!(catalog.entries[0].direction, Direction::Rtl);
        assert_eq!(catalog.entries[2].kind, PackageKind::Commentary);

        // Another repository's modules are named by it
        let other =
            crate::catalog::parse("https://sword.example.org/repo/mods.d.tar.gz", &bytes).unwrap();
        assert_eq!(other.entries[1].id, "sword-sword-example-org-kjv");
        assert_eq!(
            other.entries[1].source.import_name(),
            Some("sword:sword-example-org/KJV")
        );
        assert_eq!(
            other.entries[1].source.upstream_url(),
            Some("https://sword.example.org/repo/packages/KJV.zip")
        );
        assert_eq!(
            catalog_url("crosswire-beta").as_deref(),
            Some("https://crosswire.org/ftpmirror/pub/sword/betaraw/mods.d.tar.gz")
        );
        assert_eq!(
            license(Some("Creative Commons: BY-NC-ND 3.0")),
            "CC-BY-NC-ND-3.0"
        );
        assert_eq!(
            license(Some("Copyrighted; Free non-commercial distribution")),
            "LicenseRef-SWORD"
        );
        // A denial isn't the public domain
        assert_eq!(
            license(Some("Copyrighted; not public domain")),
            "LicenseRef-SWORD"
        );
        let to_crosswire = rights(Some(
            "Copyrighted; Permission to distribute granted to CrossWire",
        ));
        assert_eq!(
            (to_crosswire.redistributable, to_crosswire.derivatives),
            (Some(false), Some(Derivatives::None))
        );
        let sword_format = rights(Some(
            "Copyrighted; Permission granted to distribute non-commercially in SWORD format",
        ));
        assert_eq!(
            (sword_format.redistributable, sword_format.commercial),
            (Some(false), Some(false))
        );
        assert_eq!(
            rights(Some("Copyrighted; Free non-commercial distribution")),
            Rights {
                redistributable: Some(true),
                derivatives: Some(Derivatives::None),
                commercial: Some(false),
                attribution: None
            }
        );
        assert!(rights(Some("Public Domain")).is_empty());
        // Derived where SPDX names it
        assert_eq!(kjv.text_rights().derivatives, Some(Derivatives::ShareAlike));
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

    const SWORD_CONF: &str = "[Test]\nDataPath=./modules/texts/rawtext/test/\nModDrv=RawText\nSourceType=OSIS\nEncoding=UTF-8\nLang=en\nVersion=1.2\nDescription=A  Test Bible\nDistributionLicense=Creative Commons: BY-SA 4.0\nShortCopyright=Someone, 2020\nAbout=A test.\\par Second paragraph.\n";

    fn sword_test_entry() -> CatalogEntry {
        let conf = ModuleConf::parse(SWORD_CONF).unwrap();
        let repository = "https://crosswire.org/ftpmirror/pub/sword/raw/";
        entry(&conf, PackageKind::Bible, "crosswire", repository)
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
        assert_eq!(entry.source.import_name(), Some("sword:crosswire/Test"));
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
                // The same words stored again aren't linked: two verses
                ("John", 1, 2, "In the beginning was the Word.  "),
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
                // 3:36 and 4:1-2 are one entry, across the chapter
                ("John", 3, 36, "He that believeth on the Son."),
                ("John", 4, 1, "He that believeth on the Son."),
                ("John", 4, 2, "He that believeth on the Son."),
                ("John", 4, 3, "He left Judaea."),
                // An introduction that is a verse's entry isn't one
                ("Rom", 1, 0, "He that believeth.<chapter eID=\"c3\"/>"),
                ("Rom", 1, 1, "Paul, a servant."),
            ],
        );
        let dest = MemoryStorage::new();
        crate::build(&entry, &[("zip".into(), zip)], &dest).unwrap();
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
        assert!(john.contains(r#"<verse osisID="John.3.36-John.4.2" sID="John.3.36"/>"#));
        assert!(john.contains(r#"<verse osisID="John.1.2" sID="John.1.2" n="2"/>"#));
        let romans = String::from_utf8(dest.read("osis/Rom.xml").unwrap()).unwrap();
        assert!(!romans.contains("believeth"), "{romans}");
        let notice = String::from_utf8(dest.read("COPYRIGHT.txt").unwrap()).unwrap();
        assert!(
            notice.contains("Someone, 2020")
                && notice.contains("Second paragraph")
                && notice.contains("https://crosswire.org/ftpmirror/pub/sword/raw/"),
            "{notice}"
        );

        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(
            (info.version.to_string(), info.license.as_deref()),
            ("1.0.0".to_string(), Some("CC-BY-SA-4.0"))
        );
        let upstream = info.upstream.as_ref().unwrap();
        // Its entries were made into OSIS files
        assert!(upstream.converted);
        assert_eq!(
            (
                upstream.url.as_deref(),
                upstream.import.as_deref(),
                upstream.version.as_deref()
            ),
            (
                Some("https://crosswire.org/ftpmirror/pub/sword/raw/packages/Test.zip"),
                Some("sword:crosswire/Test"),
                Some("1.2")
            )
        );
        // No Versification in the conf: SWORD's default
        assert_eq!(info.versification, "KJV");
        let john = logoxide::bible_data().books().search_usfm("JHN").unwrap();
        assert_eq!(
            package.source().chapter_verses(john).unwrap(),
            [2, 0, 36, 3]
        );
        let book = package.source().book(john).unwrap();
        let across = &book.spans[4];
        assert_eq!(
            (
                across.chapter,
                across.verse,
                across.last_chapter,
                across.last
            ),
            (3, 36, 4, 2)
        );
        let span = &book.spans[2];
        let words = logoxide_text::markup::verses::slice(&book.nodes, &span.start, Some(&span.end));
        assert_eq!(
            (
                span.verse,
                span.last,
                logoxide_text::hash::verse_text(&words, logoxide_text::hash::HashRule::CURRENT)
            ),
            (
                16,
                17,
                "For God so loved the world, that he gave.".to_string()
            )
        );

        // GBF isn't read; a zip without the module is an error
        let gbf = SWORD_CONF.replace("SourceType=OSIS", "SourceType=GBF");
        let error = import(
            &entry,
            &sword_zip(&gbf, &[("John", 1, 1, "x")]),
            &MemoryStorage::new(),
        );
        assert!(error.unwrap_err().to_string().contains("OSIS and ThML"));
        let other = SWORD_CONF.replace("[Test]", "[Other]");
        assert!(
            import(
                &entry,
                &sword_zip(&other, &[("John", 1, 1, "x")]),
                &MemoryStorage::new()
            )
            .is_err()
        );
    }

    #[test]
    fn a_module_crosswire_may_distribute_isnt_redistributable() {
        let conf = SWORD_CONF.replace(
            "Creative Commons: BY-SA 4.0",
            "Copyrighted; Permission to distribute granted to CrossWire",
        );
        let zip = sword_zip(&conf, &[("John", 1, 1, "In the beginning.")]);
        let dest = MemoryStorage::new();
        import(&sword_test_entry(), &zip, &dest).unwrap();
        let manifest = dest.read_string(MANIFEST).unwrap();
        assert!(manifest.contains("[package.rights]"), "{manifest}");
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(info.license.as_deref(), Some("LicenseRef-SWORD"));
        let kinds: Vec<_> = (logoxide::license::assess(info).into_iter())
            .map(|c| c.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                logoxide::license::ConcernKind::NotRedistributable,
                logoxide::license::ConcernKind::Converted
            ]
        );
    }

    #[test]
    fn swords_languages_become_tags() {
        let entry_of = |lang: &str| {
            let conf = SWORD_CONF.replace("Lang=en", &format!("Lang={lang}"));
            let conf = ModuleConf::parse(&conf).unwrap();
            let entry = entry(&conf, PackageKind::Bible, "x", "https://x.example/");
            (entry.language, entry.language_name, entry.script)
        };
        assert_eq!(entry_of("en_US"), ("en-US".into(), None, None));
        assert_eq!(
            entry_of("sr_Latn"),
            ("sr-Latn".into(), None, Some("Latn".into()))
        );
        assert_eq!(
            entry_of("Old English"),
            ("und".into(), Some("Old English".into()), None)
        );
        // Imported, the tag is the package's language and the conf's text its name
        let conf = SWORD_CONF.replace("Lang=en", "Lang=Old English");
        let zip = sword_zip(&conf, &[("John", 1, 1, "In the beginning.")]);
        let dest = MemoryStorage::new();
        import(&sword_test_entry(), &zip, &dest).unwrap();
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        assert_eq!(
            (
                package.info().language.as_str(),
                package.info().language_name.as_deref()
            ),
            ("und", Some("Old English"))
        );
    }

    #[test]
    fn sword_versifications_keep_swords_names() {
        assert_eq!(versification(None), "KJV");
        assert_eq!(versification(Some(" ")), "KJV");
        assert_eq!(versification(Some("synodal")), "Synodal");
        assert_eq!(versification(Some("NRSVA")), "NRSVA");
        assert_eq!(versification(Some("vulg")), "Vulg");
        // One SWORD doesn't know is kept as given
        assert_eq!(versification(Some("Custom")), "Custom");
        // The names doc/manifest.md lists
        let mut names: Vec<&str> = (sword_modules::Versification::all().iter())
            .map(|v| v.name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "Calvin",
                "Catholic",
                "Catholic2",
                "DarbyFr",
                "German",
                "KJV",
                "KJVA",
                "LXX",
                "Leningrad",
                "Luther",
                "MT",
                "NRSV",
                "NRSVA",
                "Orthodox",
                "Segond",
                "Synodal",
                "SynodalProt",
                "Vulg"
            ]
        );
    }

    #[test]
    fn imports_a_thml_sword_module() {
        let entry = sword_test_entry();
        let conf = SWORD_CONF.replace("SourceType=OSIS", "SourceType=ThML");
        let zip = sword_zip(
            &conf,
            &[
                ("John", 3, 16, "For God <i>so</i> loved the world."),
                ("John", 3, 36, "He that believeth."),
                ("John", 4, 1, "He that believeth."),
                ("John", 4, 2, "He that believeth."),
            ],
        );
        let dest = MemoryStorage::new();
        import(&entry, &zip, &dest).unwrap();
        let file = String::from_utf8(dest.read("thml/John.xml").unwrap()).unwrap();
        assert!(
            file.contains(r#"<scripture passage="John 3.16" parsed="|John|3|16|0|0""#),
            "{file}"
        );
        assert!(file.contains(r#"parsed="|John|3|36|4|2""#), "{file}");
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let john = logoxide::bible_data().books().search_usfm("JHN").unwrap();
        assert_eq!(
            package.source().chapter_verses(john).unwrap(),
            [0, 0, 36, 2]
        );
        let book = package.source().book(john).unwrap();
        assert_eq!((book.spans[1].last_chapter, book.spans[1].last), (4, 2));
    }

    #[test]
    fn repositories_are_named() {
        assert_eq!(
            repository_name("https://crosswire.org/ftpmirror/pub/sword/raw/"),
            "crosswire"
        );
        assert_eq!(
            repository_name("https://www.sword.example.org/repo/"),
            "sword-example-org"
        );
        assert_eq!(catalog_url("nowhere"), None);
    }
}
