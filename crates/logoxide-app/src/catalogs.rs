/*!
Catalogs and registries, read from where the project and logoxide's config say: a URL (through an
[`Http`]), a file, or a registry's folder of repositories. What a catalog passed over, and what
couldn't be read when others could, are given to a [`Warn`] as they're found, in the words lx
prints after `warning: `; nothing here prints. A catalog at a URL is kept in a [`CatalogCache`]
when the [`CatalogFetch`] has one, and used from there as its [`Freshness`] allows; a kept copy
used because the download failed is said in the catalog's `warnings`
*/

use std::path::{Path, PathBuf};

use logoxide::{
    LogoxideError, Project, Result,
    catalog::{Catalog, CatalogSource},
    config::Config,
};

use crate::{
    Warn,
    catalog_cache::{CatalogCache, Freshness, age_words},
    http::Http,
};

/// How catalogs at URLs are had: downloaded through `http`, kept in `cache` when there's one
/// (none keeps nothing), and used from it as `freshness` allows
#[derive(Clone, Copy)]
pub struct CatalogFetch<'a> {
    pub http: &'a dyn Http,
    pub cache: Option<&'a CatalogCache>,
    pub freshness: Freshness,
}

impl<'a> CatalogFetch<'a> {
    /// Every catalog downloaded each time, none kept (the bindings, whose host does the I/O)
    pub fn uncached(http: &'a dyn Http) -> CatalogFetch<'a> {
        CatalogFetch {
            http,
            cache: None,
            freshness: Freshness::Fresh,
        }
    }

    pub fn new(
        http: &'a dyn Http,
        cache: Option<&'a CatalogCache>,
        freshness: Freshness,
    ) -> CatalogFetch<'a> {
        CatalogFetch {
            http,
            cache,
            freshness,
        }
    }

    /// The same, with another freshness
    pub fn with(self, freshness: Freshness) -> CatalogFetch<'a> {
        CatalogFetch { freshness, ..self }
    }
}

/// Whether a catalog's location is a URL it's downloaded from (and so may be kept)
pub fn is_url(location: &str) -> bool {
    location.starts_with("https://") || location.starts_with("http://")
}

/**
Each catalog that can be read, in order, each one's passed-over entries warned of; one that
can't be read is a warning, unless none can be
*/
pub fn load_catalogs(
    fetch: CatalogFetch,
    locations: &[String],
    base: &Path,
    warn: Warn,
) -> Result<Vec<Catalog>> {
    let mut catalogs = vec![];
    for location in locations {
        match load_catalog(fetch, location, base) {
            Ok(catalog) => {
                catalog.warnings.iter().cloned().for_each(&mut *warn);
                catalogs.push(catalog);
            }
            Err(e) if locations.len() > 1 => warn(e.to_string()),
            Err(e) => return Err(e),
        }
    }
    if catalogs.is_empty() {
        return Err(LogoxideError::Catalog(String::from(
            "no catalog could be read",
        )));
    }
    Ok(catalogs)
}

/**
A catalog at a URL (an `http://` one is said by the [`Http`], which sees where it was
redirected) or a path relative to `base`, in any publisher's form
([`logoxide_import::catalog::parse`]); its `warnings` say what it passed over. One at a URL is
taken from the cache as `fetch` allows, else downloaded and kept; when downloading (or reading
what came) fails, a copy kept is used, of any age, and its `warnings` say so
*/
pub fn load_catalog(fetch: CatalogFetch, location: &str, base: &Path) -> Result<Catalog> {
    if !is_url(location) {
        let path = base.join(location);
        let bytes =
            std::fs::read(&path).map_err(|e| LogoxideError::io(&path.display().to_string(), e))?;
        return logoxide_import::catalog::parse(location, &bytes);
    }
    let parse = |bytes: &[u8]| logoxide_import::catalog::parse(location, bytes);
    let Some(cache) = fetch.cache else {
        if fetch.freshness == Freshness::Offline {
            return Err(LogoxideError::Catalog(format!(
                "{location}: not downloaded offline, and no catalog is kept (catalog-max-age is 0)"
            )));
        }
        return parse(&fetch.http.get(location)?);
    };
    let kept = cache.read(location);
    if let Some(kept) = &kept {
        let usable = match fetch.freshness {
            Freshness::Fresh => cache.is_fresh(kept),
            Freshness::Offline => true,
            Freshness::Refresh => false,
        };
        // One that no longer parses (logoxide changed, or the file did) is downloaded again
        if usable && let Ok(catalog) = parse(&kept.bytes) {
            return Ok(catalog);
        }
    }
    if fetch.freshness == Freshness::Offline {
        return Err(LogoxideError::Catalog(format!(
            "{location}: not downloaded offline, and not kept (lx catalog refresh)"
        )));
    }
    let downloaded = (fetch.http.get(location)).and_then(|bytes| Ok((parse(&bytes)?, bytes)));
    match downloaded {
        Ok((mut catalog, bytes)) => {
            if let Err(e) = cache.store(location, &bytes) {
                catalog
                    .warnings
                    .push(format!("{location} couldn't be kept: {e}"));
            }
            Ok(catalog)
        }
        Err(e) => {
            let Some((mut catalog, kept)) = kept.and_then(|k| Some((parse(&k.bytes).ok()?, k)))
            else {
                return Err(e);
            };
            catalog.warnings.push(format!(
                "{location}: {e}; the copy downloaded {} ago is used",
                age_words(cache.age(&kept))
            ));
            Ok(catalog)
        }
    }
}

/**
A catalog at a URL downloaded again and, when it parses, kept (a kept copy is never used:
failing is an error); `cache` none only downloads and reads it
*/
pub fn refresh_catalog(
    http: &dyn Http,
    cache: Option<&CatalogCache>,
    url: &str,
) -> Result<Catalog> {
    let bytes = http.get(url)?;
    let catalog = logoxide_import::catalog::parse(url, &bytes)?;
    if let Some(cache) = cache {
        cache.store(url, &bytes)?;
    }
    Ok(catalog)
}

/// A catalog or registry downloaded again by [`refresh`]
#[derive(Debug)]
pub struct Refreshed {
    /// The registry's name, for a registry
    pub registry: Option<String>,
    pub url: String,
    /// The catalog read, or why it couldn't be
    pub outcome: Result<Catalog>,
}

/**
The project's catalogs and registries at URLs (its own and the config's, and the default
catalogs unless it turns them off), downloaded again and kept: every one, or those `only` names
(by URL or registry name; a URL the project doesn't search is downloaded too). A name that's
neither is an error, before anything is downloaded. Catalogs and registries in files and folders
are read each time, so they're never here
*/
pub fn refresh(
    http: &dyn Http,
    cache: Option<&CatalogCache>,
    config: &Config,
    project: &Project,
    only: &[String],
) -> Result<Vec<Refreshed>> {
    let mut all: Vec<(Option<String>, String)> = vec![];
    for (name, location) in config.registries(project.manifest()) {
        if is_url(&location) {
            all.push((Some(name), location));
        }
    }
    for location in config.catalogs(project.manifest()) {
        if is_url(&location) && !all.iter().any(|(_, url)| url == &location) {
            all.push((None, location));
        }
    }
    if !only.is_empty() {
        let mut chosen = vec![];
        for name in only {
            let found = (all.iter())
                .find(|(registry, url)| url == name || registry.as_deref() == Some(name.as_str()));
            let pick = match found {
                Some(found) => found.clone(),
                None if is_url(name) => (None, name.clone()),
                None => {
                    return Err(LogoxideError::Catalog(format!(
                        "`{name}` isn't a catalog or registry this project downloads (lx catalog list)"
                    )));
                }
            };
            if !chosen.contains(&pick) {
                chosen.push(pick);
            }
        }
        all = chosen;
    }
    Ok((all.into_iter())
        .map(|(registry, url)| {
            let outcome = refresh_catalog(http, cache, &url).map(|mut catalog| {
                if let Some(name) = &registry {
                    catalog.name = name.clone();
                }
                catalog
            });
            Refreshed {
                registry,
                url,
                outcome,
            }
        })
        .collect())
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
    fetch: CatalogFetch,
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
            && let Ok(other) = load_registry(fetch, &other, &at, project.dir())
        {
            catalogs.push(other);
        }
    }
    shadowed(&catalogs, id, &name)
}

/**
A package's git URL from what's typed: a name through one of the project's registries
(`dorean-local:books/romans`), `github:owner/repo`, or a URL. Warned of: what the registry's
catalog passed over, and another registry listing the same id
*/
pub fn registry_url(
    fetch: CatalogFetch,
    config: &Config,
    project: &Project,
    text: &str,
    warn: Warn,
) -> Result<String> {
    let registries = config.registries(project.manifest());
    let Some((registry, name)) = logoxide::catalog::split_registry_name(text, &registries) else {
        return Ok(logoxide::git::expand_url(text));
    };
    let catalog = load_registry(fetch, registry, &registries[registry], project.dir())?;
    catalog.warnings.iter().cloned().for_each(&mut *warn);
    let entry = catalog.find(name)?.ok_or_else(|| {
        LogoxideError::Catalog(format!(
            "{registry} has no package `{name}` (lx search --registry {registry})"
        ))
    })?;
    if let Some(warning) = other_registries(
        fetch,
        config,
        project,
        &catalog,
        &registries[registry],
        &entry.id,
    ) {
        warn(warning);
    }
    match &entry.source {
        CatalogSource::Git { url } => Ok(url.clone()),
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
pub fn load_registry(
    fetch: CatalogFetch,
    name: &str,
    location: &str,
    base: &Path,
) -> Result<Catalog> {
    let is_url = location.contains("://");
    let folder = registry_path(location, base);
    if is_url || !folder.is_dir() {
        let mut catalog = match is_url {
            true => load_catalog(fetch, location, base)?,
            // By its whole path, so a dependency names it from anywhere
            false => load_catalog(
                fetch,
                &canonical(&folder)?.display().to_string(),
                Path::new(""),
            )?,
        };
        catalog.rename(name);
        return Ok(catalog);
    }
    let (mut catalog, warnings) =
        logoxide::publish::registry_build::folder_catalog(name, &canonical(&folder)?)?;
    catalog.warnings.extend(warnings);
    Ok(catalog)
}

fn canonical(path: &Path) -> Result<PathBuf> {
    (path.canonicalize()).map_err(|e| LogoxideError::io(&path.display().to_string(), e))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::*;

    /// A new empty folder of its own for a test
    pub(crate) fn scratch(name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "logoxide-app-{name}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Answers each URL from a table (failing for one it lacks), counting what's asked
    #[derive(Default)]
    struct Table {
        answers: Mutex<HashMap<String, Vec<u8>>>,
        asked: AtomicUsize,
    }

    impl Table {
        fn set(&self, url: &str, text: Option<&str>) {
            let mut answers = self.answers.lock().unwrap();
            match text {
                Some(text) => answers.insert(url.to_string(), text.as_bytes().to_vec()),
                None => answers.remove(url),
            };
        }

        fn asked(&self) -> usize {
            self.asked.load(Ordering::Relaxed)
        }
    }

    impl Http for Table {
        fn get(&self, url: &str) -> Result<Vec<u8>> {
            self.asked.fetch_add(1, Ordering::Relaxed);
            (self.answers.lock().unwrap().get(url).cloned())
                .ok_or_else(|| LogoxideError::network(url, "unreachable"))
        }
    }

    const URL: &str = "https://example.org/catalog.toml";

    fn catalog(id: &str) -> String {
        format!(
            "[[package]]\nid = \"{id}\"\nname = \"N\"\nabbreviation = \"N\"\nlanguage = \"en\"\n\
             git = \"https://example.org/{id}\"\n"
        )
    }

    fn ids(catalog: &Catalog) -> Vec<&str> {
        catalog.entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn a_kept_catalog_is_used_while_its_young() {
        let dir = scratch("fresh");
        let cache = CatalogCache::new(&dir, Duration::from_secs(3600)).unwrap();
        let http = Table::default();
        http.set(URL, Some(&catalog("one")));
        let fetch = CatalogFetch::new(&http, Some(&cache), Freshness::Fresh);
        let base = Path::new("/");
        assert_eq!(ids(&load_catalog(fetch, URL, base).unwrap()), ["one"]);
        assert_eq!(http.asked(), 1);
        // Kept: not downloaded again, even when the publisher's has changed
        http.set(URL, Some(&catalog("two")));
        assert_eq!(ids(&load_catalog(fetch, URL, base).unwrap()), ["one"]);
        assert_eq!(http.asked(), 1);
        // Refreshing downloads it, and keeps what came
        let refreshed = load_catalog(fetch.with(Freshness::Refresh), URL, base).unwrap();
        assert_eq!(ids(&refreshed), ["two"]);
        assert_eq!(http.asked(), 2);
        assert_eq!(ids(&load_catalog(fetch, URL, base).unwrap()), ["two"]);
        assert_eq!(http.asked(), 2);
        // Too old: downloaded again
        let old = cache.read(URL).unwrap().fetched - 3601;
        cache.store_at(URL, catalog("old").as_bytes(), old).unwrap();
        http.set(URL, Some(&catalog("three")));
        assert_eq!(ids(&load_catalog(fetch, URL, base).unwrap()), ["three"]);
        assert_eq!(http.asked(), 3);
        // Offline: what's kept, of any age, and nothing downloaded
        cache.store_at(URL, catalog("old").as_bytes(), old).unwrap();
        let offline = fetch.with(Freshness::Offline);
        assert_eq!(ids(&load_catalog(offline, URL, base).unwrap()), ["old"]);
        let other = "https://example.org/other.toml";
        let error = load_catalog(offline, other, base).unwrap_err();
        assert!(error.to_string().contains("lx catalog refresh"), "{error}");
        assert_eq!(http.asked(), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_old_copy_is_used_when_downloading_fails() {
        let dir = scratch("fallback");
        let cache = CatalogCache::new(&dir, Duration::from_secs(60)).unwrap();
        let http = Table::default();
        let fetch = CatalogFetch::new(&http, Some(&cache), Freshness::Fresh);
        let base = Path::new("/");
        // Nothing kept: the failure is the error
        assert!(load_catalog(fetch, URL, base).is_err());
        assert_eq!(cache.read(URL), None);
        let fetched = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 3 * 86400;
        cache
            .store_at(URL, catalog("kept").as_bytes(), fetched)
            .unwrap();
        let loaded = load_catalog(fetch, URL, base).unwrap();
        assert_eq!(ids(&loaded), ["kept"]);
        assert_eq!(loaded.warnings.len(), 1);
        assert!(
            loaded.warnings[0].contains("the copy downloaded 3 days ago is used"),
            "{:?}",
            loaded.warnings
        );
        // Refreshing too; what came that isn't a catalog is never kept
        http.set(URL, Some("not = [a catalog"));
        let loaded = load_catalog(fetch.with(Freshness::Refresh), URL, base).unwrap();
        assert_eq!(ids(&loaded), ["kept"]);
        assert_eq!(cache.read(URL).unwrap().fetched, fetched);
        // But `refresh_catalog` fails
        assert!(refresh_catalog(&http, Some(&cache), URL).is_err());
        http.set(URL, Some(&catalog("new")));
        assert_eq!(
            ids(&refresh_catalog(&http, Some(&cache), URL).unwrap()),
            ["new"]
        );
        assert_eq!(cache.read(URL).unwrap().bytes, catalog("new").as_bytes());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn without_a_cache_every_catalog_is_downloaded() {
        let http = Table::default();
        http.set(URL, Some(&catalog("one")));
        let fetch = CatalogFetch::uncached(&http);
        for _ in 0..2 {
            assert_eq!(
                ids(&load_catalog(fetch, URL, Path::new("/")).unwrap()),
                ["one"]
            );
        }
        assert_eq!(http.asked(), 2);
        assert!(load_catalog(fetch.with(Freshness::Offline), URL, Path::new("/")).is_err());
        assert_eq!(http.asked(), 2);
    }

    #[test]
    fn refreshes_the_projects_catalogs_at_urls() {
        let dir = scratch("refresh");
        let registry = "https://example.org/registry.toml";
        std::fs::write(dir.join("local.toml"), catalog("local")).unwrap();
        std::fs::write(
            dir.join("logoxide.toml"),
            format!(
                "[settings]\ndefault-catalogs = false\ncatalogs = [\"{URL}\", \"local.toml\"]\n\
                 [settings.registries]\nmine = \"{registry}\"\n"
            ),
        )
        .unwrap();
        let project = Project::open(&dir).unwrap();
        let cache = CatalogCache::new(dir.join("cache"), Duration::from_secs(60)).unwrap();
        let http = Table::default();
        http.set(URL, Some(&catalog("one")));
        let config = Config::default();
        let all = refresh(&http, Some(&cache), &config, &project, &[]).unwrap();
        let urls: Vec<_> = all.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, [registry, URL]);
        assert_eq!(all[0].registry.as_deref(), Some("mine"));
        assert!(all[0].outcome.is_err());
        assert_eq!(ids(all[1].outcome.as_ref().unwrap()), ["one"]);
        assert!(cache.read(URL).is_some());
        // By registry name or URL; anything else is refused before downloading
        http.set(registry, Some(&catalog("reg")));
        let named = refresh(&http, Some(&cache), &config, &project, &["mine".into()]).unwrap();
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].outcome.as_ref().unwrap().name, "mine");
        let asked = http.asked();
        let unknown = ["local.toml".to_string()];
        assert!(refresh(&http, Some(&cache), &config, &project, &unknown).is_err());
        assert_eq!(http.asked(), asked);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
