//! A vault's (or any project's) `biblix.toml` and `biblix.lock`: the packages it asks for, where
//! each one's files are, and adding or removing them (keeping the manifest's comments).

use biblix_core::{LockedPackage, Project, library::library_dir, manifest::DetailedDependency};
use boltffi::*;

use crate::{BiblixError, PackageFeatures};

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

/// One package the project asks for, exactly as its lock file has it
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedEntry {
    pub id: String,
    /// What the manifest calls it: its id, or another name (a second fork of a package)
    pub name: String,
    /// What references name it by here, when the manifest gives its own (else the package's
    /// abbreviation: `BiblixPackage.addTo`; with these, `addToAs`)
    pub abbreviation: Option<String>,
    /// Its aliases here, when the manifest gives its own
    pub aliases: Option<Vec<String>>,
    pub version: String,
    pub location: PackageLocation,
    /// The features to open it with
    pub features: PackageFeatures,
    /// What every file hashes to (`BiblixPackage.checksum()`); none for path packages, whose
    /// files change as they're written
    pub checksum: Option<String>,
    /// The repository it came from
    pub git: Option<String>,
    /// The commit its files are from
    pub commit: Option<String>,
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
        }
    }
}

/// A dependency to add, as `biblix.toml` writes it: `git` (with at most one of `version`, `tag`,
/// `branch`, or `rev`) or `path`
#[data]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dependency {
    pub git: Option<String>,
    pub path: Option<String>,
    /// A version requirement, like Cargo's: `1.2` allows 1.2.0 up to 2.0.0
    pub version: Option<String>,
    pub tag: Option<String>,
    pub branch: Option<String>,
    pub rev: Option<String>,
    pub features: Vec<String>,
    pub default_features: bool,
    /// The package's id, when the dependency is named something else (a second fork);
    /// `package` in `biblix.toml`
    pub package_id: Option<String>,
    /// What references name it by here, in place of the package's own abbreviation
    pub abbreviation: Option<String>,
    /// Its aliases here, in place of the package's own
    pub aliases: Option<Vec<String>>,
}

impl TryFrom<Dependency> for DetailedDependency {
    type Error = BiblixError;

    fn try_from(d: Dependency) -> Result<Self, BiblixError> {
        let version = (d.version.as_deref())
            .map(semver::VersionReq::parse)
            .transpose()
            .map_err(|e| BiblixError::Manifest {
                message: format!("version: {e}"),
            })?;
        Ok(DetailedDependency {
            // `github:owner/repo` is kept as its URL
            git: d.git.as_deref().map(biblix_core::git::expand_url),
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
A project's `biblix.toml` and `biblix.lock` (a vault's manifest has no `[package]`), from
`Biblix::parse_project`. Adding or removing a dependency gives a new project: save its
`manifestText()`, then install it (`BiblixInstaller`) for its lock file

```ts
const project = biblix.parseProject(await readOrNull("biblix.toml"), await readOrNull("biblix.lock"));
for (const entry of project.packages()) {
  // entry.location: { tag: "Library", dir: "packages/bsb/1.0.1" } or a path package's folder
}
const added = project.addDependency("kjv", { git: url, version: "1.0.1", defaultFeatures: true, ... });
await write("biblix.toml", added.manifestText());
```
*/
#[derive(Clone)]
pub struct BiblixProject {
    pub(crate) project: Project,
}

impl BiblixProject {
    /// A project from its manifest's text (a new one without it) and its lock file's
    pub(crate) fn parse(
        manifest: Option<String>,
        lock: Option<String>,
    ) -> Result<Self, BiblixError> {
        let project = Project::from_text("", manifest.as_deref(), lock.as_deref())?;
        Ok(Self { project })
    }
}

#[export]
impl BiblixProject {
    /// `biblix.toml`'s text, with any dependencies added or removed (comments kept)
    pub fn manifest_text(&self) -> String {
        self.project.manifest_text()
    }

    /// The dependencies the manifest names, in order
    pub fn dependencies(&self) -> Vec<String> {
        (self.project.manifest().dependencies.keys())
            .cloned()
            .collect()
    }

    /// The packages the manifest asks for, as the lock file has them (not their dependencies);
    /// empty without a lock file
    pub fn packages(&self) -> Vec<LockedEntry> {
        let Some(lock) = self.project.lock() else {
            return Vec::new();
        };
        let manifest = self.project.manifest();
        (lock.roots())
            .map(|locked| {
                let mut entry = LockedEntry::from(locked);
                if let Some(biblix_core::manifest::Dependency::Detailed(d)) =
                    manifest.dependencies.get(&entry.name)
                {
                    entry.abbreviation = d.abbreviation.clone();
                    entry.aliases = d.aliases.clone();
                }
                entry
            })
            .collect()
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
    /// `Biblix::parse_catalog`
    pub fn catalogs(&self) -> Vec<String> {
        self.project.manifest().catalogs()
    }

    /// Whether a catalog is one of the defaults (eBible.org's or fetch.bible's list)
    pub fn is_default_catalog(&self, location: String) -> bool {
        biblix_core::catalog::DEFAULT_CATALOGS.contains(&location.as_str())
    }

    /// This project searching a catalog too (the same if it already does): a URL or a path
    /// relative to the project. Parse it first (`Biblix::parse_catalog`) to check it
    pub fn add_catalog(&self, location: String) -> Result<BiblixProject, BiblixError> {
        let mut project = self.project.clone();
        project.add_catalog(&location)?;
        Ok(Self { project })
    }

    /// This project without a catalog. Removing eBible.org's or fetch.bible's list turns the
    /// defaults off and keeps the other one
    pub fn remove_catalog(&self, location: String) -> Result<BiblixProject, BiblixError> {
        let mut project = self.project.clone();
        project.remove_catalog(&location)?;
        Ok(Self { project })
    }

    /// This project with a dependency added (or replaced); `name` is the package's id
    pub fn add_dependency(
        &self,
        name: String,
        dependency: Dependency,
    ) -> Result<BiblixProject, BiblixError> {
        let mut project = self.project.clone();
        project.add(&name, &dependency.try_into()?)?;
        Ok(Self { project })
    }

    /// This project without a dependency (the same if it hasn't one)
    pub fn remove_dependency(&self, name: String) -> Result<BiblixProject, BiblixError> {
        let mut project = self.project.clone();
        project.remove(&name)?;
        Ok(Self { project })
    }
}
