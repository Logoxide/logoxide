/*!
Installing without `git` or file access: the host downloads and reads, biblix resolves.

Resolving is the same as `bx install` ([`Resolver`]), run against what the host has given so far.
When it needs something it doesn't have (a repository's refs, a commit's archive, or an installed
package's files), it stops and asks for it; the host fetches it and asks again. Resolving is cheap
and the host is asked for each thing once, so a project with a few packages takes a few rounds.
*/

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use biblix_core::{
    LockedPackage, MemoryStorage, Storage,
    archive::{self, Refs},
    git::{Fetcher, GitRef},
    library::library_dir,
    manifest::{MANIFEST, Manifest},
    resolve::{Packages, Resolver, Update, tag_version},
};
use boltffi::*;

use crate::{BiblixError, BiblixProject, PackageFile, PackageInfo, PackageLocation};

/// Which locked packages may move to newer versions (like `bx update`)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateMode {
    /// None: keep what the lock file says where the manifest still allows it (`bx install`)
    Nothing,
    All,
    Only {
        ids: Vec<String>,
    },
    /// Like `Nothing`, but installed versions whose files changed (a reader edited them) are
    /// fetched again and their files handed back to write over them (`bx install --repair`)
    Repair,
}

/// Something biblix needs from the host before it can go on
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchRequest {
    /// GET `url` (the repository's refs) and give the answer to `provideRefs(repository, bytes)`
    Refs { repository: String, url: String },
    /// GET the first of `urls` that answers (the commit as a `.tar.gz`) and give it to
    /// `provideArchive(repository, commit, bytes)`
    Archive {
        repository: String,
        commit: String,
        urls: Vec<String>,
    },
    /// Read every file under the folder (in the library, or the project's for a path package)
    /// and give them to `provideFiles(location, files)`, or `provideMissing(location)` if it
    /// isn't there
    Files { location: PackageLocation },
}

/// What `install` did: it needs something, or it's done
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallStep {
    Fetch {
        request: FetchRequest,
    },
    /// Write `files` into the library (paths relative to its folder, under each new version's
    /// `packages/<id>/<version>/`), then `lock` as the project's `biblix.lock`
    Done {
        lock: String,
        files: Vec<PackageFile>,
    },
}

/// A repository's package before it's added: what it is, and how to ask for it
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspected {
    pub info: PackageInfo,
    /// The tag it was read from (the newest release, unless another reference was given)
    pub tag: Option<String>,
    /// The commit it was read from
    pub commit: String,
}

/// What `inspect` did: it needs something, or it found the package
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
// Copied across the boundary whole either way, so not boxed
#[allow(clippy::large_enum_variant)]
pub enum InspectStep {
    Fetch { request: FetchRequest },
    Found { inspected: Inspected },
}

/// Everything the host has given, kept between rounds
#[derive(Default)]
struct Given {
    refs: HashMap<String, Refs>,
    archives: HashMap<(String, String), Arc<MemoryStorage>>,
    files: HashMap<PackageLocation, Option<Arc<MemoryStorage>>>,
    /// Versions installed by this installer, by their folder in the library
    installed: BTreeMap<String, Arc<MemoryStorage>>,
}

/// One round: the resolver's fetcher and packages, over what's been given. What's missing is
/// asked for, and resolving stops there (with an error the installer replaces by the request)
struct Round<'a> {
    given: RefCell<&'a mut Given>,
    need: RefCell<Option<FetchRequest>>,
}

impl Round<'_> {
    fn ask(&self, request: FetchRequest) -> biblix_core::BiblixError {
        let message = format!("needs {request:?}");
        self.need.borrow_mut().get_or_insert(request);
        biblix_core::BiblixError::Fetch {
            url: String::new(),
            message,
        }
    }

    fn refs(&self, url: &str) -> biblix_core::Result<Refs> {
        let refs = self.given.borrow().refs.get(url).cloned();
        refs.ok_or_else(|| {
            self.ask(FetchRequest::Refs {
                repository: url.to_string(),
                url: archive::refs_url(url),
            })
        })
    }

    /// A repository's files at a reference, and its commit
    fn files_at(
        &self,
        url: &str,
        reference: &GitRef,
    ) -> biblix_core::Result<(Arc<MemoryStorage>, String)> {
        // A full commit from a lock file needs no refs
        let commit = match reference {
            GitRef::Rev(rev) if rev.len() == 40 => rev.clone(),
            _ => self.refs(url)?.commit(reference).ok_or_else(|| {
                biblix_core::BiblixError::Fetch {
                    url: url.to_string(),
                    message: format!("no {reference:?}"),
                }
            })?,
        };
        let key = (url.to_string(), commit.clone());
        let files = self.given.borrow().archives.get(&key).cloned();
        let files = files.ok_or_else(|| {
            self.ask(FetchRequest::Archive {
                repository: url.to_string(),
                urls: archive::archive_urls(url, &commit),
                commit: commit.clone(),
            })
        })?;
        Ok((files, commit))
    }

    fn files(&self, location: PackageLocation) -> biblix_core::Result<Option<Arc<dyn Storage>>> {
        let given = self.given.borrow();
        if let PackageLocation::Library { dir } = &location
            && let Some(files) = given.installed.get(dir)
        {
            return Ok(Some(files.clone()));
        }
        match given.files.get(&location) {
            Some(files) => Ok(files.clone().map(|f| -> Arc<dyn Storage> { f })),
            None => {
                drop(given);
                Err(self.ask(FetchRequest::Files { location }))
            }
        }
    }
}

impl Fetcher for Round<'_> {
    fn tags(&self, url: &str) -> biblix_core::Result<Vec<(String, String)>> {
        Ok(self.refs(url)?.tags)
    }

    fn fetch(
        &self,
        url: &str,
        reference: &GitRef,
        dest: &dyn Storage,
    ) -> biblix_core::Result<String> {
        let (files, commit) = self.files_at(url, reference)?;
        for path in files.walk("")? {
            dest.write(&path, &files.read(&path)?)?;
        }
        Ok(commit)
    }
}

impl Packages for Round<'_> {
    fn installed(&self, locked: &LockedPackage) -> biblix_core::Result<Option<Arc<dyn Storage>>> {
        self.files(PackageLocation::of(locked))
    }

    fn path_package(&self, relative: &str) -> biblix_core::Result<Arc<dyn Storage>> {
        let location = PackageLocation::Path {
            dir: relative.to_string(),
        };
        self.files(location)?
            .ok_or_else(|| biblix_core::BiblixError::NotFound(relative.to_string()))
    }

    fn install(&self, locked: &LockedPackage, files: &dyn Storage) -> biblix_core::Result<()> {
        let copy = MemoryStorage::new();
        for path in files.walk("")? {
            copy.write(&path, &files.read(&path)?)?;
        }
        (self.given.borrow_mut().installed).insert(library_dir(locked), Arc::new(copy));
        Ok(())
    }

    /// The same as installing: its files go back to the host, which writes them over the
    /// changed ones (its other files, a reader's annotations, are the host's to keep)
    fn repair(&self, locked: &LockedPackage, files: &dyn Storage) -> biblix_core::Result<()> {
        self.install(locked, files)
    }
}

/// What a round ended with: its result, or what it asked for
fn finish<T>(
    round: Round<'_>,
    result: biblix_core::Result<T>,
) -> Result<Result<T, FetchRequest>, BiblixError> {
    if let Some(request) = round.need.into_inner() {
        return Ok(Err(request));
    }
    Ok(Ok(result?))
}

fn files_of(storage: &dyn Storage) -> Result<Vec<PackageFile>, BiblixError> {
    let mut files = vec![];
    for path in storage.walk("")? {
        files.push(PackageFile {
            bytes: storage.read(&path)?,
            path,
        });
    }
    Ok(files)
}

/**
Installs a project's packages, and inspects repositories before they're added, with the host
doing the downloading and reading. Keep one while installing: what's given is kept

```ts
const installer = biblix.installer();
let step = installer.install(project, UpdateMode.Nothing);
while (step.tag === "Fetch") {
  const r = step.request;
  if (r.tag === "Refs") installer.provideRefs(r.repository, await get(r.url));
  else if (r.tag === "Archive") installer.provideArchive(r.repository, r.commit, await getFirst(r.urls));
  else if (await exists(r.location)) installer.provideFiles(r.location, await readAll(r.location));
  else installer.provideMissing(r.location);
  step = installer.install(project, UpdateMode.Nothing);
}
for (const file of step.files) await write(`${library}/${file.path}`, file.bytes);
await write("biblix.lock", step.lock);
```
*/
pub struct BiblixInstaller {
    given: Mutex<Given>,
}

impl BiblixInstaller {
    pub(crate) fn new() -> Self {
        Self {
            given: Mutex::default(),
        }
    }

    fn round<T>(
        &self,
        run: impl FnOnce(&Round<'_>) -> biblix_core::Result<T>,
    ) -> Result<Result<T, FetchRequest>, BiblixError> {
        let mut given = self.given.lock().map_err(|_| BiblixError::Files {
            message: String::from("an earlier call panicked"),
        })?;
        let round = Round {
            given: RefCell::new(&mut given),
            need: RefCell::default(),
        };
        let result = run(&round);
        finish(round, result)
    }
}

#[export]
impl BiblixInstaller {
    /// Resolves the project's dependencies (keeping its lock file's versions unless `update`
    /// says otherwise) and installs them, or says what it needs first
    pub fn install(
        &self,
        project: &BiblixProject,
        update: UpdateMode,
    ) -> Result<InstallStep, BiblixError> {
        let repair = update == UpdateMode::Repair;
        let update = match update {
            UpdateMode::Nothing | UpdateMode::Repair => Update::Nothing,
            UpdateMode::All => Update::All,
            UpdateMode::Only { ids } => Update::Only(ids.into_iter().collect()),
        };
        let resolved = self.round(|round| {
            Resolver {
                packages: round,
                fetcher: round,
                previous: project.project.lock(),
                update,
                repair,
            }
            .resolve(project.project.manifest())
        })?;
        let lock = match resolved {
            Ok(lock) => lock,
            Err(request) => return Ok(InstallStep::Fetch { request }),
        };
        let given = self.given.lock().map_err(|_| BiblixError::Files {
            message: String::from("an earlier call panicked"),
        })?;
        let mut files = vec![];
        for (dir, storage) in &given.installed {
            // Only versions the new lock file names (not ones an earlier attempt installed)
            if !lock.packages.iter().any(|p| &library_dir(p) == dir) {
                continue;
            }
            for file in files_of(storage.as_ref())? {
                files.push(PackageFile {
                    path: format!("{dir}/{}", file.path),
                    bytes: file.bytes,
                });
            }
        }
        Ok(InstallStep::Done {
            lock: lock.to_toml()?,
            files,
        })
    }

    /// A repository's package, read before it's added (its id names the dependency): at `tag`,
    /// `branch`, or `rev` if one is given, else its newest release, else its default branch
    pub fn inspect(
        &self,
        url: String,
        tag: Option<String>,
        branch: Option<String>,
        rev: Option<String>,
    ) -> Result<InspectStep, BiblixError> {
        let found = self.round(|round| {
            let reference = match (tag, branch, rev) {
                (Some(tag), _, _) => GitRef::Tag(tag),
                (_, Some(branch), _) => GitRef::Branch(branch),
                (_, _, Some(rev)) => GitRef::Rev(rev),
                _ => {
                    let refs = round.refs(&url)?;
                    let newest = (refs.tags.iter())
                        .filter_map(|(t, _)| Some((tag_version(t)?, t)))
                        .filter(|(v, _)| v.pre.is_empty())
                        .max();
                    match newest {
                        Some((_, tag)) => GitRef::Tag(tag.clone()),
                        None => GitRef::Head,
                    }
                }
            };
            let (files, commit) = round.files_at(&url, &reference)?;
            let manifest = Manifest::parse(&files.read_string(MANIFEST)?)?;
            let info = manifest.package.ok_or_else(|| {
                biblix_core::BiblixError::Manifest(format!("{url}: no [package]"))
            })?;
            let tag = match reference {
                GitRef::Tag(tag) => Some(tag),
                _ => None,
            };
            Ok(Inspected {
                info: PackageInfo::from(&info),
                tag,
                commit,
            })
        })?;
        Ok(match found {
            Ok(inspected) => InspectStep::Found { inspected },
            Err(request) => InspectStep::Fetch { request },
        })
    }

    /// The answer to a `Refs` request
    pub fn provide_refs(&self, repository: String, bytes: Vec<u8>) -> Result<(), BiblixError> {
        let refs = Refs::parse(&bytes).map_err(|e| BiblixError::Files {
            message: format!("{repository}: {e}"),
        })?;
        if let Ok(mut given) = self.given.lock() {
            given.refs.insert(repository, refs);
        }
        Ok(())
    }

    /// The answer to an `Archive` request: the commit as a `.tar.gz`
    pub fn provide_archive(
        &self,
        repository: String,
        commit: String,
        bytes: Vec<u8>,
    ) -> Result<(), BiblixError> {
        let files = MemoryStorage::new();
        archive::unpack(&bytes, &files).map_err(|e| BiblixError::Files {
            message: format!("{repository} at {commit}: {e}"),
        })?;
        if let Ok(mut given) = self.given.lock() {
            given.archives.insert((repository, commit), Arc::new(files));
        }
        Ok(())
    }

    /// The answer to a `Files` request: every file under the folder (paths relative to it)
    pub fn provide_files(&self, location: PackageLocation, files: Vec<PackageFile>) {
        let storage = MemoryStorage::with(files.into_iter().map(|f| (f.path, f.bytes)));
        if let Ok(mut given) = self.given.lock() {
            given.files.insert(location, Some(Arc::new(storage)));
        }
    }

    /// The answer to a `Files` request when the folder isn't there
    pub fn provide_missing(&self, location: PackageLocation) {
        if let Ok(mut given) = self.given.lock() {
            given.files.insert(location, None);
        }
    }
}
