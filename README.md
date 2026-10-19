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
lx import ebible:eng-kjv            # a publisher's download, made into a package in the library
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

For someone with notes (a vault, an editor, Obsidian) who wants `John 3:16 KJV` to resolve, show
its text, and still point at the same words after the translation is re-edited:

- **A vault**: `lx init` writes a `logoxide.toml` (a manifest without `[package]`); `lx import
  ebible:<id>`, `fetch:<id>`, `ccel:<id>`, or `sword:<repository>/<module>` makes a publisher's
  download into a package kept in the shared library, and `lx add github:owner/repo` adds one
  published as a git repository. `logoxide.lock` pins each package's exact text (its commit or
  archive hash, and a checksum of every file), so `lx install` on another device gets the same
  bytes or fails. Forks sit side by side (`package =`, their own abbreviation); a package can be
  turned off without removing it (`enabled = false`)
- **Finding texts**: `lx search` looks through your own catalogs and five publishers' lists by
  default: eBible.org's and fetch.bible's (about 2,900 translations), CCEL's Bibles and
  commentaries (ThML), and CrossWire's SWORD repository (Bibles, commentaries, and dictionaries,
  Strong's among them), grouped by the work each is an edition of. Catalogs are kept in the cache
  for a week (`--refresh`, `--offline`); `lx import --update` brings imports up to date
- **Reading**: `lx read "John 3:16 KJV"`, and `lx compare "Psalm 51:1" --bibles KJV,BSB` with each
  Bible in its own numbering through the vault's versification maps, saying what one lacks.
  Commentaries' notes on a passage (`lx show --notes`), a dictionary's entry (`--entry G25`),
  and a verse's cross-references (`--xref`); text or JSON
- **Links that survive updates**: every release ships `logoxide.verses.bin`, a hash of each verse's
  words, so a link made against one version (`&ver=1.0.2&vh=9f86d081`) finds its verse again after
  a renumbering and says when the words changed. `lx diff` and `lx verify` read the same hashes
- **Formats read**: USX, USFM, USJ, OSIS, ThML, GBF (a book a file, or a whole Bible in one);
  dictionaries (`entries/*.xml`) and cross-references (`xref/*.tsv`); books as EPUBs, with the
  references topos finds in them; Paratext `.vrs` versification maps (`lx map "Psalm 51:1" --from
  English --to Original`); `lx convert` writes a book (or a folder of them) from USFM, USX, or
  USJ into another of them
- **Front ends on the same code**: `logoxide-lsp` (hover, completion, references, and whole books
  in any editor; `crates/logoxide-lsp/README.md`), `logoxide-mcp` (an MCP server: catalogs,
  passages with their licenses, notes, entries; read-only unless `--allow-writes`;
  `crates/logoxide-mcp/README.md`), `logoxide-ffi` (npm and PyPI `logoxide`, with topos's API in
  the same package; packages opened from bytes and installed without git, so WebAssembly and
  Obsidian too; `crates/logoxide-ffi/README.md`), and the Obsidian plugin `obsidian-logoxide`,
  whose Bibles live in its vault's `Logoxide/` folder. All of them are argument parsing and
  display on `logoxide-app` (`crates/logoxide-app/README.md`)
- **Licenses are shown with every text**, never enforced: each package's SPDX license and rights
  as its publisher states them, and the MCP server attaches them to every passage

### Also: publishing

For someone making or hosting packages. These work and are specified in `doc/`, but nobody hosts
one yet, so they're kept as they are until someone asks:

- **Packages**: `lx new` (`--from book.epub` fills `[package]` from the file; `--kind collection`
  for a package of packages), `lx release` (writes the summary, checks the version against what
  changed since the last release: patch for notes and markup, minor for wording, additions, or
  titles, major for verses removed, renumbered, or joined; then commits and tags), `lx check`,
  `lx show --git URL` (a release's summary without its text)
- **Registries**: each release as an archive listed with its SHA-256 in a `catalog.toml`, so any
  static host or GitHub's releases serves thousands of books without a repository each: `lx
  registry init`, `lx publish`, `lx yank`, `lx catalog build`, `lx add --registry`; a folder of
  package repositories works as a registry too (`lx catalog add <folder> --name <registry>`)
- **Works** (`work = "kjv"`): editions of one work are grouped in search; `lx works` lists what
  logoxide's table and your catalogs name
- **Media**: audio and video listed in `[media]` by URL, size, and SHA-256, never stored; WebVTT
  timings per recording name where each verse is; `lx media add`, `lx media check`
- Every command: `init`, `new`, `add`, `remove`, `enable`, `disable`, `install`, `update`,
  `list`, `show`, `check`, `release`, `diff`, `verify`, `info`, `clean`, `search`, `works`,
  `import`, `catalog`, `registry`, `publish`, `yank`, `media`, `map`, `convert`, `read`, `compare`, with
  completion (`source <(COMPLETE=bash lx)`)

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
lx add --global github:someone/kjv         # into the library's own logoxide.toml, even in a vault
lx list --all                              # every version in the library, and who uses it

lx new --from book.epub --license CC0-1.0   # a package from one file: an EPUB's title and authors fill it in
lx new my-library --kind collection --name "My Library" --language en   # a package of packages
lx release --set-version                   # in a package's repository: summary, version check, tag
lx diff v1.0.0                             # what changed since a release
lx show --git https://example.org/bsb      # a release's summary, without its text
```

The library is `LOGOXIDE_HOME` (else `$XDG_DATA_HOME/logoxide`), or `--library DIR`: any folder,
including inside a vault's `.obsidian/` so it syncs. The cache is per device: `LOGOXIDE_CACHE`,
else `$XDG_CACHE_HOME/logoxide`. `~/.config/logoxide/config.toml` can set both, and catalogs and
registries for every project. Catalogs downloaded over HTTP are kept in the cache for
`catalog-max-age` (7 days by default; `"12h"`, `"30m"`, or seconds; 0 keeps none): `lx search
--refresh` or `lx catalog refresh` downloads them again, `lx search --offline` uses only what's
kept. Fetching git packages runs the `git` command; where it isn't
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

Rust 1.89 or later. Until the crates are published, build from this
repository with topos and sword-modules checked out beside it (see Development).

## Development

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`. The bindings
(`crates/logoxide-ffi`) are a workspace of their own that builds against a topos checkout beside
this one (`../topos`): BoltFFI only exports the items of path dependencies, and they re-export
topos's API.

- **Versions**: the workspace's version, its `logoxide`, `logoxide-markup`, `logoxide-text`,
  `logoxide-import`, and `logoxide-app` dependencies', and logoxide-ffi's must be
  the same (`scripts/check-versions.sh`, run by CI, and by the release with its tag), now 0.1.0.
- **topos**: the topos tag the bindings build against is `v` and topos-bible's version in
  `Cargo.toml` (`scripts/topos-ref.sh`); both workflows read it from there.
- **Releases**: pushing a `v*` tag runs `.github/workflows/release.yml`: it checks the versions
  against the tag, runs the workspace's and the bindings' tests, builds the wheels and the npm
  package, installs a wheel in a fresh environment and imports it
  (`crates/logoxide-ffi/scripts/smoke-test.py`), runs the Node example against the npm package,
  and then publishes to crates.io, PyPI, and npm with trusted publishing (no stored tokens). Each
  publishes only a version that isn't up yet (`scripts/crates-to-publish.sh` for the crates), so
  a version published by hand is left alone; a crate new to the workspace is published by hand
  once, then trusts the workflow in its settings on crates.io.
- **Third-party notices**: the wheels and the npm package contain logoxide-ffi compiled with its
  dependencies, so each carries a `THIRD-PARTY-NOTICES` file: every crate linked into it for that
  target, with its license and license files, from `cargo metadata`
  (`crates/logoxide-ffi/scripts/third-party-notices.py`, run by `npm run pack:wasm` and
  `npm run pack:python`; the release checks the file is in both).

License: CC0-1.0
