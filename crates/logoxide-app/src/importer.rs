/*!
Imports kept in the library: the resolver asks its [`Fetcher`] for an import's package
([`Fetcher::import`]), and [`Importing`] answers, wrapping any fetcher. It finds the import in
the project's catalogs, downloads what it needs through an [`Http`], and makes the package
([`make`], which the bindings call with the files their host downloaded): a locked import again
from the same downloads, checked by their hashes and given the locked version; a new one as the
publisher has it now, versioned against the one it replaces
*/

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, sync::Mutex};

use logoxide::{
    FsStorage, Library, LogoxideError, MemoryStorage, Result, Storage, TextReader,
    catalog::CatalogEntry,
    git::{Fetcher, GitRef},
    lock::{LockedDownload, LockedPackage},
    manifest::{MANIFEST, Manifest},
    resolve::{ImportBuild, ImportKind, ImportRequest},
};
use logoxide_import::ImportUpdate;

use crate::{catalogs::CatalogFetch, http::Http};

/// A file an import was made from: its name in the import, where it came from, and its bytes
#[derive(Clone, Debug)]
pub struct Downloaded {
    pub name: String,
    pub url: String,
    pub bytes: Vec<u8>,
}

/**
The package `entry`'s import makes from `files`, as `kind` asks: a pinned one given the locked
version (the resolver checks its checksum), a fresh one versioned against `current` (by
[`logoxide_import::compare_import`], read by `reader`: `current`'s own version when nothing
changed). The update, when there was something to compare with and it changed, comes with it
*/
pub fn make(
    entry: &CatalogEntry,
    files: &[Downloaded],
    kind: &ImportKind<'_>,
    reader: &dyn TextReader,
) -> Result<(ImportBuild, Option<ImportUpdate>)> {
    let named: Vec<(String, Vec<u8>)> = (files.iter())
        .map(|f| (f.name.clone(), f.bytes.clone()))
        .collect();
    let made = Arc::new(MemoryStorage::new());
    logoxide_import::build(entry, &named, made.as_ref())?;
    let mut update = None;
    match kind {
        ImportKind::Pinned(locked) => logoxide_import::set_version(made.as_ref(), &locked.version)?,
        ImportKind::Fresh {
            current: Some((_, current)),
        } => update = logoxide_import::compare_import(current.clone(), &made, reader)?,
        ImportKind::Fresh { current: None } => {}
    }
    let downloads = (files.iter())
        .map(|f| LockedDownload {
            name: f.name.clone(),
            url: f.url.clone(),
            sha256: Some(logoxide::publish::sha256(&f.bytes)),
        })
        .collect();
    let build = ImportBuild {
        files: copy(made.as_ref())?,
        downloads,
        upstream_version: entry.version.clone(),
    };
    Ok((build, update))
}

/// Every file of `storage`, in memory
pub(crate) fn copy(storage: &dyn Storage) -> Result<MemoryStorage> {
    let copy = MemoryStorage::new();
    for path in storage.walk("")? {
        copy.write(&path, &storage.read(&path)?)?;
    }
    Ok(copy)
}

/// The largest local bundle read, in all
const LOCAL_LIMIT: u64 = 1 << 30;

/// A URL's bytes: a `file://` one's read where it is, any other downloaded
fn get(http: &dyn Http, url: &str) -> Result<Vec<u8>> {
    let Some(path) = crate::import::local_path(url) else {
        return http.get(url);
    };
    let (dir, name) = match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => (dir, name.to_string_lossy().into_owned()),
        _ => return Err(LogoxideError::Import(format!("{url}: not a file"))),
    };
    FsStorage::new(dir).no_symlinks().read(&name)
}

/// A local folder's files (a bundle's), each named `files/<path>` (a `.git` folder's left out)
fn folder(dir: &std::path::Path, url: &str) -> Result<Vec<Downloaded>> {
    let storage = FsStorage::new(dir).no_symlinks();
    let (mut files, mut total) = (vec![], 0u64);
    for path in storage.walk("")? {
        if path.split('/').any(|part| part == ".git") {
            continue;
        }
        let bytes = storage.read(&path)?;
        total += bytes.len() as u64;
        if total > LOCAL_LIMIT {
            return Err(LogoxideError::Import(format!(
                "{}: more than {LOCAL_LIMIT} bytes",
                dir.display()
            )));
        }
        files.push(Downloaded {
            name: format!("files/{path}"),
            url: url.to_string(),
            bytes,
        });
    }
    Ok(files)
}

/// What `entry`'s import downloads as its publisher has it now: each file from the first of its
/// URLs that answers (a bundle in a local folder: the folder's files)
pub fn download(http: &dyn Http, entry: &CatalogEntry) -> Result<Vec<Downloaded>> {
    let mut files = vec![];
    for download in logoxide_import::downloads(entry)? {
        let local = (download.urls.first())
            .and_then(|url| Some((url, crate::import::local_path(url)?)))
            .filter(|(_, path)| path.is_dir());
        if let Some((url, dir)) = local {
            files.extend(folder(&dir, url)?);
            continue;
        }
        let mut got = Err(LogoxideError::Import(format!(
            "{}: no URL for {}",
            entry.id, download.name
        )));
        for url in &download.urls {
            got = get(http, url).map(|bytes| (url.clone(), bytes));
            if got.is_ok() {
                break;
            }
        }
        let (url, bytes) = got?;
        files.push(Downloaded {
            name: download.name,
            url,
            bytes,
        });
    }
    Ok(files)
}

/**
The files a locked import was made from, downloaded again: each from where it came from, then
the entry's other places for it, the first whose bytes hash as locked. When none does, the
publisher has changed it since (or no longer has it), which is said with what to do
*/
pub fn download_pinned(
    http: &dyn Http,
    entry: &CatalogEntry,
    name: &str,
    locked: &LockedPackage,
) -> Result<Vec<Downloaded>> {
    let listed = logoxide_import::downloads(entry).unwrap_or_default();
    let mut files = vec![];
    for pinned in &locked.downloads {
        let mut urls = vec![pinned.url.clone()];
        for download in listed.iter().filter(|d| d.name == pinned.name) {
            urls.extend(download.urls.iter().filter(|u| **u != pinned.url).cloned());
        }
        let mut failures = vec![];
        let mut found = None;
        for url in &urls {
            match get(http, url) {
                Ok(bytes)
                    if (pinned.sha256.as_ref())
                        .is_none_or(|s| *s == logoxide::publish::sha256(&bytes)) =>
                {
                    found = Some(Downloaded {
                        name: pinned.name.clone(),
                        url: url.clone(),
                        bytes,
                    });
                    break;
                }
                Ok(bytes) => failures.push(format!(
                    "{url} is another file now ({}, locked {})",
                    logoxide::publish::sha256(&bytes),
                    pinned.sha256.as_deref().unwrap_or_default()
                )),
                Err(e) => failures.push(e.to_string()),
            }
        }
        let file = found.ok_or_else(|| LogoxideError::Mismatch {
            url: pinned.url.clone(),
            message: format!(
                "{} can't be made again as it was locked ({}): the publisher has changed it \
                 since; `lx update {name}` locks what it has now",
                locked.label(),
                failures.join("; ")
            ),
        })?;
        files.push(file);
    }
    Ok(files)
}

/// Where an [`Importing`] finds imports' catalog entries: the project's catalogs
pub struct ImportCatalogs<'a> {
    pub fetch: CatalogFetch<'a>,
    /// The project's catalogs, in order (`Config::catalogs`)
    pub locations: Vec<String>,
    /// The project's folder, which relative catalogs are read from
    pub dir: PathBuf,
}

/**
A [`Fetcher`] that also imports: everything else is `fetcher`'s. Entries the caller already has
([`Importing::give_entry`], an import being added) and packages already made
([`Importing::prepare`], an update planned) are used before anything is looked up or downloaded.
What finding entries warns of is kept for [`Importing::warnings`]
*/
pub struct Importing<'a> {
    fetcher: &'a dyn Fetcher,
    /// Where copies already made are looked for before a fresh import downloads anything
    library: Option<&'a Library>,
    http: &'a dyn Http,
    reader: &'a dyn TextReader,
    catalogs: ImportCatalogs<'a>,
    entries: Mutex<BTreeMap<String, CatalogEntry>>,
    prepared: Mutex<BTreeMap<String, ImportBuild>>,
    warnings: Mutex<Vec<String>>,
}

impl<'a> Importing<'a> {
    pub fn new(
        fetcher: &'a dyn Fetcher,
        http: &'a dyn Http,
        reader: &'a dyn TextReader,
        catalogs: ImportCatalogs<'a>,
    ) -> Self {
        Self {
            fetcher,
            library: None,
            http,
            reader,
            catalogs,
            entries: Mutex::default(),
            prepared: Mutex::default(),
            warnings: Mutex::default(),
        }
    }

    /**
    Looks in `library` before making a fresh import: a copy another project imported, of the
    version of the text the catalog lists now, is used without downloading (only for publishers
    whose versions follow their updates: [`logoxide_import::import_is_current`])
    */
    pub fn with_library(mut self, library: &'a Library) -> Self {
        self.library = Some(library);
        self
    }

    /**
    A copy of `entry`'s import already in the library that's the text the catalog lists now:
    the newest such version whose files still have the checksum its folder names
    */
    fn in_library(&self, entry: &CatalogEntry, import: &str) -> Result<Option<ImportBuild>> {
        let Some(library) = self.library else {
            return Ok(None);
        };
        let mut best = None;
        for (id, folder) in library.installed() {
            let Some((_, short)) = folder.split_once("+i") else {
                continue;
            };
            if id != entry.id {
                continue;
            }
            let dir = library.root().join("packages").join(&id).join(&folder);
            let Ok(text) = std::fs::read_to_string(dir.join(MANIFEST)) else {
                continue;
            };
            let Some(info) = Manifest::parse(&text).ok().and_then(|m| m.package) else {
                continue;
            };
            let Some(upstream) = info.upstream else {
                continue;
            };
            if upstream.import.as_deref() != Some(import)
                || !logoxide_import::import_is_current(entry, upstream.version.as_deref())
                || best.as_ref().is_some_and(|(v, _, _)| *v >= info.version)
            {
                continue;
            }
            let checksum = logoxide::package::checksum(&FsStorage::new(&dir).no_symlinks())?;
            if !checksum.trim_start_matches("sha256:").starts_with(short) {
                continue;
            }
            best = Some((info.version, dir, upstream.version));
        }
        let Some((_, dir, upstream_version)) = best else {
            return Ok(None);
        };
        let files = logoxide::package::only_declared(&FsStorage::new(&dir).no_symlinks())?;
        let downloads = (logoxide_import::downloads(entry)?.into_iter())
            .map(|d| LockedDownload {
                url: d.urls.first().cloned().unwrap_or_default(),
                name: d.name,
                sha256: None,
            })
            .collect();
        Ok(Some(ImportBuild {
            files,
            downloads,
            upstream_version,
        }))
    }

    /// The entry for its import, found already
    pub fn give_entry(&self, entry: &CatalogEntry) {
        if let Some(import) = entry.source.import_name() {
            lock(&self.entries).insert(import.to_string(), entry.clone());
        }
    }

    /// A fresh import made already (an update planned), for the next time `import` asks for it
    pub fn prepare(&self, import: &str, build: ImportBuild) {
        lock(&self.prepared).insert(import.to_string(), build);
    }

    /// What finding imports' entries warned of
    pub fn warnings(&self) -> Vec<String> {
        std::mem::take(&mut *lock(&self.warnings))
    }

    fn entry(&self, import: &str) -> Result<CatalogEntry> {
        if let Some(entry) = lock(&self.entries).get(import) {
            return Ok(entry.clone());
        }
        let ImportCatalogs {
            fetch,
            locations,
            dir,
        } = &self.catalogs;
        let mut said = vec![];
        let entry = crate::import::find_in(*fetch, locations, dir, import, &mut |w| said.push(w));
        lock(&self.warnings).extend(said);
        let entry = entry?;
        lock(&self.entries).insert(import.to_string(), entry.clone());
        Ok(entry)
    }
}

/// A mutex's value, whether or not an earlier holder panicked
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Fetcher for Importing<'_> {
    fn tags(&self, url: &str) -> Result<Vec<(String, String)>> {
        self.fetcher.tags(url)
    }

    fn download(&self, url: &str) -> Result<Vec<u8>> {
        self.fetcher.download(url)
    }

    fn download_any(
        &self,
        urls: &[String],
        check: &dyn Fn(&str, &[u8]) -> Result<()>,
    ) -> Result<(String, Vec<u8>)> {
        self.fetcher.download_any(urls, check)
    }

    fn fetch(&self, url: &str, reference: &GitRef, dest: &dyn Storage) -> Result<String> {
        self.fetcher.fetch(url, reference, dest)
    }

    fn import(&self, request: &ImportRequest<'_>) -> Result<ImportBuild> {
        let import = request.import;
        if let ImportKind::Fresh { .. } = request.kind
            && let Some(build) = lock(&self.prepared).remove(import)
        {
            return Ok(build);
        }
        let entry = self.entry(import)?;
        let files = match &request.kind {
            ImportKind::Pinned(locked) => download_pinned(self.http, &entry, request.name, locked)?,
            ImportKind::Fresh {
                current: Some((locked, files)),
            } if logoxide_import::import_is_current(&entry, locked.upstream_version.as_deref()) => {
                // The catalog lists the version it has: nothing to download
                return Ok(ImportBuild {
                    files: copy(files.as_ref())?,
                    downloads: locked.downloads.clone(),
                    upstream_version: locked.upstream_version.clone(),
                });
            }
            // Another project's copy of the text as the catalog lists it now
            ImportKind::Fresh { .. } => match self.in_library(&entry, import)? {
                Some(build) => return Ok(build),
                None => download(self.http, &entry)?,
            },
        };
        Ok(make(&entry, &files, &request.kind, self.reader)?.0)
    }
}
