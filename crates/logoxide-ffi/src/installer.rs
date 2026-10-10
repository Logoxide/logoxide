/*!
Installing without `git` or file access: the host downloads and reads, logoxide resolves.

Resolving is the same as `lx install` ([`Resolver`]), run against what the host has given so far.
When it needs something it doesn't have (a repository's refs, a commit's archive, or an installed
package's files), it stops and asks for it; the host fetches it and asks again. Resolving is cheap
and the host is asked for each thing once, so a project with a few packages takes a few rounds.
*/

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, MutexGuard},
};

use boltffi::*;
use logoxide_core::{
    LockedPackage, MemoryStorage, Storage,
    archive::{self, Refs},
    git::{Fetcher, GitRef},
    library::library_dir,
    manifest::{MANIFEST, Manifest},
    resolve::{Frozen, Packages, Resolver, Update, tag_version},
};

use crate::{
    LogoxideCatalog, LogoxideError, LogoxideProject, PackageFile, PackageInfo, PackageLocation,
};

/// Which locked packages may move to newer versions (like `lx update`)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateMode {
    /// None: keep what the lock file says where the manifest still allows it (`lx install`)
    Nothing,
    All,
    Only {
        ids: Vec<String>,
    },
    /// Like `Nothing`, but installed versions whose files changed (a reader edited them) are
    /// fetched again and their files handed back to write over them (`lx install --repair`)
    Repair,
}

/// Something logoxide needs from the host before it can go on
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
    /// GET the first of `urls` that answers (a registry's catalog, or a release's archive:
    /// its URL, then its mirrors) and give it to `provideDownload(url, bytes)`; `url` is the
    /// first of them
    Download { url: String, urls: Vec<String> },
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
    /// `packages/<id>/<version>/`), then `lock` as the project's `logoxide.lock`
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
    downloads: HashMap<String, Vec<u8>>,
    /// Downloads the host couldn't make, and why (a catalog that's down is a warning)
    failed: HashMap<String, String>,
    /// The catalogs `load_catalogs` last finished with, merged
    catalog: Option<logoxide_core::catalog::Catalog>,
    /// Versions installed by this installer, by their folder in the library
    installed: BTreeMap<String, Arc<MemoryStorage>>,
}

/// One round: the resolver's fetcher and packages, over what's been given. What's missing is
/// asked for, and resolving stops there (with an error the installer replaces by the request).
/// Behind mutexes, so it's a `Fetcher` (`Send + Sync`); a round runs on one thread
struct Round<'a> {
    given: Mutex<&'a mut Given>,
    need: Mutex<Option<FetchRequest>>,
}

/// A lock that a panic elsewhere doesn't spoil (what's given is only ever added to)
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl<'a> Round<'a> {
    fn given(&self) -> MutexGuard<'_, &'a mut Given> {
        locked(&self.given)
    }

    fn ask(&self, request: FetchRequest) -> logoxide_core::LogoxideError {
        let message = format!("needs {request:?}");
        locked(&self.need).get_or_insert(request);
        logoxide_core::LogoxideError::network("", message)
    }

    /// A download the host has given, else asked for (`urls`: where to try, `url` first)
    fn given_download(&self, url: &str, urls: &[String]) -> logoxide_core::Result<Vec<u8>> {
        let given = self.given();
        if let Some(bytes) = given.downloads.get(url) {
            return Ok(bytes.clone());
        }
        if let Some(message) = given.failed.get(url) {
            return Err(logoxide_core::LogoxideError::network(url, message.clone()));
        }
        drop(given);
        Err(self.ask(FetchRequest::Download {
            url: url.to_string(),
            urls: urls.to_vec(),
        }))
    }

    fn refs(&self, url: &str) -> logoxide_core::Result<Refs> {
        let refs = self.given().refs.get(url).cloned();
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
    ) -> logoxide_core::Result<(Arc<MemoryStorage>, String)> {
        // A full commit from a lock file needs no refs
        let commit = match reference {
            GitRef::Rev(rev) if rev.len() == 40 => rev.clone(),
            _ => self
                .refs(url)?
                .find_commit(reference)
                .map_err(|e| match e {
                    logoxide_core::LogoxideError::Git { message, .. } => {
                        logoxide_core::LogoxideError::git(url, message)
                    }
                    e => e,
                })?,
        };
        let key = (url.to_string(), commit.clone());
        let files = self.given().archives.get(&key).cloned();
        let files = files.ok_or_else(|| {
            self.ask(FetchRequest::Archive {
                repository: url.to_string(),
                urls: archive::archive_urls(url, &commit),
                commit: commit.clone(),
            })
        })?;
        Ok((files, commit))
    }

    fn files(&self, location: PackageLocation) -> logoxide_core::Result<Option<Arc<dyn Storage>>> {
        let given = self.given();
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
    fn tags(&self, url: &str) -> logoxide_core::Result<Vec<(String, String)>> {
        Ok(self.refs(url)?.tags)
    }

    fn download(&self, url: &str) -> logoxide_core::Result<Vec<u8>> {
        self.given_download(url, &[url.to_string()])
    }

    /// One request for every place: the host tries them in turn and gives what answered as
    /// the first's
    fn download_any(
        &self,
        urls: &[String],
        check: &dyn Fn(&str, &[u8]) -> logoxide_core::Result<()>,
    ) -> logoxide_core::Result<(String, Vec<u8>)> {
        let url = urls.first().cloned().unwrap_or_default();
        let bytes = self.given_download(&url, urls)?;
        check(&url, &bytes)?;
        Ok((url, bytes))
    }

    fn fetch(
        &self,
        url: &str,
        reference: &GitRef,
        dest: &dyn Storage,
    ) -> logoxide_core::Result<String> {
        let (files, commit) = self.files_at(url, reference)?;
        for path in files.walk("")? {
            dest.write(&path, &files.read(&path)?)?;
        }
        Ok(commit)
    }
}

impl Packages for Round<'_> {
    fn installed(&self, locked: &LockedPackage) -> logoxide_core::Result<Option<Arc<dyn Storage>>> {
        self.files(PackageLocation::of(locked))
    }

    fn path_package(&self, relative: &str) -> logoxide_core::Result<Arc<dyn Storage>> {
        let location = PackageLocation::Path {
            dir: relative.to_string(),
        };
        self.files(location)?
            .ok_or_else(|| logoxide_core::LogoxideError::NotFound(relative.to_string()))
    }

    fn install(&self, locked: &LockedPackage, files: &dyn Storage) -> logoxide_core::Result<()> {
        let copy = MemoryStorage::new();
        for path in files.walk("")? {
            copy.write(&path, &files.read(&path)?)?;
        }
        (self.given().installed).insert(library_dir(locked), Arc::new(copy));
        Ok(())
    }

    /// The same as installing: its files go back to the host, which writes them over the
    /// changed ones (its other files, a reader's annotations, are the host's to keep)
    fn repair(&self, locked: &LockedPackage, files: &dyn Storage) -> logoxide_core::Result<()> {
        self.install(locked, files)
    }
}

/**
logoxide-app's [`Http`](logoxide_app::http::Http) over what the host has given: a URL it has
given (`provideDownload`) is read, one it couldn't fetch (`provideDownloadFailed`) is that
error, and any other is asked for (a `Download` request), stopping the round, so
`logoxide_app::catalogs` reads catalogs here as it does in lx
*/
struct HostHttp<'r, 'a> {
    round: &'r Round<'a>,
}

impl logoxide_app::http::Http for HostHttp<'_, '_> {
    fn get(&self, url: &str) -> logoxide_core::Result<Vec<u8>> {
        self.round.given_download(url, &[url.to_string()])
    }
}

/// What `loadCatalogs` did: it needs a catalog, or it's read them all (`catalog()` has them)
#[data]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CatalogsStep {
    Fetch {
        request: FetchRequest,
    },
    /// Every catalog that could be read is in `catalog()`; `warnings` say what couldn't be read
    /// and what the catalogs passed over (also in `catalog().warnings()`)
    Done {
        warnings: Vec<String>,
    },
}

/// What a round ended with: its result, or what it asked for
fn finish<T>(
    round: Round<'_>,
    result: logoxide_core::Result<T>,
) -> Result<Result<T, FetchRequest>, LogoxideError> {
    let need = round.need.into_inner().unwrap_or_else(|e| e.into_inner());
    if let Some(request) = need {
        return Ok(Err(request));
    }
    Ok(Ok(result?))
}

fn files_of(storage: &dyn Storage) -> Result<Vec<PackageFile>, LogoxideError> {
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
const installer = logoxide.installer();
let step = installer.install(project, UpdateMode.Nothing);
while (step.tag === "Fetch") {
  const r = step.request;
  if (r.tag === "Refs") installer.provideRefs(r.repository, await get(r.url));
  else if (r.tag === "Archive") installer.provideArchive(r.repository, r.commit, await getFirst(r.urls));
  else if (r.tag === "Download") installer.provideDownload(r.url, await getFirst(r.urls));
  else if (await exists(r.location)) installer.provideFiles(r.location, await readAll(r.location));
  else installer.provideMissing(r.location);
  step = installer.install(project, UpdateMode.Nothing);
}
for (const file of step.files) await write(`${library}/${file.path}`, file.bytes);
await write("logoxide.lock", step.lock);
```
*/
pub struct LogoxideInstaller {
    given: Mutex<Given>,
}

impl LogoxideInstaller {
    pub(crate) fn new() -> Self {
        Self {
            given: Mutex::default(),
        }
    }

    fn round<T>(
        &self,
        run: impl FnOnce(&Round<'_>) -> logoxide_core::Result<T>,
    ) -> Result<Result<T, FetchRequest>, LogoxideError> {
        let mut given = self.given.lock().map_err(|_| LogoxideError::Io {
            message: String::from("an earlier call panicked"),
        })?;
        let round = Round {
            given: Mutex::new(&mut given),
            need: Mutex::default(),
        };
        let result = run(&round);
        finish(round, result)
    }
}

#[export]
impl LogoxideInstaller {
    /// Resolves the project's dependencies (keeping its lock file's versions unless `update`
    /// says otherwise) and installs them, or says what it needs first
    pub fn install(
        &self,
        project: &LogoxideProject,
        update: UpdateMode,
    ) -> Result<InstallStep, LogoxideError> {
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
                frozen: Frozen::default(),
            }
            .resolve(project.project.manifest())
        })?;
        let lock = match resolved {
            Ok(lock) => lock,
            Err(request) => return Ok(InstallStep::Fetch { request }),
        };
        let given = self.given.lock().map_err(|_| LogoxideError::Io {
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
    ) -> Result<InspectStep, LogoxideError> {
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
                logoxide_core::LogoxideError::Manifest(format!("{url}: no [package]"))
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

    /**
    Reads the project's catalogs (its `[settings] catalogs`, then the default ones unless it
    turns them off) as lx does, through logoxide-app's `catalogs::load_catalogs`, asking for
    each one it hasn't been given; then `catalog()` is all of them merged (the first catalog's
    entry for an id). A catalog the host couldn't fetch (`provideDownloadFailed`) is a warning,
    unless none could be read. A catalog given by path isn't read here (the host reads it and
    merges `parseCatalogBytes`'s): a warning says so
    */
    pub fn load_catalogs(&self, project: &LogoxideProject) -> Result<CatalogsStep, LogoxideError> {
        let mut warnings = vec![];
        let (urls, paths): (Vec<String>, Vec<String>) = (project.project.manifest().catalogs())
            .into_iter()
            .partition(|l| l.starts_with("https://") || l.starts_with("http://"));
        for path in paths {
            warnings.push(format!(
                "the catalog `{path}` is a path: read it and give it to parseCatalogBytes"
            ));
        }
        let loaded = self.round(|round| {
            let http = HostHttp { round };
            let mut warn = |w: String| warnings.push(w);
            logoxide_app::catalogs::load_catalogs(&http, &urls, project.project.dir(), &mut warn)
        })?;
        let catalogs = match loaded {
            Ok(catalogs) => catalogs,
            Err(request) => return Ok(CatalogsStep::Fetch { request }),
        };
        let mut merged: Option<logoxide_core::catalog::Catalog> = None;
        for mut catalog in catalogs {
            // Warned of once, below, with what couldn't be read
            catalog.warnings.clear();
            merged = Some(match merged {
                None => catalog,
                Some(merged) => merged.merge(catalog),
            });
        }
        if let Some(merged) = &mut merged {
            merged.warnings = warnings.clone();
        }
        locked(&self.given).catalog = merged;
        Ok(CatalogsStep::Done { warnings })
    }

    /// The catalogs the last `loadCatalogs` read, merged (named for the first). An error before
    /// one has finished
    pub fn catalog(&self) -> Result<LogoxideCatalog, LogoxideError> {
        let catalog = locked(&self.given).catalog.clone();
        let catalog = catalog.ok_or_else(|| LogoxideError::NotFound {
            message: String::from("no catalogs read yet: loadCatalogs first"),
        })?;
        Ok(LogoxideCatalog { catalog })
    }

    /// The answer to a `Download` request the host couldn't fetch from any of its `urls`:
    /// `message` says why. Reading catalogs passes over it with a warning; installing stops
    pub fn provide_download_failed(&self, url: String, message: String) {
        locked(&self.given).failed.insert(url, message);
    }

    /// The answer to a `Download` request
    pub fn provide_download(&self, url: String, bytes: Vec<u8>) {
        if let Ok(mut given) = self.given.lock() {
            given.downloads.insert(url, bytes);
        }
    }

    /// The answer to a `Refs` request
    pub fn provide_refs(&self, repository: String, bytes: Vec<u8>) -> Result<(), LogoxideError> {
        let refs = Refs::parse(&bytes).map_err(|e| LogoxideError::Git {
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
    ) -> Result<(), LogoxideError> {
        let files = MemoryStorage::new();
        archive::unpack(&bytes, &files).map_err(|e| LogoxideError::Archive {
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
