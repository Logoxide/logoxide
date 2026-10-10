# Plan: the crate layout

The owner decided on 2026-10-10 that the `logoxide` crate is a package manager and never reads
text. Reading (formats into a tree, verse hashes, titles, versification files, timings, rendering)
moves to crates beneath it, and importing other publishers' downloads moves to a crate above it.
This plan lists every module move, in phases that each build, test, and lint clean on their own.
It continues `PLAN.md` (its phases 1–3 are done; 4.1–4.6 are in flight; 4.7, 5.8, and 5.9 are
reshaped here, see "What changes in PLAN.md").

## The layers

```
topos-bible          references: parsing, search, Translations        (topos, unchanged)
logoxide-markup      OSIS, ThML, GBF, TEI, USX into one tree, and out  (from topos; clean-room)
                     as text or HTML; references the markup tags;
                     verses found in a book
logoxide-text        what a package's files hold: books as verses,     (new)
                     hashes, titles, .vrs maps, WebVTT timings, an
                     EPUB's citations, passages read and rendered;
                     later commentaries, dictionaries, cross-references
sword-modules        SWORD containers: confs, indexes, entries         (drops its markup dependency)
logoxide             packages: manifest, lock, resolve, git, archives, (shrinks: no parser left)
                     library, registries, catalog.toml, summaries as
                     data, diffs, semver, release, publish, media files
logoxide-import      eBible.org, fetch.bible, CCEL, SWORD: their       (new; uses text, logoxide,
                     catalogs and downloads into packages              sword-modules)
lx, logoxide-ffi, logoxide-lsp, logoxide-mcp                           (wire the layers together)
```

Arrows only point down. `logoxide` and `logoxide-text` never see each other: `logoxide` asks for
the facts it needs through a trait (`TextReader`, phase A) that lx, the FFI, and the LSP implement
with `logoxide-text` in a few dozen lines each. The summary file is the contract between the two:
`logoxide-text` makes its facts, `logoxide` diffs and versions them.

## Decisions

| # | Question | Recommendation (assumed until told otherwise) |
|---|---|---|
| C1 | Names | `logoxide-markup`, `logoxide-text`, `logoxide-import`; `sword-modules` keeps its name. Rule: a crate is prefixed when it uses topos's or logoxide's types and ships in step with them; sword-modules uses neither |
| C2 | Markup and text: one crate or two | Two. The clean-room rules (spec first, golden files, no libsword) are enforceable per crate; the text crate grows in ways the spec shouldn't |
| C3 | HTML rendering | Stays, behind `logoxide-markup`'s `html` feature (on by default). It's in scope only because the front ends that need it are the project's |
| C4 | sword-modules and markup | It drops the dependency. `Entry` keeps `raw` and `markup`; `nodes`, `plain_text`, `html`, and `strongs` go (spec change in `docs/spec/api.md` first). `Markup` becomes sword-modules' own enum (it's SWORD's `SourceType`) |
| C5 | The boundary | A trait in `logoxide` (`TextReader`), returning logoxide's own summary types; implemented beside each binary, never in `logoxide-text`. No shared types crate unless the adapters grow past about fifty lines |
| C6 | Caches | `logoxide-text` caches parsed books, keyed by the files' checksum and format. `logoxide`'s `Cache` keeps only facts (verse counts, cited references) per package version, filled through the trait when a package has no summary |
| C7 | When the import crate is cut | Right after phase A: it's almost all moves, and it takes sword-modules out of the core |
| C8 | Obsidian | On logoxide-ffi alone; `ToposBook` becomes `LogoxideBook` there |

## Phase A: the boundary, inside `logoxide`

**Done (2026-10-10)**, as below but: the trait lives in `reader.rs` (`text.rs` was taken by
`printable`); its methods are `counts`, `facts`, and `timing_problems`; `.vrs` parsing,
WebVTT parsing, `titles::own_title`, and `language::dominant_script` stay ungated, since they
need no parser crate (they move in phase C all the same); A.4 (`CatalogSource::External`) is
deferred to phase D, where the import crate says what an entry must carry. The integration
tests that read real packages are `#![cfg(feature = "read")]`, as are the unit tests of
`cache`, `create`, and `media`: the package manager's own tests with made-up facts (`NoReader`)
are phase C's.

No new crates and no other repository. The point is to prove the package manager never parses,
cheaply, before anything moves.

1. **`logoxide::text`** (a new module): the trait and the types the package manager needs.
   ```rust
   pub trait TextReader: Send + Sync {
       /// The facts a summary records: verse counts per book, hashes, element counts,
       /// titles, cited passages, the mapping count, and what was skipped
       fn summarize(&self, files: &dyn Storage, manifest: &Manifest, rule: HashRule) -> Result<TextFacts>;
       /// Problems `lx check` and `lx release` refuse: untitled files, timings that don't fit
       fn check(&self, files: &dyn Storage, manifest: &Manifest) -> Result<Vec<Problem>>;
   }
   ```
   `TextFacts` is the text's half of `Summary` (`books`, `contents`, `verses`, `titles`, `cited`,
   `references`, `mappings`) plus `Vec<VerseHash>` and `Vec<Skipped>`; `summary::summarize`
   becomes "ask the reader, then add checksums, sizes, media, id, version". `NoReader`
   implements it for packages with no text (collections, books kept as files) and for tests with
   fixture summaries.
2. **Feature `read`** (on by default) gates everything that parses: `source/`, `books.rs`,
   `titles.rs`, the hashing half of `summary.rs` (`verse_text`, `book_hashes`, `verse_hash`),
   `versification::VersificationMap::{parse, from_package}` and `problems` (the `.vrs` reader;
   `Versifications`, `route`, `map`, `Mapped`, and `Project::numbering` stay, since they work on
   parsed maps), `media::{parse_timings, check_timings, timings, check_package_timings}`
   (`MediaFile`, `verify`, `store_path`, `add_media` stay), `references::{find, epub_files}`
   (`cited`, `encode`, `decode` stay), `cache.rs`'s parsed books, `create::{epub_metadata,
   opf_metadata, file_format}`, `import::thml_head`, `import::sword`'s conversion,
   `language::dominant_script`, and `catalog.rs`'s SWORD conf reading. `cargo check -p logoxide
   --no-default-features` must build: that's the proof, and CI runs it.
3. **`release::plan`, `Project::translations`, `lx check`, `import::compare_import`** take a
   `&dyn TextReader`. In this phase lx passes `logoxide::text::DefaultReader` (the `read`
   feature's own implementation); the FFI and LSP the same.
4. **`CatalogSource::External { prefix, upstream }`** replaces the `Ebible`, `Fetch`, `Ccel`, and
   `Sword` variants. `find_import`, `find_upstream`, `versions_track_updates`, and
   `works::of_source` work on the prefix. The catalog parsers stay where they are for now (phase
   D moves them).
5. **Docs**: `doc/summaries.md` says which fields the text gives and which the package manager
   adds; CLAUDE.md.

Check: `cargo check -p logoxide --no-default-features`; everything in "Checking each step".

## Phase B: markup out of topos

Independent of phase D; after A.

1. **Move `topos/crates/topos-markup` to `crates/logoxide-markup`** with its history (as the LSP
   was moved: `git filter-repo --subdirectory-filter` on a topos clone, then a merge with
   `--allow-unrelated-histories`), including `CLEANROOM.md`, `docs/spec/`, `tests/golden/`, and
   the no-panic test. Crate name `logoxide-markup`, library `logoxide_markup`; depends on
   `topos-bible` (for `Passage` and `BookId` in `refs.rs` and `verses.rs`). Features `serde` and
   `html` (C3: `html.rs` behind it). **Done** (dd3dca4, merged 1055e4a: 19 commits kept).
2. **topos**: remove the crate from its workspace; `topos-formats` loses its `markup` feature
   (`markup.rs`: `MarkupLocation` and `search_markup` move to `logoxide-text` as `search`);
   `topos-ffi` loses its `markup` feature and `book.rs` (`ToposBook` becomes `LogoxideBook` in
   logoxide-ffi, phase E; `ToposTranslations::from`/`core` stay). topos's CLAUDE.md and
   `doc/logoxide.md`'s layer table say so. A topos commit, tagged for `TOPOS_REF`. **Order**:
   E.2a (`LogoxideBook` in logoxide-ffi) comes first, since the FFI's `book()` hands
   `ToposBook::from_tree` a tree; until then B.4's `package::to_topos` carries the tree across
   through postcard. B.2 goes on a topos branch, merged when the owner says.
3. **sword-modules** (C4; **done**, sword-modules f4ff4e3, spec 3beb87b): `Markup` is
   sword-modules' own enum in `conf.rs` (`Osis`, `Thml`, `Gbf`, `Tei`, `Plaintext`); `Entry` is
   `raw` + `markup`; the `markup` re-export, `Entry::nodes`, `plain_text`, `html`, `strongs`,
   and `DictionaryModule::strongs_language`/`strongs_at` are gone; `get_strongs` takes a `&str`;
   `iter_spans`, `location`, `EntryLocation`, and `VerseSpan::passage`/`osis_ref` stay (they use
   only topos-bible). logoxide needed no change.
4. **logoxide**: `topos-bible-markup` → `logoxide-markup` in the workspace and `pub use
   logoxide_markup as markup`; the FFI's workspace likewise (a path inside this repository now,
   not into topos). **Done** (20dabfa; the workspace turns `html` off, nothing here renders).

Check: topos's and sword-modules' own checks; `cargo test` here; `NO_PANIC_ROUNDS` once.

## Phase C: `logoxide-text`

**Done (2026-10-11)**, with one change to C5: the trait's implementation (`DefaultReader`)
stayed in `logoxide` as a feature-gated adapter (`reader::with_text`, under `read`, which now
means `dep:logoxide-text` and `dep:sword-modules`) rather than three copies beside lx, the FFI,
and the LSP. The arrow that matters still points one way (logoxide-text never depends on
logoxide), the package manager alone still builds with `--no-default-features`, and the adapter
is one module that can move later if the owner prefers. Moved as the table says, except:
`books.rs` stays in the core (the text crate hashes by topos `BookId`, the adapter numbers);
the parsed-book cache stayed in `cache.rs` under `read` until C6 was done (2026-10-11,
PLAN-NEXT.md H.1: `logoxide_text::cache`, the core's `CachedPackage::book` over it, the LSP and
the bindings reading the same files); `check_package_timings`
and `[media] versification`'s counts (sword-modules) stay in the core under `read` until phase
D; the LSP's `Bible::passage` and `search` aren't moved yet (E.3). `TextFormat` is the text
crate's own enum of the four readable formats; `Format::text_format` maps to it.

After A and B. The `read` feature's modules leave `logoxide` for the new crate, and the trait's
implementation moves beside each binary.

| From `logoxide` | To `logoxide-text` |
|---|---|
| `source/` (`mod.rs`, `usx.rs`, `usfm.rs`, `osis.rs`, `thml.rs`, `whole.rs`), `Book`, `Source`, `Skipped`, `SkipReason`, `PERIPHERALS`, `NoText` | `source/`, same names |
| `books.rs` (`Book(u8)`, the fixed numbers, its snapshot test) | `books.rs`; `logoxide` keeps a copy of only the `u8` in `VerseHash` and the USFM-code table it needs to print references |
| `titles.rs` | `titles.rs` |
| `summary::{verse_text, book_hashes, verse_hash, HashRule's text rules}`; `tests/verse_hash.rs` | `hash.rs`: the rules by number; the test pins topos's text here, so a topos upgrade breaks this crate |
| `versification::{Verse, VerseRange, Mapping, VersificationMap (parse, from_files, problems, excluded, counts, passed_over), Versifications, route, map, Mapped, parse_reference}` | `versification.rs`; `logoxide` keeps `Project::numbering` and `map_reference` as thin calls on what the wiring layer hands it (or they move to the wiring layer: decide when the adapter is written) |
| `media::{VerseTiming, timings_path, parse_timings, check_timings, timings, check_package_timings}` | `timings.rs`; `logoxide::media` keeps `Media`, `MediaFile`, `verify`, `store_path`, `check_sha256` |
| `references::{FoundReference, epub_files, find}` (topos-formats' EPUB reader) | `citations.rs`; `logoxide::references` keeps `Cited`, `cited`, `encode`, `decode` (`LXR1`) |
| `cache.rs`'s parsed books (`book_header`, `CACHE_FORMAT`, postcard of `Book`) | `cache.rs` (C6, done 2026-10-11): `BookCache` keyed `<checksum>-<format>/<CODE>.postcard` under a `BookStore`; the core keeps `manifest.postcard` and `references.postcard` and adapts its `Storage` |
| `create::{file_format, epub_metadata, opf_metadata, FileMetadata}` | `metadata.rs`; `create::new_from_file` in `logoxide` takes a `FileMetadata` instead of bytes |
| `language::dominant_script` | `script.rs` |
| `package::open`'s choice of a source by the enabled formats | `open.rs`: `read_package(files, manifest, enabled) -> Box<dyn Source>`; `logoxide::Package` keeps the manifest, features, `declared_files`, `checksum`, `other_files`, `glob` |
| logoxide-lsp's `Bible::passage` (`bible.rs:402`) and `MarkupSettings`' element handling; topos-formats' `MarkupLocation` | `passage.rs` (read, slice, render as text or HTML with `TextOptions`) and `search.rs` (PLAN.md 5.8, re-homed) |

Dependencies: `topos-bible`, `topos-bible-formats` (`epub`), `logoxide-markup`, `usfm3`,
`quick-xml`, `postcard`, `sha2`, `unicode-normalization`. Features: `html` (passes through),
`serde`. Its files come through its own small read-only trait, `Files { read, list }`; the
adapters wrap `logoxide::Storage`. `tests/fixtures/english-original` is copied (the `.vrs`
tests here, the package tests in `logoxide`).

`logoxide` after this: `manifest`, `lock`, `resolve`, `git`, `archive`, `library`, `project`,
`catalog` (`catalog.toml`, folder registries, `External` entries), `registry_build`, `publish`,
`release`, `summary` (types, read, write, `diff`, `Change`, `next_version`), `references`
(`LXR1`), `media` (files), `versification` (routing, if kept), `license`, `language` (tags),
`works`, `storage`, `config`, `topos_config`, `text` (the trait), `error`. Its dependencies lose
`topos-bible-markup`, `topos-bible-formats`, `sword-modules`, `usfm3`, `quick-xml`, `zip`,
`unicode-normalization` moves with `catalog::fold` only if search stays (it does).

## Phase D: `logoxide-import`

After A; can run beside B. Mostly moves.

| From `logoxide` | To `logoxide-import` |
|---|---|
| `import.rs` whole: `ebible`, `fetch_bible`, `ccel`, `sword` and their URL helpers, `thml_head`, `sword_versification`, `osis_book_file`, `thml_book_file`, `sword_notice`, `unzip`, `upstream`, `downloads`, `build`, `compare_import`, `apply_import`, `import_removals`, `import_is_current`, `compare_publisher_versions`, `ImportUpdate` | `lib.rs` and a module per publisher: `ebible.rs`, `fetch.rs`, `ccel.rs`, `sword.rs`; `update.rs` for comparing and applying |
| `catalog.rs`: eBible.org's CSV (`csv`, `abbreviation`), fetch.bible's JSON and `fetch_bible_license`, `parse_ccel`, `parse_sword`, `conf_text`, `SWORD_REPOSITORIES`, `sword_repository_name`, `sword_catalog_url`, `CCEL_CATALOG`, `DEFAULT_CATALOGS`'s four URLs, `CatalogSource::prefix` per kind | `catalog.rs`: each publisher's parser returns `Vec<CatalogEntry>` with `External { prefix, upstream }`; `default_catalogs()`; `parse(url, bytes)` by extension |
| `license::{license_from_text, says_public_domain}` and the SWORD/eBible rights rules (PLAN.md 4.4) | `license.rs`; `logoxide::license` keeps `check_license`, `Rights`, `derived_rights`, `assess`, `is_free` |
| `works::of_import` and `data/works.toml`'s import names | `works.rs` (the table of works stays in `logoxide`; the import-name column moves) |
| `sword-modules`, `zip`, `flate2`/`tar` for `mods.d.tar.gz` | its dependencies |

`compare_import` summarizes both sides, so the crate depends on `logoxide-text` and implements
`TextReader` itself for that call (or takes one: take one, so the binaries own the only
implementation). lx's `cmd/import.rs` and the FFI's `importEbible`, `importFetchBible`,
`importCcel`, `importSword`, `importDownloads`, `importDownloaded`, `findUpstream`,
`importIsCurrent`, `updateImport`, `parseCatalogBytes`, `defaultCatalogUrls` call it. `lx
catalog list` loads catalogs through `logoxide_import::catalog::parse` when the URL is a
publisher's, else `logoxide::catalog::Catalog::parse`.

**Done** (2026-10-11, branch `phase-d`): `crates/logoxide-import` with `catalog` (`parse`,
`parse_text`, `is_publishers`, `publisher_name`), a module per publisher (`ebible`, `fetch`,
`ccel`, `sword`, each with `catalog` and `import`), `license`, and `update`; the core's
`CatalogSource` is `Git`, `Registry`, and `External { import, upstream, tracks_versions,
downloads }` (A.4, taken as the owner chose on 2026-10-11: the core keeps only what it decides
with, and the import crate's `build` dispatches on the prefix). Where the plan was changed:
`DEFAULT_CATALOGS` and the four URL constants stay in the core (a manifest's default catalogs
are the package manager's to know); `works.toml` stays whole in the core, `of_import` being a
string lookup, and `catalog::known_works` is public for the parsers; `compare_import` takes a
`TextReader`, but the crate depends on the core's `read` feature all the same, since `finish`
checks an import by opening it; the core's last use of sword-modules (`[media] versification`
counts) went to logoxide-text as `versification::system`, so the core's `read` feature is
logoxide-text alone. The FFI's `CatalogSource` has the one `External` variant and loses
`importEbible`, `importFetchBible`, `importCcel`, and `importSword` (`importDownloads` and
`importDownloaded` import every publisher); the Obsidian plugin is adapted to that.

## Phase E: wiring

After C and D, except E.2a (`LogoxideBook` on logoxide-markup, the bridge dropped), which comes
before B.2.

1. **lx**: `src/text.rs` implements `TextReader` with `logoxide-text` (summarize, check); `cmd/`
   modules call `logoxide-text` directly for `lx show`'s titles, `lx map`, `lx media check`'s
   timings, and `lx new --from`.
2. **logoxide-ffi**: depends on `logoxide-markup`, `logoxide-text`, `logoxide-import`,
   `logoxide_core`; `LogoxideBook` (from `ToposBook`, `from_tree` kept Rust-only), rendering and
   `chapterVerses` through `logoxide-text`; `LogoxidePackage.book`, `titles`, `timings`,
   `verseHashes`, `references`, `skipped` through it; the import methods through
   `logoxide-import`. The npm and PyPI package is still one `logoxide`. CI's `TOPOS_REF` checkout
   stays only for `topos-ffi`.
3. **logoxide-lsp**: `bible.rs` on `logoxide-text` (`open`, `passage`, `search`); `settings.rs`'s
   `MarkupElement` on `logoxide_markup::Kind`; `workspace.rs` keeps topos-formats for EPUB and
   PDF text.
   **Done** (2026-10-11): `logoxide-text::passage` (`Style`, `text`, `render`, `Rendered`,
   `Part`, `wrap`) holds what `bible.rs` rendered; the LSP keeps the books, maps the markup's
   kinds to its tokens (`bible::tokens`), and names the documents; `MarkupSettings::style()`
   makes the `Style`. Item 1 (lx's own `TextReader`) was superseded by C5's adapter in the
   core, and the FFI (2) reads through the core's `read` feature and `logoxide-import`; its
   `LogoxideBook.text` still renders on logoxide-markup directly.
4. **obsidian-logoxide** (C8): drop any `topos` import; `ToposBook` → `LogoxideBook`. Rebuild
   with `npm run pack:wasm`; run its headless tests.
5. **logoxide.nvim**: nothing (it speaks to the LSP).

## Phase F: what changes in PLAN.md

- **4.7 (commentaries, dictionaries, cross-references)**: the formats and readers
  (`Package::notes`, `entry`, `keys`, `cross_references`) go in `logoxide-text`; `LXE1` and
  `diff_entries` stay in `logoxide::summary` as data, with `TextFacts` gaining `entries`;
  `Project::packages_of(kind)` stays in `logoxide` (it's the lock); the SWORD dictionary import
  goes in `logoxide-import`. Not started until phase C lands.
- **5.7 (orchestration in the core)**: unchanged; the trait is what makes it possible.
- **5.8 (reading passages)**: `logoxide-text::passage`, not `CachedPackage::passage`;
  `Project::read` and `compare` become wiring-layer functions (in lx and the FFI, or a small
  `logoxide-app` crate if they're duplicated: decide when written).
- **5.9 (facade)**: `Logoxide` holds a `Box<dyn TextReader>` given by the host.
- **6 (MCP)**: logoxide-mcp depends on every layer, like lx.

**Done** (2026-10-11): PLAN.md's phase 4 intro, 4.7, 5.7, 5.8 (E.3's part done), 5.9, 6,
"Checking each step", and "How the work is split" say this.

## Docs and CI with each change

- Each crate's README opens with one sentence of scope that can refuse things: `logoxide`
  "packages, versions, and where they come from; never reads text"; `logoxide-markup` "five
  markups into one tree and out again; knows no packages"; `logoxide-text` "what's in a Bible
  or book: verses, hashes, titles, numbering; knows no packages"; `logoxide-import` "other
  publishers' downloads into packages".
- CLAUDE.md here (the layout section becomes one block per crate), topos's CLAUDE.md, topos's
  `doc/logoxide.md` (the layer table and a decision entry dated 2026-10-10), `doc/summaries.md`.
- `scripts/check-versions.sh` covers the new crates (one workspace version); `patch-unpublished.sh`
  and the `~/Development/.cargo` patch lose `topos-bible-markup`; publish order becomes topos →
  sword-modules → logoxide-markup → logoxide-text → logoxide → logoxide-import → lx, logoxide-lsp,
  logoxide-ffi. CI adds `cargo check -p logoxide --no-default-features` (phase A) and the
  no-panic test for `logoxide-markup`.

## Checking each step

`cargo fmt`, `cargo clippy --all-targets -D warnings`, `cargo test` in every workspace touched
(topos, sword-modules, here, the FFI's own, `npm run pack:wasm` when the FFI changes); `lx check`,
`lx verify`, and `lx release --dry-run` (or `lx diff`) on `~/Development/logoxide-packages/{kjv,bsb}`
give the same summaries as before (the `.bin` files byte-identical: the hash rule didn't
change); one import from each publisher after phase D; `cargo install --path crates/lx` and
`crates/logoxide-lsp`.

## How the work is split

- The session on PLAN.md phase 4 finishes its item and commits; nothing from 4.7 starts.
- A first, alone, in main (it touches most modules; a worktree would conflict with everything).
- Then B and D in parallel worktrees: B needs topos and sword-modules checkouts (the sword-modules
  spec by a clean-room agent first); D is in this repository. The FFI can't be built in a
  worktree (its `../../../topos` paths), so its checks run from main after each merge.
- C after both; E after C; F's 4.7 after E.
- Nothing is pushed or published without asking.
