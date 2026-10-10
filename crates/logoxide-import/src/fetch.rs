/*!
fetch.bible: its manifest of translations ([`catalog`], `manifest.json`) and a translation's USX
([`import`]), a file per book ([`book_url`]), with its attribution in `COPYRIGHT.txt`. Its
versions are years, which don't follow its updates: an update is found by downloading
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    license::{Derivatives, Rights},
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};

use crate::{Made, external, finish, import_id, package_id};

/// Where fetch.bible serves a translation (its books are below it)
pub fn url(id: &str) -> String {
    format!("https://v1.fetch.bible/bibles/{id}/")
}

/// Where fetch.bible serves a translation's book as USX (`code` as its catalog lists it: `jhn`)
pub fn book_url(id: &str, code: &str) -> String {
    format!(
        "https://v1.fetch.bible/bibles/{id}/usx/{}.usx",
        code.to_ascii_lowercase()
    )
}

/// fetch.bible's `manifest.json`: its translations, with their books and licenses, each a
/// `fetch:<id>` import downloading every book's USX (named by its lowercase code)
pub fn catalog(text: &str) -> Result<Catalog> {
    use serde_json::Value;
    let bad = |message: &str| LogoxideError::Catalog(format!("manifest.json: {message}"));
    let json: Value = serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
    let bibles = json
        .get("bibles")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("no bibles"))?;
    let ordered: Vec<String> = (json.get("books_ordered").and_then(Value::as_array))
        .map(|books| {
            books
                .iter()
                .filter_map(|b| Some(b.as_str()?.to_string()))
                .collect()
        })
        .unwrap_or_default();
    // The first 39 are the Old Testament's
    let (ot, nt) = ordered.split_at(ordered.len().min(39));
    let languages = json.get("languages");
    let text_of = |v: Option<&Value>| {
        v.and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    let books_of = |v: Option<&Value>, all: &[String]| match v {
        Some(Value::Bool(true)) => all.to_vec(),
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|b| Some(b.as_str()?.to_ascii_lowercase()))
            .collect(),
        _ => vec![],
    };
    let mut entries = vec![];
    for (id, bible) in bibles {
        let mut books = books_of(bible.get("books_ot"), ot);
        books.extend(books_of(bible.get("books_nt"), nt));
        if books.is_empty() {
            continue;
        }
        let names = bible.get("name");
        let name_of = |key: &str| text_of(names.and_then(|n| n.get(key)));
        let code = id.split('_').next().unwrap_or(id).to_string();
        let language_entry = languages.and_then(|l| l.get(&code));
        let (tag, raw) = language::normalize_language(&code);
        let copyright = bible.get("copyright");
        let (license, rights) = (copyright.and_then(|c| c.get("licenses")))
            .and_then(Value::as_array)
            .and_then(|l| l.first())
            .map(|l| license_of(l.get("license"), text_of(l.get("url"))))
            .map_or((None, Default::default()), |(l, r)| (Some(l), r));
        let abbreviation = name_of("english_abbrev")
            .or_else(|| name_of("local_abbrev"))
            .unwrap_or_else(|| id.to_uppercase());
        let english = name_of("english");
        let local = name_of("local");
        // Every book's USX, each download named by its code; the upstream URL is the
        // translation's folder, which they're below
        let downloads: Vec<Download> = (books.iter())
            .map(|code| Download {
                name: code.clone(),
                urls: vec![book_url(id, code)],
            })
            .collect();
        let source = match external("fetch", id, false, downloads) {
            logoxide::catalog::CatalogSource::External {
                import,
                tracks_versions,
                downloads,
                ..
            } => logoxide::catalog::CatalogSource::External {
                import,
                upstream: Some(url(id)),
                tracks_versions,
                downloads,
            },
            other => other,
        };
        entries.push(CatalogEntry {
            // Prefixed, as every import's id is: fetch.bible's ids are other catalogs' too
            id: format!("fetch-{}", package_id(id)),
            name: english
                .clone()
                .or_else(|| local.clone())
                .unwrap_or_else(|| id.clone()),
            aliases: local
                .filter(|l| Some(l) != english.as_ref())
                .into_iter()
                .collect(),
            abbreviation,
            language_name: text_of(language_entry.and_then(|l| l.get("english"))).or(raw),
            script: language::script_of_tag(&tag),
            language: tag,
            description: None,
            license,
            rights,
            copyright: text_of(copyright.and_then(|c| c.get("attribution"))),
            version: (bible.get("year").and_then(Value::as_u64)).map(|y| y.to_string()),
            kind: PackageKind::Bible,
            direction: match bible.get("direction").and_then(Value::as_str) {
                Some("rtl") => Direction::Rtl,
                _ => Direction::Ltr,
            },
            source,
            path: None,
            releases: vec![],
            keywords: vec![],
            categories: vec![],
            about_books: vec![],
            work: None,
            edition: None,
            edition_year: None,
        });
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    logoxide::catalog::known_works(&mut entries);
    Ok(Catalog {
        name: String::from("fetch.bible"),
        entries,
        warnings: vec![],
    })
}

/**
A fetch.bible license as SPDX: `cc-by-sa` with its URL's version is `CC-BY-SA-4.0`, `public` is
public domain, and its own terms (an object of restrictions: `forbid_commercial`,
`forbid_derivatives` (`true`, or `same-license`), `forbid_attributionless`) are
`LicenseRef-fetch.bible`, with the rights they give
*/
fn license_of(license: Option<&serde_json::Value>, url: Option<String>) -> (String, Rights) {
    let license = match license {
        Some(serde_json::Value::Object(terms)) => {
            let forbids = |key: &str| terms.get(key).and_then(serde_json::Value::as_bool);
            let derivatives = match terms.get("forbid_derivatives") {
                Some(serde_json::Value::Bool(true)) => Some(Derivatives::None),
                Some(serde_json::Value::Bool(false)) => Some(Derivatives::Allowed),
                Some(serde_json::Value::String(s)) if s == "same-license" => {
                    Some(Derivatives::ShareAlike)
                }
                _ => None,
            };
            let rights = Rights {
                // fetch.bible publishes only what may be shared
                redistributable: Some(true),
                derivatives,
                commercial: forbids("forbid_commercial").map(|f| !f),
                attribution: forbids("forbid_attributionless"),
            };
            return (String::from("LicenseRef-fetch.bible"), rights);
        }
        Some(license) => license.as_str(),
        None => None,
    };
    let license = match license {
        Some("public") => String::from("LicenseRef-PublicDomain"),
        Some(cc) if cc.starts_with("cc-") => {
            let version = (url.as_deref())
                .and_then(|u| u.trim_end_matches('/').rsplit('/').next())
                .filter(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.'))
                .unwrap_or("4.0")
                .to_string();
            format!("{}-{version}", cc.to_uppercase())
        }
        _ => String::from("LicenseRef-fetch.bible"),
    };
    (license, Rights::default())
}

/**
A fetch.bible translation as a package in `dest`, from each of its books' USX (`books`: each
book's code, as the entry's downloads name them, and the bytes at [`book_url`]): `logoxide.toml`,
`usx/`, and `COPYRIGHT.txt` (its attribution and license, the license file)
*/
pub fn import(entry: &CatalogEntry, books: &[(String, Vec<u8>)], dest: &dyn Storage) -> Result<()> {
    let id = import_id(entry, "fetch", "fetch.bible")?;
    let staged = MemoryStorage::new();
    for (code, data) in books {
        if !data.starts_with(b"<?xml")
            && !data.starts_with(b"<usx")
            && !data.starts_with(b"\xef\xbb\xbf")
        {
            return Err(LogoxideError::Import(format!(
                "{id}: {code} isn't USX (was it downloaded from {}?)",
                book_url(id, code)
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
        versification: None,
        keys: None,
    };
    finish(entry, made, staged, dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use logoxide::{
        catalog::FETCH_BIBLE_CATALOG,
        package::{Features, Package},
    };

    #[test]
    fn reads_fetch_bibles_manifest() {
        let json = r#"{"bibles": {
            "eng_bsb": {"name": {"local": "Berean Standard Bible", "local_abbrev": "BSB", "english": "Berean Standard Bible", "english_abbrev": "BSB"},
                "year": 2024, "direction": "ltr", "copyright": {"licenses": [{"license": "public", "url": ""}], "attribution": "BSB Publishing, LLC"},
                "books_ot": true, "books_nt": true},
            "spa_x": {"name": {"local": "Biblia", "local_abbrev": "", "english": "A Bible", "english_abbrev": "AB"},
                "year": 2020, "direction": "ltr", "copyright": {"licenses": [{"license": "cc-by-sa", "url": "https://creativecommons.org/licenses/by-sa/3.0/"}], "attribution": "Someone"},
                "books_ot": [], "books_nt": ["jhn", "rom"]},
            "heb_y": {"name": {"local": "", "local_abbrev": "", "english": "Restricted", "english_abbrev": "R"},
                "year": 2020, "direction": "rtl", "copyright": {"licenses": [{"license": {"forbid_commercial": true}, "url": ""}], "attribution": ""},
                "books_ot": ["gen"], "books_nt": false},
            "none_z": {"name": {"english": "Nothing"}, "books_ot": [], "books_nt": false}
        },
        "languages": {"eng": {"english": "English"}, "spa": {"english": "Spanish"}},
        "books_ordered": ["gen","exo","lev","num","deu","jos","jdg","rut","1sa","2sa","1ki","2ki","1ch","2ch","ezr","neh","est","job","psa","pro","ecc","sng","isa","jer","lam","ezk","dan","hos","jol","amo","oba","jon","mic","nam","hab","zep","hag","zec","mal","mat","mrk","luk","jhn","act","rom","1co","2co","gal","eph","php","col","1th","2th","1ti","2ti","tit","phm","heb","jas","1pe","2pe","1jn","2jn","3jn","jud","rev"]}"#;
        let catalog = crate::catalog::parse_text(FETCH_BIBLE_CATALOG, json).unwrap();
        assert_eq!(
            catalog.entries.len(),
            3,
            "a translation without books is left out"
        );
        let bsb = &catalog.entries[0];
        assert_eq!(
            (
                bsb.id.as_str(),
                bsb.abbreviation.as_str(),
                bsb.language.as_str()
            ),
            ("fetch-eng_bsb", "BSB", "en")
        );
        assert_eq!(bsb.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(bsb.source.import_name(), Some("fetch:eng_bsb"));
        assert_eq!(
            bsb.source.upstream_url(),
            Some("https://v1.fetch.bible/bibles/eng_bsb/")
        );
        assert!(!bsb.source.versions_track_updates());
        // Every book, by its code
        let downloads = bsb.source.downloads();
        assert_eq!((downloads.len(), downloads[42].name.as_str()), (66, "jhn"));
        assert_eq!(
            downloads[42].urls,
            ["https://v1.fetch.bible/bibles/eng_bsb/usx/jhn.usx"]
        );
        // The work logoxide's table knows the import to be
        assert_eq!(bsb.work.as_deref(), Some("bsb"));
        let x = catalog
            .entries
            .iter()
            .find(|e| e.id == "fetch-spa_x")
            .unwrap();
        assert_eq!(x.license.as_deref(), Some("CC-BY-SA-3.0"));
        assert_eq!(x.aliases, ["Biblia"]);
        assert_eq!(x.language_name.as_deref(), Some("Spanish"));
        let y = catalog
            .entries
            .iter()
            .find(|e| e.id == "fetch-heb_y")
            .unwrap();
        assert_eq!(
            (y.license.as_deref(), y.direction),
            (Some("LicenseRef-fetch.bible"), Direction::Rtl)
        );
        // Its own terms, as rights
        assert_eq!(
            y.rights,
            Rights {
                redistributable: Some(true),
                derivatives: None,
                commercial: Some(false),
                attribution: None
            }
        );
        assert_eq!(x.text_rights().derivatives, Some(Derivatives::ShareAlike));
    }

    #[test]
    fn imports_fetch_bibles_books() {
        let json = r#"{"bibles": {"eng_asv": {"name": {"english": "American Standard Version", "english_abbrev": "ASV"},
            "year": 1901, "copyright": {"licenses": [{"license": "public", "url": ""}], "attribution": "Someone"},
            "books_ot": [], "books_nt": ["jhn"]}},
            "languages": {"eng": {"english": "English"}},
            "books_ordered": ["gen","exo","lev","num","deu","jos","jdg","rut","1sa","2sa","1ki","2ki","1ch","2ch","ezr","neh","est","job","psa","pro","ecc","sng","isa","jer","lam","ezk","dan","hos","jol","amo","oba","jon","mic","nam","hab","zep","hag","zec","mal","mat","mrk","luk","jhn"]}"#;
        let catalog = catalog(json).unwrap();
        let entry = &catalog.entries[0];
        let usx = r#"<?xml version="1.0" encoding="utf-8"?><usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c" sid="JHN 3"/><para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world.<verse eid="JHN 3:16"/></para></usx>"#;
        let dest = MemoryStorage::new();
        crate::build(entry, &[("jhn".into(), usx.as_bytes().to_vec())], &dest).unwrap();
        assert_eq!(
            dest.walk("").unwrap(),
            ["COPYRIGHT.txt", "logoxide.toml", "usx/JHN.usx"]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        assert_eq!(package.books().len(), 1);
        let info = package.info();
        assert_eq!(info.work.as_deref(), Some("asv"));
        let upstream = info.upstream.as_ref().unwrap();
        assert_eq!(
            (upstream.import.as_deref(), upstream.url.as_deref()),
            (
                Some("fetch:eng_asv"),
                Some("https://v1.fetch.bible/bibles/eng_asv/")
            )
        );
        assert!(!upstream.converted);
        // A page in place of a book (fetch.bible answers unknown paths with its site)
        let page = [("jhn".to_string(), b"<!DOCTYPE html>".to_vec())];
        let error = import(entry, &page, &MemoryStorage::new()).unwrap_err();
        assert!(error.to_string().contains("isn't USX"), "{error}");
    }
}
