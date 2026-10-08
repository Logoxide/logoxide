/*!
biblix: Bible translations as versioned packages.

A package is a git repository with a `biblix.toml` ([`manifest`]) and its text in one or more
formats ([`source`]: USX, USFM). [`Package::open`] reads one from any [`Storage`] (a folder,
memory, or a host's files in WebAssembly), [`Cache`] keeps its parsed books, and
[`CachedPackage::add_to`] gives topos the translation's name, aliases, and verse counts.

The plan, with what comes next (the library, `bx`, git sources, summaries, and versioning), is
`doc/biblix.md` in topos.
*/

pub mod cache;
pub mod error;
pub mod manifest;
pub mod package;
pub mod source;
pub mod storage;

use std::sync::LazyLock;

pub use cache::{Cache, CachedPackage};
pub use error::{BiblixError, Result};
pub use manifest::Manifest;
pub use package::{Features, Package};
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
