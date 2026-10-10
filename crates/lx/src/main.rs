//! `lx`: logoxide's command line

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use logoxide::{bible_data, manifest::MANIFEST, resolve::Update};

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
mod net;
mod publish;

use cli::{CatalogCommand, Cli, Command, Format, MediaCommand, RegistryCommand};
use cmd::{
    catalog::{
        Filters, catalog_add, catalog_build, catalog_entry, catalog_list, catalog_remove,
        package_url, search,
    },
    import::{import, import_update},
    map::map,
    media::{media_add, media_check},
    package::{check, diff, new, release, show, show_remote, verify},
    project::{add, clean, info, install, is_enabled, list, remove, repair, set_enabled},
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
            .map(|p| outln!("created {}", p.dir().join(MANIFEST).display())),
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
                    errln!("lx: {error}");
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
