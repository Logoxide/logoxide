/*!
Other publishers' lists as logoxide catalogs, each entry a [`CatalogSource::External`] saying
what its import downloads:

- **eBible.org's list** (`translations.csv`, [`logoxide::catalog::EBIBLE_CATALOG`]): about
  1,500 translations it publishes as USFM ([`crate::ebible::catalog`])
- **fetch.bible's manifest** (`manifest.json`, [`logoxide::catalog::FETCH_BIBLE_CATALOG`]):
  about 1,400 translations it publishes as USX, a file per book ([`crate::fetch::catalog`])
- **CCEL's Bibles** (its page of them, [`logoxide::catalog::CCEL_CATALOG`]): about 50 Bibles it
  publishes as ThML, a file each. The page gives only titles: the language, rights, and version
  are in each file's head, read when it's imported ([`crate::ccel::catalog`])
- **CCEL's genre indexes** (`https://ccel.org/index/genre/gen-COM`, its 213 commentaries;
  `gen-BIB`, its Bibles; any other genre's works as books): each work a `ccel:<author>/<work>`
  import of its ThML ([`crate::ccel::genre_catalog`])
- **Door43's catalog** (its catalog API, [`crate::door43::CATALOG`]): the Bibles in production,
  each a release's archive ([`crate::door43::catalog`]; the API's JSON is as remembered, since
  it didn't answer when this was written)
- **SWORD repositories** (a repository's `mods.d.tar.gz`, every module's conf;
  [`logoxide::catalog::SWORD_CATALOG`] is CrossWire's): the Bibles and commentaries whose markup
  is OSIS, ThML, or GBF, and the dictionaries in any markup logoxide-markup reads, which aren't
  locked, each a module zip ([`crate::sword::catalog`])

[`parse`] reads any of them, or a `catalog.toml`, by where it came from.
*/

use logoxide::{
    Result,
    catalog::{Catalog, CatalogSource},
};

/// A catalog from a file's bytes and where it came from (a URL or a path): a SWORD repository's
/// when `location` ends in `.tar.gz`, else its text ([`parse_text`])
pub fn parse(location: &str, bytes: &[u8]) -> Result<Catalog> {
    let path = location.split(['?', '#']).next().unwrap_or(location);
    match path.to_ascii_lowercase().ends_with(".tar.gz") {
        true => crate::sword::catalog(location, bytes),
        false => parse_text(location, &String::from_utf8_lossy(bytes)),
    }
}

/// A catalog from a file's text: eBible.org's CSV when `location` ends in `.csv`, fetch.bible's
/// manifest when it ends in `.json`, CCEL's page of Bibles or a genre index of its when it's on
/// ccel.org, Door43's catalog API, else a `catalog.toml` ([`Catalog::parse`])
pub fn parse_text(location: &str, text: &str) -> Result<Catalog> {
    let path = location
        .split(['?', '#'])
        .next()
        .unwrap_or(location)
        .to_ascii_lowercase();
    if crate::door43::is_catalog(location) {
        crate::door43::catalog(text)
    } else if let Some(genre) = crate::ccel::genre_of(location) {
        crate::ccel::genre_catalog(text, genre)
    } else if path.contains("ccel.org/ccel/") {
        crate::ccel::catalog(text)
    } else if path.ends_with(".csv") {
        crate::ebible::catalog(text)
    } else if path.ends_with(".json") {
        crate::fetch::catalog(text)
    } else {
        Catalog::parse(location, text)
    }
}

/// Whether `location` is one of the publishers' lists this crate reads, by its form (as
/// [`parse`] tells them apart), rather than a `catalog.toml`
pub fn is_publishers(location: &str) -> bool {
    let path = location
        .split(['?', '#'])
        .next()
        .unwrap_or(location)
        .to_ascii_lowercase();
    path.contains("ccel.org/ccel/")
        || path.contains("ccel.org/index/genre/")
        || crate::door43::is_catalog(&path)
        || path.ends_with(".csv")
        || path.ends_with(".json")
        || path.ends_with(".tar.gz")
}

/// The entries' sources as `lx search` and hosts show them: a short name for each publisher
/// (`eBible.org`, `fetch.bible`, `CCEL`, `SWORD`, `Door43`), by an entry's import prefix
pub fn publisher_name(source: &CatalogSource) -> Option<&'static str> {
    Some(match source.prefix()? {
        "ebible" => "eBible.org",
        "fetch" => "fetch.bible",
        "ccel" => "CCEL",
        "sword" => "SWORD",
        "door43" => "Door43",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_are_told_apart_by_location() {
        let toml = "[[package]]\nid = \"kjv\"\nname = \"KJV\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\ngit = \"https://example.org/kjv\"\n";
        let catalog = parse("https://example.org/catalog.toml", toml.as_bytes()).unwrap();
        assert_eq!(catalog.entries[0].id, "kjv");
        assert_eq!(publisher_name(&catalog.entries[0].source), None);
        assert!(!is_publishers("https://example.org/catalog.toml"));
        for location in logoxide::catalog::DEFAULT_CATALOGS {
            assert!(is_publishers(location), "{location}");
        }
        // A query doesn't hide the form
        assert!(is_publishers(
            "https://ebible.org/Scriptures/translations.csv?x=1"
        ));
        assert!(parse("translations.csv", b"").is_err());
        assert!(parse("https://ccel.org/ccel/bible/", b"<html></html>").is_err());
        assert!(parse("mods.d.tar.gz", b"not gzip").is_err());
        assert!(is_publishers("https://ccel.org/index/genre/gen-COM"));
        assert!(parse("https://ccel.org/index/genre/gen-COM", b"<html></html>").is_err());
    }
}
