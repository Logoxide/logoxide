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
    /// What the manifest asks for, as `id key` (`kjv 1.0.1+ga7ff59b2`), or `rename (id) key`
    /// when the manifest calls it something else; the rest are their dependencies
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
    /// The import it was made by (`ebible:eng-kjv2006`), for a package made from another
    /// publisher's download and kept in the library by its checksum
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import: Option<String>,
    /// That publisher's version of the text (`2026-10-08`, `3.1`), as the import found it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_version: Option<String>,
    /// Each file the import was made from, and its SHA-256, so it can be made again exactly
    #[serde(default, rename = "download", skip_serializing_if = "Vec::is_empty")]
    pub downloads: Vec<LockedDownload>,
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

/**
A file an import was made from: its name in the import (`zip`, `thml`, `JHN`), where it was
downloaded, and the SHA-256 of its bytes (`sha256:` and hex). An import taken from a copy already
in the library, which nothing downloaded, has no SHA-256: made again, it's checked by its files'
checksum alone
*/
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedDownload {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl LockedPackage {
    /// `id` at `version`, from nowhere yet: the resolver fills in where it came from
    pub fn new(id: &str, version: semver::Version) -> Self {
        Self {
            id: id.to_string(),
            rename: None,
            version,
            git: None,
            tag: None,
            branch: None,
            rev: None,
            commit: None,
            folder: None,
            path: None,
            registry: None,
            archive: None,
            mirrors: vec![],
            sha256: None,
            import: None,
            upstream_version: None,
            downloads: vec![],
            checksum: None,
            features: vec![],
            default_features: true,
            dependencies: vec![],
        }
    }

    /**
    Its folder's name in the library: the version and its commit (`1.2.0+g3f9c2a1b`), so forks
    of a package at the same version, and a moved branch, never share a folder; a registry
    archive's, the version and its hash (`1.2.0+a9f86d081`); an import's, the version and its
    files' checksum (`1.0.0+i44cb8e21`), so two projects' imports share a folder exactly when
    their files are the same; a path package's, its version
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
            (None, None) => match (&self.import, &self.checksum) {
                (Some(_), Some(checksum)) => {
                    let hex = checksum.strip_prefix("sha256:").unwrap_or(checksum);
                    format!("{}+i{}", self.version, short(hex))
                }
                _ => self.version.to_string(),
            },
        }
    }

    /**
    Checks what names its folder in the library: its id is an id, its commit a whole object id
    (40 lowercase hex digits, or 64 in a SHA-256 repository), its archive's hash `sha256:` and 64
    of them, its folder a plain relative path
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
            && !((commit.len() == 40 || commit.len() == 64) && hex(commit))
        {
            return bad(format!(
                "{}: commit `{commit}` isn't a whole commit id (40 or 64 hex digits)",
                self.id
            ));
        }
        if let Some(sha256) = &self.sha256 {
            let digits = sha256.strip_prefix("sha256:").unwrap_or_default();
            if digits.len() != 64 || !hex(digits) {
                return bad(format!("{}: sha256 `{sha256}` isn't a SHA-256", self.id));
            }
        }
        if self.import.is_some() {
            let digits = (self.checksum.as_deref())
                .and_then(|c| c.strip_prefix("sha256:"))
                .unwrap_or_default();
            if digits.len() != 64 || !hex(digits) {
                return bad(format!("{}: an import needs its files' checksum", self.id));
            }
            for download in &self.downloads {
                let Some(sha256) = &download.sha256 else {
                    continue;
                };
                let digits = sha256.strip_prefix("sha256:").unwrap_or_default();
                if digits.len() != 64 || !hex(digits) {
                    return bad(format!(
                        "{}: download `{}` has no SHA-256",
                        self.id, download.name
                    ));
                }
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

    /**
    How the lock file's `dependencies` name it: `id key`, or `rename (id) key` when the manifest
    calls it something else, so two names for one commit stay two entries (sharing one folder)
    */
    pub fn name(&self) -> String {
        match &self.rename {
            Some(rename) => format!("{rename} ({}) {}", self.id, self.key()),
            None => format!("{} {}", self.id, self.key()),
        }
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

/// A release that came back with other contents than a previous lock file had: [`Lockfile::moved_since`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    /// The package, as people read it (`kjv 2.0.0`)
    pub label: String,
    /// What should have pinned it: its tag (`v2.0.0`), or its registry version
    pub release: String,
    /// What it was, and what it is now: commits, else archive hashes, else checksums
    pub before: String,
    pub after: String,
}

impl Lockfile {
    /**
    Releases in this lock that a previous one had with other contents: a tag that now names
    another commit (it was moved), or a registry version whose archive or files changed. A branch
    moving is what branches do, and isn't one
    */
    pub fn moved_since(&self, previous: &Lockfile) -> Vec<Moved> {
        let mut moved = vec![];
        for now in &self.packages {
            let release = match (&now.tag, &now.registry) {
                (Some(tag), _) => tag.clone(),
                (None, Some(_)) => format!("version {}", now.version),
                (None, None) => continue,
            };
            let before = (previous.packages.iter()).find(|p| {
                p.id == now.id
                    && p.git == now.git
                    && p.folder == now.folder
                    && p.registry == now.registry
                    && match &now.tag {
                        Some(tag) => p.tag.as_ref() == Some(tag),
                        None => p.version == now.version,
                    }
            });
            let Some(before) = before else {
                continue;
            };
            let changed = [
                (&before.commit, &now.commit),
                (&before.sha256, &now.sha256),
                (&before.checksum, &now.checksum),
            ]
            .into_iter()
            .find_map(|(was, is)| match (was, is) {
                (Some(was), Some(is)) if was != is => Some((was.clone(), is.clone())),
                _ => None,
            });
            if let Some((was, is)) = changed {
                moved.push(Moved {
                    label: now.label(),
                    release,
                    before: was,
                    after: is,
                });
            }
        }
        moved
    }

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
        // Every name must be a package here in the one form written (`id key`, or
        // `rename (id) key`): a lock in an older form is remade, not read
        let names = (lock.dependencies.iter())
            .chain(lock.packages.iter().flat_map(|p| p.dependencies.iter()));
        for name in names {
            if lock.named(name).is_none() {
                return Err(LogoxideError::Lock(format!(
                    "`{name}` names no package here (a lock written by an older lx?): delete {LOCKFILE} and run `lx install`"
                )));
            }
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

    /// A package by its name in `dependencies` (`id key`, or `rename (id) key`)
    pub fn named(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name() == name)
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
            git: Some("https://example.org/bsb".into()),
            tag: Some("v1.2.0".into()),
            commit: Some(format!("3f9c2a1b{}", "7".repeat(32))),
            checksum: Some("sha256:00".into()),
            features: vec!["sword".into()],
            dependencies: vec!["strongs 2.0.0+g01234567".into()],
            ..LockedPackage::new("bsb", "1.2.0".parse().unwrap())
        };
        assert_eq!(package.key(), "1.2.0+g3f9c2a1b");
        assert_eq!(package.name(), "bsb 1.2.0+g3f9c2a1b");
        let renamed = LockedPackage {
            rename: Some("bsb-two".into()),
            ..package.clone()
        };
        assert_eq!(renamed.key(), "1.2.0+g3f9c2a1b");
        assert_eq!(renamed.name(), "bsb-two (bsb) 1.2.0+g3f9c2a1b");
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
        let import = LockedPackage {
            import: Some("ebible:eng-kjv2006".into()),
            upstream_version: Some("2026-10-08".into()),
            downloads: vec![LockedDownload {
                name: "zip".into(),
                url: "https://ebible.org/Scriptures/eng-kjv2006_usfm.zip".into(),
                sha256: Some(format!("sha256:{}", "9f".repeat(32))),
            }],
            checksum: Some(format!("sha256:44cb8e21{}", "0".repeat(56))),
            ..LockedPackage::new("ebible-eng-kjv2006", "1.0.0".parse().unwrap())
        };
        assert_eq!(import.key(), "1.0.0+i44cb8e21");
        import.check().unwrap();
        let unpinned = LockedPackage {
            checksum: None,
            ..import.clone()
        };
        assert!(unpinned.check().is_err());
        let import_lock = Lockfile {
            packages: vec![import],
            ..Lockfile::default()
        };
        assert_eq!(
            Lockfile::parse(&import_lock.to_toml().unwrap()).unwrap(),
            import_lock
        );
        let strongs = LockedPackage {
            git: Some("https://example.org/strongs".into()),
            commit: Some(format!("01234567{}", "8".repeat(32))),
            ..LockedPackage::new("strongs", "2.0.0".parse().unwrap())
        };
        let lock = Lockfile {
            dependencies: vec![package.name()],
            packages: vec![package.clone(), branch.clone(), strongs],
            ..Lockfile::default()
        };
        assert_eq!(Lockfile::parse(&lock.to_toml().unwrap()).unwrap(), lock);
        assert!(Lockfile::parse("version = 9").is_err());
        // A name in another form (an older lock's `id version`) names no package: refused
        let old = Lockfile {
            dependencies: vec!["bsb 1.2.0".into()],
            packages: vec![package, branch],
            ..Lockfile::default()
        };
        let error = Lockfile::parse(&old.to_toml().unwrap())
            .unwrap_err()
            .to_string();
        assert!(error.contains("`bsb 1.2.0` names no package"), "{error}");
    }

    #[test]
    fn moved_releases_are_found() {
        let lock = |entries: &str| Lockfile::parse(&format!("version = 1\n{entries}")).unwrap();
        // Whole commit ids, as a lock must have them
        let full = |prefix: &str| format!("{prefix}{}", "0".repeat(32));
        let tagged = |commit: &str, checksum: &str| {
            format!(
                "[[package]]\nid = \"tb\"\nversion = \"1.0.0\"\ngit = \"https://e.org/tb\"\ntag = \"v1.0.0\"\ncommit = \"{}\"\nchecksum = \"sha256:{checksum}\"\n",
                full(commit)
            )
        };
        let branch = |commit: &str| {
            format!(
                "[[package]]\nid = \"nb\"\nversion = \"1.0.0\"\ngit = \"https://e.org/nb\"\nbranch = \"main\"\ncommit = \"{}\"\n",
                full(commit)
            )
        };
        let before = lock(&format!(
            "{}{}",
            tagged("aaaa1111", "01"),
            branch("cccc3333")
        ));
        // The same: nothing moved
        assert!(before.moved_since(&before).is_empty());
        // A tag naming another commit was moved; a branch moving isn't
        let after = lock(&format!(
            "{}{}",
            tagged("bbbb2222", "02"),
            branch("dddd4444")
        ));
        assert_eq!(
            after.moved_since(&before),
            [Moved {
                label: "tb 1.0.0".into(),
                release: "v1.0.0".into(),
                before: full("aaaa1111"),
                after: full("bbbb2222"),
            }]
        );
        // Another version isn't the same release
        let newer = lock(&tagged("bbbb2222", "02").replace("1.0.0", "1.1.0"));
        assert!(newer.moved_since(&before).is_empty());
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
