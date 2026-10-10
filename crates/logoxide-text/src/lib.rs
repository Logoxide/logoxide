/*!
logoxide-text: what's in a Bible or a book. It knows no packages, only files: a format's files
read as verses ([`source`]: USX, USFM, OSIS, ThML into logoxide-markup's tree), each verse's
words hashed by a rule that never changes ([`hash`]), a file's own title ([`titles`]), a
versification map's lines and the mapping of references through them ([`versification`]), a
recording's verse timings ([`timings`]), an EPUB's metadata ([`metadata`]) and the passages it
cites ([`citations`]), and the script a text is written in ([`script`]).

Files come through [`Files`], a read-only view of a folder, memory, or a host's files, so it runs
wherever logoxide-markup does, WebAssembly included. The package manager (`logoxide`) never
depends on this crate: whoever runs both hands it the facts this crate finds.
*/

pub mod archive;
pub mod citations;
pub mod error;
pub mod files;
pub mod hash;
pub mod metadata;
pub mod script;
pub mod skipped;
pub mod source;
pub mod timings;
pub mod titles;
pub mod versification;

use std::sync::LazyLock;

pub use error::{Result, TextError};
pub use files::{Files, MemoryFiles};
pub use logoxide_markup as markup;
pub use skipped::{PERIPHERALS, SkipReason, Skipped, is_peripheral};
pub use source::{Book, Source, TextFormat};
pub use topos_bible;

use topos_bible::data::BibleData;

static BIBLE_DATA: LazyLock<BibleData> = LazyLock::new(BibleData::with_deuterocanon);

/// topos's book names and versification, with the deuterocanon, so every book a translation can
/// have is known
pub fn bible_data() -> &'static BibleData {
    &BIBLE_DATA
}
