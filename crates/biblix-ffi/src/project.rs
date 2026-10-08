//! A vault's (or any project's) `biblix.toml` and `biblix.lock`: the packages it asks for, and
//! where each one's files are.

use biblix_core::{LockedPackage, Lockfile, Manifest};
use boltffi::*;

use crate::{BiblixError, PackageFeatures};

/// Where a locked package's files are
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackageLocation {
    /// In the library, under its folder: `packages/<id>/<version>`
    Library { dir: String },
    /// A path package, used where it is: relative to the project's folder
    Path { dir: String },
}

/// One package the project asks for, exactly as its lock file has it
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockedEntry {
    pub id: String,
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
        let location = match &locked.path {
            Some(dir) => PackageLocation::Path { dir: dir.clone() },
            None => PackageLocation::Library {
                dir: format!("packages/{}/{}", locked.id, locked.key()),
            },
        };
        Self {
            id: locked.id.clone(),
            version: locked.version.to_string(),
            location,
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

/**
A project's `biblix.toml` and `biblix.lock` (a vault's manifest has no `[package]`), from
`Biblix::parse_project`

```ts
const project = biblix.parseProject(await read("biblix.toml"), await readOrNull("biblix.lock"));
for (const entry of project.packages()) {
  // entry.location: { tag: "Library", dir: "packages/bsb/1.0.1" } or a path package's folder
}
project.defaultBible(); // "bsb", from [settings]
```
*/
pub struct BiblixProject {
    manifest: Manifest,
    lock: Option<Lockfile>,
}

impl BiblixProject {
    /// A project from its manifest's text and its lock file's, if it has one
    pub(crate) fn parse(manifest: String, lock: Option<String>) -> Result<Self, BiblixError> {
        let manifest = Manifest::parse(&manifest)?;
        let lock = lock.as_deref().map(Lockfile::parse).transpose()?;
        Ok(Self { manifest, lock })
    }
}

#[export]
impl BiblixProject {
    /// The dependencies the manifest names, in order
    pub fn dependencies(&self) -> Vec<String> {
        self.manifest.dependencies.keys().cloned().collect()
    }

    /// The packages the manifest asks for, as the lock file has them (not their dependencies);
    /// empty without a lock file
    pub fn packages(&self) -> Vec<LockedEntry> {
        let Some(lock) = &self.lock else {
            return Vec::new();
        };
        lock.roots().map(LockedEntry::from).collect()
    }

    /// Dependencies the lock file doesn't have yet (added since it was written: run `bx install`)
    pub fn unlocked(&self) -> Vec<String> {
        let locked: Vec<&str> = (self.lock.iter())
            .flat_map(|lock| lock.roots())
            .map(|p| p.id.as_str())
            .collect();
        (self.manifest.dependencies.keys())
            .filter(|name| !locked.contains(&name.as_str()))
            .cloned()
            .collect()
    }

    /// The Bible references use when they name none (`[settings] default`)
    pub fn default_bible(&self) -> Option<String> {
        (self.manifest.settings.as_ref()).and_then(|s| s.default.clone())
    }

    /// Bibles shown after the named one (`[settings] compare`)
    pub fn compare(&self) -> Vec<String> {
        (self.manifest.settings.as_ref())
            .map(|s| s.compare.clone())
            .unwrap_or_default()
    }
}
