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
use clap::{Args, Parser, Subcommand};

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

    fn fetcher(&self, library: &Library) -> GitCli {
        GitCli {
            scratch: Some(library.git_dir()),
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
        /// A package's repository: show a release's published summary
        #[arg(long)]
        git: Option<String>,
        /// The release (the newest by default)
        #[arg(long, requires = "git")]
        tag: Option<String>,
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
        id: String,
    },
    /// Installs what biblix.lock says (resolving what it doesn't have yet)
    Install,
    /// Moves packages to the newest versions their requirements allow
    Update {
        /// Only these packages (all by default)
        ids: Vec<String>,
    },
    /// The packages biblix.toml asks for, as locked
    List,
    /// A locked package: what it is, where, its size, its books and verses, and whether its files
    /// are still the ones locked
    Info {
        /// The package's id
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
        /// How many to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// One JSON array, for editors: each package's id, names, language, license, and the bx
        /// arguments that install it
        #[arg(long)]
        json: bool,
    },
    /// Makes a package from another publisher's download, puts it in the project's `imports/`,
    /// and adds it as a path package: `ebible:<id>` or `fetch:<id>` (as `bx search` shows)
    Import {
        /// `ebible:eng-asv`, `fetch:eng_bsb`
        source: String,
    },
    /// Catalogs: lists of packages anyone can host
    #[command(subcommand)]
    Catalog(CatalogCommand),
    /// A package's audio and video, listed by URL and hash
    #[command(subcommand)]
    Media(MediaCommand),
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
    /// Downloads every listed file and checks its size and hash
    Check {
        /// The package's folder
        #[arg(default_value = ".")]
        package: PathBuf,
    },
}

#[derive(Subcommand)]
enum CatalogCommand {
    /// The catalogs `bx search` looks through: the project's, then eBible.org's and fetch.bible's
    List,
    /// Searches a catalog too (a URL, or a path relative to the project): a catalog.toml,
    /// eBible.org's translations.csv, or fetch.bible's manifest.json. It's read first, to check it
    Add { location: String },
    /// Stops searching a catalog. Removing eBible.org's or fetch.bible's turns the defaults off
    /// (`default-catalogs = false`), keeping the other one
    Remove { location: String },
    /// A catalog.toml entry for a package's repository (its newest release's biblix.toml)
    Entry {
        /// The package's git URL
        git: String,
    },
}

#[derive(Args)]
struct AddArgs {
    /// A git repository's URL (or a local repository's path)
    #[arg(long, conflicts_with = "path")]
    git: Option<String>,
    /// A package's folder, used in place
    #[arg(long)]
    path: Option<String>,
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
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Show {
            package, git: None, ..
        } => show(package),
        Command::Show {
            git: Some(url),
            tag,
            ..
        } => show_remote(&cli, url, tag.as_deref()),
        Command::Release {
            path,
            set_version,
            dry_run,
        } => release(&cli, path, *set_version, *dry_run),
        Command::Diff { path, from, to } => diff(&cli, path, from, to.as_deref()),
        Command::Verify(args) => verify(args),
        Command::Check { package, cache } => check(package, cache.clone()),
        Command::Init { dir } => biblix::Project::init(dir)
            .map(|p| println!("created {}", p.dir().join(MANIFEST).display())),
        Command::Add(args) => add(&cli, args),
        Command::Remove { id } => remove(&cli, id),
        Command::Install => install(&cli, Update::Nothing),
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
            limit,
            json,
        } => search(&cli, &query.join(" "), catalog, *limit, *json),
        Command::Import { source } => import(&cli, source),
        Command::Catalog(CatalogCommand::Entry { git }) => catalog_entry(&cli, git),
        Command::Catalog(CatalogCommand::List) => catalog_list(&cli),
        Command::Catalog(CatalogCommand::Add { location }) => catalog_add(&cli, location),
        Command::Catalog(CatalogCommand::Remove { location }) => catalog_remove(&cli, location),
        Command::Media(MediaCommand::Add {
            file,
            url,
            book,
            package,
        }) => media_add(file, url, book.as_deref(), package),
        Command::Media(MediaCommand::Check { package }) => media_check(package),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bx: {error}");
            ExitCode::FAILURE
        }
    }
}

fn show(args: &PackageArgs) -> biblix::Result<()> {
    let package = args.open()?;
    let info = package.info();
    let manifest = package.manifest();
    println!(
        "{} {} ({}), {}",
        info.id, info.version, info.abbreviation, info.name
    );
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
        "formats: {} (enabled: {}; reading {:?})",
        all.join(", "),
        enabled.join(", "),
        package.source().format()
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
                d.git.as_deref().or(d.path.as_deref()).unwrap_or_default(),
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
    println!("books: {} ({})", books.len(), codes.join(" "));
    println!("checksum: {}", package.checksum()?);
    Ok(())
}

fn check(args: &PackageArgs, cache: Option<PathBuf>) -> biblix::Result<()> {
    let package = args.open()?;
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
    println!(
        "{books} books, {verses} verses, read in {:.0?}",
        start.elapsed()
    );
    Ok(())
}

fn version_req(text: &str) -> biblix::Result<semver::VersionReq> {
    text.parse()
        .map_err(|e| biblix::BiblixError::Manifest(format!("version `{text}`: {e}")))
}

fn add(cli: &Cli, args: &AddArgs) -> biblix::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let mut dependency = DetailedDependency {
        git: args.git.clone(),
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
    let id = match (&args.git, &args.path) {
        (Some(url), _) => {
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
            info.id
        }
        (None, Some(path)) => {
            let storage = FsStorage::new(project.dir().join(path));
            let manifest = biblix::Manifest::parse(&storage.read_string(MANIFEST)?)?;
            manifest
                .package
                .map(|p| p.id)
                .ok_or_else(|| biblix::BiblixError::Manifest(format!("{path}: no [package]")))?
        }
        (None, None) => {
            return Err(biblix::BiblixError::Manifest(String::from(
                "give --git or --path",
            )));
        }
    };
    project.add(&id, &dependency)?;
    project.save_manifest()?;
    let fetcher = cli.fetcher(&library);
    let lock = project.install(&library, &fetcher, Update::Only([id.clone()].into()))?;
    let locked = lock.get(&id).map(|p| p.name()).unwrap_or(id);
    println!(
        "added {locked} to {}",
        project.dir().join(MANIFEST).display()
    );
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
        println!("{}{note}", package.name());
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
        let root = if lock.dependencies.contains(&locked.name()) {
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
            _ => String::new(),
        };
        let installed = if library.is_installed(locked, project.dir()) {
            ""
        } else {
            "  NOT INSTALLED"
        };
        let about = library.open(locked, project.dir()).ok().map(|p| {
            let info = p.info();
            format!(
                "{} ({}), {}",
                info.abbreviation,
                info.name,
                info.license.as_deref().unwrap_or("no license given")
            )
        });
        println!(
            "{}  {}  {}{root}{installed}",
            locked.name(),
            about.unwrap_or_default(),
            source.trim()
        );
    }
    Ok(())
}

fn print_summary(summary: &biblix::summary::Summary) {
    println!(
        "{} {}: {} books, {} verses",
        summary.id,
        summary.version,
        summary.books.len(),
        summary.verses
    );
    let codes: Vec<&str> = summary.books.keys().map(String::as_str).collect();
    println!("books: {}", codes.join(" "));
    for (format, size) in &summary.sizes {
        println!("{format}: {:.1} MB", *size as f64 / 1_000_000.0);
    }
    let contents: Vec<String> = summary
        .contents
        .iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    println!("contents: {}", contents.join(", "));
    println!("text checksum: {}", summary.text_checksum);
}

fn show_remote(cli: &Cli, url: &str, tag: Option<&str>) -> biblix::Result<()> {
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
    let paths = [MANIFEST, biblix::summary::SUMMARY, biblix::summary::VERSES];
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
        Ok(summary) => print_summary(&summary),
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
        println!("would release v{}", plan.summary.version);
        return Ok(());
    }
    let commit = biblix::release::release(path, &plan, &git)?;
    println!(
        "released v{} ({})",
        plan.summary.version,
        &commit[..commit.len().min(12)]
    );
    Ok(())
}

fn diff(cli: &Cli, path: &std::path::Path, from: &str, to: Option<&str>) -> biblix::Result<()> {
    let library = cli.library();
    let git = cli.fetcher(&library);
    let url = path
        .canonicalize()
        .map_err(|e| biblix::BiblixError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?
        .display()
        .to_string();
    let (_, old) = biblix::release::summary_at(&git, &url, from)?;
    let new = match to {
        Some(tag) => biblix::release::summary_at(&git, &url, tag)?.1,
        None => {
            let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(path));
            biblix::summary::summarize(&Package::open(storage, Features::default())?)?.1
        }
    };
    let d = biblix::summary::diff(&old, &new);
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

fn catalog_list(cli: &Cli) -> biblix::Result<()> {
    let project = cli.project(&cli.library())?;
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

fn catalog_add(cli: &Cli, location: &str) -> biblix::Result<()> {
    let mut project = cli.project(&cli.library())?;
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
    if !project.remove_catalog(location)? {
        return Err(biblix::BiblixError::Catalog(format!(
            "{location} isn't searched (bx catalog list)"
        )));
    }
    project.save_manifest()?;
    println!("Removed {location}");
    Ok(())
}

fn search(
    cli: &Cli,
    query: &str,
    catalogs: &[String],
    limit: usize,
    json: bool,
) -> biblix::Result<()> {
    let (catalogs, base) = match catalogs.is_empty() {
        true => {
            let project = cli.project(&cli.library())?;
            (project.manifest().catalogs(), project.dir().to_path_buf())
        }
        false => (catalogs.to_vec(), PathBuf::from(".")),
    };
    let mut all = biblix::catalog::Catalog::default();
    let mut failed = 0;
    for location in &catalogs {
        // One catalog that can't be read doesn't stop the others
        match load_catalog(location, &base) {
            Ok(catalog) => all = all.merge(catalog),
            Err(e) if catalogs.len() > 1 => {
                eprintln!("warning: {e}");
                failed += 1;
            }
            Err(e) => return Err(e),
        }
    }
    if failed == catalogs.len() {
        return Err(biblix::BiblixError::Catalog(String::from(
            "no catalog could be read",
        )));
    }
    let found = all.search(query);
    if json {
        use biblix::catalog::CatalogSource;
        let entries: Vec<serde_json::Value> = (found.iter().take(limit))
            .map(|e| {
                let install = match &e.source {
                    CatalogSource::Git { url } => {
                        vec![String::from("add"), String::from("--git"), url.clone()]
                    }
                    CatalogSource::Ebible { id } => {
                        vec![String::from("import"), format!("ebible:{id}")]
                    }
                    CatalogSource::FetchBible { id, .. } => {
                        vec![String::from("import"), format!("fetch:{id}")]
                    }
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
        let source = match &entry.source {
            biblix::catalog::CatalogSource::Git { url } => format!("bx add --git {url}"),
            biblix::catalog::CatalogSource::Ebible { id } => format!("bx import ebible:{id}"),
            biblix::catalog::CatalogSource::FetchBible { id, .. } => {
                format!("bx import fetch:{id}")
            }
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
        catalogs.len(),
        if more > 0 {
            format!(" ({more} more: --limit)")
        } else {
            String::new()
        }
    );
    Ok(())
}

fn import(cli: &Cli, source: &str) -> biblix::Result<()> {
    use biblix::catalog::{CatalogSource, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG};
    let (catalog, id) = match source.split_once(':') {
        Some(("ebible", id)) => (EBIBLE_CATALOG, id),
        Some(("fetch", id)) => (FETCH_BIBLE_CATALOG, id),
        _ => {
            return Err(biblix::BiblixError::Import(format!(
                "{source}: give ebible:<id> or fetch:<id> (see bx search)"
            )));
        }
    };
    let catalog = load_catalog(catalog, Path::new("."))?;
    let entry = (catalog.entries.iter())
        .find(|e| match &e.source {
            CatalogSource::Ebible { id: i } | CatalogSource::FetchBible { id: i, .. } => i == id,
            CatalogSource::Git { .. } => false,
        })
        .ok_or_else(|| biblix::BiblixError::Import(format!("{source}: not in its catalog")))?;
    let library = cli.library();
    let mut project = cli.project(&library)?;
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
        CatalogSource::Git { .. } => unreachable!("only imports are found"),
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
    let locked = (project.lock())
        .and_then(|lock| lock.get(id))
        .cloned()
        .ok_or_else(|| {
            biblix::BiblixError::NotFound(format!("{id} isn't in the lock file (bx install)"))
        })?;
    let dir = library.package_dir(&locked, project.dir());
    let package = library.open(&locked, project.dir())?;
    let i = package.info();
    println!("{} {} ({}), {}", i.id, i.version, i.abbreviation, i.name);
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
        Some(expected) => println!("checksum: {checksum} (CHANGED: locked as {expected})"),
        None => println!("checksum: {checksum}"),
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
        if remove {
            std::fs::remove_dir_all(&dir).map_err(|e| biblix::BiblixError::Io {
                path: dir.display().to_string(),
                message: e.to_string(),
            })?;
            // Its parsed books: `<id>/<version>-<checksum>`
            let cache = library.cache_dir().join("books").join(id);
            for entry in std::fs::read_dir(&cache).into_iter().flatten().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(&format!("{key}-")) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
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

fn media_check(package: &std::path::Path) -> biblix::Result<()> {
    let storage = FsStorage::new(package);
    let manifest = biblix::Manifest::parse(&storage.read_string(MANIFEST)?)?;
    let files = manifest.media.map(|m| m.files).unwrap_or_default();
    if files.is_empty() {
        println!("no media listed");
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
