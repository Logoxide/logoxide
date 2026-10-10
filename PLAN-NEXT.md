# Plan: the rest

What remains after `PLAN.md` (the 2026-10-09 review: phases 1–3 and 4.1–4.6 done) and
`PLAN-CRATES.md` (the crate layout: phases A–F done 2026-10-11), gathered in one place and put in
an order where each step builds, tests, and lints clean on its own. It takes the earlier plans'
decisions as made (D1–D16, C1–C8) and asks only what's new. Written 2026-10-11 against logoxide
303a7cf, topos d9e6c2f, sword-modules f4ff4e3.

The layers now (arrows point down; `logoxide` never reads text):

```
topos-bible           references
logoxide-markup       markup into one tree, text and HTML
logoxide-text         what's in a Bible or a book: verses, hashes, titles, maps, timings, passages
sword-modules         SWORD containers
logoxide              packages, versions, catalogs, summaries as data; asks a TextReader
logoxide-import       publishers' lists and downloads into packages
lx, logoxide-ffi, logoxide-lsp, (logoxide-mcp)
```

## Decisions first

Each assumed as recommended until the owner says otherwise.

| # | Question | Recommendation |
|---|---|---|
| N1 | Where the operations every front end runs live (`PLAN.md` 5.7–5.9: loading catalogs over HTTP, the import flow, `read("John 3:16 KJV")`, `compare`, the `Logoxide` facade) | A new crate `logoxide-app`, above `logoxide-import`: lx, the FFI, and logoxide-mcp are three callers, which is one too many for copies. The core keeps what is the package manager's alone (`ops::catalogs` without HTTP); `logoxide-import` keeps `downloads`/`build`/`update`; `logoxide-app` strings them together with an `Http` trait (D12's `http` feature lives there, not in the core, so WebAssembly stays clean) |
| N2 | C6, a parsed-book cache in `logoxide-text` | Do it, small: `logoxide_text::cache` with a two-method store trait (`read`, `write`) and the postcard layout and header `cache.rs` has today; the core's `Cache` keeps facts (counts, references, later entries) and, under `read`, adapts a `Storage` to the store. The LSP's and FFI's parsed books then share one cache format. If it can't be done in a day, close C6 as "stays in the core under `read`" |
| N3 | Minimum Rust | 1.89 (D11 chose `std::fs::File::lock`); `rust-version` is 1.88 today, CI's msrv job follows |
| N4 | GBF SWORD modules | Read them: logoxide-markup already parses GBF, so a `TextFormat::Gbf` source in `logoxide-text` is a small reader, and `sword::catalog` stops leaving GBF modules out. Not a conversion: the module's entries are written as GBF files, as OSIS and ThML are |
| N5 | Door43 | An importer (`door43:<owner>/<repo>`, Gitea release archives into `usfm/`, `manifest.yaml`'s Dublin Core) as topos's `doc/logoxide.md` step 7 says, but only once git.door43.org answers from the owner's machine; until then a stub module with the catalog URL and a test on a saved `manifest.yaml` |
| N6 | `LogoxideBook`'s tree through the bindings | Typed records (`Node { kind, name, attributes, text, children }`), not JSON strings: BoltFFI makes them real types in each language, and hosts render by walking them. Attributes as a list of pairs |
| N7 | Publishing 0.1.0 | Its gate (`PLAN.md`: phases 1–4.5) is passed. Publish after phase G below, before 4.7 (phase I) changes summaries again (`LXE1` is additive, so it can be 0.2.0). Nothing is pushed or published without the owner, in the order memory records: topos → sword-modules → logoxide crates → npm/PyPI |
| N8 | Catalog of catalogs; `lx media publish` | Stay deferred (the owner's 2026-10-09 call); their open questions are listed under "Later" and asked before any work |

## Phase G: unblocked

Small, first; mostly the owner's.

1. **topos `v0.6.0`** at d9e6c2f (or later): CI's `TOPOS_REF` is derived from the workspace's
   `topos-bible = "0.6.0"`, so the FFI job can't run without it. Owner.
2. **sword-modules** gets a remote (`SWORD_MODULES_REPO` in CI, or crates.io). Owner.
3. **The plugin's `npm run typecheck`** fails in the owner's uncommitted `src/bible/reader.ts`
   (`findLastIndex` with a `string | false` callback at line 439). Owner's change; one-line fix.
4. **`rust-version = "1.89"`** (N3) in `Cargo.toml`, CI's msrv job, README's minimum. Me.
5. **0.1.0** (N7): `scripts/check-versions.sh v0.1.0`, the release checklist in
   `release-0-1-0-prepared` memory, then the owner publishes.

Check: CI green on main once the tags exist.

## Phase H: caches and hardening that nothing else waits for

Can run beside phase I in worktrees; each item a commit.

1. **C6** (N2), done 2026-10-11 on `phase-c6`: `logoxide_text::cache` (`BookStore` trait: `read(path)`, `write(path, bytes)`;
   `BookCache::book(store, key, source, id)` with `CACHE_FORMAT` and the header moved from
   `logoxide/src/cache.rs:77`); the core's `cache.rs` keeps `manifest.postcard` and
   `references.postcard` and, under `read`, `CachedPackage::book` through the text crate's
   cache over its `Storage`. The LSP's `Bible` (its `books` map) and the FFI's
   `LogoxidePackage.book` read through it, so a book parsed by one is found by the other.
2. **Errors** (`PLAN.md` 5.1): `#[non_exhaustive]`, `kind() -> ErrorKind` (NotFound, Io,
   Invalid, Network, Git, Archive, Integrity, Resolve, Busy, Conflict); `Fetch` split into
   `Network`, `Git`, `Archive`; messages unchanged; the FFI maps kinds, not variants. `TextError`
   (logoxide-text) and the import crate's errors map into the same kinds.
3. **Atomic writes** (5.2): `write_atomic` in `storage.rs` (temp file, fsync, rename, fsync the
   folder) for the manifest, lock, config, `catalog.toml`, and publish files; `save_manifest`
   re-reads and returns `Conflict` if the file changed since it was opened; `Library::repair`
   writes through it, removals last.
4. **Locks** (5.4, D11): exclusive advisory locks on the library (install, repair, clean,
   import) and the project (manifest and lock writes), with a timeout, else `Busy`; lock files
   in `$XDG_RUNTIME_DIR/logoxide/<hash>.lock` (else the cache folder), never in the library;
   nothing on WebAssembly; staging folders `.{key}.staging-{pid}-{counter}-{random}`; `lx clean`
   removes one only under the lock and only if older than 24 hours.
5. **Roots** (5.5): `<library>/roots/<hash>.toml` written on every install; `clean` keeps the
   union of every root's packages; `--prune-roots`.
6. **Checksums remembered** (5.6): `Storage::stamp(path)`; `<cache>/stamps/` with git's 2 s
   rule; `verify`, `--repair`, and `release` always hash.

**Done** (2026-10-11, all of H): H.1 C6 as `logoxide_text::cache` (`BookStore`, `BookCache`;
the core's `cache.rs` and the LSP read through it, keyed by checksum and format); H.2
`ErrorKind` with `Fetch` split into `Network`/`Git`/`Archive`, the FFI mapping by kind; H.3
`storage::write_atomic`, `save_manifest`'s `Conflict`; H.4 `locking.rs` (`try_lock` polled 5 s,
else `Busy` naming the holder; files under `$XDG_RUNTIME_DIR/logoxide/`, else the default
cache's `locks/`), `Library::stale_staging`, `lx clean --remove`; H.5 `roots.rs`
(`<library>/roots/<hash>.toml`, `lx clean --prune-roots`); H.6 `stamps.rs`
(`Cache::with_stamps`, set by `Library::book_cache`). Decisions the agent took are in its
commits' messages (977c194, fabb13c, c5460fc, d1295e0, 2872328).

Check: `cargo test --workspace`, the no-read build, `lx check`/`verify` on the KJV and BSB, two
`lx install`s at once on one library (one waits or says `Busy`).

## Phase I: commentaries, dictionaries, cross-references (`PLAN.md` 4.7, by layer)

The largest step. In this order, each a commit, the formats specified in `doc/` first.

1. **Formats in the manifest** (core): `dictionary` (`entries/*.xml`, D10's
   `<entry key=… headword=…>`), `xref` (TSV: `from`, `to`, optional weight, OSIS references);
   `[package] keys = "strongs-greek" | "strongs-hebrew" | "headword"` for dictionaries;
   `Format::text_format` for each; `declared_files` by their extensions. `doc/manifest.md`.
2. **Readers** (logoxide-text): `Source::notes(passage)` for commentaries (OSIS/ThML per book,
   as SWORD imports write them: bridges from linked entries, which `sword::import` already
   makes); `source/dictionary.rs` (`entry(key)`, `keys()`); `source/xref.rs`
   (`cross_references(verse)`); GBF (N4) as `TextFormat::Gbf` while here. `TextFacts` gains
   `entries`: each entry's or pair's key hash and text hash, by `HashRule`.
3. **Summaries as data** (core): `logoxide.entries.bin` (`LXE1`: the same header as `LXV2`,
   then key hash and text hash per entry), `diff_entries` (a removed key major; reworded or
   added minor) in `summary::diff`, `Change` unchanged; `Summary::read` checks the three files
   agree; `doc/summaries.md`. `Project::packages_of(kind)` with `commentaries()`,
   `dictionaries()`, `cross_references()`.
4. **SWORD dictionaries** (logoxide-import): `sword::catalog` lists dictionary modules;
   `sword::import` reads them (`DictionaryModule::iter`; Strong's from
   `Feature=GreekDef/HebrewDef`, `G0025` → `G25`) into `entries/`. If sword-modules lacks what
   this needs, its `docs/spec/api.md` is written first, clean-room, by someone who has never
   read libsword. GBF modules (N4) cataloged and imported as GBF files.
5. **lx**: `lx show` for a commentary's notes on a passage, a dictionary's entry, a verse's
   cross-references; `lx check` runs the readers; `lx import sword:crosswire/StrongsGreek`.
**Done** (2026-10-11, I.1–I.4: commits 9876661, 7fdf356, a722c95, 50e3908): `Format::{Gbf,
Dictionary, Xref}`, `[package] keys`, `doc/entries.md`; logoxide-text `source/{gbf,dictionary,
xref}.rs`, `keys.rs`, `Source::notes/keys/entry/cross_references`, `hash::entry_hashes`,
`TextFacts.entries`; `LXE1` (`Summary.entries`, `diff_entries`), `Project::packages_of`,
`commentaries`, `dictionaries` (kind `lexicon`, which already existed), `cross_references`;
`sword::catalog`/`import` for dictionaries (`entries/<module>.xml`) and GBF modules; `lx check`
counts entries. Live: StrongsGreek and MHCC imported, released, verified. sword-modules needed
no change. Not yet: I.5 (`lx show --notes/--entry/--keys/--xref`) and I.6 (bindings, plugin,
LSP), below.

6. **Bindings and front ends**: `LogoxidePackage.notes(passage)`, `entry(key)`, `keys()`,
   `crossReferences(verse)`, `LogoxideProject.commentaries()` and the rest; the Obsidian
   plugin's commentary reader (topos's `doc/logoxide.md` step 7 lists it) and Strong's lookup;
   the LSP shows a commentary's note in hover when a commentary is among the project's
   packages (a `commentaries` setting, off by default).

Check: a SWORD commentary (MHCC) and dictionary (StrongsGreek) imported and read end to end;
`lx release` of each with its `LXE1`; `lx diff` between two imports.

## Phase J: the application layer (`PLAN.md` 5.3, 5.7–5.9; N1)

1. **`logoxide-app`**: a new crate with an `Http` trait (`get`, `get_to` streaming and hashing)
   and its `http` feature's `UreqHttp` (moved from `lx/src/net.rs`: one agent, timeouts, the
   `lx/<version>` user agent, the proxy, three tries, HTTPS kept); `NetFetcher` for hosts
   without `git` (archives); `catalogs::load`/`load_registry`/`registry_url` (from
   `lx/src/cmd/catalog.rs`); the import flow `import::run` (download, stage, rename, roll back,
   from `lx/src/cmd/import.rs`), `plan_updates`/`apply_update`; `read(project, "John 3:16
   KJV")` and `compare(passage, bibles)` on `logoxide_text::passage` and the project's
   versifications. The core's `Fetcher` becomes `Send + Sync`.
2. **The facade** (5.9): `Logoxide { config, library, cache, http, fetcher, reader: Box<dyn
   TextReader> }` in `logoxide-app` with `project`, `catalogs`, `install`, `import`,
   `plan_updates`, `clean`, `read`, `compare`. lx's `cmd/` modules shrink to argument parsing
   and printing; the FFI's `Logoxide` object wraps it (its rounds and host-given bytes stay for
   WebAssembly: the FFI implements `Http` by asking the host).
3. **Narrower exports** (5.9): public in the core: the formats, `storage`, `reader`, `error`,
   `ops` (what is the package manager's); `archive` under `net`, `registry_build` under
   `publish`, `package::glob` crate-private. No deprecated re-exports. README, CLAUDE.md,
   0.2.0.

Check: lx's `tests/cli.rs` unchanged in behavior; the FFI's tests and `pack:wasm`; the Node
example installs the real packages.

## Phase K: MCP (`PLAN.md` 6)

After J (it is `logoxide-app`'s first server).

1. **logoxide-mcp** (`crates/logoxide-mcp`, rmcp, tokio over stdio, `spawn_blocking`):
   `search_catalogs`, `package_info`, `list_packages`, `read_passage`, `compare_passage`,
   `map_reference`, `show_summary`, `diff_versions`, `cited_references`, and with phase I
   `read_notes`, `lookup_entry`, `cross_references`. Untrusted text in marked fields through
   `text::printable`, each capped; every passage with its license (D15's cap); cursors at about
   20k characters; search 20 by default, 100 at most. Writes only with `--allow-writes` (D14).
2. **topos-mcp** in topos (D16): `parse_reference`, `normalize`, `find_references`,
   `index_search`, passages from topos's own Bibles; no logoxide dependency.

Check: a scripted client session against the KJV and BSB; `doc/trust.md`'s MCP section.

## Phase L: importers' leftovers (topos's `doc/logoxide.md` step 7)

Any order, as they're reachable; each in `logoxide-import`.

1. **GBF modules** (N4): with phase I.2.
2. **Door43** (N5): when git.door43.org answers; `door43:<owner>/<repo>`, the catalog API's
   Bibles (`subject=Bible`, `stage=prod`), a release's `archive/<tag>.tar.gz` into `usfm/`,
   `manifest.yaml`'s `dublin_core` for title, language, license, and version.
3. **CCEL's commentaries and books**: ThML, often a work a file; a commentary's sections tied
   to verses (`<scripRef>`/`parsed` markers) read by phase I.2's `notes`; books as ThML
   packages (`kind = "book"`), with the references topos finds in them cited as EPUBs' are.
4. **Links that follow rewording** (step 9): a link's text fragment found again when a verse's
   words changed a little (`passage::text` plus a diff of words; the hash says it changed, the
   words say where). Lowest priority.
   **Done** (2026-10-11): `logoxide_text::passage::find_fragment` (the longest run of the
   fragment's words in order, case, accents, and punctuation ignored, three in five of them
   needed) and `LogoxideBook.findFragment` in the bindings; the plugin's `verse-locator.ts` can
   call it when a `:~:text=` fragment isn't found as written.

## Phase M: bindings and front ends

1. **`LogoxideBook.nodes()`** (N6, the owner's 2026-10-09 note): the tree and verse spans as
   typed records through logoxide-ffi (no topos change now that the book is logoxide's), so a
   host can render with its own engine without parsing USX/USFM/OSIS/ThML.
2. **`LogoxideBook.text` on `logoxide_text::passage`** (the E.3 leftover), so the bindings
   and the LSP write the same text; `BookTextOptions` maps onto `Style`.
   **Done** (2026-10-11, M.1 and M.2): `LogoxideBook.nodes()`/`verseNodes()` as typed records
   (`BookNode`, `VerseNodes`), and `styledText(passage, BookTextStyle, name)` through
   `passage::text_in`; `text()` and `html()` stay as they were for the plugin.
3. **The LSP and the plugin map references** (`lx map`'s `Versifications` through the FFI's
   `LogoxideVersifications`): a reference in another Bible's numbering shown in hover and in
   the plugin's reader when the project has a map package.
   **The LSP's half is done** (2026-10-11): `Bibles::mapped` (the project's `versifications`,
   each Bible's package `versification`; a plain folder's is `own` and maps to nothing); hover
   shows a `hover-bibles` Bible numbered differently at the mapped passage, labeled with that
   numbering and what isn't in it. The plugin's half waits for the owner's uncommitted reader
   work (phase G.3).
4. **The plugin** uses `importPrefixes` for its browser's source filter and the new error kinds
   (phase H.2); its headless tests cover an import end to end once.

## Later (not planned; the owner's deferrals, N8)

- **Catalogs of catalogs** (`[[catalog]]` entries a search follows). To settle first: cycles and
  depth, whether a followed catalog's registries are usable, who wins when ids collide across
  levels, caching, and how `lx catalog list` shows them.
- **`lx media publish --to <registry>`**: media next to a registry's archives (GitHub release
  files), filling each `[[media.files]]` entry's `url`/`mirrors`. To settle: GitHub's 2 GiB
  limit, immutable versions duplicating media, and whether a registry's catalog lists media at
  all.

## Status (2026-10-11, logoxide main e872e66)

Done: G.4; all of H; I.1–I.4; L.4; M.1, M.2, M.3's LSP half. Open, in this order:

1. **I.5 and I.6 (Rust)**: `lx show` for notes, entries, keys, cross-references (JSON too,
   `tests/cli.rs` fixtures); the bindings' `LogoxidePackage.notes/entry/keys/crossReferences`
   and `LogoxideProject.commentaries/dictionaries/crossReferences` (BoltFFI's Python naming:
   no `_boltffi_<class>_<method>` collisions); the LSP's `commentaries` and `strongs` settings
   in hover. An agent for this was briefed and cut off before writing anything.
2. **I.6 (plugin)**: a commentary reader and Strong's lookup on those bindings, and M.4
   (`importPrefixes` filter, the H.2 error kinds), after the bindings exist and `pack:wasm`
   runs. The plugin's M.3 half (mapped references in its reader) waits for the owner's
   uncommitted `reader.ts` work (G.3).
3. **L.2 and L.3**: Door43 still times out from this machine (both the catalog API and the git
   host, checked 2026-10-11): build `door43.rs` on saved fixtures (`manifest.yaml`, the
   catalog API's JSON, a tar.gz made in the test) and say it's untested live. CCEL commentaries
   (ThML, `kind = "commentary"`, through `Source::notes`; find CCEL's work pages, e.g. Matthew
   Henry's `ccel/henry/mhc1.xml`) and books (`kind = "book"`; citations in ThML need
   `citations::find` extended past EPUBs). An agent for this was briefed and cut off too.
4. **J** (`logoxide-app`), then **K** (MCP), as written above.
5. The owner's: G.1 (push the local topos `v0.6.0` tag, made 2026-10-11), G.2, G.3, G.5.

## Order and split

- G first (days, mostly the owner's); H.2–H.3 and C6 in worktrees beside I.1–I.2 (they touch
  different files); I in order; J after I (the facade should see the final readers); K after J;
  L and M whenever an item is ready, except M.1–M.2 which can go any time.
- Each item is a commit with its docs (`doc/*.md`, CLAUDE.md's crate block, the crate's
  README) and the checks of `PLAN.md`'s "Checking each step" plus
  `cargo check -p logoxide --no-default-features --all-targets`.
- Worktree agents can't build logoxide-ffi in place (its `../../../topos` paths): run its
  checks from main. Nothing is pushed or published without the owner.
