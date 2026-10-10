# logoxide

Bibles, commentaries, lexicons, books, and media as versioned packages. A package is a git
repository (or a registry's release archive) with a `logoxide.toml` and its text in one or more
formats; logoxide reads it, caches it, and tells
[topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and verse
counts, so references like `John 3:16 KJV` are checked against that translation. `lx` is the
command line.

## Quick start

```sh
cargo install logoxide-cli          # the `lx` command (fetching packages uses `git` if it's installed)
```

```sh
cd ~/Notes
lx init                             # a logoxide.toml here: this folder is now a vault
lx search "king james"              # eBible.org, fetch.bible, CCEL, and CrossWire's SWORD lists
lx import ebible:eng-kjv            # a publisher's download, made into a package in imports/
lx add github:someone/bsb           # or a package published as a git repository
lx list                             # what the vault uses, from logoxide.lock
lx install                          # on another device: exactly what logoxide.lock says
```

`logoxide.toml` says which packages a vault uses and `logoxide.lock` pins their exact text; commit
or sync both with your notes. [doc/workflow.md](doc/workflow.md) walks through a vault and a
package of your own, start to finish.

**Licenses are shown, never enforced.** `lx search` and `lx show` print each text's license as
its publisher states it, but logoxide installs and reads a text whatever it says. Before you
redistribute a text (in a shared vault, a registry, a mirror, or an app), check that you have the
right to: many translations are under copyright and allow only personal use.

## What works

The plan is `doc/logoxide.md` in topos. Its steps 1 to 6 (packages, `lx` with git and path
dependencies, summaries, the language server, the bindings, versioning tools), 10 (collections
and books), 11 (versification maps), and 12 (registries) are done; 7 (importers: eBible.org,
fetch.bible, CCEL, and SWORD done, Door43 not yet), 8 (media: listing, timings, and checks done),
and 9 (links that follow renumbered verses done, reworded ones not yet) are partly done.

- `logoxide.toml` for packages and vaults (a manifest without `[package]`): formats, Cargo-style
  features (including `dep:` and `name/feature`), dependencies (`git =` with `version`, `tag`,
  `rev`, or `branch`; `registry =`; or `path =`), forks side by side (`package =`, with their own
  abbreviation), and dependencies turned off without removing them (`enabled = false`). There's
  no default registry
- Sources: USX and USFM (through [`usfm3`](https://crates.io/crates/usfm3)) a book a file, and OSIS
  and ThML Bibles (a book a file, or the whole Bible in one, as open-bibles' OSIS and CCEL's ThML
  are). Books are kept as EPUBs, and topos finds the Bible references in them
- Kinds: Bibles, commentaries (OSIS or ThML, their notes by passage: `lx show --notes`),
  dictionaries (`entries/*.xml`, Strong's or headwords: `lx show --entry G25`, `--keys`),
  cross-references (`xref/*.tsv`: `lx show --xref "John 3:16"`), books, **collections**
  (packages of packages), and **versification maps** (Paratext `.vrs` lines; `lx map "Psalm
  51:1" --from English --to Original`)
- `Storage`: a folder, memory, or a host's files, so the same code runs in WebAssembly
- A cache of verse counts and parsed books, per package version and checksum
- The library: installed versions side by side, as plain files; `logoxide.lock` pins each
  package's commit (or archive hash) and checksum, so another device installs the same text and a
  changed file is caught. A package is only what its manifest declares, so a reader's notes
  beside a book never change it
- Summaries: `lx release` writes `logoxide.summary.toml` (verse counts, sizes, what the text holds,
  titles) and `logoxide.verses.bin` (a hash of every verse's words), checks the version against
  what changed since the last release (patch: notes and markup; minor: wording, additions, split
  bridges, or titles; major: verses removed, renumbered, or joined), then commits and tags it. `lx diff`,
  `lx verify`, and `lx show --git URL` (a release's summary, without downloading its text)
- Catalogs and imports: `lx search` finds packages by name, abbreviation, language, keyword,
  category, or the books they're about, in your own catalogs (`catalog.toml`, anyone can host
  one) and five publishers' lists searched by default: eBible.org's and fetch.bible's (about 2,900
  translations), CCEL's Bibles and commentaries (ThML), and CrossWire's SWORD repository (OSIS, ThML, and GBF
  Bibles and commentaries, and its dictionaries, Strong's among them). `lx import ebible:<id>`,
  `fetch:<id>`, `ccel:<id>`, or
  `sword:<repository>/<module>` makes a download into a package in `imports/<id>/`, and
  `lx import --update` brings imports up to date
- Registries: a registry publishes each release as an archive listed with its SHA-256 in a
  `catalog.toml`, so any static host (or GitHub releases) serves thousands of books without a
  repository each. `lx registry init`, `lx publish`, `lx yank`, `lx catalog build`, and
  `lx add --registry`; a folder of package repositories works as a registry too
  (`lx catalog add <folder> --name <registry>`, then `lx add <registry>:<id>`)
- Media: audio and video are listed in `[media]` by URL, size, and SHA-256, never stored in the
  package (`lx media add <file> --url <URL>`). An audio Bible's timings say where each verse is: a
  WebVTT file per recording (`timings/JHN.vtt`), each cue named by its verse (`JHN 3:16`).
  `lx media check` checks the timings against the text and downloads and checks every file
- Reading: `lx read "John 3:16 KJV"` prints a passage from the vault's Bible it names (else the
  first), and `lx compare "Psalm 51:1" --bibles KJV,BSB` the passage in each, numbered as each
  numbers it (through the vault's versification maps), saying what one lacks; text or JSON
- `lx init`, `new`, `add`, `remove`, `enable`, `disable`, `install`, `update`, `list`, `show`,
  `check`, `release`, `diff`, `verify`, `info`, `clean`, `search`, `import`, `catalog`,
  `registry`, `publish`, `yank`, `media`, `map`, `read`, `compare`, with completion
  (`source <(COMPLETE=bash lx)`)

- `logoxide-app`: what every front end does, written once (catalogs over HTTP, imports and their
  updates, installs with or without `git`, cleaning, media checks, reading and comparing
  passages) behind one `Logoxide` facade; lx is argument parsing and printing on top of it. See
  `crates/logoxide-app/README.md`

- `logoxide-mcp`: an MCP server over stdio (`claude mcp add logoxide -- logoxide-mcp --project
  ~/notes`): catalogs searched, passages read and compared with their licenses, references
  mapped, summaries and diffs, commentaries' notes, dictionary entries, and cross-references;
  read-only unless `--allow-writes`. See `crates/logoxide-mcp/README.md`

- `logoxide-lsp`: the language server for Bible references and text (topos's `topos-lsp` until
  October 2026). Its Bibles are packages from the workspace's `logoxide.toml` (a vault), else the
  library's own, or folders (`bible`, `bibles`); see `crates/logoxide-lsp/README.md`

- `logoxide-ffi`: bindings for TypeScript/WebAssembly and Python (npm and PyPI `logoxide`), with
  topos's whole API in the same package: packages opened from the bytes the host reads, their
  cache handed back to save, a vault's manifest and lock, catalogs and imports, and installing
  without `git`, the host downloading refs and archives over HTTP; see
  `crates/logoxide-ffi/README.md`

The Obsidian plugin (`obsidian-logoxide`) uses logoxide-ffi: Bibles in its vault's `Logoxide/`
folder, added from its settings by searching the catalogs or by git URL.

## Specifications

For anyone writing another implementation, or publishing packages and catalogs by hand:

- [doc/manifest.md](doc/manifest.md): `logoxide.toml`, every key, kinds, formats, features,
  dependencies, `[media]`, `[titles]`, categories, and what a package is made of
- [doc/catalog.md](doc/catalog.md): `catalog.toml`, registries, `[[package.release]]`, how
  release archives are packed, and the checksum
- [doc/lock.md](doc/lock.md): `logoxide.lock`
- [doc/summaries.md](doc/summaries.md): `logoxide.summary.toml`, `logoxide.verses.bin` (`LXV2`),
  `logoxide.references.bin` (`LXR1`), and how changes decide version numbers
- [doc/trust.md](doc/trust.md): what is checked, what is trusted on first use, and what isn't

## Using it

```sh
lx init                                    # a logoxide.toml here (a vault); without one, lx uses the library's
lx add github:someone/bsb                  # the newest release; --version 1, --tag, --branch, --rev
lx add --git https://example.org/kjv -F strongs
lx add github:bob/kjv --name kjv-bob --abbreviation KJVB   # a fork beside the KJV
lx add --path "Bibles/My Translation"      # used in place
lx add --registry https://example.org/registry/catalog.toml romans
lx install                                 # what logoxide.lock says, on a new device
lx update [id]                             # newer versions the requirements allow
lx list

lx new --from book.epub --license CC0-1.0   # a package from one file: an EPUB's title and authors fill it in
lx new my-library --kind collection --name "My Library" --language en   # a package of packages
lx release --set-version                   # in a package's repository: summary, version check, tag
lx diff v1.0.0                             # what changed since a release
lx show --git https://example.org/bsb      # a release's summary, without its text
```

The library is `LOGOXIDE_HOME` (else `$XDG_DATA_HOME/logoxide`), or `--library DIR`: any folder,
including inside a vault's `.obsidian/` so it syncs. The cache is per device: `LOGOXIDE_CACHE`,
else `$XDG_CACHE_HOME/logoxide`. `~/.config/logoxide/config.toml` can set both, and catalogs and
registries for every project. Fetching git packages runs the `git` command; where it isn't
installed, lx reads repositories over HTTPS instead (their refs, and archives of commits).

## A package

```toml
[package]
id = "kjv"
name = "King James Version (1769)"
abbreviation = "KJV"
aliases = ["AV", "King James"]
version = "1.0.0"
language = "en"
license = "LicenseRef-PublicDomain"

[formats]
usx = { path = "usx/", feature = "usx" }
usfm = { path = "usfm/", feature = "usfm" }

[features]
default = ["usfm"]
usx = []
usfm = []
```

```sh
lx show path/to/kjv                  # what it is, the format read, its books, its checksum
lx check path/to/kjv --cache ~/.cache/logoxide   # read every book (through the cache)
lx show path/to/kjv -F usx --no-default-features
lx show path/to/mhcc --notes "John 3:16"    # a commentary's notes on a passage (--format json too)
lx show path/to/strongs --entry G25         # a dictionary's entry; --keys lists its keys
lx show path/to/tsk --xref "John 3:16"      # a verse's cross-references, with their weights
```

Every key is in [doc/manifest.md](doc/manifest.md).

## Install

```sh
cargo install logoxide-cli    # lx (uses git at run time where it's installed, else HTTPS archives)
cargo install logoxide-lsp    # the language server
npm install logoxide          # or: pip install logoxide (the bindings, with topos's API)
```

Rust 1.89 or later.

## Development

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`. The bindings
(`crates/logoxide-ffi`) are a workspace of their own that builds against a topos checkout beside
this one (`../topos`): BoltFFI only exports the items of path dependencies, and they re-export
topos's API.

- **Versions**: the workspace's version, its `logoxide`, `logoxide-markup`, `logoxide-text`,
  `logoxide-import`, and `logoxide-app` dependencies', and logoxide-ffi's must be
  the same (`scripts/check-versions.sh`, run by CI, and by the release with its tag).
- **topos**: the topos tag the bindings build against is `v` and topos-bible's version in
  `Cargo.toml` (`scripts/topos-ref.sh`); both workflows read it from there.
- **Unpublished dependencies**: until topos 0.6 and sword-modules 0.1 are on crates.io, CI builds
  against checkouts of them (`scripts/patch-unpublished.sh` writes a `[patch.crates-io]` into an
  ignored `.cargo/config.toml`). sword-modules has no public repository yet, so CI needs the
  `SWORD_MODULES_REPO` repository variable (a git URL it can clone) until it is published.
- **Releases**: `cargo publish --workspace` for the crates. Pushing a `v*` tag runs
  `.github/workflows/release.yml`: it checks the versions against the tag, runs the workspace's and
  the bindings' tests, builds the wheels and the npm package, installs a wheel in a fresh
  environment and imports it (`crates/logoxide-ffi/scripts/smoke-test.py`), runs the Node example
  against the npm package, and then publishes to PyPI and npm.
- **Third-party notices**: the wheels and the npm package contain logoxide-ffi compiled with its
  dependencies, so each carries a `THIRD-PARTY-NOTICES` file: every crate linked into it for that
  target, with its license and license files, from `cargo metadata`
  (`crates/logoxide-ffi/scripts/third-party-notices.py`, run by `npm run pack:wasm` and
  `npm run pack:python`; the release checks the file is in both).

License: CC0-1.0
