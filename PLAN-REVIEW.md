# Plan: the 2026-10-14 review

A design review of logoxide at main d0a689b (53k lines of Rust in nine crates, 438 tests, 228
commits since 2026-10-05; `cargo test --workspace` green), after `PLAN.md`, `PLAN-CRATES.md`, and
`PLAN-NEXT.md` were all but finished. It asks what the earlier plans didn't: whether the project
is ready for anyone else, and what should stop growing until it is. The findings, then the
decisions it recommends, then the work in order. The same text is published as a page for
reading beside the plans.

## Findings

1. **The design has only ever been validated by the loop that produced it.** Nothing is pushed
   or published; nothing has been installed from a URL that isn't `file://` (`doc/workflow.md`
   says so at its end); the formats in `doc/` have one implementation each. The plans keep
   producing phases (`PLAN-CATALOGS.md`, 2026-10-14) ahead of any usage signal. The code is in
   good shape (library code is panic-free; `unsafe` twice, both for SIGPIPE), so the risk isn't
   quality but that the hard questions (is the manifest right, is trust-on-first-use plus the
   lock enough, does anyone want registries) can't be answered by building more.
2. **Two audiences, one of them empty.** A: someone with notes who wants `John 3:16 KJV` to
   resolve, hover, and survive a re-edition. B: a publisher running a registry of thousands of
   books. Most recent work serves B (registries, `yank`, `works.toml`, categories, about-books,
   `lx is-enabled`, `lx catalog build`, GitHub-release registries, catalogs of catalogs); B has
   no members, and the owner can't host mirrors. A is served by `lx init; lx import …`, the
   plugin, and the LSP, and holds the real differentiators: verse-hash links that follow
   renumbering, `lx compare` through `.vrs` maps, a license on every passage.
3. **External surfaces will rot silently.** CCEL is scraped from HTML (`<h2>` links,
   `<p class="List2">`); eBible.org's CSV columns and fetch.bible's `manifest.json` are theirs to
   change; Door43 was written from memory against a service that never answered. CI tests them
   against saved fixtures, so a publisher changing a page breaks users while CI stays green.
4. **Imports download one file at a time.** `importer::download` (logoxide-app) is serial:
   fetch.bible's 66 USX files take 12 s in `lx` against 1 s in Obsidian, whose host
   parallelizes. `Http` is `Sync`, so a bounded thread scope fixes it once for lx, the MCP, and
   the native FFI.
5. **Registry dependencies bake the URL into manifests.** `{ registry = "<catalog URL>" }` is
   keyed by the URL (topos's `doc/logoxide.md`). A registry moving hosts means editing every
   dependent manifest, worst for a published package depending on another. Cargo keeps the name
   in the manifest and the URL in config and the lock. `dorean-local:<id>` exists for `lx add`
   but writes the URL.
6. **Import versions name two texts across machines.** D1 makes every fresh import `1.0.0`
   with the publisher's version in `upstream.version`; `compare_import` raises it on change, but
   only within one library. Two machines importing eBible.org's KJV a month apart both say
   `1.0.0` with different text. The lock's `+i<checksum>` key keeps installs sound, but `lx list`
   shows one version for both and a link's `&ver=1.0.0` is ambiguous (`vh=` rescues verse links,
   not exact-word ones).
7. **CLAUDE.md was a 77 KB changelog**: one paragraph per module, loaded every session (about
   20k tokens), duplicating `doc/*.md` and the crate READMEs and drifting from both.
8. Smaller: the formats (`format = 1`, `LXV2`, `LXE1`, `LXR1`, the lock's `+g`/`+a`/`+i` keys,
   the manifest's 111 keys) bind the moment anything is published, and several were revised
   this month; `Fetcher::import` puts building a package on the fetching trait; `target/` lives
   in Dropbox (still holding `biblix-lsp` artifacts); path packages aren't checksummed, so a
   vault's own translation changes silently and its links' `ver=` mean nothing.

## Decisions

Each taken as recommended until the owner says otherwise.

| # | Question | Recommendation |
|---|---|---|
| R1 | What the first release is for | Audience A only: a vault with imported and git Bibles, read in the plugin, the LSP, lx, and the MCP. B's tools stay in the tree and the docs, under "Also", and stop growing until a stranger asks for one |
| R2 | `PLAN-CATALOGS.md` | Deferred whole, its design kept. Its two owner questions (following on by default; ` (via …)` in search) aren't worth answering until a second catalog exists anywhere |
| R3 | Where module detail lives | `doc/architecture.md` now (CLAUDE.md's old crate map, moved whole, kept current until R-E folds it into each crate's rustdoc and README, after which it goes). CLAUDE.md is orientation, conventions, and decisions not derivable from the code |
| R4 | Live checks | A weekly opt-in CI job (`LIVE=1`) that reads each default catalog and imports one small package a publisher, beside the fixture tests, which stay the gate |
| R5 | Registries by name | `registry = "<name>"` in a manifest resolves through the project's `[settings.registries]` and the config, the lock recording the URL as now; the URL form stays valid. `lx add <name>:<id>` writes the name |
| R6 | Import versions in the open | D1 stands. `lx list`, `lx info`, and the plugin's links show an import's full key (`1.0.0+i9f86d081`); `ver=` in a link carries it for imports |
| R7 | `Fetcher::import` | Left as it is. An `Importer` trait beside `Fetcher` would say what it means, but every caller already wraps one; not worth the churn before a release |

## Phases

Each phase builds, tests, and lints clean on its own, with its docs in the same commit, and the
usual checks (`cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`,
`cargo check -p logoxide --no-default-features --all-targets`, the FFI's tests from main).

### R-A: ship (the owner's)

The G.1/G.2/G.5 items of `PLAN-NEXT.md`, which every plan since 2026-10-09 has routed around.
In order, as the `release-0-1-0-prepared` memory records: push topos's `v0.6.0` tag; give
sword-modules a remote; push logoxide and the KJV and BSB packages to `github.com/Logoxide`;
`cargo publish` in the order topos → sword-modules → logoxide crates → npm/PyPI; host one
`catalog.toml` at `github.com/Logoxide/catalog` listing the two packages.

Check, on a machine that has never built logoxide: `cargo install logoxide-cli && lx init && lx
import ebible:eng-kjv && lx add github:Logoxide/bsb && lx read "John 3:16 BSB"`. Until it
passes, README's quick start isn't true (topos 0.6 and sword-modules 0.1 aren't on crates.io).

### R-B: docs around audience A (done 2026-10-14)

- README's "What works" is A's path (a vault, imports, git packages, reading, links that
  survive), with publishing, registries, and works under "Also". Done.
- CLAUDE.md cut to orientation, conventions, and decisions; the crate map moved to
  `doc/architecture.md` (R3). Done.
- `PLAN-CATALOGS.md` marked deferred (R2); `PLAN-NEXT.md` points here. Done.
- Left for R-C: `lx --help` grouped by clap help headings (Reading / Vault / Publishing), so 29
  subcommands read as three small tools.

### R-S: the formats before shipping (done 2026-10-14)

A pass over `doc/manifest.md`, `catalog.md`, `lock.md`, `summaries.md`, and `entries.md` asking
what would be changed after 1.0 if it could be, done while nothing is published and no reader
has to be kept working. Three forks in worktrees, merged into main (`3a3d51c`):

- **A.1** one hash form: `[media] files.sha256` is written `sha256:<hex>` like every other hash,
  both forms read (`5746f98`)
- **A.2** the pre-release compatibility paths are gone: a lock names packages one way
  (`<id> <key>`, `<rename> (<id>) <key>`; an older form is an error saying to delete the lock
  and install again), `LXV1` isn't read, `verse-hash` and `files-checksum` are required
  (`ad3bfda`, `78c91cd`)
- **A.3** a lock's `commit` is a whole object id, 40 or 64 hex digits (`ad3bfda`)
- **A.4** `abbreviation` required only for `bible` and `commentary` (`Package::short_name`
  falls back on the id; the FFI's `abbreviation` fields are optional), `language` optional for
  `collection` and `versification-map` (read as `und`) (`a6ded57`)
- **A.5** `entries/*.xml` is well-formed XML: GBF and plain bodies inside CDATA, split at `]]>`;
  the reader joins sections and hashes the body as read (`fea5db2`)
- **A.6** the registry-by-name rule is in `manifest.md` (a value shaped like an id is a name,
  resolved through `[settings.registries]` and the config; refused in a fetched package; the
  lock keeps the URL); logoxide 0.2 refuses a name with "doesn't resolve yet" so no reader ever
  fetches `https://dorean`; resolving is R-F (`7d291da`)
- **B.7–B.11** the stale sentences (imports by the publisher's date, "three" summary files,
  "is also gaining checks", "as logoxide 0.1 reads it", unknown keys at every catalog level)
  (`7d291da`, `78c91cd`, `ad3bfda`, `fea5db2`)

Checks on the merged tree: fmt, clippy, the no-`read` build, 427 workspace tests, the FFI's 23
(one fixture had a short commit and an old lock name: fixed), KJV and BSB verify. The KJV and
BSB test packages weren't re-tagged: their v1.0.3 is `LXV2`.

### R-C: small code changes

1. **Parallel downloads** (finding 4): `importer::download` through a bounded
   `std::thread::scope` (eight at a time; every failure collected, the first reported with the
   rest named), the `Downloading` note still sent once a file, order of the result kept. Test on
   logoxide-app's fake web with a counter of concurrent requests.
2. **`lx --help` headings** (R1): `#[command(next_help_heading)]` groups in `cli.rs`; no command
   moves or changes.
3. **Dropbox**: `CARGO_TARGET_DIR` outside Dropbox in the owner's environment; `cargo clean`
   removes the `biblix-*` artifacts. Owner.
4. **Path packages warned of** (finding 8): `lx info` on a path package says its files aren't
   checksummed in the lock.
5. **Unzip through the bindings** (asked by scripture-slinger, 2026-10-17):
   `Logoxide::unzip(bytes) -> Vec<PackageFile>` in the FFI over `logoxide_text::archive::unzip`
   (its limits on entries and size kept), so a host can read a zip of book files without a zip
   library of its own. scripture-slinger reads a player's zip of USFM books today by calling
   `importDownloaded` with a made-up `ebible:` catalog entry, which only works for USFM (eBible's
   import keeps `.usfm`/`.sfm` and `copr.htm`) and needs a fake abbreviation to pass the
   manifest's checks. Test in `logoxide-ffi/src/tests.rs` on a small zip, and one that isn't.
   Done 2026-10-17: `unzip(bytes: Uint8Array): Array<PackageFile>` in the WebAssembly
   package's `logoxide.d.ts`.

### R-D: live checks (R4)

- `.github/workflows/live.yml`: weekly and on demand; `LIVE=1 cargo test -p logoxide-import
  --test live -- --ignored`, each test fetching one default catalog (a count within a band) and
  importing one small package per publisher (eBible.org, fetch.bible, CCEL, CrossWire; Door43
  when it answers), asserting its verse count. Failures open nothing automatically; the job's
  status is the signal.
- `scripts/refresh-fixtures.sh`: downloads each publisher's list again into
  `crates/logoxide-import/tests/fixtures/` with a dated note, so drift is a reviewed diff.
- Door43 checked live the first time it answers; its fixture README updated.

### R-E: the crate map into the crates (R3)

Fold `doc/architecture.md`, a crate at a time, into each crate's `lib.rs` docs (the boundary
and the module list) and README (what it's for, how it's used), until the file says nothing the
crates don't; then remove it. CI gets `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps`
so the docs that remain are checked. One crate a commit; the core first, the FFI last.

### R-F: registries by name (R5, finding 5)

`DetailedDependency.registry` may be a name: `Project::registry_url` resolves it through
`[settings.registries]` and the config (a name neither has is an error naming both places);
the lock keeps the URL as now, and `resolve.rs` reads the lock's URL for a kept entry. A
fetched package's manifest can't name a registry by name (it has no settings to resolve
against): refused, as a path is. `lx add <name>:<id>` writes `registry = "<name>"`; `lx catalog
remove` of a registry a manifest names warns. `doc/manifest.md`, `doc/lock.md`.

### R-G: import versions shown whole (R6, finding 6)

`lx list` and `lx info` print an import's key (`1.0.0+i9f86d081`); `LockedEntry.key` in the
FFI; the plugin's links put the key in `ver=` for an import, and `verse-locator.ts` reads
either form. `doc/lock.md` says a version alone doesn't name an import's text.

## Not in this plan

- A second implementation of the formats, which is what would truly test `doc/*.md`.
- Signing packages or catalogs (`doc/trust.md` says why not yet).
- Anything new for audience B: `PLAN-CATALOGS.md`, `lx media publish`, translations of works
  (all in `PLAN-NEXT.md`'s "Later").

## Order

R-A is the owner's and gates everything that matters; R-B and R-S are done, so the formats are
as they should be at a first release; R-C and R-D can be done by an agent now, in worktrees;
R-E is slow and interruptible; R-F (its rule already in the docs) and R-G after R-A.
