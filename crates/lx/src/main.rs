//! `lx`: logoxide's command line

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use logoxide::{
    Cache, Features, FsStorage, Library, MemoryStorage, Package, Project, Storage, bible_data,
    git::GitCli,
    manifest::{Dependency, DetailedDependency, MANIFEST},
    resolve::Update,
};

mod complete;
mod net;
mod publish;

use net::get;

#[derive(Parser)]
#[command(
    name = "lx",
    version,
    about = "Bibles, commentaries, lexicons, books, and media as versioned packages",
    after_help = "Exit status: 0 when it worked, 1 on an error; `lx is-enabled` exits 0 when the \
                  package is on, 1 when it's off, and 2 on an error"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// The folder with the logoxide.toml to use: the current one if it has one, else the library's
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    /// The library (LOGOXIDE_HOME, else ~/.config/logoxide/config.toml's `library`, else
    /// $XDG_DATA_HOME/logoxide)
    #[arg(long, global = true)]
    library: Option<PathBuf>,
    /// The cache (LOGOXIDE_CACHE, else the config's `cache`, else $XDG_CACHE_HOME/logoxide)
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
    /// How to print: text, or json (list, info, check, diff, verify, search, catalog list,
    /// import --update, show --references); tsv for show --references
    #[arg(long, global = true, value_enum, default_value_t = Format::Text)]
    format: Format,
}

/// What `--format` prints
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum Format {
    Text,
    Json,
    Tsv,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Json => "json",
            Format::Tsv => "tsv",
        }
    }
}

/// Prints a value as JSON
fn print_json(value: &impl serde::Serialize) -> logoxide::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| logoxide::LogoxideError::Manifest(e.to_string()))?;
    println!("{text}");
    Ok(())
}

/// logoxide's config (`~/.config/logoxide/config.toml`), read once when lx starts
static CONFIG: std::sync::OnceLock<logoxide::config::Config> = std::sync::OnceLock::new();

fn config() -> &'static logoxide::config::Config {
    CONFIG.get_or_init(Default::default)
}

impl Cli {
    /// `--library` and `--cache-dir`, else the environment's, else the config's, else the XDG
    /// folders
    fn library(&self) -> Library {
        config().library(self.library.clone(), self.cache_dir.clone())
    }

    fn project(&self, library: &Library) -> logoxide::Result<Project> {
        let dir = match &self.project {
            Some(dir) => dir.clone(),
            None if std::path::Path::new(MANIFEST).is_file() => PathBuf::from("."),
            None => library.root().to_path_buf(),
        };
        Project::open(dir)
    }

    fn fetcher(&self, library: &Library) -> Web {
        Web(GitCli {
            scratch: Some(library.git_dir()),
        })
    }
}

/// Git through the `git` command, and downloads (registries' catalogs and archives) over HTTPS
struct Web(GitCli);

impl std::ops::Deref for Web {
    type Target = GitCli;
    fn deref(&self) -> &GitCli {
        &self.0
    }
}

impl logoxide::git::Fetcher for Web {
    fn tags(&self, url: &str) -> logoxide::Result<Vec<(String, String)>> {
        self.0.tags(url)
    }

    fn fetch(
        &self,
        url: &str,
        reference: &logoxide::git::GitRef,
        dest: &dyn Storage,
    ) -> logoxide::Result<String> {
        self.0.fetch(url, reference, dest)
    }

    fn fetch_files(
        &self,
        url: &str,
        reference: &logoxide::git::GitRef,
        paths: &[&str],
        dest: &dyn Storage,
    ) -> logoxide::Result<String> {
        self.0.fetch_files(url, reference, paths, dest)
    }

    fn download(&self, url: &str) -> logoxide::Result<Vec<u8>> {
        match url.starts_with("https://") || url.starts_with("http://") {
            true => get(url),
            false => logoxide::git::read_local(url),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// What a package is: its manifest, the format read, its books, and its checksum; with --git, a
    /// release's summary, without downloading its text
    Show {
        #[command(flatten)]
        package: PackageArgs,
        /// A package's repository (or `github:owner/repo`): show a release's published summary
        #[arg(long, add = ArgValueCompleter::new(complete::package))]
        git: Option<String>,
        /// The release (the newest by default)
        #[arg(long, requires = "git")]
        tag: Option<String>,
        /// List the passages its books (EPUBs) cite, and how often
        #[arg(long)]
        references: bool,
        /// With --references: every reference, with where it is (chapter, paragraph, CFI,
        /// offsets), read from the books (so not with --git). `--format` lists them as text,
        /// tsv, or json
        #[arg(long, requires = "references", conflicts_with = "git")]
        locations: bool,
    },
    /// Writes the package's summary, checks its version against what changed since the last
    /// release, then commits and tags it
    Release {
        /// The package's repository
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Set the version to the smallest one the change allows, instead of failing
        #[arg(long)]
        set_version: bool,
        /// Only say what the release would be
        #[arg(long)]
        dry_run: bool,
    },
    /// What changed between two releases (or a release and the working files)
    Diff {
        /// The package's repository
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// A release's tag (`v1.0.0`)
        from: String,
        /// Another release's tag (the working files by default)
        to: Option<String>,
    },
    /// Builds the package's summary again and compares it with the published one
    Verify(PackageArgs),
    /// Reads every book of a package, and says what it found
    Check {
        #[command(flatten)]
        package: PackageArgs,
        /// Keep parsed books in this folder (read them from it when they're there)
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    /// Makes a new package in its own folder (a git repository): from one file (`--from
    /// book.epub`; an EPUB's title, subtitle, authors, and language fill in what isn't given), or
    /// a collection (`--kind collection`), whose packages are its dependencies (`lx add
    /// --project <its folder>`). Then `lx release` in it commits and tags it
    New(NewArgs),
    /// Starts a logoxide.toml in a folder (the current one by default), like a vault's
    Init {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Adds a package to logoxide.toml, installs it, and locks it
    Add(AddArgs),
    /// Removes a package from logoxide.toml (its files stay in the library until `lx clean`)
    Remove {
        /// The package's id
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        id: String,
    },
    /// Turns a package off without removing it (`enabled = false` in logoxide.toml): it stays
    /// locked and installed, but nothing reads it
    Disable {
        /// What logoxide.toml calls it
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        name: String,
    },
    /// Turns a package that was turned off on again
    Enable {
        /// What logoxide.toml calls it
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        name: String,
    },
    /// Installs what logoxide.lock says (resolving what it doesn't have yet)
    Install {
        /// Put back the files of installed versions that changed (edited in the library), instead
        /// of stopping; files that aren't the package's (annotations) stay
        #[arg(long)]
        repair: bool,
    },
    /// Moves packages to the newest versions their requirements allow
    Update {
        /// Only these packages (all by default)
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        ids: Vec<String>,
    },
    /// The packages logoxide.toml asks for, as locked (those turned off are marked `(off)`)
    List {
        /// Only the packages in use (logoxide.toml's that are on, and what they bring in)
        #[arg(long, conflicts_with = "disabled")]
        enabled: bool,
        /// Only the packages turned off (and what only they bring in)
        #[arg(long)]
        disabled: bool,
    },
    /// Whether a package is on: prints `on` or `off`, and exits with 0 if it's on, 1 if it's off,
    /// and 2 on an error (no such dependency, no project)
    IsEnabled {
        /// What logoxide.toml calls it
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        name: String,
    },
    /// A locked package: what it is, where, its size, its books and verses, and whether its files
    /// are still the ones locked
    Info {
        /// The package's id
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        id: String,
    },
    /// The library's versions no lock file here uses: the library's own and the project's, and
    /// any other projects named (--keep). Says which; --remove deletes them
    Clean {
        /// Delete them (and their parsed cache)
        #[arg(long)]
        remove: bool,
        /// Other projects whose lock files' versions stay (vaults elsewhere)
        #[arg(long)]
        keep: Vec<PathBuf>,
    },
    /// Finds packages in catalogs by name, abbreviation, or language (the project's catalogs,
    /// then eBible.org's and fetch.bible's lists: lx catalog list)
    Search {
        /// Words to find (every one must match)
        query: Vec<String>,
        /// A catalog to search (a URL or a path; a `.csv` is eBible.org's list), instead of the
        /// project's
        #[arg(long)]
        catalog: Vec<String>,
        /// Search only this registry (lx catalog list); with no words, list its packages
        #[arg(long, conflicts_with = "catalog", add = ArgValueCompleter::new(complete::registry))]
        registry: Option<String>,
        #[command(flatten)]
        books: BookFilterArgs,
        /// Only packages in this category (`--category systematic-theology`)
        #[arg(long, value_parser = category_values())]
        category: Option<String>,
        /// How many to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// One JSON array, for editors: each package's id, names, language, license, keywords,
        /// categories, books it's about, and the lx arguments that install it (as `--format
        /// json`)
        #[arg(long)]
        json: bool,
    },
    /// Makes a package from another publisher's download, puts it in the project's `imports/`,
    /// and adds it as a path package: `ebible:<id>`, `fetch:<id>`, `ccel:<id>`, or
    /// `sword:<repository>/<module>` (as `lx search` shows), from the project's catalogs (and
    /// for SWORD, the repositories CrossWire lists)
    ///
    /// `--update` makes the project's imports again from their publishers' downloads and replaces
    /// the ones that changed (a reader's files beside them stay), saying what changed
    Import {
        /// `ebible:eng-asv`, `fetch:eng_bsb`, `ccel:kjv`, `sword:crosswire/KJV`; with `--update`,
        /// one import (its dependency's name, or this), else all of them
        #[arg(required_unless_present = "update")]
        source: Option<String>,
        /// Update imports: download each again, and replace it if it changed
        #[arg(long)]
        update: bool,
        /// With `--update`: say what would change, and change nothing
        #[arg(long, requires = "update")]
        dry_run: bool,
        /// With `--update`: remove the files an update drops without asking (needed when lx
        /// isn't run in a terminal)
        #[arg(short, long, requires = "update")]
        yes: bool,
    },
    /// Catalogs: lists of packages anyone can host
    #[command(subcommand)]
    Catalog(CatalogCommand),
    /// Registries of archives: each package release is one file, so a registry of thousands of
    /// books needs no repository for each, and any static host (or GitHub's releases) serves it
    #[command(subcommand)]
    Registry(RegistryCommand),
    /// Adds a package's release to a registry: its files at its release tag (lx release first),
    /// as an archive, and its entry in the registry's catalog.toml. A published version never
    /// changes
    Publish {
        /// The package's folder (in its repository, or a folder of it)
        #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
        path: PathBuf,
        /// The registry: a folder, or `github:owner/repo` (its releases, through gh)
        #[arg(long)]
        to: String,
        /// Every package in the folder and its folders whose version is released (a repository of
        /// many): each not published yet is
        #[arg(long)]
        all: bool,
    },
    /// Withdraws a release from a registry: new installs and updates pass over it, while lock
    /// files that have it still install it (nobody's text changes under them)
    Yank {
        /// The package's id
        id: String,
        /// The release's version
        version: String,
        /// The registry: a folder, or `github:owner/repo`
        #[arg(long)]
        from: String,
        /// Put it back
        #[arg(long)]
        undo: bool,
    },
    /// A package's audio and video, listed by URL and hash
    #[command(subcommand)]
    Media(MediaCommand),
    /// Where a reference numbered one way is in another numbering, through the project's
    /// versification maps: `lx map "Psalm 51:1" --from English --to Original`. --from and --to
    /// name Bibles here (by name, abbreviation, or alias: their `versification`) or
    /// versifications
    Map {
        /// `Psalm 51:1`, `Joel 3`, `Malachi 4:1-6`
        reference: String,
        #[arg(long, add = ArgValueCompleter::new(complete::dependency))]
        from: String,
        #[arg(long, add = ArgValueCompleter::new(complete::dependency))]
        to: String,
        /// A map package's folder to use too (repeat for more), besides the project's
        #[arg(long = "with", value_hint = clap::ValueHint::DirPath)]
        with: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum MediaCommand {
    /// Lists a file in the package's logoxide.toml: its URL, size, and SHA-256 (from the file here)
    Add {
        /// The file, as it's served
        file: PathBuf,
        /// Where it's served; repeat for mirrors, tried in the order given
        #[arg(long, required = true)]
        url: Vec<String>,
        /// The book it reads (a USFM code)
        #[arg(long)]
        book: Option<String>,
        /// The package's folder
        #[arg(long, default_value = ".")]
        package: PathBuf,
    },
    /// Checks the timings (WebVTT, a cue per verse) against the text, then downloads every
    /// listed file from each of its URLs (mirrors too) and checks its size and hash
    Check {
        /// The package's folder
        #[arg(default_value = ".")]
        package: PathBuf,
        /// Only the timings: nothing is downloaded
        #[arg(long)]
        offline: bool,
    },
}

#[derive(Subcommand)]
enum RegistryCommand {
    /// Sets up a registry to publish to: a folder (catalog.toml and a README on hosting it), or
    /// `github:owner/repo` (the repository, made if it isn't there, and a release named `index`
    /// holding the catalog; each package release becomes a release of its own)
    Init {
        /// A folder, or `github:owner/repo`
        host: String,
        /// The registry's name in its catalog (the folder's or repository's by default)
        #[arg(long)]
        name: Option<String>,
        /// Make the GitHub repository private (readers then need access to it)
        #[arg(long)]
        private: bool,
    },
}

#[derive(Subcommand)]
enum CatalogCommand {
    /// The catalogs `lx search` looks through: the project's, then eBible.org's and fetch.bible's
    List,
    /// Searches a catalog too (a URL, or a path relative to the project): a catalog.toml,
    /// eBible.org's translations.csv, or fetch.bible's manifest.json. It's read first, to check it.
    /// With --name, a registry: a catalog or a folder of package repositories, whose packages
    /// `<name>:<id>` or `<name>:<path in the folder>` then name (`lx add dorean-local:kjv`)
    Add {
        location: String,
        /// Name it as a registry
        #[arg(long)]
        name: Option<String>,
    },
    /// Stops searching a catalog, or a registry (by its name). Removing a default one turns the
    /// defaults off (`default-catalogs = false`), keeping the others
    Remove {
        #[arg(add = ArgValueCompleter::new(complete::searched))]
        location: String,
    },
    /// A catalog.toml entry for a package's repository (its newest release's logoxide.toml)
    Entry {
        /// The package's git URL (or `github:owner/repo`)
        git: String,
    },
    /// Writes a catalog.toml of every package repository in a folder (and its folders), from each
    /// one's newest release, so the folder can be a registry anywhere (hosted, or on a phone).
    /// Entries keep their path in the folder, which names them as their ids do. A package whose
    /// newest release hasn't changed keeps its entry
    Build {
        /// The folder of repositories
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Where to write it (catalog.toml in the folder by default)
        #[arg(long)]
        output: Option<PathBuf>,
        /// Each package's git URL, `{path}` being its path in the folder and `{name}` its
        /// folder's name: `https://github.com/someone/{name}` (its `file://` URL by default)
        #[arg(long)]
        url: Option<String>,
        /// The catalog's name (the folder's by default)
        #[arg(long)]
        name: Option<String>,
        /// Read every package's release again, even unchanged ones
        #[arg(long)]
        force: bool,
    },
}

#[derive(Args)]
struct AddArgs {
    /// A git repository: `github:owner/repo` (also `gitlab:`, `codeberg:`), its URL, or a
    /// package in a registry (`dorean-local:dorean-collection`, by its id or its path there)
    #[arg(conflicts_with_all = ["git", "path"], add = ArgValueCompleter::new(complete::package))]
    repository: Option<String>,
    /// A git repository's URL (or a local repository's path), or `github:owner/repo`
    #[arg(long, conflicts_with = "path", add = ArgValueCompleter::new(complete::package))]
    git: Option<String>,
    /// A package's folder, used in place
    #[arg(long, value_hint = clap::ValueHint::DirPath)]
    path: Option<String>,
    /// A registry's catalog (a URL, or a file) to add the package from by its id or path there,
    /// without naming the registry first: `lx add --registry https://…/catalog.toml romans`
    #[arg(long, conflicts_with_all = ["git", "path"])]
    registry: Option<String>,
    /// A version requirement (`1`, `^1.2`); the newest matching tag is used
    #[arg(long, conflicts_with_all = ["tag", "branch", "rev"])]
    version: Option<String>,
    #[arg(long, conflicts_with_all = ["branch", "rev"])]
    tag: Option<String>,
    #[arg(long, conflicts_with = "rev")]
    branch: Option<String>,
    #[arg(long)]
    rev: Option<String>,
    /// Features to turn on, separated by commas
    #[arg(short = 'F', long, value_delimiter = ',')]
    features: Vec<String>,
    #[arg(long)]
    no_default_features: bool,
    /// What this project calls it, when not its id (another fork of a package it has)
    #[arg(long)]
    name: Option<String>,
    /// What references name it by here, in place of its own abbreviation
    #[arg(long)]
    abbreviation: Option<String>,
    /// Its aliases here, in place of its own (repeat for more; `--alias ""` for none)
    #[arg(long = "alias")]
    aliases: Vec<String>,
}

#[derive(Args)]
struct NewArgs {
    /// The package's folder (by default its id, in the current folder)
    dir: Option<PathBuf>,
    /// The file it's made from: .epub (a book), .usx, .usfm, .osis, or .thml (a Bible)
    #[arg(long, required_unless_present = "kind")]
    from: Option<PathBuf>,
    /// What dependencies call it (by default its folder's name, else from its name:
    /// `the-dorean-principle`)
    #[arg(long)]
    id: Option<String>,
    /// Its title (an EPUB's own by default)
    #[arg(long)]
    name: Option<String>,
    /// What references name it by (by default its name's initials: `TDP`)
    #[arg(long)]
    abbreviation: Option<String>,
    /// An author (repeat for more, in order; an EPUB's own by default)
    #[arg(long = "author")]
    authors: Vec<String>,
    /// A sentence about it (an EPUB's subtitle by default)
    #[arg(long)]
    description: Option<String>,
    /// Its first version (0.1.0 by default)
    #[arg(long)]
    version: Option<String>,
    /// bible, commentary, lexicon, book, collection, ... (by default from the file: an EPUB is a
    /// book)
    #[arg(long)]
    kind: Option<String>,
    /// BCP 47 (`en`; an EPUB's own by default)
    #[arg(long)]
    language: Option<String>,
    /// An SPDX expression (`CC0-1.0`); shown, never enforced
    #[arg(long)]
    license: Option<String>,
    #[arg(long)]
    publisher: Option<String>,
    #[arg(long)]
    homepage: Option<String>,
    /// Where its repository is (its page: `https://github.com/someone/romans`)
    #[arg(long)]
    repository: Option<String>,
    /// A word a search should find it by (repeat for more)
    #[arg(long = "keyword")]
    keywords: Vec<String>,
    /// What it's about (up to 5; repeat for more; keywords say anything narrower)
    #[arg(long = "category", value_parser = category_values())]
    categories: Vec<String>,
    /// A book of the Bible it's mainly about, by name or USFM code (`--about Romans`; repeat for
    /// more)
    #[arg(long = "about")]
    about_books: Vec<String>,
    /// Don't make the folder a git repository
    #[arg(long)]
    no_git: bool,
}

#[derive(Args)]
struct PackageArgs {
    /// The package's folder (with its logoxide.toml)
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Features to turn on, separated by commas
    #[arg(short = 'F', long, value_delimiter = ',')]
    features: Vec<String>,
    /// Leave out the default features
    #[arg(long)]
    no_default_features: bool,
}

impl PackageArgs {
    fn open(&self) -> logoxide::Result<Package> {
        let features: Vec<&str> = self.features.iter().map(String::as_str).collect();
        let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(&self.path));
        Package::open(
            storage,
            Features {
                features: &features,
                default: !self.no_default_features,
            },
        )
    }
}

fn main() -> ExitCode {
    // `COMPLETE=bash lx` writes the shell's completion script; the shell then runs lx to complete
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    match logoxide::config::Config::load() {
        Ok(loaded) => {
            let _ = CONFIG.set(loaded);
        }
        Err(e) => {
            eprintln!("lx: {e}");
            return ExitCode::FAILURE;
        }
    }
    if let Err(error) = check_format(&cli) {
        eprintln!("lx: {error}");
        return ExitCode::FAILURE;
    }
    let json = cli.format == Format::Json;
    net::set_quiet(json || matches!(cli.command, Command::Search { json: true, .. }));
    let format = cli.format.name();
    let result = match &cli.command {
        Command::Show {
            package,
            git: None,
            references,
            locations,
            ..
        } => show(&cli, package, references.then_some((format, *locations))),
        Command::Show {
            git: Some(url),
            tag,
            references,
            ..
        } => package_url(&cli, url)
            .and_then(|url| show_remote(&cli, &url, tag.as_deref(), references.then_some(format))),
        Command::Release {
            path,
            set_version,
            dry_run,
        } => release(&cli, path, *set_version, *dry_run),
        Command::Diff { path, from, to } => diff(&cli, path, from, to.as_deref()),
        Command::Verify(args) => verify(args, json),
        Command::Check { package, cache } => check(package, cache.clone(), json),
        Command::New(args) => new(args),
        Command::Init { dir } => logoxide::Project::init(dir)
            .map(|p| println!("created {}", p.dir().join(MANIFEST).display())),
        Command::Add(args) => add(&cli, args),
        Command::Remove { id } => remove(&cli, id),
        Command::Disable { name } => set_enabled(&cli, name, false),
        Command::Enable { name } => set_enabled(&cli, name, true),
        Command::Install { repair: false } => install(&cli, Update::Nothing),
        Command::Install { repair: true } => repair(&cli),
        Command::Update { ids } => install(
            &cli,
            match ids.is_empty() {
                true => Update::All,
                false => Update::Only(ids.iter().cloned().collect()),
            },
        ),
        Command::List { enabled, disabled } => list(
            &cli,
            match (enabled, disabled) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
        ),
        // Like `systemctl is-enabled`: 1 is an answer (off), so an error is 2
        Command::IsEnabled { name } => {
            return match is_enabled(&cli, name) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(error) => {
                    eprintln!("lx: {error}");
                    ExitCode::from(2)
                }
            };
        }
        Command::Info { id } => info(&cli, id),
        Command::Clean { remove, keep } => clean(&cli, *remove, keep),
        Command::Search {
            query,
            catalog,
            registry,
            books,
            category,
            limit,
            json,
        } => search(
            &cli,
            &query.join(" "),
            catalog,
            registry.as_deref(),
            Filters {
                books: books.filters(),
                category: category.as_deref(),
            },
            *limit,
            *json || cli.format == Format::Json,
        ),
        Command::Import {
            source,
            update: true,
            dry_run,
            yes,
        } => import_update(&cli, source.as_deref(), *dry_run, *yes),
        Command::Import { source, .. } => import(&cli, source.as_deref().unwrap_or_default()),
        Command::Catalog(CatalogCommand::Entry { git }) => {
            catalog_entry(&cli, &logoxide::git::expand_url(git))
        }
        Command::Catalog(CatalogCommand::List) => catalog_list(&cli),
        Command::Registry(RegistryCommand::Init {
            host,
            name,
            private,
        }) => publish::init(&publish::Host::parse(host), name.as_deref(), *private),
        Command::Publish { path, to, all } => {
            let library = cli.library();
            let (host, git) = (publish::Host::parse(to), cli.fetcher(&library));
            match all {
                true => publish::publish_all(path, &host, &git),
                false => publish::publish(path, &host, &git),
            }
        }
        Command::Yank {
            id,
            version,
            from,
            undo,
        } => publish::yank(&publish::Host::parse(from), id, version, *undo),
        Command::Catalog(CatalogCommand::Build {
            dir,
            output,
            url,
            name,
            force,
        }) => catalog_build(
            dir,
            output.as_deref(),
            url.as_deref(),
            name.as_deref(),
            *force,
        ),
        Command::Catalog(CatalogCommand::Add { location, name }) => {
            catalog_add(&cli, location, name.as_deref())
        }
        Command::Catalog(CatalogCommand::Remove { location }) => catalog_remove(&cli, location),
        Command::Media(MediaCommand::Add {
            file,
            url,
            book,
            package,
        }) => media_add(file, url, book.as_deref(), package),
        Command::Media(MediaCommand::Check { package, offline }) => media_check(package, *offline),
        Command::Map {
            reference,
            from,
            to,
            with,
        } => map(&cli, reference, from, to, with),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lx: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Whether the command prints in `--format`: JSON where it has a form for it, TSV only for
/// `lx show --references`
fn check_format(cli: &Cli) -> logoxide::Result<()> {
    let references = matches!(
        cli.command,
        Command::Show {
            references: true,
            ..
        }
    );
    let json = references
        || matches!(
            cli.command,
            Command::List { .. }
                | Command::Info { .. }
                | Command::Check { .. }
                | Command::Diff { .. }
                | Command::Verify(_)
                | Command::Search { .. }
                | Command::Catalog(CatalogCommand::List)
                | Command::Import { update: true, .. }
        );
    match cli.format {
        Format::Text => Ok(()),
        Format::Json if json => Ok(()),
        Format::Tsv if references => Ok(()),
        Format::Json => Err(logoxide::LogoxideError::Manifest(String::from(
            "--format json: only for list, info, check, diff, verify, search, catalog list, import --update, and show --references",
        ))),
        Format::Tsv => Err(logoxide::LogoxideError::Manifest(String::from(
            "--format tsv: only for show --references",
        ))),
    }
}

/// Books by their USFM codes, named: `ROM GAL` is `Romans, Galatians`
fn book_names(codes: &[String]) -> String {
    let books = bible_data().books();
    let names: Vec<String> = (codes.iter())
        .map(|code| {
            (books.search_usfm(code))
                .and_then(|id| books.get_name(id))
                .cloned()
                .unwrap_or_else(|| code.clone())
        })
        .collect();
    names.join(", ")
}

fn show(cli: &Cli, args: &PackageArgs, references: Option<(&str, bool)>) -> logoxide::Result<()> {
    let package = args.open()?;
    let info = package.info();
    let manifest = package.manifest();
    println!(
        "{} {} ({}), {}",
        info.id, info.version, info.abbreviation, info.name
    );
    if let Some(description) = &info.description {
        println!("{description}");
    }
    if !info.authors.is_empty() {
        println!("by {}", info.authors.join(", "));
    }
    println!(
        "kind: {:?}, language: {}, direction: {:?}",
        info.kind, info.language, info.direction
    );
    println!(
        "license: {}",
        info.license
            .as_deref()
            .or(info.license_file.as_deref())
            .unwrap_or("none given")
    );
    if let Some(media) = manifest.media.as_ref().filter(|m| !m.files.is_empty()) {
        let bytes: u64 = media.files.iter().map(|f| f.size).sum();
        println!(
            "media: {} files, {:.1} MB, listed by URL and hash",
            media.files.len(),
            bytes as f64 / 1e6
        );
    }
    if !info.about_books.is_empty() {
        println!("about: {}", book_names(&info.about_books));
    }
    if !info.categories.is_empty() {
        println!("categories: {}", info.categories.join(", "));
    }
    if !info.keywords.is_empty() {
        println!("keywords: {}", info.keywords.join(", "));
    }
    for (label, link) in [
        ("homepage", &info.homepage),
        ("repository", &info.repository),
        ("documentation", &info.documentation),
        ("readme", &info.readme),
    ] {
        if let Some(link) = link {
            println!("{label}: {link}");
        }
    }
    if let Some(upstream) = &info.upstream {
        println!("upstream: {upstream}");
    }
    let enabled: Vec<&str> = manifest
        .formats(package.enabled())
        .iter()
        .map(|(n, _)| *n)
        .collect();
    let all: Vec<&str> = manifest.formats.keys().map(String::as_str).collect();
    println!(
        "formats: {} (enabled: {}; reading {})",
        all.join(", "),
        enabled.join(", "),
        match package.source().format() {
            Some(format) if format.readable() => format!("{format:?}"),
            _ => String::from("no verse text"),
        }
    );
    let features: Vec<&str> = package
        .enabled()
        .features
        .iter()
        .map(String::as_str)
        .collect();
    println!(
        "features: {}",
        if features.is_empty() {
            "none".into()
        } else {
            features.join(", ")
        }
    );
    for (name, dependency) in &manifest.dependencies {
        match dependency {
            Dependency::Version(req) => println!("dependency {name}: {req}"),
            Dependency::Detailed(d) => println!(
                "dependency {name}: {}{}",
                (d.git
                    .as_deref()
                    .or(d.registry.as_deref())
                    .or(d.path.as_deref()))
                .unwrap_or_default(),
                d.version
                    .as_ref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default()
            ),
        }
    }
    let books = package.books();
    let codes: Vec<&str> = books
        .iter()
        .filter_map(|b| bible_data().books().get_usfm(*b).map(String::as_str))
        .collect();
    if !books.is_empty() {
        println!("books: {} ({})", books.len(), codes.join(" "));
    }
    println!("checksum: {}", package.checksum()?);
    // Found once per version, then read from the cache
    let found = cli.library().book_cache().references(&package)?;
    if !found.is_empty() {
        println!("references: {} (in its books)", found.len());
    }
    match references {
        Some((format, true)) => print_references(&found, format)?,
        Some((format, false)) => print_cited(&logoxide::references::cited(&found), format)?,
        None => {}
    }
    Ok(())
}

/// The passages cited, with how often, as text, TSV, or JSON
fn print_cited(cited: &[logoxide::references::Cited], format: &str) -> logoxide::Result<()> {
    match format {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(cited)
                .map_err(|e| logoxide::LogoxideError::Manifest(e.to_string()))?
        ),
        "tsv" => {
            println!("passage\tcount");
            for c in cited {
                println!("{}\t{}", c.passage, c.count);
            }
        }
        "text" => {
            for c in cited {
                match c.count {
                    1 => println!("{}", c.passage),
                    n => println!("{} ({n})", c.passage),
                }
            }
        }
        other => {
            return Err(logoxide::LogoxideError::Manifest(format!(
                "--format {other}: text, tsv, or json"
            )));
        }
    }
    Ok(())
}

/// Every reference, with where it is: as text (its passage, chapter, and paragraph, shortened),
/// TSV, or JSON
fn print_references(
    found: &[logoxide::references::FoundReference],
    format: &str,
) -> logoxide::Result<()> {
    match format {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(found)
                .map_err(|e| logoxide::LogoxideError::Manifest(e.to_string()))?
        ),
        "tsv" => {
            println!("passage\tfile\tchapter\tcfi\tstart\tend\tline\tcolumn\tspine\tline_text");
            let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
            for r in found {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    r.passage,
                    r.file,
                    clean(r.chapter.as_deref().unwrap_or("")),
                    r.cfi,
                    r.start,
                    r.end,
                    r.line,
                    r.column,
                    r.spine,
                    clean(&r.line_text)
                );
            }
        }
        "text" => {
            for r in found {
                let text: String = r.line_text.split_whitespace().collect::<Vec<_>>().join(" ");
                let short: String = text.chars().take(80).collect();
                let more = if short.len() < text.len() { "…" } else { "" };
                println!(
                    "{}  [{}]  {short}{more}",
                    r.passage,
                    r.chapter.as_deref().unwrap_or("-")
                );
            }
        }
        other => {
            return Err(logoxide::LogoxideError::Manifest(format!(
                "--format {other}: text, tsv, or json"
            )));
        }
    }
    Ok(())
}

/// What `lx check --format json` prints
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckReport {
    id: String,
    version: String,
    kind: logoxide::manifest::PackageKind,
    books: usize,
    verses: usize,
    /// A versification map's: what it maps from and to, its lines, and the verses it maps
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<serde_json::Value>,
    /// Each file's title (none when it has none)
    titles: Vec<serde_json::Value>,
    untitled: Vec<String>,
    /// Problems with the timings, and books whose cached counts differ
    problems: Vec<String>,
    ok: bool,
}

fn check(args: &PackageArgs, cache: Option<PathBuf>, json: bool) -> logoxide::Result<()> {
    let package = args.open()?;
    let (id, version) = (
        package.info().id.clone(),
        package.info().version.to_string(),
    );
    let mut problems = vec![];
    let package_kind = package.info().kind;
    let timings = logoxide::media::check_package_timings(&package)?;
    let titles = logoxide::titles::titles(&package)?;
    // A map is read whole: its lines parse, name known books, and don't overlap
    let map = (package_kind == logoxide::manifest::PackageKind::VersificationMap)
        .then(|| logoxide::versification::VersificationMap::from_package(&package))
        .transpose()?;
    let start = Instant::now();
    let (mut verses, mut books) = (0, 0);
    match cache {
        Some(dir) => {
            let cache = Cache::new(Arc::new(FsStorage::new(dir)));
            let cached = cache.open(package)?;
            for id in cached.package().books() {
                let book = cached.book(id)?;
                verses += book.spans.len();
                books += 1;
                let counts = cached
                    .manifest()
                    .books
                    .iter()
                    .find(|(b, _)| *b == id)
                    .map(|(_, c)| c);
                if counts != Some(&book.chapter_verses()) {
                    let problem = format!("{}: the cached counts differ from the book's", id.0);
                    if !json {
                        println!("{problem}");
                    }
                    problems.push(problem);
                }
            }
        }
        None => {
            for id in package.books() {
                verses += package.source().book(id)?.spans.len();
                books += 1;
            }
        }
    }
    // Each file's title: a few are listed, many counted
    let untitled: Vec<&str> = (titles.iter())
        .filter(|t| t.title.is_none())
        .map(|t| t.path.as_str())
        .collect();
    let no_title = || {
        logoxide::LogoxideError::Manifest(format!(
            "no title for {} (give one in logoxide.toml's [titles])",
            untitled.join(", ")
        ))
    };
    if json {
        problems.extend(timings.iter().map(|p| format!("timings: {p}")));
        let ok = problems.is_empty() && untitled.is_empty();
        print_json(&CheckReport {
            id,
            version,
            kind: package_kind,
            books,
            verses,
            map: map.as_ref().map(|map| {
                serde_json::json!({
                    "from": map.from,
                    "to": map.to,
                    "lines": map.mappings().len(),
                    "verses": map.len(),
                })
            }),
            titles: (titles.iter())
                .map(|t| serde_json::json!({ "path": t.path, "title": t.title }))
                .collect(),
            untitled: untitled.iter().map(|p| p.to_string()).collect(),
            problems,
            ok,
        })?;
        timings_result(&timings)?;
        return match untitled.is_empty() {
            true => Ok(()),
            false => Err(no_title()),
        };
    }
    match (package_kind, map) {
        (_, Some(map)) => {
            println!(
                "a versification map from {} to {}: {} lines, {} verses mapped",
                map.from,
                map.to,
                map.mappings().len(),
                map.len()
            )
        }
        (logoxide::manifest::PackageKind::Collection, _) => {
            println!("a collection: no text of its own, only its dependencies")
        }
        _ if books == 0 => {
            println!("no verse text (a {package_kind:?}): its files are kept as they are")
        }
        _ => println!(
            "{books} books, {verses} verses, read in {:.0?}",
            start.elapsed()
        ),
    }
    let titled: Vec<String> = (titles.iter())
        .filter_map(|t| Some(format!("{} ({})", t.title.as_deref()?, t.path)))
        .collect();
    match titled.len() {
        0 => {}
        1..=5 => println!("titles: {}", titled.join(", ")),
        n => println!("{n} files titled: {} … {}", titled[0], titled[n - 1]),
    }
    timings_report(&timings)?;
    if !untitled.is_empty() {
        return Err(no_title());
    }
    Ok(())
}

/// Prints where a reference is in another numbering, and its verses that have no counterpart
fn map(cli: &Cli, reference: &str, from: &str, to: &str, with: &[PathBuf]) -> logoxide::Result<()> {
    use logoxide::versification::{VersificationMap, parse_reference};
    let library = cli.library();
    let project = cli.project(&library)?;
    let mut maps = project.versifications(&library)?;
    for dir in with {
        let package = Package::open(Arc::new(FsStorage::new(dir)), Features::default())?;
        maps.add(VersificationMap::from_package(&package)?);
    }
    let passage = parse_reference(reference)?;
    let (from, to) = (
        project.numbering(&library, from)?,
        project.numbering(&library, to)?,
    );
    let counts = from.counts.get(&passage.book);
    let mapped = maps.map(&passage, &from.versification, &to.versification, counts)?;
    match mapped.verses.is_empty() {
        true => println!("(no counterpart in {})", to.versification),
        false => println!("{} ({})", mapped.text(), to.versification),
    }
    if !mapped.missing.is_empty() {
        println!(
            "not in {}: {} ({})",
            to.versification,
            mapped.missing_text(),
            from.versification
        );
    }
    Ok(())
}

/// Prints a package's timing problems; an error if it has any
fn timings_report(problems: &[String]) -> logoxide::Result<()> {
    for problem in problems {
        println!("timings: {problem}");
    }
    timings_result(problems)
}

/// An error if a package has timing problems
fn timings_result(problems: &[String]) -> logoxide::Result<()> {
    match problems.len() {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Media(format!(
            "{n} problem{} with the timings",
            if n == 1 { "" } else { "s" }
        ))),
    }
}

fn version_req(text: &str) -> logoxide::Result<semver::VersionReq> {
    text.parse()
        .map_err(|e| logoxide::LogoxideError::Manifest(format!("version `{text}`: {e}")))
}

fn add(cli: &Cli, args: &AddArgs) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
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
    let mut dependency = DetailedDependency {
        git: git.clone(),
        path: args.path.clone(),
        version: args.version.as_deref().map(version_req).transpose()?,
        tag: args.tag.clone(),
        branch: args.branch.clone(),
        rev: args.rev.clone(),
        features: args.features.clone(),
        default_features: !args.no_default_features,
        ..DetailedDependency::default()
    };
    // The package's id names the dependency: read it from the package
    let info: Named = match (archive, &git, &args.path) {
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
            logoxide::git::Fetcher::fetch_files(&fetcher, url, &reference, &[MANIFEST], &probe)?;
            let manifest = logoxide::Manifest::parse(&probe.read_string(MANIFEST)?)?;
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
                    .filter_map(|(t, _)| logoxide::resolve::tag_version(t))
                    .filter(|v| v.pre.is_empty())
                    .max();
                if let Some(newest) = newest {
                    dependency.version = Some(version_req(&newest.to_string())?);
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
            let (theirs, their_aliases) = project.names(other.dependency_name(), package.info());
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
    project.add(&name, &dependency)?;
    project.save_manifest()?;
    let fetcher = cli.fetcher(&library);
    let lock = project.install(&library, &fetcher, Update::Only([name.clone()].into()))?;
    let locked = lock.get(&name).map(|p| p.label()).unwrap_or(name);
    println!(
        "added {locked} to {}: installed, and locked in {}",
        project.dir().join(MANIFEST).display(),
        logoxide::lock::LOCKFILE
    );
    Ok(())
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
            Ok(Some((catalog.clone(), entry.clone())))
        }
        _ if catalog.is_some() => Err(logoxide::LogoxideError::Catalog(format!(
            "`{name}` in {location} isn't published as archives"
        ))),
        _ => Ok(None),
    }
}

fn new(args: &NewArgs) -> logoxide::Result<()> {
    use logoxide::{create, manifest::PackageKind};
    let kind = (args.kind.as_deref())
        .map(|k| {
            serde_json::from_value::<PackageKind>(serde_json::Value::String(k.to_string()))
                .map_err(|_| {
                    logoxide::LogoxideError::Manifest(format!(
                        "--kind {k}: bible, commentary, lexicon, cross-references, versification-map, book, or collection"
                    ))
                })
        })
        .transpose()?;
    // Like `cargo new <dir>`: the folder names it, unless --id does
    let from_dir = (args.dir.as_ref())
        .and_then(|d| d.file_name())
        .map(|n| create::id_from(&n.to_string_lossy()));
    let given = create::NewPackage {
        id: args.id.clone().or(from_dir),
        name: args.name.clone(),
        abbreviation: args.abbreviation.clone(),
        authors: args.authors.clone(),
        description: args.description.clone(),
        version: (args.version.as_deref())
            .map(|v| {
                v.parse()
                    .map_err(|e| logoxide::LogoxideError::Manifest(format!("--version {v}: {e}")))
            })
            .transpose()?,
        kind,
        language: args.language.clone(),
        license: args.license.clone(),
        publisher: args.publisher.clone(),
        homepage: args.homepage.clone(),
        repository: args.repository.clone(),
        keywords: args.keywords.clone(),
        categories: args.categories.clone(),
        about_books: args.about_books.clone(),
    };
    let staged = MemoryStorage::new();
    let (manifest, rights) = match (&args.from, kind) {
        (Some(file), _) => {
            let bytes = std::fs::read(file).map_err(|e| logoxide::LogoxideError::Io {
                path: file.display().to_string(),
                message: e.to_string(),
            })?;
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
            let name = name.unwrap_or_default();
            let rights = match create::file_format(&name, &bytes) {
                Some(("epub", _)) => create::epub_metadata(&bytes)?.rights,
                _ => None,
            };
            (
                create::new_from_file(&name, &bytes, &given, &staged)?,
                rights,
            )
        }
        (None, Some(PackageKind::Collection)) => (create::new_collection(&given, &staged)?, None),
        (None, _) => {
            return Err(logoxide::LogoxideError::Manifest(String::from(
                "give the file it's made from (--from), or --kind collection",
            )));
        }
    };
    let info = manifest
        .package
        .as_ref()
        .ok_or_else(|| logoxide::LogoxideError::Manifest(String::from("no [package]")))?;
    let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from(&info.id));
    if dir.exists() && std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_some()) {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "{} already exists",
            dir.display()
        )));
    }
    let dest = FsStorage::new(&dir);
    for path in staged.walk("")? {
        dest.write(&path, &staged.read(&path)?)?;
    }
    if !args.no_git {
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status();
        if !status.is_ok_and(|s| s.success()) {
            eprintln!("warning: `git init` failed in {}", dir.display());
        }
    }
    println!("created {} ({:?}) in {}", info.id, info.kind, dir.display());
    print!("{}", staged.read_string(MANIFEST)?);
    if info.license.is_none() {
        println!(
            "no license given: add one with --license (an SPDX expression, CC0-1.0); it's shown, never enforced"
        );
    }
    if let Some(rights) = rights {
        println!("the file says of its rights: {rights}");
    }
    match info.kind {
        PackageKind::Collection => println!(
            "next: lx add --project {} <each package's git URL>, then lx release there",
            dir.display()
        ),
        _ => println!(
            "next: lx release in {} (commits and tags it)",
            dir.display()
        ),
    }
    Ok(())
}

fn remove(cli: &Cli, id: &str) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    if !project.remove(id)? {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "no dependency `{id}`"
        )));
    }
    project.save_manifest()?;
    project.install(&library, &cli.fetcher(&library), Update::Nothing)?;
    println!("removed {id}");
    Ok(())
}

fn set_enabled(cli: &Cli, name: &str, enabled: bool) -> logoxide::Result<()> {
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
        (true, true) => println!("{name} is already on"),
        (false, false) => println!("{name} is already off"),
        (_, true) => println!("turned {name} on"),
        (_, false) => {
            println!("turned {name} off (still installed: `lx enable {name}` turns it on)")
        }
    }
    Ok(())
}

fn repair(cli: &Cli) -> logoxide::Result<()> {
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
        println!("repaired {label}");
    }
    println!(
        "{} packages, {} repaired, locked in {}",
        lock.packages.len(),
        changed.len(),
        project.dir().join(logoxide::lock::LOCKFILE).display()
    );
    Ok(())
}

fn install(cli: &Cli, update: Update) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let before: Vec<String> = project
        .lock()
        .map(|l| l.packages.iter().map(|p| p.name()).collect())
        .unwrap_or_default();
    let lock = project.install(&library, &cli.fetcher(&library), update)?;
    for package in &lock.packages {
        let note = if before.contains(&package.name()) {
            ""
        } else {
            " (new)"
        };
        println!("{}{note}", package.label());
    }
    println!(
        "{} packages, locked in {}",
        lock.packages.len(),
        project.dir().join(logoxide::lock::LOCKFILE).display()
    );
    Ok(())
}

/// Prints whether a dependency is on, and returns it
fn is_enabled(cli: &Cli, name: &str) -> logoxide::Result<bool> {
    let library = cli.library();
    let project = cli.project(&library)?;
    if !project.manifest().dependencies.contains_key(name) {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "no dependency `{name}`"
        )));
    }
    let enabled = project.is_enabled(name);
    println!("{}", if enabled { "on" } else { "off" });
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
fn list(cli: &Cli, only: Option<bool>) -> logoxide::Result<()> {
    let library = cli.library();
    let project = cli.project(&library)?;
    let json = cli.format == Format::Json;
    let Some(lock) = project.lock() else {
        match json {
            true => println!("[]"),
            false => println!("nothing installed yet: `lx add --git URL`, then `lx install`"),
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
                abbreviation: info.map(|i| project.names(locked.dependency_name(), i).0),
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
            let (abbreviation, _) = project.names(locked.dependency_name(), info);
            format!(
                "{} ({}), {}",
                abbreviation,
                info.name,
                info.license.as_deref().unwrap_or("no license given")
            )
        });
        println!(
            "{}  {}  {}{root}{off}{installed}",
            locked.label(),
            about.unwrap_or_default(),
            source.trim()
        );
    }
    Ok(())
}

fn print_summary(summary: &logoxide::summary::Summary) {
    match summary.verses {
        0 => println!("{} {}", summary.id, summary.version),
        verses => println!(
            "{} {}: {} books, {verses} verses",
            summary.id,
            summary.version,
            summary.books.len(),
        ),
    }
    let codes: Vec<&str> = summary.books.keys().map(String::as_str).collect();
    if !codes.is_empty() {
        println!("books: {}", codes.join(" "));
    }
    for (format, size) in &summary.sizes {
        println!("{format}: {:.1} MB", *size as f64 / 1_000_000.0);
    }
    let contents: Vec<String> = summary
        .contents
        .iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    if !contents.is_empty() {
        println!("contents: {}", contents.join(", "));
    }
    if summary.references > 0 {
        println!("references: {} (in its books)", summary.references);
    }
    println!("text checksum: {}", summary.text_checksum);
}

fn show_remote(
    cli: &Cli,
    url: &str,
    tag: Option<&str>,
    references: Option<&str>,
) -> logoxide::Result<()> {
    use logoxide::git::Fetcher;
    let library = cli.library();
    let fetcher = cli.fetcher(&library);
    let tag = match tag {
        Some(tag) => tag.to_string(),
        None => {
            let tags = fetcher.tags(url)?;
            let newest = tags
                .into_iter()
                .filter_map(|(t, _)| Some((logoxide::resolve::tag_version(&t)?, t)))
                .max();
            newest
                .map(|(_, t)| t)
                .ok_or_else(|| logoxide::LogoxideError::Resolve(format!("{url} has no releases")))?
        }
    };
    let paths = [
        MANIFEST,
        logoxide::summary::SUMMARY,
        logoxide::summary::VERSES,
        logoxide::references::REFERENCES,
    ];
    let files = MemoryStorage::new();
    fetcher.fetch_files(
        url,
        &logoxide::git::GitRef::Tag(tag.clone()),
        &paths,
        &files,
    )?;
    let manifest = logoxide::Manifest::parse(&files.read_string(MANIFEST)?)?;
    let summary = logoxide::summary::Summary::read(&files).map(|(s, _)| s);
    if let Some(info) = &manifest.package {
        println!(
            "{} ({}), {}",
            info.abbreviation,
            info.name,
            info.license.as_deref().unwrap_or("no license given")
        );
    }
    match summary {
        Ok(summary) => {
            print_summary(&summary);
            if let Some(format) = references {
                print_cited(&summary.cited, format)?;
            }
        }
        Err(_) => println!(
            "{tag} has no summary (released before `lx release`): install it to see what it holds"
        ),
    }
    Ok(())
}

fn release(
    cli: &Cli,
    path: &std::path::Path,
    set_version: bool,
    dry_run: bool,
) -> logoxide::Result<()> {
    let library = cli.library();
    let git = cli.fetcher(&library);
    let mut plan = logoxide::release::plan(path, &git)?;
    if let Some((old, d)) = &plan.previous {
        println!(
            "since {old}: {} ({:?} change)",
            logoxide::release::describe(d),
            d.change()
        );
        for v in d.changed.iter().take(10) {
            println!("  reworded {}", v.reference());
        }
        for (from, to) in d.moved.iter().take(10) {
            println!("  {} is now {}", from.reference(), to.reference());
        }
        for v in d.removed.iter().take(10) {
            println!("  removed {}", v.reference());
        }
    }
    if logoxide::release::check(&plan).is_err()
        && set_version
        && let Some(minimum) = plan.minimum.clone()
    {
        let manifest_path = path.join(MANIFEST);
        let text =
            std::fs::read_to_string(&manifest_path).map_err(|e| logoxide::LogoxideError::Io {
                path: manifest_path.display().to_string(),
                message: e.to_string(),
            })?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| logoxide::LogoxideError::Manifest(e.to_string()))?;
        doc["package"]["version"] = toml_edit::value(minimum.to_string());
        if !dry_run {
            std::fs::write(&manifest_path, doc.to_string()).map_err(|e| {
                logoxide::LogoxideError::Io {
                    path: manifest_path.display().to_string(),
                    message: e.to_string(),
                }
            })?;
        }
        println!("version set to {minimum}");
        plan.summary.version = minimum;
        // The manifest changed: summarize again (its checksum covers logoxide.toml)
        if !dry_run {
            plan = logoxide::release::plan(path, &git)?;
        }
    }
    logoxide::release::check(&plan)?;
    if dry_run {
        let (_, prefix) = git.package_in(path)?;
        let tag = logoxide::release::tag_for(&prefix, &plan.summary.version);
        println!("would release {tag}");
        return Ok(());
    }
    let (tag, commit) = logoxide::release::release(path, &plan, &git)?;
    println!("released {tag} ({})", &commit[..commit.len().min(12)]);
    Ok(())
}

/// What changed between two versions, as `--format json` prints it (`lx diff`, `lx import
/// --update`)
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffReport {
    /// In words: `3 verses reworded, 1 removed`
    description: String,
    /// `patch`, `minor`, or `major`
    change: String,
    reworded: Vec<String>,
    /// Each verse's old reference and its new one
    moved: Vec<[String; 2]>,
    removed: Vec<String>,
    added: Vec<String>,
    bridged: Vec<String>,
    /// Files whose title changed: each one's path, old title, and new title
    retitled: Vec<[String; 3]>,
    /// For packages without verses: whether their files changed
    files_changed: bool,
}

impl DiffReport {
    fn of(d: &logoxide::summary::Diff) -> DiffReport {
        let references = |verses: &[logoxide::summary::VerseHash]| -> Vec<String> {
            verses.iter().map(|v| v.reference()).collect()
        };
        DiffReport {
            description: logoxide::release::describe(d),
            change: format!("{:?}", d.change()).to_lowercase(),
            reworded: references(&d.changed),
            moved: (d.moved.iter())
                .map(|(from, to)| [from.reference(), to.reference()])
                .collect(),
            removed: references(&d.removed),
            added: references(&d.added),
            bridged: references(&d.bridged),
            retitled: (d.retitled.iter())
                .map(|(path, old, new)| [path.clone(), old.clone(), new.clone()])
                .collect(),
            files_changed: d.files_changed,
        }
    }
}

fn diff(cli: &Cli, path: &std::path::Path, from: &str, to: Option<&str>) -> logoxide::Result<()> {
    let library = cli.library();
    let git = cli.fetcher(&library);
    // A package in a folder of its repository: its tags are `<folder>/v…`
    let (root, prefix) = git.package_in(path)?;
    let url = root.display().to_string();
    let tag = |t: &str| match t.starts_with(&prefix) {
        true => t.to_string(),
        false => format!("{prefix}{t}"),
    };
    let old = logoxide::release::summary_in(&git, &url, &tag(from), &prefix)?;
    let new = match to {
        Some(to) => logoxide::release::summary_in(&git, &url, &tag(to), &prefix)?,
        None => {
            let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(path));
            logoxide::summary::summarize(&Package::open(storage, Features::default())?)?
        }
    };
    let d = logoxide::summary::diff_summaries((&old.0, &old.1), (&new.0, &new.1));
    if cli.format == Format::Json {
        return print_json(&DiffReport::of(&d));
    }
    println!(
        "{}: a {:?} change",
        logoxide::release::describe(&d),
        d.change()
    );
    for v in &d.changed {
        println!("reworded {}", v.reference());
    }
    for (from, to) in &d.moved {
        println!("{} is now {}", from.reference(), to.reference());
    }
    for v in &d.removed {
        println!("removed {}", v.reference());
    }
    for v in &d.added {
        println!("added {}", v.reference());
    }
    for v in &d.bridged {
        println!("bridged differently {}", v.reference());
    }
    Ok(())
}

fn verify(args: &PackageArgs, json: bool) -> logoxide::Result<()> {
    let package = args.open()?;
    let (built, verses) = logoxide::summary::summarize(&package)?;
    let (published, published_verses) =
        logoxide::summary::Summary::read(package.storage().as_ref()).map_err(|e| {
            logoxide::LogoxideError::Manifest(format!(
                "no published summary ({e}): run `lx release`"
            ))
        })?;
    let mut problems = vec![];
    if built != published {
        problems.push(String::from("logoxide.summary.toml doesn't match the text"));
    }
    if verses != published_verses {
        let d = logoxide::summary::diff(&published_verses, &verses);
        problems.push(format!(
            "logoxide.verses.bin doesn't match the text: {}",
            logoxide::release::describe(&d)
        ));
    }
    if json {
        print_json(&serde_json::json!({
            "id": built.id,
            "version": built.version.to_string(),
            "matches": problems.is_empty(),
            "problems": problems,
        }))?;
    }
    if problems.is_empty() {
        if json {
            return Ok(());
        }
        println!(
            "{} {}: the summary matches its text",
            built.id, built.version
        );
        Ok(())
    } else {
        Err(logoxide::LogoxideError::Manifest(problems.join("; ")))
    }
}

/// Each catalog that can be read, in order; one that can't is a warning, unless none can be
fn load_catalogs(
    locations: &[String],
    base: &Path,
) -> logoxide::Result<Vec<logoxide::catalog::Catalog>> {
    let mut catalogs = vec![];
    for location in locations {
        match load_catalog(location, base) {
            Ok(catalog) => catalogs.push(catalog),
            Err(e) if locations.len() > 1 => eprintln!("warning: {e}"),
            Err(e) => return Err(e),
        }
    }
    if catalogs.is_empty() {
        return Err(logoxide::LogoxideError::Catalog(String::from(
            "no catalog could be read",
        )));
    }
    Ok(catalogs)
}

fn load_catalog(location: &str, base: &Path) -> logoxide::Result<logoxide::catalog::Catalog> {
    let bytes = if location.starts_with("https://") || location.starts_with("http://") {
        get(location)?
    } else {
        let path = base.join(location);
        std::fs::read(&path).map_err(|e| logoxide::LogoxideError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?
    };
    logoxide::catalog::Catalog::parse_bytes(location, &bytes)
}

/**
A package's git URL from what's typed: a name through one of the project's registries
(`dorean-local:books/romans`), `github:owner/repo`, or a URL
*/
fn registry_url(project: &logoxide::Project, text: &str) -> logoxide::Result<String> {
    let registries = config().registries(project.manifest());
    let Some((registry, name)) = logoxide::catalog::split_registry_name(text, &registries) else {
        return Ok(logoxide::git::expand_url(text));
    };
    let catalog = load_registry(registry, &registries[registry], project.dir(), true)?;
    let entry = catalog.find(name)?.ok_or_else(|| {
        logoxide::LogoxideError::Catalog(format!(
            "{registry} has no package `{name}` (lx search --registry {registry})"
        ))
    })?;
    match &entry.source {
        logoxide::catalog::CatalogSource::Git { url } => Ok(url.clone()),
        logoxide::catalog::CatalogSource::Registry { .. } => Err(logoxide::LogoxideError::Catalog(
            format!("{text} is published as archives, not in a git repository"),
        )),
        other => Err(logoxide::LogoxideError::Catalog(format!(
            "{text} is imported, not added: lx import {}",
            other.import_name().unwrap_or_default()
        ))),
    }
}

/// [`registry_url`] in the project `cli` names (`lx show --git dorean-local:kjv`)
fn package_url(cli: &Cli, text: &str) -> logoxide::Result<String> {
    if !text.contains(':') || text.contains("://") {
        return Ok(logoxide::git::expand_url(text));
    }
    registry_url(&cli.project(&cli.library())?, text)
}

/// A registry's location as a path: `~/` is the home folder, a relative one the project's
fn registry_path(location: &str, base: &Path) -> PathBuf {
    match (location.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => base.join(location),
    }
}

/**
A registry's packages: a folder's repositories (every git repository with a `logoxide.toml`, at any
depth, named by its path there), else a catalog at that URL or path
*/
fn load_registry(
    name: &str,
    location: &str,
    base: &Path,
    warn: bool,
) -> logoxide::Result<logoxide::catalog::Catalog> {
    let is_url = location.contains("://");
    let folder = registry_path(location, base);
    if is_url || !folder.is_dir() {
        let mut catalog = match is_url {
            true => load_catalog(location, base)?,
            // By its whole path, so a dependency names it from anywhere
            false => load_catalog(&canonical(&folder)?.display().to_string(), Path::new(""))?,
        };
        catalog.name = name.to_string();
        return Ok(catalog);
    }
    let (catalog, warnings) = logoxide::registry_build::folder_catalog(name, &canonical(&folder)?)?;
    if warn {
        for warning in warnings {
            eprintln!("warning: {warning}");
        }
    }
    Ok(catalog)
}

fn canonical(path: &Path) -> logoxide::Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| logoxide::LogoxideError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })
}

/**
`lx catalog build`: a catalog.toml of a folder's package repositories, each from its newest
release, with its path and commit; an entry whose path, commit, and URL are unchanged is kept as
it was (unless `force`)
*/
fn catalog_build(
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
    let built = logoxide::registry_build::build_catalog(&folder, previous.as_deref(), url, name)
        .map_err(|e| match e {
            logoxide::LogoxideError::Catalog(message) => {
                logoxide::LogoxideError::Catalog(format!("{}: {message}", output.display()))
            }
            e => e,
        })?;
    std::fs::write(&output, &built.text).map_err(|e| logoxide::LogoxideError::Io {
        path: output.display().to_string(),
        message: e.to_string(),
    })?;
    let plural = if built.count == 1 { "" } else { "s" };
    println!(
        "Wrote {}: {} package{plural} ({} read, {} unchanged)",
        output.display(),
        built.count,
        built.read,
        built.kept
    );
    for skip in built.skipped {
        println!("    skipped {skip}");
    }
    Ok(())
}

fn catalog_list(cli: &Cli) -> logoxide::Result<()> {
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
            println!("{name}: {location} (registry{from})");
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
            println!("{location}{note}");
        }
        catalogs.push(serde_json::json!({ "location": location, "from": from }));
    }
    if json {
        print_json(&serde_json::json!({ "registries": registries, "catalogs": catalogs }))?;
    }
    Ok(())
}

fn catalog_add(cli: &Cli, location: &str, name: Option<&str>) -> logoxide::Result<()> {
    let mut project = cli.project(&cli.library())?;
    if let Some(name) = name {
        logoxide::catalog::check_registry_name(name)?;
        let catalog = load_registry(name, location, project.dir(), true)?;
        if !project.add_registry(name, location)? {
            println!("{name} is already {location}");
            return Ok(());
        }
        project.save_manifest()?;
        let count = catalog.entries.len();
        let plural = if count == 1 { "" } else { "s" };
        println!("Added the registry {name} ({location}): {count} package{plural}");
        for entry in &catalog.entries {
            let path = entry.path.as_deref().unwrap_or(&entry.id);
            println!(
                "    {name}:{path}  {} {}",
                entry.id,
                entry.version.as_deref().unwrap_or("")
            );
        }
        return Ok(());
    }
    let catalog = load_catalog(location, project.dir())?;
    if !project.add_catalog(location)? {
        println!("{location} is already searched");
        return Ok(());
    }
    project.save_manifest()?;
    let count = catalog.entries.len();
    let plural = if count == 1 { "" } else { "s" };
    println!("Added {location}: {count} package{plural}");
    Ok(())
}

fn catalog_remove(cli: &Cli, location: &str) -> logoxide::Result<()> {
    let mut project = cli.project(&cli.library())?;
    if project.remove_registry(location)? {
        project.save_manifest()?;
        println!("Removed the registry {location}");
        return Ok(());
    }
    if !project.remove_catalog(location)? {
        return Err(logoxide::LogoxideError::Catalog(format!(
            "{location} isn't searched (lx catalog list)"
        )));
    }
    project.save_manifest()?;
    println!("Removed {location}");
    Ok(())
}

/// What a catalog entry is, wherever it's listed: its repository, or its registry and id
fn source_key(entry: &logoxide::catalog::CatalogEntry) -> String {
    match &entry.source {
        logoxide::catalog::CatalogSource::Git { url } => url.clone(),
        logoxide::catalog::CatalogSource::Registry { catalog } => format!("{catalog}#{}", entry.id),
        other => other.import_name().unwrap_or_default(),
    }
}

/// logoxide's categories, each with what it's for, as `--category` takes them
fn category_values() -> clap::builder::PossibleValuesParser {
    (logoxide::manifest::CATEGORIES.iter())
        .map(|(name, help)| clap::builder::PossibleValue::new(*name).help(*help))
        .collect::<Vec<_>>()
        .into()
}

/**
Which books packages must be about (their `about-books`), as topos filters references: including
a testament limits to it, included genres and books add up, and exclusions always win
(`--nt -g Gospels` is the four Gospels). Genres and names are topos's, with the data its
config.toml gives (`merge-data`, ...). A package is kept when one of its books is
*/
#[derive(Args, Debug)]
struct BookFilterArgs {
    /// Only packages about a book of this testament (old/new)
    #[arg(long = "testament", short = 't')]
    testaments: Vec<String>,
    /// Same as `-t new`
    #[arg(long)]
    nt: bool,
    /// Same as `-t old`
    #[arg(long)]
    ot: bool,
    /// Leave out books of this testament
    #[arg(long = "exclude-testament")]
    exclude_testaments: Vec<String>,
    /// Only packages about a book of this genre (`-g epistles`; topos's genres)
    #[arg(long = "genre", short = 'g')]
    genres: Vec<String>,
    /// Leave out books of this genre
    #[arg(long = "exclude-genre")]
    exclude_genres: Vec<String>,
    /// Only packages about this book (`-b romans`, `-b ROM`)
    #[arg(long = "book", short = 'b')]
    books: Vec<String>,
    /// Leave out this book
    #[arg(long = "exclude-book")]
    exclude_books: Vec<String>,
}

impl BookFilterArgs {
    fn filters(&self) -> logoxide::topos_config::BookFilters {
        let mut testaments = self.testaments.clone();
        testaments.extend(self.nt.then(|| String::from("new")));
        testaments.extend(self.ot.then(|| String::from("old")));
        logoxide::topos_config::BookFilters {
            testaments,
            exclude_testaments: self.exclude_testaments.clone(),
            genres: self.genres.clone(),
            exclude_genres: self.exclude_genres.clone(),
            books: self.books.clone(),
            exclude_books: self.exclude_books.clone(),
        }
    }
}

/// What `lx search` keeps besides what its words match
struct Filters<'a> {
    books: logoxide::topos_config::BookFilters,
    category: Option<&'a str>,
}

fn search(
    cli: &Cli,
    query: &str,
    catalogs: &[String],
    registry: Option<&str>,
    filters: Filters,
    limit: usize,
    json: bool,
) -> logoxide::Result<()> {
    // The books packages must be about, with the genres and names topos's config gives
    let books = match filters.books.is_empty() {
        true => None,
        false => {
            let config = logoxide::topos_config::read(None)?;
            (filters.books).codes(&logoxide::topos_config::bible_data(&config)?)?
        }
    };
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
                match load_registry(name, location, project.dir(), true) {
                    Ok(catalog) => registries.push(catalog),
                    Err(e) => eprintln!("warning: {name}: {e}"),
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
    // What names each registry's packages: `<registry>:<path>`, by their URLs
    let mut through: std::collections::HashMap<String, String> = Default::default();
    for catalog in &registries {
        for entry in &catalog.entries {
            let path = entry.path.as_deref().unwrap_or(&entry.id);
            through.insert(source_key(entry), format!("{}:{path}", catalog.name));
        }
    }
    let searched = registries.len() + catalogs.len();
    // Every registry package is listed, even two with one id (they differ by path)
    let mut all = logoxide::catalog::Catalog {
        name: String::new(),
        entries: registries.into_iter().flat_map(|c| c.entries).collect(),
    };
    if !catalogs.is_empty() {
        all = (load_catalogs(&catalogs, &base)?.into_iter()).fold(all, |all, c| all.merge(c));
    }
    let mut found = all.search(query);
    found.retain(|e| {
        (books.as_ref()).is_none_or(|codes| e.about_books.iter().any(|b| codes.contains(b)))
            && (filters.category).is_none_or(|c| e.categories.iter().any(|x| x == c))
    });
    if json {
        let entries: Vec<serde_json::Value> = (found.iter().take(limit))
            .map(|e| {
                let install = match (&e.source, e.source.import_name()) {
                    (_, Some(name)) => vec![String::from("import"), name],
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
                    "license": e.license,
                    "keywords": e.keywords,
                    "categories": e.categories,
                    "aboutBooks": e.about_books,
                    "install": install,
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(entries));
        return Ok(());
    }
    for entry in found.iter().take(limit) {
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
        println!(
            "{} ({}): {}, {language}, {}",
            entry.abbreviation,
            entry.id,
            entry.name,
            entry.license.as_deref().unwrap_or("no license given"),
        );
        if !entry.about_books.is_empty() {
            println!("    on {}", book_names(&entry.about_books));
        }
        println!("    {source}");
    }
    let more = found.len().saturating_sub(limit);
    println!(
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

fn import(cli: &Cli, source: &str) -> logoxide::Result<()> {
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
    let entry = (catalogs.iter())
        .find_map(|c| c.find_import(source))
        .ok_or_else(|| {
            logoxide::LogoxideError::Import(format!(
                "{source}: in none of this project's catalogs (lx catalog list)"
            ))
        })?;
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
    println!(
        "imported {} ({}) into {}",
        entry.id,
        entry.name,
        dir.display()
    );
    Ok(())
}

/// Downloads what a catalog entry's import needs, and makes its package in `dest`
fn import_files(
    entry: &logoxide::catalog::CatalogEntry,
    dest: &dyn Storage,
) -> logoxide::Result<()> {
    use logoxide::catalog::CatalogSource;
    match &entry.source {
        CatalogSource::Ebible { id } => {
            let zip = get(&logoxide::import::ebible_zip_url(id))?;
            logoxide::import::ebible(entry, &zip, dest)?;
        }
        CatalogSource::FetchBible { id, books } => {
            let mut files = vec![];
            for code in books {
                files.push((
                    code.clone(),
                    get(&logoxide::import::fetch_bible_book_url(id, code))?,
                ));
            }
            logoxide::import::fetch_bible(entry, &files, dest)?;
        }
        CatalogSource::Ccel { id } => {
            let xml = get(&logoxide::import::ccel_url(id))?;
            logoxide::import::ccel(entry, &xml, dest)?;
        }
        CatalogSource::Sword {
            repository, module, ..
        } => {
            // The first of its zip's URLs that has it
            let mut zip = Err(logoxide::LogoxideError::Import(format!("{module}: no zip")));
            for url in logoxide::import::sword_zip_urls(repository, module) {
                zip = get(&url);
                if zip.is_ok() {
                    break;
                }
            }
            logoxide::import::sword(entry, &zip?, dest)?;
        }
        CatalogSource::Git { .. } | CatalogSource::Registry { .. } => {
            return Err(logoxide::LogoxideError::Import(format!(
                "{} isn't imported: add it",
                entry.id
            )));
        }
    }
    Ok(())
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
fn import_update(cli: &Cli, name: Option<&str>, dry_run: bool, yes: bool) -> logoxide::Result<()> {
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
        let current: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir));
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
            && let Some((root, _)) = upstream.split_once("/packages/")
        {
            match load_catalog(&format!("{root}/mods.d.tar.gz"), project.dir()) {
                Ok(catalog) => catalogs.push(catalog),
                Err(e) => eprintln!("warning: {dep}: {e}"),
            }
        }
        let Some(entry) = catalogs.iter().find_map(|c| c.find_upstream(&upstream)) else {
            if name == Some(dep.as_str()) {
                return Err(logoxide::LogoxideError::Import(format!(
                    "{dep} ({upstream}) is in none of this project's catalogs (lx catalog list)"
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
            diff: None,
            raised: false,
            removed: vec![],
            error: None,
        };
        let version = package.info().version.clone();
        match update_import(
            entry,
            current,
            &version,
            &dir,
            &mut report,
            UpdateOptions { dry_run, yes, json },
        ) {
            Ok(()) if report.status == "updated" => {
                changed.insert(dep);
            }
            Ok(()) => {}
            Err(error) => {
                eprintln!("lx: {dep}: {error}");
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
        println!("this project has no imports");
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
    from: String,
    to: Option<String>,
    diff: Option<DiffReport>,
    /// The publisher's version isn't newer: the version was raised for the change
    raised: bool,
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
    version: &semver::Version,
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
            println!("{text}");
        }
    };
    if logoxide::import::import_is_current(entry, version) {
        say(format!("{dep} {from}: up to date ({import_name})"));
        return Ok(());
    }
    let fresh = Arc::new(MemoryStorage::new());
    import_files(entry, fresh.as_ref())?;
    let Some(update) = compare_import(current.clone(), &fresh)? else {
        say(format!("{dep} {from}: up to date ({import_name})"));
        return Ok(());
    };
    let raised = match update.bumped {
        true => ", the version raised for it: the publisher's isn't newer",
        false => "",
    };
    say(format!(
        "{dep} {} → {}: {} (a {:?} change{raised})",
        update.from,
        update.to,
        logoxide::release::describe(&update.diff),
        update.diff.change()
    ));
    let removed = import_removals(current.as_ref(), fresh.as_ref())?;
    report.status = "changed";
    report.to = Some(update.to.to_string());
    report.diff = Some(DiffReport::of(&update.diff));
    report.raised = update.bumped;
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
    apply_import(&FsStorage::new(dir), fresh.as_ref())?;
    report.status = "updated";
    Ok(())
}

fn catalog_entry(cli: &Cli, url: &str) -> logoxide::Result<()> {
    use logoxide::git::Fetcher;
    let library = cli.library();
    let fetcher = cli.fetcher(&library);
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
    print!("{}", logoxide::catalog::Catalog::toml_entry(&info, url));
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

fn info(cli: &Cli, id: &str) -> logoxide::Result<()> {
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
    let (abbreviation, aliases) = project.names(id, &i);
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
    println!("{} ({}), {}", locked.label(), abbreviation, i.name);
    if !aliases.is_empty() {
        println!("aliases: {}", aliases.join(", "));
    }
    println!(
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
    println!("from: {from}");
    println!(
        "in: {} ({} files, {:.1} MB)",
        dir.display(),
        files.len(),
        size as f64 / 1e6
    );
    match &locked.checksum {
        Some(expected) if *expected == checksum => println!("checksum: {checksum} (as locked)"),
        Some(expected) => println!(
            "checksum: {checksum} (CHANGED: locked as {expected}; lx install --repair puts its files back)"
        ),
        None => println!("checksum: {checksum}"),
    }
    if !others.is_empty() {
        println!("not the package's (kept): {}", others.join(", "));
    }
    println!(
        "{} books, {verses} verses ({})",
        books.len(),
        cached.manifest().source_format
    );
    Ok(())
}

fn clean(cli: &Cli, remove: bool, keep: &[PathBuf]) -> logoxide::Result<()> {
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
        println!("every installed version is used");
        return Ok(());
    }
    for (id, key) in &unused {
        let dir = library.root().join("packages").join(id).join(key);
        // Files that aren't the package's (a reader's annotations) are never removed: the folder
        // stays, and says why
        let others = logoxide::package::other_files(&FsStorage::new(&dir))?;
        if !others.is_empty() {
            println!(
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
            println!("removed {id} {key}");
        } else {
            println!("unused: {id} {key} ({})", dir.display());
        }
    }
    if !remove {
        println!(
            "{} unused (other vaults may use them: --keep <their folder>); lx clean --remove deletes them",
            unused.len()
        );
    }
    Ok(())
}

fn media_add(
    file: &std::path::Path,
    urls: &[String],
    book: Option<&str>,
    package: &std::path::Path,
) -> logoxide::Result<()> {
    let bytes = std::fs::read(file).map_err(|e| logoxide::LogoxideError::Io {
        path: file.display().to_string(),
        message: e.to_string(),
    })?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (url, mirrors) = urls.split_first().ok_or_else(|| {
        logoxide::LogoxideError::Media(String::from("where is it served? (--url)"))
    })?;
    let mut entry = logoxide::media::MediaFile::of(&name, url, &bytes);
    entry.mirrors = mirrors.to_vec();
    entry.book = book.map(str::to_uppercase);
    let path = package.join(MANIFEST);
    if !path.is_file() {
        return Err(logoxide::LogoxideError::NotFound(format!(
            "{} (the package's folder: --package)",
            path.display()
        )));
    }
    let mut manifest = Project::open(package)?;
    manifest.add_media(&entry)?;
    manifest.save_manifest()?;
    println!(
        "listed {} ({:.1} MB, sha256 {}) in {}",
        entry.path,
        entry.size as f64 / 1e6,
        &entry.sha256[..16],
        path.display()
    );
    Ok(())
}

fn media_check(package: &std::path::Path, offline: bool) -> logoxide::Result<()> {
    let opened = logoxide::Package::open(Arc::new(FsStorage::new(package)), Default::default())?;
    let files = (opened.manifest().media.as_ref())
        .map(|m| m.files.clone())
        .unwrap_or_default();
    if files.is_empty() {
        println!("no media listed");
        return Ok(());
    }
    let timings = logoxide::media::check_package_timings(&opened)?;
    if opened
        .manifest()
        .media
        .as_ref()
        .is_some_and(|m| m.timings.is_some())
        && timings.is_empty()
    {
        println!("timings: ok");
    }
    timings_report(&timings)?;
    if offline {
        return Ok(());
    }
    // Every mirror too: a host falls back to it, so it must serve the same bytes
    let (mut checked, mut failed) = (0, 0);
    for file in &files {
        for url in file.urls() {
            checked += 1;
            let shown = match url == file.url {
                true => file.path.clone(),
                false => format!("{} (mirror {url})", file.path),
            };
            // Hashed as it downloads: media can be larger than memory should hold
            match net::verify_media(url, file) {
                Ok(()) => println!("ok: {shown}"),
                Err(error) => {
                    failed += 1;
                    println!("FAILED: {shown}: {error}");
                }
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Fetch {
            url: package.display().to_string(),
            message: format!("{n} of {checked} media downloads don't match"),
        }),
    }
}
