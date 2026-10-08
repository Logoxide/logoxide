//! `bx`: biblix's command line

use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

use biblix::{
    Cache, Features, FsStorage, Library, Package, Project, Storage, bible_data,
    git::GitCli,
    manifest::{Dependency, DetailedDependency, MANIFEST},
    resolve::Update,
};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "bx",
    version,
    about = "Bible translations as versioned packages"
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
    /// What a package is: its manifest, the format read, its books, and its checksum
    Show(PackageArgs),
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
        Command::Show(args) => show(args),
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
            let cached = cache.open(&package)?;
            for id in package.books() {
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
            let staging = library
                .git_dir()
                .join(format!("probe-{}", std::process::id()));
            let reference = match (&args.tag, &args.branch, &args.rev) {
                (Some(t), _, _) => biblix::git::GitRef::Tag(t.clone()),
                (_, Some(b), _) => biblix::git::GitRef::Branch(b.clone()),
                (_, _, Some(r)) => biblix::git::GitRef::Rev(r.clone()),
                _ => biblix::git::GitRef::Head,
            };
            let fetched = biblix::git::Fetcher::fetch(&fetcher, url, &reference, &staging)
                .and_then(|_| {
                    let storage = FsStorage::new(&staging);
                    biblix::Manifest::parse(&storage.read_string(MANIFEST)?)
                });
            let _ = std::fs::remove_dir_all(&staging);
            let manifest = fetched?;
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
