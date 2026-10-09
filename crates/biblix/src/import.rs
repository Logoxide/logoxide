/*!
Making packages from other publishers' downloads, so a Bible that isn't a biblix package can still
be installed: its files with a `biblix.toml` around them, which a vault then uses as a path
package (or which can be pushed anywhere as a git repository).

- **eBible.org** ([`ebible`]): a translation's `<id>_usfm.zip`, one USFM file per book, with its
  copyright in `copr.htm` (the package's license file)
- **fetch.bible** ([`fetch_bible`]): a translation's USX, a file per book
  ([`fetch_bible_book_url`]), with its attribution in `COPYRIGHT.txt`

The catalog entry ([`crate::catalog`]) gives the package's name, language, license, and version.
*/

use std::io::Read;

use crate::{
    catalog::{CatalogEntry, CatalogSource},
    error::{BiblixError, Result},
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
    let bad = |e: &dyn std::fmt::Display| BiblixError::Import(format!("not a .zip: {e}"));
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
`biblix.toml`, `usfm/` (every book's file), and `copr.htm` (its copyright, the license file). The
package's id, name, language, and license come from `entry`, its version from eBible.org's update
date. Checked by opening it: a zip without books is an error
*/
pub fn ebible(entry: &CatalogEntry, zip: &[u8], dest: &dyn Storage) -> Result<()> {
    let CatalogSource::Ebible { id } = &entry.source else {
        return Err(BiblixError::Import(format!(
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
book's code and the bytes at [`fetch_bible_book_url`]): `biblix.toml`, `usx/`, and
`COPYRIGHT.txt` (its attribution and license, the license file)
*/
pub fn fetch_bible(
    entry: &CatalogEntry,
    books: &[(String, Vec<u8>)],
    dest: &dyn Storage,
) -> Result<()> {
    let CatalogSource::FetchBible { id, .. } = &entry.source else {
        return Err(BiblixError::Import(format!(
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
            return Err(BiblixError::Import(format!(
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
        upstream: format!("https://v1.fetch.bible/bibles/{id}/"),
    };
    finish(entry, made, staged, dest)
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

/// Writes the package's `biblix.toml`, checks it opens with books, and copies it into `dest`
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
        upstream: Some(made.upstream),
        versification: String::from("own"),
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
    let text = toml::to_string(&manifest).map_err(|e| BiblixError::Import(e.to_string()))?;
    let header = format!("# Imported from {} by biblix\n", made.publisher);
    staged.write(MANIFEST, format!("{header}{text}").as_bytes())?;
    let staged = std::sync::Arc::new(staged);
    let package = Package::open(staged.clone(), Features::default())?;
    if package.books().is_empty() {
        return Err(BiblixError::Import(format!("{}: no books", entry.id)));
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
                "biblix.toml",
                "copr.htm",
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
            ["COPYRIGHT.txt", "biblix.toml", "usx/JHN.usx"]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        assert_eq!(package.books().len(), 1);
        // A page in place of a book (fetch.bible answers unknown paths with its site)
        let page = [("jhn".to_string(), b"<!DOCTYPE html>".to_vec())];
        assert!(fetch_bible(&from_fetch, &page, &MemoryStorage::new()).is_err());
        assert!(ebible(entry, b"not a zip", &MemoryStorage::new()).is_err());
    }
}
