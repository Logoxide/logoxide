/*!
Imports: a package made from another publisher's download (eBible.org, fetch.bible, CCEL, a SWORD
repository), put in the project's `imports/<id>/` and added as a path package; and updating
those, each made again and compared with the one the project has. The publishers' formats are
logoxide-import's (`downloads`, `build`, `update`); this strings them together with the
project's catalogs, an [`Http`], and the installer
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
    manifest::{Dependency, DetailedDependency, MANIFEST},
    reader::Skipped,
    resolve::Update,
};
use logoxide_import::ImportUpdate;

use crate::{
    Warn,
    catalogs::{load_catalog, load_catalogs, shadowed},
    http::Http,
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
catalog listing the same id (whose package would have the same name)
*/
pub fn find_entry(
    http: &dyn Http,
    config: &Config,
    project: &Project,
    source: &str,
    warn: Warn,
) -> Result<CatalogEntry> {
    check_source(source)?;
    let locations = config.catalogs(project.manifest());
    let mut catalogs = load_catalogs(http, &locations, project.dir(), warn)?;
    if !catalogs.iter().any(|c| c.find_import(source).is_some())
        && let Some(url) = (source.strip_prefix("sword:"))
            .and_then(|rest| rest.split_once('/'))
            .and_then(|(repository, _)| logoxide_import::sword::catalog_url(repository))
    {
        let catalog = load_catalog(http, &url, project.dir())?;
        catalog.warnings.iter().cloned().for_each(&mut *warn);
        catalogs.push(catalog);
    }
    // A CCEL work by its author and name (`ccel:henry/mhc1`) needs no catalog
    if !catalogs.iter().any(|c| c.find_import(source).is_some())
        && let Some(catalog) = logoxide_import::ccel::named_catalog(source)
    {
        catalogs.push(catalog);
    }
    let (used, entry) = (catalogs.iter())
        .find_map(|c| Some((c, c.find_import(source)?)))
        .ok_or_else(|| {
            LogoxideError::Import(format!(
                "{source}: in none of this project's catalogs (lx catalog list)"
            ))
        })?;
    if let Some(warning) = shadowed(&catalogs, &entry.id, &used.name) {
        warn(warning);
    }
    Ok(entry.clone())
}

/// An import made and added
#[derive(Debug)]
pub struct Imported {
    /// The package's id, which names the dependency
    pub id: String,
    pub name: String,
    /// Where it was made: `<project>/imports/<id>`
    pub dir: PathBuf,
    /// What the download had that isn't read as a Bible's books (front matter, a glossary,
    /// books logoxide doesn't know)
    pub skipped: Vec<Skipped>,
}

/// Downloads what a catalog entry's import needs (each file from the first of its URLs that has
/// it), and makes its package in `dest`
pub fn files(http: &dyn Http, entry: &CatalogEntry, dest: &dyn Storage) -> Result<()> {
    let mut files = vec![];
    for download in logoxide_import::downloads(entry)? {
        let mut bytes = Err(LogoxideError::Import(format!(
            "{}: no URL for {}",
            entry.id, download.name
        )));
        for url in &download.urls {
            bytes = http.get(url);
            if bytes.is_ok() {
                break;
            }
        }
        files.push((download.name, bytes?));
    }
    logoxide_import::build(entry, &files, dest)
}

/**
Makes `entry`'s package in the project's `imports/<id>/` and adds it as a path package, installed
and locked. It's made in a folder beside it and moved into place whole, so a download that fails
partway leaves nothing behind; if it can't be added and installed, the project is left as it was:
no folder, and the manifest it had
*/
pub fn run(
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
    let skipped = match Package::open(made, Features::default()) {
        Ok(package) => (package.source().skipped().iter())
            .cloned()
            .map(Into::into)
            .collect(),
        Err(_) => vec![],
    };
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
    /// Its folder
    pub dir: PathBuf,
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
}

/**
Each of the project's imports (path packages whose `upstream` a catalog entry's import writes),
or only the one `name` names (by dependency or import name), made again from its publisher's
download and compared with the one the project has, by `reader`. An entry whose versions follow
its updates (eBible.org's dates, SWORD's `Version`) is only downloaded when the catalog lists a
newer version. One that fails is [`Outcome::Failed`], and the rest go on. Warned of: what the
catalogs passed over, and a SWORD repository's catalog that couldn't be read (`<dependency>:
<error>`). Nothing is changed: [`apply_update`] replaces one
*/
pub fn plan_updates(
    http: &dyn Http,
    config: &Config,
    project: &Project,
    reader: &dyn TextReader,
    name: Option<&str>,
    warn: Warn,
) -> Result<Vec<UpdatePlan>> {
    let locations = config.catalogs(project.manifest());
    let mut catalogs = load_catalogs(http, &locations, project.dir(), warn)?;
    let paths: Vec<(String, String)> = (project.manifest().dependencies.iter())
        .filter_map(|(dep, d)| match d {
            Dependency::Detailed(d) => Some((dep.clone(), d.path.clone()?)),
            Dependency::Version(_) => None,
        })
        .collect();
    let mut plans = vec![];
    for (dep, path) in paths {
        let dir = project.dir().join(&path);
        let current: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir).no_symlinks());
        let Ok(package) = Package::open(current.clone(), Features::default()) else {
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
            match load_catalog(http, &format!("{root}/mods.d.tar.gz"), project.dir()) {
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
    let fresh = Arc::new(MemoryStorage::new());
    files(http, entry, fresh.as_ref())?;
    let Some(update) = logoxide_import::compare_import(current.clone(), &fresh, reader)? else {
        return Ok(None);
    };
    let removed = logoxide_import::import_removals(current.as_ref(), fresh.as_ref())?;
    Ok(Some(Change {
        update,
        fresh,
        removed,
    }))
}

/**
Replaces the import in `dir` with the fresh one: its declared files the fresh one lacks are
removed (only when `allow_removals`, else nothing is changed and it's an error), the rest written;
other files stay. Then lock it again (an install with `Update::Only` its dependency)
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
