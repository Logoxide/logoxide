/*!
Cleaning a library: the installed versions no project uses (none of those given locks it, and no
root, a project elsewhere that installed from this library, keeps it), and what installs that
didn't finish left
*/

use std::path::PathBuf;

use logoxide::{FsStorage, Library, LogoxideError, Project, Result};

/// How old a staging folder (a version an install was writing) must be before it's taken for a
/// dead install's
pub const STALE_STAGING: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// What cleaning found or did, said as it goes
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Cleaning {
    /// A folder an install didn't finish (it died more than a day ago), removed or not
    Staging { path: PathBuf, removed: bool },
    /// A root whose project is gone from this machine: its versions stay, unless it was
    /// forgotten
    RootGone {
        path: String,
        versions: usize,
        forgotten: bool,
    },
    /// An unused version kept: files in it aren't the package's (a reader's annotations)
    Kept {
        id: String,
        key: String,
        others: Vec<String>,
    },
    /// An unused version kept because its `logoxide.toml` doesn't read (an older lx's), so its
    /// files can't be told from a reader's; deleting it is left to the reader
    Unreadable {
        id: String,
        key: String,
        dir: PathBuf,
        error: String,
    },
    /// An unused version removed (with its parsed books)
    Removed { id: String, key: String },
    /// An unused version, left (without `remove`)
    Unused {
        id: String,
        key: String,
        dir: PathBuf,
    },
}

/// What cleaning came to
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cleaned {
    /// How many installed versions nothing uses
    pub unused: usize,
}

/**
Finds (and with `remove`, deletes) the library's versions that none of `projects` locks and no
root keeps, holding the library's lock throughout; with `prune_roots`, roots whose project is gone
are forgotten. Each step is given to `said` as it happens
*/
pub fn clean(
    library: &Library,
    projects: &[Project],
    remove: bool,
    prune_roots: bool,
    said: &mut dyn FnMut(Cleaning),
) -> Result<Cleaned> {
    // No install may write while versions are judged and removed
    let _lock = library.lock()?;
    // A version an install never finished (it died more than a day ago: a live one holds the
    // lock, and a day is longer than any install)
    for staging in library.stale_staging(STALE_STAGING) {
        if remove {
            std::fs::remove_dir_all(&staging)
                .map_err(|e| LogoxideError::io(&staging.display().to_string(), e))?;
        }
        said(Cleaning::Staging {
            path: staging,
            removed: remove,
        });
    }
    let mut used: Vec<(String, String)> = (projects.iter())
        .filter_map(|p| p.lock())
        .flat_map(|lock| lock.packages.iter())
        .filter(|p| p.path.is_none())
        .map(|p| (p.id.clone(), p.key()))
        .collect();
    // Every project that installed from this library (its roots) keeps what it locked, so a
    // vault elsewhere, or on another machine sharing the library, isn't cleaned out from here.
    // One whose folder is gone from this machine is said, and forgotten only when asked
    let mut roots = library.roots()?;
    for root in roots.iter().filter(|r| r.is_gone()) {
        let forgotten = match (prune_roots, &root.file) {
            (true, Some(file)) => {
                std::fs::remove_file(file)
                    .map_err(|e| LogoxideError::io(&file.display().to_string(), e))?;
                true
            }
            _ => false,
        };
        said(Cleaning::RootGone {
            path: root.path.clone(),
            versions: root.packages.len(),
            forgotten,
        });
    }
    if prune_roots {
        roots.retain(|r| !r.is_gone());
    }
    used.extend(roots.iter().flat_map(|r| r.package_keys()));
    let unused: Vec<(String, String)> = (library.installed().into_iter())
        .filter(|v| !used.contains(v))
        .collect();
    for (id, key) in &unused {
        let dir = library.root().join("packages").join(id).join(key);
        // Files that aren't the package's (a reader's annotations) are never removed: the folder
        // stays, and says why
        let others = match logoxide::package::other_files(&FsStorage::new(&dir)) {
            Ok(others) => others,
            // A manifest this lx can't read stops this version, not the clean
            Err(e @ LogoxideError::Manifest(_)) => {
                said(Cleaning::Unreadable {
                    id: id.clone(),
                    key: key.clone(),
                    dir,
                    error: e.to_string(),
                });
                continue;
            }
            Err(e) => return Err(e),
        };
        if !others.is_empty() {
            said(Cleaning::Kept {
                id: id.clone(),
                key: key.clone(),
                others,
            });
            continue;
        }
        if !remove {
            said(Cleaning::Unused {
                id: id.clone(),
                key: key.clone(),
                dir,
            });
            continue;
        }
        // Its parsed books are `<id>/<version>-<checksum>`: by the files' checksum, since forks
        // share versions
        let checksum = logoxide::package::checksum(&FsStorage::new(&dir))?;
        let short = checksum
            .trim_start_matches("sha256:")
            .get(..16)
            .unwrap_or_default();
        let version = key.split('+').next().unwrap_or(key);
        std::fs::remove_dir_all(&dir)
            .map_err(|e| LogoxideError::io(&dir.display().to_string(), e))?;
        let cache = library.cache_dir().join("books").join(id);
        let _ = std::fs::remove_dir_all(cache.join(format!("{version}-{short}")));
        // The id's folder too, once it has no versions (remove_dir only removes empty ones)
        let _ = std::fs::remove_dir(library.root().join("packages").join(id));
        said(Cleaning::Removed {
            id: id.clone(),
            key: key.clone(),
        });
    }
    Ok(Cleaned {
        unused: unused.len(),
    })
}

/// An installed version, and the projects that use it
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Installed {
    pub id: String,
    /// Its folder's name, `<version>+g<commit>` (or `+a`, `+i` for registries and imports)
    pub key: String,
    pub dir: PathBuf,
    /// Its own `logoxide.toml`'s `[package]`, when that reads
    pub package: Option<logoxide::manifest::Package>,
    /// Each project that locks it: those given, and every root the library records
    pub used_by: Vec<User>,
}

impl Installed {
    /// The version, without the folder's `+` part
    pub fn version(&self) -> &str {
        self.key.split('+').next().unwrap_or(&self.key)
    }
}

/// A project that uses an installed version
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct User {
    /// Its folder, as its host names it
    pub path: String,
    pub host: String,
    /// On this host, and its `logoxide.toml` gone (`lx clean --prune-roots` forgets it)
    pub gone: bool,
}

/**
Every version in the library, each with the projects that lock it: `projects` (by their lock
files) and the library's roots (projects elsewhere, or on other hosts sharing it), so a version
with none is one [`clean`] would remove. Path packages aren't in the library, so aren't listed.
Takes no lock: it only reads
*/
pub fn installed(library: &Library, projects: &[Project]) -> Result<Vec<Installed>> {
    let mut users: Vec<(User, Vec<(String, String)>)> = vec![];
    for root in library.roots()? {
        let user = User {
            path: root.path.clone(),
            host: root.host.clone(),
            gone: root.is_gone(),
        };
        users.push((user, root.package_keys()));
    }
    // A project given whose root isn't written yet (or is older than its lock) counts by its lock
    for project in projects {
        let Some(lock) = project.lock() else {
            continue;
        };
        let root = logoxide::roots::Root::of(project.dir(), lock);
        let keys = root.package_keys();
        match (users.iter_mut()).find(|(u, _)| u.host == root.host && u.path == root.path) {
            Some((_, known)) => known.extend(keys),
            None => users.push((
                User {
                    path: root.path,
                    host: root.host,
                    gone: false,
                },
                keys,
            )),
        }
    }
    let mut found = vec![];
    for (id, key) in library.installed() {
        let dir = library.root().join("packages").join(&id).join(&key);
        let package = std::fs::read_to_string(dir.join(logoxide::manifest::MANIFEST))
            .ok()
            .and_then(|text| logoxide::Manifest::parse(&text).ok())
            .and_then(|m| m.package);
        let mut used_by: Vec<User> = (users.iter())
            .filter(|(_, keys)| keys.iter().any(|(i, k)| *i == id && *k == key))
            .map(|(u, _)| u.clone())
            .collect();
        used_by.sort_by(|a, b| (&a.host, &a.path).cmp(&(&b.host, &b.path)));
        found.push(Installed {
            id,
            key,
            dir,
            package,
            used_by,
        });
    }
    Ok(found)
}
