# biblix

Bibles, commentaries, lexicons, books, and media as versioned packages (git repositories with a
`biblix.toml`), read from many formats, cached, shared, and (later) installed; `bx` is the CLI. The
plan and the owner's decisions are in topos's `doc/biblix.md` (`../topos/doc/biblix.md`): read it
before changing the design.

## Layout

- `crates/biblix`: the library
  - `manifest.rs`: `biblix.toml`. One manifest name for packages and vaults (a vault's has no
    `[package]`), like `Cargo.toml`. "Index" means only a registry. Formats, Cargo-style features,
    and `git`/`path` dependencies; no default registry. A dependency can name another package
    (`kjv-bob = { package = "kjv", … }`, for forks) and give its own `abbreviation` and
    `aliases` (`Project::names`). `[package]` has `authors` (a list) and `description`. Kinds
    include `book` and `collection` (no `[formats]`, only dependencies; a project's Bibles
    include a collection's); the `epub` format is kept as files, not read as verses
  - `storage.rs`: `Storage` (read, list, write) over a folder (`FsStorage`), memory
    (`MemoryStorage`), or a host's files in WebAssembly, so nothing else touches `std::fs`
  - `source/`: one `Source` per format, each giving topos-bible-markup's tree (`Book`: nodes and
    verse spans). USX, and USFM through `usfm3` (chosen over usfm-onion, whose USX drops the space
    between adjacent `\w` words; both gave the KJV's 31,102 verses). OSIS and ThML Bibles
    through topos-bible-markup (`whole.rs`: a file can hold every book; it's scanned for the
    books its verse markers name, cut into books, and its last parse kept). ThML's empty
    `<scripture parsed=…/>` elements (CCEL's Bibles) become verse markers. Preference: USX,
    USFM, OSIS, ThML
  - `package.rs`: opening a package picks the best readable format its enabled features turn on;
    a package that isn't a Bible (a book, a collection) may have none and opens with
    `source::NoText` (no books, so no verses in its summary). `checksum()` hashes every file but
    `.git`
  - `cache.rs`: verse counts and parsed books (postcard) per `<id>/<version>-<checksum>`;
    `CachedPackage::add_to` gives topos's `Translations` the name, aliases, and counts
  - `lock.rs`: `biblix.lock` (each package's version, commit, checksum, features, and
    dependencies; `dependencies` at the top are what the manifest asks for, each named
    `id version+g<commit>`, older lock files' `id version` still read; `rename` is the
    manifest's name when it isn't the id; `get` finds by that name)
  - `git.rs`: `Fetcher` (a repository's tags; its files at a commit, into any `Storage`).
    `GitCli` runs `git`. `expand_url`: `github:owner/repo` (also `gitlab:`, `codeberg:`) to its
    URL, wherever a URL is taken; manifests and locks keep the full URL
  - `archive.rs`: git over plain HTTP for hosts without `git`: `Refs::parse` reads the smart-HTTP
    ref listing (`refs_url`), `archive_urls` gives each host's `.tar.gz` form, `unpack` drops the
    archive's top folder. An archive hashes the same as a checkout (checked on the KJV, the BSB,
    and GitHub's own archives)
  - `catalog.rs`: catalogs to search (`Catalog::parse` by extension: a `catalog.toml` of git
    packages, eBible.org's `translations.csv` (its downloadable, redistributable rows), or
    fetch.bible's `manifest.json` (`https://v1.fetch.bible/manifest.json`, not
    `/bibles/manifest.json`, an older form)); `search` ranks exact abbreviations and ids first; a
    vault's `[settings] catalogs`, then both of those (`DEFAULT_CATALOGS`) unless
    `default-catalogs = false`. Each imported kind names its prefix (`CatalogSource::prefix`:
    `ebible`, `fetch`), and `Catalog::find_import` finds `ebible:<id>` in a catalog; `bx import`
    looks through the project's catalogs in order. `Project::add_catalog`/`remove_catalog` edit them (removing a
    default turns the defaults off and names the other; naming both again turns them back on). Registries (`[settings.registries]`, `Project::add_registry`) are named catalogs: a folder
    of package repositories (`Catalog::from_packages`; bx scans it) or a catalog, so
    `dorean-local:<id>` or `dorean-local:<path in the folder>` names a package (`Catalog::find`,
    `split_registry_name`); a shared id needs the path
  - `import.rs`: packages from other publishers' downloads: `ebible` turns `<id>_usfm.zip` into
    `biblix.toml` + `usfm/` + `copr.htm` (the license file), versioned by eBible.org's update
    date (`2026.10.8`); `fetch_bible` takes each book's USX (`fetch_bible_book_url`, lowercase
    codes; other paths answer with fetch.bible's site, which is refused) into `usx/` +
    `COPYRIGHT.txt`. A vault uses either as a path package in `imports/<id>/`
  - `media.rs`: `[media]` in a package's `biblix.toml`: each audio or video file's path, URL,
    size, SHA-256, and book, never stored in the package; `verify` checks a download,
    `store_path` keeps it by hash (`media/9f/9f86…`). Summaries count media; releases are judged
    by the text. Timings (audio Bibles: where each verse is) are WebVTT, `timings/<stem>.vtt` per
    file, each cue named by its verse (`JHN 3:16`, `JHN 3:17-18`; other cues are skipped):
    `parse_timings`, `timings`, and `check_package_timings` (verses the text lacks, another book,
    out of order, overlapping, missing or unlisted files), which `bx check`, `bx media check`, and
    `bx release` run. topos's subtitle parser isn't used: it only takes numbered cues
  - `library.rs`: installed versions in `packages/<id>/<version>+g<commit>` (so forks at one
    version never share a folder), without `.git`; path packages stay where they are.
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
    `Change` (patch/minor/major; before 1.0 the minor counts as major, like Cargo).
    `files-checksum` hashes the formats' files alone, so `diff_summaries` sees a book (no
    verses) change: a minor change. The passages a book cites go in `biblix.references.bin`
    (the summary counts its references)
  - `references.rs`: the Bible references in a package's EPUBs, found by topos-formats
    (`search_epub_reader`, pure Rust, so WebAssembly too). `find` reads the books: each
    reference's OSIS passage, file, CFI, chapter, UTF-16 range, line, column, spine item, and
    paragraph (about 0.1 s a book natively, 0.25 s in WebAssembly). Only `cited` is published
    (`BXR1`: deflate over postcard of each passage and its count, in Bible order; 2.6 KB for 937
    references): locations are only of use with the book, which gives them again
  - `package.rs` also says what a package is made of: `declared_files` (the manifest, each
    format's files by its extensions, the license file, summary files, timings, `include`
    globs); `checksum` hashes only those, so a reader's annotations beside a book never change
    it. `other_files` are kept by repairs and stop `bx clean`
  - `release.rs`: summarize, compare with the newest release tag's summary (or its text, for
    releases before summaries), check the version, write, commit, tag `v{version}`
- `crates/biblix-lsp`: the language server (topos's `topos-lsp` until October 2026, moved with
  its history so topos never depends on biblix). `bible.rs`'s `Bible` reads a package or a plain
  USX folder through biblix sources; `settings.rs` finds the project (the `project` setting, a
  workspace folder with a `biblix.toml`, else the library's own; the server's machine library is
  set by `main.rs`, and tests have none) and loads `bibles = ["id"]` or, with no Bible settings,
  the project's Bibles. Its tests read topos's `crates/topos-lib/tests/cases/complete.txt` from
  `../topos`. The Neovim plugin (`~/Development/nvim-plugins/topos-bible.nvim`) runs it
- `crates/bx`: `new` (`--from <file>`: an EPUB's title, subtitle, authors, language, and
  publisher fill in `[package]` (`create.rs`: `epub_metadata`, `new_from_file`); USX, USFM,
  OSIS, ThML need `--name` and `--language`; `--kind collection` makes a collection; the folder
  names it, like `cargo new`; then `bx release`), `init`, `add` (`bx add github:owner/repo` or `--git`/`--path`; `--name`,
  `--abbreviation`, `--alias` for forks; refuses a name or abbreviation already used), `remove`, `install`, `update`, `list`, `show` (`--git` reads a
  release's summary through a partial clone), `check`, `release` (`--set-version`), `diff`,
  `verify`, `info`, `clean` (versions no lock here uses; `--remove` deletes), `search` (catalogs; `--registry <name>`), `import ebible:<id>` or `fetch:<id>`, `catalog list|add|remove|entry|build` (`add <folder or catalog> --name <registry>`; `build <folder>` writes a catalog.toml of its repositories' newest releases, read with `git show`, each with its `path` and `commit`, unchanged ones kept unless `--force`; `--url` with `{path}`/`{name}`; folder registries read releases the same way), `media add <file> --url`
  (lists a file by size and hash), `media check` (timings, then downloads and checks every listed file; `--offline` for only the timings). Completion
  (`complete.rs`, clap_complete's dynamic engine: `source <(COMPLETE=bash bx)`) gives registries'
  packages, dependencies, and registries; bash needs `:` out of `COMP_WORDBREAKS`. `bx add`
  installs and locks what it adds. Catalogs and
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
  across. `BiblixCatalog` (`catalog.rs`) searches catalogs, and `Biblix.importEbible` and
  `importFetchBible` make downloads into a package's files. `create.rs`: `fileMetadata`,
  `newPackage` (a package's files from one file), `newCollection`, `parseReferences`, and
  `BiblixPackage.references()` and `epubFiles()`. `BiblixProject.packages()` lists each package
  the manifest asks for followed by what it brings in (`LockedEntry.dependencyOf`), so hosts see
  a collection's packages. `BiblixInstaller` (`installer.rs`) runs the ordinary resolver against what the host has
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
