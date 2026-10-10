# logoxide

The library behind [`lx`](https://crates.io/crates/logoxide-cli): Bibles, commentaries,
lexicons, books, and media as versioned packages. A package is a git repository (or a registry's
release archive) with a `logoxide.toml` and its text in one or more formats; logoxide reads it,
caches it, resolves and locks a project's packages, and tells
[topos](https://crates.io/crates/topos-bible) each translation's name, aliases, and verse counts,
so references like `John 3:16 KJV` are checked against that translation.

- Bibles are read as verses from USX, USFM (through [`usfm3`](https://crates.io/crates/usfm3)),
  OSIS, and ThML, a book a file or a whole Bible in one; books are kept as EPUBs, whose Bible
  references topos finds
- Everything reads through a `Storage` (a folder, memory, or a host's files), so the same code
  runs in WebAssembly
- Catalogs (`catalog.toml`; eBible.org's, fetch.bible's, CCEL's, and SWORD repositories' lists
  through the `logoxide-import` crate, which makes those publishers' downloads into packages),
  registries of release archives, versification maps, and audio and video listed by URL and hash

```toml
[dependencies]
logoxide = "0.1"
```

Opening a package and giving topos its verse counts:

```rust,no_run
use std::sync::Arc;

use logoxide::{Cache, Features, FsStorage, Package, topos_bible::data::translations::Translations};

fn main() -> logoxide::Result<()> {
    let package = Package::open(Arc::new(FsStorage::new("bibles/kjv")), Features::default())?;
    println!("{} {}", package.info().abbreviation, package.checksum()?);

    let cache = Cache::new(Arc::new(FsStorage::new("cache")));
    let kjv = cache.open(package)?;
    let _translations = kjv.add_to(Translations::default());
    Ok(())
}
```

Installing a vault's packages as its `logoxide.lock` says (fetching runs the `git` command):

```rust,no_run
use logoxide::{Library, Project, git::GitCli, resolve::Update};

fn main() -> logoxide::Result<()> {
    let mut project = Project::open("notes")?;
    let library = Library::new("library", "cache");
    project.install(&library, &GitCli::default(), Update::Nothing)?;
    let _translations = project.translations(&library)?;
    Ok(())
}
```

The file formats (`logoxide.toml`, `catalog.toml`, `logoxide.lock`, summaries) and the trust
model are specified in the repository's [`doc/`](https://github.com/Logoxide/logoxide/tree/main/doc)
folder. Licenses are shown, never enforced: check that you may use and redistribute a text.

For apps in TypeScript/WebAssembly or Python, the `logoxide` packages on npm and PyPI wrap this
library with topos's API. License: CC0-1.0.
