//! The command line: `lx`'s commands and their arguments (clap)

use std::{path::PathBuf, sync::Arc};

use clap::{Args, Parser, Subcommand};
use clap_complete::engine::ArgValueCompleter;
use logoxide::{Features, FsStorage, Library, Package, Project, Storage, manifest::MANIFEST};

use crate::{app, complete, config};

#[derive(Parser)]
#[command(
    name = "lx",
    version,
    about = "Bibles, commentaries, lexicons, books, and media as versioned packages",
    after_help = "Exit status: 0 when it worked, 1 on an error; `lx is-enabled` exits 0 when the \
                  package is on, 1 when it's off, and 2 on an error"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
    /// The folder with the logoxide.toml to use: the current one if it has one, else the library's
    #[arg(long, global = true)]
    pub(crate) project: Option<PathBuf>,
    /// Use the library's own logoxide.toml, even in a project (`--project <library>`)
    #[arg(long, global = true, conflicts_with = "project")]
    pub(crate) global: bool,
    /// The library (LOGOXIDE_HOME, else ~/.config/logoxide/config.toml's `library`, else
    /// $XDG_DATA_HOME/logoxide)
    #[arg(long, global = true)]
    pub(crate) library: Option<PathBuf>,
    /// The cache (LOGOXIDE_CACHE, else the config's `cache`, else $XDG_CACHE_HOME/logoxide)
    #[arg(long, global = true)]
    pub(crate) cache_dir: Option<PathBuf>,
    /// How to print: text, or json (list, info, check, diff, verify, search, catalog list,
    /// import --update, read, compare, show --references, --notes, --entry, --keys, and --xref);
    /// tsv for show --references
    #[arg(long, global = true, value_enum, default_value_t = Format::Text)]
    pub(crate) format: Format,
}

/// What `--format` prints
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Format {
    Text,
    Json,
    Tsv,
}

impl Format {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Json => "json",
            Format::Tsv => "tsv",
        }
    }
}

impl Cli {
    /// `--library` and `--cache-dir`, else the environment's, else the config's, else the XDG
    /// folders
    pub(crate) fn library(&self) -> Library {
        config().library(self.library.clone(), self.cache_dir.clone())
    }

    pub(crate) fn project(&self, library: &Library) -> logoxide::Result<Project> {
        let dir = match &self.project {
            Some(dir) => dir.clone(),
            None if self.global => library.root().to_path_buf(),
            None if std::path::Path::new(MANIFEST).is_file() => PathBuf::from("."),
            None => library.root().to_path_buf(),
        };
        let project = Project::open(dir)?;
        // Keys this lx doesn't know (a newer one's) are passed over, like Cargo's
        let keys = logoxide::manifest::Manifest::unknown_keys(&project.manifest_text())?;
        for key in keys {
            errln!("warning: {MANIFEST}: unknown key `{key}`");
        }
        for warning in project.manifest().warnings() {
            errln!("warning: {MANIFEST}: {warning}");
        }
        Ok(project)
    }

    /// What installs fetch packages with: `git` where it runs, else HTTP alone
    pub(crate) fn fetcher(&self) -> &'static dyn logoxide::git::Fetcher {
        app().fetcher()
    }

    /// The `git` command, for what needs it (releasing, publishing, a package's own tags)
    pub(crate) fn git(&self, library: &Library) -> logoxide_app::fetch::GitFetcher {
        logoxide_app::fetch::GitFetcher::new(
            config().git(Some(library.git_dir())),
            app().http_arc(),
            crate::notes(),
        )
    }
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// What a package is: its manifest, the format read, its books, and its checksum; with --git, a
    /// release's summary, without downloading its text. For what isn't a Bible, what it says:
    /// a commentary's notes on a passage (--notes), a dictionary's entry (--entry) or keys
    /// (--keys), a verse's cross-references (--xref), each as text or `--format json`
    Show {
        #[command(flatten)]
        package: PackageArgs,
        /// A package's repository (or `github:owner/repo`): show a release's published summary
        #[arg(long, add = ArgValueCompleter::new(complete::package))]
        git: Option<String>,
        /// The release (the newest by default)
        #[arg(long, requires = "git")]
        tag: Option<String>,
        /// List the passages its books (EPUBs, a book's ThML) cite, and how often
        #[arg(long)]
        references: bool,
        /// With --references: every reference, with where it is (chapter, paragraph, CFI,
        /// offsets), read from the books (so not with --git). `--format` lists them as text,
        /// tsv, or json
        #[arg(long, requires = "references", conflicts_with = "git")]
        locations: bool,
        /// A commentary's notes on a passage (`--notes "John 3:16-18"`), each labeled with the
        /// verses it covers; a Bible's are its verses
        #[arg(long, value_name = "REFERENCE", conflicts_with_all = ["git", "references", "entry", "keys", "xref"])]
        notes: Option<String>,
        /// With --notes: a note also covers the verses after it up to the commentary's next
        /// note in the chapter, so a verse it skips shows the note before it
        #[arg(long, requires = "notes")]
        stretch: bool,
        /// A dictionary's entry by key (`--entry G25`; `G0025` and `g25` find it too, a
        /// headword ignoring case and accents): its key, headword, and text
        #[arg(long, value_name = "KEY", conflicts_with_all = ["git", "references", "keys", "xref"])]
        entry: Option<String>,
        /// A dictionary's keys in its files' order (a cross-reference package's passages), one
        /// a line
        #[arg(long, conflicts_with_all = ["git", "references", "xref"])]
        keys: bool,
        /// With --keys: how many to list (all by default)
        #[arg(long, requires = "keys")]
        limit: Option<usize>,
        /// A cross-reference package's references for a verse or passage (`--xref "John
        /// 3:16"`): each line whose passage covers it, with its weight
        #[arg(long, value_name = "REFERENCE", conflicts_with_all = ["git", "references"])]
        xref: Option<String>,
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
        /// Fail if logoxide.lock would change (a dependency it lacks, or one the manifest now asks
        /// for differently), as in CI
        #[arg(long, conflicts_with = "repair")]
        locked: bool,
        /// Fetch nothing: fail if anything isn't installed already
        #[arg(long, conflicts_with = "repair")]
        offline: bool,
    },
    /// Moves packages to the newest versions their requirements allow
    Update {
        /// Only these packages (all by default)
        #[arg(add = ArgValueCompleter::new(complete::dependency))]
        ids: Vec<String>,
    },
    /// The packages logoxide.toml asks for, as locked (those turned off are marked `(off)`);
    /// --all: every version in the library, and the projects using it
    List {
        /// Only the packages in use (logoxide.toml's that are on, and what they bring in)
        #[arg(long, conflicts_with = "disabled")]
        enabled: bool,
        /// Only the packages turned off (and what only they bring in)
        #[arg(long)]
        disabled: bool,
        /// Every version in the library, whichever project installed it, with the projects that
        /// use it (those using none are what `lx clean --remove` deletes)
        #[arg(long, conflicts_with_all = ["enabled", "disabled"])]
        all: bool,
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
        /// Forget projects recorded as using this library whose folder is gone from this machine
        /// (their versions are kept until then)
        #[arg(long)]
        prune_roots: bool,
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
        /// Only packages in this language (repeat for more): a BCP 47 tag, which takes its
        /// regions and scripts too (`en` is `en-US` as well, `eng` is `en`), or the language's
        /// name as the catalog gives it (`--language Español`)
        #[arg(long)]
        language: Vec<String>,
        /// Gather what's found under each language (English, then Spanish, …), in the order of
        /// each language's best match
        #[arg(long)]
        by_language: bool,
        /// How many to show
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// One list, best first, without gathering each work's editions (every KJV) under its
        /// name
        #[arg(long)]
        flat: bool,
        /// Download the catalogs at URLs again, however recently they were kept
        /// (`catalog-max-age` in logoxide's config.toml, 7 days by default)
        #[arg(long, conflicts_with = "offline")]
        refresh: bool,
        /// Download nothing: catalogs at URLs only as they were last kept, of any age
        #[arg(long)]
        offline: bool,
        /// One JSON array, for editors: each package's id, names, language, license, keywords,
        /// categories, books it's about, and the lx arguments that install it (as `--format
        /// json`)
        #[arg(long)]
        json: bool,
    },
    /// Lists the works a package can say it's an edition of (`work = "kjv"` in its
    /// logoxide.toml): logoxide's own table, then the works the project's registries and
    /// catalogs name (`[works]` in a catalog.toml, a folder registry's works.toml), then ids
    /// packages claim that nothing names. A work is a name, not an owner: any package may claim
    /// any work
    Works {
        /// Words to find in a work's id, name, or aliases (every one must match)
        query: Vec<String>,
        /// A catalog to read (a URL or a path), instead of the project's
        #[arg(long)]
        catalog: Vec<String>,
        /// Only this registry's works and packages (lx catalog list), with logoxide's table
        #[arg(long, conflicts_with = "catalog", add = ArgValueCompleter::new(complete::registry))]
        registry: Option<String>,
        /// List each work's packages, not only how many there are
        #[arg(long)]
        packages: bool,
        /// Download the catalogs at URLs again, however recently they were kept
        #[arg(long, conflicts_with = "offline")]
        refresh: bool,
        /// Download nothing: catalogs at URLs only as they were last kept, of any age
        #[arg(long)]
        offline: bool,
    },
    /// Makes a package from another publisher's download and adds it as a dependency
    /// (`{ import = "ebible:eng-asv" }`), kept in the library like any installed package and
    /// shared by every project that locks the same files: `ebible:<id>`, `fetch:<id>`,
    /// `ccel:<id>`, or `sword:<repository>/<module>` (as `lx search` shows), from the project's
    /// catalogs (and for SWORD, the repositories CrossWire lists). `--vendor` puts it in the
    /// project's `imports/` as a path package instead
    ///
    /// `--update` makes the project's imports again from their publishers' downloads and replaces
    /// the ones that changed (a new version in the library; a vendored one in place, a reader's
    /// files beside it kept), saying what changed
    Import {
        /// `ebible:eng-asv`, `fetch:eng_bsb`, `ccel:kjv`, `sword:crosswire/KJV`; with `--update`,
        /// one import (its dependency's name, or this), else all of them
        #[arg(required_unless_present = "update")]
        source: Option<String>,
        /// Update imports: download each again, and replace it if it changed
        #[arg(long)]
        update: bool,
        /// Put it in the project's `imports/<id>/` as a path package (to edit it by hand, or to
        /// carry it with the project), not in the library
        #[arg(long, conflicts_with = "update")]
        vendor: bool,
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
        /// Publishes a package whose rights say it isn't redistributable without asking
        /// (licenses are shown, never enforced: the warning is still printed)
        #[arg(long, short)]
        yes: bool,
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
    /// A book's text in USFM, USX, or USJ (USX as JSON) written in another of them: a file (to
    /// stdout, or --output), or each book file in a folder into another folder (--output),
    /// named as it was (USJ as `.json`). What's written is normalized, not the publisher's layout
    Convert {
        /// A book file (`JHN.usfm`, `JHN.usx`, `JHN.json`; `-` reads stdin), or a folder of them
        #[arg(value_hint = clap::ValueHint::AnyPath)]
        input: PathBuf,
        /// The format to write
        #[arg(long, value_parser = crate::cmd::convert::FORMATS)]
        to: String,
        /// The format read (by default each file's: from its extension, else how it starts); in
        /// a folder, only its files in this format are converted
        #[arg(long, value_parser = crate::cmd::convert::FORMATS)]
        from: Option<String>,
        /// Where to write: a file, or a folder to write it in (a folder's books need one)
        #[arg(long, short, value_hint = clap::ValueHint::AnyPath)]
        output: Option<PathBuf>,
    },
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
    /// A passage from one of the project's Bibles: the one named after it (`lx read "John 3:16
    /// KJV"`, by abbreviation or alias), else the first; as text or `--format json`
    Read {
        /// `John 3:16`, `Ps 23 BSB`, `Romans 8:28-39 (kjv)`
        reference: String,
        #[command(flatten)]
        text: TextArgs,
    },
    /// A passage in several of the project's Bibles (all of them without --bibles), each in its
    /// own numbering through the project's versification maps, saying what of it one lacks
    Compare {
        /// `John 3:16`, numbered as the Bible named after it, else the first of --bibles
        reference: String,
        /// The Bibles, by name, abbreviation, or alias (`--bibles KJV,BSB`)
        #[arg(long, value_delimiter = ',', add = ArgValueCompleter::new(complete::dependency))]
        bibles: Vec<String>,
        #[command(flatten)]
        text: TextArgs,
    },
}

/// How `lx read` and `lx compare` write a passage
#[derive(Args, Clone, Copy)]
pub(crate) struct TextArgs {
    /// Break lines longer than this many characters
    #[arg(long)]
    pub(crate) width: Option<usize>,
    /// Without verse numbers and chapter labels
    #[arg(long)]
    pub(crate) no_labels: bool,
}

impl TextArgs {
    /// logoxide-text's reading style, with these changes
    pub(crate) fn style(self) -> logoxide_app::Style {
        let mut style = logoxide_app::Style::reading();
        style.width = self.width;
        if self.no_labels {
            style.verse_label = None;
            style.chapter_label = None;
        }
        style
    }
}

#[derive(Subcommand)]
pub(crate) enum MediaCommand {
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
pub(crate) enum RegistryCommand {
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
pub(crate) enum CatalogCommand {
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
    /// Downloads the catalogs and registries at URLs again (the project's and config.toml's,
    /// and the default catalogs), keeping them for `lx search` until they're `catalog-max-age`
    /// old (7 days by default). Catalogs and registries on this machine are read each time
    Refresh {
        /// Only these: catalog URLs or registry names (lx catalog list)
        #[arg(add = ArgValueCompleter::new(complete::searched))]
        catalogs: Vec<String>,
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
pub(crate) struct AddArgs {
    /// A git repository: `github:owner/repo` (also `gitlab:`, `codeberg:`), its URL, or a
    /// package in a registry (`dorean-local:dorean-collection`, by its id or its path there)
    #[arg(conflicts_with_all = ["git", "path"], add = ArgValueCompleter::new(complete::package))]
    pub(crate) repository: Option<String>,
    /// A git repository's URL (or a local repository's path), or `github:owner/repo`
    #[arg(long, conflicts_with = "path", add = ArgValueCompleter::new(complete::package))]
    pub(crate) git: Option<String>,
    /// A package's folder, used in place
    #[arg(long, value_hint = clap::ValueHint::DirPath)]
    pub(crate) path: Option<String>,
    /// A registry's catalog (a URL, or a file) to add the package from by its id or path there,
    /// without naming the registry first: `lx add --registry https://…/catalog.toml romans`
    #[arg(long, conflicts_with_all = ["git", "path"])]
    pub(crate) registry: Option<String>,
    /// A version requirement (`1`, `^1.2`); the newest matching tag is used
    #[arg(long, conflicts_with_all = ["tag", "branch", "rev"])]
    pub(crate) version: Option<String>,
    #[arg(long, conflicts_with_all = ["branch", "rev"])]
    pub(crate) tag: Option<String>,
    #[arg(long, conflicts_with = "rev")]
    pub(crate) branch: Option<String>,
    /// A commit, by its full id (40 hex digits, or 64)
    #[arg(long)]
    pub(crate) rev: Option<String>,
    /// The package's folder in the repository, when it isn't at the top (its releases are
    /// tagged `<folder>/v<version>`)
    #[arg(long, conflicts_with_all = ["path", "registry"])]
    pub(crate) folder: Option<String>,
    /// Features to turn on, separated by commas
    #[arg(short = 'F', long, value_delimiter = ',')]
    pub(crate) features: Vec<String>,
    #[arg(long)]
    pub(crate) no_default_features: bool,
    /// What this project calls it, when not its id (another fork of a package it has)
    #[arg(long)]
    pub(crate) name: Option<String>,
    /// What references name it by here, in place of its own abbreviation
    #[arg(long)]
    pub(crate) abbreviation: Option<String>,
    /// Its aliases here, in place of its own (repeat for more; `--alias ""` for none)
    #[arg(long = "alias")]
    pub(crate) aliases: Vec<String>,
}

#[derive(Args)]
pub(crate) struct NewArgs {
    /// The package's folder (by default its id, in the current folder)
    pub(crate) dir: Option<PathBuf>,
    /// The file it's made from: .epub (a book), .usx, .usfm, .osis, or .thml (a Bible)
    #[arg(long, required_unless_present = "kind")]
    pub(crate) from: Option<PathBuf>,
    /// What dependencies call it (by default its folder's name, else from its name:
    /// `the-dorean-principle`)
    #[arg(long)]
    pub(crate) id: Option<String>,
    /// Its title (an EPUB's own by default)
    #[arg(long)]
    pub(crate) name: Option<String>,
    /// What references name it by: for a Bible or a commentary, by default its name's initials
    /// (`BSB`); the other kinds have one only when it's given
    #[arg(long)]
    pub(crate) abbreviation: Option<String>,
    /// An author (repeat for more, in order; an EPUB's own by default)
    #[arg(long = "author")]
    pub(crate) authors: Vec<String>,
    /// A sentence about it (an EPUB's subtitle by default)
    #[arg(long)]
    pub(crate) description: Option<String>,
    /// Its first version (0.1.0 by default)
    #[arg(long)]
    pub(crate) version: Option<String>,
    /// bible, commentary, lexicon, book, collection, ... (by default from the file: an EPUB is a
    /// book)
    #[arg(long)]
    pub(crate) kind: Option<String>,
    /// BCP 47 (`en`; an EPUB's own by default; a collection needs none)
    #[arg(long)]
    pub(crate) language: Option<String>,
    /// An SPDX expression (`CC0-1.0`); shown, never enforced
    #[arg(long)]
    pub(crate) license: Option<String>,
    #[arg(long)]
    pub(crate) publisher: Option<String>,
    #[arg(long)]
    pub(crate) homepage: Option<String>,
    /// Where its repository is (its page: `https://github.com/someone/romans`)
    #[arg(long)]
    pub(crate) repository: Option<String>,
    /// A word a search should find it by (repeat for more)
    #[arg(long = "keyword")]
    pub(crate) keywords: Vec<String>,
    /// What it's about (up to 5; repeat for more; keywords say anything narrower)
    #[arg(long = "category", value_parser = category_values())]
    pub(crate) categories: Vec<String>,
    /// A book of the Bible it's mainly about, by name or USFM code (`--about Romans`; repeat for
    /// more)
    #[arg(long = "about")]
    pub(crate) about_books: Vec<String>,
    /// Don't make the folder a git repository
    #[arg(long)]
    pub(crate) no_git: bool,
}

#[derive(Args)]
pub(crate) struct PackageArgs {
    /// The package's folder (with its logoxide.toml)
    #[arg(default_value = ".")]
    pub(crate) path: PathBuf,
    /// Features to turn on, separated by commas
    #[arg(short = 'F', long, value_delimiter = ',')]
    pub(crate) features: Vec<String>,
    /// Leave out the default features
    #[arg(long)]
    pub(crate) no_default_features: bool,
}

impl PackageArgs {
    pub(crate) fn open(&self) -> logoxide::Result<Package> {
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

/// logoxide's categories, each with what it's for, as `--category` takes them
pub(crate) fn category_values() -> clap::builder::PossibleValuesParser {
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
pub(crate) struct BookFilterArgs {
    /// Only packages about a book of this testament (old/new)
    #[arg(long = "testament", short = 't')]
    pub(crate) testaments: Vec<String>,
    /// Same as `-t new`
    #[arg(long)]
    pub(crate) nt: bool,
    /// Same as `-t old`
    #[arg(long)]
    pub(crate) ot: bool,
    /// Leave out books of this testament
    #[arg(long = "exclude-testament")]
    pub(crate) exclude_testaments: Vec<String>,
    /// Only packages about a book of this genre (`-g epistles`; topos's genres)
    #[arg(long = "genre", short = 'g')]
    pub(crate) genres: Vec<String>,
    /// Leave out books of this genre
    #[arg(long = "exclude-genre")]
    pub(crate) exclude_genres: Vec<String>,
    /// Only packages about this book (`-b romans`, `-b ROM`)
    #[arg(long = "book", short = 'b')]
    pub(crate) books: Vec<String>,
    /// Leave out this book
    #[arg(long = "exclude-book")]
    pub(crate) exclude_books: Vec<String>,
}

impl BookFilterArgs {
    pub(crate) fn filters(&self) -> logoxide::topos_config::BookFilters {
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
