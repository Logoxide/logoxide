# logoxide

Bibles, commentaries, lexicons, books, and media as versioned packages (git repositories with a
`logoxide.toml`), read from many formats, cached, shared, and (later) installed; `lx` is the CLI. The
plan and the owner's decisions are in topos's `doc/logoxide.md` (`../topos/doc/logoxide.md`): read it
before changing the design.

## Layout

- `crates/logoxide`: the library
  - `manifest.rs`: `logoxide.toml`. One manifest name for packages and vaults (a vault's has no
    `[package]`), like `Cargo.toml`. "Index" means only a registry. Formats, Cargo-style features,
    and `git`/`path` dependencies; no default registry. Cargo-like metadata: `keywords`,
    `categories` (slugs), `repository`, `documentation`, `readme` (a declared file), and
    `about-books` (USFM codes of the books it's mainly about: the owner wants these said by the
    author, never derived from the text; `book_code` turns names into codes for `lx new --about`).
    Catalogs carry keywords, categories, and about-books; `Catalog::search` matches them (a word
    naming a book matches what's about it) and `lx search --book/--category` filter. A dependency can name another package
    (`kjv-bob = { package = "kjv", … }`, for forks) and give its own `abbreviation` and
    `aliases` (`Project::names`), and `enabled = false` turns it off without removing it (still
    locked and installed, so `lx clean` keeps it; `Project::set_enabled`, `is_enabled`, `dependencies_enabled`, and
    `in_use`: the lock's packages from dependencies that are on, which `bibles`, `translations`,
    and `versifications` read). `[package]` has `authors` (a list) and `description`. Kinds
    include `book` and `collection` (no `[formats]`, only dependencies; a project's Bibles
    include a collection's); the `epub` format is kept as files, not read as verses
  - `storage.rs`: `Storage` (read, list, write, remove) over a folder (`FsStorage`), memory
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
  - `cache.rs`: verse counts, parsed books, and an EPUB package's references (postcard;
    references also per logoxide version) per `<id>/<version>-<checksum>`; `Cache::references`
    without the counts (`lx show`); `CachedPackage::add_to` gives topos's `Translations` the name,
    aliases, and counts
  - `lock.rs`: `logoxide.lock` (each package's version, commit, checksum, features, and
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
    `/bibles/manifest.json`, an older form)), CCEL's page of its Bibles (`CCEL_CATALOG`, HTML:
    `parse_ccel` takes each `<h2>` link's work and title; no language or license, which the
    file's head gives on import), or a SWORD repository's `mods.d.tar.gz` (`parse_bytes` /
    `parse_sword`: Bibles and commentaries in OSIS or ThML that aren't locked; GBF isn't read).
    `SWORD_REPOSITORIES` names the HTTPS repositories in CrossWire's list (`crosswire`,
    `crosswire-beta`, `ebible`, …; others by host, `sword_repository_name`). Ids are prefixed
    (`ccel-kjv`, `sword-kjv`, `sword-<repo>-<module>` outside CrossWire's main one), since the
    first of an id wins when catalogs merge. `search` ranks exact abbreviations and ids first; a
    vault's `[settings] catalogs`, then those four (`DEFAULT_CATALOGS`: eBible.org, fetch.bible,
    CCEL, CrossWire) unless `default-catalogs = false`. Each imported kind names its prefix
    (`CatalogSource::prefix`: `ebible`, `fetch`, `ccel`, `sword`), and `Catalog::find_import`
    finds `ebible:<id>` (or `sword:crosswire/KJV`) in a catalog; `lx import` looks through the
    project's catalogs in order (and for `sword:<known repository>/…`, that repository's). `Project::add_catalog`/`remove_catalog` edit them (removing a
    default turns the defaults off and names the others; naming them all again turns them back on). Registries (`[settings.registries]`, `Project::add_registry`) are named catalogs: a folder
    of package repositories (`Catalog::from_packages`; lx scans it) or a catalog, so
    `dorean-local:<id>` or `dorean-local:<path in the folder>` names a package (`Catalog::find`,
    `split_registry_name`); a shared id needs the path
  - `import.rs`: packages from other publishers' downloads: `ebible` turns `<id>_usfm.zip` into
    `logoxide.toml` + `usfm/` + `copr.htm` (the license file), versioned by eBible.org's update
    date (`2026.10.8`); `fetch_bible` takes each book's USX (`fetch_bible_book_url`, lowercase
    codes; other paths answer with fetch.bible's site, which is refused) into `usx/` +
    `COPYRIGHT.txt`; `ccel` takes a Bible's ThML (`ccel_url`; restricted works answer with a
    page, refused) into `thml/<id>.xml`, named, dated, and licensed by its `<ThML.head>`
    (`thml_head`; version `<version>`, else `DC.Date`; right to left judged from its letters);
    `sword` unzips a module (`sword_zip_urls`: `packages/<Module>.zip`, then lowercase) into
    sword-modules' `MemoryFiles` and writes each book in the module's markup (`osis/John.xml`
    with verse milestones, or CCEL-style ThML), dropping the entries' own book and chapter
    milestones, making linked verses bridges (`n="16-17"`), and leaving out introductions that
    are another verse's entry; the conf gives version, license (`sword_license`), language, and
    `COPYRIGHT.txt`. A vault uses any of them as a path package in `imports/<id>/`. Updating:
    `upstream(entry)` is what each import writes as `upstream`, so `Catalog::find_upstream`
    finds an installed import's entry (no field records the import name); `compare_import`
    (current vs. a fresh import: `None` when the declared files match; the summaries' `Diff`;
    when the publisher's version isn't newer it compares at the current version, then raises
    it with `next_version`) and `apply_import` (removes declared files the fresh one lacks, through
    `Storage::remove`, writes the rest; other files stay).
    `CatalogSource::versions_track_updates` (eBible.org, SWORD) lets `lx` skip downloading
  - `media.rs`: `[media]` in a package's `logoxide.toml`: each audio or video file's path, URL,
    `mirrors` (tried in order after it: `MediaFile::urls`), size, SHA-256, and book, never
    stored in the package; `verify` checks a download,
    `store_path` keeps it by hash (`media/9f/9f86…`). Summaries count media; releases are judged
    by the text. Timings (audio Bibles: where each verse is) are WebVTT, `timings/<stem>.vtt` per
    file, each cue named by its verse (`JHN 3:16`, `JHN 3:17-18`; other cues are skipped):
    `parse_timings`, `timings`, and `check_package_timings` (verses the text lacks, another book,
    out of order, overlapping, missing or unlisted files), which `lx check`, `lx media check`, and
    `lx release` run. topos's subtitle parser isn't used: it only takes numbered cues
  - `library.rs`: installed versions in `packages/<id>/<version>+g<commit>` (so forks at one
    version never share a folder), without `.git`; path packages stay where they are.
    `Library::packages(base)` is the resolver's `Packages` on disk (a new version is written
    beside its folder, then renamed into place)
  - `resolve.rs`: manifest + previous lock → new lock, installing as it goes, through `Packages`
    (where installed versions and path packages are) and a `Fetcher` (git, and `download` for
    registries: paths and `file://` by default, HTTPS in lx and through the host in the
    bindings). A `registry` dependency (a catalog's URL) gets the newest release its catalog lists
    that the requirement allows, or the locked one (whose archive URL and hash the lock has, so
    no catalog is read); the archive must hash to `sha256` and its files to `checksum`; lock
    entries have `registry`, `archive`, `mirrors` (the release's, tried in turn by
    `Fetcher::download_any`, which passes over bytes that don't hash as listed), `sha256`, and
    its library folder is `<version>+a<hash>`. A lock entry is kept
    while the manifest still allows it (unless updating); fetched text is checked against the
    locked checksum; a tag's version must match its `logoxide.toml`; git packages can't have path
    dependencies
  - `project.rs`: a folder with `logoxide.toml` (edited with toml_edit, keeping comments; or
    `from_text` and `manifest_text` for hosts that do the reading and writing) and its
    lock; `translations()` gives topos the Bibles it asks for
  - `summary.rs`: `logoxide.summary.toml` (verse counts in book order, sizes, element counts
    without structural markers) and `logoxide.verses.bin` (`LXV1`, 12 bytes a verse: book,
    chapter, verse, last, 8 bytes of SHA-256 of its words with notes and headings left out);
    `diff` (removed, renumbered: old text at another number, bridged, reworded, added) and
    `Change` (patch/minor/major; before 1.0 the minor counts as major, like Cargo).
    `book_hashes` hashes one book's verses the same way (`LogoxidePackage.verseHashes`), which
    hosts put in links (`&ver=…&vh=…`) to find a verse again after an update
    `files-checksum` hashes the formats' files alone, so `diff_summaries` sees a book (no
    verses) change: a minor change. The passages a book cites go in `logoxide.references.bin`
    (the summary counts its references)
  - `references.rs`: the Bible references in a package's EPUBs, found by topos-formats
    (`search_epub_reader`, pure Rust, so WebAssembly too). `find` reads the books: each
    reference's OSIS passage, file, CFI, chapter, UTF-16 range, line, column, spine item, and
    paragraph (about 0.1 s a book natively, 0.25 s in WebAssembly). Only `cited` is published
    (`LXR1`: deflate over postcard of each passage and its count, in Bible order; 2.6 KB for 937
    references): locations are only of use with the book, which gives them again
  - `package.rs` also says what a package is made of: `declared_files` (the manifest, each
    format's files by its extensions, the license file, summary files, timings, `include`
    globs); `checksum` hashes only those, so a reader's annotations beside a book never change
    it. `other_files` are kept by repairs and stop `lx clean`
  - `release.rs`: summarize, compare with the newest release tag's summary (or its text, for
    releases before summaries), check the version, write, commit, tag `v{version}`. A package in
    a folder of its repository (`GitCli::package_in`) has its own tags, `<folder>/v{version}`,
    and its commits take only its folder (nothing new: the current commit is tagged)
  - `titles.rs`: each file's title (a Bible book's or an EPUB's): `[titles]` in the manifest,
    else the file's (`\toc2`, `\h`, `\toc1`; `dc:title`), else the book's English name (a file of
    many books: the package's name). Summaries list them (`titles`); a changed title is a major
    change (`Diff::retitled`); `lx check` and `lx release` refuse an untitled file. Hosts name
    what they show by it (`BookFile.title`, `LogoxidePackage.titles()`)
  - `publish.rs`: release archives for registries: `pack` (declared files, sorted, no times or
    owners, so the same files make the same bytes), `sha256`, `add_release` (a catalog.toml's
    `[[package.release]]`: version, url relative to the catalog, optional `mirrors`, sha256, checksum, size; a
    published version never changes), `yank` (`yanked = true`: `CatalogEntry::release`
    passes over it; locks still install it). `CatalogSource::Registry { catalog }` and
    `CatalogEntry::releases`/`release(req)`; `relative_url`
  - `versification.rs`: versification maps (`kind = "versification-map"`: `[package]
    versification` to `maps-to`, a `vrs` format of Paratext `.vrs` mapping lines, so `eng.vrs`
    reads as it is; verse 0 is a psalm's title). Unnamed verses keep their number unless another
    took it (then no counterpart); `Versifications::route` follows maps both ways and through
    each other; `Project::numbering` turns a Bible's name into its versification and counts,
    `Project::map_reference` maps. The summary counts `mappings`; any change is minor. Sample
    package: `crates/logoxide/tests/fixtures/english-original` (lines from Paratext's eng.vrs)
- `crates/logoxide-lsp`: the language server (topos's `topos-lsp` until October 2026, moved with
  its history so topos never depends on logoxide). `bible.rs`'s `Bible` reads a package or a plain
  USX folder through logoxide sources; `settings.rs` finds the project (the `project` setting, a
  workspace folder with a `logoxide.toml`, else the library's own; the server's machine library is
  set by `main.rs`, and tests have none) and loads `bibles = ["id"]` or, with no Bible settings,
  the project's Bibles. `epub = true` (opt-in) searches EPUBs (and `pdf = true` PDFs, with
  `--features pdf`) through topos's file cache on disk (the CLI's: same fingerprint), as
  `logoxide-file://` documents whose text `logoxide/fileText` gives (`workspace.rs`: `BookSearch`,
  `book_text`); logoxide.nvim opens them read-only. Its tests read topos's `crates/topos-lib/tests/cases/complete.txt` from
  `../topos`. The Neovim plugin (`~/Development/nvim-plugins/logoxide.nvim`) runs it
- `crates/lx` (package `logoxide-cli`, binary `lx`): `new` (`--from <file>`: an EPUB's title, subtitle, authors, language, and
  publisher fill in `[package]` (`create.rs`: `epub_metadata`, `new_from_file`); USX, USFM,
  OSIS, ThML need `--name` and `--language`; `--kind collection` makes a collection; the folder
  names it, like `cargo new`; then `lx release`), `init`, `add` (`lx add github:owner/repo` or `--git`/`--path`; `--name`,
  `--abbreviation`, `--alias` for forks; refuses a name or abbreviation already used), `remove`, `enable`/`disable` (`enabled = false`), `is-enabled` (prints on or off; exits 1 when off), `install`, `update`, `list` (`(off)`; `--enabled`, `--disabled`), `show` (`--git` reads a
  release's summary through a partial clone), `check`, `release` (`--set-version`), `diff`,
  `verify`, `info`, `clean` (versions no lock here uses; `--remove` deletes), `search` (catalogs; `--registry <name>`), `import ebible:<id>`, `fetch:<id>`, `ccel:<id>`, or `sword:<repository>/<module>` (`--update [name]`, `--dry-run`: make imports again, replace changed ones, re-lock them), `catalog list|add|remove|entry|build` (`add <folder or catalog> --name <registry>`; `build <folder>` writes a catalog.toml of its repositories' newest releases, read with `git show`, each with its `path` and `commit`, unchanged ones kept unless `--force`; `--url` with `{path}`/`{name}`; folder registries read releases the same way), `registry init <folder | github:owner/repo>`, `publish <package> --to …` (`--all`: every
  released package in a folder, at any depth), `yank <id> <version> --from … [--undo]`
  (`publish.rs`: a folder registry, or GitHub releases through `gh`, the catalog being release
  `index`'s file and each archive a release `<id>-v<version>`), `add --registry <catalog> <id>`,
  `media add <file> --url` (repeated: mirrors)
  (lists a file by size and hash), `media check` (timings, then downloads and checks every listed file from each of its URLs; `--offline` for only the timings), `map "<reference>" --from <bible or
  versification> --to …` (`--with <map package>` adds one not in the project). Completion
  (`complete.rs`, clap_complete's dynamic engine: `source <(COMPLETE=bash lx)`) gives registries'
  packages, dependencies, and registries; bash needs `:` out of `COMP_WORDBREAKS`. `lx add`
  installs and locks what it adds. Catalogs and
  imports come over HTTPS with ureq; packages through git. The project
  is `--project`, else the current folder if it has a `logoxide.toml`, else the library's own
- `crates/logoxide-ffi`: BoltFFI 0.31 bindings (npm and PyPI `logoxide`), re-exporting topos-ffi
  (`pub use topos_bible::*`: BoltFFI exports a local dependency's items), so apps need one
  package. Its library is `logoxide` (BoltFFI names artifacts after the package), so it depends on
  the core as `logoxide_core`; nothing else may depend on topos-bible there (both libraries are
  `topos_bible`). The host does all I/O: `Logoxide.openPackage(files, features, cache)` takes
  bytes and `LogoxidePackage.cacheWrites()` hands back what the cache wrote; `openPackageDir` is
  native only. Opening goes through the `Logoxide` object because BoltFFI's WebAssembly
  constructors return `null` on errors while methods throw. `ToposBook::from_tree` and
  `ToposTranslations::from`/`core` (Rust-only, in topos-ffi) carry logoxide's books and counts
  across. `LogoxideCatalog` (`catalog.rs`) searches catalogs, and `Logoxide.importEbible`,
  `importFetchBible`, `importCcel`, and `importSword` (a `Sword` source lists its `zipUrls`, to
  try in order) make downloads into a package's files; `parseCatalogBytes` reads any catalog,
  and a SWORD repository's `.tar.gz` only that way. Updating an import: `LogoxideCatalog.findUpstream`
  (its entry, from `PackageInfo.upstream`), `Logoxide.importIsCurrent` (skip the download), and
  `Logoxide.updateImport(current, fresh)` (`ImportUpdate`: versions, `ChangeKind`, description,
  `files` to write and `removed` paths to delete; then install with `UpdateMode.Only`). `create.rs`: `fileMetadata`,
  `newPackage` (a package's files from one file), `newCollection`, `parseReferences`, and
  `LogoxidePackage.references()` and `epubFiles()`. `LogoxideVersifications` (`versification.rs`,
  from `Logoxide.versifications()`, then `withMap(package)` or `withVrs`) maps references
  (`MappedPassage`: text, OSIS, verses, missing). `LogoxideProject.packages()` lists each package
  the manifest asks for followed by what it brings in (`LockedEntry.dependencyOf`), so hosts see
  a collection's packages; `LockedEntry.enabled` is false for one turned off (`setEnabled`) and
  what it brings in; `isEnabled`, `enabledDependencies`, `disabledDependencies`, and
  `Logoxide.packageInfo` (a package's details from its `logoxide.toml` alone, to name one that's off). `LogoxideInstaller` (`installer.rs`) runs the ordinary resolver against what the host has
  given (`Download` requests, answered with `provideDownload(url, …)` from the first of `urls`
  that answers, are registries' catalogs and archives, with their mirrors; `LogoxideProject.registries()`, `addRegistry`, `removeRegistry`;
  `Dependency.registry`); whatever is missing (refs, an archive, an installed package's files) stops the round and
  is asked for (`FetchRequest`), and the host calls again. Its tests are unit tests
  (`src/tests.rs`): an rlib would collide with the core's `liblogoxide.rlib`. Build with
  `npm install && npm run pack:wasm` or `npm run pack:python`; the examples open and install the
  real packages (`examples/node/install.ts`, whose lock and library `lx install` accepts)

## Decisions

- Its own repository (github.com/Logoxide/logoxide); depends on topos 0.6 and sword-modules 0.1
  from crates.io (until they're published, `~/Development/.cargo/config.toml`, outside every repository, patches in
  `../topos` and `../sword-modules`). The CLI's package is `logoxide-cli` (`lx` on crates.io is
  taken), its binary `lx`. `crates/logoxide-ffi` is excluded from the workspace: a workspace of its
  own whose `topos-ffi` and topos crates are paths into `../topos` (BoltFFI only exports path
  dependencies' items; one topos-bible for both), so CI checks out topos at `TOPOS_REF`.
  sword-modules is used without its installer: modules are read from memory through
  its `Files`, so WebAssembly too. topos never depends on logoxide. sword-modules is a
  clean-room crate: read its `CLAUDE.md` and `CLEANROOM.md` before changing it (interface into
  `docs/spec/api.md` first; never read libsword's source)
- Door43 isn't imported yet: git.door43.org wasn't reachable from the owner's machine
  (October 2026). The plan is in topos's `doc/logoxide.md` (step 7)
- Licenses are shown, never enforced. No default registry (the owner can't host mirrors)
- One package for apps: logoxide-ffi includes topos's API
- Summaries with verse hashes are published for every package (not only Bibles); the parsed cache
  is never distributed
- Audio and video are referenced, never stored: a package lists each file's URL, size, and
  SHA-256, with its transcript and verse timings; releases are judged by those
- Hand-made test packages (BSB from a DBL bundle as USX, KJV from fetch.bible as USFM) are git
  repositories in `~/Development/logoxide-packages/`, tagged `v1.0.0`

## Conventions

- Like topos: no `unwrap` in library code, errors in `LogoxideError`, comments and docs in plain
  prose, `cargo fmt` and `cargo clippy --all-targets -D warnings` clean
- Check behavior on the real packages (`lx check ~/Development/logoxide-packages/kjv`), not only
  unit tests
- Commit messages end with a `Co-Authored-By` line
