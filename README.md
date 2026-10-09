# biblix

Bibles, commentaries, lexicons, books, and media as versioned packages. A package is a git
repository with a `biblix.toml` and its text in one or more formats; biblix reads it, caches it, and
tells [topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and
verse counts, so references like `John 3:16 KJV` are checked against that translation. `bx` is the
command line.

What works (steps 1 to 5 of the plan, and eBible.org's importer from 7, `doc/biblix.md` in topos):

- `biblix.toml` for packages and vaults (a manifest without `[package]`): formats, Cargo-style
  features (including `dep:` and `name/feature`), and dependencies (`git =` with `version`,
  `tag`, `rev`, or `branch`; or `path =`). There's no default registry
- Sources: a folder of USX books, a folder of USFM books (read through
  [`usfm3`](https://crates.io/crates/usfm3) into USX), and OSIS and ThML Bibles (a book a file,
  or the whole Bible in one, as open-bibles' OSIS and CCEL's ThML are)
- `Storage`: a folder, memory, or (later) a host's files, so the same code runs in WebAssembly
- A cache of verse counts and parsed books, per package version and checksum
- The library: installed versions side by side, as plain files; a project's `biblix.lock` pins
  each package's commit and checksum, so another device installs the same text, and a changed
  file is caught
- Summaries: `bx release` writes `biblix.summary.toml` (verse counts, sizes, what the text holds)
  and `biblix.verses.bin` (a hash of every verse's text), checks the version against what changed
  since the last release (patch: notes and markup; minor: wording or additions; major: verses
  removed or renumbered), then commits and tags it. `bx diff`, `bx verify`, and
  `bx show --git URL` (a release's summary, without downloading its text)
- `bx init`, `add`, `remove`, `install`, `update`, `list`, `show`, `check`, `release`, `diff`,
  `verify`

- `biblix-lsp`: the language server for Bible references and text (topos's `topos-lsp` until
  October 2026). Its Bibles are packages from the workspace's `biblix.toml` (a vault), else the
  library's own, or folders (`bible`, `bibles`); see `crates/biblix-lsp/README.md`

- `biblix-ffi`: bindings for TypeScript/WebAssembly and Python (npm and PyPI `biblix`), with
  topos's whole API in the same package: packages opened from the bytes the host reads, their
  cache handed back to save, a vault's manifest and lock (adding and removing dependencies), and
  installing without `git`, the host downloading refs and archives over HTTP; see
  `crates/biblix-ffi/README.md`

- Catalogs and imports: `bx search` finds packages by name, abbreviation, or language in
  catalogs (a `catalog.toml` of git packages anyone can host, or eBible.org's and fetch.bible's
  lists, about 2,900 translations, searched by default), and `bx import ebible:<id>` or
  `fetch:<id>` makes their download (USFM or USX) into a package the project uses in
  `imports/<id>/`. `bx catalog add <URL or path>` searches your own catalogs too (`[settings]
  catalogs`, before the defaults; `bx catalog list`, `bx catalog remove`; removing a default
  sets `default-catalogs = false`), and `bx catalog entry <git URL>` writes a catalog's entry
  for a package

- Media: audio and video are listed in `[media]` by URL, size, and SHA-256, never stored in the
  package (`bx media add <file> --url <URL>`). An audio Bible's timings say where each verse is:
  a WebVTT file per recording (`timings/JHN.vtt`), each cue named by its verse (`JHN 3:16`).
  `bx media check` checks the timings against the text and downloads and checks every file

The Obsidian plugin (`obsidian-topos-bible`) uses biblix-ffi: Bibles in its vault's `Biblix/`
folder, added from its settings by searching the catalogs or by git URL.

## Using it

[doc/workflow.md](doc/workflow.md) walks through a vault and a package of your own, start to
finish.

```sh
bx init                                    # a biblix.toml here (a vault); without one, bx uses the library's
bx add github:someone/bsb                  # the newest release; --version 1, --tag, --branch, --rev
bx add --git https://example.org/kjv -F strongs
bx add github:bob/kjv --name kjv-bob --abbreviation KJVB   # a fork beside the KJV
bx add --path "Bibles/My Translation"      # used in place
bx install                                 # what biblix.lock says, on a new device
bx update [id]                             # newer versions the requirements allow
bx list

bx release --set-version                   # in a package's repository: summary, version check, tag
bx diff v1.0.0                             # what changed since a release
bx show --git https://example.org/bsb      # a release's summary, without its text
```

The library is `BIBLIX_HOME` (else `$XDG_DATA_HOME/biblix`), or `--library DIR`: any folder,
including inside a vault's `.obsidian/` so it syncs. The cache is per device: `BIBLIX_CACHE`, else
`$XDG_CACHE_HOME/biblix`. Fetching uses the `git` command for now.

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
bx show path/to/kjv                  # what it is, the format read, its books, its checksum
bx check path/to/kjv --cache ~/.cache/biblix   # read every book (through the cache)
bx show path/to/kjv -F usx --no-default-features
```

## Development

topos is a path dependency (`../topos`) until topos 0.6 is released. `cargo test`,
`cargo clippy --all-targets -- -D warnings`, `cargo fmt`.

License: CC0-1.0
