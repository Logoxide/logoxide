# biblix

Bible translations as versioned packages (git repositories with a `biblix.toml`), read from many
formats, cached, shared, and (later) installed; `bx` is the CLI. The plan and the owner's
decisions are in topos's `doc/biblix.md` (`../topos/doc/biblix.md`): read it before changing the
design.

## Layout

- `crates/biblix`: the library
  - `manifest.rs`: `biblix.toml`. One manifest name for packages and vaults (a vault's has no
    `[package]`), like `Cargo.toml`. "Index" means only a registry. Formats, Cargo-style features,
    and `git`/`path` dependencies; no default registry
  - `storage.rs`: `Storage` (read, list, write) over a folder (`FsStorage`), memory
    (`MemoryStorage`), or a host's files in WebAssembly, so nothing else touches `std::fs`
  - `source/`: one `Source` per format, each giving topos-bible-markup's tree (`Book`: nodes and
    verse spans). USX, and USFM through `usfm3` (chosen over usfm-onion, whose USX drops the space
    between adjacent `\w` words; both gave the KJV's 31,102 verses)
  - `package.rs`: opening a package picks the best readable format its enabled features turn on;
    `checksum()` hashes every file but `.git`
  - `cache.rs`: verse counts and parsed books (postcard) per `<id>/<version>-<checksum>`;
    `CachedPackage::add_to` gives topos's `Translations` the name, aliases, and counts
- `crates/bx`: `bx show`, `bx check`

## Decisions

- Its own repository; depends on topos (`../topos`, path dependencies until topos 0.6) and, later,
  sword-modules (`../sword-modules`) for SWORD. topos never depends on biblix
- Licenses are shown, never enforced. No default registry (the owner can't host mirrors)
- One package for apps later: biblix-ffi includes topos's API
- Summaries with verse hashes are published for every package (not only Bibles); the parsed cache
  is never distributed
- Hand-made test packages (BSB from a DBL bundle as USX, KJV from fetch.bible as USFM) are git
  repositories in `~/Development/biblix-packages/`, tagged `v1.0.0`

## Conventions

- Like topos: no `unwrap` in library code, errors in `BiblixError`, comments and docs in plain
  prose, `cargo fmt` and `cargo clippy --all-targets -D warnings` clean
- Check behavior on the real packages (`bx check ~/Development/biblix-packages/kjv`), not only
  unit tests
- Commit messages end with a `Co-Authored-By` line
