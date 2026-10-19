/*!
eBible.org: its list of translations ([`catalog`], `translations.csv`) and a translation's
`<id>_usfm.zip` ([`import`]), one USFM file per book, with its copyright in `copr.htm` (the
package's license file). Its versions are update dates, which follow its updates
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    license::Rights,
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};

use crate::{Made, external, finish, import_id, package_id, unzip};

/// Where eBible.org serves a translation's USFM
pub fn zip_url(id: &str) -> String {
    format!("https://ebible.org/Scriptures/{id}_usfm.zip")
}

/// eBible.org's `translations.csv`: the translations it lets anyone download, each an
/// `ebible:<id>` import of its zip
pub fn catalog(text: &str) -> Result<Catalog> {
    let rows = csv(text.trim_start_matches('\u{feff}'));
    let mut rows = rows.into_iter();
    let header = rows
        .next()
        .ok_or_else(|| LogoxideError::Catalog(String::from("an empty translations.csv")))?;
    let column = |name: &str| {
        header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| LogoxideError::Catalog(format!("translations.csv has no {name} column")))
    };
    let [
        language,
        id,
        language_name,
        title,
        description,
        copyright,
        short_title,
    ] = [
        "languageCode",
        "translationId",
        "languageNameInEnglish",
        "title",
        "description",
        "Copyright",
        "shortTitle",
    ]
    .map(column);
    let [
        language,
        id,
        language_name,
        title,
        description,
        copyright,
        short_title,
    ] = [
        language?,
        id?,
        language_name?,
        title?,
        description?,
        copyright?,
        short_title?,
    ];
    let [redistributable, downloadable, direction, updated] = [
        "Redistributable",
        "downloadable",
        "textDirection",
        "UpdateDate",
    ]
    .map(column);
    let [redistributable, downloadable, direction, updated] =
        [redistributable?, downloadable?, direction?, updated?];
    // Its script by name (`Latin`), in newer lists
    let script = column("script").ok();
    let get = |row: &[String], i: usize| row.get(i).map(|s| s.trim().to_string());
    let yes = |row: &[String], i: usize| get(row, i).is_some_and(|v| v == "True");
    let mut entries = vec![];
    for row in rows {
        if !yes(&row, downloadable) || !yes(&row, redistributable) {
            continue;
        }
        let (Some(code), Some(ebible_id)) = (get(&row, language), get(&row, id)) else {
            continue;
        };
        let (tag, raw) = language::normalize_language(&code);
        let copyright = get(&row, copyright).filter(|c| !c.is_empty());
        // The list gives only a statement (the license is in the download's copr.htm, which
        // its import reads), and lists only what may be redistributed
        let license = (copyright.as_deref())
            .and_then(crate::license::license_from_text)
            .unwrap_or_else(|| String::from("LicenseRef-eBible"));
        let name = get(&row, title)
            .filter(|t| !t.is_empty())
            .or_else(|| get(&row, short_title))
            .unwrap_or_else(|| ebible_id.clone());
        let downloads = vec![Download {
            name: String::from("zip"),
            urls: vec![zip_url(&ebible_id)],
        }];
        entries.push(CatalogEntry {
            // Prefixed: eBible.org's ids (`eng-web`) could be other catalogs' too, and the
            // first of an id wins when catalogs merge
            id: format!("ebible-{}", package_id(&ebible_id)),
            abbreviation: Some(abbreviation(&ebible_id, &code)),
            name,
            aliases: vec![],
            language_name: get(&row, language_name).filter(|n| !n.is_empty()).or(raw),
            script: (script.and_then(|i| get(&row, i)))
                .and_then(|s| language::script_code(&s))
                .or_else(|| language::script_of_tag(&tag)),
            language: tag,
            description: get(&row, description).filter(|d| !d.is_empty()),
            license: Some(license),
            rights: Rights {
                redistributable: Some(true),
                ..Default::default()
            },
            copyright,
            version: get(&row, updated).filter(|d| !d.is_empty()),
            kind: PackageKind::Bible,
            direction: match get(&row, direction).as_deref() {
                Some("rtl") => Direction::Rtl,
                _ => Direction::Ltr,
            },
            source: external("ebible", &ebible_id, true, downloads),
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
    logoxide::catalog::known_works(&mut entries);
    Ok(Catalog {
        name: String::from("eBible.org"),
        entries,
        works: vec![],
        warnings: vec![],
    })
}

/**
An eBible.org translation's `<id>_usfm.zip` (from [`zip_url`]) as a package in `dest`:
`logoxide.toml`, `usfm/` (every book's file), and `copr.htm` (its copyright, the license file). The
package's id, name, and language come from `entry`, and eBible.org's update date is the upstream
version. Its license is read from `copr.htm` (a Creative Commons license by its link or name, or
the public domain; [`crate::license::license_from_text`]), else the entry's. Checked by opening
it: a zip without books is an error
*/
pub fn import(entry: &CatalogEntry, zip: &[u8], dest: &dyn Storage) -> Result<()> {
    let id = import_id(entry, "ebible", "eBible.org")?;
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
    let mut entry = entry.clone();
    if let Some(license) = (license_file.as_deref())
        .and_then(|file| staged.read_string(file).ok())
        .and_then(|text| crate::license::license_from_text(&text))
    {
        entry.license = Some(license);
    }
    let made = Made {
        format: "usfm",
        license_file,
        publisher: "eBible.org",
        publisher_name: None,
        homepage: format!("https://ebible.org/{id}/"),
        versification: None,
        keys: None,
        note: None,
    };
    finish(&entry, made, staged, dest)
}

/// What references could name an eBible.org translation by: its id without the language code
/// (`eng-asv` is `ASV`, `engwebp` is `WEBP`)
fn abbreviation(id: &str, language: &str) -> String {
    let rest = id
        .strip_prefix(language)
        .map(|r| r.trim_start_matches(['-', '_']))
        .filter(|r| !r.is_empty())
        .unwrap_or(id);
    rest.to_uppercase()
}

/// Rows of a CSV file: fields split by commas, quoted fields with `""` for a quote (and
/// commas or line breaks inside)
fn csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::tests::zip;
    use logoxide::{
        catalog::{CatalogSource, EBIBLE_CATALOG},
        package::{Features, Package},
    };

    pub(crate) const EBIBLE: &str = "\u{feff}\"languageCode\",\"translationId\",\"languageName\",\"languageNameInEnglish\",\"dialect\",\"homeDomain\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"publicationURL\",\"OTbooks\",\"OTchapters\",\"OTverses\",\"NTbooks\",\"NTchapters\",\"NTverses\",\"DCbooks\",\"DCchapters\",\"DCverses\",\"FCBHID\",\"Certified\",\"inScript\",\"swordName\",\"rodCode\",\"textDirection\",\"downloadable\",\"font\",\"shortTitle\",\"PODISBN\",\"script\",\"sourceDate\"
\"eng\",\"eng-asv\",\"English\",\"English\",\"Archaic American\",\"ebible.org\",\"American Standard Version (1901)\",\"The American Standard Version of the Holy Bible, first published in 1901.\",\"True\",\"public domain\",\"2026-10-08\",\"http://ebible.org/eng-asv/\",\"39\",\"929\",\"23145\",\"27\",\"260\",\"7957\",\"0\",\"0\",\"0\",\"ENGASV\",\"True\",\"\",\"engASV1901eb\",\"00025\",\"ltr\",\"True\",\"Gentium\",\"American Standard Version (1901)\",\"\",\"Latin\",\"2026-10-08\"
\"eng\",\"engamp\",\"English\",\"English\",\"American\",\"eBible.org\",\"The Amplified Bible (2015)\",\"The Amplified Bible, 2015\",\"False\",\"Copyright © 2015 The Lockman Foundation\",\"2024-07-27\",\"\",\"39\",\"929\",\"23145\",\"27\",\"260\",\"7958\",\"0\",\"0\",\"0\",\"\",\"False\",\"\",\"\",\"\",\"ltr\",\"False\",\"\",\"Amplified\",\"\",\"Latin\",\"2024-07-27\"
\"heb\",\"hebwlc\",\"Hebrew\",\"Hebrew, Ancient\",\"\",\"ebible.org\",\"Westminster Leningrad Codex\",\"The \"\"WLC\"\", with vowels, and a comma, here\",\"True\",\"Copyright © 2020 J. Alan Groves Center\",\"2020-01-01\",\"\",\"39\",\"929\",\"23213\",\"0\",\"0\",\"0\",\"0\",\"0\",\"0\",\"\",\"True\",\"\",\"\",\"\",\"rtl\",\"True\",\"\",\"WLC\",\"\",\"Hebrew\",\"2020-01-01\"
";

    #[test]
    fn reads_ebibles_list() {
        let catalog = crate::catalog::parse_text(EBIBLE_CATALOG, EBIBLE).unwrap();
        // The Amplified can't be downloaded
        assert_eq!(catalog.entries.len(), 2);
        let asv = &catalog.entries[0];
        assert_eq!(
            (asv.id.as_str(), asv.short_name(), asv.language.as_str()),
            ("ebible-eng-asv", "ASV", "en")
        );
        assert_eq!(asv.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(
            asv.source,
            CatalogSource::External {
                import: "ebible:eng-asv".into(),
                upstream: Some("https://ebible.org/Scriptures/eng-asv_usfm.zip".into()),
                tracks_versions: true,
                downloads: vec![Download {
                    name: "zip".into(),
                    urls: vec!["https://ebible.org/Scriptures/eng-asv_usfm.zip".into()]
                }],
            }
        );
        assert_eq!(asv.version.as_deref(), Some("2026-10-08"));
        // The work logoxide's table knows the import to be
        assert_eq!(asv.work.as_deref(), Some("asv"));
        let wlc = &catalog.entries[1];
        assert_eq!(wlc.abbreviation.as_deref(), Some("WLC"));
        assert_eq!(wlc.direction, Direction::Rtl);
        // The list's script, by name
        assert_eq!(
            (asv.script.as_deref(), wlc.script.as_deref()),
            (Some("Latn"), Some("Hebr"))
        );
        assert_eq!(wlc.license.as_deref(), Some("LicenseRef-eBible"));
        // The list has only what may be redistributed
        assert_eq!(wlc.rights.redistributable, Some(true));
        assert_eq!(
            wlc.description.as_deref(),
            Some("The \"WLC\", with vowels, and a comma, here")
        );
        // Found by its import name, and by the prefix alone nothing else
        assert_eq!(catalog.find_import("ebible:eng-asv"), Some(asv));
        assert!(catalog.find_import("fetch:eng-asv").is_none());
        assert!(catalog.find_import("eng-asv").is_none());
    }

    #[test]
    fn imports_an_ebible_zip() {
        let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
        let catalog = catalog(csv).unwrap();
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
        assert_eq!(
            crate::downloads(entry).unwrap(),
            [Download {
                name: "zip".into(),
                urls: vec![zip_url("eng-asv")]
            }]
        );
        let dest = MemoryStorage::new();
        crate::tests::build_twice(entry, &[("zip".into(), bytes)], &dest);
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
                info.short_name(),
                info.version.to_string()
            ),
            ("ebible-eng-asv", "ASV", "1.0.0".to_string())
        );
        let upstream = info.upstream.as_ref().unwrap();
        assert_eq!(
            (
                upstream.url.as_deref(),
                upstream.import.as_deref(),
                upstream.version.as_deref()
            ),
            (
                Some("https://ebible.org/Scriptures/eng-asv_usfm.zip"),
                Some("ebible:eng-asv"),
                Some("2026-10-08")
            )
        );
        assert_eq!(info.license_file.as_deref(), Some("copr.htm"));
        assert_eq!(info.license.as_deref(), Some("LicenseRef-PublicDomain"));
        // The work logoxide's table knows ebible:eng-asv to be
        assert_eq!(info.work.as_deref(), Some("asv"));
        assert_eq!(package.books().len(), 1);
        assert!(import(entry, &zip(&[("copr.htm", "")]), &MemoryStorage::new()).is_err());
        // The license is read from copr.htm, as its link gives it; eBible.org's list says it may
        // be redistributed, and the license says the rest
        let licensed = MemoryStorage::new();
        let copr = "<p>Copyright © 2009 Someone. This translation is made available to you under the terms of the <a href='http://creativecommons.org/licenses/by-nc-nd/4.0/'>Creative Commons Attribution-Noncommercial-No Derivatives license 4.0.</a></p>";
        let book = "\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n";
        import(
            entry,
            &zip(&[("73-JHNeng-asv.usfm", book), ("copr.htm", copr)]),
            &licensed,
        )
        .unwrap();
        let package = Package::open(std::sync::Arc::new(licensed), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(info.license.as_deref(), Some("CC-BY-NC-ND-4.0"));
        let rights = info.text_rights();
        assert_eq!(
            (
                rights.redistributable,
                rights.commercial,
                rights.derivatives
            ),
            (
                Some(true),
                Some(false),
                Some(logoxide::license::Derivatives::None)
            )
        );
        // Not converted: eBible.org's USFM is kept as it is
        assert!(!info.upstream.as_ref().unwrap().converted);
        assert!(import(entry, b"not a zip", &MemoryStorage::new()).is_err());
        // Another publisher's entry isn't eBible.org's
        let mut other = entry.clone();
        other.source = external("ccel", "asv", false, vec![]);
        let error = import(&other, &zip(&[]), &MemoryStorage::new()).unwrap_err();
        assert!(
            error.to_string().contains("isn't from eBible.org"),
            "{error}"
        );
    }
}
