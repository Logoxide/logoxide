# logoxide

Bibles, commentaries, lexicons, books, and media as versioned packages (git repositories with a
`logoxide.toml`), read from many formats, cached, shared, and (later) installed; `lx` is the CLI. The
plan and the owner's decisions are in topos's `doc/logoxide.md` (`../topos/doc/logoxide.md`): read it
before changing the design. `PLAN-NEXT.md` is what remains (decisions N1–N8, phases G–M), after
`PLAN.md` (the 2026-10-09 review) and `PLAN-CRATES.md` (the crate layout, done).

## Layout

- `crates/logoxide`: the library. The package manager never reads text (the owner's decision of
  2026-10-10; `PLAN-CRATES.md` has the phases): everything that parses a format is behind the
  `read` feature (on by default; `cargo check -p logoxide --no-default-features --all-targets`
  must build, and CI runs it), and the rest asks a `reader::TextReader` for the facts it needs
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
    `library`, `cache`, `catalogs`, `[registries]`, `git-timeout`, `git-list-timeout`, machine-wide (flags and env win over
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
    `lx search -t/--nt/--ot/-g/-b/--exclude-*` keeps packages with any about-book among them. A dependency can name another package
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
    `Catalog::search` matches a work's names; `search_grouped`/`group_by_work` gather editions
    (`WorkGroup`); `Project::same_works` (a Bible without `work` takes that of another with its
    package id: forks inherit it) is what lx warns of after add, install, and import;
    `works::describe` for lx info and show. The FFI's `PackageInfo` and `CatalogEntry` carry
    `work`, `edition`, `editionYear`; `LogoxideCatalog.searchGrouped`
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
    that isn't a Bible (a book, a collection) may have none and opens with `NoText` (no books,
    so no verses in its summary). `checksum()` hashes every file but `.git`
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
  - `stamps.rs`: checksums remembered by each declared file's `Storage::stamp` (size and
    modification time; `None` for memory and a host's files, so they're always hashed) in
    `<cache>/stamps/<id>/<version>-<hash of Storage::location>.postcard`, used only when the
    paths and every stamp match and each file is at least `RACY` (2 s) older than the stamp file
    (git's rule for racy files); `lx verify`, `--repair`, `lx release`, and `lx info`'s checksum
    always hash
  - `lock.rs`: `logoxide.lock` (each package's version, commit, checksum, features, and
    dependencies; `dependencies` at the top are what the manifest asks for, each named
    `id version+g<commit>`, or `rename (id) version+g<commit>` when renamed, so two names for one
    commit are two entries sharing a folder; older lock files' `id version` still read; `rename`
    is the manifest's name when it isn't the id; `get` finds by that name). `moved_since` finds
    releases that came back changed (a moved tag), which `lx install`/`lx update` warn of; a kept
    commit that can't be fetched says it may be gone from the host. `LockedPackage::check`
    refuses a commit or sha256 that isn't hex and an id that isn't one, so a lock can't name a
    folder outside the library
  - `git.rs`: `Fetcher` (a repository's tags; its files at a commit, into any `Storage`).
    `GitCli` runs `git`. `expand_url`: `github:owner/repo` (also `gitlab:`, `codeberg:`) to its
    URL, wherever a URL is taken; manifests and locks keep the full URL. `GitCli` refuses
    arguments starting with `-`, puts `--` before repositories, runs with `protocol.ext.allow=never`
    and `GIT_PROTOCOL_FROM_USER=0`, and never follows a symlink in a checkout. Every git command
    has a deadline (`run`: spawned, waited on, killed): `fetch_timeout` (`FETCH_TIMEOUT`, 10
    minutes) and `list_timeout` for `ls-remote` (`LIST_TIMEOUT`, 60 s), set from the config's
    `git-timeout`/`git-list-timeout` (`Config::git`); the error names the limit. `core.fsmonitor`
    is off for every git command (also `registry_build::git_in`); fetching (`Mode::Fetching`, in
    a new repository, `in_clone`) also turns off LFS's filters (`GIT_LFS_SKIP_SMUDGE`, pointers
    kept), points `core.hooksPath` at an empty folder, and runs SSH with `BatchMode=yes` unless
    the user chose their own (`GIT_SSH_COMMAND`, `GIT_SSH`, `core.sshCommand`). Fetched packages'
    dependencies can't use `git://` (resolve.rs); lx warns of the project's own. `is_insecure`
    (`http://`, `git://`, `ftp://`: warned of) and `is_local`
  - `archive.rs`: git over plain HTTP for hosts without `git`: `Refs::parse` reads the smart-HTTP
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
    first of an id wins when catalogs merge. `search` ranks exact abbreviations and ids first,
    and folds case, every combining mark (NFD: accents, breathings, Hebrew points), and final
    sigma away (`fold`, with `unicode-normalization`), so `ιησους` finds `Ἰησοῦς`; a vault's
    `[settings] catalogs`, then those four (`DEFAULT_CATALOGS`: eBible.org, fetch.bible,
    CCEL, CrossWire; the URLs are the core's, what a manifest means by a default) unless
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
    `warn_other_registries`). `registry_build.rs`: folder registries (`folder_catalog`, `build_catalog`,
    `newest_release`, read with `git show`)
  - `media.rs`: `[media]` in a package's `logoxide.toml`: each audio or video file's path, URL,
    `mirrors` (tried in order after it: `MediaFile::urls`), size, SHA-256, and book, never
    stored in the package (its `sha256` must be 64 hex digits); `verify` checks a download
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
    keeps the lock as it is, `offline` fetches nothing; `Project::install_frozen`), through `Packages`
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
    builds an older rule's side (or an `LXV1` one) again from its tag's text, so a rule change
    is never rewording. `diff` (removed, renumbered: old text at another number,
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
  - `references.rs`: the Bible references in a package's EPUBs, found by logoxide-text's
    `citations` (topos-formats' `search_epub_reader`, pure Rust, so WebAssembly too). `find`
    (`read`) reads the books: each reference's OSIS passage, file, CFI, chapter, UTF-16 range,
    line, column, spine item, and paragraph (about 0.1 s a book natively, 0.25 s in
    WebAssembly). Only `cited` is published
    (`LXR1`: deflate over postcard of each passage and its count, in Bible order; 2.6 KB for 937
    references; inflated to at most `MAX_REFERENCES_BYTES`): locations are only of use with the
    book, which gives them again
  - `package.rs` also says what a package is made of: `declared_files` (the manifest, each
    format's files by its extensions, the license file, summary files, timings, `include`
    globs, matched without backtracking); `checksum` hashes only those, so a reader's annotations beside a book never change
    it. `other_files` are kept by repairs and stop `lx clean`
  - `release.rs`: summarize, compare with the newest release tag's summary (or its text, for
    releases before summaries), check the version, write, commit, tag `v{version}`. A package in
    a folder of its repository (`GitCli::package_in`) has its own tags, `<folder>/v{version}`,
    and its commits take only its folder (nothing new: the current commit is tagged)
  - `titles.rs` (`read`): each file's title (a Bible book's or an EPUB's): `[titles]` in the
    manifest, else the file's own as logoxide-text's `titles` reads it (`\toc2`, `\h`, `\toc1`;
    `dc:title`), else the book's English name (a file of many books: the package's name).
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
- `crates/logoxide-import`: packages from other publishers' downloads (PLAN-CRATES.md phase D,
  October 2026; logoxide's `import.rs`, its catalog parsers, and `license_from_text` until
  then). Above the core, as lx and the bindings are: it depends on `logoxide` with its `read`
  feature (an import is checked by reading it), logoxide-text, and sword-modules, which the
  core no longer does. `catalog::parse(location, bytes)` (and `parse_text`) reads any
  publisher's list by its form (`.csv` eBible.org's, `.json` fetch.bible's, `ccel.org/ccel/`
  CCEL's, `.tar.gz` a SWORD repository's; else the core's `Catalog::parse`), each entry an
  `External` source made by `external(prefix, id, tracks_versions, downloads)`;
  `is_publishers`, `publisher_name`. A module per publisher, each with `catalog` and `import`:
  `ebible` (`translations.csv`'s downloadable, redistributable rows; `<id>_usfm.zip` into
  `logoxide.toml` + `usfm/` + `copr.htm`, the license file read for a Creative Commons license
  or the public domain; its upstream version eBible.org's update date), `fetch` (`manifest.json`,
  `https://v1.fetch.bible/manifest.json`, not `/bibles/manifest.json`, an older form; each
  book's USX, a download named by its lowercase code (`book_url`; other paths answer with
  fetch.bible's site, refused) into `usx/` + `COPYRIGHT.txt`), `ccel` (each `<h2>` link's work
  and title; a Bible's ThML (`url`; restricted works answer with a page, refused) into
  `thml/<id>.xml`, named, dated, and licensed by its `<ThML.head>` (`thml_head`; upstream
  version `<version>`, else `DC.Date`; its script, and so its direction, judged from its
  letters), and `sword` (`REPOSITORIES` names the HTTPS repositories in CrossWire's list,
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
  conf's `SourceType`; `keys` from `Feature=GreekDef`/`HebrewDef`, else headword
  (`dictionary_keys`); stored keys in normal form, `00025` → `G25` (`import_key`); a TEI entry's
  first `<orth>` as its headword; `@LINK` aliases as `link` entries), `Made.keys` writes the
  manifest's `keys`, and `finish` refuses one with no entries as it refuses a Bible with no
  books; the repository root is read back from the entry's first download URL; the
  conf's `Versification` is kept (`sword::versification`: SWORD's names as SWORD spells them,
  `KJV` when absent; other imports say `own`); the conf gives version, license
  (`sword::license`) and rights (`sword::rights`: permission given to CrossWire or only in
  SWORD's format is `redistributable = false`, `derivatives = "none"`; "free non-commercial
  distribution" is redistributable without derivatives, since the statement grants nothing
  more, which keeps `license::assess`'s `Converted` concern), language, and `COPYRIGHT.txt`).
  `license.rs`: `license_from_text`, `says_public_domain` (refuses negations). A vault uses any
  import as a path package in `imports/<id>/`. Versions (the owner's D1): a new import is
  `IMPORT_VERSION` (1.0.0), logoxide's own; the publisher's version as it gives it
  (`2026-10-08`, `3.1`) goes in `[package.upstream] version` (`manifest::Upstream`: `url`,
  `import` like `ebible:eng-asv`, `version`, `converted` for SWORD, and `note`, words for a
  hand-made package's source with no URL, which then has none), which `upstream(entry)` writes
  and catalog entries' `version` holds for imports. `downloads(entry)` and
  `build(entry, files, dest)` (dispatching on the prefix) are the one way every host imports
  (lx's `cmd/import.rs`, `Logoxide.importDownloads`/`importDownloaded`); `PREFIXES` is what
  `lx import` accepts. `update.rs`: `compare_import(current, fresh, reader)` (compared at the
  current version: `None` when the declared files match; else the summaries' `Diff`, and the
  version raised by `next_version`; `ImportUpdate` has the publisher's
  `upstream_from`/`upstream_to`), `apply_import` (removes declared files the fresh one lacks,
  through `Storage::remove`, writes the rest; other files stay), `import_removals`,
  `import_is_current(entry, upstream.version)` (compares the publisher's versions,
  `compare_publisher_versions`, never the package's, and only where `tracks_versions`, so `lx`
  can skip downloading)
- `crates/logoxide-text`: what's in a Bible or a book, knowing no packages (PLAN-CRATES.md phase
  C, October 2026; logoxide's `source/`, `versification.rs`, and the parsing halves of `summary`,
  `titles`, `media`, `references`, `create`, and `language` until then). Files come through its
  read-only `Files` (`MemoryFiles`; the core's `reader::StorageFiles` wraps a `Storage`); errors
  are `TextError`. `source/`: one `Source` per `TextFormat` (`source::open`), each giving
  logoxide-markup's tree (`Book`: nodes and verse spans). USX, and USFM through `usfm3` (chosen
  over usfm-onion, whose USX drops the space between adjacent `\w` words; both gave the KJV's
  31,102 verses). OSIS and ThML Bibles through logoxide-markup (`whole.rs`: a file can hold every
  book; it's scanned for the books its verse markers name, cut into books, and its last parse
  kept). ThML's empty `<scripture parsed=…/>` elements (CCEL's Bibles) become verse markers.
  GBF (`gbf.rs`, decision N4) the same way: its `<SB…>` marks name a file's books (OSIS id, USFM
  code, English name, or GBF's numbers 1–39 and 64–90), `<SC…>`/`<SV…>` become chapter and verse
  markers (a mark without a number is the last plus one; `<SV16><SV17>` with nothing between is
  one bridge), each book wrapped in a division. Preference: USX, USFM, OSIS, ThML, GBF. A
  commentary is any verse source: `Source::notes(passage)` (a default on the trait over `book`
  and `passage_ranges`) gives each span covering the passage as a `Note` with its tree. Two
  formats hold no verses and open as sources with no books, so `open` returns one `Box<dyn
  Source>` for any format: `dictionary.rs` (`TextFormat::Dictionary`, `entries/*.xml` as
  `doc/entries.md` specifies: the root's `markup` names the bodies' markup; files indexed once,
  bodies parsed on `Source::entry(key)`; `keys()` in file order; an `<entry link=…/>` alias
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
  starts with `book_header`'s line naming `CACHE_FORMAT` (4) and the crate's version, else
  it's parsed again, since postcard can't tell a stale layout; `decode` also refuses another
  book's file), the one format the core's `Cache`, the LSP, and the bindings share.
  `passage`: a passage (`text`) or a whole book (`render`: `Rendered`, each verse's and chapter
  label's range, `part_at`, `passage_ranges`) as plain text in a `Style` (labels, hidden kinds,
  templates, `width` to `wrap` at), marked by the markup's kinds (PLAN-CRATES.md E.3; the LSP's
  `bible.rs` until then). `citations::find`: an EPUB's references through topos-formats. `versification::system(name)`:
  SWORD's versification systems (sword-modules' `Versification`, as `System`) with each
  chapter's verse count, for a recording numbered by one. `metadata`: an EPUB's
  `FileMetadata` (`epub_metadata`, `opf_metadata`, `elements`); `archive::unzip` (limits);
  `script::dominant_script`. Its `bible_data()` is topos's with the deuterocanon, as the core's
- `crates/logoxide-markup`: Bible markup (OSIS, ThML, GBF, TEI, USX, plain text) parsed into one
  tolerant tree (`Node`, `Element`, `Kind`), `Filter`, `Flattener`/`plain_text`, verses found in a
  book (`verses::verses`, `slice`, `passage_ranges`), references the markup tags, and HTML
  (`HtmlRenderer`, behind the default `html` feature, which nothing here turns on yet). topos's
  topos-bible-markup until October 2026, moved with its history (PLAN-CRATES.md phase B). A
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
  the project's Bibles. `epub = true` (opt-in) searches EPUBs (and `pdf = true` PDFs, with
  `--features pdf`) through topos's file cache on disk (the CLI's: same fingerprint), as
  `logoxide-file://` documents whose text `logoxide/fileText` gives (`workspace.rs`: `BookSearch`,
  `book_text`); logoxide.nvim opens them read-only. References and search commands run on their
  own threads (`server::Search`, a snapshot of what they need); `$/cancelRequest` stops them
  (`workspace::Stop`, checked between files) with -32800. `server.rs` holds the state and
  document sync, `server/` a module per feature and the tests. Its tests read a copy of topos's
  `crates/topos-lib/tests/cases/complete.txt` (`tests/fixtures/`, copied again when topos's
  changes). The Neovim plugin (`~/Development/nvim-plugins/logoxide.nvim`) runs it
- `crates/lx` (package `logoxide-cli`, binary `lx`): `new` (`--from <file>`: an EPUB's title, subtitle, authors, language, and
  publisher fill in `[package]` (`create.rs`: `epub_metadata`, `new_from_file`); USX, USFM,
  OSIS, ThML need `--name` and `--language`; `--kind collection` makes a collection; the folder
  names it, like `cargo new`; then `lx release`), `init`, `add` (`lx add github:owner/repo` or `--git`/`--path`; `--name`,
  `--abbreviation`, `--alias` for forks; refuses a name or abbreviation already used), `remove`, `enable`/`disable` (`enabled = false`), `is-enabled` (prints on or off; exits 0 on, 1 off, 2 on errors), `install` (`--locked`: fail if the lock would change; `--offline`: fetch nothing; `--repair`), `update` (a name neither the manifest nor the lock has is refused), `list` (`(off)`; `--enabled`, `--disabled`), `show` (`--git` reads a
  release's summary through a partial clone; `--notes "<reference>"` a commentary's notes on a
  passage, each labeled with its verses (`cmd/package.rs`'s `lookup`, `Source::notes`),
  `--entry <key>` a dictionary's entry (key, headword, text; a missing one is an error as text,
  `null` as JSON), `--keys [--limit N]` its keys, `--xref "<reference>"` a cross-reference
  package's lines covering any verse of it, with weights; text or JSON), `check` (counts a
  commentary's notes and a dictionary's or cross-reference package's entries, reading each),
  `release` (`--set-version`), `diff`,
  `verify`, `info`, `clean` (versions no lock here uses and no root keeps; `--remove` deletes; gone roots
  reported, `--prune-roots` forgets them), `search` (catalogs; `--registry <name>`; each work's editions under its name, `--flat` not), `import ebible:<id>`, `fetch:<id>`, `ccel:<id>`, or `sword:<repository>/<module>` (built in a temporary folder, renamed into place, removed if adding fails; `--update [name]`,
  `--dry-run`, `--yes`: make imports again, replace changed ones, re-lock them; one failing doesn't
  stop the rest, and removing files is asked about in a terminal, else needs `--yes`), `catalog list|add|remove|entry|build` (`add <folder or catalog> --name <registry>`; `build <folder>` writes a catalog.toml of its repositories' newest releases, read with `git show`, each with its `path` and `commit`, unchanged ones kept unless `--force`; `--url` with `{path}`/`{name}`; folder registries read releases the same way), `registry init <folder | github:owner/repo>`, `publish <package> --to …` (`--all`: every
  released package in a folder, at any depth), `yank <id> <version> --from … [--undo]`
  (`publish.rs`: a folder registry, or GitHub releases through `gh`, the catalog being release
  `index`'s file and each archive a release `<id>-v<version>`), `add --registry <catalog> <id>`,
  `media add <file> --url` (repeated: mirrors)
  (lists a file by size and hash), `media check` (timings, then downloads and checks every listed file from each of its URLs; `--offline` for only the timings), `map "<reference>" --from <bible or
  versification> --to …` (`--with <map package>` adds one not in the project). Completion
  (`complete.rs`, clap_complete's dynamic engine: `source <(COMPLETE=bash lx)`) gives registries'
  packages, dependencies, and registries; bash needs `:` out of `COMP_WORDBREAKS`. `lx add`
  installs and locks what it adds (`--path` is written relative to the manifest; an absolute path
  in a lock is warned of; a repository without release tags needs `--branch`, `--rev`, or
  `--tag`; tags like `v1.0` that aren't semver are warned of: `resolve::not_semver_tags`), and `lx add` and `lx remove` write the manifest only once that
  succeeds. Catalogs and
  imports come over HTTPS with ureq (`net.rs`: one agent, timeouts, `lx/<version>` user agent,
  the environment's proxy, three tries; media hashed as it's read; an HTTPS URL is fetched
  `https_only`, so a redirect to plain HTTP is refused, and plain HTTP is warned of there,
  naming where it was redirected); packages through git. The
  project is `--project`, else the current folder if it has a `logoxide.toml`, else the library's
  own. `--format text|json` (JSON for list, info, check, diff, verify, catalog list, search,
  import --update, show --references, --notes, --entry, --keys, and --xref; others refuse it). `lx publish` prints
  `license::assess`'s concerns, and for a package whose rights say it isn't redistributable
  warns loudly and asks (`--yes` answers; required outside a terminal: the owner's D8, still
  never enforced); `lx info` prints the rights. `main.rs` dispatches; `cli.rs` (clap), `net.rs`, and
  `cmd/{project,package,catalog,import,media,map}.rs`; `tests/cli.rs` runs lx in temporary
  folders (assert_cmd). On Unix lx restores SIGPIPE's default, so `lx … | head` ends quietly.
  Everything lx prints but JSON goes through `outln!`/`out!`/`errln!` (`main.rs`: `println!`
  through `text::printable`), so catalogs', manifests', and git's text can't drive the terminal
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
  each naming its parent, with kind, tag name, attributes, text, milestone; PLAN-NEXT.md M.1,
  the owner's N6: typed records, not JSON) and `verseNodes()` (each verse's marker, first, and
  last node by index), so a host renders with its own engine without parsing the formats;
  `LogoxidePackage.book` gives one through `from_tree` (Rust-only). Commentaries,
  dictionaries, and cross-references (PLAN-NEXT.md I.6): `LogoxidePackage.notes(passage,
  BookTextOptions)` (`Note`: chapter, verse, last, lastChapter, part, text, html; logoxide-text's
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
  `importPrefixes` (`ebible`, `fetch`, `ccel`, `sword`), `swordCatalogUrl(name)` (a
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

- Like topos: no `unwrap` in library code, errors in `LogoxideError` (`#[non_exhaustive]`; callers
  branch on `kind()`: `ErrorKind`'s `NotFound`, `Io`, `Invalid`, `Network`, `Git`, `Archive`,
  `Integrity`, `Resolve`, `Busy`, `Conflict`; `Io` and `Network` keep a `source()`; the FFI's
  `LogoxideError` is the kinds), comments and docs in plain
  prose, `cargo fmt` and `cargo clippy --all-targets -D warnings` clean
- Check behavior on the real packages (`lx check ~/Development/logoxide-packages/kjv`), not only
  unit tests
- Commit messages end with a `Co-Authored-By` line
- Formats for other implementations are specified in `doc/` (`manifest.md`, `catalog.md`,
  `lock.md`, `summaries.md`, `entries.md`, `trust.md`): change them with the code. `scripts/` has CI's helpers
  (`check-versions.sh`, `topos-ref.sh`, `patch-unpublished.sh`); the minimum Rust is 1.89
