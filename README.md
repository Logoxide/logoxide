# logoxide

Bibles, commentaries, lexicons, books, and media as versioned packages. A package is a git
repository with a `logoxide.toml` and its text in one or more formats; logoxide reads it, caches it, and
tells [topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and
verse counts, so references like `John 3:16 KJV` are checked against that translation. `lx` is the
command line.

What works (steps 1 to 5 of the plan, and eBible.org's importer from 7, `doc/logoxide.md` in topos):

- `logoxide.toml` for packages and vaults (a manifest without `[package]`): formats, Cargo-style
  features (including `dep:` and `name/feature`), and dependencies (`git =` with `version`,
  `tag`, `rev`, or `branch`; or `path =`). There's no default registry
- Sources: a folder of USX books, a folder of USFM books (read through
  [`usfm3`](https://crates.io/crates/usfm3) into USX), and OSIS and ThML Bibles (a book a file,
  or the whole Bible in one, as open-bibles' OSIS and CCEL's ThML are)
- `Storage`: a folder, memory, or (later) a host's files, so the same code runs in WebAssembly
- A cache of verse counts and parsed books, per package version and checksum
- The library: installed versions side by side, as plain files; a project's `logoxide.lock` pins
  each package's commit and checksum, so another device installs the same text, and a changed
  file is caught
- Summaries: `lx release` writes `logoxide.summary.toml` (verse counts, sizes, what the text holds)
  and `logoxide.verses.bin` (a hash of every verse's text), checks the version against what changed
  since the last release (patch: notes and markup; minor: wording or additions; major: verses
  removed or renumbered), then commits and tags it. `lx diff`, `lx verify`, and
  `lx show --git URL` (a release's summary, without downloading its text)
- `lx init`, `add`, `remove`, `install`, `update`, `list`, `show`, `check`, `release`, `diff`,
  `verify`

- `logoxide-lsp`: the language server for Bible references and text (topos's `topos-lsp` until
  October 2026). Its Bibles are packages from the workspace's `logoxide.toml` (a vault), else the
  library's own, or folders (`bible`, `bibles`); see `crates/logoxide-lsp/README.md`

- `logoxide-ffi`: bindings for TypeScript/WebAssembly and Python (npm and PyPI `logoxide`), with
  topos's whole API in the same package: packages opened from the bytes the host reads, their
  cache handed back to save, a vault's manifest and lock (adding and removing dependencies), and
  installing without `git`, the host downloading refs and archives over HTTP; see
  `crates/logoxide-ffi/README.md`

- Catalogs and imports: `lx search` finds packages by name, abbreviation, or language in
  catalogs (a `catalog.toml` of git packages anyone can host, or eBible.org's and fetch.bible's
  lists, about 2,900 translations, searched by default), and `lx import ebible:<id>` or
  `fetch:<id>` makes their download (USFM or USX) into a package the project uses in
  `imports/<id>/`. `lx catalog add <URL or path>` searches your own catalogs too (`[settings]
  catalogs`, before the defaults; `lx catalog list`, `lx catalog remove`; removing a default
  sets `default-catalogs = false`), and `lx catalog entry <git URL>` writes a catalog's entry
  for a package

- Media: audio and video are listed in `[media]` by URL, size, and SHA-256, never stored in the
  package (`lx media add <file> --url <URL>`). An audio Bible's timings say where each verse is:
  a WebVTT file per recording (`timings/JHN.vtt`), each cue named by its verse (`JHN 3:16`).
  `lx media check` checks the timings against the text and downloads and checks every file

The Obsidian plugin (`obsidian-logoxide`) uses logoxide-ffi: Bibles in its vault's `Logoxide/`
folder, added from its settings by searching the catalogs or by git URL.

## Using it

[doc/workflow.md](doc/workflow.md) walks through a vault and a package of your own, start to
finish.

```sh
lx init                                    # a logoxide.toml here (a vault); without one, lx uses the library's
lx add github:someone/bsb                  # the newest release; --version 1, --tag, --branch, --rev
lx add --git https://example.org/kjv -F strongs
lx add github:bob/kjv --name kjv-bob --abbreviation KJVB   # a fork beside the KJV
lx add --path "Bibles/My Translation"      # used in place
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
including inside a vault's `.obsidian/` so it syncs. The cache is per device: `LOGOXIDE_CACHE`, else
`$XDG_CACHE_HOME/logoxide`. Fetching uses the `git` command for now.

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
versification = "KJV"

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
```

## Install

```sh
cargo install logoxide-cli    # lx
cargo install logoxide-lsp    # the language server
npm install logoxide          # or: pip install logoxide (the bindings, with topos's API)
```

## Development

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`. The bindings
(`crates/logoxide-ffi`) are a workspace of their own that builds against a topos checkout beside
this one (`../topos`): BoltFFI only exports the items of path dependencies, and they re-export
topos's API. Releases: `cargo publish --workspace` for the crates; pushing a `v*` tag publishes the
npm and PyPI packages (`.github/workflows/release.yml`).

License: CC0-1.0
