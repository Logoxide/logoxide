/*!
`logoxide.toml`: a package's manifest, or a vault's (a manifest without `[package]`), like
`Cargo.toml`.

```toml
[package]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
version = "1.0.0"
language = "en"
license = "LicenseRef-PublicDomain"
keywords = ["modern english", "literal"]
repository = "https://github.com/someone/bsb"

[formats]
usx = { path = "usx/" }
sword = { path = "sword/", feature = "sword" }

[features]
default = ["usx"]
usx = []
sword = []
strongs = ["dep:strongs-greek"]

[dependencies]
strongs-greek = { git = "https://example.org/strongs-greek", version = "2", optional = true }

[settings]
default = "bsb"
```
*/

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::{LogoxideError, Result};

/// The manifest's file name, in a package and in a vault
pub const MANIFEST: &str = "logoxide.toml";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// What this package is; a vault's manifest has none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<Package>,
    /// Each format of the text, by name (`usx`, `usfm`, `sword`, ...)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub formats: BTreeMap<String, FormatEntry>,
    /// Each feature and what it turns on: other features, `dep:name` (an optional dependency), or
    /// `name/feature` (a dependency's feature)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub features: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, Dependency>,
    /// A vault's (or a machine's) choices: the default Bible, the ones to compare
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Settings>,
    /// Audio and video, listed by URL and hash, never stored in the package
    /// ([`crate::media`])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<crate::media::Media>,
    /// Files' titles, by path, in place of what the files say ([`crate::titles`])
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub titles: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Package {
    /// What dependencies call it: lowercase letters, digits, `-`, and `_`
    pub id: String,
    pub name: String,
    /// What references name it by (`John 3:16 BSB`)
    pub abbreviation: String,
    /// Who wrote it (a book's authors, a translation's translators), in order
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// A sentence or two about it (a book's subtitle)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub version: semver::Version,
    #[serde(default)]
    pub kind: PackageKind,
    /// BCP 47: `en`, `grc`
    pub language: String,
    #[serde(default, skip_serializing_if = "Direction::is_ltr")]
    pub direction: Direction,
    /// An SPDX expression (`CC0-1.0`, `LicenseRef-PublicDomain`); shown, never enforced
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Where its files are kept (its git repository's page)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Where it's documented, if not in its README
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// Its README's path in the package (part of the package, like `license-file`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,
    /// Words a search should find it by, besides its names (`justification`, `puritan`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// What it's about: up to [`MAX_CATEGORIES`] of [`CATEGORIES`] (`systematic-theology`,
    /// `church-history`); `keywords` say anything narrower
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// The books of the Bible it's mainly about, as their USFM codes (`["ROM", "GAL"]` for a
    /// commentary on Romans and Galatians). Said by its author, never worked out from its text;
    /// not the books a Bible has
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub about_books: Vec<String>,
    /// Where the text was imported from, if anywhere
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// `own` (the text's verse markers say), or a named system: `KJV`, `NRSV`, `MT`, ...
    #[serde(default = "own", skip_serializing_if = "is_own")]
    pub versification: String,
    /// For a versification map: the versification it maps `versification` to
    /// ([`crate::versification`])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maps_to: Option<String>,
    /// More files the package is made of, as patterns (`README.md`, `images/**`): besides
    /// these, only what the manifest names is (see [`crate::package::declared_files`])
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
}

fn own() -> String {
    String::from("own")
}

fn is_own(versification: &str) -> bool {
    versification == "own"
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageKind {
    #[default]
    Bible,
    Commentary,
    Lexicon,
    CrossReferences,
    VersificationMap,
    Book,
    /// Other packages, as its dependencies (a library: a feature can turn on each one), and no
    /// text of its own
    Collection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    fn is_ltr(&self) -> bool {
        *self == Direction::Ltr
    }
}

/// Where a format's files are, and the feature that turns it on (always on without one)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormatEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

/// The formats logoxide knows the names of
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Format {
    Usx,
    Usfm,
    Usj,
    Osis,
    Sword,
    Thml,
    Zefania,
    /// A book as an EPUB: kept and summarized as files, not read as verses (topos finds the
    /// references in it)
    Epub,
    /// A versification map's lines, as Paratext's `.vrs` files write them
    /// ([`crate::versification`])
    Vrs,
}

impl Format {
    pub fn from_name(name: &str) -> Option<Format> {
        Some(match name.to_ascii_lowercase().as_str() {
            "usx" => Format::Usx,
            "usfm" => Format::Usfm,
            "usj" => Format::Usj,
            "osis" => Format::Osis,
            "sword" => Format::Sword,
            "thml" => Format::Thml,
            "zefania" => Format::Zefania,
            "epub" => Format::Epub,
            "vrs" => Format::Vrs,
            _ => return None,
        })
    }

    /// The extensions of its files (`None`: every file under its path, for formats logoxide keeps
    /// without reading, like a SWORD module's folder)
    pub fn extensions(self) -> Option<&'static [&'static str]> {
        Some(match self {
            Format::Usx => &["usx"],
            Format::Usfm => &["usfm", "sfm"],
            Format::Usj => &["json"],
            Format::Osis => &["xml", "osis"],
            Format::Thml => &["xml", "thml"],
            Format::Zefania => &["xml"],
            Format::Epub => &["epub"],
            Format::Vrs => &["vrs"],
            Format::Sword => return None,
        })
    }

    /// Whether logoxide reads it yet
    pub fn readable(self) -> bool {
        matches!(
            self,
            Format::Usx | Format::Usfm | Format::Osis | Format::Thml
        )
    }
}

/// A dependency: a version requirement alone, or where it comes from
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
// A manifest has a few dependencies, so their size doesn't matter
#[allow(clippy::large_enum_variant)]
pub enum Dependency {
    /// `bsb = "1"`: from a registry (none exist yet)
    Version(semver::VersionReq),
    Detailed(DetailedDependency),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DetailedDependency {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// A registry's catalog (its URL, or a file's path) whose releases are archives: the newest
    /// release matching `version` is downloaded ([`crate::publish`])
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The newest tag matching this requirement (`v1.2.0` matches `1`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<semver::VersionReq>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The package's folder in its repository (`romans`), for a repository of many packages:
    /// its releases are tagged `romans/v1.0.0` (as `lx release` tags them), and only that
    /// folder's files are the package
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Only used when a feature turns it on (`dep:name`)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default = "yes", skip_serializing_if = "Clone::clone")]
    pub default_features: bool,
    /// The package's id, when the dependency is named something else (`kjv-bob = { package =
    /// "kjv", … }`), so two forks of a package can be used side by side
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// What references name it by here, in place of the package's own abbreviation (`KJVB`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abbreviation: Option<String>,
    /// Its aliases here, in place of the package's own
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// `false` turns it off without removing it: it stays locked and installed (so `lx clean`
    /// keeps it, and turning it on again needs no download), but nothing reads it
    #[serde(default = "yes", skip_serializing_if = "Clone::clone")]
    pub enabled: bool,
}

/// As a manifest without the fields reads: default features and the dependency on
impl Default for DetailedDependency {
    fn default() -> Self {
        Self {
            git: None,
            registry: None,
            path: None,
            version: None,
            tag: None,
            rev: None,
            branch: None,
            folder: None,
            optional: false,
            features: vec![],
            default_features: true,
            package: None,
            abbreviation: None,
            aliases: None,
            enabled: true,
        }
    }
}

impl Dependency {
    /// Whether it's on (`enabled = false` turns it off without removing it)
    pub fn enabled(&self) -> bool {
        match self {
            Dependency::Version(_) => true,
            Dependency::Detailed(d) => d.enabled,
        }
    }
}

impl DetailedDependency {
    /// The id of the package it names: `package`, else the dependency's own name
    pub fn package_id<'a>(&'a self, name: &'a str) -> &'a str {
        self.package.as_deref().unwrap_or(name)
    }
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Settings {
    /// The Bible references use when they name none
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Bibles shown after the named one
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compare: Vec<String>,
    /// Catalogs (or registries) to search for packages, besides eBible.org's and fetch.bible's
    /// lists: URLs or paths (see [`crate::catalog`]). Their entries come first
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogs: Vec<String>,
    /// Whether eBible.org's and fetch.bible's lists are searched too
    #[serde(default = "yes", skip_serializing_if = "Clone::clone")]
    pub default_catalogs: bool,
    /// Registries by name: `dorean-local = "~/logoxide-packages"` lets `dorean-local:kjv` (or
    /// `dorean-local:books/kjv`, its folder there) name a package. Each is a folder of package
    /// repositories, or a catalog (see [`crate::catalog`]); they're searched first
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub registries: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default: None,
            compare: Vec::new(),
            catalogs: Vec::new(),
            default_catalogs: true,
            registries: BTreeMap::new(),
        }
    }
}

impl Manifest {
    /// The registries by name (`[settings.registries]`)
    pub fn registries(&self) -> BTreeMap<String, String> {
        (self.settings.as_ref())
            .map(|s| s.registries.clone())
            .unwrap_or_default()
    }

    /// The catalogs to search: `[settings] catalogs`, then eBible.org's and fetch.bible's lists
    /// (unless `default-catalogs = false`)
    pub fn catalogs(&self) -> Vec<String> {
        self.catalogs_with(&[])
    }

    /// [`Manifest::catalogs`] with `more` (the machine's, from logoxide's config) after the
    /// manifest's own and before the default ones
    pub fn catalogs_with(&self, more: &[String]) -> Vec<String> {
        let settings = self.settings.clone().unwrap_or_default();
        let mut catalogs = settings.catalogs;
        for catalog in more {
            if !catalogs.contains(catalog) {
                catalogs.push(catalog.clone());
            }
        }
        if settings.default_catalogs {
            for default in crate::catalog::DEFAULT_CATALOGS {
                if !catalogs.iter().any(|c| c == default) {
                    catalogs.push(default.to_string());
                }
            }
        }
        catalogs
    }
}

/**
The categories a package can be in, and what each is for: subjects (what kind of package it is,
a commentary or a lexicon, is its `kind`). Traditions, eras, and narrower topics are keywords
*/
pub const CATEGORIES: [(&str, &str); 21] = [
    // The Bible
    (
        "biblical-theology",
        "Themes traced through Scripture (covenant, kingdom)",
    ),
    (
        "introduction",
        "Old and New Testament introductions and surveys, authorship, canon",
    ),
    (
        "background",
        "The Bible's history, archaeology, geography, and culture",
    ),
    ("hermeneutics", "How to interpret Scripture"),
    (
        "biblical-languages",
        "Greek, Hebrew, and Aramaic grammars and readers",
    ),
    // Theology
    ("systematic-theology", "Doctrine by topic"),
    (
        "creeds-and-confessions",
        "Creeds, confessions, and catechisms",
    ),
    ("apologetics", "Defending the faith"),
    ("ethics", "Christian ethics and moral theology"),
    // History
    ("church-history", "The church through the centuries"),
    ("biography", "Lives, memoirs, and letters"),
    // The church and its ministry
    ("preaching", "Homiletics: how to preach"),
    ("sermons", "Collected sermons"),
    (
        "pastoral",
        "Pastoral ministry, counseling, and church leadership",
    ),
    ("worship", "Liturgy, hymns, and prayer books"),
    ("missions", "Missions and evangelism"),
    // The Christian life
    ("spiritual-life", "Discipleship, prayer, and holiness"),
    ("devotional", "Daily readings and meditations"),
    ("family", "Marriage, parenting, and the home"),
    ("children", "Written for children"),
    // Reference
    (
        "reference",
        "Dictionaries, encyclopedias, atlases, and concordances",
    ),
];

/// How many categories a package can be in, so each one means something
pub const MAX_CATEGORIES: usize = 5;

/// Checks `keywords`, `categories`, and `about-books`
fn check_metadata(package: &Package) -> Result<()> {
    let err = |message: String| Err(LogoxideError::Manifest(message));
    let mut seen = BTreeSet::new();
    for keyword in &package.keywords {
        if keyword.trim().is_empty() || keyword.trim() != keyword {
            return err(format!(
                "keyword `{keyword}`: give words, without spaces around them"
            ));
        }
        if !seen.insert(keyword.to_lowercase()) {
            return err(format!("keyword `{keyword}` is listed twice"));
        }
    }
    if package.categories.len() > MAX_CATEGORIES {
        return err(format!(
            "{} categories: a package can be in at most {MAX_CATEGORIES} (keywords say more)",
            package.categories.len()
        ));
    }
    let mut seen = BTreeSet::new();
    for category in &package.categories {
        if !CATEGORIES.iter().any(|(c, _)| c == category) {
            let all: Vec<&str> = CATEGORIES.iter().map(|(c, _)| *c).collect();
            return err(format!(
                "category `{category}` is none of logoxide's ({}); a keyword can say it",
                all.join(", ")
            ));
        }
        if !seen.insert(category) {
            return err(format!("category `{category}` is listed twice"));
        }
    }
    let mut seen = BTreeSet::new();
    for book in &package.about_books {
        if crate::bible_data().books().search_usfm(book).is_none()
            || book.to_ascii_uppercase() != *book
        {
            return err(match book_code(book) {
                Some(code) => format!("about-books: `{book}` is written `{code}`"),
                None => format!("about-books: `{book}` is no book's USFM code (`ROM`, `1CO`)"),
            });
        }
        if !seen.insert(book) {
            return err(format!("about-books: `{book}` is listed twice"));
        }
    }
    Ok(())
}

/// A book's USFM code from its code in any case or its name (`rom`, `Romans`, `1 Cor` are `ROM`,
/// `ROM`, `1CO`)
pub fn book_code(name: &str) -> Option<String> {
    let books = crate::bible_data().books();
    let id = (books.search_usfm(&name.trim().to_ascii_uppercase()))
        .or_else(|| books.search(name.trim()))?;
    books.get_usfm(id).cloned()
}

/// What a feature selection turns on
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Enabled {
    pub features: BTreeSet<String>,
    /// Optional dependencies turned on (`dep:name`)
    pub dependencies: BTreeSet<String>,
    /// Features of dependencies (`name/feature`)
    pub dependency_features: BTreeMap<String, BTreeSet<String>>,
}

impl Manifest {
    /// Reads and checks a manifest
    pub fn parse(text: &str) -> Result<Manifest> {
        let manifest: Manifest =
            toml::from_str(text).map_err(|e| LogoxideError::Manifest(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| LogoxideError::Manifest(e.to_string()))
    }

    /// Checks what the types can't: ids, feature names, and where dependencies come from
    pub fn validate(&self) -> Result<()> {
        let err = |message: String| Err(LogoxideError::Manifest(message));
        if let Some(package) = &self.package {
            if !is_valid_id(&package.id) {
                return err(format!(
                    "package id `{}`: use lowercase letters, digits, `-`, and `_`",
                    package.id
                ));
            }
            check_metadata(package)?;
            // A collection is only its dependencies
            match (
                package.kind == PackageKind::Collection,
                self.formats.is_empty(),
            ) {
                (false, true) => return err(String::from("a package needs [formats]")),
                (true, false) => {
                    return err(String::from(
                        "a collection has no [formats]: its packages are its dependencies",
                    ));
                }
                _ => {}
            }
            // A map names both versifications
            match (package.kind, &package.maps_to) {
                (PackageKind::VersificationMap, None) => {
                    return err(String::from(
                        "a versification map names what it maps to: maps-to = \"Original\"",
                    ));
                }
                (PackageKind::VersificationMap, Some(_)) if is_own(&package.versification) => {
                    return err(String::from(
                        "a versification map names what it maps from: versification = \"English\"",
                    ));
                }
                (PackageKind::VersificationMap, _) | (_, None) => {}
                (_, Some(_)) => {
                    return err(String::from(
                        "only a versification map has maps-to (kind = \"versification-map\")",
                    ));
                }
            }
        }
        for (name, format) in &self.formats {
            if let Some(feature) = &format.feature
                && !self.features.contains_key(feature)
            {
                return err(format!(
                    "format `{name}` needs feature `{feature}`, which [features] doesn't have"
                ));
            }
        }
        for (feature, turns_on) in &self.features {
            for item in turns_on {
                if let Some(dep) = item.strip_prefix("dep:") {
                    match self.dependencies.get(dep) {
                        Some(Dependency::Detailed(d)) if d.optional => {}
                        _ => {
                            return err(format!(
                                "feature `{feature}`: `{item}` must name an optional dependency"
                            ));
                        }
                    }
                } else if let Some((dep, _)) = item.split_once('/') {
                    if !self.dependencies.contains_key(dep) {
                        return err(format!("feature `{feature}`: `{item}` names no dependency"));
                    }
                } else if !self.features.contains_key(item) {
                    return err(format!("feature `{feature}`: `{item}` is no feature"));
                }
            }
        }
        for (name, dependency) in &self.dependencies {
            let Dependency::Detailed(d) = dependency else {
                continue;
            };
            let sources = [d.git.is_some(), d.path.is_some(), d.registry.is_some()];
            if sources.iter().filter(|s| **s).count() != 1 {
                return err(format!(
                    "dependency `{name}`: give one of `git`, `registry`, or `path`"
                ));
            }
            let pins = [&d.tag, &d.rev, &d.branch]
                .iter()
                .filter(|p| p.is_some())
                .count();
            if pins > 1 || (pins == 1 && d.git.is_none()) {
                return err(format!(
                    "dependency `{name}`: at most one of `tag`, `rev`, and `branch`, with `git`"
                ));
            }
            check_dependency(name, d)?;
        }
        if let Some(media) = &self.media {
            for file in &media.files {
                crate::media::check_sha256(&file.sha256)?;
            }
        }
        Ok(())
    }

    /**
    The keys in a manifest's text that logoxide doesn't know, as dotted paths
    (`package.funding`, `dependencies.kjv.mirror`), so a caller can warn of them as Cargo does.
    They're no error: a newer logoxide may write keys an older one doesn't read
    */
    pub fn unknown_keys(text: &str) -> Result<Vec<String>> {
        let table: toml::Table =
            toml::from_str(text).map_err(|e| LogoxideError::Manifest(e.to_string()))?;
        let mut unknown = vec![];
        let check = |at: &str, value: &toml::Value, fields: &[&str], unknown: &mut Vec<String>| {
            if let Some(table) = value.as_table() {
                for key in table.keys() {
                    if !fields.contains(&key.as_str()) {
                        unknown.push(format!("{at}.{key}"));
                    }
                }
            }
        };
        for (key, value) in &table {
            match key.as_str() {
                "package" => check(key, value, fields_of::<Package>(), &mut unknown),
                "formats" | "dependencies" => {
                    let fields = match key.as_str() {
                        "formats" => fields_of::<FormatEntry>(),
                        _ => fields_of::<DetailedDependency>(),
                    };
                    for (name, entry) in value.as_table().into_iter().flatten() {
                        check(&format!("{key}.{name}"), entry, fields, &mut unknown);
                    }
                }
                "settings" => check(key, value, fields_of::<Settings>(), &mut unknown),
                "media" => {
                    check(key, value, fields_of::<crate::media::Media>(), &mut unknown);
                    let files = value.get("files").and_then(toml::Value::as_array);
                    for file in files.into_iter().flatten() {
                        let fields = fields_of::<crate::media::MediaFile>();
                        check("media.files", file, fields, &mut unknown);
                    }
                }
                _ if fields_of::<Manifest>().contains(&key.as_str()) => {}
                _ => unknown.push(key.clone()),
            }
        }
        unknown.sort();
        unknown.dedup();
        Ok(unknown)
    }
}

/// Whether `id` can be a package's id (or a dependency's name): lowercase letters, digits, `-`,
/// and `_`. Ids name folders in the library, so nothing else is allowed
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && (id.bytes())
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// [`is_valid_id`] as an error naming what the id is of
pub fn check_id(what: &str, id: &str) -> Result<()> {
    match is_valid_id(id) {
        true => Ok(()),
        false => Err(LogoxideError::Manifest(format!(
            "{what} `{id}`: use lowercase letters, digits, `-`, and `_`"
        ))),
    }
}

/**
Checks a dependency's `rev`: a commit's full object id (40 hex digits, or 64 for SHA-256
repositories). A shorter prefix could come to name another commit as the repository grows, and
only a whole one can be fetched by itself from a host
*/
pub fn check_rev(rev: &str) -> Result<()> {
    if matches!(rev.len(), 40 | 64) && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(LogoxideError::Manifest(format!(
        "rev `{rev}` isn't a full commit id (40 or 64 hex digits: `git rev-parse {rev}` gives \
         it); name a branch with `branch`, a tag with `tag`"
    )))
}

/**
Checks what a dependency gives git and the library: its name and `package` are ids, and nothing
it hands git could be read as an option (`--upload-pack=…`), so a manifest from anywhere can't run
commands. A revision is a commit's hex; a folder is a plain relative path
*/
fn check_dependency(name: &str, d: &DetailedDependency) -> Result<()> {
    let err = |message: String| Err(LogoxideError::Manifest(message));
    check_id("dependency", name)?;
    if let Some(package) = &d.package {
        check_id(&format!("dependency `{name}`: package"), package)?;
    }
    for (key, value) in [
        ("git", &d.git),
        ("registry", &d.registry),
        ("tag", &d.tag),
        ("branch", &d.branch),
        ("rev", &d.rev),
    ] {
        if let Some(value) = value
            && (value.starts_with('-') || value.trim().is_empty() || value.contains('\0'))
        {
            return err(format!(
                "dependency `{name}`: `{key} = \"{value}\"` isn't one"
            ));
        }
    }
    if let Some(rev) = &d.rev {
        check_rev(rev).map_err(|e| LogoxideError::Manifest(format!("dependency `{name}`: {e}")))?;
    }
    if let Some(folder) = &d.folder {
        let plain = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.starts_with('-')
                && !part.contains([':', '\\', '\0', '*', '?', '[', '~', '^'])
        };
        if !folder.trim_matches('/').split('/').all(plain) {
            return err(format!(
                "dependency `{name}`: folder `{folder}` isn't a folder in the repository"
            ));
        }
        if d.git.is_none() {
            return err(format!(
                "dependency `{name}`: `folder` names a folder of a `git` repository"
            ));
        }
    }
    Ok(())
}

/// The field names serde reads for a struct: its `deserialize_struct` hands them to the
/// deserializer, which this one keeps (and fails, having nothing to give)
fn fields_of<T: serde::de::DeserializeOwned>() -> &'static [&'static str] {
    use serde::de::Visitor;
    #[derive(Debug)]
    struct Fields(&'static [&'static str]);
    impl std::fmt::Display for Fields {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "fields")
        }
    }
    impl std::error::Error for Fields {}
    impl serde::de::Error for Fields {
        fn custom<M: std::fmt::Display>(_: M) -> Self {
            Fields(&[])
        }
    }
    struct Capture;
    impl<'de> serde::Deserializer<'de> for Capture {
        type Error = Fields;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> std::result::Result<V::Value, Fields> {
            Err(Fields(&[]))
        }
        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> std::result::Result<V::Value, Fields> {
            Err(Fields(fields))
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
            option unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier
            ignored_any
        }
    }
    match T::deserialize(Capture) {
        Err(Fields(fields)) => fields,
        Ok(_) => &[],
    }
}

impl Manifest {
    /// What `features` (plus `default`, unless left out) turn on, following each feature
    pub fn enable(&self, features: &[&str], default: bool) -> Result<Enabled> {
        let mut enabled = Enabled::default();
        let mut todo: Vec<String> = features.iter().map(|f| f.to_string()).collect();
        if default && self.features.contains_key("default") {
            todo.push(String::from("default"));
        }
        while let Some(feature) = todo.pop() {
            if !enabled.features.insert(feature.clone()) {
                continue;
            }
            let Some(turns_on) = self.features.get(&feature) else {
                return Err(LogoxideError::Manifest(format!("no feature `{feature}`")));
            };
            for item in turns_on {
                if let Some(dep) = item.strip_prefix("dep:") {
                    enabled.dependencies.insert(dep.to_string());
                } else if let Some((dep, f)) = item.split_once('/') {
                    enabled.dependencies.insert(dep.to_string());
                    (enabled
                        .dependency_features
                        .entry(dep.to_string())
                        .or_default())
                    .insert(f.to_string());
                } else {
                    todo.push(item.clone());
                }
            }
        }
        enabled.features.remove("default");
        Ok(enabled)
    }

    /// The formats the enabled features turn on (and those without a feature), by name
    pub fn formats(&self, enabled: &Enabled) -> Vec<(&str, &FormatEntry)> {
        self.formats
            .iter()
            .filter(|(_, f)| {
                f.feature
                    .as_ref()
                    .is_none_or(|f| enabled.features.contains(f))
            })
            .map(|(name, f)| (name.as_str(), f))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A commentary's manifest with these lines added to its `[package]`
    fn commentary(lines: &str) -> Result<Manifest> {
        Manifest::parse(&format!(
            "[package]\nid = \"gill\"\nname = \"Gill on Romans\"\nabbreviation = \"GILL\"\n\
             version = \"1.0.0\"\nlanguage = \"en\"\nkind = \"commentary\"\n{lines}\n\
             [formats]\nusx = {{ path = \"usx/\" }}\n"
        ))
    }

    #[test]
    fn metadata_like_cargos() {
        let manifest = commentary(
            r#"keywords = ["justification", "Puritan"]
categories = ["church-history", "biography"]
about-books = ["ROM", "1CO"]
repository = "https://github.com/someone/gill"
documentation = "https://example.org/gill"
readme = "README.md""#,
        )
        .unwrap();
        let info = manifest.package.as_ref().unwrap();
        assert_eq!(info.about_books, ["ROM", "1CO"]);
        assert_eq!(info.categories, ["church-history", "biography"]);
        assert_eq!(info.readme.as_deref(), Some("README.md"));
        // Written back the same way
        assert_eq!(
            Manifest::parse(&manifest.to_toml().unwrap()).unwrap(),
            manifest
        );
        assert!(manifest.to_toml().unwrap().contains("about-books = ["));

        let error = |lines: &str| commentary(lines).unwrap_err().to_string();
        assert!(error(r#"about-books = ["Romans"]"#).contains("`Romans` is written `ROM`"));
        assert!(error(r#"about-books = ["rom"]"#).contains("`rom` is written `ROM`"));
        assert!(error(r#"about-books = ["XYZ"]"#).contains("no book's USFM code"));
        assert!(error(r#"about-books = ["ROM", "ROM"]"#).contains("listed twice"));
        assert!(error(r#"categories = ["reformed"]"#).contains("none of logoxide's"));
        assert!(error(r#"categories = ["Church History"]"#).contains("none of logoxide's"));
        assert!(error(r#"categories = ["ethics", "ethics"]"#).contains("listed twice"));
        let six =
            r#"categories = ["ethics", "sermons", "worship", "missions", "family", "children"]"#;
        assert!(error(six).contains("at most 5"));
        assert!(error(r#"keywords = [" grace"]"#).contains("without spaces"));
        assert!(error(r#"keywords = ["grace", "Grace"]"#).contains("listed twice"));
    }

    #[test]
    fn book_codes_from_names() {
        assert_eq!(book_code("Romans").as_deref(), Some("ROM"));
        assert_eq!(book_code("rom").as_deref(), Some("ROM"));
        assert_eq!(book_code("1 Corinthians").as_deref(), Some("1CO"));
        assert_eq!(book_code("Song of Songs").as_deref(), Some("SNG"));
        assert_eq!(book_code("nothing"), None);
    }

    const BSB: &str = r#"
[package]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
aliases = ["Berean"]
version = "1.2.0"
language = "en"
license = "LicenseRef-PublicDomain"

[formats]
usx = { path = "usx/", feature = "usx" }
sword = { path = "sword/", feature = "sword" }

[features]
default = ["usx", "headings"]
usx = []
sword = []
headings = []
strongs = ["dep:strongs-greek", "tsk/full"]

[dependencies]
strongs-greek = { git = "https://example.org/strongs", version = "2", optional = true }
tsk = { git = "https://example.org/tsk", tag = "v1.0.0" }
"#;

    #[test]
    fn reads_a_package() {
        let m = Manifest::parse(BSB).unwrap();
        let p = m.package.as_ref().unwrap();
        assert_eq!(
            (p.id.as_str(), p.version.to_string()),
            ("bsb", "1.2.0".into())
        );
        assert_eq!(p.kind, PackageKind::Bible);
        assert_eq!(p.versification, "own");
        // Round trip
        assert_eq!(Manifest::parse(&m.to_toml().unwrap()).unwrap(), m);
    }

    #[test]
    fn features_turn_on_formats_and_dependencies() {
        let m = Manifest::parse(BSB).unwrap();
        let names = |e: &Enabled| {
            m.formats(e)
                .iter()
                .map(|(n, _)| n.to_string())
                .collect::<Vec<_>>()
        };
        let default = m.enable(&[], true).unwrap();
        assert_eq!(
            default.features,
            BTreeSet::from(["usx".into(), "headings".into()])
        );
        assert_eq!(names(&default), ["usx"]);
        let more = m.enable(&["sword", "strongs"], false).unwrap();
        assert_eq!(names(&more), ["sword"]);
        assert_eq!(
            more.dependencies,
            BTreeSet::from(["strongs-greek".into(), "tsk".into()])
        );
        assert_eq!(
            more.dependency_features["tsk"],
            BTreeSet::from(["full".into()])
        );
        assert!(m.enable(&["nope"], false).is_err());
    }

    #[test]
    fn a_vault_has_no_package() {
        let m = Manifest::parse(
            r#"
[dependencies]
bsb = { git = "https://example.org/bsb", version = "1" }
mine = { path = "Bibles/Mine" }
later = "2"

[settings]
default = "bsb"
compare = ["mine"]
"#,
        )
        .unwrap();
        assert!(m.package.is_none());
        assert!(matches!(m.dependencies["later"], Dependency::Version(_)));
        assert_eq!(m.settings.unwrap().default.as_deref(), Some("bsb"));
    }

    #[test]
    fn a_vaults_catalogs_come_before_the_defaults() {
        use crate::catalog::{
            CCEL_CATALOG, DEFAULT_CATALOGS, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG, SWORD_CATALOG,
        };
        let catalogs = |settings: &str| {
            Manifest::parse(&format!("[dependencies]\n[settings]\n{settings}"))
                .unwrap()
                .catalogs()
        };
        assert_eq!(Manifest::default().catalogs(), DEFAULT_CATALOGS);
        assert_eq!(
            catalogs("catalogs = [\"mine.toml\"]"),
            [
                "mine.toml",
                EBIBLE_CATALOG,
                FETCH_BIBLE_CATALOG,
                CCEL_CATALOG,
                SWORD_CATALOG
            ]
        );
        // Named again: listed once, where the vault puts it
        let again = format!("catalogs = [\"{FETCH_BIBLE_CATALOG}\", \"mine.toml\"]");
        assert_eq!(
            catalogs(&again),
            [
                FETCH_BIBLE_CATALOG,
                "mine.toml",
                EBIBLE_CATALOG,
                CCEL_CATALOG,
                SWORD_CATALOG
            ]
        );
        assert_eq!(
            catalogs("catalogs = [\"mine.toml\"]\ndefault-catalogs = false"),
            ["mine.toml"]
        );
    }

    #[test]
    fn mistakes_are_explained() {
        let bad = |extra: &str| {
            Manifest::parse(&format!("{BSB}\n{extra}"))
                .unwrap_err()
                .to_string()
        };
        assert!(
            bad("x = { path = \"a\", git = \"b\" }")
                .contains("give one of `git`, `registry`, or `path`")
        );
        assert!(bad("y = { path = \"a\", tag = \"v1\" }").contains("with `git`"));
        let wrong_dep = BSB.replace("dep:strongs-greek", "dep:tsk");
        assert!(
            Manifest::parse(&wrong_dep)
                .unwrap_err()
                .to_string()
                .contains("optional dependency")
        );
        let typo = BSB.replace("abbreviation", "abbrev");
        assert!(Manifest::parse(&typo).is_err());
        // Nothing that git reads as an option, and revisions are commits
        for (dependency, message) in [
            ("x = { git = \"--upload-pack=touch /tmp/x\" }", "isn't one"),
            (
                "x = { git = \"https://e.org/x\", tag = \"-x\" }",
                "isn't one",
            ),
            (
                "x = { git = \"https://e.org/x\", branch = \"--x\" }",
                "isn't one",
            ),
            (
                "x = { git = \"https://e.org/x\", rev = \"--x\" }",
                "isn't one",
            ),
            (
                "x = { git = \"https://e.org/x\", rev = \"main\" }",
                "isn't a full commit id",
            ),
            (
                "x = { git = \"https://e.org/x\", rev = \"abc\" }",
                "isn't a full commit id",
            ),
            (
                "x = { git = \"https://e.org/x\", rev = \"3f9c2a1b\" }",
                "isn't a full commit id",
            ),
            ("x = { registry = \"-r\" }", "isn't one"),
            ("\"../x\" = { git = \"https://e.org/x\" }", "lowercase"),
            (
                "x = { git = \"https://e.org/x\", package = \"../x\" }",
                "lowercase",
            ),
            (
                "x = { git = \"https://e.org/x\", folder = \"../x\" }",
                "isn't a folder",
            ),
            (
                "x = { path = \"x\", folder = \"x\" }",
                "of a `git` repository",
            ),
        ] {
            let error = bad(dependency);
            assert!(error.contains(message), "{dependency}: {error}");
        }
        let fine = format!(
            "{BSB}\nx = {{ git = \"https://e.org/x\", rev = \"{}\" }}\n",
            "3f".repeat(20)
        );
        assert!(Manifest::parse(&fine).is_ok());
        let sha256 = format!(
            "{BSB}\nx = {{ git = \"https://e.org/x\", rev = \"{}\" }}\n",
            "3f".repeat(32)
        );
        assert!(Manifest::parse(&sha256).is_ok());
        let folder = format!("{BSB}\nx = {{ git = \"https://e.org/x\", folder = \"books/x\" }}\n");
        let folder = Manifest::parse(&folder).unwrap();
        let Dependency::Detailed(x) = &folder.dependencies["x"] else {
            panic!("detailed")
        };
        assert_eq!(x.folder.as_deref(), Some("books/x"));
    }

    #[test]
    fn unknown_keys_are_named_not_refused() {
        // A newer lx's keys: read, and named so a caller can warn
        let newer = BSB
            .replace(
                "license = \"LicenseRef-PublicDomain\"",
                "license = \"LicenseRef-PublicDomain\"\nfunding = \"https://e.org\"",
            )
            .replace(
                "tag = \"v1.0.0\" }",
                "tag = \"v1.0.0\", mirror = \"https://m.org\" }",
            )
            .replace(
                "usx = { path = \"usx/\", feature = \"usx\" }",
                "usx = { path = \"usx/\", feature = \"usx\", encoding = \"utf-8\" }",
            )
            + "\n[settings]\ndefault = \"bsb\"\ntheme = \"dark\"\n\n[media]\nplayer = \"x\"\n\n[[media.files]]\npath = \"a.mp3\"\nurl = \"u\"\nsize = 0\nsha256 = \"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\"\nlength = 3\n\n[plugins]\nx = 1\n";
        let manifest = Manifest::parse(&newer).unwrap();
        assert_eq!(manifest.package.unwrap().id, "bsb");
        assert_eq!(
            Manifest::unknown_keys(&newer).unwrap(),
            [
                "dependencies.tsk.mirror",
                "formats.usx.encoding",
                "media.files.length",
                "media.player",
                "package.funding",
                "plugins",
                "settings.theme"
            ]
        );
        assert!(Manifest::unknown_keys(BSB).unwrap().is_empty());
        // Values still have to make sense
        assert!(Manifest::parse(&BSB.replace("version = \"1.2.0\"", "version = 3")).is_err());
        let id = BSB.replace("id = \"bsb\"", "id = \"BSB 2\"");
        assert!(
            Manifest::parse(&id)
                .unwrap_err()
                .to_string()
                .contains("lowercase")
        );
    }
}
