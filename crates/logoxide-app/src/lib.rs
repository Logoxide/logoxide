/*!
What every logoxide front end does (lx, the bindings, a language or MCP server), in one place:
catalogs and registries read over HTTP ([`catalogs`]), packages fetched with or without `git`
([`fetch`]), imports made and updated ([`import`]), libraries cleaned ([`clean`]), media checked
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

pub mod catalogs;
pub mod clean;
pub mod fetch;
pub mod http;
pub mod import;
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
    fetcher: Box<dyn Fetcher + Send + Sync>,
    reader: Box<dyn TextReader>,
}

impl Logoxide {
    pub fn new(
        config: Config,
        library: Library,
        http: Arc<dyn http::Http>,
        fetcher: Box<dyn Fetcher + Send + Sync>,
        reader: Box<dyn TextReader>,
    ) -> Logoxide {
        Logoxide {
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

    pub fn fetcher(&self) -> &(dyn Fetcher + Send + Sync) {
        self.fetcher.as_ref()
    }

    pub fn reader(&self) -> &dyn TextReader {
        self.reader.as_ref()
    }

    /// The project in `dir` (a folder with a `logoxide.toml`), else the library's own
    pub fn project(&self, dir: Option<PathBuf>) -> Result<Project> {
        Project::open(dir.unwrap_or_else(|| self.library.root().to_path_buf()))
    }

    /// The project's catalogs (its own, then the config's, then the defaults), in order
    pub fn catalogs(
        &self,
        project: &Project,
        warn: Warn,
    ) -> Result<Vec<logoxide::catalog::Catalog>> {
        let locations = self.config.catalogs(project.manifest());
        catalogs::load_catalogs(self.http(), &locations, project.dir(), warn)
    }

    /// A registry of the project's (or the config's) by name
    pub fn registry(&self, project: &Project, name: &str) -> Result<logoxide::catalog::Catalog> {
        let registries = self.config.registries(project.manifest());
        let location = registries.get(name).ok_or_else(|| {
            logoxide::LogoxideError::Catalog(format!("no registry `{name}` (lx catalog list)"))
        })?;
        catalogs::load_registry(self.http(), name, location, project.dir())
    }

    /// Installs what's locked, moving what `update` allows, within what `frozen` allows; the
    /// lock written
    pub fn install(
        &self,
        project: &mut Project,
        update: Update,
        frozen: Frozen,
    ) -> Result<Lockfile> {
        let lock = match frozen == Frozen::default() {
            true => project.install(&self.library, self.fetcher(), update)?,
            false => project.install_frozen(&self.library, self.fetcher(), frozen)?,
        };
        Ok(lock.clone())
    }

    /// An import (`ebible:eng-kjv2006`) found in the project's catalogs, made in its
    /// `imports/<id>/`, added, and installed ([`import::find_entry`], [`import::run`])
    pub fn import(
        &self,
        project: &mut Project,
        source: &str,
        warn: Warn,
    ) -> Result<import::Imported> {
        let entry = import::find_entry(self.http(), &self.config, project, source, warn)?;
        import::run(project, &self.library, &entry, self.http(), self.fetcher())
    }

    /// What updating the project's imports (or the one `name` names) would do
    /// ([`import::plan_updates`])
    pub fn plan_updates(
        &self,
        project: &Project,
        name: Option<&str>,
        warn: Warn,
    ) -> Result<Vec<import::UpdatePlan>> {
        let reader = self.reader();
        import::plan_updates(self.http(), &self.config, project, reader, name, warn)
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
