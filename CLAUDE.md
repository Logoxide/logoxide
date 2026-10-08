# biblix

Bibles, commentaries, lexicons, books, and media as versioned packages (git repositories with a
`biblix.toml`), read from many formats, cached, shared, and (later) installed; `bx` is the CLI. The
plan and the owner's decisions are in topos's `doc/biblix.md` (`../topos/doc/biblix.md`): read it
before changing the design.

## Layout

- `crates/biblix`: the library
  - `manifest.rs`: `biblix.toml`. One manifest name for packages and vaults (a vault's has no
    `[package]`), like `Cargo.toml`. "Index" means only a registry. Formats, Cargo-style features,
    and `git`/`path` dependencies; no default registry
  - `storage.rs`: `Storage` (read, list, write) over a folder (`FsStorage`), memory
    (`MemoryStorage`), or a host's files in WebAssembly, so nothing else touches `std::fs`
  - `source/`: one `Source` per format, each giving topos-bible-markup's tree (`Book`: nodes and
    verse spans). USX, and USFM through `usfm3` (chosen over usfm-onion, whose USX drops the space
    between adjacent `\w` words; both gave the KJV's 31,102 verses)
  - `package.rs`: opening a package picks the best readable format its enabled features turn on;
    `checksum()` hashes every file but `.git`
  - `cache.rs`: verse counts and parsed books (postcard) per `<id>/<version>-<checksum>`;
    `CachedPackage::add_to` gives topos's `Translations` the name, aliases, and counts
  - `lock.rs`: `biblix.lock` (each package's version, commit, checksum, features, and
    dependencies; `dependencies` at the top are what the manifest asks for)
  - `git.rs`: `Fetcher` (a repository's tags; its files at a commit, into any `Storage`).
    `GitCli` runs `git`
  - `archive.rs`: git over plain HTTP for hosts without `git`: `Refs::parse` reads the smart-HTTP
    ref listing (`refs_url`), `archive_urls` gives each host's `.tar.gz` form, `unpack` drops the
    archive's top folder. An archive hashes the same as a checkout (checked on the KJV, the BSB,
    and GitHub's own archives)
  - `catalog.rs`: catalogs to search (`Catalog::parse`: a `catalog.toml` of git packages, or
    eBible.org's `translations.csv`, only its downloadable, redistributable rows); `search` ranks
    exact abbreviations and ids first; a vault's `[settings] catalogs`, else eBible.org's list
  - `import.rs`: packages from other publishers' downloads: `ebible` turns `<id>_usfm.zip` into
    `biblix.toml` + `usfm/` + `copr.htm` (the license file), versioned by eBible.org's update
    date (`2026.10.8`); a vault uses it as a path package in `imports/<id>/`
  - `library.rs`: installed versions in `packages/<id>/<version>` (or `<version>+g<commit>` when
    not from its release tag), without `.git`; path packages stay where they are.
    `Library::packages(base)` is the resolver's `Packages` on disk (a new version is written
    beside its folder, then renamed into place)
  - `resolve.rs`: manifest + previous lock → new lock, installing as it goes, through `Packages`
    (where installed versions and path packages are) and a `Fetcher`. A lock entry is kept
    while the manifest still allows it (unless updating); fetched text is checked against the
    locked checksum; a tag's version must match its `biblix.toml`; git packages can't have path
    dependencies
  - `project.rs`: a folder with `biblix.toml` (edited with toml_edit, keeping comments; or
    `from_text` and `manifest_text` for hosts that do the reading and writing) and its
    lock; `translations()` gives topos the Bibles it asks for
  - `summary.rs`: `biblix.summary.toml` (verse counts in book order, sizes, element counts
    without structural markers) and `biblix.verses.bin` (`BXV1`, 12 bytes a verse: book,
    chapter, verse, last, 8 bytes of SHA-256 of its words with notes and headings left out);
    `diff` (removed, renumbered: old text at another number, bridged, reworded, added) and
    `Change` (patch/minor/major; before 1.0 the minor counts as major, like Cargo)
  - `release.rs`: summarize, compare with the newest release tag's summary (or its text, for
    releases before summaries), check the version, write, commit, tag `v{version}`
- `crates/biblix-lsp`: the language server (topos's `topos-lsp` until October 2026, moved with
  its history so topos never depends on biblix). `bible.rs`'s `Bible` reads a package or a plain
  USX folder through biblix sources; `settings.rs` finds the project (the `project` setting, a
  workspace folder with a `biblix.toml`, else the library's own; the server's machine library is
  set by `main.rs`, and tests have none) and loads `bibles = ["id"]` or, with no Bible settings,
  the project's Bibles. Its tests read topos's `crates/topos-lib/tests/cases/complete.txt` from
  `../topos`. The Neovim plugin (`~/Development/nvim-plugins/topos-bible.nvim`) runs it
- `crates/bx`: `init`, `add`, `remove`, `install`, `update`, `list`, `show` (`--git` reads a
  release's summary through a partial clone), `check`, `release` (`--set-version`), `diff`,
  `verify`, `search` (catalogs), `import ebible:<id>`, `catalog entry <git URL>`. Catalogs and
  imports come over HTTPS with ureq; packages through git. The project
  is `--project`, else the current folder if it has a `biblix.toml`, else the library's own
- `crates/biblix-ffi`: BoltFFI 0.31 bindings (npm and PyPI `biblix`), re-exporting topos-ffi
  (`pub use topos_bible::*`: BoltFFI exports a local dependency's items), so apps need one
  package. Its library is `biblix` (BoltFFI names artifacts after the package), so it depends on
  the core as `biblix_core`; nothing else may depend on topos-bible there (both libraries are
  `topos_bible`). The host does all I/O: `Biblix.openPackage(files, features, cache)` takes
  bytes and `BiblixPackage.cacheWrites()` hands back what the cache wrote; `openPackageDir` is
  native only. Opening goes through the `Biblix` object because BoltFFI's WebAssembly
  constructors return `null` on errors while methods throw. `ToposBook::from_tree` and
  `ToposTranslations::from`/`core` (Rust-only, in topos-ffi) carry biblix's books and counts
  across. `BiblixCatalog` (`catalog.rs`) searches catalogs, and `Biblix.importEbible` makes an
  eBible.org download into a package's files. `BiblixInstaller` (`installer.rs`) runs the ordinary resolver against what the host has
  given; whatever is missing (refs, an archive, an installed package's files) stops the round and
  is asked for (`FetchRequest`), and the host calls again. Its tests are unit tests
  (`src/tests.rs`): an rlib would collide with the core's `libbiblix.rlib`. Build with
  `npm install && npm run pack:wasm` or `npm run pack:python`; the examples open and install the
  real packages (`examples/node/install.ts`, whose lock and library `bx install` accepts)

## Decisions

- Its own repository; depends on topos (`../topos`, path dependencies until topos 0.6) and, later,
  sword-modules (`../sword-modules`) for SWORD. topos never depends on biblix
- Licenses are shown, never enforced. No default registry (the owner can't host mirrors)
- One package for apps: biblix-ffi includes topos's API
- Summaries with verse hashes are published for every package (not only Bibles); the parsed cache
  is never distributed
- Audio and video are referenced, never stored: a package lists each file's URL, size, and
  SHA-256, with its transcript and verse timings; releases are judged by those
- Hand-made test packages (BSB from a DBL bundle as USX, KJV from fetch.bible as USFM) are git
  repositories in `~/Development/biblix-packages/`, tagged `v1.0.0`

## Conventions

- Like topos: no `unwrap` in library code, errors in `BiblixError`, comments and docs in plain
  prose, `cargo fmt` and `cargo clippy --all-targets -D warnings` clean
- Check behavior on the real packages (`bx check ~/Development/biblix-packages/kjv`), not only
  unit tests
- Commit messages end with a `Co-Authored-By` line
