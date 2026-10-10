//! Commands on a project's packages: add, remove, enable and disable, install, list, info, clean

use std::path::PathBuf;

use logoxide::{
    FsStorage, MemoryStorage, Project, Storage,
    manifest::{DetailedDependency, MANIFEST},
    resolve::{Frozen, Update},
};

use crate::{
    cli::{AddArgs, Cli, Format},
    cmd::catalog::{load_registry, registry_url, warn_other_registries},
    config, print_json,
};

fn version_req(text: &str) -> logoxide::Result<semver::VersionReq> {
    text.parse()
        .map_err(|e| logoxide::LogoxideError::Manifest(format!("version `{text}`: {e}")))
}

pub(crate) fn add(cli: &Cli, args: &AddArgs) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    // Before anything is fetched: a short one is refused, and says what to give
    if let Some(rev) = &args.rev {
        logoxide::manifest::check_rev(rev)?;
    }
    let named = args.repository.as_ref().or(args.git.as_ref());
    // A registry's package published as archives: added with `registry`, not `git`
    let archive = match (&args.registry, named) {
        (Some(_), None) => {
            return Err(logoxide::LogoxideError::Manifest(String::from(
                "name the package: lx add --registry <catalog> <id or path>",
            )));
        }
        (registry, Some(text)) => archive_entry(&project, text, registry.as_deref())?,
        (None, None) => None,
    };
    let git = match archive {
        Some(_) => None,
        None => named.map(|url| registry_url(&project, url)).transpose()?,
    };
    // A path is kept relative to the manifest, so the project (and its lock) can move
    let path = (args.path.as_deref()).map(|path| relative_path(project.dir(), path));
    let mut dependency = DetailedDependency {
        git: git.clone(),
        path: path.clone(),
        version: args.version.as_deref().map(version_req).transpose()?,
        tag: args.tag.clone(),
        branch: args.branch.clone(),
        rev: args.rev.clone(),
        folder: args.folder.clone(),
        features: args.features.clone(),
        default_features: !args.no_default_features,
        ..DetailedDependency::default()
    };
    // The package's id names the dependency: read it from the package
    let info: Named = match (archive, &git, &path) {
        (Some((catalog, entry)), _, _) => {
            if dependency.version.is_none()
                && let Some(newest) = entry.release(None)
            {
                dependency.version = Some(version_req(&newest.version)?);
            }
            dependency.registry = Some(catalog);
            Named {
                id: entry.id,
                abbreviation: entry.abbreviation,
                aliases: entry.aliases,
            }
        }
        (None, Some(url), _) => {
            let fetcher = cli.fetcher(&library);
            let reference = match (&args.tag, &args.branch, &args.rev) {
                (Some(t), _, _) => logoxide::git::GitRef::Tag(t.clone()),
                (_, Some(b), _) => logoxide::git::GitRef::Branch(b.clone()),
                (_, _, Some(r)) => logoxide::git::GitRef::Rev(r.clone()),
                _ => logoxide::git::GitRef::Head,
            };
            // Only its manifest, where the host allows that
            let probe = MemoryStorage::new();
            let manifest_path = match &args.folder {
                Some(folder) => format!("{}/{MANIFEST}", folder.trim_end_matches('/')),
                None => String::from(MANIFEST),
            };
            logoxide::git::Fetcher::fetch_files(
                &fetcher,
                url,
                &reference,
                &[manifest_path.as_str()],
                &probe,
            )?;
            let manifest = logoxide::Manifest::parse(&probe.read_string(&manifest_path)?)?;
            let info = manifest
                .package
                .ok_or_else(|| logoxide::LogoxideError::Manifest(format!("{url}: no [package]")))?;
            // Like Cargo: without a tag, branch, or revision, require the newest release's version
            if dependency.version.is_none()
                && dependency.tag.is_none()
                && dependency.branch.is_none()
                && dependency.rev.is_none()
            {
                let tags = logoxide::git::Fetcher::tags(&fetcher, url)?;
                let newest = tags
                    .iter()
                    .filter_map(|(t, _)| {
                        logoxide::resolve::folder_tag_version(t, args.folder.as_deref())
                    })
                    .filter(|v| v.pre.is_empty())
                    .max();
                let odd = logoxide::resolve::not_semver_note(
                    tags.iter().map(|(t, _)| t.as_str()),
                    args.folder.as_deref(),
                );
                match newest {
                    Some(newest) => {
                        dependency.version = Some(version_req(&newest.to_string())?);
                        if !odd.is_empty() {
                            errln!("warning: {url}: some tags aren't releases{odd}");
                        }
                    }
                    // Without a release, what would be locked is whatever its default branch
                    // holds now: say so by asking for the branch, or a commit
                    None => {
                        return Err(logoxide::LogoxideError::Resolve(format!(
                            "{url} has no releases (tags like v1.0.0){odd}: give --branch or --rev \
                             to follow a branch or keep a commit, or --tag"
                        )));
                    }
                }
            }
            info.into()
        }
        (None, None, Some(path)) => {
            let storage = FsStorage::new(project.dir().join(path));
            let manifest = logoxide::Manifest::parse(&storage.read_string(MANIFEST)?)?;
            manifest
                .package
                .ok_or_else(|| logoxide::LogoxideError::Manifest(format!("{path}: no [package]")))?
                .into()
        }
        (None, None, None) => {
            return Err(logoxide::LogoxideError::Manifest(String::from(
                "give a repository (github:owner/repo, or --git URL) or --path",
            )));
        }
    };
    let name = args.name.clone().unwrap_or_else(|| info.id.clone());
    if name != info.id {
        dependency.package = Some(info.id.clone());
    }
    dependency.abbreviation = args.abbreviation.clone();
    if !args.aliases.is_empty() {
        dependency.aliases = Some(
            args.aliases
                .iter()
                .filter(|a| !a.is_empty())
                .cloned()
                .collect(),
        );
    }
    // Another source under the same name would replace it: a fork needs its own name
    if let Some(logoxide::manifest::Dependency::Detailed(existing)) =
        project.manifest().dependencies.get(&name)
        && (
            existing.git.as_deref(),
            existing.registry.as_deref(),
            existing.path.as_deref(),
        ) != (
            dependency.git.as_deref(),
            dependency.registry.as_deref(),
            dependency.path.as_deref(),
        )
    {
        let from = (existing.git.as_deref())
            .or(existing.registry.as_deref())
            .or(existing.path.as_deref())
            .unwrap_or("?");
        return Err(logoxide::LogoxideError::Manifest(format!(
            "`{name}` is already {from}: give this one another name (--name {name}-2)"
        )));
    }
    // References name each Bible by its abbreviation and aliases: two can't share one
    if let Some(lock) = project.lock() {
        let abbreviation =
            (dependency.abbreviation.clone()).unwrap_or_else(|| info.abbreviation.clone());
        let aliases = (dependency.aliases.clone()).unwrap_or_else(|| info.aliases.clone());
        let ours: Vec<String> = std::iter::once(abbreviation.clone())
            .chain(aliases)
            .collect();
        for other in lock.roots().filter(|p| p.dependency_name() != name) {
            let Ok(package) = library.open(other, project.dir()) else {
                continue;
            };
            let (theirs, their_aliases) = project.names(other, package.info());
            let clash = (std::iter::once(&theirs).chain(&their_aliases))
                .find(|n| ours.iter().any(|o| o.eq_ignore_ascii_case(n)));
            if let Some(clash) = clash {
                return Err(logoxide::LogoxideError::Manifest(format!(
                    "`{clash}` already names {} here: give this one another (--abbreviation {abbreviation}2, --alias)",
                    other.dependency_name()
                )));
            }
        }
    }
    // Installed from the edited manifest before it's written: if installing fails, the file is
    // as it was
    project.add(&name, &dependency)?;
    let fetcher = cli.fetcher(&library);
    let lock = (project.install(&library, &fetcher, Update::Only([name.clone()].into()))?).clone();
    project.save_manifest()?;
    let locked = lock.get(&name).map(|p| p.label()).unwrap_or(name);
    outln!(
        "added {locked} to {}: installed, and locked in {}",
        project.dir().join(MANIFEST).display(),
        logoxide::lock::LOCKFILE
    );
    warn_of_clashes(&project, &library);
    warn_of_absolute_paths(&lock);
    Ok(())
}

/**
`path` (as given: absolute, or relative to the project's folder, else to the current one)
relative to the project's folder, when both can be found; else as given
*/
fn relative_path(project: &std::path::Path, path: &str) -> String {
    let given = std::path::Path::new(path);
    let found = match given.is_absolute() {
        true => given.to_path_buf(),
        false => match project.join(given).exists() {
            true => project.join(given),
            false => given.to_path_buf(),
        },
    };
    let (Ok(from), Ok(to)) = (project.canonicalize(), found.canonicalize()) else {
        return path.to_string();
    };
    let (from, to): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    let shared = (from.iter().zip(&to)).take_while(|(a, b)| a == b).count();
    // Nothing in common (another drive): it can only be absolute
    if shared == 0 {
        return path.to_string();
    }
    let up = std::iter::repeat_n(String::from(".."), from.len() - shared);
    let down = (to[shared..].iter()).map(|c| c.as_os_str().to_string_lossy().into_owned());
    let parts: Vec<String> = up.chain(down).collect();
    match parts.is_empty() {
        true => String::from("."),
        false => parts.join("/"),
    }
}

/// Warns of path packages named by an absolute path: the lock file names it too, and another
/// machine won't have it there
pub(crate) fn warn_of_absolute_paths(lock: &logoxide::Lockfile) {
    for locked in &lock.packages {
        if let Some(path) = &locked.path
            && std::path::Path::new(path).is_absolute()
        {
            errln!(
                "warning: {}: path {path} is absolute, so {} won't work on another machine \
                 (give it relative to {MANIFEST})",
                locked.label(),
                logoxide::lock::LOCKFILE
            );
        }
    }
}

/**
Warns of names two of the project's Bibles answer to (a package the manifest asks for at one
version and a collection at another): references naming one can't say which
*/
pub(crate) fn warn_of_clashes(project: &Project, library: &logoxide::Library) {
    match project.name_clashes(library) {
        Ok(clashes) => {
            for clash in clashes {
                errln!(
                    "warning: `{}` names {}: references to it can't say which (give one an \
                     `abbreviation` in {MANIFEST})",
                    clash.name,
                    clash.packages.join(" and ")
                );
            }
        }
        Err(e) => errln!("warning: {e}"),
    }
}

/// What a package being added is called: its id, abbreviation, and aliases
struct Named {
    id: String,
    abbreviation: String,
    aliases: Vec<String>,
}

impl From<logoxide::manifest::Package> for Named {
    fn from(info: logoxide::manifest::Package) -> Named {
        Named {
            id: info.id,
            abbreviation: info.abbreviation,
            aliases: info.aliases,
        }
    }
}

/**
A package a registry publishes as archives, and the registry's catalog (as a dependency names
it): through one of the project's registries (`dorean:the-dorean-principle`), or in the catalog
`--registry` gives. None when `text` names no such package (a git URL, or a registry's git package)
*/
fn archive_entry(
    project: &logoxide::Project,
    text: &str,
    catalog: Option<&str>,
) -> logoxide::Result<Option<(String, logoxide::catalog::CatalogEntry)>> {
    let registries = config().registries(project.manifest());
    let (registry, location, name) = match catalog {
        Some(catalog) => ("--registry", catalog.to_string(), text),
        None => match logoxide::catalog::split_registry_name(text, &registries) {
            Some((registry, name)) => (registry, registries[registry].clone(), name),
            None => return Ok(None),
        },
    };
    let loaded = load_registry(registry, &location, project.dir(), true)?;
    let entry = loaded.find(name)?.ok_or_else(|| {
        logoxide::LogoxideError::Catalog(format!("{location} has no package `{name}`"))
    })?;
    match &entry.source {
        logoxide::catalog::CatalogSource::Registry { catalog } => {
            warn_other_registries(project, &loaded, &location, &entry.id);
            Ok(Some((catalog.clone(), entry.clone())))
        }
        _ if catalog.is_some() => Err(logoxide::LogoxideError::Catalog(format!(
            "`{name}` in {location} isn't published as archives"
        ))),
        _ => Ok(None),
    }
}

pub(crate) fn remove(cli: &Cli, id: &str) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    if !project.remove(id)? {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "no dependency `{id}`"
        )));
    }
    // Written only once the rest still installs
    project.install(&library, &cli.fetcher(&library), Update::Nothing)?;
    project.save_manifest()?;
    outln!("removed {id}");
    Ok(())
}

pub(crate) fn set_enabled(cli: &Cli, name: &str, enabled: bool) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let was = project.is_enabled(name);
    if !project.set_enabled(name, enabled)? {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "no dependency `{name}`"
        )));
    }
    // Nothing to install or lock: it stays as it is either way
    project.save_manifest()?;
    match (was, enabled) {
        (true, true) => outln!("{name} is already on"),
        (false, false) => outln!("{name} is already off"),
        (_, true) => outln!("turned {name} on"),
        (_, false) => {
            outln!("turned {name} off (still installed: `lx enable {name}` turns it on)")
        }
    }
    Ok(())
}

pub(crate) fn repair(cli: &Cli) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    // Which installed versions changed, to say what was put back
    let changed: Vec<String> = (project.lock().into_iter())
        .flat_map(|lock| lock.packages.iter())
        .filter(|p| p.checksum.is_some() && library.is_installed(p, project.dir()))
        .filter(|p| {
            let dir = FsStorage::new(library.package_dir(p, project.dir()));
            logoxide::package::checksum(&dir).ok() != p.checksum
        })
        .map(|p| p.label())
        .collect();
    let lock = project.repair(&library, &cli.fetcher(&library))?;
    for label in &changed {
        outln!("repaired {label}");
    }
    outln!(
        "{} packages, {} repaired, locked in {}",
        lock.packages.len(),
        changed.len(),
        project.dir().join(logoxide::lock::LOCKFILE).display()
    );
    Ok(())
}

/// Installs what's locked, moving what `update` allows, within what `frozen` allows
pub(crate) fn install(cli: &Cli, update: Update, frozen: Frozen) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let previous = project.lock().cloned();
    let before: Vec<String> = (previous.iter())
        .flat_map(|l| l.packages.iter().map(|p| p.name()))
        .collect();
    let fetcher = cli.fetcher(&library);
    let lock = match frozen == Frozen::default() {
        true => project.install(&library, &fetcher, update)?,
        false => project.install_frozen(&library, &fetcher, frozen)?,
    };
    // A release whose tag now names another commit: what was read before may not be what's
    // read now
    for moved in previous.map(|p| lock.moved_since(&p)).unwrap_or_default() {
        errln!(
            "warning: {}: {} was moved ({} before, {} now): its text may not be what was locked",
            moved.label,
            moved.release,
            moved.before,
            moved.after
        );
    }
    warn_of_absolute_paths(lock);
    for package in &lock.packages {
        let note = if before.contains(&package.name()) {
            ""
        } else {
            " (new)"
        };
        outln!("{}{note}", package.label());
    }
    outln!(
        "{} packages, locked in {}",
        lock.packages.len(),
        project.dir().join(logoxide::lock::LOCKFILE).display()
    );
    warn_of_clashes(&project, &library);
    Ok(())
}

/// Prints whether a dependency is on, and returns it
pub(crate) fn is_enabled(cli: &Cli, name: &str) -> logoxide::Result<bool> {
    let library = cli.library();
    let project = cli.project(&library)?;
    if !project.manifest().dependencies.contains_key(name) {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "no dependency `{name}`"
        )));
    }
    let enabled = project.is_enabled(name);
    outln!("{}", if enabled { "on" } else { "off" });
    Ok(enabled)
}

/// A locked package, as `lx list --format json` prints it
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    /// What the manifest calls it (its id unless renamed)
    name: String,
    id: String,
    version: String,
    /// `kjv 1.0.1`, or `kjv-bob (kjv) 1.0.1`
    label: String,
    /// Brought in by another package, not asked for by the manifest
    dependency: bool,
    enabled: bool,
    installed: bool,
    /// What references name it by here, its title, and its license (when it's installed)
    abbreviation: Option<String>,
    title: Option<String>,
    license: Option<String>,
    git: Option<String>,
    tag: Option<String>,
    branch: Option<String>,
    commit: Option<String>,
    path: Option<String>,
    registry: Option<String>,
}

/// The locked packages; with `only`, those in use (`true`) or not (`false`)
pub(crate) fn list(cli: &Cli, only: Option<bool>) -> logoxide::Result<()> {
    let library = cli.library();
    let project = cli.project(&library)?;
    let json = cli.format == Format::Json;
    let Some(lock) = project.lock() else {
        match json {
            true => outln!("[]"),
            false => outln!("nothing installed yet: `lx add --git URL`, then `lx install`"),
        }
        return Ok(());
    };
    let in_use: std::collections::BTreeSet<String> =
        project.in_use().iter().map(|p| p.name()).collect();
    if json {
        let mut listed = vec![];
        for locked in &lock.packages {
            let used = in_use.contains(&locked.name());
            if only.is_some_and(|wanted| wanted != used) {
                continue;
            }
            let info = library.open(locked, project.dir()).ok();
            let info = info.as_ref().map(|p| p.info());
            listed.push(Listed {
                name: locked.dependency_name().to_string(),
                id: locked.id.clone(),
                version: locked.version.to_string(),
                label: locked.label(),
                dependency: !lock.roots().any(|r| std::ptr::eq(r, locked)),
                enabled: used,
                installed: library.is_installed(locked, project.dir()),
                abbreviation: info.map(|i| project.names(locked, i).0),
                title: info.map(|i| i.name.clone()),
                license: info.and_then(|i| i.license.clone()),
                git: locked.git.clone(),
                tag: locked.tag.clone(),
                branch: locked.branch.clone(),
                commit: locked.commit.clone(),
                path: locked.path.clone(),
                registry: locked.registry.clone(),
            });
        }
        return print_json(&listed);
    }
    for locked in &lock.packages {
        let root = if lock.roots().any(|r| std::ptr::eq(r, locked)) {
            ""
        } else {
            "  (a dependency)"
        };
        let used = in_use.contains(&locked.name());
        if only.is_some_and(|wanted| wanted != used) {
            continue;
        }
        let off = match used {
            true => "",
            false => "  (off)",
        };
        let source = match (&locked.git, &locked.path) {
            (Some(git), _) => format!(
                "{git} {}",
                locked
                    .tag
                    .as_deref()
                    .or(locked.branch.as_deref())
                    .unwrap_or("")
            ),
            (_, Some(path)) => format!("path {path}"),
            _ => match &locked.registry {
                Some(registry) => format!("registry {registry}"),
                None => String::new(),
            },
        };
        let installed = if library.is_installed(locked, project.dir()) {
            ""
        } else {
            "  NOT INSTALLED"
        };
        let about = library.open(locked, project.dir()).ok().map(|p| {
            let info = p.info();
            let (abbreviation, _) = project.names(locked, info);
            format!(
                "{} ({}), {}",
                abbreviation,
                info.name,
                info.license.as_deref().unwrap_or("no license given")
            )
        });
        outln!(
            "{}  {}  {}{root}{off}{installed}",
            locked.label(),
            about.unwrap_or_default(),
            source.trim()
        );
    }
    Ok(())
}

/// A locked package, as `lx info --format json` prints it
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoReport {
    name: String,
    id: String,
    version: String,
    label: String,
    abbreviation: String,
    aliases: Vec<String>,
    title: String,
    kind: logoxide::manifest::PackageKind,
    language: String,
    license: Option<String>,
    license_file: Option<String>,
    git: Option<String>,
    commit: Option<String>,
    path: Option<String>,
    registry: Option<String>,
    /// Where it is, how many files, and their bytes
    dir: String,
    files: usize,
    size: u64,
    checksum: String,
    locked_checksum: Option<String>,
    /// Its files are still the ones locked (true when the lock has no checksum)
    unchanged: bool,
    /// Files in its folder that aren't the package's (a reader's annotations)
    other_files: Vec<String>,
    books: usize,
    verses: usize,
    format: String,
}

pub(crate) fn info(cli: &Cli, id: &str) -> logoxide::Result<()> {
    let library = cli.library();
    let project = cli.project(&library)?;
    // The manifest's own, else one they brought in (a collection's)
    let locked = (project.lock())
        .and_then(|lock| {
            lock.get(id)
                .or_else(|| lock.packages.iter().find(|p| p.id == id))
        })
        .cloned()
        .ok_or_else(|| {
            logoxide::LogoxideError::NotFound(format!("{id} isn't in the lock file (lx install)"))
        })?;
    let dir = library.package_dir(&locked, project.dir());
    let package = library.open(&locked, project.dir())?;
    let i = package.info().clone();
    let (abbreviation, aliases) = project.names(&locked, &i);
    let storage = FsStorage::new(&dir);
    let files = storage.walk("")?;
    let size: u64 = (files.iter())
        .filter_map(|f| std::fs::metadata(dir.join(f)).ok())
        .map(|m| m.len())
        .sum();
    let checksum = package.checksum()?;
    let others = logoxide::package::other_files(&storage)?;
    let cached = library.book_cache().open(package)?;
    let books = &cached.manifest().books;
    let verses: usize = (books.iter())
        .flat_map(|(_, counts)| counts.iter().map(|&c| c as usize))
        .sum();
    if cli.format == Format::Json {
        return print_json(&InfoReport {
            name: locked.dependency_name().to_string(),
            id: locked.id.clone(),
            version: locked.version.to_string(),
            label: locked.label(),
            abbreviation,
            aliases,
            title: i.name.clone(),
            kind: i.kind,
            language: i.language.clone(),
            license: i.license.clone(),
            license_file: i.license_file.clone(),
            git: locked.git.clone(),
            commit: locked.commit.clone(),
            path: locked.path.clone(),
            registry: locked.registry.clone(),
            dir: dir.display().to_string(),
            files: files.len(),
            size,
            unchanged: locked.checksum.as_ref().is_none_or(|c| *c == checksum),
            checksum,
            locked_checksum: locked.checksum.clone(),
            other_files: others,
            books: books.len(),
            verses,
            format: cached.manifest().source_format.to_string(),
        });
    }
    outln!("{} ({}), {}", locked.label(), abbreviation, i.name);
    if !aliases.is_empty() {
        outln!("aliases: {}", aliases.join(", "));
    }
    outln!(
        "kind: {:?}, language: {}, license: {}",
        i.kind,
        i.language,
        (i.license.as_deref())
            .or(i.license_file.as_deref())
            .unwrap_or("none given")
    );
    let from = match (&locked.git, &locked.path) {
        (Some(git), _) => format!("{git} at {}", locked.commit.as_deref().unwrap_or("?")),
        (_, Some(path)) => format!("path {path}"),
        _ => String::from("?"),
    };
    outln!("from: {from}");
    outln!(
        "in: {} ({} files, {:.1} MB)",
        dir.display(),
        files.len(),
        size as f64 / 1e6
    );
    match &locked.checksum {
        Some(expected) if *expected == checksum => outln!("checksum: {checksum} (as locked)"),
        Some(expected) => outln!(
            "checksum: {checksum} (CHANGED: locked as {expected}; lx install --repair puts its files back)"
        ),
        None => outln!("checksum: {checksum}"),
    }
    if !others.is_empty() {
        outln!("not the package's (kept): {}", others.join(", "));
    }
    outln!(
        "{} books, {verses} verses ({})",
        books.len(),
        cached.manifest().source_format
    );
    Ok(())
}

pub(crate) fn clean(cli: &Cli, remove: bool, keep: &[PathBuf]) -> logoxide::Result<()> {
    let library = cli.library();
    let mut projects = vec![Project::open(library.root())?, cli.project(&library)?];
    for dir in keep {
        projects.push(Project::open(dir)?);
    }
    let used: Vec<(String, String)> = (projects.iter())
        .filter_map(|p| p.lock())
        .flat_map(|lock| lock.packages.iter())
        .filter(|p| p.path.is_none())
        .map(|p| (p.id.clone(), p.key()))
        .collect();
    let unused: Vec<(String, String)> = (library.installed().into_iter())
        .filter(|v| !used.contains(v))
        .collect();
    if unused.is_empty() {
        outln!("every installed version is used");
        return Ok(());
    }
    for (id, key) in &unused {
        let dir = library.root().join("packages").join(id).join(key);
        // Files that aren't the package's (a reader's annotations) are never removed: the folder
        // stays, and says why
        let others = logoxide::package::other_files(&FsStorage::new(&dir))?;
        if !others.is_empty() {
            outln!(
                "kept {id} {key}: {} file{} not the package's ({})",
                others.len(),
                if others.len() == 1 { " is" } else { "s are" },
                others.join(", ")
            );
            continue;
        }
        if remove {
            // Its parsed books are `<id>/<version>-<checksum>`: by the files' checksum, since
            // forks share versions
            let checksum = logoxide::package::checksum(&FsStorage::new(&dir))?;
            let short = checksum
                .trim_start_matches("sha256:")
                .get(..16)
                .unwrap_or_default();
            let version = key.split('+').next().unwrap_or(key);
            std::fs::remove_dir_all(&dir).map_err(|e| logoxide::LogoxideError::Io {
                path: dir.display().to_string(),
                message: e.to_string(),
            })?;
            let cache = library.cache_dir().join("books").join(id);
            let _ = std::fs::remove_dir_all(cache.join(format!("{version}-{short}")));
            // The id's folder too, once it has no versions (remove_dir only removes empty ones)
            let _ = std::fs::remove_dir(library.root().join("packages").join(id));
            outln!("removed {id} {key}");
        } else {
            outln!("unused: {id} {key} ({})", dir.display());
        }
    }
    if !remove {
        outln!(
            "{} unused (other vaults may use them: --keep <their folder>); lx clean --remove deletes them",
            unused.len()
        );
    }
    Ok(())
}
