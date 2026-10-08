# biblix

Bible translations as versioned packages. A package is a git repository with a `biblix.toml` and
its text in one or more formats; biblix reads it, caches it, and tells
[topos](https://github.com/MasterTemple/topos-bible) the translation's name, aliases, and verse
counts, so references like `John 3:16 KJV` are checked against that translation. `bx` is the
command line.

Status: the first step of the plan (`doc/biblix.md` in topos). What works:

- `biblix.toml`: packages and vaults (a manifest without `[package]`), with formats, features
  (Cargo-style, including `dep:` and `name/feature`), and dependencies (`git =` with `version`,
  `tag`, `rev`, or `branch`; or `path =`)
- Sources: a folder of USX books, and a folder of USFM books (read through
  [`usfm3`](https://crates.io/crates/usfm3) into USX). Book files are found by name (`JHN.usx`,
  `44JHNBSB.usx`, `43-JHN.usfm`) or by the code inside them
- `Storage`: a folder, memory, or (later) a host's files, so the same code runs in WebAssembly
- A cache of verse counts and parsed books, per package version and checksum
- `bx show` and `bx check`

Next: the library (installed versions side by side), `bx add`/`install` with git sources and a
lock file, summaries (`biblix.summary.toml` and per-verse hashes), then biblix-ffi.

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
