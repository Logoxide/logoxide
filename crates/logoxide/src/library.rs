/*!
The library: installed package versions, side by side, as plain files.

```text
<library>/                        LOGOXIDE_HOME, else $XDG_DATA_HOME/logoxide (any folder: inside
  logoxide.toml                     .obsidian/ to sync with a vault); this machine's own
  logoxide.lock                     requirements and their lock, for apps without a vault
  packages/<id>/<version>/        each version, without .git, so it syncs well
  packages/<id>/<version>+g<commit>/   from a branch or a revision rather than its release
<cache>/                          per device, never synced: LOGOXIDE_CACHE, else $XDG_CACHE_HOME/logoxide
  books/                          parsed books and verse counts (see cache.rs)
  git/                            scratch space while fetching
```
*/

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    cache::Cache,
    error::{LogoxideError, Result},
    lock::LockedPackage,
    manifest::MANIFEST,
    package::{Features, Package},
    resolve::Packages,
    storage::{FsStorage, Storage},
};

#[derive(Clone, Debug)]
pub struct Library {
    root: PathBuf,
    cache: PathBuf,
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> PathBuf {
    env_dir("HOME").unwrap_or_else(|| PathBuf::from("."))
}

impl Library {
    pub fn new(root: impl Into<PathBuf>, cache: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cache: cache.into(),
        }
    }

    /// `LOGOXIDE_HOME`, else `$XDG_DATA_HOME/logoxide`, else `~/.local/share/logoxide`
    pub fn default_root() -> PathBuf {
        env_dir("LOGOXIDE_HOME")
            .or_else(|| env_dir("XDG_DATA_HOME").map(|d| d.join("logoxide")))
            .unwrap_or_else(|| home().join(".local/share/logoxide"))
    }

    /// `LOGOXIDE_CACHE`, else `$XDG_CACHE_HOME/logoxide`, else `~/.cache/logoxide`
    pub fn default_cache() -> PathBuf {
        env_dir("LOGOXIDE_CACHE")
            .or_else(|| env_dir("XDG_CACHE_HOME").map(|d| d.join("logoxide")))
            .unwrap_or_else(|| home().join(".cache/logoxide"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache
    }

    /// Where a locked version is (or goes): in the library, or its own folder for a path package
    /// (relative to `base`, the folder of the manifest that named it)
    pub fn package_dir(&self, locked: &LockedPackage, base: &Path) -> PathBuf {
        match &locked.path {
            Some(path) => base.join(path),
            None => self
                .root
                .join("packages")
                .join(&locked.id)
                .join(locked.key()),
        }
    }

    /// Whether a locked version's files are in the library
    pub fn is_installed(&self, locked: &LockedPackage, base: &Path) -> bool {
        self.package_dir(locked, base).join(MANIFEST).is_file()
    }

    /// Every installed version: `(id, folder name)`
    pub fn installed(&self) -> Vec<(String, String)> {
        let mut found = vec![];
        let Ok(ids) = std::fs::read_dir(self.root.join("packages")) else {
            return found;
        };
        for id in ids.flatten() {
            let Ok(versions) = std::fs::read_dir(id.path()) else {
                continue;
            };
            for version in versions.flatten() {
                if version.path().join(MANIFEST).is_file() {
                    found.push((
                        id.file_name().to_string_lossy().into_owned(),
                        version.file_name().to_string_lossy().into_owned(),
                    ));
                }
            }
        }
        found.sort();
        found
    }

    /// A locked version, opened with its locked features
    pub fn open(&self, locked: &LockedPackage, base: &Path) -> Result<Package> {
        let dir = self.package_dir(locked, base);
        if !dir.join(MANIFEST).is_file() {
            return Err(LogoxideError::NotFound(format!(
                "{} (not installed: run `lx install`)",
                locked.name()
            )));
        }
        open_locked(Arc::new(FsStorage::new(dir)), locked)
    }

    /// The library seen from a project in `base` (where its path packages are), for resolving
    pub fn packages<'a>(&'a self, base: &'a Path) -> LibraryPackages<'a> {
        LibraryPackages {
            library: self,
            base,
        }
    }

    /// The cache of parsed books
    pub fn book_cache(&self) -> Cache {
        Cache::new(Arc::new(FsStorage::new(self.cache.join("books"))))
    }

    /// Scratch space for fetching
    pub fn git_dir(&self) -> PathBuf {
        self.cache.join("git")
    }
}

/// A locked version's files, opened with its locked features
pub fn open_locked(storage: Arc<dyn Storage>, locked: &LockedPackage) -> Result<Package> {
    let features: Vec<&str> = locked.features.iter().map(String::as_str).collect();
    Package::open(
        storage,
        Features {
            features: &features,
            default: locked.default_features,
        },
    )
}

/// The folder in the library a locked version goes in (`packages/<id>/<key>`)
pub fn library_dir(locked: &LockedPackage) -> String {
    format!("packages/{}/{}", locked.id, locked.key())
}

/// A [`Library`] from a project's folder: what [`Packages`] needs to resolve
pub struct LibraryPackages<'a> {
    library: &'a Library,
    base: &'a Path,
}

impl Packages for LibraryPackages<'_> {
    fn installed(&self, locked: &LockedPackage) -> Result<Option<Arc<dyn Storage>>> {
        Ok(self
            .library
            .is_installed(locked, self.base)
            .then(|| -> Arc<dyn Storage> {
                Arc::new(FsStorage::new(self.library.package_dir(locked, self.base)))
            }))
    }

    fn path_package(&self, relative: &str) -> Result<Arc<dyn Storage>> {
        Ok(Arc::new(FsStorage::new(self.base.join(relative))))
    }

    /// Its own files rewritten in place, and ones it doesn't have removed (a book added to its
    /// `usx/`); other files (annotations) stay
    fn repair(&self, locked: &LockedPackage, files: &dyn Storage) -> Result<()> {
        let dir = FsStorage::new(self.library.package_dir(locked, self.base));
        let wanted = files.walk("")?;
        for path in crate::package::declared_files(&dir)? {
            if !wanted.contains(&path) {
                let full = self.library.package_dir(locked, self.base).join(&path);
                std::fs::remove_file(&full)
                    .map_err(|e| LogoxideError::io(&full.display().to_string(), e))?;
            }
        }
        for path in wanted {
            dir.write(&path, &files.read(&path)?)?;
        }
        Ok(())
    }

    /// Written beside its folder, then moved into place, so a half-written version is never
    /// taken for an installed one
    fn install(&self, locked: &LockedPackage, files: &dyn Storage) -> Result<()> {
        let dir = self.library.package_dir(locked, self.base);
        let staging =
            dir.with_file_name(format!(".{}.staging-{}", locked.key(), std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        let result = (|| {
            let to = FsStorage::new(&staging);
            for path in files.walk("")? {
                to.write(&path, &files.read(&path)?)?;
            }
            std::fs::rename(&staging, &dir)
                .map_err(|e| LogoxideError::io(&dir.display().to_string(), e))
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result
    }
}
