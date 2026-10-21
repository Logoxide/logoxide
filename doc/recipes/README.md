# Recipes

What apps built on logoxide most often do, each in TypeScript (the npm package), Python (the
PyPI package), and Rust (the crates). Start with [Bibles in your app](../apps.md), which gets the
text: each recipe starts from its helpers (`lx`, `topos`, `get`) and runs in order, as one
program in each language.

Each recipe was run against logoxide 0.1.1, with live downloads from fetch.bible, eBible.org,
CrossWire, and CCEL; what a line prints follows `→`. The publishers' lists change, so counts can
drift. CI doesn't run these, since they download.

| Recipe | For |
|---|---|
| [Bibles in your app](../apps.md) | finding translations, downloading a book at a time or a whole Bible, files a user brings, verses, caching |
| [Showing passages](showing-passages.md) | a reader view: what someone typed, as HTML with a stylesheet or as text with its citation; chapters; a verse for the day |
| [References in text](references-in-text.md) | finding references in notes or posts, linking them, showing their verses on hover, storing them, completing them as they're typed |
| [Comparing translations](comparing-translations.md) | a parallel view, what one translation lacks, and translations numbered differently (`.vrs` maps) |
| [Searching a Bible's words](searching-text.md) | a word index of your own: words, phrases, search as you type, highlighting, and how fast and large it is |
| [Links that survive corrections](stable-links.md) | bookmarks and highlights that find their verse and words again after a translation is corrected or renumbered |
| [Commentaries, dictionaries, and cross-references](commentaries-and-dictionaries.md) | Matthew Henry, Calvin, the Treasury of Scripture Knowledge, Strong's, Torrey, from CrossWire and CCEL |
| [Audio with timings](audio-with-timings.md) | highlighting each verse as a recording plays, seeking to a verse, checking and keeping recordings |
| [A passage server](passage-server.md) | a small HTTP API serving passages as JSON, with caching and errors |
| [Reading a vault](reading-a-vault.md) | an app reading the Bibles its user keeps with `lx`, shared with the editor and Obsidian |
| [Converting formats](converting-formats.md) | USFM, USX, and USJ, one book or a whole Bible, in code or with `lx convert` |

Every method is in the [bindings' README](../../crates/logoxide-ffi/README.md); `lx`, the
command line, is in [its own README](../../crates/lx/README.md) and the
[walk-through](../workflow.md).
