//! Catalogs: packages to search by name or language instead of by URL (see logoxide's `catalog`).

use boltffi::*;
use logoxide_core::{
    catalog::{
        Catalog, CatalogEntry as CoreEntry, CatalogRelease as CoreRelease,
        CatalogSource as CoreSource, Download as CoreDownload, WorkFrom as CoreWorkFrom,
    },
    manifest::Direction,
};

use crate::{ImportDownload, PackageKind};

/// Where a listed package comes from
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogSource {
    /// A logoxide package: add it with `{ git: url }` and install it
    Git { url: String },
    /// A package a registry publishes as archives (the entry's `releases`): add it with
    /// `{ registry: catalog, version }` and install it
    Registry { catalog: String },
    /// Another publisher's download (eBible.org, fetch.bible, CCEL, a SWORD repository), made
    /// into a package by importing it, the same way for every publisher: fetch each of
    /// `downloads` (`Logoxide::import_downloads` lists them too), then give their bytes, by
    /// name, to `Logoxide::import_downloaded` for the package's files
    External {
        /// Its import name, as `lx import` takes it: the publisher's prefix (`ebible`, `fetch`,
        /// `ccel`, `sword`), a colon, and the publisher's own id (`ebible:eng-asv`,
        /// `sword:crosswire/KJV`)
        import: String,
        /// The download the import is made from (the first of a SWORD module's zips), which the
        /// import keeps as `Upstream.url`, so `LogoxideCatalog.findUpstream` finds the entry by
        /// it
        upstream: Option<String>,
        /// Whether a newer download always has a newer `version` in the catalog, so an import at
        /// the catalog's version needn't be downloaded again to know it's current
        /// (`Logoxide::import_is_current`): eBible.org's dates and SWORD modules' versions do;
        /// fetch.bible's and CCEL's don't
        tracks_versions: bool,
        /// Each file the import needs, by name, with the URLs to try in turn: one `zip` for
        /// eBible.org and SWORD, one `thml` for CCEL, and each book by its lowercase code
        /// (`jhn`) for fetch.bible
        downloads: Vec<ImportDownload>,
    },
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
    /// What references name it by (`BSB`): a Bible or a commentary always has one; a book, a
    /// collection, or another kind may not
    pub abbreviation: Option<String>,
    pub aliases: Vec<String>,
    /// BCP 47 (`en`), or ISO 639-3 (`eng`) as eBible.org gives it; `und` when unknown
    pub language: String,
    /// The language's name: the catalog's (`Español`), else its English name where logoxide
    /// knows it (`Korean`)
    pub language_name: Option<String>,
    /// Its script, ISO 15924 (`Latn`, `Hebr`), when the catalog or the language tag says
    pub script: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    /// What its license allows: as the catalog says, with the rest derived from `license`
    pub rights: crate::Rights,
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
    /// The work it's an edition of (`kjv`): a logoxide catalog's, or the one logoxide's table of
    /// works knows an import to be
    pub work: Option<String>,
    /// Which edition (`1769-oxford`), and its year, when a logoxide catalog says
    pub edition: Option<String>,
    pub edition_year: Option<i32>,
    /// For a dictionary, when the catalog says: what its entries are keyed by
    /// (`strongs-greek`, `strongs-hebrew`, or `headword`)
    pub keys: Option<String>,
}

/// A work's entries in a search (`LogoxideCatalog.searchGrouped`), or one entry of none
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkGroup {
    /// The work's id (`kjv`), if its entries name one
    pub work: Option<String>,
    /// Its name (`King James Version`), from logoxide's table of works, else a catalog's, else
    /// its id
    pub name: Option<String>,
    pub entries: Vec<CatalogEntry>,
}

/// Where a work is named (`LogoxideCatalog.works`)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkFrom {
    /// logoxide's own table of works
    Logoxide,
    /// A catalog's `[works]` (a registry's), by the catalog's name
    Catalog { name: String },
    /// Nowhere: only packages claim it
    Unnamed,
}

/// A work packages can say they're editions of, and the packages that do
/// (`LogoxideCatalog.works`)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkInfo {
    /// What a package's `work` names it by (`kjv`)
    pub id: String,
    /// None when nothing names it
    pub name: Option<String>,
    pub aliases: Vec<String>,
    /// Its Wikidata item (`Q…`), when known
    pub wikidata: Option<String>,
    pub from: WorkFrom,
    pub entries: Vec<CatalogEntry>,
}

impl From<&CoreEntry> for CatalogEntry {
    fn from(e: &CoreEntry) -> Self {
        Self {
            id: e.id.clone(),
            name: e.name.clone(),
            abbreviation: e.abbreviation.clone(),
            aliases: e.aliases.clone(),
            language: e.language.clone(),
            language_name: (e.language_name.clone())
                .or_else(|| logoxide_core::language::language_name(&e.language).map(String::from)),
            script: e.script.clone(),
            description: e.description.clone(),
            license: e.license.clone(),
            rights: e.text_rights().into(),
            copyright: e.copyright.clone(),
            version: e.version.clone(),
            kind: e.kind.into(),
            right_to_left: e.direction == Direction::Rtl,
            source: match &e.source {
                CoreSource::Git { url } => CatalogSource::Git { url: url.clone() },
                CoreSource::Registry { catalog } => CatalogSource::Registry {
                    catalog: catalog.clone(),
                },
                CoreSource::External {
                    import,
                    upstream,
                    tracks_versions,
                    downloads,
                } => CatalogSource::External {
                    import: import.clone(),
                    upstream: upstream.clone(),
                    tracks_versions: *tracks_versions,
                    downloads: downloads.iter().map(ImportDownload::from).collect(),
                },
            },
            path: e.path.clone(),
            releases: e.releases.iter().map(Into::into).collect(),
            keywords: e.keywords.clone(),
            categories: e.categories.clone(),
            about_books: e.about_books.clone(),
            work: e.work.clone(),
            edition: e.edition.clone(),
            edition_year: e.edition_year,
            keys: e.keys.map(|k| k.name().to_string()),
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
            script: e.script,
            description: e.description,
            license: e.license,
            rights: e.rights.into(),
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
                CatalogSource::External {
                    import,
                    upstream,
                    tracks_versions,
                    downloads,
                } => CoreSource::External {
                    import,
                    upstream,
                    tracks_versions,
                    downloads: downloads.into_iter().map(CoreDownload::from).collect(),
                },
            },
            path: e.path,
            releases: e.releases.into_iter().map(Into::into).collect(),
            keywords: e.keywords,
            categories: e.categories,
            about_books: e.about_books,
            work: e.work,
            edition: e.edition,
            edition_year: e.edition_year,
            // A name logoxide doesn't know (from a newer one) is left out
            keys: e
                .keys
                .as_deref()
                .and_then(logoxide_core::manifest::Keys::from_name),
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

    /// `search`'s packages grouped by the work each is an edition of (every KJV together), in
    /// the order of each work's best; at most `limit` packages (all for 0)
    pub fn search_grouped(&self, query: String, limit: u32) -> Vec<WorkGroup> {
        let found = self.catalog.search(&query);
        let limit = if limit == 0 {
            found.len()
        } else {
            limit as usize
        };
        (self
            .catalog
            .group_by_work(found.into_iter().take(limit))
            .into_iter())
        .map(|group| WorkGroup {
            name: group.name.clone(),
            work: group.work.clone(),
            entries: group.entries.into_iter().map(CatalogEntry::from).collect(),
        })
        .collect()
    }

    /// Every work a package can say it's an edition of: logoxide's table, then those the
    /// catalogs name (`[works]`), then ids packages claim that nothing names, each with the
    /// packages claiming it; with `query`, those whose id, name, or aliases hold each word
    pub fn works(&self, query: String) -> Vec<WorkInfo> {
        (self.catalog.works_listing(&query).into_iter())
            .map(|w| WorkInfo {
                id: w.id,
                name: w.name,
                aliases: w.aliases,
                wikidata: w.wikidata,
                from: match w.from {
                    CoreWorkFrom::Logoxide => WorkFrom::Logoxide,
                    CoreWorkFrom::Catalog(name) => WorkFrom::Catalog { name },
                    CoreWorkFrom::Unnamed => WorkFrom::Unnamed,
                },
                entries: w.entries.into_iter().map(CatalogEntry::from).collect(),
            })
            .collect()
    }

    /// The entry an imported package was made from, by its `upstream` (`PackageInfo.upstream`:
    /// its import name, else its download): how to update an import (`Logoxide::update_import`)
    pub fn find_upstream(&self, upstream: crate::Upstream) -> Option<CatalogEntry> {
        self.catalog
            .find_upstream(&upstream.into())
            .map(CatalogEntry::from)
    }

    /// The entry for an import (`ebible:eng-kjv2006`): what an installer's `ImportEntry` request
    /// asks for (`LogoxideInstaller.provideImportEntry`)
    pub fn find_import(&self, import: String) -> Option<CatalogEntry> {
        self.catalog.find_import(&import).map(CatalogEntry::from)
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
