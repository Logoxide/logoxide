/*!
What every logoxide front end does (lx, the bindings, a language or MCP server), in one place:
catalogs and registries read over HTTP ([`catalogs`]), packages fetched with or without `git`
([`fetch`]), imports made, kept in the library, and updated ([`import`], [`importer`]), libraries cleaned ([`clean`]), media checked
([`media`]), and passages read and compared ([`read`]), behind one [`Logoxide`].

It sits above the package manager (`logoxide`) and the importers (`logoxide-import`), and prints
nothing: downloads and plain-HTTP warnings are said through a [`Notes`] callback, and what to warn
of (a catalog's passed-over entries, an id two catalogs list) to a [`Warn`] as it's found, in the
words lx prints after `warning: `. Downloads go through an
[`Http`](http::Http): [`http::UreqHttp`] with the `ureq` feature (on by default), else the host's
own, so WebAssembly builds without a network stack
*/

use std::{path::PathBuf, sync::Arc};

use logoxide::{
    Library, Lockfile, Project, Result, TextReader,
    config::Config,
    git::Fetcher,
    resolve::{Frozen, Update},
};

pub mod catalog_cache;
pub mod catalogs;
pub mod clean;
pub mod fetch;
pub mod http;
pub mod import;
pub mod importer;
pub mod media;
pub mod read;

pub use logoxide_text::passage::{Style, Text};

/// Something said while working, which a front end shows or not
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Note {
    /// A download starting
    Downloading { url: String },
    /// A download over plain HTTP (perhaps redirected there): what it sends can't be checked
    Insecure {
        url: String,
        redirected_to: Option<String>,
    },
    /// A repository fetched without encryption (`git://`, `http://`)
    Unencrypted { url: String },
}

impl Note {
    /// Whether it's a warning (else it's news)
    pub fn is_warning(&self) -> bool {
        !matches!(self, Note::Downloading { .. })
    }
}

/// In lx's words (a warning without its `warning: `)
impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Note::Downloading { url } => write!(f, "Downloading {url}"),
            Note::Insecure {
                url,
                redirected_to: None,
            } => write!(f, "{url} is plain HTTP: what it sends can't be checked"),
            Note::Insecure {
                url,
                redirected_to: Some(at),
            } => write!(
                f,
                "{url} (redirected to {at}) is plain HTTP: what it sends can't be checked"
            ),
            Note::Unencrypted { url } => write!(
                f,
                "{url} isn't encrypted: what it sends could be changed on the way"
            ),
        }
    }
}

/// Where [`Note`]s go
pub type Notes = Arc<dyn Fn(&Note) + Send + Sync>;

/// Where warnings go as they're found, each in words (without `warning: `)
pub type Warn<'a> = &'a mut dyn FnMut(String);

/// Notes that go nowhere
pub fn quiet() -> Notes {
    Arc::new(|_: &Note| {})
}

/**
A front end's logoxide: its config, library (and cache), downloads, fetcher, and text reader,
with what's done with them. Every method is one of the modules' functions with these filled in
*/
pub struct Logoxide {
    config: Config,
    library: Library,
    http: Arc<dyn http::Http>,
    fetcher: Box<dyn Fetcher>,
    reader: Box<dyn TextReader>,
    /// Catalogs downloaded over HTTP, kept for the config's `catalog-max-age` (none at zero)
    catalog_cache: Option<catalog_cache::CatalogCache>,
}

impl Logoxide {
    pub fn new(
        config: Config,
        library: Library,
        http: Arc<dyn http::Http>,
        fetcher: Box<dyn Fetcher>,
        reader: Box<dyn TextReader>,
    ) -> Logoxide {
        let catalog_cache =
            catalog_cache::CatalogCache::in_cache(library.cache_dir(), config.catalog_max_age());
        Logoxide {
            catalog_cache,
            config,
            library,
            http,
            fetcher,
            reader,
        }
    }

    /**
    A native host's: downloads through [`http::UreqHttp`] sending `user_agent`, packages through
    `git` where it runs (the config's timeouts, cloning under the library) and over HTTP where it
    doesn't, and logoxide-text reading the text
    */
    #[cfg(feature = "ureq")]
    pub fn default_native(
        config: Config,
        library: Library,
        user_agent: &str,
        notes: Notes,
    ) -> Logoxide {
        let http: Arc<dyn http::Http> = Arc::new(http::UreqHttp::new(user_agent, notes.clone()));
        let git = config.git(Some(library.git_dir()));
        let fetcher = fetch::fetcher(git, http.clone(), notes);
        Logoxide::new(
            config,
            library,
            http,
            fetcher,
            Box::new(logoxide::reader::DefaultReader),
        )
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn http(&self) -> &dyn http::Http {
        self.http.as_ref()
    }

    /// The [`Http`](http::Http), to hold
    pub fn http_arc(&self) -> Arc<dyn http::Http> {
        self.http.clone()
    }

    /// Where downloaded catalogs are kept (`<cache>/catalogs`), unless `catalog-max-age` is 0
    pub fn catalog_cache(&self) -> Option<&catalog_cache::CatalogCache> {
        self.catalog_cache.as_ref()
    }

    /// Catalogs had through this one's downloads and cache, as `freshness` allows
    pub fn catalog_fetch(&self, freshness: catalog_cache::Freshness) -> catalogs::CatalogFetch<'_> {
        catalogs::CatalogFetch::new(self.http(), self.catalog_cache(), freshness)
    }

    pub fn fetcher(&self) -> &dyn Fetcher {
        self.fetcher.as_ref()
    }

    pub fn reader(&self) -> &dyn TextReader {
        self.reader.as_ref()
    }

    /// The project in `dir` (a folder with a `logoxide.toml`), else the library's own
    pub fn project(&self, dir: Option<PathBuf>) -> Result<Project> {
        Project::open(dir.unwrap_or_else(|| self.library.root().to_path_buf()))
    }

    /// The project's catalogs (its own, then the config's, then the defaults), in order; those
    /// at URLs from the cache while they're young enough
    pub fn catalogs(
        &self,
        project: &Project,
        warn: Warn,
    ) -> Result<Vec<logoxide::catalog::Catalog>> {
        let locations = self.config.catalogs(project.manifest());
        let fetch = self.catalog_fetch(catalog_cache::Freshness::Fresh);
        catalogs::load_catalogs(fetch, &locations, project.dir(), warn)
    }

    /// The project's catalogs and registries at URLs downloaded again and kept (all, or those
    /// `only` names; [`catalogs::refresh`])
    pub fn refresh_catalogs(
        &self,
        project: &Project,
        only: &[String],
    ) -> Result<Vec<catalogs::Refreshed>> {
        let cache = self.catalog_cache();
        catalogs::refresh(self.http(), cache, &self.config, project, only)
    }

    /// A registry of the project's (or the config's) by name
    pub fn registry(&self, project: &Project, name: &str) -> Result<logoxide::catalog::Catalog> {
        let registries = self.config.registries(project.manifest());
        let location = registries.get(name).ok_or_else(|| {
            logoxide::LogoxideError::Catalog(format!("no registry `{name}` (lx catalog list)"))
        })?;
        let fetch = self.catalog_fetch(catalog_cache::Freshness::Fresh);
        catalogs::load_registry(fetch, name, location, project.dir())
    }

    /**
    The fetcher installs go through: this one's, which also makes imports through the
    project's catalogs ([`importer::Importing`])
    */
    pub fn importing(&self, project: &Project) -> importer::Importing<'_> {
        let catalogs = importer::ImportCatalogs {
            fetch: self.catalog_fetch(catalog_cache::Freshness::Fresh),
            locations: self.config.catalogs(project.manifest()),
            dir: project.dir().to_path_buf(),
        };
        importer::Importing::new(self.fetcher(), self.http(), self.reader(), catalogs)
            .with_library(&self.library)
    }

    /// Installs what's locked, moving what `update` allows, within what `frozen` allows; the
    /// lock written. Imports not in the library are made again from their locked downloads
    pub fn install(
        &self,
        project: &mut Project,
        update: Update,
        frozen: Frozen,
        warn: Warn,
    ) -> Result<Lockfile> {
        let importing = self.importing(project);
        let installed = self.install_with(project, &importing, update, frozen);
        importing.warnings().into_iter().for_each(&mut *warn);
        installed
    }

    /// [`Logoxide::install`] through `importing` (with what it was given: an update's builds)
    pub fn install_with(
        &self,
        project: &mut Project,
        importing: &importer::Importing<'_>,
        update: Update,
        frozen: Frozen,
    ) -> Result<Lockfile> {
        let lock = match frozen == Frozen::default() {
            true => project.install(&self.library, importing, update)?,
            false => project.install_frozen(&self.library, importing, frozen)?,
        };
        Ok(lock.clone())
    }

    /**
    An import (`ebible:eng-kjv2006`) found in the project's catalogs, added, and installed: kept
    in the library ([`import::run`]), or, `vendored`, made in the project's `imports/<id>/`
    ([`import::run_vendored`])
    */
    pub fn import(
        &self,
        project: &mut Project,
        source: &str,
        vendored: bool,
        warn: Warn,
    ) -> Result<import::Imported> {
        let fetch = self.catalog_fetch(catalog_cache::Freshness::Fresh);
        let entry = import::find_entry(fetch, &self.config, project, source, &mut *warn)?;
        if vendored {
            let importing = self.importing(project);
            let imported =
                import::run_vendored(project, &self.library, &entry, self.http(), &importing);
            importing.warnings().into_iter().for_each(&mut *warn);
            return imported;
        }
        let importing = self.importing(project);
        let imported = import::run(project, &self.library, &entry, &importing);
        importing.warnings().into_iter().for_each(&mut *warn);
        imported
    }

    /// What updating the project's imports (or the one `name` names) would do, the catalogs
    /// downloaded again ([`import::plan_updates`])
    pub fn plan_updates(
        &self,
        project: &Project,
        name: Option<&str>,
        warn: Warn,
    ) -> Result<Vec<import::UpdatePlan>> {
        let reader = self.reader();
        let fetch = self.catalog_fetch(catalog_cache::Freshness::Refresh);
        let library = &self.library;
        import::plan_updates(fetch, &self.config, project, library, reader, name, warn)
    }

    /// The library cleaned of what none of `projects` locks and no root keeps
    /// ([`clean::clean`])
    pub fn clean(
        &self,
        projects: &[Project],
        remove: bool,
        prune_roots: bool,
        said: &mut dyn FnMut(clean::Cleaning),
    ) -> Result<clean::Cleaned> {
        clean::clean(&self.library, projects, remove, prune_roots, said)
    }

    /// Every version in the library, with the projects (`projects` and the library's roots)
    /// that use it ([`clean::installed`])
    pub fn installed(&self, projects: &[Project]) -> Result<Vec<clean::Installed>> {
        clean::installed(&self.library, projects)
    }

    /// A passage from one of the project's Bibles (`John 3:16 KJV`; [`read::read`])
    pub fn read(&self, project: &Project, reference: &str, style: &Style) -> Result<read::Read> {
        read::read(project, &self.library, self.reader(), reference, style)
    }

    /// A passage in several of the project's Bibles ([`read::compare`])
    pub fn compare(
        &self,
        project: &Project,
        reference: &str,
        bibles: &[String],
        style: &Style,
    ) -> Result<Vec<read::Compared>> {
        read::compare(
            project,
            &self.library,
            self.reader(),
            reference,
            bibles,
            style,
        )
    }
}
