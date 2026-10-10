/*!
Catalogs: lists of packages to search, so a Bible can be found by its name or language instead of
its URL. A catalog holds what each package is and where it comes from, never its text, so anyone
can host one (a file in a git repository is enough). Two kinds are read:

- **biblix catalogs** (`catalog.toml`): packages published as git repositories, each as its
  `biblix.toml` describes it (`bx catalog entry <url>` writes an entry)
- **eBible.org's list** (`translations.csv`, [`EBIBLE_CATALOG`]): about 1,500 translations it
  publishes as USFM, imported into packages when installed ([`crate::import`])
- **fetch.bible's manifest** (`manifest.json`, [`FETCH_BIBLE_CATALOG`]): about 1,400 translations
  it publishes as USX, a file per book, imported the same way
- **CCEL's Bibles** (its page of them, [`CCEL_CATALOG`]): about 50 Bibles it publishes as ThML,
  a file each. The page gives only titles: the language, rights, and version are in each file's
  head, read when it's imported
- **SWORD repositories** (a repository's `mods.d.tar.gz`, every module's conf;
  [`SWORD_CATALOG`] is CrossWire's): the Bibles and commentaries whose markup is OSIS or ThML,
  and which aren't locked, each a module zip imported into a package

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

A vault names its catalogs in `[settings] catalogs` (URLs or paths; a `.csv` is eBible.org's
form, a `.json` fetch.bible's); without any, eBible.org's and fetch.bible's lists are searched.

**Registries** are catalogs with a name (`[settings.registries]`), so a package can be named
through one: `dorean-local:dorean-collection` by its id, or `dorean-local:books/romans` by its
path there. A registry is a catalog, or a folder of package repositories: each git repository
with a `biblix.toml` in it, at any depth, is a package ([`Catalog::from_packages`]), named by
its path in the folder and found at its `file://` URL.

```toml
[settings.registries]
dorean-local = "~/Documents/biblix-dorean-repos"
```
*/

use serde::{Deserialize, Serialize};

use crate::{
    error::{BiblixError, Result},
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

/**
The SWORD repositories in CrossWire's list of them (`masterRepoList.conf`) served over HTTP(S), by
the name an import gives each (`sword:crosswire/KJV`). A repository's modules are at its root:
its catalog is `mods.d.tar.gz` there, a module's zip `packages/<module>.zip`. Others are named by
their host ([`sword_repository_name`])
*/
pub const SWORD_REPOSITORIES: [(&str, &str); 9] = [
    (
        "crosswire",
        "https://crosswire.org/ftpmirror/pub/sword/raw/",
    ),
    (
        "crosswire-beta",
        "https://crosswire.org/ftpmirror/pub/sword/betaraw/",
    ),
    (
        "crosswire-attic",
        "https://crosswire.org/ftpmirror/pub/sword/atticraw/",
    ),
    (
        "crosswire-wycliffe",
        "https://crosswire.org/ftpmirror/pub/sword/wyclifferaw/",
    ),
    (
        "bible-org",
        "https://crosswire.org/ftpmirror/pub/bible.org/sword/",
    ),
    ("ebible", "https://ebible.org/sword/"),
    ("dbg", "https://crosswire.org/ftpmirror/pub/sword/dbgraw/"),
    (
        "lockman",
        "https://crosswire.org/ftpmirror/pub/sword/lockmanraw/",
    ),
    ("xiphos", "http://ftp.xiphos.org/pub/xiphos/"),
];

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
    /// A biblix package in a git repository
    Git { url: String },
    /// A translation eBible.org publishes as USFM: `id`'s zip, imported into a package
    Ebible { id: String },
    /// A translation fetch.bible publishes as USX: each of `books` (lowercase USFM codes),
    /// imported into a package
    FetchBible { id: String, books: Vec<String> },
    /// A Bible CCEL publishes as ThML (`kjv`: `https://ccel.org/ccel/bible/kjv.xml`), imported
    /// into a package
    Ccel { id: String },
    /// A module a SWORD repository publishes (`repository` is its root URL, `module` its name,
    /// `id` the two as an import names them: `crosswire/KJV`), imported into a package
    Sword {
        id: String,
        repository: String,
        module: String,
    },
    /// A package a registry publishes as archives (its entry's `releases`): `catalog` is the
    /// registry's catalog, which a dependency names as its `registry`
    Registry { catalog: String },
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
    /// Withdrawn (`bx yank`): new installs don't choose it, but a lock file that has it still
    /// installs it, so nobody's text changes under them
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yanked: bool,
}

impl CatalogSource {
    /// Every kind's import prefix: what `ebible:<id>` and `fetch:<id>` start with
    pub const PREFIXES: [&'static str; 4] = ["ebible", "fetch", "ccel", "sword"];

    /// The prefix that names this kind's imports (`ebible`, `fetch`); a git package has none,
    /// since it's added by its URL
    pub fn prefix(&self) -> Option<&'static str> {
        match self {
            CatalogSource::Git { .. } | CatalogSource::Registry { .. } => None,
            CatalogSource::Ebible { .. } => Some("ebible"),
            CatalogSource::FetchBible { .. } => Some("fetch"),
            CatalogSource::Ccel { .. } => Some("ccel"),
            CatalogSource::Sword { .. } => Some("sword"),
        }
    }

    /// The publisher's own id, for the kinds that are imported
    pub fn import_id(&self) -> Option<&str> {
        match self {
            CatalogSource::Git { .. } | CatalogSource::Registry { .. } => None,
            CatalogSource::Ebible { id }
            | CatalogSource::FetchBible { id, .. }
            | CatalogSource::Ccel { id }
            | CatalogSource::Sword { id, .. } => Some(id),
        }
    }

    /**
    Whether a newer download always has a newer version here, so an import at the catalog's
    version needn't be downloaded again to know it's current: eBible.org's update dates and
    SWORD modules' `Version`s do; fetch.bible's year and CCEL's (unlisted) edition don't
    */
    pub fn versions_track_updates(&self) -> bool {
        matches!(
            self,
            CatalogSource::Ebible { .. } | CatalogSource::Sword { .. }
        )
    }

    /// What `bx import` takes for it: `ebible:eng-asv`, `fetch:eng_asv`
    pub fn import_name(&self) -> Option<String> {
        Some(format!("{}:{}", self.prefix()?, self.import_id()?))
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
    /// The language's name, when the catalog gives it (`English`)
    pub language_name: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    /// A copyright statement, when the catalog gives one
    pub copyright: Option<String>,
    /// The newest version, when the catalog says (eBible.org's: its update date, `2026.10.8`)
    pub version: Option<String>,
    pub kind: PackageKind,
    pub direction: Direction,
    pub source: CatalogSource,
    /// Its path in a registry folder (`books/romans`), which names it there as its id does
    pub path: Option<String>,
    /// The releases a registry publishes as archives, oldest first (none for other sources)
    pub releases: Vec<CatalogRelease>,
}

impl CatalogEntry {
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
    description: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    kind: PackageKind,
    #[serde(default)]
    direction: Direction,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    git: Option<String>,
    /// A registry's archives (`[[package.release]]`)
    #[serde(default, rename = "release")]
    releases: Vec<CatalogRelease>,
    /// Its path in the folder the catalog was built from (`bx catalog build`), which names it
    /// as its id does
    #[serde(default)]
    path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct TomlCatalog {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "package")]
    packages: Vec<TomlEntry>,
}

/// A catalog's packages
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub name: String,
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// A catalog from a file's bytes: a SWORD repository's when `name` ends in `.tar.gz`, else
    /// its text ([`Catalog::parse`])
    pub fn parse_bytes(name: &str, bytes: &[u8]) -> Result<Catalog> {
        let path = name.split(['?', '#']).next().unwrap_or(name);
        match path.to_ascii_lowercase().ends_with(".tar.gz") {
            true => Catalog::parse_sword(name, bytes),
            false => Catalog::parse(name, &String::from_utf8_lossy(bytes)),
        }
    }

    /**
    A SWORD repository's catalog: its `mods.d.tar.gz` (at `location`, its root's
    `mods.d.tar.gz`), every module's conf. Listed are the Bibles and commentaries that aren't
    locked (enciphered without a key) and whose markup is OSIS or ThML (what biblix reads),
    each with its conf's description, language, version, and license
    */
    pub fn parse_sword(location: &str, bytes: &[u8]) -> Result<Catalog> {
        use std::io::Read;
        use sword_modules::{Markup, ModuleKind};
        let bad = |e: std::io::Error| BiblixError::Catalog(format!("mods.d.tar.gz: {e}"));
        let repository = location.strip_suffix("mods.d.tar.gz").unwrap_or(location);
        let short = sword_repository_name(repository);
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        let mut entries = vec![];
        for file in archive.entries().map_err(bad)? {
            let mut file = file.map_err(bad)?;
            let is_conf = (file.path().ok())
                .is_some_and(|p| p.to_string_lossy().to_ascii_lowercase().ends_with(".conf"));
            if !file.header().entry_type().is_file() || !is_conf {
                continue;
            }
            let mut data = vec![];
            file.by_ref()
                .take(1 << 20)
                .read_to_end(&mut data)
                .map_err(bad)?;
            let Ok(confs) = sword_modules::conf::parse_conf(&conf_text(&data)) else {
                continue;
            };
            for conf in confs {
                let kind = match conf.module_kind() {
                    Ok(ModuleKind::Bible) => PackageKind::Bible,
                    Ok(ModuleKind::Commentary) => PackageKind::Commentary,
                    _ => continue,
                };
                let readable = matches!(conf.source_type(), Markup::Osis | Markup::Thml);
                if conf.is_locked() || !readable || conf.get("ModDrv") == Some("RawFiles") {
                    continue;
                }
                entries.push(sword_entry(&conf, kind, &short, repository));
            }
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Catalog {
            name: format!("SWORD: {short}"),
            entries,
        })
    }

    /// A catalog from a file's text: eBible.org's CSV when `name` ends in `.csv`, fetch.bible's
    /// manifest when it ends in `.json`, CCEL's page when it's on ccel.org, else a
    /// `catalog.toml`
    pub fn parse(name: &str, text: &str) -> Result<Catalog> {
        let path = name
            .split(['?', '#'])
            .next()
            .unwrap_or(name)
            .to_ascii_lowercase();
        if path.contains("ccel.org/ccel/") {
            Catalog::parse_ccel(text)
        } else if path.ends_with(".csv") {
            Catalog::parse_ebible(text)
        } else if path.ends_with(".json") {
            Catalog::parse_fetch_bible(text)
        } else {
            Catalog::parse_toml_at(Some(name), text)
        }
    }

    /// fetch.bible's `manifest.json`: its translations, with their books and licenses
    pub fn parse_fetch_bible(text: &str) -> Result<Catalog> {
        use serde_json::Value;
        let bad = |message: &str| BiblixError::Catalog(format!("manifest.json: {message}"));
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
            let language = languages.and_then(|l| l.get(&code));
            let copyright = bible.get("copyright");
            let license = (copyright.and_then(|c| c.get("licenses")))
                .and_then(Value::as_array)
                .and_then(|l| l.first())
                .map(|l| fetch_bible_license(l.get("license"), text_of(l.get("url"))));
            let abbreviation = name_of("english_abbrev")
                .or_else(|| name_of("local_abbrev"))
                .unwrap_or_else(|| id.to_uppercase());
            let english = name_of("english");
            let local = name_of("local");
            entries.push(CatalogEntry {
                id: package_id(id),
                name: english
                    .clone()
                    .or_else(|| local.clone())
                    .unwrap_or_else(|| id.clone()),
                aliases: local
                    .filter(|l| Some(l) != english.as_ref())
                    .into_iter()
                    .collect(),
                abbreviation,
                language_name: text_of(language.and_then(|l| l.get("english"))),
                language: code,
                description: None,
                license,
                copyright: text_of(copyright.and_then(|c| c.get("attribution"))),
                version: (bible.get("year").and_then(Value::as_u64)).map(|y| format!("{y}.0.0")),
                kind: PackageKind::Bible,
                direction: match bible.get("direction").and_then(Value::as_str) {
                    Some("rtl") => Direction::Rtl,
                    _ => Direction::Ltr,
                },
                source: CatalogSource::FetchBible {
                    id: id.clone(),
                    books,
                },
                path: None,
                releases: vec![],
            });
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Catalog {
            name: String::from("fetch.bible"),
            entries,
        })
    }

    /**
    CCEL's page of its Bibles ([`CCEL_CATALOG`]): each work's id and title, from the links in
    its headings (`<h2><a href="https://ccel.org/ccel/bible/kjv/kjv">King James Version of the
    Holy Bible</a></h2>`). The page doesn't say a work's language or rights (its file's head
    does, [`crate::import::ccel`]): the language is `und` until then, and the license unknown
    */
    pub fn parse_ccel(text: &str) -> Result<Catalog> {
        let mut entries: Vec<CatalogEntry> = vec![];
        for heading in text.split("<h2").skip(1) {
            let Some((heading, _)) = heading.split_once("</h2>") else {
                continue;
            };
            let Some(href) = (heading.split_once("href=\""))
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(href, _)| href)
            else {
                continue;
            };
            // `…/ccel/bible/<work>/<work>`: a work of the author `bible`
            let mut parts = href.trim_end_matches('/').rsplit('/');
            let (Some(_), Some(work), Some("bible")) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let title = (heading.split_once('>'))
                .and_then(|(_, rest)| rest.split_once("<a"))
                .map(|(_, rest)| rest)
                .and_then(|rest| rest.split_once('>'))
                .and_then(|(_, rest)| rest.split_once("</a>"))
                .map(|(title, _)| topos_bible_markup::decode_entities(title))
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| work.to_string());
            if (entries.iter()).any(|e| e.source.import_id() == Some(work)) {
                continue;
            }
            entries.push(CatalogEntry {
                // Prefixed: CCEL's ids (`kjv`, `web`) are other catalogs' too, and the first of
                // an id wins when catalogs merge
                id: format!("ccel-{}", package_id(work)),
                name: title,
                abbreviation: work.to_uppercase(),
                aliases: vec![],
                language: String::from("und"),
                language_name: None,
                description: None,
                license: None,
                copyright: None,
                version: None,
                kind: PackageKind::Bible,
                direction: Direction::Ltr,
                source: CatalogSource::Ccel {
                    id: work.to_string(),
                },
                path: None,
                releases: vec![],
            });
        }
        if entries.is_empty() {
            return Err(BiblixError::Catalog(String::from(
                "CCEL's page lists no Bibles (has it changed?)",
            )));
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Catalog {
            name: String::from("CCEL"),
            entries,
        })
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
            toml::from_str(text).map_err(|e| BiblixError::Catalog(e.to_string()))?;
        let mut entries = vec![];
        for e in catalog.packages {
            let source = match &e.git {
                // `github:owner/repo` is listed by its URL
                Some(git) => CatalogSource::Git {
                    url: crate::git::expand_url(git),
                },
                None if e.releases.is_empty() => {
                    return Err(BiblixError::Catalog(format!(
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
                .collect();
            entries.push(CatalogEntry {
                id: e.id,
                name: e.name,
                abbreviation: e.abbreviation,
                aliases: e.aliases,
                language: e.language,
                language_name: e.language_name,
                description: e.description,
                license: e.license,
                copyright: None,
                version: e.version,
                kind: e.kind,
                direction: e.direction,
                source,
                path: e.path,
                releases,
            });
        }
        Ok(Catalog {
            name: catalog.name.unwrap_or_default(),
            entries,
        })
    }

    /// eBible.org's `translations.csv`: the translations it lets anyone download
    pub fn parse_ebible(text: &str) -> Result<Catalog> {
        let rows = csv(text.trim_start_matches('\u{feff}'));
        let mut rows = rows.into_iter();
        let header = rows
            .next()
            .ok_or_else(|| BiblixError::Catalog(String::from("an empty translations.csv")))?;
        let column = |name: &str| {
            header.iter().position(|h| h == name).ok_or_else(|| {
                BiblixError::Catalog(format!("translations.csv has no {name} column"))
            })
        };
        let [
            language,
            id,
            language_name,
            title,
            description,
            copyright,
            short_title,
        ] = [
            "languageCode",
            "translationId",
            "languageNameInEnglish",
            "title",
            "description",
            "Copyright",
            "shortTitle",
        ]
        .map(column);
        let [
            language,
            id,
            language_name,
            title,
            description,
            copyright,
            short_title,
        ] = [
            language?,
            id?,
            language_name?,
            title?,
            description?,
            copyright?,
            short_title?,
        ];
        let [redistributable, downloadable, direction, updated] = [
            "Redistributable",
            "downloadable",
            "textDirection",
            "UpdateDate",
        ]
        .map(column);
        let [redistributable, downloadable, direction, updated] =
            [redistributable?, downloadable?, direction?, updated?];
        let get = |row: &[String], i: usize| row.get(i).map(|s| s.trim().to_string());
        let yes = |row: &[String], i: usize| get(row, i).is_some_and(|v| v == "True");
        let mut entries = vec![];
        for row in rows {
            if !yes(&row, downloadable) || !yes(&row, redistributable) {
                continue;
            }
            let (Some(code), Some(ebible_id)) = (get(&row, language), get(&row, id)) else {
                continue;
            };
            let copyright = get(&row, copyright).filter(|c| !c.is_empty());
            let public = copyright
                .as_deref()
                .is_some_and(|c| c.to_ascii_lowercase().contains("public domain"));
            let name = get(&row, title)
                .filter(|t| !t.is_empty())
                .or_else(|| get(&row, short_title))
                .unwrap_or_else(|| ebible_id.clone());
            entries.push(CatalogEntry {
                id: package_id(&ebible_id),
                abbreviation: abbreviation(&ebible_id, &code),
                name,
                aliases: vec![],
                language_name: get(&row, language_name).filter(|n| !n.is_empty()),
                language: code,
                description: get(&row, description).filter(|d| !d.is_empty()),
                license: Some(String::from(if public {
                    "LicenseRef-PublicDomain"
                } else {
                    "LicenseRef-eBible"
                })),
                copyright,
                version: get(&row, updated).and_then(|d| date_version(&d)),
                kind: PackageKind::Bible,
                direction: match get(&row, direction).as_deref() {
                    Some("rtl") => Direction::Rtl,
                    _ => Direction::Ltr,
                },
                source: CatalogSource::Ebible { id: ebible_id },
                path: None,
                releases: vec![],
            });
        }
        Ok(Catalog {
            name: String::from("eBible.org"),
            entries,
        })
    }

    /// The entries `query` matches, best first: every word of it must be in the entry's id,
    /// name, abbreviation, aliases, language, or description (ignoring case); an exact
    /// abbreviation or id comes first, then names and languages, then descriptions
    pub fn search(&self, query: &str) -> Vec<&CatalogEntry> {
        let words: Vec<String> = query.split_whitespace().map(fold).collect();
        let mut found: Vec<(u32, &CatalogEntry)> = (self.entries.iter())
            .filter_map(|entry| Some((score(entry, &words)?, entry)))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        found.into_iter().map(|(_, e)| e).collect()
    }

    /// The entry an import name (`ebible:eng-asv`, as [`CatalogSource::import_name`] writes it)
    /// names, if this catalog lists it
    pub fn find_import(&self, name: &str) -> Option<&CatalogEntry> {
        (self.entries.iter()).find(|e| e.source.import_name().as_deref() == Some(name))
    }

    /// The entry an imported package was made from, by the `upstream` its import wrote
    /// ([`crate::import::upstream`])
    pub fn find_upstream(&self, upstream: &str) -> Option<&CatalogEntry> {
        (self.entries.iter())
            .find(|e| crate::import::upstream(e).as_deref() == Some(upstream.trim()))
    }

    /**
    A registry folder's packages: each `(path, manifest)` is a repository at that path in the
    folder (`books/romans`, `/`-separated) and its `biblix.toml`, at `<base_url>/<path>`. A
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
                    id: info.id,
                    name: info.name,
                    abbreviation: info.abbreviation,
                    aliases: info.aliases,
                    language: info.language,
                    language_name: None,
                    description: info.description,
                    license: info.license,
                    copyright: None,
                    version: Some(info.version.to_string()),
                    kind: info.kind,
                    direction: info.direction,
                    source: CatalogSource::Git {
                        url: format!("{base}/{path}"),
                    },
                    path: Some(path),
                    releases: vec![],
                })
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Catalog {
            name: name.to_string(),
            entries,
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
                Err(BiblixError::Catalog(format!(
                    "{} packages are `{name}` in {}: name one by its path ({})",
                    many.len(),
                    self.name,
                    paths.join(", ")
                )))
            }
        }
    }

    /// This catalog with another's entries after its own (an id listed twice keeps the first)
    pub fn merge(mut self, other: Catalog) -> Catalog {
        for entry in other.entries {
            if !self.entries.iter().any(|e| e.id == entry.id) {
                self.entries.push(entry);
            }
        }
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
        if let Some(description) = &info.description {
            table.insert("description", toml_edit::value(description));
        }
        table.insert("version", toml_edit::value(info.version.to_string()));
        if let Some(license) = &info.license {
            table.insert("license", toml_edit::value(license));
        }
        if info.kind != PackageKind::Bible {
            let kind = toml::Value::try_from(info.kind)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            table.insert("kind", toml_edit::value(kind));
        }
        if info.direction == Direction::Rtl {
            table.insert("direction", toml_edit::value("rtl"));
        }
        table.insert("git", toml_edit::value(git));
        table
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
        return Err(BiblixError::Catalog(format!(
            "registry name `{name}`: use lowercase letters, digits, `-`, and `_`"
        )));
    }
    if RESERVED_REGISTRY_NAMES.contains(&name) {
        return Err(BiblixError::Catalog(format!(
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

/// A fetch.bible license as SPDX: `cc-by-sa` with its URL's version is `CC-BY-SA-4.0`, `public`
/// is public domain, and its own terms (an object of restrictions) are `LicenseRef-fetch.bible`
fn fetch_bible_license(license: Option<&serde_json::Value>, url: Option<String>) -> String {
    match license.and_then(serde_json::Value::as_str) {
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
    }
}

/// What an import names a SWORD repository by (its root URL): its name in
/// [`SWORD_REPOSITORIES`], else its host's (`sword.example.org` is `sword-example-org`)
pub fn sword_repository_name(repository: &str) -> String {
    let root = repository.trim_end_matches('/');
    if let Some((name, _)) =
        (SWORD_REPOSITORIES.iter()).find(|(_, url)| url.trim_end_matches('/') == root)
    {
        return name.to_string();
    }
    let host = root.split("://").nth(1).unwrap_or(root);
    let host = host.split('/').next().unwrap_or(host);
    package_id(host.trim_start_matches("www.")).replace('.', "-")
}

/// A SWORD repository's catalog, by the name an import gives it (`crosswire`)
pub fn sword_catalog_url(name: &str) -> Option<String> {
    (SWORD_REPOSITORIES.iter())
        .find(|(n, _)| *n == name)
        .map(|(_, url)| format!("{url}mods.d.tar.gz"))
}

/// A conf file's text: UTF-8, else Latin-1 (as older confs are)
fn conf_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// A SWORD module's catalog entry, from its conf
pub(crate) fn sword_entry(
    conf: &sword_modules::ModuleConf,
    kind: PackageKind,
    repository_name: &str,
    repository: &str,
) -> CatalogEntry {
    let module = conf.name().to_string();
    let text = |key: &str| {
        (conf.get(key))
            .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|v| !v.is_empty())
    };
    // A repository other than CrossWire's adds its name, so its module's id is its own
    let id = match repository_name {
        "crosswire" => format!("sword-{}", package_id(&module)),
        other => format!("sword-{other}-{}", package_id(&module)),
    };
    let copyright = text("ShortCopyright").or_else(|| text("Copyright"));
    CatalogEntry {
        id,
        name: text("Description").unwrap_or_else(|| module.clone()),
        abbreviation: text("Abbreviation").unwrap_or_else(|| module.clone()),
        aliases: vec![],
        language: conf.lang().unwrap_or("und").to_string(),
        language_name: None,
        // Its first paragraph (the whole is in the package's license file)
        description: (conf.about_text()).and_then(|a| {
            (a.split('\n'))
                .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                .find(|p| !p.is_empty())
        }),
        license: Some(sword_license(conf.get("DistributionLicense"))),
        copyright,
        version: conf.version().and_then(dotted_version),
        kind,
        direction: match conf.direction() {
            sword_modules::conf::Direction::RightToLeft => Direction::Rtl,
            _ => Direction::Ltr,
        },
        source: CatalogSource::Sword {
            id: format!("{repository_name}/{module}"),
            repository: repository.to_string(),
            module,
        },
        path: None,
        releases: vec![],
    }
}

/**
A SWORD module's `DistributionLicense` as SPDX: public domain, a Creative Commons license
(`Creative Commons: BY-SA 4.0` is `CC-BY-SA-4.0`), the GPL; anything else (`Copyrighted; Free
non-commercial distribution`) is `LicenseRef-SWORD`, its statement in the package's license file
*/
pub(crate) fn sword_license(license: Option<&str>) -> String {
    let Some(license) = license.map(str::trim) else {
        return String::from("LicenseRef-SWORD");
    };
    let lower = license.to_ascii_lowercase();
    if lower.contains("public domain") {
        return String::from("LicenseRef-PublicDomain");
    }
    if let Some(cc) = lower.strip_prefix("creative commons:") {
        let mut parts = cc.split_whitespace();
        let (Some(terms), version) = (parts.next(), parts.next()) else {
            return String::from("LicenseRef-SWORD");
        };
        if terms.eq_ignore_ascii_case("zero") || terms.eq_ignore_ascii_case("cc0") {
            return String::from("CC0-1.0");
        }
        return format!("CC-{}-{}", terms.to_uppercase(), version.unwrap_or("4.0"));
    }
    match lower.as_str() {
        "gpl" | "gplv2" | "gpl v2" => String::from("GPL-2.0-only"),
        "gplv3" | "gpl v3" => String::from("GPL-3.0-only"),
        _ => String::from("LicenseRef-SWORD"),
    }
}

/// A semantic version from a dotted one (`1.0` is `1.0.0`, `3.1` is `3.1.0`; more parts than
/// three, or anything else, is none)
pub(crate) fn dotted_version(text: &str) -> Option<String> {
    let parts: Vec<u64> = (text.trim().split('.'))
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [major] => Some(format!("{major}.0.0")),
        [major, minor] => Some(format!("{major}.{minor}.0")),
        [major, minor, patch] => Some(format!("{major}.{minor}.{patch}")),
        _ => None,
    }
}

/// A version from a date (`2026-10-08` is `2026.10.8`), so newer downloads have newer versions
pub(crate) fn date_version(date: &str) -> Option<String> {
    let mut parts = date.split('-').map(|p| p.parse::<u32>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some(format!("{y}.{m}.{d}"))
}

/// Lowercase, without accents' combining marks, for matching
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

fn score(entry: &CatalogEntry, words: &[String]) -> Option<u32> {
    if words.is_empty() {
        return Some(0);
    }
    let exact = [fold(&entry.abbreviation), fold(&entry.id)];
    let names: Vec<String> = [&entry.name]
        .into_iter()
        .chain(&entry.aliases)
        .chain(&entry.language_name)
        .chain([&entry.language])
        .map(|s| fold(s))
        .collect();
    let description = entry.description.as_deref().map(fold).unwrap_or_default();
    let mut total = 0;
    for word in words {
        total += if exact.iter().any(|e| e == word) {
            100
        } else if exact.iter().any(|e| e.contains(word.as_str())) {
            40
        } else if names
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

/// A package id from eBible.org's (`eng-asv`, `engkjv2006`): lowercase letters, digits, `-`, `_`
pub(crate) fn package_id(id: &str) -> String {
    id.to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            _ => '-',
        })
        .collect()
}

/// What references could name an eBible.org translation by: its id without the language code
/// (`eng-asv` is `ASV`, `engwebp` is `WEBP`)
fn abbreviation(id: &str, language: &str) -> String {
    let rest = id
        .strip_prefix(language)
        .map(|r| r.trim_start_matches(['-', '_']))
        .filter(|r| !r.is_empty())
        .unwrap_or(id);
    rest.to_uppercase()
}

/// Rows of a CSV file: fields split by commas, quoted fields with `""` for a quote (and
/// commas or line breaks inside)
fn csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const EBIBLE: &str = "\u{feff}\"languageCode\",\"translationId\",\"languageName\",\"languageNameInEnglish\",\"dialect\",\"homeDomain\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"publicationURL\",\"OTbooks\",\"OTchapters\",\"OTverses\",\"NTbooks\",\"NTchapters\",\"NTverses\",\"DCbooks\",\"DCchapters\",\"DCverses\",\"FCBHID\",\"Certified\",\"inScript\",\"swordName\",\"rodCode\",\"textDirection\",\"downloadable\",\"font\",\"shortTitle\",\"PODISBN\",\"script\",\"sourceDate\"
\"eng\",\"eng-asv\",\"English\",\"English\",\"Archaic American\",\"ebible.org\",\"American Standard Version (1901)\",\"The American Standard Version of the Holy Bible, first published in 1901.\",\"True\",\"public domain\",\"2026-10-08\",\"http://ebible.org/eng-asv/\",\"39\",\"929\",\"23145\",\"27\",\"260\",\"7957\",\"0\",\"0\",\"0\",\"ENGASV\",\"True\",\"\",\"engASV1901eb\",\"00025\",\"ltr\",\"True\",\"Gentium\",\"American Standard Version (1901)\",\"\",\"Latin\",\"2026-10-08\"
\"eng\",\"engamp\",\"English\",\"English\",\"American\",\"eBible.org\",\"The Amplified Bible (2015)\",\"The Amplified Bible, 2015\",\"False\",\"Copyright © 2015 The Lockman Foundation\",\"2024-07-27\",\"\",\"39\",\"929\",\"23145\",\"27\",\"260\",\"7958\",\"0\",\"0\",\"0\",\"\",\"False\",\"\",\"\",\"\",\"ltr\",\"False\",\"\",\"Amplified\",\"\",\"Latin\",\"2024-07-27\"
\"heb\",\"hebwlc\",\"Hebrew\",\"Hebrew, Ancient\",\"\",\"ebible.org\",\"Westminster Leningrad Codex\",\"The \"\"WLC\"\", with vowels, and a comma, here\",\"True\",\"Copyright © 2020 J. Alan Groves Center\",\"2020-01-01\",\"\",\"39\",\"929\",\"23213\",\"0\",\"0\",\"0\",\"0\",\"0\",\"0\",\"\",\"True\",\"\",\"\",\"\",\"rtl\",\"True\",\"\",\"WLC\",\"\",\"Hebrew\",\"2020-01-01\"
";

    #[test]
    fn reads_ebibles_list() {
        let catalog = Catalog::parse(EBIBLE_CATALOG, EBIBLE).unwrap();
        // The Amplified can't be downloaded
        assert_eq!(catalog.entries.len(), 2);
        let asv = &catalog.entries[0];
        assert_eq!(
            (
                asv.id.as_str(),
                asv.abbreviation.as_str(),
                asv.language.as_str()
            ),
            ("eng-asv", "ASV", "eng")
        );
        assert_eq!(asv.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(
            asv.source,
            CatalogSource::Ebible {
                id: "eng-asv".into()
            }
        );
        assert_eq!(asv.version.as_deref(), Some("2026.10.8"));
        let wlc = &catalog.entries[1];
        assert_eq!(wlc.abbreviation, "WLC");
        assert_eq!(wlc.direction, Direction::Rtl);
        assert_eq!(wlc.license.as_deref(), Some("LicenseRef-eBible"));
        assert_eq!(
            wlc.description.as_deref(),
            Some("The \"WLC\", with vowels, and a comma, here")
        );
    }

    #[test]
    fn finds_imports_by_prefix() {
        let catalog = Catalog::parse(EBIBLE_CATALOG, EBIBLE).unwrap();
        let asv = catalog.find_import("ebible:eng-asv").unwrap();
        assert_eq!(asv.source.import_name().as_deref(), Some("ebible:eng-asv"));
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
    fn searches_and_merges() {
        let ebible = Catalog::parse("translations.csv", EBIBLE).unwrap();
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
        assert_eq!(ids("asv"), ["eng-asv"]);
        assert_eq!(ids("english"), ["eng-asv", "bsb"]);
        assert_eq!(ids("hebrew codex"), ["hebwlc"]);
        assert_eq!(ids("berean"), ["bsb"]);
        assert!(ids("nothing like this").is_empty());
        assert_eq!(ids("").len(), 3);
        assert!(Catalog::parse_toml("[[package]]\nid = 1").is_err());
    }

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
        let catalog = Catalog::parse(FETCH_BIBLE_CATALOG, json).unwrap();
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
            ("eng_bsb", "BSB", "eng")
        );
        assert_eq!(bsb.license.as_deref(), Some("LicenseRef-PublicDomain"));
        let CatalogSource::FetchBible { books, .. } = &bsb.source else {
            panic!()
        };
        assert_eq!((books.len(), books[42].as_str()), (66, "jhn"));
        let x = catalog.entries.iter().find(|e| e.id == "spa_x").unwrap();
        assert_eq!(x.license.as_deref(), Some("CC-BY-SA-3.0"));
        assert_eq!(x.aliases, ["Biblia"]);
        assert_eq!(x.language_name.as_deref(), Some("Spanish"));
        let y = catalog.entries.iter().find(|e| e.id == "heb_y").unwrap();
        assert_eq!(
            (y.license.as_deref(), y.direction),
            (Some("LicenseRef-fetch.bible"), Direction::Rtl)
        );
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

    #[test]
    fn reads_a_sword_repositorys_modules() {
        let conf = |name: &str, extra: &str| {
            format!(
                "[{name}]\nDataPath=./modules/texts/ztext/{name}/\nDescription={name} Bible\nLang=de\n{extra}\n"
            )
        };
        let files = [
            (
                "mods.d/kjv.conf",
                conf(
                    "KJV",
                    "ModDrv=zText\nSourceType=OSIS\nVersion=3.1\nDistributionLicense=GPL",
                ),
            ),
            (
                "mods.d/isv.conf",
                conf("ISV", "ModDrv=zText\nSourceType=ThML\nDirection=RtoL"),
            ),
            (
                "mods.d/mhc.conf",
                conf("MHC", "ModDrv=zCom\nSourceType=ThML"),
            ),
            // GBF (the default), locked, a dictionary, personal notes: left out
            ("mods.d/old.conf", conf("Old", "ModDrv=zText")),
            (
                "mods.d/nasb.conf",
                conf("NASB", "ModDrv=zText\nSourceType=OSIS\nCipherKey="),
            ),
            (
                "mods.d/easton.conf",
                conf("Easton", "ModDrv=RawLD\nSourceType=ThML"),
            ),
            (
                "mods.d/notes.conf",
                conf("Notes", "ModDrv=RawFiles\nSourceType=ThML"),
            ),
        ];
        let mut tar = tar::Builder::new(Vec::new());
        for (path, text) in &files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, text.as_bytes()).unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &tar.into_inner().unwrap()).unwrap();
        let bytes = gz.finish().unwrap();

        let catalog = Catalog::parse_bytes(SWORD_CATALOG, &bytes).unwrap();
        assert_eq!(catalog.name, "SWORD: crosswire");
        let ids: Vec<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["sword-isv", "sword-kjv", "sword-mhc"]);
        let kjv = catalog.find_import("sword:crosswire/KJV").unwrap();
        assert_eq!(
            (
                kjv.version.as_deref(),
                kjv.license.as_deref(),
                kjv.language.as_str()
            ),
            (Some("3.1.0"), Some("GPL-2.0-only"), "de")
        );
        let CatalogSource::Sword {
            repository, module, ..
        } = &kjv.source
        else {
            panic!()
        };
        assert_eq!(
            crate::import::sword_zip_urls(repository, module),
            [
                "https://crosswire.org/ftpmirror/pub/sword/raw/packages/KJV.zip",
                "https://crosswire.org/ftpmirror/pub/sword/raw/packages/kjv.zip"
            ]
        );
        assert_eq!(catalog.entries[0].direction, Direction::Rtl);
        assert_eq!(catalog.entries[2].kind, PackageKind::Commentary);

        // Another repository's modules are named by it
        let other =
            Catalog::parse_bytes("https://sword.example.org/repo/mods.d.tar.gz", &bytes).unwrap();
        assert_eq!(other.entries[1].id, "sword-sword-example-org-kjv");
        assert_eq!(
            other.entries[1].source.import_name().as_deref(),
            Some("sword:sword-example-org/KJV")
        );
        assert_eq!(
            sword_catalog_url("crosswire-beta").as_deref(),
            Some("https://crosswire.org/ftpmirror/pub/sword/betaraw/mods.d.tar.gz")
        );
        assert_eq!(
            sword_license(Some("Creative Commons: BY-NC-ND 3.0")),
            "CC-BY-NC-ND-3.0"
        );
        assert_eq!(
            sword_license(Some("Copyrighted; Free non-commercial distribution")),
            "LicenseRef-SWORD"
        );
    }
}
