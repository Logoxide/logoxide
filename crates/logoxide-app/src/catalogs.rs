/*!
Catalogs and registries, read from where the project and logoxide's config say: a URL (through an
[`Http`]), a file, or a registry's folder of repositories. What a catalog passed over, and what
couldn't be read when others could, come back as warnings in the words lx prints after
`warning: `; nothing here prints
*/

use std::path::{Path, PathBuf};

use logoxide::{
    LogoxideError, Project, Result,
    catalog::{Catalog, CatalogSource},
    config::Config,
};

use crate::http::Http;

/// Catalogs read, and what to warn of: each one's passed-over entries, and each that couldn't be
/// read when another could
#[derive(Debug, Default)]
pub struct Loaded {
    pub catalogs: Vec<Catalog>,
    pub warnings: Vec<String>,
}

/// Each catalog that can be read, in order; one that can't is a warning, unless none can be
pub fn load_catalogs(http: &dyn Http, locations: &[String], base: &Path) -> Result<Loaded> {
    let mut loaded = Loaded::default();
    for location in locations {
        match load_catalog(http, location, base) {
            Ok(catalog) => {
                loaded.warnings.extend(catalog.warnings.iter().cloned());
                loaded.catalogs.push(catalog);
            }
            Err(e) if locations.len() > 1 => loaded.warnings.push(e.to_string()),
            Err(e) => return Err(e),
        }
    }
    if loaded.catalogs.is_empty() {
        return Err(LogoxideError::Catalog(String::from(
            "no catalog could be read",
        )));
    }
    Ok(loaded)
}

/**
A catalog at a URL (an `http://` one is said by the [`Http`], which sees where it was
redirected) or a path relative to `base`, in any publisher's form
([`logoxide_import::catalog::parse`]); its `warnings` say what it passed over
*/
pub fn load_catalog(http: &dyn Http, location: &str, base: &Path) -> Result<Catalog> {
    let bytes = if location.starts_with("https://") || location.starts_with("http://") {
        http.get(location)?
    } else {
        let path = base.join(location);
        std::fs::read(&path).map_err(|e| LogoxideError::io(&path.display().to_string(), e))?
    };
    logoxide_import::catalog::parse(location, &bytes)
}

/// The warning when more than one of `catalogs` lists `id` ([`Catalog::shadowed`]), naming
/// `used`, the one whose entry is taken
pub fn shadowed(catalogs: &[Catalog], id: &str, used: &str) -> Option<String> {
    let shadowed = Catalog::shadowed(catalogs);
    let (_, names) = shadowed.iter().find(|(shadowed, _)| shadowed == id)?;
    Some(format!(
        "`{id}` is in {}; {used}'s is used",
        names.join(", ")
    ))
}

/**
The warning when another of the project's registries also lists `id`, which `registry` (the
catalog at `location` a package is being added from, one of the project's or `--registry`'s)
gives: two sources answer to that id, and only the one named is used
*/
pub fn other_registries(
    http: &dyn Http,
    config: &Config,
    project: &Project,
    registry: &Catalog,
    location: &str,
    id: &str,
) -> Option<String> {
    let registries = config.registries(project.manifest());
    let mut used = registry.clone();
    if !registries.contains_key(&used.name) {
        used.name = location.to_string();
    }
    let name = used.name.clone();
    let mut catalogs = vec![used];
    for (other, at) in registries {
        if other != name
            && let Ok(other) = load_registry(http, &other, &at, project.dir())
        {
            catalogs.push(other);
        }
    }
    shadowed(&catalogs, id, &name)
}

/// A package's git URL, and what to warn of on the way
#[derive(Debug)]
pub struct Resolved {
    pub url: String,
    pub warnings: Vec<String>,
}

/**
A package's git URL from what's typed: a name through one of the project's registries
(`dorean-local:books/romans`), `github:owner/repo`, or a URL
*/
pub fn registry_url(
    http: &dyn Http,
    config: &Config,
    project: &Project,
    text: &str,
) -> Result<Resolved> {
    let registries = config.registries(project.manifest());
    let Some((registry, name)) = logoxide::catalog::split_registry_name(text, &registries) else {
        return Ok(Resolved {
            url: logoxide::git::expand_url(text),
            warnings: vec![],
        });
    };
    let catalog = load_registry(http, registry, &registries[registry], project.dir())?;
    let mut warnings = catalog.warnings.clone();
    let entry = catalog.find(name)?.ok_or_else(|| {
        LogoxideError::Catalog(format!(
            "{registry} has no package `{name}` (lx search --registry {registry})"
        ))
    })?;
    warnings.extend(other_registries(
        http,
        config,
        project,
        &catalog,
        &registries[registry],
        &entry.id,
    ));
    match &entry.source {
        CatalogSource::Git { url } => Ok(Resolved {
            url: url.clone(),
            warnings,
        }),
        CatalogSource::Registry { .. } => Err(LogoxideError::Catalog(format!(
            "{text} is published as archives, not in a git repository"
        ))),
        other => Err(LogoxideError::Catalog(format!(
            "{text} is imported, not added: lx import {}",
            other.import_name().unwrap_or_default()
        ))),
    }
}

/// A registry's location as a path: `~/` is the home folder, a relative one the project's
fn registry_path(location: &str, base: &Path) -> PathBuf {
    match (location.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => base.join(location),
    }
}

/**
A registry's packages: a folder's repositories (every git repository with a `logoxide.toml`, at
any depth, named by its path there), else a catalog at that URL or path. Its `warnings` say what
was passed over
*/
pub fn load_registry(http: &dyn Http, name: &str, location: &str, base: &Path) -> Result<Catalog> {
    let is_url = location.contains("://");
    let folder = registry_path(location, base);
    if is_url || !folder.is_dir() {
        let mut catalog = match is_url {
            true => load_catalog(http, location, base)?,
            // By its whole path, so a dependency names it from anywhere
            false => load_catalog(
                http,
                &canonical(&folder)?.display().to_string(),
                Path::new(""),
            )?,
        };
        catalog.name = name.to_string();
        return Ok(catalog);
    }
    let (mut catalog, warnings) =
        logoxide::registry_build::folder_catalog(name, &canonical(&folder)?)?;
    catalog.warnings.extend(warnings);
    Ok(catalog)
}

fn canonical(path: &Path) -> Result<PathBuf> {
    (path.canonicalize()).map_err(|e| LogoxideError::io(&path.display().to_string(), e))
}
