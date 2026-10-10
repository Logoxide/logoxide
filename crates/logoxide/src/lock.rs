/*!
`logoxide.lock`: the exact text a vault (or a machine's library) uses, like `Cargo.lock`. Each
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

use crate::error::{LogoxideError, Result};

/// The lock file's name, beside `logoxide.toml`
pub const LOCKFILE: &str = "logoxide.lock";

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
    /// Its folder in the repository, for a repository of many packages (tagged `<folder>/v…`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// A folder used in place (not copied into the library), relative to the manifest
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The registry's catalog it was chosen from, for a release downloaded as an archive
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// That release's archive, and the SHA-256 of its bytes (`sha256:` and hex)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
    /// Other places serving the same archive, tried in order after `archive`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
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
    of a package at the same version, and a moved branch, never share a folder; a registry
    archive's, the version and its hash (`1.2.0+a9f86d081`); a path package's, its version
    */
    pub fn key(&self) -> String {
        // Only hex digits name the folder, whatever a lock built elsewhere holds ([`Self::check`])
        let short = |hex: &str| -> String {
            (hex.chars().filter(char::is_ascii_hexdigit))
                .take(8)
                .collect()
        };
        match (&self.commit, &self.sha256) {
            (Some(commit), _) => format!("{}+g{}", self.version, short(commit)),
            // A registry's archive: by its hash, so two registries' releases never share one
            (None, Some(sha256)) => {
                let hex = sha256.strip_prefix("sha256:").unwrap_or(sha256);
                format!("{}+a{}", self.version, short(hex))
            }
            (None, None) => self.version.to_string(),
        }
    }

    /**
    Checks what names its folder in the library: its id is an id, its commit lowercase hex (7 to
    64 digits), its archive's hash `sha256:` and 64 of them, its folder a plain relative path
    */
    pub fn check(&self) -> Result<()> {
        let bad = |message: String| Err(LogoxideError::Lock(message));
        if !crate::manifest::is_valid_id(&self.id) {
            return bad(format!("`{}` isn't a package id", self.id));
        }
        let hex = |s: &str| {
            s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if let Some(commit) = &self.commit
            && !((7..=64).contains(&commit.len()) && hex(commit))
        {
            return bad(format!("{}: commit `{commit}` isn't a commit", self.id));
        }
        if let Some(sha256) = &self.sha256 {
            let digits = sha256.strip_prefix("sha256:").unwrap_or_default();
            if digits.len() != 64 || !hex(digits) {
                return bad(format!("{}: sha256 `{sha256}` isn't a SHA-256", self.id));
            }
        }
        if let Some(folder) = &self.folder {
            let plain = |p: &str| !p.is_empty() && p != "." && p != ".." && !p.starts_with('-');
            if !folder.split('/').all(plain) || folder.contains([':', '\\', '\0']) {
                return bad(format!("{}: folder `{folder}` isn't a folder", self.id));
            }
        }
        Ok(())
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
        let lock: Lockfile =
            toml::from_str(text).map_err(|e| LogoxideError::Lock(e.to_string()))?;
        if lock.version != LOCK_VERSION {
            return Err(LogoxideError::Lock(format!(
                "version {} (this logoxide reads version {LOCK_VERSION})",
                lock.version
            )));
        }
        for package in &lock.packages {
            package.check()?;
        }
        Ok(lock)
    }

    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string_pretty(self).map_err(|e| LogoxideError::Lock(e.to_string()))?;
        Ok(format!(
            "# Written by lx: the exact text this uses. Commit it with logoxide.toml.\n{body}"
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
            folder: None,
            path: None,
            registry: None,
            archive: None,
            mirrors: vec![],
            sha256: None,
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
        let archive = LockedPackage {
            git: None,
            commit: None,
            registry: Some("https://example.org/catalog.toml".into()),
            archive: Some("https://example.org/bsb/bsb-1.2.0.tar.gz".into()),
            sha256: Some("sha256:9f86d081884c7d65".into()),
            ..package.clone()
        };
        assert_eq!(archive.key(), "1.2.0+a9f86d081");
        let lock = Lockfile {
            packages: vec![package, branch],
            ..Lockfile::default()
        };
        assert_eq!(Lockfile::parse(&lock.to_toml().unwrap()).unwrap(), lock);
        assert!(Lockfile::parse("version = 9").is_err());
    }

    #[test]
    fn what_names_folders_is_checked() {
        let lock = |entry: &str| {
            Lockfile::parse(&format!(
                "version = 1\n[[package]]\nid = \"bsb\"\nversion = \"1.0.0\"\n{entry}\n"
            ))
        };
        let sha = format!("sha256:{}", "9f".repeat(32));
        assert!(lock(&format!("commit = \"{}\"", "a1".repeat(20))).is_ok());
        assert!(lock(&format!("sha256 = \"{sha}\"")).is_ok());
        assert!(lock("folder = \"books/romans\"").is_ok());
        for bad in [
            "commit = \"../../x\"",
            "commit = \"é\"",
            "commit = \"ABCDEF12\"",
            "commit = \"\"",
            "sha256 = \"sha256:../../x\"",
            "sha256 = \"9f86d081\"",
            "folder = \"../x\"",
        ] {
            assert!(lock(bad).is_err(), "{bad}");
        }
        let bad_id = "version = 1\n[[package]]\nid = \"../x\"\nversion = \"1.0.0\"\n";
        assert!(Lockfile::parse(bad_id).is_err());
    }
}
