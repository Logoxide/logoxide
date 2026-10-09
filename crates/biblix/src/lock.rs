/*!
`biblix.lock`: the exact text a vault (or a machine's library) uses, like `Cargo.lock`. Each
package's version, commit, and checksum, so every device installs the same text and links mean the
same thing everywhere.

```toml
version = 1

[[package]]
id = "bsb"
version = "1.0.0"
git = "https://example.org/bsb"
tag = "v1.0.0"
commit = "b0d4882…"
checksum = "sha256:44cb…"
features = ["usx"]
```
*/

use serde::{Deserialize, Serialize};

use crate::error::{BiblixError, Result};

/// The lock file's name, beside `biblix.toml`
pub const LOCKFILE: &str = "biblix.lock";

/// The lock file format's version
const LOCK_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lockfile {
    pub version: u32,
    /// What the manifest asks for, as `id key` (`kjv 1.0.1+ga7ff59b2`; the rest are their
    /// dependencies). Lock files from before forks name them `id version`, still read
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, rename = "package")]
    pub packages: Vec<LockedPackage>,
}

impl Default for Lockfile {
    fn default() -> Self {
        Self {
            version: LOCK_VERSION,
            dependencies: vec![],
            packages: vec![],
        }
    }
}

/// One installed package version, and exactly where it came from
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LockedPackage {
    pub id: String,
    /// What the manifest calls it, when that isn't its id (`kjv-bob = { package = "kjv", … }`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename: Option<String>,
    pub version: semver::Version,
    /// The repository it came from (or `path`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// How it was asked for: the tag, branch, or revision (the commit is what's used)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// A folder used in place (not copied into the library), relative to the manifest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The checksum of every file (not for path packages, whose files change as they're written)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// The features turned on, besides the defaults (unless `default-features = false`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default = "yes", skip_serializing_if = "Clone::clone")]
    pub default_features: bool,
    /// The packages it depends on, as `id version`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
}

fn yes() -> bool {
    true
}

impl LockedPackage {
    /**
    Its folder's name in the library: the version and its commit (`1.2.0+g3f9c2a1b`), so forks
    of a package at the same version, and a moved branch, never share a folder; a path package's
    is its version
    */
    pub fn key(&self) -> String {
        match &self.commit {
            Some(commit) => format!("{}+g{}", self.version, &commit[..commit.len().min(8)]),
            None => self.version.to_string(),
        }
    }

    /// `id key`, how the lock file's `dependencies` name it
    pub fn name(&self) -> String {
        format!("{} {}", self.id, self.key())
    }

    /// What the manifest calls it: its rename, else its id
    pub fn dependency_name(&self) -> &str {
        self.rename.as_deref().unwrap_or(&self.id)
    }

    /// How people read it: `kjv 1.0.1`, or `kjv-bob (kjv) 1.0.1` when renamed
    pub fn label(&self) -> String {
        match &self.rename {
            Some(rename) => format!("{rename} ({}) {}", self.id, self.version),
            None => format!("{} {}", self.id, self.version),
        }
    }
}

impl Lockfile {
    pub fn parse(text: &str) -> Result<Lockfile> {
        let lock: Lockfile = toml::from_str(text).map_err(|e| BiblixError::Lock(e.to_string()))?;
        if lock.version != LOCK_VERSION {
            return Err(BiblixError::Lock(format!(
                "version {} (this biblix reads version {LOCK_VERSION})",
                lock.version
            )));
        }
        Ok(lock)
    }

    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string_pretty(self).map_err(|e| BiblixError::Lock(e.to_string()))?;
        Ok(format!(
            "# Written by bx: the exact text this uses. Commit it with biblix.toml.\n{body}"
        ))
    }

    /// A package the manifest asks for, by what it calls it (its id, or its rename)
    pub fn get(&self, name: &str) -> Option<&LockedPackage> {
        self.roots().find(|p| p.dependency_name() == name)
    }

    /// A package by its name in `dependencies` (`id key`, or `id version` in older lock files)
    pub fn named(&self, name: &str) -> Option<&LockedPackage> {
        (self.packages.iter().find(|p| p.name() == name))
            .or_else(|| (self.packages.iter()).find(|p| format!("{} {}", p.id, p.version) == name))
    }

    /// The packages the manifest asks for (not their dependencies)
    pub fn roots(&self) -> impl Iterator<Item = &LockedPackage> {
        self.dependencies.iter().filter_map(|name| self.named(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_names_folders() {
        let package = LockedPackage {
            id: "bsb".into(),
            rename: None,
            version: "1.2.0".parse().unwrap(),
            git: Some("https://example.org/bsb".into()),
            tag: Some("v1.2.0".into()),
            branch: None,
            rev: None,
            commit: Some("3f9c2a1b77".into()),
            path: None,
            checksum: Some("sha256:00".into()),
            features: vec!["sword".into()],
            default_features: true,
            dependencies: vec!["strongs 2.0.0".into()],
        };
        assert_eq!(package.key(), "1.2.0+g3f9c2a1b");
        assert_eq!(package.name(), "bsb 1.2.0+g3f9c2a1b");
        let branch = LockedPackage {
            tag: None,
            branch: Some("main".into()),
            ..package.clone()
        };
        assert_eq!(branch.key(), "1.2.0+g3f9c2a1b");
        let path = LockedPackage {
            git: None,
            commit: None,
            path: Some("imports/bsb".into()),
            ..package.clone()
        };
        assert_eq!(path.key(), "1.2.0");
        let lock = Lockfile {
            packages: vec![package, branch],
            ..Lockfile::default()
        };
        assert_eq!(Lockfile::parse(&lock.to_toml().unwrap()).unwrap(), lock);
        assert!(Lockfile::parse("version = 9").is_err());
    }
}
