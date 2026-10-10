/*!
The library: installed package versions, side by side, as plain files.

```text
<library>/                        LOGOXIDE_HOME, else config.toml's `library`, else $XDG_DATA_HOME/logoxide (any folder: inside
  logoxide.toml                     .obsidian/ to sync with a vault); this machine's own
  logoxide.lock                     requirements and their lock, for apps without a vault
  packages/<id>/<version>/        each version, without .git, so it syncs well
  packages/<id>/<version>+g<commit>/   from a branch or a revision rather than its release
  packages/<id>/.<key>.staging-<pid>-<n>-<random>/   a version being written (renamed into place when whole)
  roots/<hash>.toml               each project that installed from here and what it locks, so `lx clean` keeps them (roots.rs)
<cache>/                          per device, never synced: LOGOXIDE_CACHE, else config.toml's `cache`, else $XDG_CACHE_HOME/logoxide
  books/                          parsed books and verse counts (see cache.rs)
  git/                            scratch space while fetching
  stamps/<id>/<version>-<place>.postcard   each package's checksum by its files' sizes and times (stamps.rs)
  locks/                          the library's and projects' lock files (locking.rs), unless $XDG_RUNTIME_DIR is set
```
*/

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    cache::Cache,
    error::{LogoxideError, Result},
    lock::{LockedPackage, Lockfile},
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

pub(crate) fn env_dir(var: &str) -> Option<PathBuf> {
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

    /// The cache of parsed books, remembering each package's checksum by its files' stamps
    /// (`<cache>/stamps/`, `stamps.rs`)
    pub fn book_cache(&self) -> Cache {
        Cache::new(Arc::new(FsStorage::new(self.cache.join("books"))))
            .with_stamps(Arc::new(FsStorage::new(self.cache.join("stamps"))))
    }

    /// Scratch space for fetching
    pub fn git_dir(&self) -> PathBuf {
        self.cache.join("git")
    }

    /// Where the projects that use this library are recorded ([`crate::roots`])
    pub fn roots_dir(&self) -> PathBuf {
        self.root.join(crate::roots::ROOTS_DIR)
    }

    /// Records a project as one that uses this library, with what its lock holds here, so
    /// [`Self::roots`] keeps them from a clean run elsewhere (written on every install)
    pub fn record_root(&self, project: &Path, lock: &Lockfile) -> Result<()> {
        crate::roots::write_root(&self.roots_dir(), &crate::roots::Root::of(project, lock))
    }

    /// Every project recorded as using this library
    pub fn roots(&self) -> Result<Vec<crate::roots::Root>> {
        crate::roots::read_roots(&self.roots_dir())
    }

    /// The library's lock ([`crate::locking`]), held while versions are installed or removed
    pub fn lock(&self) -> Result<crate::locking::Guard> {
        crate::locking::lock(&self.root, "the library at")
    }

    /**
    Staging folders (a version being written, `packages/<id>/.<key>.staging-…`) older than
    `max_age`: left by an install that died. Remove them only under the library's [`lock`](Self::lock)
    (a live install's can't be told from a dead one's otherwise) and only when old enough that no
    install could still be writing them (`lx clean` takes a day)
    */
    pub fn stale_staging(&self, max_age: std::time::Duration) -> Vec<PathBuf> {
        let mut found = vec![];
        let Ok(ids) = std::fs::read_dir(self.root.join("packages")) else {
            return found;
        };
        let now = std::time::SystemTime::now();
        for id in ids.flatten() {
            let Ok(entries) = std::fs::read_dir(id.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let staging = name.starts_with('.') && name.contains(".staging-");
                let old = (entry.metadata().and_then(|m| m.modified()).ok())
                    .and_then(|modified| now.duration_since(modified).ok())
                    .is_some_and(|age| age >= max_age);
                if staging && old && entry.path().is_dir() {
                    found.push(entry.path());
                }
            }
        }
        found.sort();
        found
    }
}

/// A name for a version's staging folder, beside its place and unlike any other process's or
/// run's: `.<key>.staging-<pid>-<n>-<random>`
fn staging_dir(dir: &Path, key: &str) -> PathBuf {
    use std::{
        hash::{BuildHasher, RandomState},
        sync::atomic::{AtomicU64, Ordering},
    };
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    // RandomState is seeded at random for each process, so hashing anything with a fresh one gives
    // a number another run won't
    let random = RandomState::new().hash_one(n) & 0xffff_ffff;
    dir.with_file_name(format!(
        ".{key}.staging-{}-{n}-{random:08x}",
        std::process::id()
    ))
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

    /// Its own files rewritten in place (each atomically, so a reader never sees half a book),
    /// then ones it doesn't have removed (a book added to its `usx/`), last, so a repair that
    /// stops partway has taken nothing away; other files (annotations) stay
    fn repair(&self, locked: &LockedPackage, files: &dyn Storage) -> Result<()> {
        let dir = FsStorage::new(self.library.package_dir(locked, self.base));
        let declared = crate::package::declared_files(&dir)?;
        let wanted = files.walk("")?;
        for path in &wanted {
            dir.write(path, &files.read(path)?)?;
        }
        for path in declared {
            if !wanted.contains(&path) {
                dir.remove(&path)?;
            }
        }
        Ok(())
    }

    /// Written beside its folder, then moved into place, so a half-written version is never
    /// taken for an installed one
    fn install(&self, locked: &LockedPackage, files: &dyn Storage) -> Result<()> {
        let dir = self.library.package_dir(locked, self.base);
        let staging = staging_dir(&dir, &locked.key());
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
