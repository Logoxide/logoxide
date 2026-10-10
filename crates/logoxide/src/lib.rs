/*!
logoxide: Bibles, commentaries, lexicons, books, and media as versioned packages.

A package is a git repository (or a registry's archive) with a `logoxide.toml` ([`manifest`]) and
its text in one or more formats. Bibles are read as verses from USX, USFM (through `usfm3`), OSIS,
and ThML (by the `logoxide-text` crate, behind the `read` feature: a book a file, or a whole Bible
in one file); books are kept as EPUBs, whose Bible references topos finds ([`references`]);
versification maps are Paratext `.vrs` lines (logoxide-text's `versification`); audio and video
are listed by URL and hash, never stored ([`media`]).

- [`Package::open`] reads a package from any [`Storage`] (a folder, memory, or a host's files in
  WebAssembly), choosing the best readable format its features turn on; [`Cache`] keeps its
  parsed books, and [`CachedPackage::add_to`] gives topos the translation's name, aliases, and
  verse counts, so `John 3:16 KJV` is checked against that translation
- [`Project`] is a folder with a `logoxide.toml` (a vault's has no `[package]`) and its
  `logoxide.lock` ([`lock`]): [`Project::install`] resolves its dependencies (git, registry
  archives, or paths) into the [`Library`], pinned by commit and checksum
- [`catalog`] searches catalogs (`catalog.toml`; other publishers' lists, read by the
  `logoxide-import` crate, which turns their downloads into packages)
- [`summary`] says what a version holds (verse counts, a hash of every verse) and how much a new
  version changed, which [`release`] checks against its version number

```no_run
use std::sync::Arc;

use logoxide::{
    Cache, Features, FsStorage, Package, reader::DefaultReader,
    topos_bible::data::translations::Translations,
};

fn main() -> logoxide::Result<()> {
    // A package's folder: its logoxide.toml and its text
    let package = Package::open(Arc::new(FsStorage::new("bibles/kjv")), Features::default())?;
    let info = package.info();
    println!("{} {}: {}", info.abbreviation, info.version, package.checksum()?);

    // Parsed books and verse counts are kept in a cache folder, by version and checksum
    let cache = Cache::new(Arc::new(FsStorage::new("cache")));
    let kjv = cache.open(package, &DefaultReader)?;
    if let Some(john) = logoxide::bible_data().books().search_usfm("JHN") {
        println!("John has {} chapters", kjv.book(john)?.chapter_verses().len());
    }

    // topos checks references naming the translation against its own verse counts
    let _translations = kjv.add_to(Translations::default());
    Ok(())
}
```

A vault's Bibles, installed as its lock file says (fetching runs the `git` command):

```no_run
use logoxide::{Library, Project, git::GitCli, reader::DefaultReader, resolve::Update};

fn main() -> logoxide::Result<()> {
    let mut project = Project::open("notes")?;
    let library = Library::new("library", "cache");
    project.install(&library, &GitCli::default(), Update::Nothing)?;
    let _translations = project.translations(&library, &DefaultReader)?;
    Ok(())
}
```

The file formats (`logoxide.toml`, `catalog.toml`, `logoxide.lock`, summaries) and the trust
model are specified in the repository's `doc/` folder, for other implementations.
*/

pub mod books;
pub mod cache;
pub mod catalog;
pub mod config;
pub mod create;
pub mod error;
pub mod git;
pub mod language;
pub mod library;
pub mod license;
pub mod lock;
pub mod locking;
pub mod manifest;
pub mod media;
pub mod net;
pub mod package;
pub mod project;
pub mod publish;
pub mod reader;
pub mod references;
pub mod release;
pub mod resolve;
pub mod roots;
mod stamps;
pub mod storage;
pub mod summary;
pub mod text;
#[cfg(feature = "read")]
pub mod titles;
pub mod topos_config;
pub mod works;

use std::sync::LazyLock;

pub use cache::{Cache, CachedPackage};
pub use error::{LogoxideError, Result};
pub use library::Library;
pub use lock::{LockedPackage, Lockfile};
pub use manifest::Manifest;
pub use package::{Features, Package};
pub use project::Project;
pub use reader::TextReader;
pub use storage::{FsStorage, MemoryStorage, Storage};
pub use topos_bible;

use topos_bible::data::BibleData;

static BIBLE_DATA: LazyLock<BibleData> = LazyLock::new(BibleData::with_deuterocanon);

/// topos's book names and versification, with the deuterocanon, so every book a translation can
/// have is known
pub fn bible_data() -> &'static BibleData {
    &BIBLE_DATA
}
