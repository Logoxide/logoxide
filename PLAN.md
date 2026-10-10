# Plan: fixing the 2026-10-09 review

Every finding from the review of logoxide as a package manager, with how to fix it, in an order
where each step is a commit that builds, tests, and lints clean. Findings were checked against the
code (logoxide 6753717, topos 7b80b5b). Decisions the owner must make are marked **Decide**, each
with a recommendation; the plan assumes the recommendation until told otherwise. The owner
chose D1, D2, D11, and the release timing below on 2026-10-10 (each the recommendation).

Phases 1 to 4.5 change formats or the manifest, so they come **before 0.1.0 is published** (it
is prepared, not pushed; the owner chose to wait). Phases 5 and 6 can be 0.2.

## Decisions first

| # | Question | Recommendation |
|---|---|---|
| D1 (chosen) | How imports are versioned | logoxide's own semver from diffs, starting at 1.0.0; the publisher's version or date kept in `[package.upstream] version` |
| D2 (chosen) | How bridges and titles count | joining verses (4,5 → 4-5) major; splitting (4-5 → 4,5) minor; a changed title minor |
| D3 | Verse-hash normalization | NFC (not NFKC); drop U+200B, U+FEFF, U+00AD, U+2060; keep ZWJ/ZWNJ |
| D4 | Books topos doesn't know; a book in two files | warn and skip; two files for one book is an error |
| D5 | Versification names for SWORD imports | SWORD's own (`KJV`, `Synodal`, `LXX`, `Vulg`, …), matched case-insensitively |
| D6 | Esther's lettered chapters (A–F) | not in the format: mapped to ESG's numbered chapters on import, else skipped with a warning |
| D7 | Is converting markup on import a derivative (ND licenses)? | no, but `lx publish` says so when `derivatives = "none"` |
| D8 | `lx publish` of a package that isn't redistributable | a loud warning and a confirmation (`--yes` skips): still "shown, never enforced" |
| D9 | Who owns the table of works | logoxide (`data/works.toml`), Wikidata QIDs optional |
| D10 | Dictionary format | logoxide's `<entry key=… headword=…>` XML, not raw TEI |
| D11 (chosen) | File locking | `std::fs::File::lock`: minimum Rust 1.88 → 1.89, no new crate |
| D12 | HTTP in the core | a `http` feature (ureq), off by default (WebAssembly stays clean), on in lx and logoxide-mcp |
| D13 | Async | the API stays synchronous; servers use `spawn_blocking` |
| D14 | MCP writes | read-only unless started with `--allow-writes`; only the project's catalogs and registries, no raw git URLs or paths |
| D15 | MCP output of text that isn't free | an output cap (500 verses a call, never more than half a book) with the license on every result; a limit on output, not enforcement |
| D16 | Where the MCP servers live | logoxide-mcp here; topos-mcp in topos, with no logoxide dependency |

## Phase 1: correctness fixes in the resolver, lx, and media

Small and independent; can run in parallel with phase 2.

1. **`lx add` / `lx remove` save the manifest only after installing succeeds**
   (`crates/lx/src/cmd/project.rs:184`, `:255`). Install from the edited in-memory project; on
   failure the file is untouched. Test: `lx add` of a missing repository leaves `logoxide.toml`
   byte-identical.
2. **Two names for one commit stay two names** (`resolve.rs:169-179`). The lock entry's key adds
   the manifest name when it isn't the id (`kjv` and `kjv-two` at one commit are two entries
   sharing one library folder). `lock.md` updated. Test: both `KJV` and `KJV2` reach
   `translations()`; `lx disable kjv` turns off only `kjv`.
3. **Revisions must be full object ids** (`manifest.rs:750`): 40 or 64 hex digits; `lx add --rev`
   with a short one says to give the full id. (`archive::Refs::find_commit`'s prefix stays: it
   matches advertised refs, not arbitrary commits.)
4. **`lx update <name>` refuses a name the manifest doesn't have** (`resolve.rs:56`,
   `main.rs:101`).
5. **Kept lock entries are found by who asked for them** (`resolve.rs:346`, `:553`): a root
   dependency matches the lock's top-level `dependencies` entry first, then any entry; a
   package's dependency matches the entry its parent locked. Test: lock with kjv 1.0.0 (from a
   collection) and 2.0.0 (root, `>=1`) keeps 2.0.0 on `lx install`.
6. **Diamonds are said, not hidden.** `Project::names` applies a root dependency's
   `abbreviation`/`aliases` only to the root's copy; `bibles()` reports two Bibles answering to
   one abbreviation, which `lx install`/`lx check` warn of and the FFI exposes.
7. **A locked commit that's gone, and a moved tag.** A failed fetch of a kept commit says the
   commit is no longer on the host and to run `lx update <name>`; `lx update` warns when a
   version comes back with another commit or checksum ("v2.0.0 was moved").
8. **`lx install --locked`** (fail if the lock would change: CI) and **`--offline`** (no network;
   fail if anything isn't installed), as Cargo's. Library: `Update::Nothing` plus a `Frozen` flag
   on the resolver.
9. **Tags that aren't semver** (`v1.0`) are warned of; `lx add` of a repository with no release
   tags refuses unless `--branch`/`--rev` is given.
10. **Path dependencies**: kept relative to the manifest; an absolute path is warned of (the lock
    isn't portable). **Registry releases must list `checksum`** (refused otherwise), so an
    already-installed folder is always verified.
11. **Media timings** (`media.rs`): verse 0 accepted where the text has it (the callback gives
    the chapter's verse numbers, not a count); `timings_path` keeps the file's folder
    (`audio/JHN.mp3` → `timings/audio/JHN.vtt`), with an optional per-file `timings` path;
    `check_package_timings` walks subfolders. `doc/manifest.md` updated.
12. **Catalog search folding** (`catalog.rs:1174`): NFD, then drop every combining mark (Mn),
    final sigma → sigma. Tests: `Ἰησοῦς`/`ιησους`, `בְּרֵאשִׁית`/`בראשית`, `José`/`jose`.
    Adds `unicode-normalization` (also needed in phase 3).

## Phase 2: hardening

1. **git has a deadline** (`git.rs:171`): spawn, wait with a timeout (default 10 minutes for
   fetches, 60 s for `ls-remote`, configurable), kill on expiry; `Fetch` names the timeout.
2. **git can't run what a repository asks for**: `GIT_LFS_SKIP_SMUDGE=1` and
   `-c filter.lfs.process= -c filter.lfs.smudge=` (also makes checksums the same with and
   without LFS); `-c core.hooksPath=<empty temp dir>`; `GIT_SSH_COMMAND="ssh -o BatchMode=yes"`
   unless the user set their own; `git://` refused in a fetched package's dependencies (still
   warned of in the user's own). In repositories lx didn't create (`package_in`,
   `commit_and_tag`, `local_versions`, `registry_build`): `-c core.fsmonitor=false`.
3. **Redirects**: HTTPS → HTTP refused in `net.rs` (ureq's `https_only` or checking each hop); the
   insecure warning judges the final URL.
4. **Terminal output is escaped**: a core `text::printable` strips C0/C1 controls (but `\n`,
   `\t`), ANSI/OSC sequences, and bidi controls (U+202A–202E, U+2066–2069); lx prints catalog
   and manifest fields and git's stderr through it. MCP reuses it (phase 6).
5. **Unpacking streams to storage** (`archive.rs:230`): entries are written as read, under the
   same limits; `MAX_UNPACKED_BYTES` lowered to 1 GiB.
6. **EPUBs get total limits** (topos `topos-formats/src/epub/container.rs`, which now caps each
   file at 256 MiB): add a total-bytes and an entry-count cap, as `archive.rs` has. A topos
   commit; it must follow topos's clean-room rules for that reader (`CLEANROOM.md`).
7. **Symlinks in project folders**: `FsStorage` gets a `no_symlinks` mode (checked with
   `symlink_metadata` along the path), used by `lx publish` (packing) and `apply_import`
   (writing).
8. **Catalog ids are checked** (`check_id`) when a catalog is parsed (`catalog.rs:602`); a bad
   one is passed over with a warning. `lx add`/`lx import` warn when the id is shadowed by an
   earlier catalog (`Catalog::shadowed`), not only `lx search`.

## Phase 3: formats (summaries, books, verses, cache)

Topos first, then logoxide. All in before re-cutting 0.1.0; the KJV and BSB test packages'
summaries are regenerated and re-tagged at the end.

1. **topos**: add JUB, 1MQ, 2MQ, 3MQ, REP, 4BA, PS3, DAG, 2BA, LBA, EZA, 5EZ, 6EZ to the book
   data; `VerseSpan` gets `part: u8` (0 whole, 1 = a, …) so `\v 20a`/`\v 20b` are two verses
   (today both are verse 20, and `diff`'s map keeps only one). Release topos.
2. **Cache keyed by format** (`cache.rs:77-136`): `<id>/<version>-<short>-<format>/`; the
   manifest records and compares the logoxide version (as references do); `CACHE_FORMAT = 2`.
   Test: a package with USX and USFM opened with each in turn shares nothing.
3. **A fixed book table** (new `books.rs`): logoxide owns the numbers in `.bin` files (1–66
   Protestant order, 67–90 topos's current ids with 70 and 78 reserved, 91–103 the books above,
   104–199 reserved, 200–214 peripherals). `VerseHash.book` becomes `Book(u8)`, converted to
   topos's `BookId` by USFM code. Tests: snapshot of the table; every code topos knows is in it.
   The table goes in `doc/summaries.md`.
4. **Diff classification** (`summary.rs:404`, D2): compare the range a verse covers before its
   hash; a number covered by a new bridge is `joined` (major), not `removed`; a split bridge is
   minor; a changed title minor. Tests: split, join, widen, split with rewording, partial verses.
5. **Verse-hash rule 1 and the `.bin` header** (D3): `verse_text` takes a `HashRule`; rule 1 is
   NFC, the four invisible characters dropped, Unicode whitespace collapsed. `.bin` header:
   magic, rule (u16), record length (u8, 12), reserved; records: book, chapter, verse, last, part,
   7 bytes of hash; readers skip bytes past what they know. `logoxide.summary.toml` says
   `verse-hash = 1` and must agree. `lx verify` refuses a rule newer than it knows; `diff` and
   `release` rebuild an old side written with another rule from its tag's text, so a rule change
   is never a rewording. Golden tests (`tests/verse_hash.rs`) pin topos's `plain_text` on USFM,
   USX, and OSIS snippets (nested `\w`, `\add`, a note in a word, NFD Hebrew, ZWSP, NBSP, a
   bridge, a partial verse): if a topos upgrade breaks them, the rule number goes up. An
   `#[ignore]`d test on `~/Development/logoxide-packages` checks the BSB in NFD hashes the same.
6. **Unknown and duplicate books** (`source/mod.rs:141`, `whole.rs:49`, D4): `Source::skipped()`
   says what was passed over and why; peripherals are noted, unknown books warned of, two files
   for one book refused. `lx check`, `release`, and `import` print them.
7. **Versification** (`versification.rs`): `.vrs` parses partial verses (mapped as whole verses,
   the part kept for display); excluded lines (`-PSA 9:21`) are kept and map to `None`, listed in
   `missing`; verse-count lines kept for `problems()`; `pairs()` asserts equal lengths. SWORD
   imports keep `Versification=` (absent means `KJV`, D5); `doc/manifest.md` lists the names.
   Lettered Esther chapters per D6.

## Phase 4: content model

In dependency order. 4.1–4.5 change the manifest, so they come before 0.1.0; 4.6–4.7 can follow.

1. **Import versions and `[package.upstream]`** (D1). `upstream` becomes a table:
   `url`, `import` (`ebible:eng-kjv2006`: the import name, which nothing records today), and
   `version` (the publisher's, as given). `import_is_current` (`import.rs:784`) compares the
   catalog's version with `upstream.version`, never the package's (today a raised version hides
   the publisher's next release for good). New imports start at 1.0.0 and move by
   `next_version`, so `lx add` writes `^1` and updates cross years. `Catalog::find_upstream`
   matches `import`, then `url`.
2. **Prefixed ids for every source**: `ebible-…` and `fetch-…` (`catalog.rs:462`, `:741`), as
   `doc/catalog.md` already promises.
3. **Language and script**: `language` checked as BCP 47 (`language-tags`); new optional `script`
   (ISO 15924, default from the tag); `direction` defaults from the script. SWORD's `Lang`
   normalized (`en_US` → `en-US`, else `und` with the raw value kept); CCEL's guess sets both
   script and direction. FFI and catalog entries carry `script`.
4. **Licenses**: `license` checked as an SPDX expression (`spdx`, lax, `LicenseRef-*` allowed);
   optional `[package.rights]` (`redistributable`, `derivatives = allowed | share-alike | none`,
   `commercial`, `attribution`), derived for known SPDX ids. eBible's license read from the
   Creative Commons URL in `copr.htm` (not `LicenseRef-eBible` for all); the "public domain"
   tests in eBible and `sword_license` (`catalog.rs:1124`) refuse negations; SWORD's
   "permission to distribute in SWORD format" becomes `redistributable = false`,
   `derivatives = "none"`; `upstream.converted = true` when an import rewrites markup (D7). The
   license logic moves into the library (`license::assess`) for lx, the FFI, and MCP; `lx
   publish` per D8; catalog entries carry the license and rights.
5. **Works and editions** (D9): `work = "kjv"`, `edition = "1769-oxford"`, `edition-year`;
   `data/works.toml` lists each work's name, aliases, optional QID, and the import names known to
   be it. Imports fill `work` from their import name, never by guessing from titles; forks with
   `package =` inherit it. `Catalog::search_grouped` (lx search groups by work, `--flat` for the
   old view); the FFI's `PackageInfo`/`CatalogEntry` gain `work`/`edition`; lx warns when two
   dependencies are the same work.
6. **Audio-only packages**: `[media] text = "<dependency>"` and/or `versification = "…"`; such a
   package may have no `[formats]`; timings are checked against that text or the
   versification's counts.
7. **Commentaries, dictionaries, cross-references** (the largest step):
   - Commentaries stay OSIS/ThML per book, but SWORD imports take bridges from linked entries,
     not equal text (today two notes that merely match are merged). `Package::notes(passage)`.
     Ranges across chapters need topos-bible-markup to read `osisID="John.3.36-John.4.2"` (a
     topos commit).
   - Dictionaries (D10): format `dictionary` (`entries/*.xml`), `keys = "strongs-greek" |
     "strongs-hebrew" | "headword"`, `Package::entry(key)`, `keys()`.
   - Cross-references: format `xref`, TSV of `from`, `to`, optional weight (OSIS);
     `Package::cross_references(verse)`.
   - Summaries for them: `logoxide.entries.bin` (`LXE1`, the same header as phase 3: key hash
     and text hash per entry or pair); `diff_entries` (a removed key major; reworded or added
     minor).
   - `Project::packages_of(kind)` with `commentaries()`, `dictionaries()`, `cross_references()`;
     mirrored in the FFI.
   - SWORD: `parse_sword` catalogs dictionary modules (`catalog.rs:368`); `import::sword` reads
     them (`DictionaryModule::iter`; Strong's from `Feature=GreekDef/HebrewDef`, `G0025` →
     `G25`).
   - **sword-modules** needs an API for linked entries (`VerseModule::iter_spans`). Clean-room:
     specify it in its `docs/spec/api.md` first, then implement, by someone who has never read
     libsword.

## Phase 5: logoxide as a library

Each a commit; together they make 0.2.0.

1. **Errors**: `#[non_exhaustive]`; `kind() -> ErrorKind` (NotFound, Io, Invalid, Network, Git,
   Archive, Integrity, Resolve, Busy, Conflict), which the FFI maps; `Io` and `Network` keep
   their `source()`; `Fetch` splits into `Network`, `Git`, `Archive` (`archive.rs:222`).
   Messages unchanged.
2. **Atomic writes**: `write_atomic` (temp file in the folder, fsync, rename, fsync the folder)
   for the manifest and lock (`project.rs:391`, `:431`), config, `catalog.toml`, and publish
   files; `save_manifest` re-reads first and returns `Conflict` if the file changed since it was
   opened; `Library::repair` writes through it, removals last.
3. **`Fetcher: Send + Sync`; an `Http` trait** (`get`, and `get_to` for streaming and hashing
   media); the `http` feature's `UreqHttp` (moved from `lx/src/net.rs`, D12); `NetFetcher`, which
   uses archives where `git` isn't installed. The FFI keeps its rounds and gives bytes itself.
4. **Locks** (D11): exclusive advisory locks on the library (install, repair, clean, import) and
   the project (manifest and lock writes), taken with a timeout, else `Busy`. Lock files live in
   `$XDG_RUNTIME_DIR/logoxide/<hash of the path>.lock` (else the cache folder), never in the
   library, which Dropbox syncs. Readers take none (installs are renames). Nothing on
   WebAssembly. Staging folders become `.{key}.staging-{pid}-{counter}-{random}`
   (`library.rs:208`); `lx clean` removes one only under the lock and only if older than 24 hours.
5. **Projects that use a library** (GC roots): `<library>/roots/<hash>.toml` (path, host, updated,
   the packages it locks), written on every install; `clean` keeps the union of every root's
   packages, so another vault sharing the library keeps its own (`cmd/project.rs:606`); roots
   whose project is gone on this host are reported, `--prune-roots` removes them.
6. **Checksums remembered**: `Storage::stamp(path)` (size, mtime; `None` for memory and the
   host's storage); `<cache>/stamps/` keeps each declared file's stamp with the checksum, used
   only when every stamp matches and is at least 2 s older than the stamp file (git's rule for
   racy files). `verify`, `--repair`, and `release` always hash. `Cache::open` stops hashing every
   file on every open (`cache.rs:126`).
7. **Orchestration in the core**: `ops::catalogs` (`load_catalogs`, `load_registry`,
   `registry_url` from `lx/src/cmd/catalog.rs:16`); `ops::import` (download, stage, rename, roll
   back, from `cmd/import.rs:22`; `plan_updates`/`apply_update` from `:147`; lx keeps only the
   prompt); `license` (phase 4.4). The FFI's copies (`ffi/src/lib.rs:213`, `project.rs:290`)
   call them.
8. **Reading passages**: `Bible::passage` (`logoxide-lsp/src/bible.rs:402`) moves to
   `CachedPackage::passage`; `Project::read("John 3:16 KJV")`; `Project::compare(passage,
   bibles)` maps into each Bible's versification and lists what's missing. The LSP builds on it.
9. **A facade and narrower exports**: a `Logoxide` struct (config, library, cache, `Http`,
   `Fetcher`; `project`, `catalogs`, `install`, `import`, `plan_updates`, `clean`, `read`,
   `compare`); public: the formats, `storage`, `source`, `net`, `ops`, `read`, `error`;
   `archive` under `net`, `registry_build` under `publish`, `package::glob` crate-private. No
   deprecated re-exports (nothing is published). README, CLAUDE.md, 0.2.0.

## Phase 6: MCP servers

On rmcp (the official Rust SDK), tokio over stdio, the library through `spawn_blocking`.

1. **logoxide-mcp** (`crates/logoxide-mcp`, read-only): `search_catalogs`, `package_info`,
   `list_packages`, `read_passage`, `compare_passage`, `map_reference`, `show_summary`,
   `diff_versions`, `cited_references`.
   - Untrusted content (text, notes, catalog and manifest fields, licenses) in structured fields
     marked as such with their source, passed through `text::printable`, each field capped.
   - Every passage carries its license and copyright; non-free text capped per D15.
   - About 20k characters a response with cursors; search 20 results by default, at most 100.
2. **Writes** (D14): `add`, `install`, `import`, `update_import`, `enable`, `disable`, `remove`,
   only with `--allow-writes`; only from the project's catalogs and registries (`--catalog` adds
   one); no git URLs, paths, or `file://`; the same locks as lx.
3. **topos-mcp** in topos (D16): `parse_reference`, `normalize`, `find_references` (notes and
   files), `index_search`, and passages from the Bibles topos is configured with.

## Docs with each change

`doc/manifest.md` (timings, upstream table, work/edition, script, rights, formats, keys,
`[media] text`, versification names), `doc/catalog.md` (fields, prefixes, required checksum),
`doc/lock.md` (renames, `--locked`), `doc/summaries.md` (hash rule, header, book table, change
kinds, `LXE1`), `doc/trust.md` (git hardening, redirects, MCP), CLAUDE.md, and topos's
`doc/logoxide.md` (decisions D1–D16 once made).

## Checking each step

`cargo fmt`, `cargo clippy --all-targets -D warnings`, `cargo test` (and the FFI's own workspace
and `npm run pack:wasm` when it's touched); `lx check` and `lx verify` on
`~/Development/logoxide-packages/{kjv,bsb}`; an import from each source (`ebible:`, `fetch:`,
`ccel:`, `sword:`) once phase 4 lands; then `cargo install --path crates/lx` and
`crates/logoxide-lsp`.

## How the work is split

- Phases 1 and 2: in parallel, each in its own worktree, one commit per item.
- Phase 3: topos first (books, `VerseSpan.part`), then logoxide items in order (3.2 is free to
  go first).
- Phase 4: 4.1–4.5 in order; 4.6 after 4.3; 4.7 last, with the sword-modules spec written first
  by a clean-room agent.
- Phase 5: in order; 5.1–5.2 can start any time.
- Phase 6 after 5.

Nothing is pushed or published without asking.
