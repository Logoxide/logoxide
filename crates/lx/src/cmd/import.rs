//! `lx import`: packages made from other publishers' downloads, and updating them

use std::{path::Path, sync::Arc};

use logoxide::{
    Features, FsStorage, MemoryStorage, Package, Storage,
    manifest::{Dependency, DetailedDependency, MANIFEST},
    resolve::Update,
};

use crate::{
    cli::{Cli, Format},
    cmd::{
        catalog::{load_catalog, load_catalogs, warn_shadowed},
        package::{DiffReport, print_skipped},
    },
    config,
    net::get,
    print_json,
};

pub(crate) fn import(cli: &Cli, source: &str) -> logoxide::Result<()> {
    use logoxide::catalog::CatalogSource;
    let prefixes = CatalogSource::PREFIXES
        .map(|p| format!("{p}:<id>"))
        .join(" or ");
    let known = (source.split_once(':'))
        .is_some_and(|(prefix, _)| CatalogSource::PREFIXES.contains(&prefix));
    if !known {
        return Err(logoxide::LogoxideError::Import(format!(
            "{source}: give {prefixes} (see lx search)"
        )));
    }
    let library = cli.library();
    let mut project = cli.project(&library)?;
    // The project's catalogs, in order: the first that lists it is the one used
    let mut catalogs = load_catalogs(&config().catalogs(project.manifest()), project.dir())?;
    // A SWORD repository CrossWire lists (`sword:crosswire-beta/…`) is read even if the project
    // doesn't search it
    if !catalogs.iter().any(|c| c.find_import(source).is_some())
        && let Some(url) = (source.strip_prefix("sword:"))
            .and_then(|rest| rest.split_once('/'))
            .and_then(|(repository, _)| logoxide::catalog::sword_catalog_url(repository))
    {
        catalogs.push(load_catalog(&url, project.dir())?);
    }
    let (used, entry) = (catalogs.iter())
        .find_map(|c| Some((c, c.find_import(source)?)))
        .ok_or_else(|| {
            logoxide::LogoxideError::Import(format!(
                "{source}: in none of this project's catalogs (lx catalog list)"
            ))
        })?;
    // Another catalog listing this id would have its own package under the same name
    warn_shadowed(&catalogs, &entry.id, &used.name);
    let relative = format!("imports/{}", entry.id);
    let dir = project.dir().join(&relative);
    if dir.exists() {
        return Err(logoxide::LogoxideError::Import(format!(
            "{} already exists",
            dir.display()
        )));
    }
    // Made in a folder beside it, then moved into place whole: a download that fails partway
    // leaves nothing behind for the next try to stop at
    let staging = (project.dir().join("imports")).join(format!(
        ".{}.partial-{}",
        entry.id,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&staging);
    let made = import_files(entry, &FsStorage::new(&staging)).and_then(|()| {
        std::fs::rename(&staging, &dir).map_err(|e| logoxide::LogoxideError::Io {
            path: dir.display().to_string(),
            message: e.to_string(),
        })
    });
    if let Err(error) = made {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }
    // What the download had that isn't read as a Bible's books (front matter, a glossary, books
    // logoxide doesn't know)
    let made: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir).no_symlinks());
    if let Ok(package) = Package::open(made, Features::default()) {
        print_skipped(package.source().skipped());
    }
    let dependency = DetailedDependency {
        path: Some(relative),
        default_features: true,
        ..DetailedDependency::default()
    };
    // If it can't be added and installed, the project is left as it was: no folder, and the
    // manifest it had
    let manifest_path = project.dir().join(MANIFEST);
    let manifest = manifest_path.is_file().then(|| project.manifest_text());
    let mut add = || -> logoxide::Result<()> {
        project.add(&entry.id, &dependency)?;
        project.save_manifest()?;
        project.install(&library, &cli.fetcher(&library), Update::Nothing)?;
        Ok(())
    };
    if let Err(error) = add() {
        let _ = std::fs::remove_dir_all(&dir);
        let _ = match manifest {
            Some(text) => std::fs::write(&manifest_path, text),
            None => std::fs::remove_file(&manifest_path),
        };
        return Err(error);
    }
    outln!(
        "imported {} ({}) into {}",
        entry.id,
        entry.name,
        dir.display()
    );
    crate::cmd::project::warn_of_same_works(&project, &library);
    Ok(())
}

/// Downloads what a catalog entry's import needs, and makes its package in `dest`
fn import_files(
    entry: &logoxide::catalog::CatalogEntry,
    dest: &dyn Storage,
) -> logoxide::Result<()> {
    let mut files = vec![];
    for download in logoxide::import::downloads(entry)? {
        // The first of its URLs that has it
        let mut bytes = Err(logoxide::LogoxideError::Import(format!(
            "{}: no URL for {}",
            entry.id, download.name
        )));
        for url in &download.urls {
            // `get` warns of plain HTTP
            bytes = get(url);
            if bytes.is_ok() {
                break;
            }
        }
        files.push((download.name, bytes?));
    }
    logoxide::import::build(entry, &files, dest)
}

/**
`lx import --update`: each of the project's imports (path packages whose `upstream` a catalog
entry's import writes), or the one named, made again from its publisher's download and compared
with the one the project has; changed ones are replaced (unless `dry_run`) and locked again. An
entry whose versions follow its updates (eBible.org's dates, SWORD's `Version`) is only
downloaded when the catalog lists a newer version. One that fails is said, and the rest go on;
those replaced are locked again whatever failed, and then it's an error. Files an update removes
are asked about first in a terminal (unless `yes`); elsewhere, `yes` must allow it
*/
pub(crate) fn import_update(
    cli: &Cli,
    name: Option<&str>,
    dry_run: bool,
    yes: bool,
) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let json = cli.format == Format::Json;
    let mut catalogs = load_catalogs(&config().catalogs(project.manifest()), project.dir())?;
    let paths: Vec<(String, String)> = (project.manifest().dependencies.iter())
        .filter_map(|(dep, d)| match d {
            Dependency::Detailed(d) => Some((dep.clone(), d.path.clone()?)),
            Dependency::Version(_) => None,
        })
        .collect();
    let (mut found, mut changed) = (0, std::collections::BTreeSet::new());
    let (mut reports, mut failed) = (vec![], vec![]);
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
            match load_catalog(&format!("{root}/mods.d.tar.gz"), project.dir()) {
                Ok(catalog) => catalogs.push(catalog),
                Err(e) => errln!("warning: {dep}: {e}"),
            }
        }
        let Some(entry) = catalogs.iter().find_map(|c| c.find_upstream(&upstream)) else {
            if name == Some(dep.as_str()) {
                let named = (upstream.import.as_deref())
                    .or(upstream.url.as_deref())
                    .unwrap_or("its upstream");
                return Err(logoxide::LogoxideError::Import(format!(
                    "{dep} ({named}) is in none of this project's catalogs (lx catalog list)"
                )));
            }
            continue;
        };
        let import_name = entry.source.import_name().unwrap_or_default();
        if name.is_some_and(|n| n != dep && n != import_name) {
            continue;
        }
        found += 1;
        let mut report = ImportReport {
            dependency: dep.clone(),
            import: import_name.clone(),
            status: "current",
            from: package.info().version.to_string(),
            to: None,
            upstream_from: upstream.version.clone(),
            upstream_to: None,
            diff: None,
            removed: vec![],
            error: None,
        };
        match update_import(
            entry,
            current,
            &dir,
            &mut report,
            UpdateOptions { dry_run, yes, json },
        ) {
            Ok(()) if report.status == "updated" => {
                changed.insert(dep);
            }
            Ok(()) => {}
            Err(error) => {
                errln!("lx: {dep}: {error}");
                report.status = "failed";
                report.error = Some(error.to_string());
                failed.push(dep);
            }
        }
        reports.push(report);
    }
    if found == 0
        && let Some(name) = name
    {
        return Err(logoxide::LogoxideError::Import(format!(
            "{name}: no import of that name in this project"
        )));
    }
    if found == 0 && !json {
        outln!("this project has no imports");
    }
    // Those replaced are locked again, whatever else failed
    let locked = match changed.is_empty() {
        true => Ok(()),
        false => {
            (project.install(&library, &cli.fetcher(&library), Update::Only(changed))).map(|_| ())
        }
    };
    if json {
        print_json(&reports)?;
    }
    locked?;
    match failed.len() {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Import(format!(
            "{n} import{} not updated: {}",
            if n == 1 { "" } else { "s" },
            failed.join(", ")
        ))),
    }
}

/// What `lx import --update` did with one import, for `--format json`
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReport {
    /// What the manifest calls it
    dependency: String,
    /// `ebible:eng-asv`
    import: String,
    /// `current` (nothing to do), `changed` (with --dry-run), `updated`, `declined` (its removals
    /// weren't allowed), or `failed`
    status: &'static str,
    /// The package's versions (logoxide's, raised by what the update changes)
    from: String,
    to: Option<String>,
    /// The publisher's versions (`[package.upstream] version`), as given
    upstream_from: Option<String>,
    upstream_to: Option<String>,
    diff: Option<DiffReport>,
    /// The files the update removes (those the fresh import doesn't have)
    removed: Vec<String>,
    error: Option<String>,
}

/// How `lx import --update` goes: only saying what would change, removing files without asking,
/// printing JSON
#[derive(Clone, Copy)]
struct UpdateOptions {
    dry_run: bool,
    yes: bool,
    json: bool,
}

/// One import made again and, if it changed, replaced (unless `dry_run`): what happened is in
/// `report`
fn update_import(
    entry: &logoxide::catalog::CatalogEntry,
    current: Arc<dyn Storage>,
    dir: &Path,
    report: &mut ImportReport,
    options: UpdateOptions,
) -> logoxide::Result<()> {
    use logoxide::import::{apply_import, compare_import, import_removals};
    let UpdateOptions { dry_run, yes, json } = options;
    let (dep, from, import_name) = (
        report.dependency.clone(),
        report.from.clone(),
        report.import.clone(),
    );
    let say = |text: String| {
        if !json {
            outln!("{text}");
        }
    };
    if logoxide::import::import_is_current(entry, report.upstream_from.as_deref()) {
        say(format!("{dep} {from}: up to date ({import_name})"));
        return Ok(());
    }
    let fresh = Arc::new(MemoryStorage::new());
    import_files(entry, fresh.as_ref())?;
    let Some(update) = compare_import(current.clone(), &fresh)? else {
        say(format!("{dep} {from}: up to date ({import_name})"));
        return Ok(());
    };
    let publisher = match (&update.upstream_from, &update.upstream_to) {
        (Some(old), Some(new)) if old != new => format!("; the publisher's {old} → {new}"),
        (None, Some(new)) => format!("; the publisher's {new}"),
        _ => String::new(),
    };
    say(format!(
        "{dep} {} → {}: {} (a {:?} change{publisher})",
        update.from,
        update.to,
        logoxide::release::describe(&update.diff),
        update.diff.change()
    ));
    let removed = import_removals(current.as_ref(), fresh.as_ref())?;
    report.status = "changed";
    report.to = Some(update.to.to_string());
    report.diff = Some(DiffReport::of(&update.diff));
    report.upstream_to = update.upstream_to.clone();
    report.removed = removed.clone();
    if dry_run {
        if !removed.is_empty() {
            say(format!("    would remove {}", removed.join(", ")));
        }
        return Ok(());
    }
    if !removed.is_empty() && !yes {
        use std::io::IsTerminal;
        let what = format!(
            "{dep}'s update removes {} file{} from {}: {}",
            removed.len(),
            if removed.len() == 1 { "" } else { "s" },
            dir.display(),
            removed.join(", ")
        );
        if !std::io::stdin().is_terminal() {
            return Err(logoxide::LogoxideError::Import(format!(
                "{what}; pass --yes to allow that"
            )));
        }
        eprint!("{what}. Remove them? [y/N] ");
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            say(format!("{dep}: left as it was"));
            report.status = "declined";
            return Ok(());
        }
    }
    apply_import(&FsStorage::new(dir).no_symlinks(), fresh.as_ref())?;
    report.status = "updated";
    Ok(())
}
