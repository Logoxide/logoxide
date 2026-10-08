//! Catalogs: packages to search by name or language instead of by URL (see biblix's `catalog`).

use biblix_core::{
    catalog::{Catalog, CatalogEntry as CoreEntry, CatalogSource as CoreSource},
    import::{ebible_zip_url, fetch_bible_book_url},
    manifest::{Direction, PackageKind as CoreKind},
};
use boltffi::*;

use crate::PackageKind;

/// Where a listed package comes from
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogSource {
    /// A biblix package: add it with `{ git: url }` and install it
    Git { url: String },
    /// A translation eBible.org publishes: download `zip_url`, then `Biblix::import_ebible`
    Ebible { id: String, zip_url: String },
    /// A translation fetch.bible publishes: download each of `books`, then
    /// `Biblix::import_fetch_bible`
    FetchBible { id: String, books: Vec<FetchBook> },
}

/// A book fetch.bible publishes, and where
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchBook {
    /// Its USFM code as fetch.bible writes it (`jhn`)
    pub code: String,
    pub url: String,
}

/// One package a catalog lists
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    /// What references name it by (`BSB`)
    pub abbreviation: String,
    pub aliases: Vec<String>,
    /// BCP 47 (`en`), or ISO 639-3 (`eng`) as eBible.org gives it
    pub language: String,
    pub language_name: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub copyright: Option<String>,
    /// The newest version, when the catalog says
    pub version: Option<String>,
    pub kind: PackageKind,
    pub right_to_left: bool,
    pub source: CatalogSource,
}

impl From<&CoreEntry> for CatalogEntry {
    fn from(e: &CoreEntry) -> Self {
        Self {
            id: e.id.clone(),
            name: e.name.clone(),
            abbreviation: e.abbreviation.clone(),
            aliases: e.aliases.clone(),
            language: e.language.clone(),
            language_name: e.language_name.clone(),
            description: e.description.clone(),
            license: e.license.clone(),
            copyright: e.copyright.clone(),
            version: e.version.clone(),
            kind: e.kind.into(),
            right_to_left: e.direction == Direction::Rtl,
            source: match &e.source {
                CoreSource::Git { url } => CatalogSource::Git { url: url.clone() },
                CoreSource::Ebible { id } => CatalogSource::Ebible {
                    id: id.clone(),
                    zip_url: ebible_zip_url(id),
                },
                CoreSource::FetchBible { id, books } => CatalogSource::FetchBible {
                    id: id.clone(),
                    books: (books.iter())
                        .map(|code| FetchBook {
                            code: code.clone(),
                            url: fetch_bible_book_url(id, code),
                        })
                        .collect(),
                },
            },
        }
    }
}

impl From<CatalogEntry> for CoreEntry {
    fn from(e: CatalogEntry) -> Self {
        Self {
            id: e.id,
            name: e.name,
            abbreviation: e.abbreviation,
            aliases: e.aliases,
            language: e.language,
            language_name: e.language_name,
            description: e.description,
            license: e.license,
            copyright: e.copyright,
            version: e.version,
            kind: match e.kind {
                PackageKind::Bible => CoreKind::Bible,
                PackageKind::Commentary => CoreKind::Commentary,
                PackageKind::Lexicon => CoreKind::Lexicon,
                PackageKind::CrossReferences => CoreKind::CrossReferences,
                PackageKind::VersificationMap => CoreKind::VersificationMap,
                PackageKind::Book => CoreKind::Book,
            },
            direction: match e.right_to_left {
                true => Direction::Rtl,
                false => Direction::Ltr,
            },
            source: match e.source {
                CatalogSource::Git { url } => CoreSource::Git { url },
                CatalogSource::Ebible { id, .. } => CoreSource::Ebible { id },
                CatalogSource::FetchBible { id, books } => CoreSource::FetchBible {
                    id,
                    books: books.into_iter().map(|b| b.code).collect(),
                },
            },
        }
    }
}

/**
A catalog's packages, from `Biblix::parse_catalog`; catalogs merge, so one search covers them all

```ts
let catalog = biblix.parseCatalog(url, await getText(url)); // for each of project.catalogs()
catalog = catalog.merge(other);
catalog.search("world english", 20); // best first
```
*/
pub struct BiblixCatalog {
    pub(crate) catalog: Catalog,
}

#[export]
impl BiblixCatalog {
    /// The catalog's name (`eBible.org`)
    pub fn name(&self) -> String {
        self.catalog.name.clone()
    }

    /// How many packages it lists
    pub fn count(&self) -> u32 {
        self.catalog.entries.len() as u32
    }

    /// The packages every word of `query` matches (id, name, abbreviation, aliases, language,
    /// or description, ignoring case), best first; at most `limit` (all for 0)
    pub fn search(&self, query: String, limit: u32) -> Vec<CatalogEntry> {
        let found = self.catalog.search(&query);
        let limit = if limit == 0 {
            found.len()
        } else {
            limit as usize
        };
        found
            .into_iter()
            .take(limit)
            .map(CatalogEntry::from)
            .collect()
    }

    /// This catalog and another's packages (an id in both is this one's)
    pub fn merge(&self, other: &BiblixCatalog) -> BiblixCatalog {
        BiblixCatalog {
            catalog: self.catalog.clone().merge(other.catalog.clone()),
        }
    }
}
