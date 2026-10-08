# biblix

Bibles, commentaries, lexicons, books, and media as versioned packages. A package is a git
repository with a `biblix.toml` and its text in one or more formats; biblix reads it, caches it, and
tells [topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and
verse counts, so references like `John 3:16 KJV` are checked against that translation. `bx` is the
command line.

What works (steps 1 to 4 of the plan, and the start of 5, `doc/biblix.md` in topos):

- `biblix.toml` for packages and vaults (a manifest without `[package]`): formats, Cargo-style
  features (including `dep:` and `name/feature`), and dependencies (`git =` with `version`,
  `tag`, `rev`, or `branch`; or `path =`). There's no default registry
- Sources: a folder of USX books, and a folder of USFM books (read through
  [`usfm3`](https://crates.io/crates/usfm3) into USX)
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

Next: the Obsidian plugin moves to biblix-ffi.

## Using it

```sh
bx init                                    # a biblix.toml here (a vault); without one, bx uses the library's
bx add --git https://example.org/bsb       # the newest release; --version 1, --tag, --branch, --rev
bx add --git https://example.org/kjv -F strongs
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
