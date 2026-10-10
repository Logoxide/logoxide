/*!
logoxide: Bibles, commentaries, lexicons, books, and media as versioned packages.

A package is a git repository with a `logoxide.toml` ([`manifest`]) and its text in one or more
formats ([`source`]: USX, USFM). [`Package::open`] reads one from any [`Storage`] (a folder,
memory, or a host's files in WebAssembly), [`Cache`] keeps its parsed books, and
[`CachedPackage::add_to`] gives topos the translation's name, aliases, and verse counts.

The plan, with what comes next (the library, `lx`, git sources, summaries, and versioning), is
`doc/logoxide.md` in topos.
*/

pub mod archive;
pub mod cache;
pub mod catalog;
pub mod config;
pub mod create;
pub mod error;
pub mod git;
pub mod import;
pub mod library;
pub mod lock;
pub mod manifest;
pub mod media;
pub mod package;
pub mod project;
pub mod publish;
pub mod references;
pub mod release;
pub mod resolve;
pub mod source;
pub mod storage;
pub mod summary;
pub mod titles;
pub mod topos_config;
pub mod versification;

use std::sync::LazyLock;

pub use cache::{Cache, CachedPackage};
pub use error::{LogoxideError, Result};
pub use library::Library;
pub use lock::{LockedPackage, Lockfile};
pub use manifest::Manifest;
pub use package::{Features, Package};
pub use project::Project;
pub use storage::{FsStorage, MemoryStorage, Storage};
pub use topos_bible;
pub use topos_bible_markup as markup;

use topos_bible::data::BibleData;

static BIBLE_DATA: LazyLock<BibleData> = LazyLock::new(BibleData::with_deuterocanon);

/// topos's book names and versification, with the deuterocanon, so every book a translation can
/// have is known
pub fn bible_data() -> &'static BibleData {
    &BIBLE_DATA
}
