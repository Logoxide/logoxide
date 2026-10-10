//! `bx`: biblix's command line

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

use biblix::{
    Cache, Features, FsStorage, Library, MemoryStorage, Package, Project, Storage, bible_data,
    git::GitCli,
    manifest::{Dependency, DetailedDependency, MANIFEST},
    resolve::Update,
};
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;

mod complete;
mod publish;

#[derive(Parser)]
#[command(
    name = "bx",
    version,
    about = "Bibles, commentaries, lexicons, books, and media as versioned packages"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// The folder with the biblix.toml to use: the current one if it has one, else the library's
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    /// The library (BIBLIX_HOME, else $XDG_DATA_HOME/biblix)
    #[arg(long, global = true)]
    library: Option<PathBuf>,
    /// The cache (BIBLIX_CACHE, else $XDG_CACHE_HOME/biblix)
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
}

impl Cli {
    fn library(&self) -> Library {
        Library::new(
            self.library.clone().unwrap_or_else(Library::default_root),
            self.cache_dir
                .clone()
                .unwrap_or_else(Library::default_cache),
        )
    }

    fn project(&self, library: &Library) -> biblix::Result<Project> {
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

impl biblix::git::Fetcher for Web {
    fn tags(&self, url: &str) -> biblix::Result<Vec<(String, String)>> {
        self.0.tags(url)
    }

    fn fetch(
        &self,
        url: &str,
        reference: &biblix::git::GitRef,
        dest: &dyn Storage,
    ) -> biblix::Result<String> {
        self.0.fetch(url, reference, dest)
    }

    fn fetch_files(
        &self,
        url: &str,
        reference: &biblix::git::GitRef,
        paths: &[&str],
        dest: &dyn Storage,
    ) -> biblix::Result<String> {
        self.0.fetch_files(url, reference, paths, dest)
    }

    fn download(&self, url: &str) -> biblix::Result<Vec<u8>> {
        match url.starts_with("https://") || url.starts_with("http://") {
            true => get(url),
            false => biblix::git::read_local(url),
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
        /// offsets), read from the books (so not with --git)
        #[arg(long, requires = "references", conflicts_with = "git")]
        locations: bool,
        /// How to list them: text, tsv, or json
        #[arg(long, default_value = "text", requires = "references")]
        format: String,
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
    /// a collection (`--kind collection`), whose packages are its dependencies (`bx add
    /// --project <its folder>`). Then `bx release` in it commits and tags it
    New(NewArgs),
    /// Starts a biblix.toml in a folder (the current one by default), like a vault's
    Init {
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Adds a package to biblix.toml, installs it, and locks it
    Add(AddArgs),
    /// Removes a package from biblix.toml (its files stay in the library)
    Remove {
        /// The package's id
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        id: String,
    },
    /// Installs what biblix.lock says (resolving what it doesn't have yet)
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
    /// The packages biblix.toml asks for, as locked
    List,
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
    /// then eBible.org's and fetch.bible's lists: bx catalog list)
    Search {
        /// Words to find (every one must match)
        query: Vec<String>,
        /// A catalog to search (a URL or a path; a `.csv` is eBible.org's list), instead of the
        /// project's
        #[arg(long)]
        catalog: Vec<String>,
        /// Search only this registry (bx catalog list); with no words, list its packages
        #[arg(long, conflicts_with = "catalog", add = ArgValueCompleter::new(complete::registry))]
        registry: Option<String>,
        /// How many to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// One JSON array, for editors: each package's id, names, language, license, and the bx
        /// arguments that install it
        #[arg(long)]
        json: bool,
    },
    /// Makes a package from another publisher's download, puts it in the project's `imports/`,
    /// and adds it as a path package: `ebible:<id>` or `fetch:<id>` (as `bx search` shows), from
    /// the project's catalogs
    Import {
        /// `ebible:eng-asv`, `fetch:eng_bsb`
        source: String,
    },
    /// Catalogs: lists of packages anyone can host
    #[command(subcommand)]
    Catalog(CatalogCommand),
    /// Registries of archives: each package release is one file, so a registry of thousands of
    /// books needs no repository for each, and any static host (or GitHub's releases) serves it
    #[command(subcommand)]
    Registry(RegistryCommand),
    /// Adds a package's release to a registry: its files at its release tag (bx release first),
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
    /// versification maps: `bx map "Psalm 51:1" --from English --to Original`. --from and --to
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
    /// Lists a file in the package's biblix.toml: its URL, size, and SHA-256 (from the file here)
    Add {
        /// The file, as it's served
        file: PathBuf,
        /// Where it's served
        #[arg(long)]
        url: String,
        /// The book it reads (a USFM code)
        #[arg(long)]
        book: Option<String>,
        /// The package's folder
        #[arg(long, default_value = ".")]
        package: PathBuf,
    },
    /// Checks the timings (WebVTT, a cue per verse) against the text, then downloads every
    /// listed file and checks its size and hash
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
    /// The catalogs `bx search` looks through: the project's, then eBible.org's and fetch.bible's
    List,
    /// Searches a catalog too (a URL, or a path relative to the project): a catalog.toml,
    /// eBible.org's translations.csv, or fetch.bible's manifest.json. It's read first, to check it.
    /// With --name, a registry: a catalog or a folder of package repositories, whose packages
    /// `<name>:<id>` or `<name>:<path in the folder>` then name (`bx add dorean-local:kjv`)
    Add {
        location: String,
        /// Name it as a registry
        #[arg(long)]
        name: Option<String>,
    },
    /// Stops searching a catalog, or a registry (by its name). Removing eBible.org's or
    /// fetch.bible's turns the defaults off (`default-catalogs = false`), keeping the other one
    Remove {
        #[arg(add = ArgValueCompleter::new(complete::searched))]
        location: String,
    },
    /// A catalog.toml entry for a package's repository (its newest release's biblix.toml)
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
    /// without naming the registry first: `bx add --registry https://…/catalog.toml romans`
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
    /// Don't make the folder a git repository
    #[arg(long)]
    no_git: bool,
}

#[derive(Args)]
struct PackageArgs {
    /// The package's folder (with its biblix.toml)
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
    fn open(&self) -> biblix::Result<Package> {
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
    // `COMPLETE=bash bx` writes the shell's completion script; the shell then runs bx to complete
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Show {
            package,
            git: None,
            references,
            locations,
            format,
            ..
        } => show(
            &cli,
            package,
            references.then_some((format.as_str(), *locations)),
        ),
        Command::Show {
            git: Some(url),
            tag,
            references,
            format,
            ..
        } => package_url(&cli, url).and_then(|url| {
            show_remote(
                &cli,
                &url,
                tag.as_deref(),
                references.then_some(format.as_str()),
            )
        }),
        Command::Release {
            path,
            set_version,
            dry_run,
        } => release(&cli, path, *set_version, *dry_run),
        Command::Diff { path, from, to } => diff(&cli, path, from, to.as_deref()),
        Command::Verify(args) => verify(args),
        Command::Check { package, cache } => check(package, cache.clone()),
        Command::New(args) => new(args),
        Command::Init { dir } => biblix::Project::init(dir)
            .map(|p| println!("created {}", p.dir().join(MANIFEST).display())),
        Command::Add(args) => add(&cli, args),
        Command::Remove { id } => remove(&cli, id),
        Command::Install { repair: false } => install(&cli, Update::Nothing),
        Command::Install { repair: true } => repair(&cli),
        Command::Update { ids } => install(
            &cli,
            match ids.is_empty() {
                true => Update::All,
                false => Update::Only(ids.iter().cloned().collect()),
            },
        ),
        Command::List => list(&cli),
        Command::Info { id } => info(&cli, id),
        Command::Clean { remove, keep } => clean(&cli, *remove, keep),
        Command::Search {
            query,
            catalog,
            registry,
            limit,
            json,
        } => search(
            &cli,
            &query.join(" "),
            catalog,
            registry.as_deref(),
            *limit,
            *json,
        ),
        Command::Import { source } => import(&cli, source),
        Command::Catalog(CatalogCommand::Entry { git }) => {
            catalog_entry(&cli, &biblix::git::expand_url(git))
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
            eprintln!("bx: {error}");
            ExitCode::FAILURE
        }
    }
}

fn show(cli: &Cli, args: &PackageArgs, references: Option<(&str, bool)>) -> biblix::Result<()> {
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
        Some((format, false)) => print_cited(&biblix::references::cited(&found), format)?,
        None => {}
    }
    Ok(())
}

/// The passages cited, with how often, as text, TSV, or JSON
fn print_cited(cited: &[biblix::references::Cited], format: &str) -> biblix::Result<()> {
    match format {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(cited)
                .map_err(|e| biblix::BiblixError::Manifest(e.to_string()))?
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
            return Err(biblix::BiblixError::Manifest(format!(
                "--format {other}: text, tsv, or json"
            )));
        }
    }
    Ok(())
}

/// Every reference, with where it is: as text (its passage, chapter, and paragraph, shortened),
/// TSV, or JSON
fn print_references(
    found: &[biblix::references::FoundReference],
    format: &str,
) -> biblix::Result<()> {
    match format {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(found)
                .map_err(|e| biblix::BiblixError::Manifest(e.to_string()))?
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
            return Err(biblix::BiblixError::Manifest(format!(
                "--format {other}: text, tsv, or json"
            )));
        }
    }
    Ok(())
}

fn check(args: &PackageArgs, cache: Option<PathBuf>) -> biblix::Result<()> {
    let package = args.open()?;
    let package_kind = package.info().kind;
    let timings = biblix::media::check_package_timings(&package)?;
    let titles = biblix::titles::titles(&package)?;
    // A map is read whole: its lines parse, name known books, and don't overlap
    let map = (package_kind == biblix::manifest::PackageKind::VersificationMap)
        .then(|| biblix::versification::VersificationMap::from_package(&package))
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
                    println!("{}: the cached counts differ from the book's", id.0);
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
        (biblix::manifest::PackageKind::Collection, _) => {
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
    // Each file's title: a few are listed, many counted
    let untitled: Vec<&str> = (titles.iter())
        .filter(|t| t.title.is_none())
        .map(|t| t.path.as_str())
        .collect();
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
        return Err(biblix::BiblixError::Manifest(format!(
            "no title for {} (give one in biblix.toml's [titles])",
            untitled.join(", ")
        )));
    }
    Ok(())
}

/// Prints where a reference is in another numbering, and its verses that have no counterpart
fn map(cli: &Cli, reference: &str, from: &str, to: &str, with: &[PathBuf]) -> biblix::Result<()> {
    use biblix::versification::{VersificationMap, parse_reference};
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
fn timings_report(problems: &[String]) -> biblix::Result<()> {
    for problem in problems {
        println!("timings: {problem}");
    }
    match problems.len() {
        0 => Ok(()),
        n => Err(biblix::BiblixError::Media(format!(
            "{n} problem{} with the timings",
            if n == 1 { "" } else { "s" }
        ))),
    }
}

fn version_req(text: &str) -> biblix::Result<semver::VersionReq> {
    text.parse()
        .map_err(|e| biblix::BiblixError::Manifest(format!("version `{text}`: {e}")))
}

fn add(cli: &Cli, args: &AddArgs) -> biblix::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let named = args.repository.as_ref().or(args.git.as_ref());
    // A registry's package published as archives: added with `registry`, not `git`
    let archive = match (&args.registry, named) {
        (Some(_), None) => {
            return Err(biblix::BiblixError::Manifest(String::from(
                "name the package: bx add --registry <catalog> <id or path>",
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
                (Some(t), _, _) => biblix::git::GitRef::Tag(t.clone()),
                (_, Some(b), _) => biblix::git::GitRef::Branch(b.clone()),
                (_, _, Some(r)) => biblix::git::GitRef::Rev(r.clone()),
                _ => biblix::git::GitRef::Head,
            };
            // Only its manifest, where the host allows that
            let probe = MemoryStorage::new();
            biblix::git::Fetcher::fetch_files(&fetcher, url, &reference, &[MANIFEST], &probe)?;
            let manifest = biblix::Manifest::parse(&probe.read_string(MANIFEST)?)?;
            let info = manifest
                .package
                .ok_or_else(|| biblix::BiblixError::Manifest(format!("{url}: no [package]")))?;
            // Like Cargo: without a tag, branch, or revision, require the newest release's version
            if dependency.version.is_none()
                && dependency.tag.is_none()
                && dependency.branch.is_none()
                && dependency.rev.is_none()
            {
                let tags = biblix::git::Fetcher::tags(&fetcher, url)?;
                let newest = tags
                    .iter()
                    .filter_map(|(t, _)| biblix::resolve::tag_version(t))
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
            let manifest = biblix::Manifest::parse(&storage.read_string(MANIFEST)?)?;
            manifest
                .package
                .ok_or_else(|| biblix::BiblixError::Manifest(format!("{path}: no [package]")))?
                .into()
        }
        (None, None, None) => {
            return Err(biblix::BiblixError::Manifest(String::from(
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
    if let Some(biblix::manifest::Dependency::Detailed(existing)) =
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
        return Err(biblix::BiblixError::Manifest(format!(
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
                return Err(biblix::BiblixError::Manifest(format!(
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
        biblix::lock::LOCKFILE
    );
    Ok(())
}

/// What a package being added is called: its id, abbreviation, and aliases
struct Named {
    id: String,
    abbreviation: String,
    aliases: Vec<String>,
}

impl From<biblix::manifest::Package> for Named {
    fn from(info: biblix::manifest::Package) -> Named {
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
    project: &biblix::Project,
    text: &str,
    catalog: Option<&str>,
) -> biblix::Result<Option<(String, biblix::catalog::CatalogEntry)>> {
    let registries = project.manifest().registries();
    let (registry, location, name) = match catalog {
        Some(catalog) => ("--registry", catalog.to_string(), text),
        None => match biblix::catalog::split_registry_name(text, &registries) {
            Some((registry, name)) => (registry, registries[registry].clone(), name),
            None => return Ok(None),
        },
    };
    let loaded = load_registry(registry, &location, project.dir(), true)?;
    let entry = loaded.find(name)?.ok_or_else(|| {
        biblix::BiblixError::Catalog(format!("{location} has no package `{name}`"))
    })?;
    match &entry.source {
        biblix::catalog::CatalogSource::Registry { catalog } => {
            Ok(Some((catalog.clone(), entry.clone())))
        }
        _ if catalog.is_some() => Err(biblix::BiblixError::Catalog(format!(
            "`{name}` in {location} isn't published as archives"
        ))),
        _ => Ok(None),
    }
}

fn new(args: &NewArgs) -> biblix::Result<()> {
    use biblix::{create, manifest::PackageKind};
    let kind = (args.kind.as_deref())
        .map(|k| {
            serde_json::from_value::<PackageKind>(serde_json::Value::String(k.to_string()))
                .map_err(|_| {
                    biblix::BiblixError::Manifest(format!(
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
                    .map_err(|e| biblix::BiblixError::Manifest(format!("--version {v}: {e}")))
            })
            .transpose()?,
        kind,
        language: args.language.clone(),
        license: args.license.clone(),
        publisher: args.publisher.clone(),
        homepage: args.homepage.clone(),
    };
    let staged = MemoryStorage::new();
    let (manifest, rights) = match (&args.from, kind) {
        (Some(file), _) => {
            let bytes = std::fs::read(file).map_err(|e| biblix::BiblixError::Io {
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
            return Err(biblix::BiblixError::Manifest(String::from(
                "give the file it's made from (--from), or --kind collection",
            )));
        }
    };
    let info = manifest
        .package
        .as_ref()
        .ok_or_else(|| biblix::BiblixError::Manifest(String::from("no [package]")))?;
    let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from(&info.id));
    if dir.exists() && std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_some()) {
        return Err(biblix::BiblixError::Manifest(format!(
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
            "next: bx add --project {} <each package's git URL>, then bx release there",
            dir.display()
        ),
        _ => println!(
            "next: bx release in {} (commits and tags it)",
            dir.display()
        ),
    }
    Ok(())
}

fn remove(cli: &Cli, id: &str) -> biblix::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    if !project.remove(id)? {
        return Err(biblix::BiblixError::Manifest(format!(
            "no dependency `{id}`"
        )));
    }
    project.save_manifest()?;
    project.install(&library, &cli.fetcher(&library), Update::Nothing)?;
    println!("removed {id}");
    Ok(())
}

fn repair(cli: &Cli) -> biblix::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    // Which installed versions changed, to say what was put back
    let changed: Vec<String> = (project.lock().into_iter())
        .flat_map(|lock| lock.packages.iter())
        .filter(|p| p.checksum.is_some() && library.is_installed(p, project.dir()))
        .filter(|p| {
            let dir = FsStorage::new(library.package_dir(p, project.dir()));
            biblix::package::checksum(&dir).ok() != p.checksum
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
        project.dir().join(biblix::lock::LOCKFILE).display()
    );
    Ok(())
}

fn install(cli: &Cli, update: Update) -> biblix::Result<()> {
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
        project.dir().join(biblix::lock::LOCKFILE).display()
    );
    Ok(())
}

fn list(cli: &Cli) -> biblix::Result<()> {
    let library = cli.library();
    let project = cli.project(&library)?;
    let Some(lock) = project.lock() else {
        println!("nothing installed yet: `bx add --git URL`, then `bx install`");
        return Ok(());
    };
    for locked in &lock.packages {
        let root = if lock.roots().any(|r| std::ptr::eq(r, locked)) {
            ""
        } else {
            "  (a dependency)"
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
            "{}  {}  {}{root}{installed}",
            locked.label(),
            about.unwrap_or_default(),
            source.trim()
        );
    }
    Ok(())
}

fn print_summary(summary: &biblix::summary::Summary) {
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
) -> biblix::Result<()> {
    use biblix::git::Fetcher;
    let library = cli.library();
    let fetcher = cli.fetcher(&library);
    let tag = match tag {
        Some(tag) => tag.to_string(),
        None => {
            let tags = fetcher.tags(url)?;
            let newest = tags
                .into_iter()
                .filter_map(|(t, _)| Some((biblix::resolve::tag_version(&t)?, t)))
                .max();
            newest
                .map(|(_, t)| t)
                .ok_or_else(|| biblix::BiblixError::Resolve(format!("{url} has no releases")))?
        }
    };
    let paths = [
        MANIFEST,
        biblix::summary::SUMMARY,
        biblix::summary::VERSES,
        biblix::references::REFERENCES,
    ];
    let files = MemoryStorage::new();
    fetcher.fetch_files(url, &biblix::git::GitRef::Tag(tag.clone()), &paths, &files)?;
    let manifest = biblix::Manifest::parse(&files.read_string(MANIFEST)?)?;
    let summary = biblix::summary::Summary::read(&files).map(|(s, _)| s);
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
            "{tag} has no summary (released before `bx release`): install it to see what it holds"
        ),
    }
    Ok(())
}

fn release(
    cli: &Cli,
    path: &std::path::Path,
    set_version: bool,
    dry_run: bool,
) -> biblix::Result<()> {
    let library = cli.library();
    let git = cli.fetcher(&library);
    let mut plan = biblix::release::plan(path, &git)?;
    if let Some((old, d)) = &plan.previous {
        println!(
            "since {old}: {} ({:?} change)",
            biblix::release::describe(d),
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
    if biblix::release::check(&plan).is_err()
        && set_version
        && let Some(minimum) = plan.minimum.clone()
    {
        let manifest_path = path.join(MANIFEST);
        let text =
            std::fs::read_to_string(&manifest_path).map_err(|e| biblix::BiblixError::Io {
                path: manifest_path.display().to_string(),
                message: e.to_string(),
            })?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| biblix::BiblixError::Manifest(e.to_string()))?;
        doc["package"]["version"] = toml_edit::value(minimum.to_string());
        if !dry_run {
            std::fs::write(&manifest_path, doc.to_string()).map_err(|e| {
                biblix::BiblixError::Io {
                    path: manifest_path.display().to_string(),
                    message: e.to_string(),
                }
            })?;
        }
        println!("version set to {minimum}");
        plan.summary.version = minimum;
        // The manifest changed: summarize again (its checksum covers biblix.toml)
        if !dry_run {
            plan = biblix::release::plan(path, &git)?;
        }
    }
    biblix::release::check(&plan)?;
    if dry_run {
        let (_, prefix) = git.package_in(path)?;
        let tag = biblix::release::tag_for(&prefix, &plan.summary.version);
        println!("would release {tag}");
        return Ok(());
    }
    let (tag, commit) = biblix::release::release(path, &plan, &git)?;
    println!("released {tag} ({})", &commit[..commit.len().min(12)]);
    Ok(())
}

fn diff(cli: &Cli, path: &std::path::Path, from: &str, to: Option<&str>) -> biblix::Result<()> {
    let library = cli.library();
    let git = cli.fetcher(&library);
    // A package in a folder of its repository: its tags are `<folder>/v…`
    let (root, prefix) = git.package_in(path)?;
    let url = root.display().to_string();
    let tag = |t: &str| match t.starts_with(&prefix) {
        true => t.to_string(),
        false => format!("{prefix}{t}"),
    };
    let old = biblix::release::summary_in(&git, &url, &tag(from), &prefix)?;
    let new = match to {
        Some(to) => biblix::release::summary_in(&git, &url, &tag(to), &prefix)?,
        None => {
            let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(path));
            biblix::summary::summarize(&Package::open(storage, Features::default())?)?
        }
    };
    let d = biblix::summary::diff_summaries((&old.0, &old.1), (&new.0, &new.1));
    println!(
        "{}: a {:?} change",
        biblix::release::describe(&d),
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

fn verify(args: &PackageArgs) -> biblix::Result<()> {
    let package = args.open()?;
    let (built, verses) = biblix::summary::summarize(&package)?;
    let (published, published_verses) = biblix::summary::Summary::read(package.storage().as_ref())
        .map_err(|e| {
            biblix::BiblixError::Manifest(format!("no published summary ({e}): run `bx release`"))
        })?;
    let mut problems = vec![];
    if built != published {
        problems.push(String::from("biblix.summary.toml doesn't match the text"));
    }
    if verses != published_verses {
        let d = biblix::summary::diff(&published_verses, &verses);
        problems.push(format!(
            "biblix.verses.bin doesn't match the text: {}",
            biblix::release::describe(&d)
        ));
    }
    if problems.is_empty() {
        println!(
            "{} {}: the summary matches its text",
            built.id, built.version
        );
        Ok(())
    } else {
        Err(biblix::BiblixError::Manifest(problems.join("; ")))
    }
}

/// A URL's body (catalogs and imports; packages come through git)
fn get(url: &str) -> biblix::Result<Vec<u8>> {
    let fail = |e: ureq::Error| biblix::BiblixError::Fetch {
        url: url.to_string(),
        message: e.to_string(),
    };
    let mut response = ureq::get(url).call().map_err(fail)?;
    response
        .body_mut()
        .with_config()
        .limit(1 << 30)
        .read_to_vec()
        .map_err(fail)
}

/// A catalog from a URL or a path (relative to `base`)
/// Each catalog that can be read, in order; one that can't is a warning, unless none can be
fn load_catalogs(
    locations: &[String],
    base: &Path,
) -> biblix::Result<Vec<biblix::catalog::Catalog>> {
    let mut catalogs = vec![];
    for location in locations {
        match load_catalog(location, base) {
            Ok(catalog) => catalogs.push(catalog),
            Err(e) if locations.len() > 1 => eprintln!("warning: {e}"),
            Err(e) => return Err(e),
        }
    }
    if catalogs.is_empty() {
        return Err(biblix::BiblixError::Catalog(String::from(
            "no catalog could be read",
        )));
    }
    Ok(catalogs)
}

fn load_catalog(location: &str, base: &Path) -> biblix::Result<biblix::catalog::Catalog> {
    let bytes = if location.starts_with("https://") || location.starts_with("http://") {
        get(location)?
    } else {
        let path = base.join(location);
        std::fs::read(&path).map_err(|e| biblix::BiblixError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?
    };
    biblix::catalog::Catalog::parse(location, &String::from_utf8_lossy(&bytes))
}

/**
A package's git URL from what's typed: a name through one of the project's registries
(`dorean-local:books/romans`), `github:owner/repo`, or a URL
*/
fn registry_url(project: &biblix::Project, text: &str) -> biblix::Result<String> {
    let registries = project.manifest().registries();
    let Some((registry, name)) = biblix::catalog::split_registry_name(text, &registries) else {
        return Ok(biblix::git::expand_url(text));
    };
    let catalog = load_registry(registry, &registries[registry], project.dir(), true)?;
    let entry = catalog.find(name)?.ok_or_else(|| {
        biblix::BiblixError::Catalog(format!(
            "{registry} has no package `{name}` (bx search --registry {registry})"
        ))
    })?;
    match &entry.source {
        biblix::catalog::CatalogSource::Git { url } => Ok(url.clone()),
        biblix::catalog::CatalogSource::Registry { .. } => Err(biblix::BiblixError::Catalog(
            format!("{text} is published as archives, not in a git repository"),
        )),
        other => Err(biblix::BiblixError::Catalog(format!(
            "{text} is imported, not added: bx import {}",
            other.import_name().unwrap_or_default()
        ))),
    }
}

/// [`registry_url`] in the project `cli` names (`bx show --git dorean-local:kjv`)
fn package_url(cli: &Cli, text: &str) -> biblix::Result<String> {
    if !text.contains(':') || text.contains("://") {
        return Ok(biblix::git::expand_url(text));
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
A registry's packages: a folder's repositories (every git repository with a `biblix.toml`, at any
depth, named by its path there), else a catalog at that URL or path
*/
fn load_registry(
    name: &str,
    location: &str,
    base: &Path,
    warn: bool,
) -> biblix::Result<biblix::catalog::Catalog> {
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
    let folder = canonical(&folder)?;
    let mut packages = vec![];
    for (path, dir) in package_repositories(&folder)? {
        match newest_release(&dir) {
            Ok(Some(release)) => match biblix::Manifest::parse(&release.manifest) {
                Ok(parsed) => packages.push((path, parsed)),
                Err(e) if warn => eprintln!("warning: {path} {}: {e}", release.tag),
                Err(_) => {}
            },
            Ok(None) => {}
            Err(e) if warn => eprintln!("warning: {path}: {e}"),
            Err(_) => {}
        }
    }
    let url = format!("file://{}", folder.display());
    Ok(biblix::catalog::Catalog::from_packages(
        name, &url, packages,
    ))
}

fn canonical(path: &Path) -> biblix::Result<PathBuf> {
    path.canonicalize().map_err(|e| biblix::BiblixError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

/**
Every package repository in a folder, at any depth (a git repository with a `biblix.toml`), with
its path there (`books/romans`), in path order. A repository's own folders and hidden folders
aren't looked in
*/
fn package_repositories(folder: &Path) -> biblix::Result<Vec<(String, PathBuf)>> {
    let mut found = vec![];
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if dir.join(".git").exists() && dir.join(MANIFEST).is_file() {
            let path = dir.strip_prefix(folder).unwrap_or(&dir);
            let path: Vec<String> = (path.components())
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            found.push((path.join("/"), dir));
            continue;
        }
        let entries = std::fs::read_dir(&dir).map_err(|e| biblix::BiblixError::Io {
            path: dir.display().to_string(),
            message: e.to_string(),
        })?;
        for entry in entries.flatten() {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if !hidden && entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

/// A local repository's newest release
struct Release {
    tag: String,
    commit: String,
    /// Its `biblix.toml` at that tag
    manifest: String,
}

/// Runs git in a repository; what it printed
fn git_in(repo: &Path, args: &[&str]) -> biblix::Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| biblix::BiblixError::Io {
            path: repo.display().to_string(),
            message: format!("git: {e}"),
        })?;
    if !output.status.success() {
        return Err(biblix::BiblixError::Io {
            path: repo.display().to_string(),
            message: format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/**
A local repository's newest release (its highest `v1.2.3` tag, not a prerelease) and its
`biblix.toml` there, read from git without checking anything out; none before its first release
*/
fn newest_release(repo: &Path) -> biblix::Result<Option<Release>> {
    let refs = git_in(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:strip=2) %(objectname) %(*objectname)",
            "refs/tags",
        ],
    )?;
    let newest = (refs.lines())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let tag = parts.next()?;
            let object = parts.next()?;
            // An annotated tag names its commit after the tag object
            let commit = parts.next().unwrap_or(object);
            let version = biblix::resolve::tag_version(tag).filter(|v| v.pre.is_empty())?;
            Some((version, tag.to_string(), commit.to_string()))
        })
        .max_by(|a, b| a.0.cmp(&b.0));
    let Some((_, tag, commit)) = newest else {
        return Ok(None);
    };
    let manifest = git_in(repo, &["show", &format!("{commit}:{MANIFEST}")])?;
    Ok(Some(Release {
        tag,
        commit,
        manifest,
    }))
}

/**
`bx catalog build`: a catalog.toml of a folder's package repositories, each from its newest
release, with its path and commit; an entry whose path, commit, and URL are unchanged is kept as
it was (unless `force`)
*/
fn catalog_build(
    dir: &Path,
    output: Option<&Path>,
    url: Option<&str>,
    name: Option<&str>,
    force: bool,
) -> biblix::Result<()> {
    let folder = canonical(dir)?;
    let output = output.map_or_else(|| folder.join("catalog.toml"), Path::to_path_buf);
    let name = name.map(String::from).unwrap_or_else(|| {
        (folder.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    // The entries written last time, by path
    let mut previous: std::collections::HashMap<String, toml_edit::Table> = Default::default();
    if !force && let Ok(text) = std::fs::read_to_string(&output) {
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| biblix::BiblixError::Catalog(format!("{}: {e}", output.display())))?;
        if let Some(packages) = doc.get("package").and_then(|p| p.as_array_of_tables()) {
            for table in packages.iter() {
                if let Some(path) = table.get("path").and_then(|p| p.as_str()) {
                    previous.insert(path.to_string(), table.clone());
                }
            }
        }
    }
    let (mut tables, mut read, mut kept, mut skipped) = (vec![], 0, 0, vec![]);
    for (path, repo) in package_repositories(&folder)? {
        let folder_name = path.rsplit('/').next().unwrap_or(&path);
        let git = match url {
            Some(template) => template
                .replace("{path}", &path)
                .replace("{name}", folder_name),
            None => format!("file://{}", repo.display()),
        };
        let release = match newest_release(&repo) {
            Ok(Some(release)) => release,
            Ok(None) => {
                skipped.push(format!("{path} (no release yet: bx release)"));
                continue;
            }
            Err(e) => {
                skipped.push(format!("{path} ({e})"));
                continue;
            }
        };
        let unchanged = previous.remove(&path).filter(|t| {
            t.get("commit").and_then(|c| c.as_str()) == Some(release.commit.as_str())
                && t.get("git").and_then(|g| g.as_str()) == Some(git.as_str())
        });
        if let Some(table) = unchanged {
            tables.push(table);
            kept += 1;
            continue;
        }
        let info = match biblix::Manifest::parse(&release.manifest) {
            Ok(manifest) => match manifest.package {
                Some(info) => info,
                None => {
                    skipped.push(format!("{path} (a vault, not a package)"));
                    continue;
                }
            },
            Err(e) => {
                skipped.push(format!("{path} {} ({e})", release.tag));
                continue;
            }
        };
        let mut table = biblix::catalog::Catalog::entry_table(&info, &git);
        table.insert("path", toml_edit::value(&path));
        table.insert("commit", toml_edit::value(&release.commit));
        tables.push(table);
        read += 1;
    }
    let count = tables.len();
    let text = format!(
        "# Written by bx catalog build: the newest release of each package in {}\n{}",
        folder.display(),
        biblix::catalog::Catalog::toml_document(Some(&name), tables)
    );
    std::fs::write(&output, text).map_err(|e| biblix::BiblixError::Io {
        path: output.display().to_string(),
        message: e.to_string(),
    })?;
    let plural = if count == 1 { "" } else { "s" };
    println!(
        "Wrote {}: {count} package{plural} ({read} read, {kept} unchanged)",
        output.display()
    );
    for skip in skipped {
        println!("    skipped {skip}");
    }
    Ok(())
}

fn catalog_list(cli: &Cli) -> biblix::Result<()> {
    let project = cli.project(&cli.library())?;
    for (name, location) in project.manifest().registries() {
        println!("{name}: {location} (registry)");
    }
    let defaults = biblix::catalog::DEFAULT_CATALOGS;
    for location in project.manifest().catalogs() {
        let note = match defaults.contains(&location.as_str()) {
            true => " (default)",
            false => "",
        };
        println!("{location}{note}");
    }
    Ok(())
}

fn catalog_add(cli: &Cli, location: &str, name: Option<&str>) -> biblix::Result<()> {
    let mut project = cli.project(&cli.library())?;
    if let Some(name) = name {
        biblix::catalog::check_registry_name(name)?;
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

fn catalog_remove(cli: &Cli, location: &str) -> biblix::Result<()> {
    let mut project = cli.project(&cli.library())?;
    if project.remove_registry(location)? {
        project.save_manifest()?;
        println!("Removed the registry {location}");
        return Ok(());
    }
    if !project.remove_catalog(location)? {
        return Err(biblix::BiblixError::Catalog(format!(
            "{location} isn't searched (bx catalog list)"
        )));
    }
    project.save_manifest()?;
    println!("Removed {location}");
    Ok(())
}

/// What a catalog entry is, wherever it's listed: its repository, or its registry and id
fn source_key(entry: &biblix::catalog::CatalogEntry) -> String {
    match &entry.source {
        biblix::catalog::CatalogSource::Git { url } => url.clone(),
        biblix::catalog::CatalogSource::Registry { catalog } => format!("{catalog}#{}", entry.id),
        other => other.import_name().unwrap_or_default(),
    }
}

fn search(
    cli: &Cli,
    query: &str,
    catalogs: &[String],
    registry: Option<&str>,
    limit: usize,
    json: bool,
) -> biblix::Result<()> {
    // The project's registries come first, then its catalogs
    let mut registries = vec![];
    let (catalogs, base) = match catalogs.is_empty() {
        true => {
            let project = cli.project(&cli.library())?;
            let named = project.manifest().registries();
            if let Some(name) = registry
                && !named.contains_key(name)
            {
                return Err(biblix::BiblixError::Catalog(format!(
                    "no registry `{name}` (bx catalog list)"
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
                None => project.manifest().catalogs(),
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
    let mut all = biblix::catalog::Catalog {
        name: String::new(),
        entries: registries.into_iter().flat_map(|c| c.entries).collect(),
    };
    if !catalogs.is_empty() {
        all = (load_catalogs(&catalogs, &base)?.into_iter()).fold(all, |all, c| all.merge(c));
    }
    let found = all.search(query);
    if json {
        let entries: Vec<serde_json::Value> = (found.iter().take(limit))
            .map(|e| {
                let install = match (&e.source, e.source.import_name()) {
                    (_, Some(name)) => vec![String::from("import"), name],
                    _ if through.contains_key(&source_key(e)) => {
                        vec![String::from("add"), through[&source_key(e)].clone()]
                    }
                    (biblix::catalog::CatalogSource::Git { url }, None) => {
                        vec![String::from("add"), String::from("--git"), url.clone()]
                    }
                    (biblix::catalog::CatalogSource::Registry { catalog }, None) => vec![
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
            (_, Some(name)) => format!("bx import {name}"),
            _ if through.contains_key(&source_key(entry)) => {
                format!("bx add {}", through[&source_key(entry)])
            }
            (biblix::catalog::CatalogSource::Git { url }, None) => format!("bx add --git {url}"),
            (biblix::catalog::CatalogSource::Registry { catalog }, None) => {
                format!("bx add --registry {catalog} {}", entry.id)
            }
            (_, None) => String::new(),
        };
        println!(
            "{} ({}): {}, {language}, {}\n    {source}",
            entry.abbreviation,
            entry.id,
            entry.name,
            entry.license.as_deref().unwrap_or("no license given"),
        );
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

fn import(cli: &Cli, source: &str) -> biblix::Result<()> {
    use biblix::catalog::CatalogSource;
    let prefixes = CatalogSource::PREFIXES
        .map(|p| format!("{p}:<id>"))
        .join(" or ");
    let known = (source.split_once(':'))
        .is_some_and(|(prefix, _)| CatalogSource::PREFIXES.contains(&prefix));
    if !known {
        return Err(biblix::BiblixError::Import(format!(
            "{source}: give {prefixes} (see bx search)"
        )));
    }
    let library = cli.library();
    let mut project = cli.project(&library)?;
    // The project's catalogs, in order: the first that lists it is the one used
    let catalogs = load_catalogs(&project.manifest().catalogs(), project.dir())?;
    let entry = (catalogs.iter())
        .find_map(|c| c.find_import(source))
        .ok_or_else(|| {
            biblix::BiblixError::Import(format!(
                "{source}: in none of this project's catalogs (bx catalog list)"
            ))
        })?;
    let relative = format!("imports/{}", entry.id);
    let dir = project.dir().join(&relative);
    if dir.exists() {
        return Err(biblix::BiblixError::Import(format!(
            "{} already exists",
            dir.display()
        )));
    }
    let dest = FsStorage::new(&dir);
    match &entry.source {
        CatalogSource::Ebible { id } => {
            let zip = get(&biblix::import::ebible_zip_url(id))?;
            biblix::import::ebible(entry, &zip, &dest)?;
        }
        CatalogSource::FetchBible { id, books } => {
            let mut files = vec![];
            for code in books {
                files.push((
                    code.clone(),
                    get(&biblix::import::fetch_bible_book_url(id, code))?,
                ));
            }
            biblix::import::fetch_bible(entry, &files, &dest)?;
        }
        CatalogSource::Git { .. } | CatalogSource::Registry { .. } => {
            unreachable!("only imports are found")
        }
    }
    let dependency = DetailedDependency {
        path: Some(relative),
        default_features: true,
        ..DetailedDependency::default()
    };
    project.add(&entry.id, &dependency)?;
    project.save_manifest()?;
    project.install(&library, &cli.fetcher(&library), Update::Nothing)?;
    println!(
        "imported {} ({}) into {}",
        entry.id,
        entry.name,
        dir.display()
    );
    Ok(())
}

fn catalog_entry(cli: &Cli, url: &str) -> biblix::Result<()> {
    use biblix::git::Fetcher;
    let library = cli.library();
    let fetcher = cli.fetcher(&library);
    let tags = fetcher.tags(url)?;
    let newest = (tags.into_iter())
        .filter_map(|(t, _)| Some((biblix::resolve::tag_version(&t)?, t)))
        .filter(|(v, _)| v.pre.is_empty())
        .max();
    let reference = match newest {
        Some((_, tag)) => biblix::git::GitRef::Tag(tag),
        None => biblix::git::GitRef::Head,
    };
    let files = MemoryStorage::new();
    fetcher.fetch_files(url, &reference, &[MANIFEST], &files)?;
    let manifest = biblix::Manifest::parse(&files.read_string(MANIFEST)?)?;
    let info = manifest
        .package
        .ok_or_else(|| biblix::BiblixError::Manifest(format!("{url}: no [package]")))?;
    print!("{}", biblix::catalog::Catalog::toml_entry(&info, url));
    Ok(())
}

fn info(cli: &Cli, id: &str) -> biblix::Result<()> {
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
            biblix::BiblixError::NotFound(format!("{id} isn't in the lock file (bx install)"))
        })?;
    let dir = library.package_dir(&locked, project.dir());
    let package = library.open(&locked, project.dir())?;
    let i = package.info();
    let (abbreviation, aliases) = project.names(id, i);
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
    let storage = FsStorage::new(&dir);
    let files = storage.walk("")?;
    let size: u64 = (files.iter())
        .filter_map(|f| std::fs::metadata(dir.join(f)).ok())
        .map(|m| m.len())
        .sum();
    println!(
        "in: {} ({} files, {:.1} MB)",
        dir.display(),
        files.len(),
        size as f64 / 1e6
    );
    let checksum = package.checksum()?;
    match &locked.checksum {
        Some(expected) if *expected == checksum => println!("checksum: {checksum} (as locked)"),
        Some(expected) => println!(
            "checksum: {checksum} (CHANGED: locked as {expected}; bx install --repair puts its files back)"
        ),
        None => println!("checksum: {checksum}"),
    }
    let others = biblix::package::other_files(&storage)?;
    if !others.is_empty() {
        println!("not the package's (kept): {}", others.join(", "));
    }
    let cached = library.book_cache().open(package)?;
    let books = &cached.manifest().books;
    let verses: usize = (books.iter())
        .flat_map(|(_, counts)| counts.iter().map(|&c| c as usize))
        .sum();
    println!(
        "{} books, {verses} verses ({})",
        books.len(),
        cached.manifest().source_format
    );
    Ok(())
}

fn clean(cli: &Cli, remove: bool, keep: &[PathBuf]) -> biblix::Result<()> {
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
        let others = biblix::package::other_files(&FsStorage::new(&dir))?;
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
            let checksum = biblix::package::checksum(&FsStorage::new(&dir))?;
            let short = checksum
                .trim_start_matches("sha256:")
                .get(..16)
                .unwrap_or_default();
            let version = key.split('+').next().unwrap_or(key);
            std::fs::remove_dir_all(&dir).map_err(|e| biblix::BiblixError::Io {
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
            "{} unused (other vaults may use them: --keep <their folder>); bx clean --remove deletes them",
            unused.len()
        );
    }
    Ok(())
}

fn media_add(
    file: &std::path::Path,
    url: &str,
    book: Option<&str>,
    package: &std::path::Path,
) -> biblix::Result<()> {
    let io = |path: &std::path::Path, e: std::io::Error| biblix::BiblixError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    };
    let bytes = std::fs::read(file).map_err(|e| io(file, e))?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut entry = biblix::media::MediaFile::of(&name, url, &bytes);
    entry.book = book.map(str::to_uppercase);
    let path = package.join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| io(&path, e))?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| biblix::BiblixError::Manifest(e.to_string()))?;
    let media = doc
        .entry("media")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| biblix::BiblixError::Manifest(String::from("[media] isn't a table")))?;
    let files = media
        .entry("files")
        .or_insert(toml_edit::Item::ArrayOfTables(
            toml_edit::ArrayOfTables::new(),
        ))
        .as_array_of_tables_mut()
        .ok_or_else(|| biblix::BiblixError::Manifest(String::from("media.files isn't a list")))?;
    // Listed again: replaced
    let mut kept = toml_edit::ArrayOfTables::new();
    for table in files.iter() {
        if table.get("path").and_then(|p| p.as_str()) != Some(&name) {
            kept.push(table.clone());
        }
    }
    let mut table = toml_edit::Table::new();
    table.insert("path", toml_edit::value(&entry.path));
    table.insert("url", toml_edit::value(&entry.url));
    table.insert("size", toml_edit::value(entry.size as i64));
    table.insert("sha256", toml_edit::value(&entry.sha256));
    if let Some(book) = &entry.book {
        table.insert("book", toml_edit::value(book));
    }
    kept.push(table);
    *files = kept;
    // Still a manifest biblix reads
    biblix::Manifest::parse(&doc.to_string())?;
    std::fs::write(&path, doc.to_string()).map_err(|e| io(&path, e))?;
    println!(
        "listed {} ({:.1} MB, sha256 {}) in {}",
        entry.path,
        entry.size as f64 / 1e6,
        &entry.sha256[..16],
        path.display()
    );
    Ok(())
}

fn media_check(package: &std::path::Path, offline: bool) -> biblix::Result<()> {
    let opened = biblix::Package::open(Arc::new(FsStorage::new(package)), Default::default())?;
    let files = (opened.manifest().media.as_ref())
        .map(|m| m.files.clone())
        .unwrap_or_default();
    if files.is_empty() {
        println!("no media listed");
        return Ok(());
    }
    let timings = biblix::media::check_package_timings(&opened)?;
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
    let mut failed = 0;
    for file in &files {
        match get(&file.url).and_then(|bytes| biblix::media::verify(file, &bytes)) {
            Ok(()) => println!("ok: {}", file.path),
            Err(error) => {
                failed += 1;
                println!("FAILED: {}: {error}", file.path);
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => Err(biblix::BiblixError::Fetch {
            url: package.display().to_string(),
            message: format!("{n} of {} media files don't match", files.len()),
        }),
    }
}
