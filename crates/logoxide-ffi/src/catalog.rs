//! Catalogs: packages to search by name or language instead of by URL (see logoxide's `catalog`).

use boltffi::*;
use logoxide_core::{
    catalog::{
        Catalog, CatalogEntry as CoreEntry, CatalogRelease as CoreRelease,
        CatalogSource as CoreSource,
    },
    import::{ccel_url, ebible_zip_url, fetch_bible_book_url, sword_zip_urls},
    manifest::Direction,
};

use crate::PackageKind;

/// Where a listed package comes from
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogSource {
    /// A logoxide package: add it with `{ git: url }` and install it
    Git { url: String },
    /// A package a registry publishes as archives (the entry's `releases`): add it with
    /// `{ registry: catalog, version }` and install it
    Registry { catalog: String },
    /// A translation eBible.org publishes: download `zip_url`, then `Logoxide::import_ebible`
    Ebible { id: String, zip_url: String },
    /// A translation fetch.bible publishes: download each of `books`, then
    /// `Logoxide::import_fetch_bible`
    FetchBible { id: String, books: Vec<FetchBook> },
    /// A Bible CCEL publishes as ThML: download `url`, then `Logoxide::import_ccel` (its file's
    /// head gives the language and rights the catalog doesn't)
    Ccel { id: String, url: String },
    /// A module a SWORD repository publishes (`id` is `<repository>/<module>`, as `sword:` names
    /// it): download the first of `zip_urls` that answers, then `Logoxide::import_sword`
    Sword {
        id: String,
        repository: String,
        module: String,
        zip_urls: Vec<String>,
    },
}

/// A book fetch.bible publishes, and where
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchBook {
    /// Its USFM code as fetch.bible writes it (`jhn`)
    pub code: String,
    pub url: String,
}

/// One release a registry publishes: its archive, and what it hashes to
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogRelease {
    pub version: String,
    pub url: String,
    /// Other places serving the same archive, to try in order when `url` fails
    pub mirrors: Vec<String>,
    /// The SHA-256 of the archive (`sha256:` and hex)
    pub sha256: String,
    /// What its files hash to, as a lock file has it
    pub checksum: Option<String>,
    pub size: Option<u64>,
    /// Withdrawn: new installs pass over it (a lock file that has it still installs it)
    pub yanked: bool,
}

impl From<&CoreRelease> for CatalogRelease {
    fn from(r: &CoreRelease) -> Self {
        Self {
            version: r.version.clone(),
            url: r.url.clone(),
            mirrors: r.mirrors.clone(),
            sha256: r.sha256.clone(),
            checksum: r.checksum.clone(),
            size: r.size,
            yanked: r.yanked,
        }
    }
}

impl From<CatalogRelease> for CoreRelease {
    fn from(r: CatalogRelease) -> Self {
        Self {
            version: r.version,
            url: r.url,
            mirrors: r.mirrors,
            sha256: r.sha256,
            checksum: r.checksum,
            size: r.size,
            yanked: r.yanked,
        }
    }
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
    /// The newest version, when the catalog says: for an import, the publisher's own as it gives
    /// it (`2026-10-08`), which the import keeps as `Upstream.version`
    pub version: Option<String>,
    pub kind: PackageKind,
    pub right_to_left: bool,
    pub source: CatalogSource,
    /// Its path in a registry folder (`books/romans`), which names it there as its id does
    pub path: Option<String>,
    /// A registry's releases of it, oldest first (none for other sources)
    pub releases: Vec<CatalogRelease>,
    /// Its manifest's keywords, categories, and books of the Bible it's mainly about (USFM
    /// codes); only a logoxide catalog's entries have them
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub about_books: Vec<String>,
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
                CoreSource::Registry { catalog } => CatalogSource::Registry {
                    catalog: catalog.clone(),
                },
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
                CoreSource::Ccel { id } => CatalogSource::Ccel {
                    id: id.clone(),
                    url: ccel_url(id),
                },
                CoreSource::Sword {
                    id,
                    repository,
                    module,
                } => CatalogSource::Sword {
                    id: id.clone(),
                    repository: repository.clone(),
                    module: module.clone(),
                    zip_urls: sword_zip_urls(repository, module),
                },
            },
            path: e.path.clone(),
            releases: e.releases.iter().map(Into::into).collect(),
            keywords: e.keywords.clone(),
            categories: e.categories.clone(),
            about_books: e.about_books.clone(),
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
            kind: e.kind.into(),
            direction: match e.right_to_left {
                true => Direction::Rtl,
                false => Direction::Ltr,
            },
            source: match e.source {
                CatalogSource::Git { url } => CoreSource::Git { url },
                CatalogSource::Registry { catalog } => CoreSource::Registry { catalog },
                CatalogSource::Ebible { id, .. } => CoreSource::Ebible { id },
                CatalogSource::FetchBible { id, books } => CoreSource::FetchBible {
                    id,
                    books: books.into_iter().map(|b| b.code).collect(),
                },
                CatalogSource::Ccel { id, .. } => CoreSource::Ccel { id },
                CatalogSource::Sword {
                    id,
                    repository,
                    module,
                    ..
                } => CoreSource::Sword {
                    id,
                    repository,
                    module,
                },
            },
            path: e.path,
            releases: e.releases.into_iter().map(Into::into).collect(),
            keywords: e.keywords,
            categories: e.categories,
            about_books: e.about_books,
        }
    }
}

/**
A catalog's packages, from `Logoxide::parse_catalog`; catalogs merge, so one search covers them all

```ts
let catalog = logoxide.parseCatalog(url, await getText(url)); // for each of project.catalogs()
catalog = catalog.merge(other);
catalog.search("world english", 20); // best first
```
*/
pub struct LogoxideCatalog {
    pub(crate) catalog: Catalog,
}

#[export]
impl LogoxideCatalog {
    /// The catalog's name (`eBible.org`)
    pub fn name(&self) -> String {
        self.catalog.name.clone()
    }

    /// How many packages it lists
    pub fn count(&self) -> u32 {
        self.catalog.entries.len() as u32
    }

    /// What reading it passed over, and why (an entry whose id isn't one), to warn of
    pub fn warnings(&self) -> Vec<String> {
        self.catalog.warnings.clone()
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

    /// The entry an imported package was made from, by its `upstream` (`PackageInfo.upstream`:
    /// its import name, else its download): how to update an import (`Logoxide::update_import`)
    pub fn find_upstream(&self, upstream: crate::Upstream) -> Option<CatalogEntry> {
        self.catalog
            .find_upstream(&upstream.into())
            .map(CatalogEntry::from)
    }

    /// This catalog and another's packages (an id in both is this one's)
    pub fn merge(&self, other: &LogoxideCatalog) -> LogoxideCatalog {
        LogoxideCatalog {
            catalog: self.catalog.clone().merge(other.catalog.clone()),
        }
    }
}

/// How much a change can break links: notes or markup (`Patch`), wording or added verses
/// (`Minor`), verses removed or renumbered (`Major`)
#[data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Patch,
    Minor,
    Major,
}

impl From<logoxide_core::summary::Change> for ChangeKind {
    fn from(change: logoxide_core::summary::Change) -> Self {
        use logoxide_core::summary::Change;
        match change {
            Change::Patch => ChangeKind::Patch,
            Change::Minor => ChangeKind::Minor,
            Change::Major => ChangeKind::Major,
        }
    }
}

/// What updating an imported package changes (`Logoxide::update_import`), and the files to do it
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportUpdate {
    pub from: String,
    pub to: String,
    pub change: ChangeKind,
    /// What changed, in words (`1 verse reworded`)
    pub description: String,
    /// The publisher's versions before and after (`Upstream.version`), as it gives them; the
    /// package's `from` and `to` are logoxide's, `to` raised by what changed
    pub upstream_from: Option<String>,
    pub upstream_to: Option<String>,
    /// The package's files now, to write over the old (its `logoxide.toml` has `to`)
    pub files: Vec<crate::PackageFile>,
    /// Its old files to delete (ones the publisher dropped); a reader's files beside it aren't
    /// among them
    pub removed: Vec<String>,
}
