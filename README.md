# biblix

Bible translations as versioned packages. A package is a git repository with a `biblix.toml` and
its text in one or more formats; biblix reads it, caches it, and tells
[topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and verse
counts, so references like `John 3:16 KJV` are checked against that translation. `bx` is the
command line.

What works (steps 1 and 2 of the plan, `doc/biblix.md` in topos):

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
- `bx init`, `add`, `remove`, `install`, `update`, `list`, `show`, `check`

Next: summaries (`biblix.summary.toml` and per-verse hashes), topos's LSP using the library, then
biblix-ffi with an archive fetcher for WebAssembly.

## Using it

```sh
bx init                                    # a biblix.toml here (a vault); without one, bx uses the library's
bx add --git https://example.org/bsb       # the newest release; --version 1, --tag, --branch, --rev
bx add --git https://example.org/kjv -F strongs
bx add --path "Bibles/My Translation"      # used in place
bx install                                 # what biblix.lock says, on a new device
bx update [id]                             # newer versions the requirements allow
bx list
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
