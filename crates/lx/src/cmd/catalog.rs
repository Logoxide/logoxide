//! Catalogs and registries: reading them, searching them, and the project's list of them

use std::path::{Path, PathBuf};

use logoxide::{MemoryStorage, Storage, manifest::MANIFEST};
use logoxide_app::catalog_cache::Freshness;

use crate::{
    app, book_names,
    cli::{Cli, Format},
    config, print_json, warn,
};

/// Each catalog that can be read, in order, those at URLs from the cache as `freshness` allows;
/// one that can't be read is a warning, unless none can be
pub(crate) fn load_catalogs(
    locations: &[String],
    base: &Path,
    freshness: Freshness,
) -> logoxide::Result<Vec<logoxide::catalog::Catalog>> {
    let fetch = app().catalog_fetch(freshness);
    logoxide_app::catalogs::load_catalogs(fetch, locations, base, &mut warn())
}

/**
Warns when another of the project's registries also lists `id`, which `registry` (the catalog at
`location` a package is being added from, one of the project's or `--registry`'s) gives: two
sources answer to that id, and only the one named is used
*/
pub(crate) fn warn_other_registries(
    project: &logoxide::Project,
    registry: &logoxide::catalog::Catalog,
    location: &str,
    id: &str,
) {
    let fetch = app().catalog_fetch(Freshness::Fresh);
    let other =
        logoxide_app::catalogs::other_registries(fetch, config(), project, registry, location, id);
    if let Some(warning) = other {
        errln!("warning: {warning}");
    }
}

/// A catalog, warning of the entries it passed over
pub(crate) fn load_catalog(
    location: &str,
    base: &Path,
    freshness: Freshness,
) -> logoxide::Result<logoxide::catalog::Catalog> {
    let fetch = app().catalog_fetch(freshness);
    let catalog = logoxide_app::catalogs::load_catalog(fetch, location, base)?;
    for warning in &catalog.warnings {
        errln!("warning: {warning}");
    }
    Ok(catalog)
}

/**
A package's git URL from what's typed: a name through one of the project's registries
(`dorean-local:books/romans`), `github:owner/repo`, or a URL. A registry at a URL is downloaded
again, so what was published since it was kept is found
*/
pub(crate) fn registry_url(project: &logoxide::Project, text: &str) -> logoxide::Result<String> {
    let fetch = app().catalog_fetch(Freshness::Refresh);
    logoxide_app::catalogs::registry_url(fetch, config(), project, text, &mut warn())
}

/// [`registry_url`] in the project `cli` names (`lx show --git dorean-local:kjv`)
pub(crate) fn package_url(cli: &Cli, text: &str) -> logoxide::Result<String> {
    if !text.contains(':') || text.contains("://") {
        return Ok(logoxide::git::expand_url(text));
    }
    registry_url(&cli.project(&cli.library())?, text)
}

/**
A registry's packages: a folder's repositories (every git repository with a `logoxide.toml`, at any
depth, named by its path there), else a catalog at that URL or path (from the cache as
`freshness` allows); what it passed over is warned of when `warn`
*/
pub(crate) fn load_registry(
    name: &str,
    location: &str,
    base: &Path,
    warn: bool,
    freshness: Freshness,
) -> logoxide::Result<logoxide::catalog::Catalog> {
    let fetch = app().catalog_fetch(freshness);
    let catalog = logoxide_app::catalogs::load_registry(fetch, name, location, base)?;
    if warn {
        for warning in &catalog.warnings {
            errln!("warning: {warning}");
        }
    }
    Ok(catalog)
}

fn canonical(path: &Path) -> logoxide::Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| logoxide::LogoxideError::io(&path.display().to_string(), e))
}

/**
`lx catalog build`: a catalog.toml of a folder's package repositories, each from its newest
release, with its path and commit; an entry whose path, commit, and URL are unchanged is kept as
it was (unless `force`)
*/
pub(crate) fn catalog_build(
    dir: &Path,
    output: Option<&Path>,
    url: Option<&str>,
    name: Option<&str>,
    force: bool,
) -> logoxide::Result<()> {
    let folder = canonical(dir)?;
    let output = output.map_or_else(|| folder.join("catalog.toml"), Path::to_path_buf);
    let previous = match force {
        true => None,
        false => std::fs::read_to_string(&output).ok(),
    };
    let built =
        logoxide::publish::registry_build::build_catalog(&folder, previous.as_deref(), url, name)
            .map_err(|e| match e {
            logoxide::LogoxideError::Catalog(message) => {
                logoxide::LogoxideError::Catalog(format!("{}: {message}", output.display()))
            }
            e => e,
        })?;
    logoxide::storage::write_atomic(&output, built.text.as_bytes())
        .map_err(|e| logoxide::LogoxideError::io(&output.display().to_string(), e))?;
    let plural = if built.count == 1 { "" } else { "s" };
    outln!(
        "Wrote {}: {} package{plural} ({} read, {} unchanged)",
        output.display(),
        built.count,
        built.read,
        built.kept
    );
    for skip in built.skipped {
        outln!("    skipped {skip}");
    }
    if built.works > 0 {
        let plural = if built.works == 1 { "" } else { "s" };
        outln!("    {} work{plural} named", built.works);
    }
    for warning in built.warnings {
        errln!("warning: {warning}");
    }
    Ok(())
}

pub(crate) fn catalog_list(cli: &Cli) -> logoxide::Result<()> {
    let project = cli.project(&cli.library())?;
    // Where each comes from: the project, logoxide's config, or the defaults
    let own = project.manifest().registries();
    let json = cli.format == Format::Json;
    let mut registries = vec![];
    for (name, location) in config().registries(project.manifest()) {
        let from = match own.contains_key(&name) {
            true => "",
            false => ", from config.toml",
        };
        if !json {
            outln!("{name}: {location} (registry{from})");
        }
        registries.push(serde_json::json!({
            "name": name,
            "location": location,
            "from": if from.is_empty() { "project" } else { "config" },
        }));
    }
    let defaults = logoxide::catalog::DEFAULT_CATALOGS;
    let mine = project
        .manifest()
        .settings
        .clone()
        .unwrap_or_default()
        .catalogs;
    let mut catalogs = vec![];
    for location in config().catalogs(project.manifest()) {
        let (note, from) = if defaults.contains(&location.as_str()) {
            (" (default)", "default")
        } else if !mine.contains(&location) {
            (" (from config.toml)", "config")
        } else {
            ("", "project")
        };
        if !json {
            outln!("{location}{note}");
        }
        catalogs.push(serde_json::json!({ "location": location, "from": from }));
    }
    if json {
        print_json(&serde_json::json!({ "registries": registries, "catalogs": catalogs }))?;
    }
    Ok(())
}

/// `lx catalog refresh`: the project's catalogs and registries at URLs (or those named)
/// downloaded again and kept, each said refreshed or failed
pub(crate) fn catalog_refresh(cli: &Cli, only: &[String]) -> logoxide::Result<()> {
    let project = cli.project(&cli.library())?;
    let app = app();
    let json = cli.format == Format::Json;
    if app.catalog_cache().is_none() && !json {
        errln!("warning: catalog-max-age is 0 in logoxide's config.toml: nothing is kept");
    }
    let refreshed = app.refresh_catalogs(&project, only)?;
    let mut failed = 0;
    let mut report = vec![];
    for one in &refreshed {
        let named = match &one.registry {
            Some(name) => format!("{name} ({})", one.url),
            None => one.url.clone(),
        };
        match &one.outcome {
            Ok(catalog) => {
                if !json {
                    let count = catalog.entries.len();
                    let plural = if count == 1 { "" } else { "s" };
                    outln!("Refreshed {named}: {count} package{plural}");
                    for warning in &catalog.warnings {
                        errln!("warning: {warning}");
                    }
                }
            }
            Err(e) => {
                failed += 1;
                if !json {
                    errln!("Failed {named}: {e}");
                }
            }
        }
        report.push(serde_json::json!({
            "url": one.url,
            "registry": one.registry,
            "refreshed": one.outcome.is_ok(),
            "packages": one.outcome.as_ref().ok().map(|c| c.entries.len()),
            "error": one.outcome.as_ref().err().map(|e| e.to_string()),
        }));
    }
    if json {
        print_json(&serde_json::Value::Array(report))?;
    } else if refreshed.is_empty() {
        outln!("No catalog or registry here is downloaded: nothing to refresh");
    }
    match failed {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Catalog(format!(
            "{n} of {} couldn't be refreshed",
            refreshed.len()
        ))),
    }
}

pub(crate) fn catalog_add(cli: &Cli, location: &str, name: Option<&str>) -> logoxide::Result<()> {
    let mut project = cli.project(&cli.library())?;
    if let Some(name) = name {
        logoxide::catalog::check_registry_name(name)?;
        let catalog = load_registry(name, location, project.dir(), true, Freshness::Refresh)?;
        if !project.add_registry(name, location)? {
            outln!("{name} is already {location}");
            return Ok(());
        }
        project.save_manifest()?;
        let count = catalog.entries.len();
        let plural = if count == 1 { "" } else { "s" };
        outln!("Added the registry {name} ({location}): {count} package{plural}");
        for entry in &catalog.entries {
            let path = entry.path.as_deref().unwrap_or(&entry.id);
            outln!(
                "    {name}:{path}  {} {}",
                entry.id,
                entry.version.as_deref().unwrap_or("")
            );
        }
        return Ok(());
    }
    let catalog = load_catalog(location, project.dir(), Freshness::Refresh)?;
    if !project.add_catalog(location)? {
        outln!("{location} is already searched");
        return Ok(());
    }
    project.save_manifest()?;
    let count = catalog.entries.len();
    let plural = if count == 1 { "" } else { "s" };
    outln!("Added {location}: {count} package{plural}");
    Ok(())
}

pub(crate) fn catalog_remove(cli: &Cli, location: &str) -> logoxide::Result<()> {
    let mut project = cli.project(&cli.library())?;
    if project.remove_registry(location)? {
        project.save_manifest()?;
        outln!("Removed the registry {location}");
        return Ok(());
    }
    if !project.remove_catalog(location)? {
        return Err(logoxide::LogoxideError::Catalog(format!(
            "{location} isn't searched (lx catalog list)"
        )));
    }
    project.save_manifest()?;
    outln!("Removed {location}");
    Ok(())
}

/// What a catalog entry is, wherever it's listed: its repository, or its registry and id
fn source_key(entry: &logoxide::catalog::CatalogEntry) -> String {
    match &entry.source {
        logoxide::catalog::CatalogSource::Git { url } => url.clone(),
        logoxide::catalog::CatalogSource::Registry { catalog } => format!("{catalog}#{}", entry.id),
        other => other.import_name().unwrap_or_default().to_string(),
    }
}

/// An entry's edition, as `lx search` and `lx works` show it (` [1769-oxford]`, ` [2nd, 1885]`)
fn edition_words(entry: &logoxide::catalog::CatalogEntry) -> String {
    match (&entry.edition, entry.edition_year) {
        // An edition named by its year needn't say it twice
        (Some(edition), Some(year)) if edition.contains(&year.to_string()) => {
            format!(" [{edition}]")
        }
        (Some(edition), Some(year)) => format!(" [{edition}, {year}]"),
        (Some(edition), None) => format!(" [{edition}]"),
        (None, Some(year)) => format!(" [{year}]"),
        (None, None) => String::new(),
    }
}

/// What `lx search` keeps besides what its words match
pub(crate) struct Filters<'a> {
    pub(crate) books: logoxide::topos_config::BookFilters,
    pub(crate) category: Option<&'a str>,
}

/// How `lx search` shows what it finds: at most `limit`, as JSON, or as a list not grouped by
/// work
#[derive(Clone, Copy)]
pub(crate) struct Shown {
    pub(crate) limit: usize,
    pub(crate) json: bool,
    pub(crate) flat: bool,
}

/// `lx search`'s catalogs at URLs: from the cache while they're young enough, downloaded again
/// (`--refresh`), or only from the cache (`--offline`)
pub(crate) fn search_freshness(refresh: bool, offline: bool) -> Freshness {
    match (refresh, offline) {
        (true, _) => Freshness::Refresh,
        (_, true) => Freshness::Offline,
        _ => Freshness::Fresh,
    }
}

/// The project's registries and catalogs (or `catalogs`, or only `registry`) as one
pub(crate) struct Gathered {
    /// Every registry package (even two with one id: they differ by path), then the catalogs'
    pub(crate) all: logoxide::catalog::Catalog,
    /// Ids more than one catalog lists ([`logoxide::catalog::Catalog::shadowed`])
    pub(crate) shadowed: Vec<(String, Vec<String>)>,
    /// What names each registry's packages: `<registry>:<path>`, by their [`source_key`]
    pub(crate) through: std::collections::HashMap<String, String>,
    /// How many registries and catalogs were read
    pub(crate) searched: usize,
}

pub(crate) fn gather(
    cli: &Cli,
    catalogs: &[String],
    registry: Option<&str>,
    freshness: Freshness,
) -> logoxide::Result<Gathered> {
    // The project's registries come first, then its catalogs
    let mut registries = vec![];
    let (catalogs, base) = match catalogs.is_empty() {
        true => {
            let project = cli.project(&cli.library())?;
            let named = config().registries(project.manifest());
            if let Some(name) = registry
                && !named.contains_key(name)
            {
                return Err(logoxide::LogoxideError::Catalog(format!(
                    "no registry `{name}` (lx catalog list)"
                )));
            }
            for (name, location) in &named {
                if registry.is_some_and(|r| r != name) {
                    continue;
                }
                match load_registry(name, location, project.dir(), true, freshness) {
                    Ok(catalog) => registries.push(catalog),
                    Err(e) => errln!("warning: {name}: {e}"),
                }
            }
            let catalogs = match registry {
                Some(_) => vec![],
                None => config().catalogs(project.manifest()),
            };
            (catalogs, project.dir().to_path_buf())
        }
        false => (catalogs.to_vec(), PathBuf::from(".")),
    };
    let mut through: std::collections::HashMap<String, String> = Default::default();
    for catalog in &registries {
        for entry in &catalog.entries {
            let path = entry.path.as_deref().unwrap_or(&entry.id);
            through.insert(source_key(entry), format!("{}:{path}", catalog.name));
        }
    }
    let searched = registries.len() + catalogs.len();
    let mut all = logoxide::catalog::Catalog::default();
    for catalog in registries {
        all.entries.extend(catalog.entries);
        all.add_works(catalog.works);
    }
    let mut shadowed = vec![];
    if !catalogs.is_empty() {
        let loaded = load_catalogs(&catalogs, &base, freshness)?;
        shadowed = logoxide::catalog::Catalog::shadowed(&loaded);
        all = (loaded.into_iter()).fold(all, |all, c| all.merge(c));
    }
    Ok(Gathered {
        all,
        shadowed,
        through,
        searched,
    })
}

pub(crate) fn search(
    cli: &Cli,
    query: &str,
    catalogs: &[String],
    registry: Option<&str>,
    filters: Filters,
    shown: Shown,
    freshness: Freshness,
) -> logoxide::Result<()> {
    let Shown { limit, json, flat } = shown;
    // The books packages must be about, with the genres and names topos's config gives
    let books = match filters.books.is_empty() {
        true => None,
        false => {
            let config = logoxide::topos_config::read(None)?;
            (filters.books).codes(&logoxide::topos_config::bible_data(&config)?)?
        }
    };
    let Gathered {
        all,
        shadowed,
        through,
        searched,
    } = gather(cli, catalogs, registry, freshness)?;
    let mut found = all.search(query);
    found.retain(|e| {
        (books.as_ref()).is_none_or(|codes| e.about_books.iter().any(|b| codes.contains(b)))
            && (filters.category).is_none_or(|c| e.categories.iter().any(|x| x == c))
    });
    // A package another catalog also lists by its id: only the first catalog's is shown
    for (id, names) in &shadowed {
        if found.iter().take(limit).any(|e| &e.id == id) {
            errln!(
                "warning: `{id}` is in {}; {}'s is shown",
                names.join(", "),
                names[0]
            );
        }
    }
    if json {
        let entries: Vec<serde_json::Value> = (found.iter().take(limit))
            .map(|e| {
                let install = match (&e.source, e.source.import_name()) {
                    (_, Some(name)) => vec![String::from("import"), name.to_string()],
                    _ if through.contains_key(&source_key(e)) => {
                        vec![String::from("add"), through[&source_key(e)].clone()]
                    }
                    (logoxide::catalog::CatalogSource::Git { url }, None) => {
                        vec![String::from("add"), String::from("--git"), url.clone()]
                    }
                    (logoxide::catalog::CatalogSource::Registry { catalog }, None) => vec![
                        String::from("add"),
                        String::from("--registry"),
                        catalog.clone(),
                        e.id.clone(),
                    ],
                    (_, None) => vec![],
                };
                serde_json::json!({
                    "id": e.id,
                    "abbreviation": e.abbreviation,
                    "name": e.name,
                    "language": e.language_name.as_deref().unwrap_or(&e.language),
                    "languageTag": e.language,
                    "script": e.script,
                    "license": e.license,
                    "rights": e.text_rights(),
                    "keywords": e.keywords,
                    "categories": e.categories,
                    "aboutBooks": e.about_books,
                    "work": e.work,
                    "edition": e.edition,
                    "editionYear": e.edition_year,
                    "install": install,
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(entries));
        return Ok(());
    }
    let print = |entry: &logoxide::catalog::CatalogEntry, indent: &str| {
        let language = entry.language_name.as_deref().unwrap_or(&entry.language);
        let source = match (&entry.source, entry.source.import_name()) {
            (_, Some(name)) => format!("lx import {name}"),
            _ if through.contains_key(&source_key(entry)) => {
                format!("lx add {}", through[&source_key(entry)])
            }
            (logoxide::catalog::CatalogSource::Git { url }, None) => format!("lx add --git {url}"),
            (logoxide::catalog::CatalogSource::Registry { catalog }, None) => {
                format!("lx add --registry {catalog} {}", entry.id)
            }
            (_, None) => String::new(),
        };
        let edition = edition_words(entry);
        outln!(
            "{indent}{} ({}): {}{edition}, {language}, {}",
            entry.abbreviation,
            entry.id,
            entry.name,
            entry.license.as_deref().unwrap_or("no license given"),
        );
        if !entry.about_books.is_empty() {
            outln!("{indent}    on {}", book_names(&entry.about_books));
        }
        outln!("{indent}    {source}");
    };
    let listed = found.iter().take(limit).copied();
    match flat {
        true => listed.for_each(|entry| print(entry, "")),
        // Each work's editions together, under its name, where there are two or more
        false => {
            for group in all.group_by_work(listed) {
                match (&group.name, &group.work, group.entries.len()) {
                    (Some(name), Some(work), n) if n > 1 => {
                        outln!("{name} (work `{work}`): {n} editions");
                        for entry in &group.entries {
                            print(entry, "  ");
                        }
                    }
                    _ => group.entries.iter().for_each(|entry| print(entry, "")),
                }
            }
        }
    }
    let more = found.len().saturating_sub(limit);
    outln!(
        "{} found in {} catalog(s){}",
        found.len(),
        searched,
        if more > 0 {
            format!(" ({more} more: --limit)")
        } else {
            String::new()
        }
    );
    Ok(())
}

/**
`lx works`: every work a package can say it's an edition of, as the project's registries and
catalogs (or `catalogs`, or `registry`) see them, with where each is named and how many packages
claim it (`packages`: which)
*/
pub(crate) fn works(
    cli: &Cli,
    query: &str,
    catalogs: &[String],
    registry: Option<&str>,
    packages: bool,
    json: bool,
    freshness: Freshness,
) -> logoxide::Result<()> {
    use logoxide::catalog::WorkFrom;
    let Gathered { all, .. } = gather(cli, catalogs, registry, freshness)?;
    // What gathering them passed over (two catalogs naming one work otherwise)
    for warning in &all.warnings {
        errln!("warning: {warning}");
    }
    let listing = all.works_listing(query);
    if json {
        let works: Vec<serde_json::Value> = (listing.iter())
            .map(|w| {
                let (from, catalog) = match &w.from {
                    WorkFrom::Logoxide => ("logoxide", None),
                    WorkFrom::Catalog(name) => ("catalog", Some(name)),
                    WorkFrom::Unnamed => ("unnamed", None),
                };
                let entries: Vec<serde_json::Value> = (w.entries.iter())
                    .map(|e| {
                        serde_json::json!({
                            "id": e.id,
                            "abbreviation": e.abbreviation,
                            "name": e.name,
                            "edition": e.edition,
                            "editionYear": e.edition_year,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "id": w.id,
                    "name": w.name,
                    "aliases": w.aliases,
                    "wikidata": w.wikidata,
                    "from": from,
                    "catalog": catalog,
                    "packages": entries,
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(works));
        return Ok(());
    }
    for work in &listing {
        let n = work.entries.len();
        let claimed = match n {
            0 => String::from("no packages"),
            1 => String::from("1 package"),
            n => format!("{n} packages"),
        };
        match (&work.name, &work.from) {
            (Some(name), WorkFrom::Logoxide) => {
                outln!("{}: {name} (logoxide's table), {claimed}", work.id)
            }
            (Some(name), WorkFrom::Catalog(catalog)) => {
                outln!("{}: {name} (named by {catalog}), {claimed}", work.id)
            }
            _ => outln!("{}: named by nothing, claimed by {claimed}", work.id),
        }
        if !work.aliases.is_empty() {
            outln!("    also {}", work.aliases.join(", "));
        }
        if let Some(qid) = &work.wikidata {
            outln!("    https://www.wikidata.org/wiki/{qid}");
        }
        if packages {
            for entry in &work.entries {
                let edition = edition_words(entry);
                outln!(
                    "    {} ({}): {}{edition}",
                    entry.abbreviation,
                    entry.id,
                    entry.name
                );
            }
        }
    }
    let count = |from: fn(&WorkFrom) -> bool| listing.iter().filter(|w| from(&w.from)).count();
    outln!(
        "{} work(s): {} logoxide's, {} the catalogs', {} named by nothing",
        listing.len(),
        count(|f| *f == WorkFrom::Logoxide),
        count(|f| matches!(f, WorkFrom::Catalog(_))),
        count(|f| *f == WorkFrom::Unnamed),
    );
    Ok(())
}

pub(crate) fn catalog_entry(cli: &Cli, url: &str) -> logoxide::Result<()> {
    let fetcher = cli.fetcher();
    let tags = fetcher.tags(url)?;
    let newest = (tags.into_iter())
        .filter_map(|(t, _)| Some((logoxide::resolve::tag_version(&t)?, t)))
        .filter(|(v, _)| v.pre.is_empty())
        .max();
    let reference = match newest {
        Some((_, tag)) => logoxide::git::GitRef::Tag(tag),
        None => logoxide::git::GitRef::Head,
    };
    let files = MemoryStorage::new();
    fetcher.fetch_files(url, &reference, &[MANIFEST], &files)?;
    let manifest = logoxide::Manifest::parse(&files.read_string(MANIFEST)?)?;
    let info = manifest
        .package
        .ok_or_else(|| logoxide::LogoxideError::Manifest(format!("{url}: no [package]")))?;
    out!("{}", logoxide::catalog::Catalog::toml_entry(&info, url));
    Ok(())
}
