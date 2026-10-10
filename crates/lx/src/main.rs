//! `lx`: logoxide's command line

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use logoxide::{
    bible_data,
    manifest::MANIFEST,
    resolve::{Frozen, Update},
};

/// `println!` with what isn't safe for a terminal taken out ([`logoxide::text::printable`]): what
/// lx prints holds catalogs', manifests', and git's text, which could hold escape sequences
macro_rules! outln {
    () => { println!() };
    ($($arg:tt)*) => { println!("{}", logoxide::text::printable(&format!($($arg)*))) };
}

/// `print!` through [`logoxide::text::printable`], as [`outln`]
macro_rules! out {
    ($($arg:tt)*) => { print!("{}", logoxide::text::printable(&format!($($arg)*))) };
}

/// `eprintln!` through [`logoxide::text::printable`], as [`outln`]
macro_rules! errln {
    () => { eprintln!() };
    ($($arg:tt)*) => { eprintln!("{}", logoxide::text::printable(&format!($($arg)*))) };
}

mod cli;
mod cmd;
mod complete;
mod publish;

use cli::{CatalogCommand, Cli, Command, Format, MediaCommand, RegistryCommand};
use cmd::{
    catalog::{
        Filters, Shown, catalog_add, catalog_build, catalog_entry, catalog_list, catalog_remove,
        package_url, search,
    },
    import::{import, import_update},
    map::map,
    media::{media_add, media_check},
    package::{Lookup, check, diff, lookup, new, release, show, show_remote, verify},
    project::{add, clean, info, install, is_enabled, list, remove, repair, set_enabled},
    read::{compare, read},
};

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

/// What lx works with (logoxide-app's facade: the config, library, downloads, fetcher, and
/// reader), made once the command line is read
static APP: std::sync::OnceLock<logoxide_app::Logoxide> = std::sync::OnceLock::new();

/// Whether downloads go unsaid (JSON output)
static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn app() -> &'static logoxide_app::Logoxide {
    APP.get_or_init(|| {
        logoxide_app::Logoxide::default_native(
            config().clone(),
            config().library(None, None),
            &user_agent(),
            notes(),
        )
    })
}

/// What lx's downloads say they are
fn user_agent() -> String {
    format!(
        "lx/{} (+https://github.com/Logoxide/logoxide)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Each download said on stderr (unless lx prints JSON), and each warning
fn notes() -> logoxide_app::Notes {
    std::sync::Arc::new(|note: &logoxide_app::Note| {
        if note.is_warning() {
            errln!("warning: {note}");
        } else if !QUIET.load(std::sync::atomic::Ordering::Relaxed) {
            errln!("{note}");
        }
    })
}

/// Prints each warning as it's found
fn warn() -> impl FnMut(String) {
    |warning: String| errln!("warning: {warning}")
}

fn main() -> ExitCode {
    // Rust ignores SIGPIPE, so printing after the reader has gone (`lx list | head`) panics; the
    // default ends lx quietly, as it does other commands
    #[cfg(unix)]
    // SAFETY: nothing else has started yet, and SIG_DFL is a valid disposition for SIGPIPE
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    // `COMPLETE=bash lx` writes the shell's completion script; the shell then runs lx to complete
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    match logoxide::config::Config::load() {
        Ok(loaded) => {
            let _ = CONFIG.set(loaded);
        }
        Err(e) => {
            errln!("lx: {e}");
            return ExitCode::FAILURE;
        }
    }
    if let Err(error) = check_format(&cli) {
        errln!("lx: {error}");
        return ExitCode::FAILURE;
    }
    let json = cli.format == Format::Json;
    QUIET.store(
        json || matches!(cli.command, Command::Search { json: true, .. }),
        std::sync::atomic::Ordering::Relaxed,
    );
    let _ = APP.set(logoxide_app::Logoxide::default_native(
        config().clone(),
        cli.library(),
        &user_agent(),
        notes(),
    ));
    let format = cli.format.name();
    let result = match &cli.command {
        Command::Show {
            package,
            git: None,
            notes: Some(reference),
            stretch,
            ..
        } => lookup(package, Lookup::Notes(reference, *stretch), json),
        Command::Show {
            package,
            git: None,
            entry: Some(key),
            ..
        } => lookup(package, Lookup::Entry(key), json),
        Command::Show {
            package,
            git: None,
            keys: true,
            limit,
            ..
        } => lookup(package, Lookup::Keys(*limit), json),
        Command::Show {
            package,
            git: None,
            xref: Some(reference),
            ..
        } => lookup(package, Lookup::CrossReferences(reference), json),
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
        Command::Check { package, cache } => check(package, cache.clone(), json, &cli.library()),
        Command::New(args) => new(args),
        Command::Init { dir } => logoxide::Project::init(dir)
            .map(|p| outln!("created {}", p.dir().join(MANIFEST).display())),
        Command::Add(args) => add(&cli, args),
        Command::Remove { id } => remove(&cli, id),
        Command::Disable { name } => set_enabled(&cli, name, false),
        Command::Enable { name } => set_enabled(&cli, name, true),
        Command::Install { repair: true, .. } => repair(&cli),
        Command::Install {
            locked, offline, ..
        } => install(
            &cli,
            Update::Nothing,
            Frozen {
                locked: *locked,
                offline: *offline,
            },
        ),
        Command::Update { ids } => install(
            &cli,
            match ids.is_empty() {
                true => Update::All,
                false => Update::Only(ids.iter().cloned().collect()),
            },
            Frozen::default(),
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
                    errln!("lx: {error}");
                    ExitCode::from(2)
                }
            };
        }
        Command::Info { id } => info(&cli, id),
        Command::Clean {
            remove,
            keep,
            prune_roots,
        } => clean(&cli, *remove, keep, *prune_roots),
        Command::Search {
            query,
            catalog,
            registry,
            books,
            category,
            limit,
            flat,
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
            Shown {
                limit: *limit,
                json: *json || cli.format == Format::Json,
                flat: *flat,
            },
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
        Command::Publish { path, to, all, yes } => {
            let library = cli.library();
            let (host, git) = (publish::Host::parse(to), cli.git(&library));
            match all {
                true => publish::publish_all(path, &host, &git, *yes),
                false => publish::publish(path, &host, &git, *yes),
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
        Command::Media(MediaCommand::Check { package, offline }) => {
            media_check(package, *offline, &cli.library())
        }
        Command::Map {
            reference,
            from,
            to,
            with,
        } => map(&cli, reference, from, to, with),
        Command::Read { reference, text } => read(&cli, reference, text.style(), json),
        Command::Compare {
            reference,
            bibles,
            text,
        } => compare(&cli, reference, bibles, text.style(), json),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            errln!("lx: {error}");
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
    // A commentary's notes, a dictionary's entry or keys, a verse's cross-references
    let lookup = matches!(
        cli.command,
        Command::Show { notes: Some(_), .. }
            | Command::Show { entry: Some(_), .. }
            | Command::Show { keys: true, .. }
            | Command::Show { xref: Some(_), .. }
    );
    let json = references
        || lookup
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
                | Command::Read { .. }
                | Command::Compare { .. }
        );
    match cli.format {
        Format::Text => Ok(()),
        Format::Json if json => Ok(()),
        Format::Tsv if references => Ok(()),
        Format::Json => Err(logoxide::LogoxideError::Manifest(String::from(
            "--format json: only for list, info, check, diff, verify, search, catalog list, import --update, read, compare, and show --references, --notes, --entry, --keys, or --xref",
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
