# Architecture: the crates, module by module

A paragraph per module, kept current: a change to a module changes its paragraph here. It is
being folded into each crate's rustdoc and README, after which this file goes. The
specifications of the formats are the other files in this folder.

## Layout

- `crates/logoxide`: the library. The package manager never reads text (the owner's decision of
  2026-10-10): everything that parses a format is behind the
  `read` feature (on by default; `cargo check -p logoxide --no-default-features --all-targets`
  must build, and CI runs it), and the rest asks a `reader::TextReader` for the facts it needs.
  Its exports: the formats, `storage`, `reader`, `error`, and the
  package manager's modules stay public (each is used by lx, the import, the app, the LSP, or
  the FFI); `archive` is `net::archive`, `registry_build` is `publish::registry_build`, and
  `stamps` and `package::glob` are crate-private. No re-exports under the old paths
  - `reader.rs`: the boundary. `TextReader` (`counts`: each book's verse counts; `facts`: a
    `TextFacts`, the text's half of a summary, with the verse hashes by a `HashRule`, titles and
    the untitled files, cited passages, the mapping count, `entries` (each dictionary entry's or
    cross-reference passage's `EntryHash`: 8 bytes of the key's hash, 7 of its text's by the
    verse rule, from logoxide-text's `hash::entry_hashes`), and `Skipped`; `timing_problems`) is
    taken by `summary::summarize`, `release::plan`/`summary_in`, `Cache::open`,
    `Project::translations`/`numbering`/`map_reference`, and `import::compare_import`. `NoReader`
    reads nothing (tests, hosts that only install); `DefaultReader` (`read`) is the adapter onto
    `crates/logoxide-text` (the owner's C5, taken as a feature-gated adapter in the core rather
    than three copies beside lx, the FFI, and the LSP; logoxide-text never depends on logoxide):
    `StorageFiles` makes a `Storage` the `Files` logoxide-text reads, `From<TextError>` maps its
    errors, `versification_map(package)` reads a map package's `.vrs` files, and the text crate's
    `Skipped` and `VerseRecord` (topos `BookId`) become the core's `Skipped` and `VerseHash`
    (`books.rs` numbers, which stay the summary format's). `Package::text_format` says which
    format is being read (`Format::is_verse_text`: not a dictionary's or cross-references',
    which `Project::bibles` leaves out) without a parser; `Package::source()` (`read`) is
    logoxide-text's `Source`
  - `config.rs`: logoxide's own `~/.config/logoxide/config.toml` (or `LOGOXIDE_CONFIG`):
    `library`, `cache`, `catalogs`, `[registries]`, `git-timeout`, `git-list-timeout`,
    `catalog-max-age` (seconds or `"90s"`/`"30m"`/`"12h"`/`"7d"`/`"2w"`, `parse_duration`;
    `CATALOG_MAX_AGE` 7 days; 0 keeps none; `Config::catalog_max_age`), machine-wide (flags and env win over
    library/cache; a project's catalogs come first and its registry names win). lx reads it once
    at start (a bad file is an error), logoxide-lsp for its library. The owner chose no `[topos]`
    table: topos's settings stay in topos's config, and no `lx refs`/`lx topos` command
  - `manifest.rs`: `logoxide.toml`. One manifest name for packages and vaults (a vault's has no
    `[package]`), like `Cargo.toml`. "Index" means only a registry. Formats, Cargo-style features,
    and `git`/`path` dependencies; no default registry. Cargo-like metadata: `keywords`,
    `categories` (a fixed list, `manifest::CATEGORIES`: 21 subjects, at most 5 a package; the owner
    chose a fixed list, with keywords for anything narrower), `repository`, `documentation`, `readme` (a declared file), and
    `about-books` (USFM codes of the books it's mainly about: the owner wants these said by the
    author, never derived from the text; `book_code` turns names into codes for `lx new --about`).
    Catalogs carry keywords, categories, about-books, and a dictionary's `keys`; `Catalog::search` matches them (a word
    naming a book matches what's about it) and `lx search --category` filters. `topos_config.rs` reads topos's `config.toml` book data
    (`data`/`merge-data`/`remove-data`, as the CLI and logoxide-lsp do) and `BookFilters`
    (topos's `BibleFilter`: testaments, genres, books, and exclusions) gives the codes kept;
    `lx search -t/--nt/--ot/-g/-b/--exclude-*` keeps packages with any about-book among them. A dependency is one of `git`,
    `registry`, `path`, or `import` (`{ import = "ebible:eng-asv" }`, `is_import_name`: a
    lowercase prefix, a colon, an id). A `registry` shaped like a name (`is_registry_name`: no
    `/`, `:`, or `.`) is a registry's name by `doc/manifest.md`'s rule, refused until names are
    resolved, so it's never fetched as a URL. A dependency can name another package
    (`kjv-bob = { package = "kjv", … }`, for forks) and give its own `abbreviation` and
    `aliases` (`Project::names`), and `enabled = false` turns it off without removing it (still
    locked and installed, so `lx clean` keeps it; `Project::set_enabled`, `is_enabled`, `dependencies_enabled`, and
    `in_use`: the lock's packages from dependencies that are on, which `bibles`, `translations`,
    and `versifications` read). `[package]` has `authors` (a list) and `description`. Kinds
    include `book` and `collection` (no `[formats]`, only dependencies; a project's Bibles
    include a collection's); the `epub` format is kept as files, not read as verses. Formats
    `gbf` (GBF with its `<SB>`/`<SC>`/`<SV>` sync marks, `.gbf` and `.txt`), `dictionary`
    (`entries/*.xml`, `<entry key=… headword=… link=…>` bodies in the markup the root declares,
    the owner's D10) and `xref` (`xref/*.tsv`: `from`, `to`, optional weight, OSIS references)
    are specified in `doc/entries.md`; `[package] keys` (`Keys`: `strongs-greek`,
    `strongs-hebrew`, `headword`) is required with a `dictionary` format and refused without
    one; a dictionary's kind is `lexicon` (no new kind), cross-references' `cross-references`.
    Keys it doesn't know are passed over, not refused (`Manifest::unknown_keys`, which lx warns of), so an
    older lx reads a newer manifest. `validate` refuses a `git`/`registry`/`tag`/`branch`/`rev`
    that starts with `-` (git would take it as an option), a `rev` that isn't a full commit id (`check_rev`: 40 or 64 hex digits), and names and
    `package =` that aren't ids (`is_valid_id`, `check_id`). A git dependency's `folder` names a
    package in a folder of its repository (tags `<folder>/v<version>`, `lx add --folder`)
  - `works.rs`: works and editions (the owner's D9): `[package] work = "kjv"`, `edition`,
    `edition-year`; `data/works.toml` is logoxide's table (each work's name, aliases, optional
    Wikidata QID, and the import names checked against the publishers' lists to be it: KJV,
    ASV, WEB, BSB, Douay-Rheims, LXX, Vulgate so far; no QIDs yet). `of_import`/`of_source`
    give an import its work (catalog entries and `import::finish`), never guessed from titles.
    Catalogs name works of their own (`[works.<id>]`, a folder registry's `works.toml`,
    `WORKS_FILE`: name, aliases, wikidata; `parse_declared` checks each (`check`) and passes over
    logoxide's ids and any `imports`, warned) into `Catalog::works` (`DeclaredWork`: the work and
    its catalog's name; `add_works` on merge, the first kept, a differing one warned of;
    `Catalog::rename` renames them with a registry); a work is a name, never owned: any package
    may claim any id. `Catalog::work(id)` (the table's, else the catalogs'),
    `works_listing(query)` (`WorkListing`: the table's, the catalogs', then ids only entries claim,
    `WorkFrom::Logoxide`/`Catalog`/`Unnamed`, each with its claiming entries; `lx works`,
    `LogoxideCatalog.works`); `lx catalog build` writes the folder's `works.toml` as `[works]`
    (else keeps the previous catalog's). `Catalog::search` matches a work's names;
    `search_grouped`/`Catalog::group_by_work` gather editions (`WorkGroup`, with its `name`); `Project::same_works` (a Bible without `work` takes that of another with its
    package id: forks inherit it) is what lx warns of after add, install, and import;
    `works::describe` for lx info and show. The FFI's `PackageInfo` and `CatalogEntry` carry
    `work`, `edition`, `editionYear`; `LogoxideCatalog.searchGrouped`, `works(query)` (`WorkInfo`, `WorkFrom`)
  - `language.rs`: `language` is checked as well-formed BCP 47 (`language-tags`, its grammar
    only), `script` as ISO 15924's form. `canonical` gives a tag's canonical form: the registry's
    replacements (`language-tags`' `canonicalize`, but a suppressed script is kept: `en-Latn`
    stays), then the shortest ISO 639 code (`ISO_639_1`, the 184 two-letter codes with their
    639-3/639-2T and 639-2B forms, made from SIL's `iso-639-3.tab`: `eng`→`en`, `ger`/`deu`→`de`,
    `hbs`→`sh`; `grc`, `hbo` stay); a manifest's tag that isn't canonical is warned of
    (`Manifest::warnings`, printed when lx opens a project and by `lx check`, JSON `warnings`),
    not refused; `search_codes` lets `Catalog::search` match `eng` to `en` and back; `Package::text_script` is
    `script`, else the tag's subtag, and `text_direction` is `direction`, else the script's
    (`RTL_SCRIPTS`), else left to right (`direction` is an `Option` in the manifest). Imports:
    `normalize_language` (SWORD's `en_US` is `en-US`, then canonical, so eBible.org's, fetch.bible's,
    CCEL's, and SWORD's codes are written canonical; what isn't a tag is `und`, the publisher's
    text kept in `language-name`), eBible.org's `script` column by name (`script_code`), and
    CCEL's script judged from its letters (`dominant_script`), which sets the direction. An
    import writes `direction` only where the script doesn't imply it
  - `license.rs`: `license` is checked as an SPDX expression (`spdx`, lax; `LicenseRef-*`);
    `[package.rights]` (`Rights`: `redistributable`, `derivatives` allowed/share-alike/none,
    `commercial`, `attribution`), with `derived_rights` for a single known license (CC, public
    domain, permissive, copyleft) and what's said winning (`Package::text_rights`,
    `CatalogEntry::text_rights`). `says_public_domain` refuses negations ("not public domain");
    `license_from_text` reads a Creative Commons link or name (eBible.org's `copr.htm`, which its
    import reads). `assess(info) -> Vec<Concern>` (moved from lx's publish.rs): no license, not
    free, not redistributable (`is_serious`), and `Converted` (`upstream.converted`, set by SWORD
    imports, when derivatives are "none": not a derivative to logoxide, the owner's D7, but
    said). Catalog entries carry `rights` (eBible.org's list: redistributable; fetch.bible's
    terms objects; SWORD's; `catalog.toml`'s `rights`, which `entry_table` writes)
  - `storage.rs`: `Storage` (read, list, write, remove) over a folder (`FsStorage`), memory
    (`MemoryStorage`), or a host's files in WebAssembly, so nothing else touches `std::fs`.
    `FsStorage` takes only plain relative paths (no `..`, `:`, or NUL) and writes through
    `write_atomic` (a unique temporary file beside the target, synced, renamed over it, the folder
    synced: whole or not at all; also for the manifest, the lock, `catalog.toml`, and what
    `lx publish` writes); `stamp(path)` (size and mtime) and `location()` (the canonical root)
    are `FsStorage`'s, `None` elsewhere; `FsStorage::no_symlinks()` never goes through a link under its root
    (`symlink_metadata` part by part: refused to read, write, or remove, left out of listings),
    used by `lx release` and `lx publish` (so the summary is of what the commit packs) and by
    `lx import --update` (`apply_import`)
  - `package.rs`: opening a package picks the best readable format its enabled features turn on
    (`Format::text_format`, opened by logoxide-text's `source::open` under `read`); a package
    that isn't a Bible (a collection) may have none and opens with `NoText` (no books, so no
    verses in its summary), and a book (`kind = "book"`) always does, so its ThML isn't read as
    verses. `checksum()` hashes every file but `.git`
  - `cache.rs`: verse counts, parsed books, and an EPUB package's references (postcard; the
    counts and references also per logoxide version) per `<id>/<version>-<checksum>-<format>`
    (a package read as USX and then USFM shares nothing). The books are logoxide-text's
    `cache::BookCache` (C6, the owner's N2) keyed `<checksum>-<format>`, over the version's
    `books/` folder (`VersionBooks`, under `read`: a `BookStore` over the `Storage` that drops
    the key, since the folder's name carries it, so nothing moved on disk);
    `reader::StorageBooks` is the plain adapter for other callers. `Cache::references`
    without the counts (`lx show`); `CachedPackage::add_to` gives topos's `Translations` the name,
    aliases, and counts. `Library::book_cache` adds `with_stamps(<cache>/stamps)`, so `Cache::open`
    takes a package's checksum from `stamps.rs` instead of hashing every file each time
  - `stamps.rs` (crate-private): checksums remembered by each declared file's `Storage::stamp` (size and
    modification time; `None` for memory and a host's files, so they're always hashed) in
    `<cache>/stamps/<id>/<version>-<hash of Storage::location>.postcard`, used only when the
    paths and every stamp match and each file is at least `RACY` (2 s) older than the stamp file
    (git's rule for racy files); `lx verify`, `--repair`, `lx release`, and `lx info`'s checksum
    always hash
  - `lock.rs`: `logoxide.lock` (each package's version, commit, checksum, features, and
    dependencies; `dependencies` at the top are what the manifest asks for, each named
    `id version+g<commit>`, or `rename (id) version+g<commit>` when renamed, so two names for one
    commit are two entries sharing a folder; a name in another form is refused by `parse`, which
    says to delete the lock and install again; `rename`
    is the manifest's name when it isn't the id; `get` finds by that name; `LockedPackage::new`
    fills in nothing; an import's entry has `import`, `upstream-version`, `[[package.download]]`s
    (`LockedDownload`: name, url, optional sha256) and a required checksum, its key
    `<version>+i<8 hex of the checksum>`). `moved_since` finds
    releases that came back changed (a moved tag), which `lx install`/`lx update` warn of; a kept
    commit that can't be fetched says it may be gone from the host. `LockedPackage::check`
    refuses a commit that isn't a whole object id (40 or 64 hex digits), a sha256 that isn't 64,
    and an id that isn't one, so a lock can't name a folder outside the library
  - `git.rs`: `Fetcher` (a repository's tags; its files at a commit, into any `Storage`).
    `GitCli` runs `git`. `expand_url`: `github:owner/repo` (also `gitlab:`, `codeberg:`) to its
    URL, wherever a URL is taken; manifests and locks keep the full URL. `GitCli` refuses
    arguments starting with `-`, puts `--` before repositories, runs with `protocol.ext.allow=never`
    and `GIT_PROTOCOL_FROM_USER=0`, and never follows a symlink in a checkout. Every git command
    has a deadline (`run`: spawned, waited on, killed): `fetch_timeout` (`FETCH_TIMEOUT`, 10
    minutes) and `list_timeout` for `ls-remote` (`LIST_TIMEOUT`, 60 s), set from the config's
    `git-timeout`/`git-list-timeout` (`Config::git`); the error names the limit. `core.fsmonitor`
    is off for every git command (also `publish::registry_build::git_in`); fetching (`Mode::Fetching`, in
    a new repository, `in_clone`) also turns off LFS's filters (`GIT_LFS_SKIP_SMUDGE`, pointers
    kept), points `core.hooksPath` at an empty folder, and runs SSH with `BatchMode=yes` unless
    the user chose their own (`GIT_SSH_COMMAND`, `GIT_SSH`, `core.sshCommand`). Fetched packages'
    dependencies can't use `git://` (resolve.rs); lx warns of the project's own. `is_insecure`
    (`http://`, `git://`, `ftp://`: warned of) and `is_local`. `git_available()` (`git --version`
    answers, asked once) is how logoxide-app's `fetch::fetcher` picks `git` or HTTP. `Fetcher`
    is `Send + Sync` (the FFI's rounds hold mutexes), so logoxide-app boxes any
  - `net/archive.rs` (`net::archive`): git over plain HTTP for hosts without `git`: `Refs::parse` reads the smart-HTTP
    ref listing (`refs_url`), `archive_urls` gives each host's `.tar.gz` form, `unpack` drops the
    archive's top folder. An archive hashes the same as a checkout (checked on the KJV, the BSB,
    and GitHub's own archives). Unpacking (and `import::unzip`) stops at `MAX_UNPACKED_BYTES`
    (1 GiB), `MAX_ENTRY_BYTES`, and `MAX_ENTRIES`; `unpack` reads the archive twice, first its
    names and declared sizes (the top folder, the limits, before anything is written), then
    writing each file to storage as it's read; `Refs::find_commit` takes a revision of at least
    `MIN_REV_PREFIX` hex digits naming one commit
  - `catalog.rs`: catalogs to search: `Catalog::parse` reads a `catalog.toml` of git packages
    and registry releases; other publishers' lists (eBible.org's `translations.csv`,
    fetch.bible's `manifest.json`, CCEL's page of its Bibles, a SWORD repository's
    `mods.d.tar.gz`) are read by `crates/logoxide-import` into the same `Catalog`, each entry a
    `CatalogSource::External` (phase D, October 2026): its `import` name (`ebible:eng-kjv2006`,
    `sword:crosswire/KJV`: a prefix naming the publisher, a colon, the publisher's id), the
    `upstream` URL the import is made from, `tracks_versions` (whether a newer download always
    has a newer `version`: eBible.org's dates and SWORD's `Version`s do), and `downloads` (each
    file the import needs, by name, with URLs to try in turn), so the package manager knows no
    publisher, only what an entry says of itself (`prefix`, `import_id`, `import_name`,
    `upstream_url`, `downloads`, `versions_track_updates`). Ids are prefixed
    (`ebible-eng-kjv2006`, `fetch-eng_kjv`, `ccel-kjv`, `sword-kjv`, `sword-<repo>-<module>` outside CrossWire's main one), since the
    first of an id wins when catalogs merge. An entry follows the manifest's rules by kind
    (`abbreviation` for a Bible or a commentary, `language` unless a collection or a map, else
    `und`): one that lacks what its kind needs is passed over, warned of. `search` ranks exact
    abbreviations (`CatalogEntry::short_name`: else the id) and ids first,
    and folds case, every combining mark (NFD: accents, breathings, Hebrew points), and final
    sigma away (`fold`, with `unicode-normalization`), so `ιησους` finds `Ἰησοῦς`; a vault's
    `[settings] catalogs`, then those five (`DEFAULT_CATALOGS`: eBible.org, fetch.bible,
    CCEL's Bibles and its commentaries' genre index, CrossWire; the URLs are the core's, what a manifest means by a default) unless
    `default-catalogs = false`. `Catalog::find_import` finds `ebible:<id>` (or
    `sword:crosswire/KJV`) in a catalog, `find_upstream` an imported package's entry by its
    `[package.upstream]` `import`, then `url`; `known_works` gives imports the works
    `works.toml` knows their names to be; `lx import` looks through the
    project's catalogs in order (and for `sword:<known repository>/…`, that repository's). `Project::add_catalog`/`remove_catalog` edit them (removing a
    default turns the defaults off and names the others; naming them all again turns them back on). Registries (`[settings.registries]`, `Project::add_registry`) are named catalogs: a folder
    of package repositories (`Catalog::from_packages`; lx scans it) or a catalog, so
    `dorean-local:<id>` or `dorean-local:<path in the folder>` names a package (`Catalog::find`,
    `split_registry_name`); a shared id needs the path. A `catalog.toml` may say `format = 1`
    (`CATALOG_FORMAT`; a higher one asks for a newer lx); a remote catalog can't list a local
    path or `file://` URL; an entry whose id isn't one (`check_id`) is passed over, said in
    `Catalog::warnings` (lx prints them as it loads a catalog; `LogoxideCatalog.warnings()`);
    `Catalog::shadowed` gives ids more than one catalog lists (lx search and lx import warn,
    `warn_shadowed`; lx add through a registry warns when another registry lists the id,
    `warn_other_registries`). `publish/registry_build.rs` (`publish::registry_build`): folder registries (`folder_catalog`, `build_catalog`,
    `newest_release`, read with `git show`)
  - `media.rs`: `[media]` in a package's `logoxide.toml`: each audio or video file's path, URL,
    `mirrors` (tried in order after it: `MediaFile::urls`), size, SHA-256, and book, never
    stored in the package (its `sha256` is `sha256:` and 64 hex digits, as every hash is written;
    the bare digits are read too: `sha256_digits`, `sha256_matches`); `verify` checks a download
    (`LogoxideError::Mismatch` when it isn't the one listed); `Project::add_media` lists one,
    `store_path` keeps it by hash (`media/9f/9f86…`). Summaries count media; releases are judged
    by the text. Timings (audio Bibles: where each verse is) are WebVTT, a file's own `timings`, else
    its path in the timings folder with `.vtt` (`audio/JHN.mp3` → `timings/audio/JHN.vtt`, so
    audio and video of a book don't collide: `timings_path`), each cue named by its verse
    (`JHN 3:16`, `JHN 3:17-18`; other cues are skipped): `parse_timings`, `timings`, and
    `check_package_timings` (verses the text lacks, checked against each chapter's verse numbers,
    so verse 0 is one where the text has it; another book, out of order, overlapping, missing
    files, or unlisted ones anywhere under the folder), which `lx check`, `lx media check`, and
    `lx release` run. topos's subtitle parser isn't used: it only takes numbered cues. Audio
    only: `[media] text = "<dependency>"` and/or `versification = "KJV"` (`Media::names_its_text`)
    let a package have no `[formats]` (it opens with `NoText` even as a Bible, and isn't one of
    a project's Bibles); `check_package_timings_with(package, text)` checks against the text
    `Project::media_text` opens (lx check and media check; not installed: warned, verses
    unchecked), else its own, else the versification's counts from SWORD's systems
    (logoxide-text's `versification::system`; release checks without the dependency). The FFI's `LogoxidePackage.mediaText()`,
    `mediaVersification()`
  - `library.rs`: installed versions in `packages/<id>/<version>+g<commit>` (so forks at one
    version never share a folder), without `.git`; path packages stay where they are.
    `Library::packages(base)` is the resolver's `Packages` on disk (a new version is written
    beside its folder in `.<key>.staging-<pid>-<n>-<random>`, then renamed into place; a repair
    writes each file atomically and removes what the version no longer has last). `Library::lock`
    is the library's lock (`lx clean` holds it; `stale_staging(age)` lists staging folders an
    install left, which `lx clean --remove` deletes only under the lock and only after a day)
  - `roots.rs`: `<library>/roots/<hash of host and canonical path>.toml` (`Root`: `path`, `host`,
    `updated`, `packages` as `<id> <key>`), written on every install (`Library::record_root`), so
    `lx clean` keeps the union of every root's packages (`Library::roots`; a vault elsewhere, or on
    another machine sharing the library, keeps its own); `Root::is_gone` (this host, its manifest
    gone) is reported, `lx clean --prune-roots` removes it; one that can't be read stops a clean.
    `doc/lock.md` has the format
  - `locking.rs`: exclusive advisory locks (`std::fs::File::try_lock`, polled up to
    `LOCK_TIMEOUT`, 5 s, else `Busy` naming the holder, which the lock file records) on the
    library (install, repair, clean) and a project (manifest and lock writes); lock files in
    `$XDG_RUNTIME_DIR/logoxide/<hash of the canonical path>.lock`, else `<default cache>/locks/`,
    never in the library (Dropbox syncs it); `lock_all` takes several in one order, one folder
    once; readers take none; a no-op in WebAssembly
  - `resolve.rs`: manifest + previous lock → new lock, installing as it goes (`Frozen`: `locked`
    keeps the lock as it is, `offline` fetches nothing; `Project::install_frozen`); an `import`
    dependency (`import_release`) is asked of the fetcher (`Fetcher::import`, whose default says
    it can't: the core knows no publisher) as an `ImportRequest`: `ImportKind::Pinned(locked)`
    when locked but not in the library (made again, given the locked version, checked against
    the locked checksum, else an error saying `lx update <name>`), `Fresh { current }` when new
    or updating (versioned against `current`); kept in the library by checksum, through `Packages`
    (where installed versions and path packages are) and a `Fetcher` (git, and `download` for
    registries: paths and `file://` by default, HTTPS in lx and through the host in the
    bindings). A `registry` dependency (a catalog's URL) gets the newest release its catalog lists
    that the requirement allows, or the locked one (whose archive URL and hash the lock has, so
    no catalog is read); the archive must hash to `sha256` and its files to `checksum` (a release without one is refused); lock
    entries have `registry`, `archive`, `mirrors` (the release's, tried in turn by
    `Fetcher::download_any`, which passes over bytes that don't hash as listed), `sha256`, and
    its library folder is `<version>+a<hash>`. A lock entry is kept
    while the manifest still allows it (unless updating), found first among what the same asker
    locked (`Resolver::kept`: the manifest's top-level `dependencies`, or its parent's); fetched text is checked against the
    locked checksum; a tag's version must match its `logoxide.toml`. A package from another
    machine (git or a registry) can't name a path, a local or `file://` repository, or a local
    registry; a local one can (`folder_tag_version` reads a `folder`'s tags)
  - `project.rs`: a folder with `logoxide.toml` (edited with toml_edit, keeping comments; or
    `from_text` and `manifest_text` for hosts that do the reading and writing) and its
    lock; `save_manifest` keeps the text it read and refuses with `Conflict` when the file
    changed since (open again, redo the change); `install` holds the library's and the project's
    locks (`locking.rs`, below) from resolving through writing the lock, `save_manifest` the
    project's (`take_lock`); `translations()` gives topos the Bibles it asks for; `names` gives a dependency's
    `abbreviation`/`aliases` to the manifest's own copy only, and `name_clashes` says which names two
    Bibles answer to (a diamond: the manifest's kjv 2 and a collection's kjv 1), which `lx install`
    and `lx add` warn of; `packages_of(library, kind)` opens the packages of a kind among
    `in_use`, with `commentaries()`, `dictionaries()` (kind `lexicon`), and `cross_references()`
  - `summary.rs`: `logoxide.summary.toml` (verse counts in book order, sizes, element counts
    without structural markers; `verse-hash = 1`) and `logoxide.verses.bin` (`LXV2`: a header
    of the hash rule (u16), the record length (12; readers skip bytes past what they know), and a
    reserved byte, then a record a verse: book (`books.rs`'s `Book`: logoxide's own fixed numbers,
    matched to topos's `BookId` by USFM code; 1–66 as topos's, 67–90 topos 0.6's, 91–103 the
    other USFM books, 200–214 peripherals, a snapshot test so none changes), chapter, verse,
    last, part (`20a` is 1; `VerseHash::key` includes it), 7 bytes of SHA-256 of its words with
    notes and headings left out). `HashRule` (the owner's D3): rule 1 is topos's `plain_text`,
    NFC, U+200B/FEFF/00AD/2060 dropped (ZWJ/ZWNJ kept), Unicode whitespace collapsed; a rule never
    changes (`tests/verse_hash.rs` pins topos's text on USFM, USX, and OSIS: if a topos upgrade
    breaks it, add rule 2). `Summary::read` checks the summary and the `.bin` agree and refuses a
    newer rule (`LogoxideError::Newer`, which `lx verify` passes on); `release::summary_in`
    builds an older rule's side (or one whose summary files can't be read) again from its tag's
    text, so a rule change is never rewording; `files-checksum` and `verse-hash` are required. `diff` (removed, renumbered: old text at another number,
    joined: a number now inside another verse's bridge, or parts made whole, split: a bridge
    narrowed with each number still there, or a verse cut into parts (minor), bridged: one that
    lost a number, reworded, added; ranges are compared before hashes, the owner's D2) and
    `Change` (patch/minor/major; before 1.0 the minor counts as major, and before 0.1 every change
    is, like Cargo). Text counts as moved only to a number that held other text (a psalm's
    repeated refrain reworded is a rewording), and empty verses never move.
    `book_hashes` hashes one book's verses the same way (`LogoxidePackage.verseHashes`: part,
    and 14 hex digits), which hosts put in links (`&ver=…&vh=…`, the first 8 digits) to find a
    verse again after an update.
    `files-checksum` hashes the formats' files alone, so `diff_summaries` sees a book (no
    verses) change: a minor change. The passages a book cites go in `logoxide.references.bin`
    (the summary counts its references). Dictionaries and cross-references publish
    `logoxide.entries.bin` (`LXE1`: `LXV2`'s header with its own magic and a record length of
    15, then a record an entry: 8 bytes of the key's hash, `HASH_BYTES` of its text's by the
    verse rule; `EntryHash`, `encode_entries`/`decode_entries`; the summary counts `entries`,
    and `Summary::read` checks the count and the rule agree), carried in `Summary::entry_hashes`
    (`serde(skip)`, like `cited`, so `read`, `write`, and `summarize` keep their shapes);
    `diff_entries` (an entry removed is major, reworded or added minor: `Diff::entries_removed`,
    `entries_changed`, `entries_added`, which `release::describe` names) runs in `diff_summaries`,
    and `files_changed` only counts when neither side has verses or entries
  - `references.rs`: the Bible references in a package's books (`book_files`: its EPUBs, as
    `epub_files` gives them, and a book's ThML files; the cache and titles use it), found by
    logoxide-text's `citations` (topos-formats' `search_epub_reader`, pure Rust, so WebAssembly
    too; ThML through logoxide-markup and topos's matcher). `find`
    (`read`) reads the books: each reference's OSIS passage, file, CFI, chapter, UTF-16 range,
    line, column, spine item, and paragraph (about 0.1 s a book natively, 0.25 s in
    WebAssembly). Only `cited` is published
    (`LXR1`: deflate over postcard of each passage and its count, in Bible order; 2.6 KB for 937
    references; inflated to at most `MAX_REFERENCES_BYTES`): locations are only of use with the
    book, which gives them again
  - `package.rs` also says what a package is made of: `declared_files` (the manifest, each
    format's files by its extensions, the license file, summary files, timings, `include`
    globs, matched without backtracking by the crate-private `glob`); `checksum` hashes only those, so a reader's annotations beside a book never change
    it. `other_files` are kept by repairs and stop `lx clean`
  - `release.rs`: summarize, compare with the newest release tag's summary (or its text, for
    releases before summaries), check the version, write, commit, tag `v{version}`. A package in
    a folder of its repository (`GitCli::package_in`) has its own tags, `<folder>/v{version}`,
    and its commits take only its folder (nothing new: the current commit is tagged)
  - `titles.rs` (`read`): each file's title (a Bible book's, an EPUB's, a book's ThML): `[titles]`
    in the manifest, else the file's own as logoxide-text's `titles` reads it (`\toc2`, `\h`,
    `\toc1`; `dc:title`; `DC.Title`, `thml_title`), else the book's English name (a file of many books: the package's name).
    Summaries list them (`titles`); a changed title is a minor
    change (`Diff::retitled`, the owner's choice); `lx check` and `lx release` refuse an untitled file. Hosts name
    what they show by it (`BookFile.title`, `LogoxidePackage.titles()`)
  - `text.rs`: `printable` drops control characters (but `\n`, `\t`), whole ANSI/OSC escape
    sequences (7-bit and C1), and bidi controls (U+202A–202E, U+2066–2069) from text bound for a
    terminal
  - `publish.rs`: release archives for registries: `pack` (declared files, sorted, no times or
    owners, so the same files make the same bytes), `sha256`, `add_release` (a catalog.toml's
    `[[package.release]]`: version, url relative to the catalog, optional `mirrors`, sha256, checksum, size; a
    published version never changes), `yank` (`yanked = true`: `CatalogEntry::release`
    passes over it; locks still install it). `CatalogSource::Registry { catalog }` and
    `CatalogEntry::releases`/`release(req)`; `relative_url`
  - Versification maps (`kind = "versification-map"`: `[package] versification` to `maps-to`, a
    `vrs` format of Paratext `.vrs` mapping lines, so `eng.vrs` reads as it is; verse 0 is a
    psalm's title) are read by logoxide-text's `versification` (`reader::versification_map`
    gathers a package's `.vrs` files). Unnamed verses keep their number unless another
    took it (then no counterpart); `Versifications::route` follows maps both ways and through
    each other (names ignore case); `Project::numbering` (`read`) turns a Bible's name into its
    versification and counts, `Project::map_reference` maps. Partial verses (`ESG 1:1a`) map as
    whole verses, the part kept in `VerseRange` (`first_part`, `last_part`, its `Display`), and
    lines naming parts overlap nothing; excluded lines (`-PSA 9:21`, `excluded()`) map to `None`
    both ways (so `Mapped::missing`); count lines (`counts()`) let `problems()` say which verses
    don't exist, only said (`lx check` warns), since Paratext's own files have some (the BSB's
    ESG 8:36-41); mapping lines naming a book topos doesn't know are passed over
    (`passed_over()`, warned of). Several lines may map onto one target verse (Paratext's data
    is authoritative: the BSB's lines onto Greek Esther 5:2); mapped back, it's every verse
    mapped onto it (Original ESG 5:2 is the BSB's 5:11-16). A verse on the left of overlapping
    lines (not each naming it alone) and uneven ranges stay errors (`mistakes`, and
    `Mapping::pairs` returns one). The summary counts `mappings`; any change is minor. Sample
    package: `crates/logoxide/tests/fixtures/english-original` (lines from Paratext's eng.vrs).
    The BSB's DBL `versification.vrs` (`~/Downloads/bsb-usx/…/release/`) loads whole (2424
    verses; its `S3Y = DAG` lines map), with two lines past its counts warned of
- `crates/logoxide-import`: packages from other publishers' downloads (October 2026; logoxide's `import.rs`, its catalog parsers, and `license_from_text` until
  then). Above the core, as lx and the bindings are: it depends on `logoxide` with its `read`
  feature (an import is checked by reading it), logoxide-text, and sword-modules, which the
  core no longer does. `catalog::parse(location, bytes)` (and `parse_text`) reads any
  publisher's list by its form (`.csv` eBible.org's, `.json` fetch.bible's, `ccel.org/ccel/`
  CCEL's page of Bibles, `ccel.org/index/genre/gen-XXX` a CCEL genre index, `.tar.gz` a SWORD
  repository's; else the core's `Catalog::parse`), each entry an
  `External` source made by `external(prefix, id, tracks_versions, downloads)`;
  `is_publishers`, `publisher_name`. A module per publisher, each with `catalog` and `import`:
  `ebible` (`translations.csv`'s downloadable, redistributable rows; `<id>_usfm.zip` into
  `logoxide.toml` + `usfm/` + `copr.htm`, the license file read for a Creative Commons license
  or the public domain; its upstream version eBible.org's update date), `fetch` (`manifest.json`,
  `https://v1.fetch.bible/manifest.json`, not `/bibles/manifest.json`, an older form; each
  book's USX, a download named by its lowercase code (`book_url`; other paths answer with
  fetch.bible's site, refused) into `usx/` + `COPYRIGHT.txt`), `ccel` (`catalog`: each `<h2>` link's Bible
  and title, `ccel:<id>`; `genre_catalog(text, genre)`: a genre index's `<p class="List2">`
  links `/ccel/<author>/<work>.html`, `ccel:<author>/<work>` (a Bible's still `ccel:<id>`), ids
  `ccel-<author>-<work>`, the kind by `genre_kind` (`gen-COM` commentaries, `gen-BIB` Bibles,
  else books), the author's name the description, other links passed over; `gen-COM` is a
  default catalog (the owner's, 2026-10-10); `named_catalog(import)`: the one work a
  `ccel:<author>/<work>` names, which lx uses when no catalog lists it, a book until its head
  says; a work's ThML (`url`; restricted works answer with a page, refused) into
  `thml/<work>.xml`, named, dated, licensed, and kinded by its `<ThML.head>` (`thml_head`;
  `DC.Type` `Text.Bible`/`Text.Commentary`/else a book, `ThmlHead::kind`, wins over the entry's;
  upstream version `<version>`, else `DC.Date`; its script, and so its direction, judged from
  its letters; a commentary that's a scan, no markers, fails as a Bible with no books does), and `sword` (`REPOSITORIES` names the HTTPS repositories in CrossWire's list,
  `crosswire`, `crosswire-beta`, `ebible`, …; others by host, `repository_name`;
  `catalog_url(name)`; `mods.d.tar.gz`'s Bibles and commentaries in OSIS, ThML, or GBF and its
  dictionaries (RawLD, RawLD4, zLD, in any markup; listed as `kind = "lexicon"`) that aren't
  locked; `zip_urls`: `packages/<Module>.zip`, then lowercase; the zip into sword-modules'
  `MemoryFiles`, each book written in the module's markup (`osis/John.xml` with verse
  milestones, CCEL-style ThML, or `gbf/John.gbf` (N4: `<SBJohn>`, `<SCn>`, `<SVn>`, a bridge
  `<SV16><SV17>`; a run into the next chapter has its text at its first verse and the next
  chapter's verses as an empty bridge ended by `<CM>`)), dropping the entries' own book and
  chapter milestones or sync marks, making linked verses bridges from sword-modules'
  `iter_spans` (`n="16-17"`, or one id into the next chapter, `osisID="John.3.36-John.4.2"`,
  which logoxide-markup reads as one span to `last_chapter`), and leaving out introductions
  stored at an earlier verse's entry (`EntryLocation`); a dictionary (`Module::Dictionary`) is
  `entries/<module>.xml` in `doc/entries.md`'s form (`entries_file`: the root's `markup` is the
  conf's `SourceType`; a GBF or plain entry's body inside CDATA sections, `]]>` in it split
  into two, so the file is well-formed XML; `keys` from `Feature=GreekDef`/`HebrewDef`, else headword
  (`dictionary_keys`); stored keys in normal form, `00025` → `G25` (`import_key`); a TEI entry's
  first `<orth>` as its headword; `@LINK` aliases as `link` entries), `Made.keys` writes the
  manifest's `keys`, and `finish` refuses one with no entries as it refuses a Bible with no
  books (a book, `kind = "book"`, needs neither: CCEL's books, `DC.Type` `Text.Monograph`, are
  ThML whose references are found); the repository root is read back from the entry's first download URL; the
  conf's `Versification` is kept (`sword::versification`: SWORD's names as SWORD spells them,
  `KJV` when absent; other imports say `own`); the conf gives version, license
  (`sword::license`) and rights (`sword::rights`: permission given to CrossWire or only in
  SWORD's format is `redistributable = false`, `derivatives = "none"`; "free non-commercial
  distribution" is redistributable without derivatives, since the statement grants nothing
  more, which keeps `license::assess`'s `Converted` concern), language, and `COPYRIGHT.txt`).
  `door43` (`CATALOG`, the catalog API's `search?subject=Bible&stage=prod`, recognized by
  `is_catalog`, `door43.org/api/v1/catalog`; its JSON `{"ok", "data": [{owner, name, title,
  language, language_title, language_direction, subject, branch_or_tag_name, release.tag_name,
  tarbar_url, …}]}` read loosely, as remembered, since the API never answered; `subject`
  "Bible" only; `door43:<owner>/<repo>`, ids `door43-<owner>-<repo>`, the version the release's
  tag, `tracks_versions`; the download `archive` at `archive_url(repo, tag)`, then
  `tarbar_url`; the archive unpacked by the core's `net::archive::unpack` into a `MemoryStorage`,
  `manifest.yaml` (serde_yaml 0.9.34, deprecated but frozen) required, its `dublin_core` for
  the title, description, language and direction, license (`rights`, `CC BY-SA 4.0` as SPDX), and
  upstream version (`version`, else the tag); every `.usfm` into `usfm/`, `LICENSE.md` the
  license file when present; versification `own`).
  `burrito` (R-I, 2026-10-19: Scripture Burrito 1.x and DBL text release bundles; no catalog:
  `burrito:<location>` and `dbl:<location>`, `https://`, `http://`, or `file://` (`%`-escaped,
  import names have no spaces); `entry(import)` and `named_catalog` from the name alone, the
  download `bundle`, its id and name the location's last part, `tracks_versions` false, a
  `file://` one with no `upstream` URL; `bundle_entry(location, files)` the entry named by the
  metadata, `dbl:` when it has `metadata.xml`; the files a `.zip` or `.tar.gz` (`bundle`) or a
  folder's (`files/<path>`); the root the shallowest folder with `metadata.json`, else
  `metadata.xml`; `metadata.json` read as `serde_json::Value`, loosely: `meta.version` 1.x,
  category `source`/`derived`, flavor `scripture/textTranslation` only, `identification` (name,
  abbreviation, description, localized by `meta.defaultLocale`, else `en`, else the first;
  `primary`'s first authority, id, and `revision`), `languages[0]`, `copyright` (`publicDomain`,
  `licenses[].url` through `license_from_text`, `licenses[].ingredient` the license file,
  `shortStatements` without tags into `COPYRIGHT.txt`), the `rightsHolder` agency else
  `publication` (`Made.publisher_name`, its `url` the homepage); `metadata.xml` with roxmltree:
  `type/medium` `text` only, `identification`, `language` (`ldml`, else `iso`), the rights
  holder, `copyright`'s `statementContent`, `manifest/resource`, and the default publication's
  `structure` (its books, so the other publications' and `source/` files stay out);
  ingredients' text by extension and MIME type (`usx`, `usfm`/`sfm`/`text/x-usfm`, `usj` or JSON
  starting `"type":"USJ"`), the first of USX, USFM, USJ it has into `<format>/<file name>` (two of
  one name an error), each checked by `size` and `md5` (md-5), a file whose line endings alone
  differ taken as it is (the spec's own example burrito has git's LFs for Paratext's CRLFs), a
  path with `..`, `\`, or a leading `/` never read, one listed but missing an error, else at
  `ingredients/<path>` (Proskomma's); `described` sets every field from the metadata whatever the
  entry said, so a pinned build from `entry(import)` is the bytes `bundle_entry`'s made; the id
  the abbreviation's (`package_id`), the upstream version the revision, `Made.note` the
  bundle's identity (`Scripture Burrito ptx b01d…, revision c772… (Paratext 9.1.0.0)`)).
  `Made` also has `publisher_name` (the package's publisher when the download names it; else
  `publisher`, which is also the header's "Imported from") and `note`; an empty `homepage` is
  none. `named_catalog(import)` is CCEL's or a bundle's.
  `license.rs`: `license_from_text`, `says_public_domain` (refuses negations). A vault uses any
  import as a path package in `imports/<id>/`. Versions (the owner's D1): a new import is
  `IMPORT_VERSION` (1.0.0), logoxide's own; the publisher's version as it gives it
  (`2026-10-08`, `3.1`) goes in `[package.upstream] version` (`manifest::Upstream`: `url`,
  `import` like `ebible:eng-asv`, `version`, `converted` for SWORD, and `note`, words for a
  hand-made package's source with no URL, which then has none), which `upstream(entry)` writes
  and catalog entries' `version` holds for imports. `downloads(entry)` and
  `build(entry, files, dest)` (dispatching on the prefix) are the one way every host imports
  (logoxide-app's `import`, `Logoxide.importDownloads`/`importDownloaded`); `PREFIXES` is what
  `lx import` accepts. `update.rs`: `compare_import(current, fresh, reader)` (compared at the
  current version: `None` when the declared files match; else the summaries' `Diff`, and the
  version raised by `next_version`; `ImportUpdate` has the publisher's
  `upstream_from`/`upstream_to`), `apply_import` (removes declared files the fresh one lacks,
  through `Storage::remove`, writes the rest; other files stay), `import_removals`,
  `import_is_current(entry, upstream.version)` (compares the publisher's versions,
  `compare_publisher_versions`, never the package's, and only where `tracks_versions`, so `lx`
  can skip downloading)
- `crates/logoxide-text`: what's in a Bible or a book, knowing no packages (October
  2026; logoxide's `source/`, `versification.rs`, and the parsing halves of `summary`,
  `titles`, `media`, `references`, `create`, and `language` until then). Files come through its
  read-only `Files` (`MemoryFiles`; the core's `reader::StorageFiles` wraps a `Storage`); errors
  are `TextError`. `source/`: one `Source` per `TextFormat` (`source::open`), each giving
  logoxide-markup's tree (`Book`: nodes and verse spans); `TextFormat::detect(name, head)` names
  a file's format from its extension, else from how it starts (`\id`, `<usx`, `<osis`, `<ThML`,
  `"type": "USJ"`: lx convert's and the FFI's `detectFormat`). USX (`.usx`, or `.xml` as some
  publishers name it), and USFM through `usfm3` (chosen
  over usfm-onion, whose USX drops the space between adjacent `\w` words; both gave the KJV's
  31,102 verses). The other way, `usx_to_usfm` (`to_usfm.rs`) reads USX into usfm3's document and has
  usfm3 write it: whitespace runs one space, end milestones left out, a character style inside
  another `\+`, a note's `category` a `\cat`. Checked by round trip (`tests/verse_hash.rs`): the
  pinned fixture, and (ignored, on the real packages) the KJV through USX and back and the BSB,
  whose only differences are Psalm 119's 21 acrostic headings (USX's reader gives one after a
  verse's end to the next verse, USFM's to the verse before it). USJ (`usj.rs`) is USX as JSON,
  so it's read by writing it as USX (`usj_to_usx`: each verse's and chapter's end milestone
  where the next starts, a missing `sid` made from the book and numbers; usfm3's
  `attributes: [{key, value}]` read too) and written from USX by our own `usx_to_usj` (usfm3's
  USJ writer nests attributes that way, not as the node's keys); end milestones are left out.
  `source::convert(text, from, to)` goes between USFM, USX, and USJ through USX (`lx convert`).
  Checked the same way: the BSB through USJ differs from its USX only in Psalm 119's headings
  (as through USFM), and back into USX and USJ again is the same USJ; the KJV's USFM through USJ
  is the same verses. OSIS and ThML Bibles through logoxide-markup (`whole.rs`: a file can hold every
  book; it's scanned for the books its verse markers name, cut into books, and its last parse
  kept). ThML's empty `<scripture parsed=…/>` elements (CCEL's Bibles) and `<scripCom parsed=…/>` (CCEL's commentaries) become verse markers; a chapter's own (`|Gen|1|0|0|0`) is verse 0 (chapter introductions are notes on it), and a `div1`–`div6` whose first marker is in another chapter ends the verse before it (an End milestone naming nothing), or starts that chapter's verse 0 when its first marker is one (`mark_verses`).
  GBF (`gbf.rs`, decision N4) the same way: its `<SB…>` marks name a file's books (OSIS id, USFM
  code, English name, or GBF's numbers 1–39 and 64–90), `<SC…>`/`<SV…>` become chapter and verse
  markers (a mark without a number is the last plus one; `<SV16><SV17>` with nothing between is
  one bridge), each book wrapped in a division. Preference: USX, USFM, USJ, OSIS, ThML, GBF. `loose.rs`: a Bible from loose files with no manifest (`open_loose`, `LooseSource`; the FFI's `Logoxide.openBible` and `LogoxideBible`): each file's format by `TextFormat::detect`, `.zip`s opened (`__MACOSX` and dotfiles left out), each format's files copied into a `MemoryFiles` folder of its own under the extension its source reads and opened as that format, and each book read from its best format; files of no format (`SkipReason::NotText`) and books another file has (`SkipReason::Duplicate`, a note, not a warning) passed over and said, by the paths they were given as (a zip's file as `kjv.zip/usfm/JHN.usfm`). A
  commentary is any verse source: `Source::notes(passage)` (a default on the trait over `book`
  and `passage_ranges`) gives each span covering the passage as a `Note` with its tree;
  `source::book_notes(book, passage, stretch)` is it over a parsed book (the LSP, lx, the MCP,
  and the FFI call it), `stretch` also taking the span before each of the passage's ranges in
  its chapter when none covers the range's first verse (a note over the verses after it up to
  the next: the owner's 2026-10-10 call, a reader's setting, off by default). Two
  formats hold no verses and open as sources with no books, so `open` returns one `Box<dyn
  Source>` for any format: `dictionary.rs` (`TextFormat::Dictionary`, `entries/*.xml` as
  `doc/entries.md` specifies: the root's `markup` names the bodies' markup; a body ends at the
  first `</entry>` outside a CDATA section (`entry_end`), and one that is CDATA sections alone
  reads as their text joined (`cdata_text`), so GBF and plain bodies sit in well-formed XML;
  files indexed once, bodies parsed on `Source::entry(key)`; `keys()` in file order; an `<entry link=…/>` alias
  reads as its target, one step) and `xref.rs` (`TextFormat::Xref`, `xref/*.tsv` lines parsed
  by topos's `parse_osis`; `Source::cross_references(verse)` is every line whose `from` covers
  it; `keys()` the distinct `from`s and `entry(from)` their targets as one text, which is what
  the summary hashes). `keys.rs`: `normalize_key` (Strong's `G0025` → `G25`), `fold` (the
  catalog's, copied: case, combining marks, final sigma), `key_hash` (8 bytes of SHA-256 of the
  normal form). `hash::entry_hashes(source, rule)` gives an `EntryRecord` a key. `Source::skipped` says what was passed over (`Skipped`,
  `SkipReason`): peripherals (`PERIPHERALS`: FRT, INT, … XXG) noted, books topos doesn't know
  (JUB until it does) and files naming no book warned of (the owner's D4); a book in two files is
  an error naming both. `lx check` (and its JSON's `skipped`), `lx release`, and `lx import`
  print them; the FFI has `LogoxidePackage.skipped()`. Esther's lettered chapters (`Esth.A.1`,
  CCEL's `|Esth|A|1|`) aren't in the format (the owner's D6; the simpler choice, skipping, was
  taken: no import makes them): OSIS and ThML markers naming one are passed over with what
  follows them (`whole::without_lettered`) and warned of (`SkipReason::LetteredChapter`).
  `hash`: `HashRule` (rule 1, the owner's D3: `plain_text` with `Filter::text()`, NFC,
  U+200B/FEFF/00AD/2060 dropped, ZWJ/ZWNJ kept, Unicode whitespace collapsed; a rule never
  changes: `tests/verse_hash.rs` pins it on USFM, USX, and OSIS, so a markup upgrade that breaks
  it means rule 2), `book_hashes` (`VerseRecord`s by topos `BookId`; the core numbers them),
  `count_elements`. `titles`: a file's own title (`own_title`, `book_file_titles`,
  `epub_title`). `versification`: `.vrs` maps and routing (below, under the core's bullet).
  `timings`: WebVTT cues by verse (`parse_timings`, `check_timings`, `book_verses`).
  `cache`: parsed books kept between runs (`BookCache::book(source, id)`: memory, else the
  `BookStore`'s `<key>/<CODE>.postcard`, else parsed and written; `MemoryBookStore`; each file
  starts with `book_header`'s line naming `text_version` (`CACHE_FORMAT`, 4, and the crate's version: what text read from a file depends on, the FFI's `Logoxide.textVersion`, which hosts key what they keep by), else
  it's parsed again, since postcard can't tell a stale layout; `decode` also refuses another
  book's file), the one format the core's `Cache`, the LSP, and the bindings share.
  `passage`: a passage (`text`) or a whole book (`render`: `Rendered`, each verse's and chapter
  label's range, `part_at`, `passage_ranges`) as plain text in a `Style` (labels, hidden kinds,
  templates, `width` to `wrap` at, `indents` by USFM marker for poetry, `verse_lines`, `paragraph_gaps`), marked by the markup's kinds (the LSP's
  `bible.rs` until then). `verse_texts`: every verse of a book as reading text in one call
  (`VerseText`: rule 1's text, so its hash is the summary's; the headings before it; whether it
  starts a paragraph or stanza, or a line of poetry, from the layout `paragraph_gaps` writes:
  the FFI's `LogoxideBook.verseTexts`). `citations::find`: an EPUB's references through topos-formats, and a ThML book's (`is_thml`: `.xml`, `.thml`): parsed by logoxide-markup, its text nodes joined with `\n` as topos's XML search does (the head left out), searched by `BibleMatcher::search`, each `Citation` with no CFI, spine 0, UTF-16 offsets and line and column in that text, and the innermost titled `divN`'s title as its chapter. `versification::system(name)`:
  SWORD's versification systems (sword-modules' `Versification`, as `System`) with each
  chapter's verse count, for a recording numbered by one. `metadata`: an EPUB's
  `FileMetadata` (`epub_metadata`, `opf_metadata`, `elements`); `archive::unzip` (limits);
  `script::dominant_script`. Its `bible_data()` is topos's with the deuterocanon, as the core's
- `crates/logoxide-markup`: Bible markup (OSIS, ThML, GBF, TEI, USX, plain text) parsed into one
  tolerant tree (`Node`, `Element`, `Kind`), `Filter`, `Flattener`/`plain_text`, verses found in a
  book (`verses::verses`, `slice`, `passage_ranges`), references the markup tags, and HTML
  (`HtmlRenderer`, behind the default `html` feature, which nothing here turns on yet). topos's
  topos-bible-markup until October 2026, moved with its history. A
  clean-room rewrite: change its `docs/spec/` first, then the code; its golden files
  (`tests/golden/`) and `NO_PANIC_ROUNDS=100000 cargo test --release -p logoxide-markup --test
  no_panic` check it; never read libsword or other SWORD readers' code (its `CLEANROOM.md`)
- `crates/logoxide-lsp`: the language server (topos's `topos-lsp` until October 2026, moved with
  its history so topos never depends on logoxide). `bible.rs`'s `Bible` reads a package or a plain
  USX folder through logoxide sources (`Books`: a package through the core's `Cache` in
  logoxide's cache folder, the one `lx` fills, so a book parsed by either is found by the other,
  and its verse counts with it; a USX folder through logoxide-text's `BookCache` keyed by its
  files' checksum under `<cache>/folders/`; the cache is the config's, taken only when the
  server has a machine library, so tests keep books in memory), carries its package's `versification` (`own` for a plain folder) so `Bibles::mapped` reads a reference in another Bible's numbering through the project's map packages (`Project::versifications`; hover labels a `hover-bibles` Bible numbered differently with the mapped passage and what isn't in it), and renders hover text and whole books through
  logoxide-text's `passage` (`MarkupSettings::style()` makes its `Style`; `bible::tokens` maps
  the markup's kinds to the editor's semantic tokens; `Part` and `Rendered` are logoxide-text's);
  `settings.rs` finds the project (the `project` setting, a
  workspace folder with a `logoxide.toml`, else the library's own; the server's machine library is
  set by `main.rs`, and tests have none) and loads `bibles = ["id"]` or, with no Bible settings,
  the project's Bibles. `commentaries` (off; `true` for every commentary `Project::commentaries`
  gives, or ids) loads each as a `Bible` (`Bible::from_package`, opened once and kept), and
  hover adds its notes on the passage after the Bible text (`Bible::notes`, the spans
  `Source::notes` would give, from the cached book; `bible::nodes_text` writes a tree in the
  markup's style without labels; each note labeled `*MHCC* (John 3:16-18)`, at most
  `MAX_HOVER_NOTES` a commentary; `stretch-notes = true` stretches them, `book_notes`);
  `hover-max` (`Server::hover_limit`, default `MAX_HOVER_TEXT`, 0 none) cuts hover and
  completion texts, `hover-trim` (`Server::hover_shown`, `Text::trim_start`) drops their leading
  whitespace; `hover-line` (`Server::line_hover`, after Strong's and verse hover) shows every
  reference on a line hovered away from them, each `Server::reference_hover` between rules;
  `strongs = true` keeps `Project::dictionaries` whose `keys`
  are Strong's (`dictionary.rs`: `Dictionary`, and `strongs_at`, an uppercase `G`/`H` number
  standing alone, normalized by logoxide-text's `normalize_key`), and hovering one shows its
  entry (`Source::entry`). `epub = true` (opt-in) searches EPUBs (and `pdf = true` PDFs, with
  `--features pdf`) through topos's file cache on disk (the CLI's: same fingerprint), as
  `logoxide-file://` documents whose text `logoxide/fileText` gives (`workspace.rs`: `BookSearch`,
  `book_text`); logoxide.nvim opens them read-only. References and search commands run on their
  own threads (`server::Search`, a snapshot of what they need); `$/cancelRequest` stops them
  (`workspace::Stop`, checked between files) with -32800. `server.rs` holds the state and
  document sync, `server/` a module per feature and the tests. Its tests read a copy of topos's
  `crates/topos-lib/tests/cases/complete.txt` (`tests/fixtures/`, copied again when topos's
  changes). The Neovim plugin (`~/Development/nvim-plugins/logoxide.nvim`) runs it
- `crates/logoxide-app`: what every front end does (the owner's N1), above
  logoxide-import, as lx, the FFI, and logoxide-mcp are callers of it. It prints nothing:
  downloads and plain-HTTP or unencrypted-repository warnings go to a `Notes` callback (`Note`:
  `Downloading`, `Insecure`, `Unencrypted`; `Display` is lx's words, `is_warning`), and what to
  warn of as it's found to a `Warn` (`&mut dyn FnMut(String)`, lx's words after `warning: `).
  `http.rs`: `Http` (`get`; `get_to(url, sink)` streams, the sink breaking off, tried again only
  before it has bytes), `NoHttp`, and `UreqHttp::new(user_agent, notes)` behind the `ureq`
  feature (default; moved from lx's `net.rs`: 20 s connect, 60 s response, 1 h body, the
  environment's proxy, three tries 500·3ⁿ ms apart, HTTPS kept HTTPS, 1 GiB for `get`), so
  WebAssembly builds with `default-features = false`. `fetch.rs`: `GitFetcher` (`GitCli`, derefs
  to it, plus downloads through `Http` and an `Unencrypted` note once a URL), `NetFetcher` (refs
  from smart HTTP, cached; a commit's archive from `archive_urls` in turn, unpacked; a local
  repository needs git), `fetcher(git, http, notes)` picks by `git::git_available`.
  `catalogs.rs`: `load_catalogs`, `load_catalog` (a URL, else a path; `warnings` in the catalog),
  `load_registry` (a folder's repositories or a catalog; the folder's warnings join the
  catalog's), `registry_url`, `shadowed` and `other_registries` (the warnings' words), each
  taking a `CatalogFetch` (`http`, an optional `CatalogCache`, a `Freshness`; `uncached` for the
  FFI). `catalog_cache.rs`: catalogs at `http(s)://` URLs kept as downloaded in
  `<cache>/catalogs/<sha256 of the URL, 32 hex>.catalog` (a `logoxide-catalog 1 <fetched secs>
  <url>` line, then the bytes; only what parsed; `write_atomic`), used while younger than
  `catalog-max-age` (`Fresh`), never (`Refresh`), or at any age with nothing downloaded
  (`Offline`); when a download or its parse fails a kept copy of any age is used and its
  `warnings` say how old (`age_words`); paths and registry folders are read each time.
  `refresh_catalog` (no fallback) and `refresh(http, cache, config, project, only)` (the
  project's and config's URL catalogs and URL registries, or those named by URL or registry
  name: `Refreshed`). `find_entry` refreshes when no kept copy lists the import; `plan_updates`
  always refreshes.
  `importer.rs` (phase O, the owner's 2026-10-10 request: imports in the shared library):
  `Importing` wraps any `Fetcher` and answers `Fetcher::import` (entries from the project's
  catalogs, `give_entry`; builds `prepare`d by a planned update; `with_library`: a fresh import
  first takes a library copy of the version the catalog lists, for publishers whose versions
  track updates, so a second project downloads nothing); `make(entry, downloads, kind, reader)`
  (shared with the FFI), `download`, `download_pinned` (each locked download by its SHA-256,
  else `Mismatch` saying the publisher changed it); both read a `file://` URL where it is
  (through `FsStorage`), and `download` a `file://` folder as its files (`files/<path>`, `.git`
  left out, 1 GiB at most). Builds are deterministic (logoxide-import's
  tests build each publisher's twice: `build_twice`).
  `import.rs`: `check_source`, `find_entry` (the project's catalogs, a CrossWire-listed SWORD
  repository's too; a bundle's from its name alone, no catalog read), `bundle_source(source,
  dir)` (`https://…`, or a path that's there, as `burrito:<URL>`; `file_url`, `local_path`),
  `bundle(http, entry)` (the bundle read and its entry named by its metadata: `Logoxide::import`
  does it first, vendors a `file://` one always, and `prepare`s the build for a library one so
  it isn't downloaded twice), `files` (each download from the first URL that has it, then `build`),
  `run` (adds `{ import = … }` and installs through an `Importing`: `Imported` with `version`,
  the library `dir`, `skipped`), `run_vendored` (from downloads already made; the old path package in `imports/<id>/`:
  staged, renamed into place, added, installed, else rolled back), `plan_updates` (both kinds:
  `UpdatePlan { dependency, import, dir, in_library, from, upstream_from, outcome: Current |
  Changed(Box<Change { update, fresh, removed, downloads, upstream_version }>) | Failed }`) and
  `apply_update(change, dir, allow_removals)` for a vendored one; a library one is installed
  with `Update::Only` and `Change::build` prepared, beside the old version. `clean.rs`: `clean(library, projects, remove,
  prune_roots, said)` under the library's lock, each step a `Cleaning` given as it happens
  (`STALE_STAGING`); `installed(library, projects)` (no lock) each version with its `used_by`
  (`User`: path, host, gone). `media.rs`: `verify(http, url, file)`, hashed as it downloads. `read.rs`:
  `read(project, library, reader, reference, style)` (topos's `parse_with_translation` over
  `Project::translations`; the Bible named, else `Project::bibles`' first: `Read { bible,
  abbreviation, passage, reference, osis, text, versification }`) and `compare(…, bibles,
  style)` (numbered by the Bible named, else the first of `bibles`; each other numbering mapped
  through `Project::versifications`, `own` read as written: `Compared { reference, passages,
  text, missing, mapped, versification }`); errors are `Resolve`. `lib.rs`: `Logoxide { config,
  library, http, fetcher, reader }` (`new`, `default_native` under `ureq`; `project`,
  `catalogs`, `registry`, `importing`, `install` (through `importing`, its warnings to a
  `Warn`), `install_with`, `import(project, source, vendored, warn)`, `plan_updates`, `clean`,
  `read`, `compare`;
  `catalog_cache` from the library's cache and `catalog-max-age`, `catalog_fetch(freshness)`,
  `refresh_catalogs`; the MCP server's catalogs come through it, so they're kept too)
- `crates/lx` (package `logoxide-cli`, binary `lx`): `new` (`--from <file>`: an EPUB's title, subtitle, authors, language, and
  publisher fill in `[package]` (`create.rs`: `epub_metadata`, `new_from_file`); USX, USFM,
  USJ (`.usj`, or `.json` saying `"type": "USJ"`), OSIS, ThML need `--name` and `--language`; `--kind collection` makes a collection; the folder
  names it, like `cargo new`; then `lx release`), `init`, `add` (`lx add github:owner/repo` or `--git`/`--path`; `--name`,
  `--abbreviation`, `--alias` for forks; refuses a name or abbreviation already used), `remove`, `enable`/`disable` (`enabled = false`), `is-enabled` (prints on or off; exits 0 on, 1 off, 2 on errors), `install` (`--locked`: fail if the lock would change; `--offline`: fetch nothing; `--repair`), `update` (a name neither the manifest nor the lock has is refused), `list` (`(off)`; `--enabled`, `--disabled`), `show` (`--git` reads a
  release's summary through a partial clone; `--notes "<reference>"` a commentary's notes on a
  passage, each labeled with its verses (`cmd/package.rs`'s `lookup`, `book_notes`; `--stretch`),
  `--entry <key>` a dictionary's entry (key, headword, text; a missing one is an error as text,
  `null` as JSON), `--keys [--limit N]` its keys, `--xref "<reference>"` a cross-reference
  package's lines covering any verse of it, with weights; text or JSON), `check` (counts a
  commentary's notes and a dictionary's or cross-reference package's entries, reading each),
  `release` (`--set-version`), `diff`,
  `verify`, `info`, `clean` (versions no lock here uses and no root keeps; `--remove` deletes; gone roots
  reported, `--prune-roots` forgets them), `search` (catalogs; `--registry <name>`; each work's editions under its name, `--flat` not; `--language` (repeated) keeps packages in any of them (`CatalogEntry::in_language`: a BCP 47 range by RFC 4647's basic filtering over canonical tags, `language::matches_range`, or the catalog's language name), `--by-language` gathers under each language first (`Catalog::group_by_language`, by `language::primary_language`, each named in English by `language::language_name`: `ISO_639_1`'s fourth column, SIL's `Ref_Name` without qualifiers; entries show `CatalogEntry::language_shown`, the catalog's name, else that, else the tag, as the FFI's `CatalogEntry.languageName` and the MCP's `languageName` do);
  `--refresh` downloads URL catalogs again, `--offline` only what's kept; `cmd/catalog.rs`'s
  `gather` reads the registries and catalogs both it and `works` use), `works [query]
  [--registry|--catalog] [--packages]` (every work: logoxide's table, the catalogs', then ids
  only packages claim, with how many packages claim each), `import ebible:<id>`, `fetch:<id>`, `ccel:<id>` (a Bible; `ccel:<author>/<work>` any other work, found without a catalog), `door43:<owner>/<repo>` (from Door43's catalog, added), a bundle's URL or path (`bundle_source`: anything `check_source` refuses that's `https://` or a path that's there), or `sword:<repository>/<module>` (an `import` dependency kept in the library; `--vendor`: a path package in `imports/<id>/`, built in a temporary folder, renamed into place, removed if adding fails; `--update [name]`,
  `--dry-run`, `--yes`: make imports again, replace changed ones (a new library version, or a
  vendored one's files), re-lock them; one failing doesn't stop the rest, and removing a vendored
  one's files is asked about in a terminal, else needs `--yes`; `lx update <import>` takes the
  publisher's text now; `add`, `remove`, `install`, `update` all go through `app().install`), `catalog list|add|remove|refresh|entry|build` (`refresh [<URL or registry>…]`: the URL
  catalogs and registries downloaded again and kept, each said refreshed or failed, JSON too;
  `add`, `lx add` through a registry, and `--registry` read theirs afresh; completion uses kept
  copies of URL registries offline) (`add <folder or catalog> --name <registry>`; `build <folder>` writes a catalog.toml of its repositories' newest releases, read with `git show`, each with its `path` and `commit`, unchanged ones kept unless `--force`; `--url` with `{path}`/`{name}`; folder registries read releases the same way), `registry init <folder | github:owner/repo>`, `publish <package> --to …` (`--all`: every
  released package in a folder, at any depth), `yank <id> <version> --from … [--undo]`
  (`publish.rs`: a folder registry, or GitHub releases through `gh`, the catalog being release
  `index`'s file and each archive a release `<id>-v<version>`), `add --registry <catalog> <id>`,
  `media add <file> --url` (repeated: mirrors)
  (lists a file by size and hash), `media check` (timings, then downloads and checks every listed file from each of its URLs; `--offline` for only the timings), `map "<reference>" --from <bible or
  versification> --to …` (`--with <map package>` adds one not in the project), `read
  "<reference>"` (the passage from the Bible named after it, else the first: `John 3:16 KJV`;
  `--width`, `--no-labels`), `compare "<reference>" [--bibles KJV,BSB]` (each Bible's text in its
  own numbering, mapped, with what it lacks; `cmd/read.rs`), `convert <file or folder> --to
  usfm|usx|usj` (logoxide-text's `source::convert`; `cmd/convert.rs`: a file to stdout, made
  printable only on a terminal, or `--output`; `-` is stdin; a folder's book files into the
  folder `--output`, USJ as `.json`; the format read is each file's extension, else how it
  starts, or `--from`, which in a folder chooses the files). Completion
  (`complete.rs`, clap_complete's dynamic engine: `source <(COMPLETE=bash lx)`) gives registries'
  packages, dependencies, and registries; bash needs `:` out of `COMP_WORDBREAKS`. `lx add`
  installs and locks what it adds (`--path` is written relative to the manifest; an absolute path
  in a lock is warned of; a repository without release tags needs `--branch`, `--rev`, or
  `--tag`; tags like `v1.0` that aren't semver are warned of: `resolve::not_semver_tags`), and `lx add` and `lx remove` write the manifest only once that
  succeeds. lx is argument parsing and printing on logoxide-app: one `Logoxide` a process
  (`main.rs`'s `app()`, `default_native` with the `lx/<version>` user agent; its `Notes` print
  each download unless JSON is printed, and each warning), which loads catalogs and registries,
  imports and plans their updates, installs, cleans, checks media, reads, and compares; lx keeps
  the prompts, the JSON reports, and the words. Packages come through git where it's installed,
  else over HTTPS (`NetFetcher`); `Cli::git` is the `git` command for release, diff, and
  publish. The
  project is `--project`, else the library's own with `--global` (no `-g`: `lx search -g` is a
  genre), else the current folder if it has a `logoxide.toml`, else the library's own; `lx list
  --all` (`list_all`) lists every version in the library with the projects locking it
  (logoxide-app's `clean::installed`: the library's own, this one, and every root; none means
  `lx clean --remove` deletes it). `--format text|json` (JSON for list, info, check, diff, verify, catalog list, catalog refresh, search, works,
  import --update, read, compare, show --references, --notes, --entry, --keys, and --xref;
  others refuse it). `lx publish` prints
  `license::assess`'s concerns, and for a package whose rights say it isn't redistributable
  warns loudly and asks (`--yes` answers; required outside a terminal: the owner's D8, still
  never enforced); `lx info` prints the rights. `main.rs` dispatches; `cli.rs` (clap) and
  `cmd/{project,package,catalog,import,media,map,read}.rs`; `tests/cli.rs` runs lx in temporary
  folders (assert_cmd). On Unix lx restores SIGPIPE's default, so `lx … | head` ends quietly.
  Everything lx prints but JSON goes through `outln!`/`out!`/`errln!` (`main.rs`: `println!`
  through `text::printable`), so catalogs', manifests', and git's text can't drive the terminal
- `crates/logoxide-mcp` (binary `logoxide-mcp`): the MCP server (the owner's D14 and
  D15) on rmcp 3.5 over stdio, on logoxide-app's `Logoxide` (read, compare,
  catalogs, registries, install, import) and the core's `Project`, `Library`, `Cache`, and
  `Summary`, as lx is; the project is chosen as lx chooses it (`--project`, else the current
  folder with a `logoxide.toml`, else the library's own), `--library`/`--cache-dir` too.
  `tools.rs`: each tool a plain function `(&Context, Params) -> Result<Value, String>` (tested
  without a client in `src/tests.rs` on `tests/fixtures/`: Bibles `one` (CC0, `English`) and
  `arr` (`LicenseRef-AllRightsReserved`, `Shifted`), the map `eng-shi`, `mhcc`, `strongs`, `tsk`,
  and the `vault` using them with a `catalog.toml`); `server.rs`: rmcp's `#[tool_router]`, each
  call in `spawn_blocking`, its JSON as structured content (and text), its failure a tool error;
  `install` and `import` are removed from the router without `--allow-writes`. Tools:
  `search_catalogs` (registries then catalogs, loaded once a session; 20 a page, at most 100),
  `list_packages`, `package_info` (a package by name, id, abbreviation, or alias: `find`),
  `read_passage`, `compare_passage`, `map_reference`, `show_summary` (installed: published, else
  built; a version: `release::summary_in` through the app's fetcher from the lock's git URL),
  `diff_versions`, `cited_references`, `read_notes` (`stretch`), `lookup_entry`, `cross_references`;
  `install` (`Update::Nothing`, `locked`), `import` (only an entry of the project's own catalogs,
  never a URL or path, then `import::run`). `limits.rs`: `Untrusted` (an object with `source` and
  fields through `text::printable`, capped: `SHORT` 300, `LONG` 2,000, `TEXT` 16,000), `page`
  (about `PAGE_CHARS` 20,000 of JSON a page; cursors `<FNV of the question>.<offset>`, refused for
  another question), `chunks` (a passage's text cut at lines); D15: text not known to be free
  (`is_free`: a free SPDX license, or rights saying redistributable, commercial, derivatives) is
  withheld past `MAX_VERSES` (500) a call or more than half a book (by the package's cached
  counts), every passage with `license` (or `licenses` by package). `tests/session.rs` spawns the
  binary and speaks JSON-RPC over stdio: the fixture vault, and the real KJV and BSB (as git
  dependencies) when `~/Development/logoxide-packages` has them (their tags before v1.0.3 are
  from before the rename: `biblix.toml`, so a diff from 1.0.0 is a tool error). Nothing but
  JSON-RPC on stdout; notes to stderr
- `crates/logoxide-ffi`: BoltFFI 0.31 bindings (npm and PyPI `logoxide`), re-exporting topos-ffi
  (`pub use topos_bible::*`: BoltFFI exports a local dependency's items), so apps need one
  package. Its library is `logoxide` (BoltFFI names artifacts after the package), so it depends on
  the core as `logoxide_core`; nothing else may depend on topos-bible there (both libraries are
  `topos_bible`). The host does all I/O: `Logoxide.openPackage(files, features, cache)` takes
  bytes and `LogoxidePackage.cacheWrites()` hands back what the cache wrote; `openPackageDir` is
  native only. Opening goes through the `Logoxide` object because BoltFFI's WebAssembly
  constructors return `null` on errors while methods throw. A book's text is its own
  `LogoxideBook` (`book.rs`, topos-ffi's `ToposBook` until October 2026, on logoxide-markup with
  its `html` feature, which only this crate turns on): `parse(topos, markup, text)` (an unknown
  markup is `InvalidMarkup`), `verses` (`BookVerse`), `chapterVerses`, passages and chapters as
  HTML or text (`BookTextOptions`), `styledText(passage, BookTextStyle, name)` (logoxide-text's
  `passage::text_in`, the text the language server shows, with `TextMark`s by `NodeKind` for a
  host that colors it), and the tree itself: `nodes()` (`BookNode`: the list in document order,
  each naming its parent, with kind, tag name, attributes, text, milestone; the owner's N6: typed records, not JSON) and `verseNodes()` (each verse's marker, first
  node, and first node after it, by index; a place past an element's last child is the first
  node after the element), so a host renders with its own engine without parsing the formats;
  `LogoxidePackage.book` gives one through `from_tree` (Rust-only). Commentaries,
  dictionaries, and cross-references: `LogoxidePackage.notes(passage,
  BookTextOptions, stretch)` (`Note`: chapter, verse, last, lastChapter, part, text, html; logoxide-text's
  `Source::notes` over the cached book), `entry(key, options)` (`Entry`: key, headword, text,
  html; `null` for a key it lacks), `keys()`, and `crossReferences(verse)` (`CrossReference`:
  from, to, weight; `verse` OSIS or a reference topos reads); `PackageInfo.keys` and
  `CatalogEntry.keys` (`strongs-greek`, …). `LogoxideProject.commentaries(manifests)`,
  `dictionaries`, `crossReferences` take each entry's `logoxide.toml` text from the host
  (`PackageManifest`: name, manifest; the lock doesn't say a package's kind) and give the
  `packages()` in use of that kind, as the core's `Project::packages_of`. topos-ffi comes without its `markup` feature, so it exports no `ToposBook`,
  `BookVerse`, or `BookTextOptions` to clash with these. `ToposTranslations::from`/`core`
  (Rust-only, in topos-ffi) carry logoxide's counts across. `LogoxideCatalog` (`catalog.rs`) searches catalogs (`CatalogEntry`, like
  `PackageInfo`, carries `script` and the language's name; its `CatalogSource` is `Git`,
  `Registry`, or `External` with `import` (emitted `_import`, as `Upstream`'s is), `upstream`,
  `tracksVersions`, and `downloads`, each an `ImportDownload` with its `urls` to try in order),
  and one pair of methods imports from every publisher: `Logoxide.importDownloads(entry)` (what
  to fetch) and `importDownloaded(entry, files)` (the package's files), through logoxide-import
  (phase D: `importEbible`, `importFetchBible`, `importCcel`, and `importSword` are gone);
  `Logoxide.unzip(bytes)` gives a zip's files (logoxide-text's `archive::unzip`, its limits
  kept), so a host reads a zip of book files without a zip library or a made-up import;
  `importPrefixes` (`ebible`, `fetch`, `ccel`, `sword`, `door43`, `burrito`, `dbl`),
  `importBundleEntry(location, files)` (a bundle's entry from its bytes, `bundle_entry`), `swordCatalogUrl(name)` (a
  CrossWire-listed repository's `mods.d.tar.gz`), `defaultCatalogUrls`, `isInsecureUrl`. Errors keep
  `Network` (`Fetch`), `Integrity` (`Mismatch`, `Checksum`, `Changed`), and `Resolve` apart from
  `Files`; `PackageInfo` and `CatalogEntry` carry `rights` (said and derived),
  `LogoxidePackage.licenseConcerns()` gives `license::assess`'s concerns; `parseCatalog` and `parseCatalogBytes` read
  publishers' lists and `catalog.toml` through `logoxide_import::catalog` (`parse_text`, `parse`);
  a SWORD repository's `.tar.gz` only as bytes. Updating an import: `LogoxideCatalog.findUpstream`
  (its entry, from `PackageInfo.upstream`, an `Upstream` record), `Logoxide.importIsCurrent(entry,
  upstream.version)` (skip the download), and
  `Logoxide.updateImport(current, fresh)` (`ImportUpdate`: versions, the publisher's
  `upstreamFrom`/`upstreamTo`, `ChangeKind`, description,
  `files` to write and `removed` paths to delete; then install with `UpdateMode.Only`). `create.rs`: `fileMetadata`,
  `newPackage` (a package's files from one file), `newCollection`, `parseReferences`, and
  `LogoxidePackage.references()` and `epubFiles()`. `LogoxideVersifications` (`versification.rs`,
  from `Logoxide.versifications()`, then `withMap(package)` or `withVrs`) maps references
  (`MappedPassage`: text, OSIS, verses, missing). `LogoxideProject.packages()` lists each package
  the manifest asks for followed by what it brings in (`LockedEntry.dependencyOf`), so hosts see
  a collection's packages; `LockedEntry.enabled` is false for one turned off (`setEnabled`) and
  what it brings in; `nameClashes` (Bibles answering to one name, from `NamedBible`s); `isEnabled`,
  `enabledDependencies`, `disabledDependencies`, and
  `Logoxide.readPackageInfo` (a package's details from its `logoxide.toml` alone, to name one that's off; not `packageInfo`: BoltFFI's Python names each method `_boltffi_<class>_<method>` in snake case, so `Logoxide.package_info` was `LogoxidePackage.info`, and no two may meet that way). `LogoxideInstaller` (`installer.rs`) runs the ordinary resolver against what the host has
  given (`Download` requests, answered with `provideDownload(url, …)` from the first of `urls`
  that answers, are registries' catalogs and archives, with their mirrors, and imports'
  downloads; `ImportEntry { import }`, answered with `provideImportEntry(entry)`
  (`LogoxideCatalog.findImport`), when `loadCatalogs` didn't list an `import` dependency's
  entry: the round's `Fetcher::import` makes it with logoxide-app's `importer::make`;
  `Dependency.import`, `LockedEntry.import`; `LogoxideProject.registries()`, `addRegistry`, `removeRegistry`;
  `Dependency.registry`); whatever is missing (refs, an archive, an installed package's files) stops the round and
  is asked for (`FetchRequest`), and the host calls again. A round (`Round`, behind mutexes, so
  it's a `Fetcher`, `Send + Sync`) is also logoxide-app's `Http` (`HostHttp`, over the downloads
  given), so `LogoxideInstaller.loadCatalogs(project)` (`CatalogsStep`: `Fetch`, else `Done`
  with warnings; `catalog()` is them merged) reads the project's catalogs through
  `logoxide_app::catalogs::load_catalogs` as lx does, `provideDownloadFailed(url, message)`
  making one a warning (paths aren't read there: warned). Natively (like `openPackageDir`),
  `Logoxide.readPassage(projectDir, libraryDir, cacheDir, reference, BookTextStyle)` and
  `comparePassage(…, bibles, style)` are logoxide-app's `read` (`read.rs`: `ReadPassage`,
  `ComparedPassage` with topos-ffi's `Passage`s and `StyledText` cut at the style's `limit`).
  It depends on logoxide-app without `ureq`. Its tests are unit tests
  (`src/tests.rs`): an rlib would collide with the core's `liblogoxide.rlib`. Build with
  `npm install && npm run pack:wasm` or `npm run pack:python`; the examples open and install the
  real packages (`examples/node/install.ts`, whose lock and library `lx install` accepts), and
  `examples/node/readme.ts` and `examples/python/readme.py` run every example in its README (one
  per export, in TypeScript and Python) against the KJV and BSB and `examples/fixtures`, answering
  their downloads offline, and check what each prints. npm's and PyPI's READMEs are that one in
  their own language (`scripts/readme.py`, run by the pack scripts): the other language's code
  blocks left out, `<!-- source -->` passages (building, the checks) too, `<!-- ts: … -->` and
  `<!-- python: … -->` comments shown in theirs, and Python's prose names in snake_case (`null`
  as `None`). Writing one checks it first, failing the pack (`readme.py check` does both): its
  marks whole, its examples in ts-then-python pairs, its links to headings real, the examples
  against the package built (TypeScript type-checked by `tsc` against `logoxide.d.ts`; Python
  parsed and its imports, attributes, arguments, and record fields checked against
  `__init__.pyi`, following what each variable holds), and the API's names in the prose. The npm package is WebAssembly in Node
  too, so `openPackageDir`, `readPassage`, and `comparePassage` are Python's (and Swift's and
  Kotlin's) alone

