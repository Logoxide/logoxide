//! `bx`: biblix's command line

use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

use biblix::{Cache, Features, FsStorage, Package, Storage, bible_data, manifest::Dependency};
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
    let result = match cli.command {
        Command::Show(args) => show(&args),
        Command::Check { package, cache } => check(&package, cache),
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
