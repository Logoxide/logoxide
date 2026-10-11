/*!
Imports: a package made from another publisher's download (eBible.org, fetch.bible, CCEL, a SWORD
repository), added as an `import` dependency and kept in the library like a fetched package
([`run`]; made by [`crate::importer`]), or put in the project's `imports/<id>/` as a path package
([`run_vendored`]); and updating either, each made again and compared with the one the project
has. The publishers' formats are logoxide-import's (`downloads`, `build`, `update`); this strings
them together with the project's catalogs, an [`Http`], and the installer
*/

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use logoxide::{
    Features, FsStorage, Library, LogoxideError, MemoryStorage, Package, Project, Result, Storage,
    TextReader,
    catalog::CatalogEntry,
    config::Config,
    git::Fetcher,
    lock::LockedDownload,
    manifest::{Dependency, DetailedDependency, MANIFEST},
    reader::Skipped,
    resolve::{ImportBuild, Update},
};
use logoxide_import::ImportUpdate;

use crate::{
    Warn,
    catalog_cache::Freshness,
    catalogs::{CatalogFetch, load_catalog, load_catalogs, shadowed},
    http::Http,
    importer::{self, Importing},
};

/// Whether `source` names an import by a prefix this knows (`ebible:<id>`), else an error saying
/// what to give
pub fn check_source(source: &str) -> Result<()> {
    let known = (source.split_once(':'))
        .is_some_and(|(prefix, _)| logoxide_import::PREFIXES.contains(&prefix));
    if known {
        return Ok(());
    }
    let prefixes = (logoxide_import::PREFIXES.map(|p| format!("{p}:<id>"))).join(" or ");
    Err(LogoxideError::Import(format!(
        "{source}: give {prefixes} (see lx search)"
    )))
}

/**
The entry `source` (`ebible:eng-kjv2006`, `sword:crosswire/KJV`) names in the project's catalogs,
the first that lists it; a SWORD repository CrossWire lists (`sword:crosswire-beta/…`) is read
even if the project doesn't search it. Warned of: what the catalogs passed over, and another
catalog listing the same id (whose package would have the same name). When none of the copies
`fetch`'s cache keeps lists it, they're downloaded again (it may be newer than they are)
*/
pub fn find_entry(
    fetch: CatalogFetch,
    config: &Config,
    project: &Project,
    source: &str,
    warn: Warn,
) -> Result<CatalogEntry> {
    let locations = config.catalogs(project.manifest());
    find_in(fetch, &locations, project.dir(), source, warn)
}

/// [`find_entry`] in the catalogs at `locations` (relative ones in `dir`) as `fetch` has them,
/// downloaded again when none of the kept copies lists it
pub(crate) fn find_in(
    fetch: CatalogFetch,
    locations: &[String],
    dir: &Path,
    source: &str,
    warn: Warn,
) -> Result<CatalogEntry> {
    check_source(source)?;
    let not_found = || {
        LogoxideError::Import(format!(
            "{source}: in none of this project's catalogs (lx catalog list)"
        ))
    };
    if fetch.cache.is_some() && fetch.freshness == Freshness::Fresh {
        let mut said = vec![];
        if let Some(entry) = find_in_catalogs(fetch, locations, dir, source, &mut |w| said.push(w))?
        {
            said.into_iter().for_each(&mut *warn);
            return Ok(entry);
        }
        let fetch = fetch.with(Freshness::Refresh);
        return find_in_catalogs(fetch, locations, dir, source, warn)?.ok_or_else(not_found);
    }
    find_in_catalogs(fetch, locations, dir, source, warn)?.ok_or_else(not_found)
}

/// The entry in the catalogs at `locations` as `fetch` has them; none when none lists it
fn find_in_catalogs(
    fetch: CatalogFetch,
    locations: &[String],
    dir: &Path,
    source: &str,
    warn: Warn,
) -> Result<Option<CatalogEntry>> {
    let mut catalogs = load_catalogs(fetch, locations, dir, warn)?;
    if !catalogs.iter().any(|c| c.find_import(source).is_some())
        && let Some(url) = (source.strip_prefix("sword:"))
            .and_then(|rest| rest.split_once('/'))
            .and_then(|(repository, _)| logoxide_import::sword::catalog_url(repository))
    {
        let catalog = load_catalog(fetch, &url, dir)?;
        catalog.warnings.iter().cloned().for_each(&mut *warn);
        catalogs.push(catalog);
    }
    // A CCEL work by its author and name (`ccel:henry/mhc1`) needs no catalog
    if !catalogs.iter().any(|c| c.find_import(source).is_some())
        && let Some(catalog) = logoxide_import::ccel::named_catalog(source)
    {
        catalogs.push(catalog);
    }
    let Some((used, entry)) = (catalogs.iter()).find_map(|c| Some((c, c.find_import(source)?)))
    else {
        return Ok(None);
    };
    if let Some(warning) = shadowed(&catalogs, &entry.id, &used.name) {
        warn(warning);
    }
    Ok(Some(entry.clone()))
}

/// An import made and added
#[derive(Debug)]
pub struct Imported {
    /// The package's id, which names the dependency
    pub id: String,
    pub name: String,
    /// Where it is: its folder in the library, or `<project>/imports/<id>` when vendored
    pub dir: PathBuf,
    /// Its version, as locked
    pub version: String,
    /// What the download had that isn't read as a Bible's books (front matter, a glossary,
    /// books logoxide doesn't know)
    pub skipped: Vec<Skipped>,
}

/// Downloads what a catalog entry's import needs (each file from the first of its URLs that has
/// it), and makes its package in `dest`
pub fn files(http: &dyn Http, entry: &CatalogEntry, dest: &dyn Storage) -> Result<()> {
    let named: Vec<(String, Vec<u8>)> = (importer::download(http, entry)?.into_iter())
        .map(|f| (f.name, f.bytes))
        .collect();
    logoxide_import::build(entry, &named, dest)
}

/// What a package read as text passed over
fn skipped_in(storage: Arc<dyn Storage>) -> Vec<Skipped> {
    match Package::open(storage, Features::default()) {
        Ok(package) => (package.source().skipped().iter())
            .cloned()
            .map(Into::into)
            .collect(),
        Err(_) => vec![],
    }
}

/**
Adds `entry`'s import as a dependency (`{ import = "ebible:eng-kjv2006" }`, named by its id) and
installs it: `importing` makes it, and the library keeps it by its files' checksum, shared with
every project that locks the same files. The manifest is written only once it's installed and
locked
*/
pub fn run(
    project: &mut Project,
    library: &Library,
    entry: &CatalogEntry,
    importing: &Importing<'_>,
) -> Result<Imported> {
    let import = (entry.source.import_name())
        .ok_or_else(|| LogoxideError::Import(format!("{} isn't imported: add it", entry.id)))?;
    if project.manifest().dependencies.contains_key(&entry.id) {
        return Err(LogoxideError::Import(format!(
            "{} is a dependency here already (lx import --update {} makes it again)",
            entry.id, entry.id
        )));
    }
    importing.give_entry(entry);
    let dependency = DetailedDependency {
        import: Some(import.to_string()),
        ..DetailedDependency::default()
    };
    project.add(&entry.id, &dependency)?;
    let only = Update::Only([entry.id.clone()].into());
    let lock = project.install(library, importing, only)?.clone();
    project.save_manifest()?;
    let locked = lock
        .get(&entry.id)
        .ok_or_else(|| LogoxideError::Import(format!("{}: installed, but not locked", entry.id)))?;
    let dir = library.package_dir(locked, project.dir());
    let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir).no_symlinks());
    Ok(Imported {
        id: entry.id.clone(),
        name: entry.name.clone(),
        version: locked.version.to_string(),
        dir,
        skipped: skipped_in(storage),
    })
}

/**
Makes `entry`'s package in the project's `imports/<id>/` and adds it as a path package, installed
and locked: vendored, for a project that carries its texts or a text to edit by hand. It's made
in a folder beside it and moved into place whole, so a download that fails partway leaves nothing
behind; if it can't be added and installed, the project is left as it was: no folder, and the
manifest it had
*/
pub fn run_vendored(
    project: &mut Project,
    library: &Library,
    entry: &CatalogEntry,
    http: &dyn Http,
    fetcher: &dyn Fetcher,
) -> Result<Imported> {
    let relative = format!("imports/{}", entry.id);
    let dir = project.dir().join(&relative);
    if dir.exists() {
        return Err(LogoxideError::Import(format!(
            "{} already exists",
            dir.display()
        )));
    }
    let staging = (project.dir().join("imports")).join(format!(
        ".{}.partial-{}",
        entry.id,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&staging);
    let made = files(http, entry, &FsStorage::new(&staging)).and_then(|()| {
        std::fs::rename(&staging, &dir)
            .map_err(|e| LogoxideError::io(&dir.display().to_string(), e))
    });
    if let Err(error) = made {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    let made: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir).no_symlinks());
    let version = (Package::open(made.clone(), Features::default()))
        .map(|p| p.info().version.to_string())
        .unwrap_or_default();
    let skipped = skipped_in(made);
    let dependency = DetailedDependency {
        path: Some(relative),
        default_features: true,
        ..DetailedDependency::default()
    };
    let manifest_path = project.dir().join(MANIFEST);
    let manifest = manifest_path.is_file().then(|| project.manifest_text());
    let mut add = || -> Result<()> {
        project.add(&entry.id, &dependency)?;
        project.save_manifest()?;
        project.install(library, fetcher, Update::Nothing)?;
        Ok(())
    };
    if let Err(error) = add() {
        let _ = std::fs::remove_dir_all(&dir);
        let _ = match manifest {
            Some(text) => logoxide::storage::write_atomic(&manifest_path, text.as_bytes()),
            None => std::fs::remove_file(&manifest_path),
        };
        return Err(error);
    }
    Ok(Imported {
        id: entry.id.clone(),
        name: entry.name.clone(),
        version,
        dir,
        skipped,
    })
}

/// One import, and what updating it would do
#[derive(Debug)]
pub struct UpdatePlan {
    /// What the manifest calls it
    pub dependency: String,
    /// `ebible:eng-asv`
    pub import: String,
    /// Its folder: in the library for an import dependency, else the project's `imports/<id>`
    pub dir: PathBuf,
    /// Whether it's kept in the library (an `import` dependency), not vendored in the project
    pub in_library: bool,
    /// The package's version now
    pub from: String,
    /// The publisher's version now (`[package.upstream] version`), as given
    pub upstream_from: Option<String>,
    pub outcome: Outcome,
}

/// What updating an import would do
#[derive(Debug)]
pub enum Outcome {
    /// Nothing: the catalog lists no newer version, or the fresh import has the same files
    Current,
    /// It changed
    Changed(Box<Change>),
    /// It couldn't be made again or compared
    Failed(LogoxideError),
}

/// An import made again that differs from the project's
#[derive(Debug)]
pub struct Change {
    /// The versions, the publisher's, and the summaries' difference
    pub update: ImportUpdate,
    /// The fresh import's files
    pub fresh: Arc<MemoryStorage>,
    /// The files replacing it removes (those the fresh import doesn't have)
    pub removed: Vec<String>,
    /// What it was made from, for its lock entry
    pub downloads: Vec<LockedDownload>,
    pub upstream_version: Option<String>,
}

impl Change {
    /// The fresh import as the resolver takes it ([`Importing::prepare`])
    pub fn build(&self) -> Result<ImportBuild> {
        Ok(ImportBuild {
            files: importer::copy(self.fresh.as_ref())?,
            downloads: self.downloads.clone(),
            upstream_version: self.upstream_version.clone(),
        })
    }
}

/**
Each of the project's imports (`import` dependencies, from their copy in the library, and path
packages whose `upstream` a catalog entry's import writes), or only the one `name` names (by
dependency or import name), made again from its publisher's download and compared with the one
the project has, by `reader`. An entry whose versions follow its updates (eBible.org's dates,
SWORD's `Version`) is only downloaded when the catalog lists a newer version. One that fails is
[`Outcome::Failed`], and the rest go on. Warned of: what the catalogs passed over, and a SWORD
repository's catalog that couldn't be read (`<dependency>: <error>`). The catalogs are downloaded
again, whatever `fetch`'s cache keeps, so no update is missed for an old copy. Nothing is
changed: [`apply_update`] replaces a vendored one, and an import dependency is replaced by
installing with its [`Change::build`] prepared
*/
pub fn plan_updates(
    fetch: CatalogFetch,
    config: &Config,
    project: &Project,
    library: &Library,
    reader: &dyn TextReader,
    name: Option<&str>,
    warn: Warn,
) -> Result<Vec<UpdatePlan>> {
    let locations = config.catalogs(project.manifest());
    let fetch = fetch.with(Freshness::Refresh);
    let http = fetch.http;
    let mut catalogs = load_catalogs(fetch, &locations, project.dir(), warn)?;
    // Each import: what the manifest calls it, its folder, and whether it's in the library
    let mut imports: Vec<(String, PathBuf, bool)> = vec![];
    for (dep, d) in &project.manifest().dependencies {
        let Dependency::Detailed(d) = d else {
            continue;
        };
        if let Some(path) = &d.path {
            imports.push((dep.clone(), project.dir().join(path), false));
        } else if d.import.is_some() {
            let Some(locked) = project.lock().and_then(|l| l.get(dep)) else {
                if name == Some(dep.as_str()) {
                    return Err(LogoxideError::Import(format!(
                        "{dep} isn't locked yet (lx install)"
                    )));
                }
                continue;
            };
            imports.push((
                dep.clone(),
                library.package_dir(locked, project.dir()),
                true,
            ));
        }
    }
    let mut plans = vec![];
    for (dep, dir, in_library) in imports {
        let current: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir).no_symlinks());
        let Ok(package) = Package::open(current.clone(), Features::default()) else {
            if in_library && name == Some(dep.as_str()) {
                return Err(LogoxideError::Import(format!(
                    "{dep} isn't installed (lx install)"
                )));
            }
            continue;
        };
        let Some(upstream) = package.info().upstream.clone() else {
            continue;
        };
        // A SWORD module from a repository the project doesn't search: its catalog is beside
        // its packages
        if !catalogs
            .iter()
            .any(|c| c.find_upstream(&upstream).is_some())
            && let Some((root, _)) =
                (upstream.url.as_deref()).and_then(|u| u.split_once("/packages/"))
        {
            match load_catalog(fetch, &format!("{root}/mods.d.tar.gz"), project.dir()) {
                Ok(catalog) => {
                    catalog.warnings.iter().cloned().for_each(&mut *warn);
                    catalogs.push(catalog);
                }
                Err(e) => warn(format!("{dep}: {e}")),
            }
        }
        // A CCEL work imported by its name
        if !catalogs
            .iter()
            .any(|c| c.find_upstream(&upstream).is_some())
            && let Some(catalog) =
                (upstream.import.as_deref()).and_then(logoxide_import::ccel::named_catalog)
        {
            catalogs.push(catalog);
        }
        let Some(entry) = catalogs.iter().find_map(|c| c.find_upstream(&upstream)) else {
            if name == Some(dep.as_str()) {
                let named = (upstream.import.as_deref())
                    .or(upstream.url.as_deref())
                    .unwrap_or("its upstream");
                return Err(LogoxideError::Import(format!(
                    "{dep} ({named}) is in none of this project's catalogs (lx catalog list)"
                )));
            }
            continue;
        };
        let import_name = entry.source.import_name().unwrap_or_default();
        if name.is_some_and(|n| n != dep && n != import_name) {
            continue;
        }
        let outcome = match logoxide_import::import_is_current(entry, upstream.version.as_deref()) {
            true => Outcome::Current,
            false => match remade(http, entry, current, reader) {
                Ok(Some(change)) => Outcome::Changed(Box::new(change)),
                Ok(None) => Outcome::Current,
                Err(error) => Outcome::Failed(error),
            },
        };
        plans.push(UpdatePlan {
            dependency: dep,
            import: import_name.to_string(),
            dir,
            in_library,
            from: package.info().version.to_string(),
            upstream_from: upstream.version.clone(),
            outcome,
        });
    }
    if plans.is_empty()
        && let Some(name) = name
    {
        return Err(LogoxideError::Import(format!(
            "{name}: no import of that name in this project"
        )));
    }
    Ok(plans)
}

/// An import made again and compared with `current`: `None` when its declared files match
fn remade(
    http: &dyn Http,
    entry: &CatalogEntry,
    current: Arc<dyn Storage>,
    reader: &dyn TextReader,
) -> Result<Option<Change>> {
    let files = importer::download(http, entry)?;
    let named: Vec<(String, Vec<u8>)> = (files.iter())
        .map(|f| (f.name.clone(), f.bytes.clone()))
        .collect();
    let fresh = Arc::new(MemoryStorage::new());
    logoxide_import::build(entry, &named, fresh.as_ref())?;
    let Some(update) = logoxide_import::compare_import(current.clone(), &fresh, reader)? else {
        return Ok(None);
    };
    let removed = logoxide_import::import_removals(current.as_ref(), fresh.as_ref())?;
    let downloads = (files.iter())
        .map(|f| LockedDownload {
            name: f.name.clone(),
            url: f.url.clone(),
            sha256: Some(logoxide::publish::sha256(&f.bytes)),
        })
        .collect();
    Ok(Some(Change {
        update,
        fresh,
        removed,
        downloads,
        upstream_version: entry.version.clone(),
    }))
}

/**
Replaces the vendored import in `dir` with the fresh one: its declared files the fresh one lacks
are removed (only when `allow_removals`, else nothing is changed and it's an error), the rest
written; other files stay. Then lock it again (an install with `Update::Only` its dependency).
An import kept in the library isn't changed in place: install with its [`Change::build`]
prepared instead, which leaves the version it replaces to other projects and `lx clean`
*/
pub fn apply_update(change: &Change, dir: &Path, allow_removals: bool) -> Result<()> {
    if !change.removed.is_empty() && !allow_removals {
        return Err(LogoxideError::Import(format!(
            "updating {} removes {}",
            dir.display(),
            change.removed.join(", ")
        )));
    }
    logoxide_import::apply_import(&FsStorage::new(dir).no_symlinks(), change.fresh.as_ref())
}
