//! A vault's (or any project's) `logoxide.toml` and `logoxide.lock`: the packages it asks for, where
//! each one's files are, and adding or removing them (keeping the manifest's comments).

use boltffi::*;
use logoxide_core::{LockedPackage, Project, library::library_dir, manifest::DetailedDependency};

use crate::{LogoxideError, PackageFeatures, PackageKind};

/// Where a package's files are
#[data]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PackageLocation {
    /// In the library, under its folder: `packages/<id>/<version>+g<commit>`
    Library { dir: String },
    /// A path package, used where it is: relative to the project's folder
    Path { dir: String },
}

impl PackageLocation {
    pub(crate) fn of(locked: &LockedPackage) -> Self {
        match &locked.path {
            Some(dir) => PackageLocation::Path { dir: dir.clone() },
            None => PackageLocation::Library {
                dir: library_dir(locked),
            },
        }
    }
}

/// A Bible the host shows, by what references name it: for `LogoxideProject.nameClashes`
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedBible {
    /// How it's shown (`kjv 1.0.0`, or the entry's name)
    pub label: String,
    /// Its abbreviation here (`LockedEntry.abbreviation`, else the package's own)
    pub abbreviation: String,
    /// Its aliases here (`LockedEntry.aliases`, else the package's own)
    pub aliases: Vec<String>,
}

/// Two or more Bibles answering to one name
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameClash {
    /// The name, as the first of them spells it
    pub name: String,
    /// The label of each Bible answering to it
    pub packages: Vec<String>,
}

/// A registry a project names
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    pub name: String,
    pub location: String,
}

/// One package the project asks for, exactly as its lock file has it
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedEntry {
    pub id: String,
    /// What the manifest calls it: its id, or another name (a second fork of a package)
    pub name: String,
    /// What references name it by here, when the manifest gives its own (else the package's
    /// abbreviation: `LogoxidePackage.addTo`; with these, `addToAs`)
    pub abbreviation: Option<String>,
    /// Its aliases here, when the manifest gives its own
    pub aliases: Option<Vec<String>>,
    pub version: String,
    pub location: PackageLocation,
    /// The features to open it with
    pub features: PackageFeatures,
    /// What every file hashes to (`LogoxidePackage.checksum()`); none for path packages, whose
    /// files change as they're written
    pub checksum: Option<String>,
    /// The repository it came from
    pub git: Option<String>,
    /// The commit its files are from
    pub commit: Option<String>,
    /// The package the manifest asks for that brings it in (what the manifest calls that one),
    /// when the manifest doesn't ask for it itself. A collection's packages are the project's
    /// Bibles and books; another package's dependencies aren't
    pub dependency_of: Option<String>,
    /// Whether it's in use: `false` when the manifest turned it off (`enabled = false`), or
    /// turned off what brings it in. It stays installed; hosts don't read it
    pub enabled: bool,
}

impl From<&LockedPackage> for LockedEntry {
    fn from(locked: &LockedPackage) -> Self {
        Self {
            id: locked.id.clone(),
            name: locked.dependency_name().to_string(),
            abbreviation: None,
            aliases: None,
            version: locked.version.to_string(),
            location: PackageLocation::of(locked),
            features: PackageFeatures {
                features: locked.features.clone(),
                default_features: locked.default_features,
            },
            checksum: locked.checksum.clone(),
            git: locked.git.clone(),
            commit: locked.commit.clone(),
            dependency_of: None,
            enabled: true,
        }
    }
}

/// A package's `logoxide.toml`, as the host read it from a `LockedEntry`'s location
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageManifest {
    /// The entry's `name`
    pub name: String,
    /// The text of its `logoxide.toml`
    pub manifest: String,
}

/// A dependency to add, as `logoxide.toml` writes it: `git` (with at most one of `version`, `tag`,
/// `branch`, or `rev`) or `path`
#[data]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    pub git: Option<String>,
    /// A registry's catalog whose releases are archives (`CatalogSource::Registry`'s `catalog`)
    pub registry: Option<String>,
    pub path: Option<String>,
    /// A version requirement, like Cargo's: `1.2` allows 1.2.0 up to 2.0.0
    pub version: Option<String>,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub rev: Option<String>,
    pub features: Vec<String>,
    pub default_features: bool,
    /// The package's id, when the dependency is named something else (a second fork);
    /// `package` in `logoxide.toml`
    pub package_id: Option<String>,
    /// What references name it by here, in place of the package's own abbreviation
    pub abbreviation: Option<String>,
    /// Its aliases here, in place of the package's own
    pub aliases: Option<Vec<String>>,
}

impl TryFrom<Dependency> for DetailedDependency {
    type Error = LogoxideError;

    fn try_from(d: Dependency) -> Result<Self, LogoxideError> {
        let version = (d.version.as_deref())
            .map(semver::VersionReq::parse)
            .transpose()
            .map_err(|e| LogoxideError::Invalid {
                message: format!("version: {e}"),
            })?;
        Ok(DetailedDependency {
            // `github:owner/repo` is kept as its URL
            git: d.git.as_deref().map(logoxide_core::git::expand_url),
            registry: d.registry,
            path: d.path,
            version,
            tag: d.tag,
            branch: d.branch,
            rev: d.rev,
            features: d.features,
            default_features: d.default_features,
            package: d.package_id,
            abbreviation: d.abbreviation,
            aliases: d.aliases,
            ..DetailedDependency::default()
        })
    }
}

/**
A project's `logoxide.toml` and `logoxide.lock` (a vault's manifest has no `[package]`), from
`Logoxide::parse_project`. Adding or removing a dependency gives a new project: save its
`manifestText()`, then install it (`LogoxideInstaller`) for its lock file

```ts
const project = logoxide.parseProject(await readOrNull("logoxide.toml"), await readOrNull("logoxide.lock"));
for (const entry of project.packages()) {
  // entry.location: { tag: "Library", dir: "packages/bsb/1.0.1" } or a path package's folder
}
const added = project.addDependency("kjv", { git: url, version: "1.0.1", defaultFeatures: true, ... });
await write("logoxide.toml", added.manifestText());
```
*/
#[derive(Clone)]
pub struct LogoxideProject {
    pub(crate) project: Project,
}

impl LogoxideProject {
    /// A project from its manifest's text (a new one without it) and its lock file's
    pub(crate) fn parse(
        manifest: Option<String>,
        lock: Option<String>,
    ) -> Result<Self, LogoxideError> {
        let project = Project::from_text("", manifest.as_deref(), lock.as_deref())?;
        Ok(Self { project })
    }

    /// The packages in use of a kind, as the core's `Project::packages_of` finds them, by the
    /// manifests the host read
    fn packages_of(
        &self,
        kind: PackageKind,
        manifests: Vec<PackageManifest>,
    ) -> Result<Vec<LockedEntry>, LogoxideError> {
        let manifests: std::collections::HashMap<String, String> = (manifests.into_iter())
            .map(|m| (m.name, m.manifest))
            .collect();
        let mut found = vec![];
        for entry in self.packages().into_iter().filter(|e| e.enabled) {
            let Some(text) = manifests.get(&entry.name) else {
                continue;
            };
            let manifest = logoxide_core::manifest::Manifest::parse(text).map_err(|e| {
                LogoxideError::Invalid {
                    message: format!("{}'s logoxide.toml: {e}", entry.name),
                }
            })?;
            let is = (manifest.package.as_ref()).is_some_and(|p| PackageKind::from(p.kind) == kind);
            if is {
                found.push(entry);
            }
        }
        Ok(found)
    }
}

#[export]
impl LogoxideProject {
    /// `logoxide.toml`'s text, with any dependencies added or removed (comments kept)
    pub fn manifest_text(&self) -> String {
        self.project.manifest_text()
    }

    /// The dependencies the manifest names, in order
    pub fn dependencies(&self) -> Vec<String> {
        (self.project.manifest().dependencies.keys())
            .cloned()
            .collect()
    }

    /// The packages the manifest asks for, as the lock file has them, each followed by what it
    /// brings in (`dependencyOf` names the one asked for), each package once; empty without a
    /// lock file
    pub fn packages(&self) -> Vec<LockedEntry> {
        let Some(lock) = self.project.lock() else {
            return Vec::new();
        };
        let manifest = self.project.manifest();
        let mut entries = vec![];
        let in_use: std::collections::BTreeSet<String> =
            self.project.in_use().iter().map(|p| p.name()).collect();
        // A package both asked for and brought in is listed as asked for
        let roots: std::collections::BTreeSet<String> = lock.roots().map(|p| p.name()).collect();
        let mut seen = std::collections::BTreeSet::new();
        for root in lock.roots() {
            let mut todo = vec![(root, None::<String>)];
            while let Some((locked, of)) = todo.pop() {
                if (of.is_some() && roots.contains(&locked.name())) || !seen.insert(locked.name()) {
                    continue;
                }
                let mut entry = LockedEntry::from(locked);
                if of.is_none()
                    && let Some(logoxide_core::manifest::Dependency::Detailed(d)) =
                        manifest.dependencies.get(&entry.name)
                {
                    entry.abbreviation = d.abbreviation.clone();
                    entry.aliases = d.aliases.clone();
                }
                entry.dependency_of = of;
                entry.enabled = in_use.contains(&locked.name());
                let of = Some(root.dependency_name().to_string());
                let inside = (locked.dependencies.iter()).filter_map(|name| lock.named(name));
                todo.extend(inside.rev().map(|p| (p, of.clone())));
                entries.push(entry);
            }
        }
        entries
    }

    /// Dependencies the lock file doesn't have yet (added since it was written: install them)
    pub fn unlocked(&self) -> Vec<String> {
        let locked: Vec<&str> = (self.project.lock().iter())
            .flat_map(|lock| lock.roots())
            .map(|p| p.dependency_name())
            .collect();
        (self.project.manifest().dependencies.keys())
            .filter(|name| !locked.contains(&name.as_str()))
            .cloned()
            .collect()
    }

    /// The Bible references use when they name none (`[settings] default`)
    pub fn default_bible(&self) -> Option<String> {
        (self.project.manifest().settings.as_ref()).and_then(|s| s.default.clone())
    }

    /// Bibles shown after the named one (`[settings] compare`)
    pub fn compare(&self) -> Vec<String> {
        (self.project.manifest().settings.as_ref())
            .map(|s| s.compare.clone())
            .unwrap_or_default()
    }

    /// The catalogs to search (`[settings] catalogs`, then eBible.org's and fetch.bible's lists
    /// unless `default-catalogs = false`): URLs or paths relative to the project, for
    /// `Logoxide::parse_catalog`
    pub fn catalogs(&self) -> Vec<String> {
        self.project.manifest().catalogs()
    }

    /// The registries the project names (`[settings.registries]`), by name: each a catalog (a URL,
    /// or a path relative to the project), or a folder of repositories (which only `lx` reads)
    pub fn registries(&self) -> Vec<Registry> {
        (self.project.manifest().registries().into_iter())
            .map(|(name, location)| Registry { name, location })
            .collect()
    }

    /// This project naming a registry (replacing one of that name): `name:<id>` then names its
    /// packages
    pub fn add_registry(
        &self,
        name: String,
        location: String,
    ) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        project.add_registry(&name, &location)?;
        Ok(Self { project })
    }

    /// This project without a registry
    pub fn remove_registry(&self, name: String) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        project.remove_registry(&name)?;
        Ok(Self { project })
    }

    /// Whether a catalog is one of the defaults (eBible.org's, fetch.bible's, CCEL's, and
    /// CrossWire's SWORD repository's lists)
    pub fn is_default_catalog(&self, location: String) -> bool {
        logoxide_core::catalog::DEFAULT_CATALOGS.contains(&location.as_str())
    }

    /// This project searching a catalog too (the same if it already does): a URL or a path
    /// relative to the project. Parse it first (`Logoxide::parse_catalog`) to check it
    pub fn add_catalog(&self, location: String) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        project.add_catalog(&location)?;
        Ok(Self { project })
    }

    /// This project without a catalog. Removing a default one turns the defaults off and keeps
    /// the others
    pub fn remove_catalog(&self, location: String) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        project.remove_catalog(&location)?;
        Ok(Self { project })
    }

    /// This project with a dependency added (or replaced); `name` is the package's id
    pub fn add_dependency(
        &self,
        name: String,
        dependency: Dependency,
    ) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        project.add(&name, &dependency.try_into()?)?;
        Ok(Self { project })
    }

    /// This project without a dependency (the same if it hasn't one)
    pub fn remove_dependency(&self, name: String) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        project.remove(&name)?;
        Ok(Self { project })
    }

    /**
    This project with a dependency turned on or off (`enabled = false`): off, it stays in the
    manifest and the lock, and installed, but `packages()` marks it (and what it brings in)
    `enabled: false` for hosts to leave out. An error for a name the manifest doesn't have
    */
    pub fn set_enabled(
        &self,
        name: String,
        enabled: bool,
    ) -> Result<LogoxideProject, LogoxideError> {
        let mut project = self.project.clone();
        if !project.set_enabled(&name, enabled)? {
            return Err(LogoxideError::NotFound {
                message: format!("no dependency `{name}`"),
            });
        }
        Ok(Self { project })
    }

    /**
    The names two or more of these Bibles answer to, ignoring case (a package the manifest asks
    for at one version and a collection at another, both `KJV`): references naming one can't
    say which, so hosts should say so
    */
    pub fn name_clashes(&self, bibles: Vec<NamedBible>) -> Vec<NameClash> {
        let bibles = (bibles.into_iter()).map(|b| (b.label, b.abbreviation, b.aliases));
        (logoxide_core::project::name_clashes(bibles).into_iter())
            .map(|c| NameClash {
                name: c.name,
                packages: c.packages,
            })
            .collect()
    }

    /**
    The commentaries in use (`kind = "commentary"`): of `packages()`, those that are on and whose
    `logoxide.toml` (given by the host, by the entry's name: the lock doesn't say what a package
    is) makes it one, in that order. An entry whose manifest isn't given is left out; one that
    can't be read is an error naming it
    */
    pub fn commentaries(
        &self,
        manifests: Vec<PackageManifest>,
    ) -> Result<Vec<LockedEntry>, LogoxideError> {
        self.packages_of(PackageKind::Commentary, manifests)
    }

    /// The dictionaries in use (`kind = "lexicon"`), as `commentaries` finds them
    pub fn dictionaries(
        &self,
        manifests: Vec<PackageManifest>,
    ) -> Result<Vec<LockedEntry>, LogoxideError> {
        self.packages_of(PackageKind::Lexicon, manifests)
    }

    /// The cross-reference packages in use (`kind = "cross-references"`), as `commentaries`
    /// finds them
    pub fn cross_references(
        &self,
        manifests: Vec<PackageManifest>,
    ) -> Result<Vec<LockedEntry>, LogoxideError> {
        self.packages_of(PackageKind::CrossReferences, manifests)
    }

    /// Whether a dependency is on (any name the manifest doesn't have is)
    pub fn is_enabled(&self, name: String) -> bool {
        self.project.is_enabled(&name)
    }

    /// What the manifest calls its dependencies that are on, by name
    pub fn enabled_dependencies(&self) -> Vec<String> {
        self.project.dependencies_enabled(true)
    }

    /// What the manifest calls its dependencies that are turned off (`enabled = false`), by name
    pub fn disabled_dependencies(&self) -> Vec<String> {
        self.project.dependencies_enabled(false)
    }
}
