# logoxide

Bibles, commentaries, lexicons, books, and media as versioned packages. A package is a git
repository (or a registry's release archive) with a `logoxide.toml` and its text in one or more
formats; logoxide reads it, caches it, and tells
[topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and verse
counts, so references like `John 3:16 KJV` are checked against that translation.

[crates.io](https://crates.io/crates/logoxide) · [npm](https://www.npmjs.com/package/logoxide) ·
[PyPI](https://pypi.org/project/logoxide/) · [docs.rs](https://docs.rs/logoxide)

## Quick start

```sh
cargo install logoxide-cli    # the `lx` command
cd ~/Notes
lx init                       # a logoxide.toml here: this folder is now a vault
lx search "king james"        # eBible.org, fetch.bible, CCEL, and CrossWire's SWORD lists
lx import ebible:eng-kjv      # a publisher's download, made into a package and locked
lx read "John 3:16 KJV"
lx install                    # on another device: exactly what logoxide.lock says
```

`logoxide.toml` says which packages a vault uses and `logoxide.lock` pins their exact text; commit
or sync both with your notes. [crates/lx/README.md](crates/lx/README.md) is `lx` by task, and
[doc/workflow.md](doc/workflow.md) walks through a vault and a package of your own.

Building an app? [doc/apps.md](doc/apps.md) shows how to find, download, and read Bibles in
TypeScript, Python, and Rust: a book at a time from fetch.bible, a whole Bible from eBible.org, or
the files your user brings. [doc/recipes/](doc/recipes/README.md) goes on from there: passages as
HTML, uploads your user renames and adds to, references in text, parallel translations, search, commentaries, audio, a passage server.

An AI coding assistant can do the setting up: install the logoxide skills (an app's Bibles, or
`lx` and a vault with its editor and Claude integrations) in Claude Code with
`/plugin install logoxide --marketplace Logoxide/logoxide`, or copy them into another
assistant's skills folder ([plugins/logoxide](plugins/logoxide/README.md)).

**Licenses are shown, never enforced.** Each text's license is printed as its publisher states
it, but logoxide installs and reads a text whatever it says. Before you redistribute a text, check
that you have the right to: many translations are under copyright and allow only personal use.

## Packages

| | Install | What it is |
|---|---|---|
| [`lx`](crates/lx/README.md) | [`cargo install logoxide-cli`](https://crates.io/crates/logoxide-cli) | the command line |
| [logoxide-lsp](crates/logoxide-lsp/README.md) | [`cargo install logoxide-lsp`](https://crates.io/crates/logoxide-lsp) | a language server: references' hover, completion, and passages in any editor |
| [logoxide-mcp](crates/logoxide-mcp/README.md) | [`cargo install logoxide-mcp`](https://crates.io/crates/logoxide-mcp) | an MCP server: catalogs, passages with their licenses, notes, entries |
| [bindings](crates/logoxide-ffi/README.md) | [`npm install logoxide`](https://www.npmjs.com/package/logoxide) · [`pip install logoxide`](https://pypi.org/project/logoxide/) | logoxide and topos's API for JavaScript (WebAssembly) and Python |
| [logoxide](crates/logoxide/README.md) | [crates.io](https://crates.io/crates/logoxide) | packages, the library, the lock, catalogs; never reads text itself |
| [logoxide-app](crates/logoxide-app/README.md) | [crates.io](https://crates.io/crates/logoxide-app) | what every front end does, behind one `Logoxide` facade |
| [logoxide-import](crates/logoxide-import/README.md) | [crates.io](https://crates.io/crates/logoxide-import) | publishers' downloads as packages: eBible.org, fetch.bible, CCEL, SWORD, Door43, Burritos, DBL |
| [logoxide-text](crates/logoxide-text/README.md) | [crates.io](https://crates.io/crates/logoxide-text) | what's in a Bible or a book: verses, hashes, titles, versification maps, citations |
| [logoxide-markup](crates/logoxide-markup/README.md) | [crates.io](https://crates.io/crates/logoxide-markup) | OSIS, ThML, GBF, TEI, and USX as one tree; text and HTML |
| [logoxide-quotes](crates/logoxide-quotes/README.md) | not published yet | every quotation of Scripture in a text or a transcript |

The Obsidian plugin, `obsidian-logoxide`, is built on the npm package. Rust 1.89 or later.

<details>
<summary><strong>What works</strong></summary>

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
  Strong's among them), grouped by the work each is an edition of
- **Reading**: `lx read`, and `lx compare` with each Bible in its own numbering through the
  vault's versification maps, saying what one lacks; commentaries' notes on a passage, a
  dictionary's entries, and a verse's cross-references; text or JSON
- **Links that survive updates**: every release ships `logoxide.verses.bin`, a hash of each verse's
  words, so a link made against one version (`&ver=1.0.2&vh=9f86d081`) finds its verse again after
  a renumbering and says when the words changed. `lx diff` and `lx verify` read the same hashes
- **Formats read**: USX, USFM, USJ, OSIS, ThML, GBF (a book a file, or a whole Bible in one);
  dictionaries (`entries/*.xml`) and cross-references (`xref/*.tsv`); books as EPUBs, with the
  references topos finds in them; Paratext `.vrs` versification maps; `lx convert` between USFM,
  USX, and USJ

</details>

<details>
<summary><strong>Also: publishing</strong></summary>

For someone making or hosting packages. These work and are specified in `doc/`, but nobody hosts
one yet, so they're kept as they are until someone asks:

- **Packages**: `lx new` (`--from book.epub` fills `[package]` from the file; `--kind collection`
  for a package of packages), `lx release` (writes the summary, checks the version against what
  changed since the last release: patch for notes and markup, minor for wording, additions, or
  titles, major for verses removed, renumbered, or joined; then commits and tags), `lx check`
- **Registries**: each release as an archive listed with its SHA-256 in a `catalog.toml`, so any
  static host or GitHub's releases serves thousands of books without a repository each: `lx
  registry init`, `lx publish`, `lx yank`, `lx catalog build`, `lx add --registry`
- **Works** (`work = "kjv"`): editions of one work are grouped in search; `lx works` lists them
- **Media**: audio and video listed in `[media]` by URL, size, and SHA-256, never stored; WebVTT
  timings per recording name where each verse is; `lx media add`, `lx media check`

A package's `logoxide.toml`:

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

</details>

<details>
<summary><strong>Specifications</strong></summary>

For anyone writing another implementation, or publishing packages and catalogs by hand:

- [doc/manifest.md](doc/manifest.md): `logoxide.toml`, every key, kinds, formats, features,
  dependencies, `[media]`, `[titles]`, categories, and what a package is made of
- [doc/catalog.md](doc/catalog.md): `catalog.toml`, registries, `[[package.release]]`, how
  release archives are packed, and the checksum
- [doc/lock.md](doc/lock.md): `logoxide.lock`
- [doc/summaries.md](doc/summaries.md): `logoxide.summary.toml`, `logoxide.verses.bin` (`LXV2`),
  `logoxide.references.bin` (`LXR1`), and how changes decide version numbers
- [doc/entries.md](doc/entries.md): dictionaries' entries and cross-references
- [doc/trust.md](doc/trust.md): what is checked, what is trusted on first use, and what isn't

</details>

<details>
<summary><strong>Development</strong></summary>

`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`. The crate map is
[doc/architecture.md](doc/architecture.md). The bindings (`crates/logoxide-ffi`) are a workspace
of their own that builds against a topos checkout beside this one (`../topos`): BoltFFI only
exports the items of path dependencies, and they re-export topos's API.

- **Versions**: the workspace's version, its `logoxide`, `logoxide-markup`, `logoxide-text`,
  `logoxide-import`, and `logoxide-app` dependencies', and logoxide-ffi's must be
  the same (`scripts/check-versions.sh`, run by CI, and by the release with its tag), now 0.1.2.
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

</details>

License: CC0-1.0
