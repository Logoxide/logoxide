//! Making packages from a file (an EPUB, a Bible's USX or USFM) or as a collection, and the Bible
//! references in a package's books (where each is, read from the books; the passages a release
//! cites, from its published file), with the host doing the reading and writing.

use boltffi::*;
use logoxide_core::{create as core, references as core_references};

use crate::{LogoxideError, PackageFile, PackageKind};

/// What a file says about itself (an EPUB's package document), to fill in a new package's form
#[data]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileMetadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    /// Its authors, in order (editors, illustrators, and other roles left out)
    pub authors: Vec<String>,
    /// BCP 47 (`en-US`)
    pub language: Option<String>,
    pub publisher: Option<String>,
    /// Its rights statement, to show (the license is the person's to give)
    pub rights: Option<String>,
}

impl From<core::FileMetadata> for FileMetadata {
    fn from(m: core::FileMetadata) -> Self {
        Self {
            title: m.title,
            subtitle: m.subtitle,
            authors: m.authors,
            language: m.language,
            publisher: m.publisher,
            rights: m.rights,
        }
    }
}

/// What to put in a new package's `[package]`: each one given wins over the file's metadata,
/// and the rest come from it (`id` from the name, `abbreviation` from its initials, `version`
/// 0.1.0)
#[data]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewPackage {
    pub id: Option<String>,
    pub name: Option<String>,
    pub abbreviation: Option<String>,
    /// In order; empty takes the file's
    pub authors: Vec<String>,
    pub description: Option<String>,
    /// A semantic version (`1.0.0`)
    pub version: Option<String>,
    /// By default from the file: an EPUB is a book, USX and USFM a Bible
    pub kind: Option<PackageKind>,
    /// BCP 47 (`en`)
    pub language: Option<String>,
    /// An SPDX expression (`CC0-1.0`); shown, never enforced
    pub license: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
}

impl TryFrom<NewPackage> for core::NewPackage {
    type Error = LogoxideError;

    fn try_from(n: NewPackage) -> Result<Self, LogoxideError> {
        let version = (n.version.as_deref())
            .map(semver::Version::parse)
            .transpose()
            .map_err(|e| LogoxideError::Manifest {
                message: format!("version: {e}"),
            })?;
        Ok(core::NewPackage {
            id: n.id,
            name: n.name,
            abbreviation: n.abbreviation,
            authors: n.authors,
            description: n.description,
            version,
            kind: n.kind.map(Into::into),
            language: n.language,
            license: n.license,
            publisher: n.publisher,
            homepage: n.homepage,
        })
    }
}

/// A passage a release's books cite, and how many times (from `logoxide.references.bin`)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CitedPassage {
    /// An OSIS reference (`Matt.10.8`): `Topos.parseOsis`
    pub passage: String,
    pub count: u32,
}

impl From<core_references::Cited> for CitedPassage {
    fn from(c: core_references::Cited) -> Self {
        Self {
            passage: c.passage,
            count: c.count,
        }
    }
}

/// A Bible reference found in a package's book
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundReference {
    /// The passage, as an OSIS reference (`John.3.16`, `John.3.16-John.3.18`): `Topos.parseOsis`
    pub passage: String,
    /// The file it's in, in the package (`epub/the-dorean-principle.epub`)
    pub file: String,
    /// Where it is in the file: an EPUB CFI range
    pub cfi: String,
    /// The chapter it's in, when the book's table of contents names one
    pub chapter: Option<String>,
    /// Its UTF-16 range in the book's text (each content document's text, a blank line apart)
    pub start: u32,
    pub end: u32,
    /// The 1-based line of the book's text it starts on, and its 1-based UTF-16 column there
    pub line: u32,
    pub column: u32,
    /// Its content document's place in the spine
    pub spine_index: u32,
    /// The line (paragraph) it starts in
    pub line_text: String,
}

impl From<core_references::FoundReference> for FoundReference {
    fn from(r: core_references::FoundReference) -> Self {
        Self {
            passage: r.passage,
            file: r.file,
            cfi: r.cfi,
            chapter: r.chapter,
            start: r.start,
            end: r.end,
            line: r.line,
            column: r.column,
            spine_index: r.spine,
            line_text: r.line_text,
        }
    }
}

/// Every file in a storage, as the host writes them
pub(crate) fn files_of(
    storage: &logoxide_core::MemoryStorage,
) -> Result<Vec<PackageFile>, LogoxideError> {
    let mut out = vec![];
    for path in logoxide_core::Storage::walk(storage, "")? {
        out.push(PackageFile {
            bytes: logoxide_core::Storage::read(storage, &path)?,
            path,
        });
    }
    Ok(out)
}

/// A new package from one file: see `Logoxide::new_package`
pub(crate) fn new_package(
    file_name: &str,
    bytes: &[u8],
    given: NewPackage,
) -> Result<Vec<PackageFile>, LogoxideError> {
    let files = logoxide_core::MemoryStorage::new();
    core::new_from_file(file_name, bytes, &given.try_into()?, &files)?;
    files_of(&files)
}

/// A new collection: see `Logoxide::new_collection`
pub(crate) fn new_collection(given: NewPackage) -> Result<Vec<PackageFile>, LogoxideError> {
    let files = logoxide_core::MemoryStorage::new();
    core::new_collection(&given.try_into()?, &files)?;
    files_of(&files)
}

/// What a file says about itself: an EPUB's title, authors, ...; nothing for other formats
pub(crate) fn file_metadata(file_name: &str, bytes: &[u8]) -> Result<FileMetadata, LogoxideError> {
    Ok(match core::file_format(file_name, bytes) {
        Some(("epub", _)) => core::epub_metadata(bytes)?.into(),
        _ => FileMetadata::default(),
    })
}
