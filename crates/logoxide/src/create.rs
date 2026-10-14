/*!
Making a new package from a single file (`lx new --from book.epub`): the file in its format's
folder, with a `logoxide.toml` around it. What the file says about itself fills in the manifest,
and anything given ([`NewPackage`]) wins over it:

- **EPUB** (a book): its title, subtitle (the description), every author (`dc:creator`, leaving
  out editors, illustrators, and other roles), language, and publisher, from its package document
  (the `.opf` its `META-INF/container.xml` names)
- **USX, USFM, OSIS, ThML** (a Bible, or a book of one): nothing is read from them yet, so a name
  and a language must be given

A collection ([`new_collection`]) has no file: its packages are its dependencies.
*/

#[cfg(feature = "read")]
use crate::manifest::FormatEntry;
use crate::{
    error::{LogoxideError, Result},
    manifest::{MANIFEST, Manifest, Package as Info, PackageKind},
    storage::Storage,
};
#[cfg(feature = "read")]
pub use logoxide_text::metadata::{FileMetadata, epub_metadata, opf_metadata};

/// What to put in a new package's `[package]`; each one given wins over the file's metadata
#[derive(Clone, Debug, Default)]
pub struct NewPackage {
    pub id: Option<String>,
    pub name: Option<String>,
    pub abbreviation: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub version: Option<semver::Version>,
    pub kind: Option<PackageKind>,
    pub language: Option<String>,
    pub license: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    /// The books it's mainly about, by USFM code or name (`ROM`, `Romans`)
    pub about_books: Vec<String>,
}

impl NewPackage {
    /// `about_books` as USFM codes (`Romans` is `ROM`)
    fn about_book_codes(&self) -> Result<Vec<String>> {
        (self.about_books.iter())
            .map(|name| {
                crate::manifest::book_code(name).ok_or_else(|| {
                    LogoxideError::Manifest(format!(
                        "about-books: `{name}` is no book of the Bible"
                    ))
                })
            })
            .collect()
    }
}

/// The format a file is in, from its extension (and, for `.xml`, its root element), and the kind
/// of package it makes
pub fn file_format(name: &str, bytes: &[u8]) -> Option<(&'static str, PackageKind)> {
    let lower = name.to_ascii_lowercase();
    let extension = lower.rsplit_once('.').map(|(_, e)| e)?;
    Some(match extension {
        "epub" => ("epub", PackageKind::Book),
        "usx" => ("usx", PackageKind::Bible),
        "usfm" | "sfm" => ("usfm", PackageKind::Bible),
        "osis" => ("osis", PackageKind::Bible),
        "thml" => ("thml", PackageKind::Bible),
        "xml" => {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_lowercase();
            match () {
                _ if head.contains("<osis") => ("osis", PackageKind::Bible),
                _ if head.contains("<thml") => ("thml", PackageKind::Bible),
                _ if head.contains("<usx") => ("usx", PackageKind::Bible),
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// A package id from a name: lowercase ASCII letters and digits, words joined by `-`
/// (`God's Word Is Not Bound` → `gods-word-is-not-bound`)
pub fn id_from(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars().filter(|c| *c != '\'' && *c != '’') {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_end_matches('-').to_string()
}

/// An abbreviation from a name: each word's first letter or digit, capitalized
/// (`The Dorean Principle` → `TDP`)
pub fn abbreviation_from(name: &str) -> String {
    (name.split_whitespace())
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .flat_map(char::to_uppercase)
        .collect()
}

/// Writes a new package made from one file into `dest`: its `logoxide.toml`, and the file in its
/// format's folder (`epub/the-dorean-principle.epub`). Gives the manifest
#[cfg(feature = "read")]
pub fn new_from_file(
    file_name: &str,
    bytes: &[u8],
    given: &NewPackage,
    dest: &dyn Storage,
) -> Result<Manifest> {
    let (format, kind) = file_format(file_name, bytes).ok_or_else(|| {
        LogoxideError::Import(format!(
            "{file_name}: not a format logoxide packages (.epub, .usx, .usfm, .osis, .thml, or their .xml)"
        ))
    })?;
    let metadata = match format {
        "epub" => epub_metadata(bytes)?,
        _ => FileMetadata::default(),
    };
    let name = (given.name.clone())
        .or(metadata.title.clone())
        .ok_or_else(|| missing(file_name, "a name (--name)"))?;
    let language = (given.language.clone())
        .or(metadata.language.clone())
        .ok_or_else(|| missing(file_name, "a language (--language, BCP 47: en)"))?;
    let kind = given.kind.unwrap_or(kind);
    let info = Info {
        id: given.id.clone().unwrap_or_else(|| id_from(&name)),
        // What references name: given, else the name's initials; the other kinds get one only
        // when it's given
        abbreviation: (given.abbreviation.clone())
            .or_else(|| kind.needs_abbreviation().then(|| abbreviation_from(&name))),
        name,
        authors: match given.authors.is_empty() {
            true => metadata.authors.clone(),
            false => given.authors.clone(),
        },
        description: given.description.clone().or(metadata.subtitle.clone()),
        aliases: vec![],
        version: (given.version.clone()).unwrap_or_else(|| semver::Version::new(0, 1, 0)),
        kind,
        language,
        language_name: None,
        script: None,
        direction: None,
        license: given.license.clone(),
        rights: None,
        license_file: None,
        publisher: given.publisher.clone().or(metadata.publisher.clone()),
        homepage: given.homepage.clone(),
        repository: given.repository.clone(),
        documentation: None,
        readme: None,
        keywords: given.keywords.clone(),
        categories: given.categories.clone(),
        about_books: given.about_book_codes()?,
        work: None,
        edition: None,
        edition_year: None,
        upstream: None,
        versification: String::from("own"),
        maps_to: None,
        keys: None,
        include: vec![],
    };
    let manifest = Manifest {
        package: Some(info),
        formats: [(
            format.to_string(),
            FormatEntry {
                path: format!("{format}/"),
                feature: None,
            },
        )]
        .into(),
        ..Manifest::default()
    };
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    write(
        &manifest,
        &format!("# Made by logoxide from {base}\n"),
        dest,
    )?;
    dest.write(&format!("{format}/{base}"), bytes)?;
    Ok(manifest)
}

/// Writes a new collection's `logoxide.toml` into `dest`: no text, its packages added afterwards
/// as its dependencies (`lx add --project <its folder>`)
pub fn new_collection(given: &NewPackage, dest: &dyn Storage) -> Result<Manifest> {
    let name = (given.name.clone()).ok_or_else(|| missing("a collection", "a name (--name)"))?;
    // A collection has no language or abbreviation of its own unless given one: left out, its
    // manifest reads with `und`
    let language = given.language.clone().unwrap_or_default();
    let info = Info {
        id: given.id.clone().unwrap_or_else(|| id_from(&name)),
        abbreviation: given.abbreviation.clone(),
        name,
        authors: given.authors.clone(),
        description: given.description.clone(),
        aliases: vec![],
        version: (given.version.clone()).unwrap_or_else(|| semver::Version::new(0, 1, 0)),
        kind: PackageKind::Collection,
        language,
        language_name: None,
        script: None,
        direction: None,
        license: given.license.clone(),
        rights: None,
        license_file: None,
        publisher: given.publisher.clone(),
        homepage: given.homepage.clone(),
        repository: given.repository.clone(),
        documentation: None,
        readme: None,
        keywords: given.keywords.clone(),
        categories: given.categories.clone(),
        about_books: given.about_book_codes()?,
        work: None,
        edition: None,
        edition_year: None,
        upstream: None,
        versification: String::from("own"),
        maps_to: None,
        keys: None,
        include: vec![],
    };
    let manifest = Manifest {
        package: Some(info),
        ..Manifest::default()
    };
    write(
        &manifest,
        "# A collection: its packages are its dependencies (lx add --project <this folder>)\n",
        dest,
    )?;
    Ok(manifest)
}

fn missing(what: &str, needs: &str) -> LogoxideError {
    LogoxideError::Import(format!("{what}: give {needs}; the file doesn't say"))
}

fn write(manifest: &Manifest, header: &str, dest: &dyn Storage) -> Result<()> {
    manifest.validate()?;
    let text = toml::to_string(manifest).map_err(|e| LogoxideError::Manifest(e.to_string()))?;
    // Each format on one line (`epub = { path = "epub/" }`), as manifests are written by hand
    let mut doc: toml_edit::DocumentMut =
        (text.parse()).map_err(|e: toml_edit::TomlError| LogoxideError::Manifest(e.to_string()))?;
    if let Some(formats) = doc.get_mut("formats").and_then(|f| f.as_table_mut()) {
        let inline: Vec<(String, toml_edit::InlineTable)> = (formats.iter())
            .filter_map(|(name, f)| Some((name.to_string(), f.as_table()?.clone())))
            .map(|(name, table)| (name, table.into_inline_table()))
            .collect();
        formats.clear();
        for (name, table) in inline {
            formats.insert(&name, toml_edit::value(table));
        }
    }
    dest.write(MANIFEST, format!("{header}{doc}").as_bytes())
}

#[cfg(all(test, feature = "read"))]
mod tests {
    use super::*;

    #[test]
    fn names_from_titles() {
        assert_eq!(id_from("God's Word Is Not Bound"), "gods-word-is-not-bound");
        assert_eq!(id_from("  Hello, World! 2 "), "hello-world-2");
        assert_eq!(abbreviation_from("The Dorean Principle"), "TDP");
        assert_eq!(abbreviation_from("God's Word Is Not Bound"), "GWINB");
    }

    #[test]
    fn makes_packages() {
        let dest = crate::MemoryStorage::new();
        let given = NewPackage {
            name: Some("Notes".into()),
            language: Some("en".into()),
            ..NewPackage::default()
        };
        let made = new_from_file("dir/JHN.usfm", b"\\id JHN", &given, &dest).unwrap();
        let info = made.package.unwrap();
        assert_eq!((info.id.as_str(), info.kind), ("notes", PackageKind::Bible));
        assert!(dest.read("usfm/JHN.usfm").is_ok());
        // Without a language, it says which flag gives one
        let no_language = NewPackage {
            name: Some("Notes".into()),
            ..NewPackage::default()
        };
        let error = new_from_file("JHN.usfm", b"", &no_language, &dest).unwrap_err();
        assert!(error.to_string().contains("--language"), "{error}");
        assert!(new_from_file("notes.txt", b"", &given, &dest).is_err());

        let collection = crate::MemoryStorage::new();
        let made = new_collection(&given, &collection).unwrap();
        let info = made.package.unwrap();
        assert_eq!(info.kind, PackageKind::Collection);
        assert!(made.formats.is_empty());
        // A collection has no language or abbreviation of its own unless given one: neither is
        // written, and what's written reads back (the language as `und`)
        let unnamed = NewPackage {
            name: Some("My Library".into()),
            ..NewPackage::default()
        };
        let made = new_collection(&unnamed, &collection).unwrap();
        let info = made.package.unwrap();
        assert_eq!(
            (info.abbreviation.as_deref(), info.language.as_str()),
            (None, "")
        );
        let written = collection.read_string(MANIFEST).unwrap();
        assert!(
            !written.contains("language") && !written.contains("abbreviation"),
            "{written}"
        );
        let read = Manifest::parse(&written).unwrap().package.unwrap();
        assert_eq!(
            (read.abbreviation.as_deref(), read.language.as_str()),
            (None, "und")
        );
    }
}
