/*!
`biblix.toml`: a package's manifest, or a vault's (a manifest without `[package]`), like
`Cargo.toml`.

```toml
[package]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
version = "1.0.0"
language = "en"
license = "LicenseRef-PublicDomain"

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

use crate::error::{BiblixError, Result};

/// The manifest's file name, in a package and in a vault
pub const MANIFEST: &str = "biblix.toml";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
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
    /// Where the text was imported from, if anywhere
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// `own` (the text's verse markers say), or a named system: `KJV`, `NRSV`, `MT`, ...
    #[serde(default = "own", skip_serializing_if = "is_own")]
    pub versification: String,
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
#[serde(deny_unknown_fields)]
pub struct FormatEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

/// The formats biblix knows the names of
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
            _ => return None,
        })
    }

    /// The extensions of its files (`None`: every file under its path, for formats biblix keeps
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
            Format::Sword => return None,
        })
    }

    /// Whether biblix reads it yet
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DetailedDependency {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
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
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
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
    /// Registries by name: `dorean-local = "~/biblix-packages"` lets `dorean-local:kjv` (or
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
        let settings = self.settings.clone().unwrap_or_default();
        let mut catalogs = settings.catalogs;
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
            toml::from_str(text).map_err(|e| BiblixError::Manifest(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| BiblixError::Manifest(e.to_string()))
    }

    /// Checks what the types can't: ids, feature names, and where dependencies come from
    pub fn validate(&self) -> Result<()> {
        let err = |message: String| Err(BiblixError::Manifest(message));
        if let Some(package) = &self.package {
            let ok = !package.id.is_empty()
                && package.id.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                });
            if !ok {
                return err(format!(
                    "package id `{}`: use lowercase letters, digits, `-`, and `_`",
                    package.id
                ));
            }
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
            if d.git.is_some() == d.path.is_some() {
                return err(format!("dependency `{name}`: give `git` or `path`"));
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
        }
        Ok(())
    }

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
                return Err(BiblixError::Manifest(format!("no feature `{feature}`")));
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
        use crate::catalog::{DEFAULT_CATALOGS, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG};
        let catalogs = |settings: &str| {
            Manifest::parse(&format!("[dependencies]\n[settings]\n{settings}"))
                .unwrap()
                .catalogs()
        };
        assert_eq!(Manifest::default().catalogs(), DEFAULT_CATALOGS);
        assert_eq!(
            catalogs("catalogs = [\"mine.toml\"]"),
            ["mine.toml", EBIBLE_CATALOG, FETCH_BIBLE_CATALOG]
        );
        // Named again: listed once, where the vault puts it
        let again = format!("catalogs = [\"{FETCH_BIBLE_CATALOG}\", \"mine.toml\"]");
        assert_eq!(
            catalogs(&again),
            [FETCH_BIBLE_CATALOG, "mine.toml", EBIBLE_CATALOG]
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
        assert!(bad("x = { path = \"a\", git = \"b\" }").contains("give `git` or `path`"));
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
        let id = BSB.replace("id = \"bsb\"", "id = \"BSB 2\"");
        assert!(
            Manifest::parse(&id)
                .unwrap_err()
                .to_string()
                .contains("lowercase")
        );
    }
}
