/*!
Catalogs: lists of packages to search, so a Bible can be found by its name or language instead of
its URL. A catalog holds what each package is and where it comes from, never its text, so anyone
can host one (a file in a git repository is enough). This module reads **logoxide catalogs**
(`catalog.toml`): packages published as git repositories or as a registry's archives, each as its
`logoxide.toml` describes it (`lx catalog entry <url>` writes an entry). Other publishers' own
lists (eBible.org's `translations.csv`, [`EBIBLE_CATALOG`]; fetch.bible's `manifest.json`,
[`FETCH_BIBLE_CATALOG`]; CCEL's page of its Bibles, [`CCEL_CATALOG`]; a SWORD repository's
`mods.d.tar.gz`, [`SWORD_CATALOG`] CrossWire's) are read by the `logoxide-import` crate into the
same [`Catalog`], each entry an [`CatalogSource::External`] that says what to download; the
package manager knows no publisher, only what an entry says of itself.

```toml
# catalog.toml
name = "Bibles in English"

[[package]]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
language = "en"
license = "LicenseRef-PublicDomain"
git = "https://github.com/someone/bsb"
```

A vault names its catalogs in `[settings] catalogs` (URLs or paths); without any, the four
publishers' lists are searched ([`DEFAULT_CATALOGS`]).

**Registries** are catalogs with a name (`[settings.registries]`), so a package can be named
through one: `dorean-local:dorean-collection` by its id, or `dorean-local:books/romans` by its
path there. A registry is a catalog, or a folder of package repositories: each git repository
with a `logoxide.toml` in it, at any depth, is a package ([`Catalog::from_packages`]), named by
its path in the folder and found at its `file://` URL.

```toml
[settings.registries]
dorean-local = "~/Documents/logoxide-dorean-repos"
```
*/

use serde::{Deserialize, Serialize};

use crate::{
    error::{LogoxideError, Result},
    manifest::{Direction, PackageKind},
};

/// eBible.org's list of the translations it publishes
pub const EBIBLE_CATALOG: &str = "https://ebible.org/Scriptures/translations.csv";

/// fetch.bible's list of the translations it publishes
pub const FETCH_BIBLE_CATALOG: &str = "https://v1.fetch.bible/manifest.json";

/// CCEL's page of the Bibles it publishes (HTML: it has no other list)
pub const CCEL_CATALOG: &str = "https://ccel.org/ccel/bible/";

/// CrossWire's main SWORD repository's list of modules (every module's conf)
pub const SWORD_CATALOG: &str = "https://crosswire.org/ftpmirror/pub/sword/raw/mods.d.tar.gz";

/// The catalogs searched when a vault names none
pub const DEFAULT_CATALOGS: [&str; 4] = [
    EBIBLE_CATALOG,
    FETCH_BIBLE_CATALOG,
    CCEL_CATALOG,
    SWORD_CATALOG,
];

/// Where a listed package comes from
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogSource {
    /// A logoxide package in a git repository
    Git { url: String },
    /// A package a registry publishes as archives (its entry's `releases`): `catalog` is the
    /// registry's catalog, which a dependency names as its `registry`
    Registry { catalog: String },
    /**
    Another publisher's download, imported into a package (by `logoxide-import`, which lists
    them): what the package manager needs to know of it, and nothing of the publisher
    */
    External {
        /// Its import name, as `lx import` takes it: a prefix naming the publisher
        /// (`ebible`, `fetch`, `ccel`, `sword`), a colon, and the publisher's own id
        /// (`ebible:eng-asv`, `sword:crosswire/KJV`)
        import: String,
        /// The download the import is made from (the first of a SWORD module's zips), which
        /// the import writes as `[package.upstream] url`, so [`Catalog::find_upstream`] can
        /// find an entry by it
        #[serde(default, skip_serializing_if = "Option::is_none")]
        upstream: Option<String>,
        /// Whether a newer download always has a newer `version` here, so an import at the
        /// catalog's version needn't be downloaded again to know it's current: eBible.org's
        /// update dates and SWORD modules' `Version`s do; fetch.bible's year and CCEL's
        /// (unlisted) edition don't
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        tracks_versions: bool,
        /// Each file the import needs, by name, with the URLs to try in turn (a zip; fetch.bible's
        /// books, each by its code), so every host downloads the same files the same way and
        /// hands their bytes, by name, to `logoxide-import`
        downloads: Vec<Download>,
    },
}

/// A file an import needs: `name` is what the import knows it by, and it comes from the first of
/// `urls` that answers
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Download {
    pub name: String,
    pub urls: Vec<String>,
}

/// One release a registry publishes: its archive and what it hashes to
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRelease {
    pub version: String,
    /// Its `.tar.gz` (relative to the catalog in its file; whole once read)
    pub url: String,
    /// Other places serving the same archive, tried in order after `url` (relative to the
    /// catalog too)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrors: Vec<String>,
    /// The SHA-256 of the archive's bytes (`sha256:` and hex)
    pub sha256: String,
    /// What the package's files hash to (`Package::checksum`), as a lock file has it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// The archive's size in bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Withdrawn (`lx yank`): new installs don't choose it, but a lock file that has it still
    /// installs it, so nobody's text changes under them
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yanked: bool,
}

impl CatalogSource {
    /// The prefix that names an import's publisher (`ebible`, `fetch`); a git or registry
    /// package has none, since it's added, not imported
    pub fn prefix(&self) -> Option<&str> {
        Some(self.import_name()?.split_once(':')?.0)
    }

    /// The publisher's own id (`eng-asv`, `crosswire/KJV`), for an import
    pub fn import_id(&self) -> Option<&str> {
        Some(self.import_name()?.split_once(':')?.1)
    }

    /// Whether a newer download always has a newer version listed ([`CatalogSource::External`]'s
    /// `tracks_versions`); never for what isn't imported
    pub fn versions_track_updates(&self) -> bool {
        matches!(
            self,
            CatalogSource::External {
                tracks_versions: true,
                ..
            }
        )
    }

    /// What `lx import` takes for it (`ebible:eng-asv`, `fetch:eng_asv`); none for what isn't
    /// imported
    pub fn import_name(&self) -> Option<&str> {
        match self {
            CatalogSource::External { import, .. } => Some(import),
            CatalogSource::Git { .. } | CatalogSource::Registry { .. } => None,
        }
    }

    /// The download an import is made from, for an import
    pub fn upstream_url(&self) -> Option<&str> {
        match self {
            CatalogSource::External { upstream, .. } => upstream.as_deref(),
            CatalogSource::Git { .. } | CatalogSource::Registry { .. } => None,
        }
    }

    /// What an import downloads ([`CatalogSource::External`]'s `downloads`); nothing for what
    /// isn't imported
    pub fn downloads(&self) -> &[Download] {
        match self {
            CatalogSource::External { downloads, .. } => downloads,
            CatalogSource::Git { .. } | CatalogSource::Registry { .. } => &[],
        }
    }
}

/// One package a catalog lists
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The package's id (`bsb`, `eng-asv`)
    pub id: String,
    pub name: String,
    /// What references name it by (`BSB`)
    pub abbreviation: String,
    pub aliases: Vec<String>,
    /// BCP 47 (`en`), or ISO 639-3 (`eng`) as eBible.org gives it
    pub language: String,
    /// The language's name, when the catalog gives it (`English`), or the publisher's own text
    /// for a language that isn't a tag (`und`)
    pub language_name: Option<String>,
    /// Its script (ISO 15924: `Latn`, `Hebr`), when the catalog says or its language tag does
    pub script: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    /// What its license allows, as the catalog says it (a `catalog.toml`'s `rights`, a
    /// publisher's terms); what's derived from `license` isn't here (see
    /// [`CatalogEntry::text_rights`])
    pub rights: crate::license::Rights,
    /// A copyright statement, when the catalog gives one
    pub copyright: Option<String>,
    /**
    The newest version, when the catalog says: a logoxide package's, or for an import the
    publisher's own, as it gives it (eBible.org's update date `2026-10-08`, a SWORD module's
    `Version`, fetch.bible's year), which an import keeps as its `[package.upstream] version`
    */
    pub version: Option<String>,
    pub kind: PackageKind,
    pub direction: Direction,
    pub source: CatalogSource,
    /// Its path in a registry folder (`books/romans`), which names it there as its id does
    pub path: Option<String>,
    /// The releases a registry publishes as archives, oldest first (none for other sources)
    pub releases: Vec<CatalogRelease>,
    /// Its manifest's `keywords`, `categories`, and `about-books` (a logoxide catalog's entries;
    /// other catalogs don't say)
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub about_books: Vec<String>,
    /// The work it's an edition of (`kjv`): a logoxide catalog's `work`, or for an import, the
    /// work logoxide's table knows its import name to be ([`crate::works`])
    pub work: Option<String>,
    /// Which edition (`1769-oxford`), and its year, when a logoxide catalog says
    pub edition: Option<String>,
    pub edition_year: Option<i32>,
    /// What a dictionary's keys are (`[package] keys`: Strong's Greek or Hebrew numbers, or
    /// headwords), when the catalog says; none for other kinds
    pub keys: Option<crate::manifest::Keys>,
}

impl CatalogEntry {
    /// What its license allows: its `rights`, with what they don't say derived from its license
    pub fn text_rights(&self) -> crate::license::Rights {
        (self.rights).or(crate::license::derived_rights(self.license.as_deref()))
    }

    /// Whether it's mainly about the book with this USFM code (`ROM`), as its `about-books` says
    pub fn is_about(&self, book: &str) -> bool {
        self.about_books.iter().any(|b| b == book)
    }

    /**
    The newest release `req` allows (without one, the newest that isn't a pre-release), from a
    registry's archives; a yanked one never
    */
    pub fn release(&self, req: Option<&semver::VersionReq>) -> Option<&CatalogRelease> {
        (self.releases.iter())
            .filter(|r| !r.yanked)
            .filter_map(|r| Some((r.version.parse::<semver::Version>().ok()?, r)))
            .filter(|(v, _)| match req {
                Some(req) => req.matches(v),
                None => v.pre.is_empty(),
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, r)| r)
    }
}

/// The `[[package]]` tables of a `catalog.toml`
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct TomlEntry {
    id: String,
    name: String,
    abbreviation: String,
    #[serde(default)]
    aliases: Vec<String>,
    language: String,
    #[serde(default)]
    language_name: Option<String>,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    rights: crate::license::Rights,
    #[serde(default)]
    kind: PackageKind,
    /// Without one, its script's
    #[serde(default)]
    direction: Option<Direction>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    git: Option<String>,
    /// A registry's archives (`[[package.release]]`)
    #[serde(default, rename = "release")]
    releases: Vec<CatalogRelease>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    about_books: Vec<String>,
    #[serde(default)]
    work: Option<String>,
    #[serde(default)]
    edition: Option<String>,
    #[serde(default)]
    edition_year: Option<i32>,
    #[serde(default)]
    keys: Option<crate::manifest::Keys>,
    /// Its path in the folder the catalog was built from (`lx catalog build`), which names it
    /// as its id does
    #[serde(default)]
    path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct TomlCatalog {
    /// The layout's version ([`CATALOG_FORMAT`]); none is 1
    #[serde(default)]
    format: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "package")]
    packages: Vec<TomlEntry>,
}

/**
The `catalog.toml` layout this logoxide reads and writes (`format = 1`; a catalog without it is 1).
A catalog of a later format is refused with a request to upgrade, rather than misread
*/
pub const CATALOG_FORMAT: u32 = 1;

/// A catalog's packages
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub name: String,
    pub entries: Vec<CatalogEntry>,
    /// What reading it passed over, and why (an entry whose id isn't one), for a warning
    pub warnings: Vec<String>,
}

impl Catalog {
    /// A `catalog.toml` read from `location` (a URL or a path): [`Catalog::parse_toml_at`].
    /// Other publishers' lists are read by `logoxide-import`'s `catalog::parse`, which falls
    /// back to this
    pub fn parse(location: &str, text: &str) -> Result<Catalog> {
        Catalog::parse_toml_at(Some(location), text)
    }

    pub fn parse_toml(text: &str) -> Result<Catalog> {
        Catalog::parse_toml_at(None, text)
    }

    /**
    A `catalog.toml` read from `location` (a URL or a path), which its relative archive URLs
    are relative to. A package needs `git` or releases
    */
    pub fn parse_toml_at(location: Option<&str>, text: &str) -> Result<Catalog> {
        let catalog: TomlCatalog =
            toml::from_str(text).map_err(|e| LogoxideError::Catalog(e.to_string()))?;
        let format = catalog.format.unwrap_or(1);
        if format > CATALOG_FORMAT {
            return Err(LogoxideError::Catalog(format!(
                "{}is format {format}, newer than this lx reads ({CATALOG_FORMAT}): upgrade lx",
                location.map(|l| format!("{l} ")).unwrap_or_default()
            )));
        }
        // A catalog from elsewhere names nothing on this machine: its packages, archives, and
        // mirrors are elsewhere too (a relative one is beside it)
        let remote = location.is_some_and(|l| !crate::git::is_local(l));
        let check = |id: &str, url: &str| match remote && crate::git::is_local(url) {
            true => Err(LogoxideError::Catalog(format!(
                "{id}: `{url}` is on this machine, but the catalog isn't"
            ))),
            false => Ok(()),
        };
        let (mut entries, mut warnings) = (vec![], vec![]);
        for e in catalog.packages {
            // An id names a folder in the library and a lock entry: one that isn't is passed
            // over, not the whole catalog
            if let Err(LogoxideError::Manifest(why)) = crate::manifest::check_id("package", &e.id) {
                let at = location.map(|l| format!("{l}: ")).unwrap_or_default();
                warnings.push(format!("{at}passed over {why}"));
                continue;
            }
            if let Some(git) = &e.git {
                check(&e.id, git)?;
            }
            let source = match &e.git {
                // `github:owner/repo` is listed by its URL
                Some(git) => CatalogSource::Git {
                    url: crate::git::expand_url(git),
                },
                None if e.releases.is_empty() => {
                    return Err(LogoxideError::Catalog(format!(
                        "{}: no `git` and no releases",
                        e.id
                    )));
                }
                None => CatalogSource::Registry {
                    catalog: location.unwrap_or_default().to_string(),
                },
            };
            let releases = (e.releases.into_iter())
                .map(|r| {
                    let whole = |url: &String| match location {
                        Some(base) => relative_url(base, url),
                        None => url.clone(),
                    };
                    CatalogRelease {
                        url: whole(&r.url),
                        mirrors: r.mirrors.iter().map(whole).collect(),
                        ..r
                    }
                })
                .collect::<Vec<CatalogRelease>>();
            for release in &releases {
                for url in std::iter::once(&release.url).chain(&release.mirrors) {
                    check(&e.id, url)?;
                }
            }
            let script = (e.script.clone()).or_else(|| crate::language::script_of_tag(&e.language));
            entries.push(CatalogEntry {
                id: e.id,
                name: e.name,
                abbreviation: e.abbreviation,
                aliases: e.aliases,
                language: e.language,
                language_name: e.language_name,
                direction: (e.direction)
                    .unwrap_or_else(|| crate::language::script_direction(script.as_deref())),
                script,
                description: e.description,
                license: e.license,
                rights: e.rights,
                copyright: None,
                version: e.version,
                kind: e.kind,
                source,
                path: e.path,
                releases,
                keywords: e.keywords,
                categories: e.categories,
                about_books: e.about_books,
                work: e.work,
                edition: e.edition,
                edition_year: e.edition_year,
                keys: e.keys,
            });
        }
        Ok(Catalog {
            name: catalog.name.unwrap_or_default(),
            entries,
            warnings,
        })
    }

    /**
    [`Catalog::search`]'s entries grouped by the work each is an edition of
    ([`group_by_work`]): `search_grouped("king james")` gives one group of every KJV edition the
    catalogs list, then the rest
    */
    pub fn search_grouped(&self, query: &str) -> Vec<WorkGroup<'_>> {
        group_by_work(self.search(query))
    }

    /// The entries `query` matches, best first: every word of it must be in the entry's id,
    /// name, abbreviation, aliases, language, keywords, categories, its work's names, or
    /// description (ignoring case), or name a book it's about (`romans`); an exact abbreviation
    /// or id comes first, then names, languages, keywords, categories, and books, then
    /// descriptions
    pub fn search(&self, query: &str) -> Vec<&CatalogEntry> {
        let words: Vec<(String, Option<String>)> = (query.split_whitespace())
            .map(|word| (fold(word), crate::manifest::book_code(word)))
            .collect();
        let mut found: Vec<(u32, &CatalogEntry)> = (self.entries.iter())
            .filter_map(|entry| Some((score(entry, &words)?, entry)))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        found.into_iter().map(|(_, e)| e).collect()
    }

    /// The entry an import name (`ebible:eng-asv`, as [`CatalogSource::import_name`] writes it)
    /// names, if this catalog lists it
    pub fn find_import(&self, name: &str) -> Option<&CatalogEntry> {
        (self.entries.iter()).find(|e| e.source.import_name() == Some(name))
    }

    /// The entry an imported package was made from, by the `[package.upstream]` its import wrote:
    /// its import name (`ebible:eng-asv`), else its download's URL
    pub fn find_upstream(&self, upstream: &crate::manifest::Upstream) -> Option<&CatalogEntry> {
        (upstream.import.as_deref())
            .and_then(|name| self.find_import(name.trim()))
            .or_else(|| {
                let url = upstream.url.as_deref()?.trim();
                (self.entries.iter()).find(|e| e.source.upstream_url() == Some(url))
            })
    }

    /**
    A registry folder's packages: each `(path, manifest)` is a repository at that path in the
    folder (`books/romans`, `/`-separated) and its `logoxide.toml`, at `<base_url>/<path>`. A
    manifest without `[package]` (a vault's) isn't a package
    */
    pub fn from_packages(
        name: &str,
        base_url: &str,
        packages: Vec<(String, crate::manifest::Manifest)>,
    ) -> Catalog {
        let base = base_url.trim_end_matches('/');
        let mut entries: Vec<CatalogEntry> = (packages.into_iter())
            .filter_map(|(path, manifest)| {
                let info = manifest.package?;
                Some(CatalogEntry {
                    script: info.text_script(),
                    direction: info.text_direction(),
                    id: info.id,
                    name: info.name,
                    abbreviation: info.abbreviation,
                    aliases: info.aliases,
                    language: info.language,
                    language_name: info.language_name,
                    description: info.description,
                    license: info.license,
                    rights: info.rights.unwrap_or_default(),
                    copyright: None,
                    version: Some(info.version.to_string()),
                    kind: info.kind,
                    source: CatalogSource::Git {
                        url: format!("{base}/{path}"),
                    },
                    path: Some(path),
                    releases: vec![],
                    keywords: info.keywords,
                    categories: info.categories,
                    about_books: info.about_books,
                    work: info.work,
                    edition: info.edition,
                    edition_year: info.edition_year,
                    keys: info.keys,
                })
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Catalog {
            name: name.to_string(),
            entries,
            warnings: vec![],
        }
    }

    /**
    The entry `name` names in this registry: the one at that path (`books/romans`), else the
    one with that id. An id two entries share names neither: give the path
    */
    pub fn find(&self, name: &str) -> Result<Option<&CatalogEntry>> {
        let name = name.trim_matches('/');
        if let Some(entry) = (self.entries.iter()).find(|e| e.path.as_deref() == Some(name)) {
            return Ok(Some(entry));
        }
        let found: Vec<&CatalogEntry> = (self.entries.iter()).filter(|e| e.id == name).collect();
        match found.as_slice() {
            [] => Ok(None),
            [entry] => Ok(Some(entry)),
            many => {
                let paths: Vec<&str> = (many.iter())
                    .map(|e| e.path.as_deref().unwrap_or("?"))
                    .collect();
                Err(LogoxideError::Catalog(format!(
                    "{} packages are `{name}` in {}: name one by its path ({})",
                    many.len(),
                    self.name,
                    paths.join(", ")
                )))
            }
        }
    }

    /**
    The ids more than one of `catalogs` lists, and the names of those that list each, in order:
    merged ([`Catalog::merge`]), the first one's entry is kept and the others' are hidden, which a
    caller may warn of
    */
    pub fn shadowed(catalogs: &[Catalog]) -> Vec<(String, Vec<String>)> {
        let mut listed: indexmap::IndexMap<&str, Vec<String>> = indexmap::IndexMap::new();
        for catalog in catalogs {
            let mut seen = std::collections::BTreeSet::new();
            for entry in &catalog.entries {
                if seen.insert(entry.id.as_str()) {
                    listed
                        .entry(entry.id.as_str())
                        .or_default()
                        .push(catalog.name.clone());
                }
            }
        }
        (listed.into_iter())
            .filter(|(_, names)| names.len() > 1)
            .map(|(id, names)| (id.to_string(), names))
            .collect()
    }

    /// This catalog with another's entries (and warnings) after its own (an id listed twice
    /// keeps the first)
    pub fn merge(mut self, other: Catalog) -> Catalog {
        for entry in other.entries {
            if !self.entries.iter().any(|e| e.id == entry.id) {
                self.entries.push(entry);
            }
        }
        self.warnings.extend(other.warnings);
        self
    }

    /// A `[[package]]` table for a `catalog.toml`, from a package's manifest and its repository
    pub fn toml_entry(info: &crate::manifest::Package, git: &str) -> String {
        Catalog::toml_document(None, vec![Catalog::entry_table(info, git)])
    }

    /// A `catalog.toml`: its name, if given, and these `[[package]]` tables
    /// ([`Catalog::entry_table`]), in order
    pub fn toml_document(name: Option<&str>, tables: Vec<toml_edit::Table>) -> String {
        let mut doc = toml_edit::DocumentMut::new();
        doc.insert("format", toml_edit::value(i64::from(CATALOG_FORMAT)));
        if let Some(name) = name {
            doc.insert("name", toml_edit::value(name));
        }
        let mut packages = toml_edit::ArrayOfTables::new();
        for table in tables {
            packages.push(table);
        }
        doc.insert("package", toml_edit::Item::ArrayOfTables(packages));
        doc.to_string()
    }

    /// A package's `[[package]]` table, from its manifest (a release's) and its repository
    pub fn entry_table(info: &crate::manifest::Package, git: &str) -> toml_edit::Table {
        let mut table = toml_edit::Table::new();
        table.insert("id", toml_edit::value(&info.id));
        table.insert("name", toml_edit::value(&info.name));
        table.insert("abbreviation", toml_edit::value(&info.abbreviation));
        if !info.aliases.is_empty() {
            let aliases: toml_edit::Array = info.aliases.iter().map(String::as_str).collect();
            table.insert("aliases", toml_edit::value(aliases));
        }
        table.insert("language", toml_edit::value(&info.language));
        if let Some(name) = &info.language_name {
            table.insert("language-name", toml_edit::value(name));
        }
        if let Some(script) = &info.text_script() {
            table.insert("script", toml_edit::value(script));
        }
        if let Some(description) = &info.description {
            table.insert("description", toml_edit::value(description));
        }
        table.insert("version", toml_edit::value(info.version.to_string()));
        if let Some(license) = &info.license {
            table.insert("license", toml_edit::value(license));
        }
        if let Some(rights) = info.rights.filter(|r| !r.is_empty())
            && let Ok(toml::Value::Table(said)) = toml::Value::try_from(rights)
        {
            let mut inline = toml_edit::InlineTable::new();
            for (key, value) in said {
                let value = match value {
                    toml::Value::Boolean(b) => toml_edit::Value::from(b),
                    other => toml_edit::Value::from(other.as_str().unwrap_or_default()),
                };
                inline.insert(&key, value);
            }
            table.insert("rights", toml_edit::value(inline));
        }
        if info.kind != PackageKind::Bible {
            let kind = toml::Value::try_from(info.kind)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            table.insert("kind", toml_edit::value(kind));
        }
        if info.text_direction() == Direction::Rtl {
            table.insert("direction", toml_edit::value("rtl"));
        }
        for (key, value) in [("work", &info.work), ("edition", &info.edition)] {
            if let Some(value) = value {
                table.insert(key, toml_edit::value(value));
            }
        }
        if let Some(year) = info.edition_year {
            table.insert("edition-year", toml_edit::value(i64::from(year)));
        }
        if let Some(keys) = info.keys {
            table.insert("keys", toml_edit::value(keys.name()));
        }
        for (key, list) in [
            ("keywords", &info.keywords),
            ("categories", &info.categories),
            ("about-books", &info.about_books),
        ] {
            if !list.is_empty() {
                let list: toml_edit::Array = list.iter().map(String::as_str).collect();
                table.insert(key, toml_edit::value(list));
            }
        }
        table.insert("git", toml_edit::value(git));
        table
    }
}

/// Entries of one work ([`group_by_work`]), or one entry of none
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkGroup<'a> {
    /// The work's id (`kjv`), if its entries name one
    pub work: Option<String>,
    pub entries: Vec<&'a CatalogEntry>,
}

impl WorkGroup<'_> {
    /// The work's name, from logoxide's table of works (else its id), if the group has a work
    pub fn name(&self) -> Option<String> {
        let work = self.work.as_deref()?;
        Some(crate::works::get(work).map_or_else(|| work.to_string(), |w| w.name.clone()))
    }
}

/**
Entries grouped by the work each is an edition of, in the order of each work's first entry (so a
ranked list stays ranked), entries in their order within a group; an entry with no work is a
group of its own
*/
pub fn group_by_work<'a>(
    entries: impl IntoIterator<Item = &'a CatalogEntry>,
) -> Vec<WorkGroup<'a>> {
    let mut groups: Vec<WorkGroup<'a>> = vec![];
    for entry in entries {
        match (entry.work.as_ref())
            .and_then(|work| groups.iter_mut().find(|g| g.work.as_ref() == Some(work)))
        {
            Some(group) => group.entries.push(entry),
            None => groups.push(WorkGroup {
                work: entry.work.clone(),
                entries: vec![entry],
            }),
        }
    }
    groups
}

/// Gives imports the works logoxide's table knows their import names to be
/// ([`crate::works::of_source`]), where the entries don't say; what a publisher's list is read
/// into (`logoxide-import`)
pub fn known_works(entries: &mut [CatalogEntry]) {
    for entry in entries {
        if entry.work.is_none() {
            entry.work = crate::works::of_source(&entry.source);
        }
    }
}

/**
`url` as seen from a catalog at `base`: a URL or an absolute path stays as it is; a relative one
is beside the catalog (`https://example.org/r/catalog.toml` and `bsb/bsb-1.0.0.tar.gz` are
`https://example.org/r/bsb/bsb-1.0.0.tar.gz`)
*/
pub fn relative_url(base: &str, url: &str) -> String {
    if url.contains("://") || url.starts_with('/') {
        return url.to_string();
    }
    match base.rfind('/') {
        Some(slash) => format!("{}/{url}", &base[..slash]),
        None => url.to_string(),
    }
}

/// Names a registry can't have: what URLs and other names already start with
pub const RESERVED_REGISTRY_NAMES: [&str; 13] = [
    "github", "gitlab", "codeberg", "ebible", "fetch", "ccel", "sword", "door43", "http", "https",
    "file", "ssh", "git",
];

/// Whether `name` can name a registry: lowercase letters, digits, `-`, and `_`, not reserved
pub fn check_registry_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && (name.chars()).all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_".contains(c));
    if !valid {
        return Err(LogoxideError::Catalog(format!(
            "registry name `{name}`: use lowercase letters, digits, `-`, and `_`"
        )));
    }
    if RESERVED_REGISTRY_NAMES.contains(&name) {
        return Err(LogoxideError::Catalog(format!(
            "registry name `{name}` is taken (it starts URLs or other names)"
        )));
    }
    Ok(())
}

/// A name through a registry, `dorean-local:books/romans`, split into the registry (one of
/// `registries`) and the package's name there
pub fn split_registry_name<'a>(
    text: &'a str,
    registries: &std::collections::BTreeMap<String, String>,
) -> Option<(&'a str, &'a str)> {
    let (registry, name) = text.split_once(':')?;
    (registries.contains_key(registry) && !name.is_empty()).then_some((registry, name))
}

/**
Text as search matches it: lowercase, decomposed (NFD) so that every combining mark (accents,
Greek breathings, Hebrew points and cantillation, Arabic vowels) can be dropped, then composed
again, with a final sigma as any other. `Ἰησοῦς` matches `ιησους`, `בְּרֵאשִׁית` `בראשית`, and
`José` `jose`
*/
fn fold(text: &str) -> String {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    text.to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| if c == 'ς' { 'σ' } else { c })
        .nfc()
        .collect()
}

/// How well `words` (each with the book it names, if any) match `entry`, if every one does
fn score(entry: &CatalogEntry, words: &[(String, Option<String>)]) -> Option<u32> {
    if words.is_empty() {
        return Some(0);
    }
    let exact = [fold(&entry.abbreviation), fold(&entry.id)];
    // Its work's names (`Authorized Version` finds every edition of the KJV)
    let work = (entry.work.as_deref()).and_then(crate::works::get);
    let names: Vec<String> = [&entry.name]
        .into_iter()
        .chain(&entry.aliases)
        .chain(&entry.language_name)
        .chain(&entry.keywords)
        .chain(work.map(|w| &w.name))
        .chain(work.into_iter().flat_map(|w| &w.aliases))
        .map(|s| fold(s))
        // Its language by any of its codes (`eng` finds `en`, and `en` finds `eng`)
        .chain(crate::language::search_codes(&entry.language))
        // `church-history` is two words
        .chain(entry.categories.iter().map(|c| fold(&c.replace('-', " "))))
        .collect();
    let description = entry.description.as_deref().map(fold).unwrap_or_default();
    let mut total = 0;
    for (word, book) in words {
        let about = book.as_ref().is_some_and(|b| entry.about_books.contains(b));
        total += if exact.iter().any(|e| e == word) {
            100
        } else if exact.iter().any(|e| e.contains(word.as_str())) {
            40
        } else if about
            || names
                .iter()
                .any(|n| n.split_whitespace().any(|w| w == word))
        {
            30
        } else if names.iter().any(|n| n.contains(word.as_str())) {
            20
        } else if description.contains(word.as_str()) {
            5
        } else {
            return None;
        };
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entries as logoxide-import reads eBible.org's list into (its ASV and the WLC), so the
    /// package manager's side is tested without a publisher
    fn ebible() -> Catalog {
        let entry = |id: &str, name: &str, abbreviation: &str, language: &str, name_of: &str| {
            let import = format!("ebible:{id}");
            CatalogEntry {
                id: format!("ebible-{id}"),
                name: name.into(),
                abbreviation: abbreviation.into(),
                aliases: vec![],
                language: language.into(),
                language_name: Some(name_of.into()),
                script: None,
                description: None,
                license: Some("LicenseRef-eBible".into()),
                rights: Default::default(),
                copyright: None,
                version: Some("2026-10-08".into()),
                kind: PackageKind::Bible,
                direction: Direction::Ltr,
                source: CatalogSource::External {
                    upstream: Some(format!("https://ebible.org/Scriptures/{id}_usfm.zip")),
                    import,
                    tracks_versions: true,
                    downloads: vec![],
                },
                path: None,
                releases: vec![],
                keywords: vec![],
                categories: vec![],
                about_books: vec![],
                work: None,
                edition: None,
                edition_year: None,
                keys: None,
            }
        };
        let mut entries = vec![
            entry(
                "eng-asv",
                "American Standard Version (1901)",
                "ASV",
                "en",
                "English",
            ),
            entry(
                "hebwlc",
                "Westminster Leningrad Codex",
                "WLC",
                "he",
                "Hebrew",
            ),
        ];
        known_works(&mut entries);
        Catalog {
            name: String::from("eBible.org"),
            entries,
            warnings: vec![],
        }
    }

    #[test]
    fn editions_are_grouped_by_work() {
        let mine = Catalog::parse_toml(
            r#"
[[package]]
id = "asv-1901"
name = "ASV, the 1901 printing"
abbreviation = "ASV1901"
language = "en"
work = "asv"
edition = "1901-nelson"
edition-year = 1901
git = "https://example.org/asv"

[[package]]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
language = "en"
git = "https://example.org/bsb"
"#,
        )
        .unwrap();
        let all = mine.merge(ebible());
        // eBible.org's ASV is the ASV by its import name
        let asv = all.find_import("ebible:eng-asv").unwrap();
        assert_eq!(asv.work.as_deref(), Some("asv"));
        assert_eq!(all.entries[0].edition_year, Some(1901));
        // Found by its work's name, and gathered under it
        let groups = all.search_grouped("american standard");
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].work.as_deref(), Some("asv"));
        assert_eq!(
            groups[0].name().as_deref(),
            Some("American Standard Version")
        );
        let ids: Vec<&str> = groups[0].entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["asv-1901", "ebible-eng-asv"]);
        // An entry of no work is a group of its own
        let groups = all.search_grouped("");
        assert_eq!(
            (groups.iter())
                .map(|g| (g.work.as_deref(), g.entries.len()))
                .collect::<Vec<_>>(),
            [(Some("asv"), 2), (None, 1), (None, 1)]
        );
        // A catalog writes them back
        let info = crate::Manifest::parse(
            "[package]\nid = \"kjv\"\nname = \"KJV\"\nabbreviation = \"KJV\"\nversion = \"1.0.0\"\n\
             language = \"en\"\nwork = \"kjv\"\nedition = \"1769-oxford\"\nedition-year = 1769\n\
             [formats]\nusx = { path = \"usx/\" }\n",
        )
        .unwrap()
        .package
        .unwrap();
        let text = Catalog::toml_entry(&info, "https://example.org/kjv");
        let entry = &Catalog::parse_toml(&text).unwrap().entries[0];
        assert_eq!(
            (
                entry.work.as_deref(),
                entry.edition.as_deref(),
                entry.edition_year
            ),
            (Some("kjv"), Some("1769-oxford"), Some(1769))
        );
    }

    #[test]
    fn a_dictionarys_keys_are_listed() {
        let info = crate::Manifest::parse(
            "[package]\nid = \"strongs\"\nname = \"Strong's\"\nabbreviation = \"SG\"\n\
             version = \"1.0.0\"\nlanguage = \"en\"\nkind = \"lexicon\"\n\
             keys = \"strongs-greek\"\n[formats]\ndictionary = { path = \"entries/\" }\n",
        )
        .unwrap()
        .package
        .unwrap();
        let text = Catalog::toml_entry(&info, "https://example.org/strongs");
        let entry = &Catalog::parse_toml(&text).unwrap().entries[0];
        assert_eq!(entry.keys, Some(crate::manifest::Keys::StrongsGreek));
        // A folder registry's entries carry them too
        let manifest = crate::Manifest {
            package: Some(info),
            ..crate::Manifest::default()
        };
        let folder = Catalog::from_packages("local", "/r", vec![("strongs".into(), manifest)]);
        assert_eq!(
            folder.entries[0].keys,
            Some(crate::manifest::Keys::StrongsGreek)
        );
        // A Bible's entry has none
        let bible = Catalog::parse_toml(
            "[[package]]\nid = \"b\"\nname = \"B\"\nabbreviation = \"B\"\nlanguage = \"en\"\n\
             git = \"https://example.org/b\"\n",
        )
        .unwrap();
        assert_eq!(bible.entries[0].keys, None);
    }

    #[test]
    fn finds_imports_by_prefix() {
        let catalog = ebible();
        let asv = catalog.find_import("ebible:eng-asv").unwrap();
        assert_eq!(asv.source.import_name(), Some("ebible:eng-asv"));
        assert_eq!(asv.source.prefix(), Some("ebible"));
        assert_eq!(asv.source.import_id(), Some("eng-asv"));
        assert!(asv.source.versions_track_updates());
        // Another kind's prefix, or none, finds nothing
        assert!(catalog.find_import("fetch:eng-asv").is_none());
        assert!(catalog.find_import("eng-asv").is_none());
        let git = CatalogSource::Git {
            url: "https://example.org/bsb".into(),
        };
        assert_eq!(git.import_name(), None);
    }

    #[test]
    fn lists_short_names_by_url() {
        let catalog = Catalog::parse_toml(
            "[[package]]\nid = \"kjv\"\nname = \"KJV\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\ngit = \"github:someone/kjv\"\n",
        )
        .unwrap();
        assert_eq!(
            catalog.entries[0].source,
            CatalogSource::Git {
                url: "https://github.com/someone/kjv".into()
            }
        );
    }

    #[test]
    fn folds_accents_points_and_final_sigma() {
        assert_eq!(fold("Ἰησοῦς"), fold("ιησους"));
        assert_eq!(fold("ΙΗΣΟΥΣ"), "ιησουσ");
        assert_eq!(fold("בְּרֵאשִׁית"), fold("בראשית"));
        assert_eq!(fold("José"), "jose");
        // Composed again: a Korean word stays whole syllables
        assert_eq!(fold("성경"), "성경");
        let catalog = Catalog::parse_toml(
            r#"
[[package]]
id = "tr"
name = "Ἡ Καινὴ Διαθήκη"
abbreviation = "TR"
language = "grc"
keywords = ["Ἰησοῦς"]
git = "https://example.org/tr"

[[package]]
id = "wlc"
name = "תּוֹרָה נְבִיאִים וּכְתוּבִים"
abbreviation = "WLC"
language = "hbo"
keywords = ["בְּרֵאשִׁית"]
git = "https://example.org/wlc"

[[package]]
id = "rv"
name = "Reina-Valera"
abbreviation = "RV"
language = "es"
description = "Traducción de Casiodoro de Reina, revisada por Cipriano de Valera; José"
git = "https://example.org/rv"

[[package]]
id = "luther"
name = "Lutherbibel"
abbreviation = "LUT"
language = "deu"
git = "https://example.org/luther"
"#,
        )
        .unwrap();
        let ids = |q: &str| (catalog.search(q).iter().map(|e| e.id.clone())).collect::<Vec<_>>();
        assert_eq!(ids("ιησους"), ["tr"]);
        assert_eq!(ids("καινη διαθηκη"), ["tr"]);
        assert_eq!(ids("בראשית"), ["wlc"]);
        assert_eq!(ids("תורה"), ["wlc"]);
        assert_eq!(ids("jose"), ["rv"]);
        assert_eq!(ids("traduccion"), ["rv"]);
        // A language by any of its codes: `spa` finds `es`, and `de` finds a package that
        // says `deu`
        assert_eq!(ids("spa"), ["rv"]);
        assert_eq!(ids("ger"), ["luther"]);
        assert_eq!(ids("de").first().map(String::as_str), Some("luther"));
    }

    #[test]
    fn searches_and_merges() {
        let ebible = ebible();
        let mine = Catalog::parse_toml(
            r#"
name = "Mine"
[[package]]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
language = "en"
language-name = "English"
git = "https://example.org/bsb"
"#,
        )
        .unwrap();
        let all = mine.merge(ebible);
        let ids = |q: &str| {
            all.search(q)
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("asv"), ["ebible-eng-asv"]);
        assert_eq!(ids("english"), ["ebible-eng-asv", "bsb"]);
        assert_eq!(ids("hebrew codex"), ["ebible-hebwlc"]);
        assert_eq!(ids("berean"), ["bsb"]);
        assert!(ids("nothing like this").is_empty());
        assert_eq!(ids("").len(), 3);
        assert!(Catalog::parse_toml("[[package]]\nid = 1").is_err());
    }

    #[test]
    fn finds_what_a_package_is_about() {
        let info = crate::manifest::Manifest::parse(
            r#"[package]
id = "gill-romans"
name = "Gill on Romans"
abbreviation = "GILL"
version = "1.0.0"
language = "en"
kind = "commentary"
keywords = ["justification"]
categories = ["church-history"]
about-books = ["ROM"]
[formats]
usx = { path = "usx/" }
"#,
        )
        .unwrap()
        .package
        .unwrap();
        // A catalog keeps them, and reads them back
        let text = Catalog::toml_entry(&info, "https://example.org/gill");
        assert!(text.contains("about-books = [\"ROM\"]"), "{text}");
        let catalog = Catalog::parse_toml(&text).unwrap().merge(ebible());
        let gill = &catalog.entries[0];
        assert_eq!(
            (
                &gill.keywords[..],
                &gill.categories[..],
                &gill.about_books[..]
            ),
            (
                &["justification".to_string()][..],
                &["church-history".to_string()][..],
                &["ROM".to_string()][..]
            )
        );
        assert!(gill.is_about("ROM") && !gill.is_about("GAL"));
        let ids = |q: &str| {
            catalog
                .search(q)
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
        };
        // By the book's name or code, a keyword, or a category's words
        assert_eq!(ids("romans"), ["gill-romans"]);
        assert_eq!(ids("rom"), ["gill-romans"]);
        assert_eq!(ids("justification"), ["gill-romans"]);
        assert_eq!(ids("church history"), ["gill-romans"]);
        assert!(ids("galatians").is_empty());
    }

    #[test]
    fn writes_an_entry() {
        let manifest = crate::Manifest::parse(
            "[package]\nid = \"bsb\"\nname = \"Berean Standard Bible\"\nabbreviation = \"BSB\"\nversion = \"1.0.0\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\"\n[formats]\nusx = { path = \"usx/\" }\n",
        )
        .unwrap();
        let text = Catalog::toml_entry(
            manifest.package.as_ref().unwrap(),
            "https://example.org/bsb",
        );
        let catalog = Catalog::parse_toml(&text).unwrap();
        assert_eq!(catalog.entries[0].id, "bsb");
        assert_eq!(
            catalog.entries[0].source,
            CatalogSource::Git {
                url: "https://example.org/bsb".into()
            }
        );
    }

    #[test]
    fn a_registry_folder_names_packages_by_path_or_id() {
        let package = |id: &str| {
            crate::manifest::Manifest::parse(&format!(
                "[package]\nid = \"{id}\"\nname = \"{id}\"\nabbreviation = \"X\"\n\
                 version = \"1.0.0\"\nlanguage = \"en\"\n\
                 [formats]\nusx = {{ path = \"usx/\" }}\n"
            ))
            .unwrap()
        };
        let vault = crate::manifest::Manifest::parse("[dependencies]\n").unwrap();
        let catalog = Catalog::from_packages(
            "local",
            "file:///packages/",
            vec![
                (String::from("kjv"), package("kjv")),
                (String::from("books/romans"), package("romans")),
                (String::from("forks/romans"), package("romans")),
                (String::from("vault"), vault),
            ],
        );
        assert_eq!(catalog.entries.len(), 3);
        let url = |name: &str| match &catalog.find(name).unwrap().unwrap().source {
            CatalogSource::Git { url } => url.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(url("kjv"), "file:///packages/kjv");
        assert_eq!(url("books/romans"), "file:///packages/books/romans");
        assert_eq!(url("/forks/romans/"), "file:///packages/forks/romans");
        // Two packages are `romans`: the path has to say which
        let error = catalog.find("romans").unwrap_err().to_string();
        assert!(error.contains("books/romans, forks/romans"), "{error}");
        assert!(catalog.find("asv").unwrap().is_none());
    }

    #[test]
    fn registry_names_are_checked_and_split() {
        assert!(check_registry_name("dorean-local").is_ok());
        assert!(check_registry_name("github").is_err());
        assert!(check_registry_name("Dorean").is_err());
        assert!(check_registry_name("").is_err());
        let registries = std::collections::BTreeMap::from([(
            String::from("dorean-local"),
            String::from("~/repos"),
        )]);
        assert_eq!(
            split_registry_name("dorean-local:books/romans", &registries),
            Some(("dorean-local", "books/romans"))
        );
        assert_eq!(split_registry_name("github:a/b", &registries), None);
        assert_eq!(split_registry_name("dorean-local:", &registries), None);
    }

    #[test]
    fn a_built_catalog_names_its_packages_by_path() {
        let manifest = crate::manifest::Manifest::parse(
            "[package]\nid = \"romans\"\nname = \"Romans\"\nabbreviation = \"ROM\"\n\
             version = \"1.2.0\"\nlanguage = \"en\"\nkind = \"book\"\n\
             [formats]\nepub = { path = \"epub/\" }\n",
        )
        .unwrap();
        let info = manifest.package.unwrap();
        let mut table = Catalog::entry_table(&info, "file:///repos/books/romans");
        table.insert("path", toml_edit::value("books/romans"));
        table.insert("commit", toml_edit::value("3f9c2a1b"));
        let text = Catalog::toml_document(Some("mine"), vec![table]);
        let catalog = Catalog::parse("catalog.toml", &text).unwrap();
        assert_eq!(catalog.name, "mine");
        let entry = catalog.find("books/romans").unwrap().unwrap();
        assert_eq!(entry.id, "romans");
        assert_eq!(entry.version.as_deref(), Some("1.2.0"));
        assert_eq!(entry.kind, PackageKind::Book);
        assert_eq!(catalog.find("romans").unwrap(), Some(entry));
    }

    const ENTRY: &str =
        "[[package]]\nid = \"kjv\"\nname = \"KJV\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\n";

    fn release(url: &str) -> String {
        format!(
            "{ENTRY}[[package.release]]\nversion = \"1.0.0\"\nurl = \"{url}\"\nsha256 = \"sha256:00\"\n"
        )
    }

    #[test]
    fn a_remote_catalog_names_nothing_local() {
        let remote = "https://example.org/r/catalog.toml";
        for url in ["/etc/passwd", "file:///etc/passwd"] {
            let error = Catalog::parse(remote, &release(url)).unwrap_err();
            assert!(error.to_string().contains("on this machine"), "{error}");
            // A local catalog may
            assert!(Catalog::parse("/srv/r/catalog.toml", &release(url)).is_ok());
        }
        let mirrored = format!(
            "{}mirrors = [\"/srv/kjv.tar.gz\"]\n",
            release("kjv-1.0.0.tar.gz")
        );
        assert!(Catalog::parse(remote, &mirrored).is_err());
        let git = format!("{ENTRY}git = \"/srv/kjv\"\n");
        assert!(Catalog::parse(remote, &git).is_err());
        // Relative ones are beside it, so remote too
        let catalog = Catalog::parse(remote, &release("kjv/kjv-1.0.0.tar.gz")).unwrap();
        assert_eq!(
            catalog.entries[0].releases[0].url,
            "https://example.org/r/kjv/kjv-1.0.0.tar.gz"
        );
    }

    #[test]
    fn catalogs_say_their_format() {
        assert!(Catalog::parse_toml(&format!("format = 1\n{ENTRY}git = \"x\"\n")).is_ok());
        let error = Catalog::parse_toml(&format!("format = 2\n{ENTRY}git = \"x\"\n")).unwrap_err();
        assert!(error.to_string().contains("upgrade lx"), "{error}");
        let written = Catalog::toml_document(Some("mine"), vec![]);
        assert!(written.starts_with("format = 1\n"), "{written}");
        assert!(Catalog::parse_toml(&written).is_ok());
    }

    #[test]
    fn entries_whose_ids_arent_are_passed_over() {
        let entry = |id: &str| {
            ENTRY.replace("\"kjv\"", &format!("\"{id}\"")) + "git = \"https://example.com/x\"\n"
        };
        let text: String = ["kjv", "../../etc", "KJV", "", "bsb"].map(entry).concat();
        let catalog =
            Catalog::parse_toml_at(Some("https://example.com/catalog.toml"), &text).unwrap();
        let ids: Vec<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["kjv", "bsb"]);
        assert_eq!(catalog.warnings.len(), 3, "{:?}", catalog.warnings);
        assert!(
            catalog.warnings[0].starts_with("https://example.com/catalog.toml: passed over")
                && catalog.warnings[0].contains("`../../etc`"),
            "{:?}",
            catalog.warnings
        );
        // Merging keeps every catalog's warnings
        let merged = Catalog::default().merge(catalog);
        assert_eq!(merged.warnings.len(), 3);
    }

    #[test]
    fn shadowed_ids_are_found() {
        let catalog = |name: &str, ids: &[&str]| {
            let entries: String = (ids.iter())
                .map(|id| ENTRY.replace("\"kjv\"", &format!("\"{id}\"")) + "git = \"x\"\n")
                .collect();
            let mut catalog = Catalog::parse_toml(&entries).unwrap();
            catalog.name = name.to_string();
            catalog
        };
        let catalogs = [
            catalog("mine", &["kjv", "bsb"]),
            catalog("ebible", &["web", "kjv"]),
            catalog("other", &["kjv", "web", "asv"]),
        ];
        let names = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(
            Catalog::shadowed(&catalogs),
            [
                (String::from("kjv"), names(&["mine", "ebible", "other"])),
                (String::from("web"), names(&["ebible", "other"])),
            ]
        );
        // Merging still keeps the first
        let merged = (catalogs.iter().cloned()).reduce(Catalog::merge).unwrap();
        assert_eq!(merged.entries.len(), 4);
        let ids: Vec<&str> = merged.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["kjv", "bsb", "web", "asv"]);
    }
}
