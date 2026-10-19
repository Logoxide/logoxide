/*!
logoxide-import: packages from other publishers' downloads, so a Bible that isn't a logoxide
package can still be installed: its files with a `logoxide.toml` around them, which a project
uses as an `import` dependency kept in the library (logoxide-app's `importer`), or as a path
package (or which can be pushed anywhere as a git repository).

- **eBible.org** ([`ebible`]): a translation's `<id>_usfm.zip`, one USFM file per book, with its
  copyright in `copr.htm` (the package's license file)
- **fetch.bible** ([`fetch`]): a translation's USX, a file per book, with its attribution in
  `COPYRIGHT.txt`
- **CCEL** ([`ccel`]): a Bible's ThML, the whole Bible in one file, described by its own head
  (title, language, rights, version), with its rights in `COPYRIGHT.txt`
- **Door43** ([`door43`]): a Bible's release, its repository's archive at the tag, a resource
  container whose `manifest.yaml` describes it, its USFM a file per book
- **SWORD** ([`sword`]): a module's zip from its repository, read with sword-modules from
  memory: a Bible's or commentary's entries in their own markup (OSIS, ThML, or GBF), a file per
  book, or a dictionary's as `entries/<module>.xml`, with its conf's copyright and license in
  `COPYRIGHT.txt`
- **Bundles** ([`burrito`]): a Scripture Burrito (`metadata.json`) or a Digital Bible Library
  text release bundle (`metadata.xml`) a translator or publisher hands over, by URL or as a local
  file or folder, its USX, USFM, or USJ checked by the MD5s its metadata lists

Each publisher's list is read into a logoxide [`Catalog`](logoxide::catalog::Catalog) ([`catalog::parse`]), every entry a
[`CatalogSource::External`] naming its import (`ebible:eng-asv`), the download it's made from,
and what to fetch ([`downloads`]); [`build`] makes the package from the bytes. A bundle has no
list: its import names where it is, and [`named_catalog`] makes its entry from that. The package
manager keeps only that, and knows no publisher.

A new import is version 1.0.0 ([`IMPORT_VERSION`]), logoxide's own: the publisher's version (an
update date, a module's `Version`) is kept in `[package.upstream]`, and each update raises the
package's version by what it changes ([`update::compare_import`]), so `lx add` can ask for `^1`
and updates cross the publisher's years.
*/

pub mod burrito;
pub mod catalog;
pub mod ccel;
pub mod door43;
pub mod ebible;
pub mod fetch;
pub mod license;
pub mod sword;
pub mod update;

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, CatalogSource, Download},
    manifest::{Format, FormatEntry, Keys, MANIFEST, Manifest, Package as Info, Upstream},
    package::{Features, Package},
    storage::{MemoryStorage, Storage},
};

pub use update::{
    ImportUpdate, apply_import, compare_import, compare_publisher_versions, import_is_current,
    import_removals, set_version,
};

/// Every publisher's import prefix: what `ebible:<id>` and `fetch:<id>` start with
pub const PREFIXES: [&str; 7] = [
    "ebible", "fetch", "ccel", "sword", "door43", "burrito", "dbl",
];

/// The version a new import has: logoxide's own, which updates raise by what they change
pub const IMPORT_VERSION: semver::Version = semver::Version::new(1, 0, 0);

/// Each file in a `.zip`, by path (folders left out), within logoxide-text's limits
pub fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    Ok(logoxide_text::archive::unzip(bytes)?)
}

/**
What importing `entry` downloads, so every host fetches the same files the same way: eBible.org's
zip, fetch.bible's books (each named by its code), CCEL's ThML, a SWORD module's zip, or a
Door43 release's archive (its URLs tried in turn). Packages from git or a registry aren't imported, which is an error
*/
pub fn downloads(entry: &CatalogEntry) -> Result<Vec<Download>> {
    match &entry.source {
        CatalogSource::External { downloads, .. } => Ok(downloads.clone()),
        CatalogSource::Git { .. } | CatalogSource::Registry { .. } => Err(LogoxideError::Import(
            format!("{} isn't imported: add it", entry.id),
        )),
    }
}

/// The package `entry`'s import makes, written into `dest`, from the bytes of each of its
/// [`downloads`] (by name), whichever publisher it comes from
pub fn build(entry: &CatalogEntry, files: &[(String, Vec<u8>)], dest: &dyn Storage) -> Result<()> {
    let named = |name: &str| {
        (files.iter())
            .find(|(n, _)| n == name)
            .map(|(_, bytes)| bytes.as_slice())
            .ok_or_else(|| LogoxideError::Import(format!("{}: {name} wasn't downloaded", entry.id)))
    };
    match entry.source.prefix() {
        Some("ebible") => ebible::import(entry, named("zip")?, dest),
        Some("fetch") => fetch::import(entry, files, dest),
        Some("ccel") => ccel::import(entry, named("thml")?, dest),
        Some("sword") => sword::import(entry, named("zip")?, dest),
        Some("door43") => door43::import(entry, named("archive")?, dest),
        Some("burrito" | "dbl") => burrito::import(entry, files, dest),
        Some(other) => Err(LogoxideError::Import(format!(
            "{}: no import reads `{other}:` downloads",
            entry.id
        ))),
        None => Err(LogoxideError::Import(format!(
            "{} isn't imported: add it",
            entry.id
        ))),
    }
}

/**
What an import of `entry` writes as its package's `[package.upstream]`: the download it was made
from, its import name, and the publisher's version the entry lists, so an imported package names
the catalog entry it came from ([`Catalog::find_upstream`](logoxide::catalog::Catalog::find_upstream)); none for packages that aren't
imported
*/
pub fn upstream(entry: &CatalogEntry) -> Option<Upstream> {
    let CatalogSource::External {
        import, upstream, ..
    } = &entry.source
    else {
        return None;
    };
    Some(Upstream {
        url: upstream.clone(),
        import: Some(import.clone()),
        version: entry.version.clone(),
        // A SWORD module's entries are made into OSIS or ThML files; the other publishers'
        // files are kept as they are
        converted: entry.source.prefix() == Some("sword"),
        note: None,
    })
}

/**
A catalog of the one entry an import names when no publisher's list is needed to find it: a CCEL
work by its author and name (`ccel:henry/mhc1`), or a bundle by where it is
(`burrito:https://example.org/web.zip`); none for another
*/
pub fn named_catalog(import: &str) -> Option<Catalog> {
    ccel::named_catalog(import).or_else(|| burrito::named_catalog(import))
}

/// The entry's [`CatalogSource::External`] fields for an import of `prefix:id`, with its
/// download(s): what each publisher's list is read into
pub(crate) fn external(
    prefix: &str,
    id: &str,
    tracks_versions: bool,
    downloads: Vec<Download>,
) -> CatalogSource {
    CatalogSource::External {
        import: format!("{prefix}:{id}"),
        upstream: (downloads.first()).and_then(|d| d.urls.first().cloned()),
        tracks_versions,
        downloads,
    }
}

/// A package id from a publisher's (`eng-asv`, `engkjv2006`, `KJV`): lowercase letters, digits,
/// `-`, `_`
pub(crate) fn package_id(id: &str) -> String {
    id.to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            _ => '-',
        })
        .collect()
}

/// A publisher's id after its import prefix (`eng-asv` of `ebible:eng-asv`), checked to be the
/// publisher's
pub(crate) fn import_id<'a>(
    entry: &'a CatalogEntry,
    prefix: &str,
    publisher: &str,
) -> Result<&'a str> {
    match entry.source.prefix() == Some(prefix) {
        true => entry
            .source
            .import_id()
            .ok_or_else(|| LogoxideError::Import(format!("{}: no import id", entry.id))),
        false => Err(LogoxideError::Import(format!(
            "{} isn't from {publisher}",
            entry.id
        ))),
    }
}

/// How an import made its package
pub(crate) struct Made {
    /// The format its books are in, in a folder of that name
    pub(crate) format: &'static str,
    pub(crate) license_file: Option<String>,
    /// Where it came from (`eBible.org`): the package's publisher, unless `publisher_name` says
    pub(crate) publisher: &'static str,
    /// The package's publisher, when the download names it (a bundle's rights holder)
    pub(crate) publisher_name: Option<String>,
    /// None when empty
    pub(crate) homepage: String,
    /// The versification the publisher names (SWORD's conf), else the package's `own`
    pub(crate) versification: Option<String>,
    /// For a dictionary, what its entries are keyed by
    pub(crate) keys: Option<Keys>,
    /// Where the text came from, in words (`[package.upstream] note`)
    pub(crate) note: Option<String>,
}

/// Writes the package's `logoxide.toml`, checks it opens with books (a Bible's or a commentary's)
/// or entries (a dictionary's; a book needs neither), and copies it into `dest`
pub(crate) fn finish(
    entry: &CatalogEntry,
    made: Made,
    staged: MemoryStorage,
    dest: &dyn Storage,
) -> Result<()> {
    use logoxide::language::{script_direction, script_of_tag};
    // The direction is written only where the script doesn't say it
    let script = (entry.script.clone()).or_else(|| script_of_tag(&entry.language));
    let implied = script_direction(script.as_deref());
    let info = Info {
        id: entry.id.clone(),
        name: entry.name.clone(),
        abbreviation: entry.abbreviation.clone(),
        authors: vec![],
        description: entry.description.clone(),
        aliases: entry.aliases.clone(),
        version: IMPORT_VERSION,
        kind: entry.kind,
        language: entry.language.clone(),
        language_name: entry.language_name.clone(),
        script: entry.script.clone(),
        direction: (entry.direction != implied).then_some(entry.direction),
        // A license SPDX can't read (a publisher's odd version) is the publisher's own
        license: (entry.license.clone()).map(|license| {
            match logoxide::license::check_license(&license) {
                Ok(()) => license,
                Err(_) => format!("LicenseRef-{}", made.publisher.replace(' ', "-")),
            }
        }),
        rights: (!entry.rights.is_empty()).then_some(entry.rights),
        license_file: made.license_file,
        publisher: Some(
            made.publisher_name
                .unwrap_or_else(|| made.publisher.to_string()),
        ),
        homepage: (!made.homepage.is_empty()).then_some(made.homepage),
        repository: None,
        documentation: None,
        readme: None,
        keywords: entry.keywords.clone(),
        categories: entry.categories.clone(),
        about_books: entry.about_books.clone(),
        // The work logoxide's table knows the import to be, never guessed from its title
        work: (entry.work.clone()).or_else(|| logoxide::works::of_source(&entry.source)),
        edition: entry.edition.clone(),
        edition_year: entry.edition_year,
        upstream: upstream(entry).map(|u| Upstream {
            note: made.note,
            ..u
        }),
        versification: made.versification.unwrap_or_else(|| String::from("own")),
        maps_to: None,
        keys: made.keys,
        include: vec![],
    };
    // A dictionary's files are in `entries/`; every other format's folder is named for it
    let folder = match Format::from_name(made.format) {
        Some(Format::Dictionary) => "entries",
        _ => made.format,
    };
    let manifest = Manifest {
        package: Some(info),
        formats: [(
            made.format.to_string(),
            FormatEntry {
                path: format!("{folder}/"),
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
    let verses = (package.text_format()).is_some_and(|(f, _)| f.is_verse_text());
    // A book (CCEL's ThML) isn't read as verses or entries: its references are found in it
    let book = entry.kind == logoxide::manifest::PackageKind::Book;
    if verses && package.books().is_empty() {
        return Err(LogoxideError::Import(format!("{}: no books", entry.id)));
    }
    if !verses && !book && package.source().keys().is_empty() {
        return Err(LogoxideError::Import(format!("{}: no entries", entry.id)));
    }
    for path in staged.walk("")? {
        dest.write(&path, &staged.read(&path)?)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    /// A zip of these files, as a publisher's download
    pub(crate) fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        zip_bytes(
            &(files.iter())
                .map(|(path, text)| (*path, text.as_bytes()))
                .collect::<Vec<_>>(),
        )
    }

    pub(crate) fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, bytes) in files {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /**
    Builds `entry`'s import into `dest`, and again elsewhere: the two must be the same bytes, so
    a locked import made again on another machine has the checksum it was locked with
    */
    pub(crate) fn build_twice(
        entry: &logoxide::catalog::CatalogEntry,
        files: &[(String, Vec<u8>)],
        dest: &dyn logoxide::storage::Storage,
    ) {
        super::build(entry, files, dest).unwrap();
        use logoxide::storage::Storage;
        let again = super::MemoryStorage::new();
        super::build(entry, files, &again).unwrap();
        assert_eq!(dest.walk("").unwrap(), again.walk("").unwrap());
        for path in again.walk("").unwrap() {
            assert_eq!(
                dest.read(&path).unwrap(),
                again.read(&path).unwrap(),
                "{path}"
            );
        }
    }

    #[test]
    fn what_isnt_imported_is_said() {
        let catalog = logoxide::catalog::Catalog::parse_toml(
            "[[package]]\nid = \"kjv\"\nname = \"KJV\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\ngit = \"https://example.org/kjv\"\n",
        )
        .unwrap();
        let entry = &catalog.entries[0];
        assert!(super::downloads(entry).is_err());
        assert!(super::build(entry, &[], &super::MemoryStorage::new()).is_err());
        assert_eq!(super::upstream(entry), None);
        assert_eq!(super::package_id("eng-ASV 1901"), "eng-asv-1901");
    }
}
