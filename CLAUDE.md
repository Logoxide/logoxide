# logoxide

Bibles, commentaries, lexicons, books, and media as versioned packages (git repositories, or a
registry's archives, with a `logoxide.toml`), read from many formats, cached, shared, and
installed; `lx` is the CLI. The owner's decisions are in topos's `doc/logoxide.md`
(`../topos/doc/logoxide.md`): read it before changing the design.

## Where things are

- **Plans**: `PLAN-REVIEW.md` is current (the 2026-10-14 review: decisions R1–R7, phases R-A to
  R-G; R-A, shipping, is the owner's and gates the rest). Before it, in order: `PLAN.md` (the
  2026-10-09 review), `PLAN-CRATES.md` (the crate layout), `PLAN-NEXT.md` (N1–N8, phases G–P),
  all done but the owner's items. `PLAN-CATALOGS.md` is deferred whole (R2).
- **The crate map**: `doc/architecture.md`, a paragraph per module, kept current until phase
  R-E folds it into each crate's rustdoc and README. Read the crate's paragraph before changing
  it, and change the paragraph with the code.
- **Specifications** for other implementations: `doc/manifest.md`, `catalog.md`, `lock.md`,
  `summaries.md`, `entries.md`, `trust.md`; `doc/workflow.md` is the user's walk-through.
  Change them with the code. They bind once anything is published.
- **CI**: `.github/workflows/ci.yml` (fmt, test, clippy, the no-`read` build, msrv, the
  bindings against a topos checkout at `TOPOS_REF`); `release.yml` on a `v*` tag. `scripts/`
  has its helpers (`check-versions.sh`, `topos-ref.sh`, `patch-unpublished.sh`).
- **Beside this repository**: `../topos` (references; never depends on logoxide),
  `../sword-modules` (clean-room: read its `CLAUDE.md` and `CLEANROOM.md` first; interface into
  its `docs/spec/api.md` before code; never read libsword), `~/Development/logoxide-packages/`
  (the hand-made KJV and BSB, git repositories tagged), `~/Development/nvim-plugins/logoxide.nvim`,
  and the Obsidian plugin `obsidian-logoxide`.

## Layers

Arrows point down; nothing depends on what's above it.

```
topos-bible           references: parsing, search, Translations (names, aliases, counts)
logoxide-markup       OSIS, ThML, GBF, TEI, USX, plain text into one tolerant tree; text and HTML
logoxide-text         what's in a Bible or a book: verses, hashes, titles, .vrs maps, timings,
                      passages as text, citations, the parsed-book cache; knows no packages
sword-modules         SWORD containers, read from memory (WebAssembly too)
logoxide              packages, the library, the lock, catalogs, summaries as data; never reads
                      text: asks a `reader::TextReader` (`read` feature: the adapter onto logoxide-text)
logoxide-import       publishers' lists and downloads into packages (eBible.org, fetch.bible,
                      CCEL, SWORD, Door43)
logoxide-app          what every front end does: catalogs over HTTP (cached), imports, installs
                      with or without git, clean, media, read, compare; one `Logoxide` facade
lx · logoxide-ffi · logoxide-mcp · logoxide-lsp
```

The boundaries that matter most:

- **The core never reads text.** `cargo check -p logoxide --no-default-features --all-targets`
  must build (CI runs it). Everything parsing a format is behind `read`; the rest asks a
  `TextReader` for facts (`counts`, `facts`, `timing_problems`).
- **Nothing but `storage.rs` touches `std::fs`.** `Storage` is a folder, memory, or a host's
  files, so the same code runs in WebAssembly; writes go through `write_atomic`.
- **Fetching is behind `Fetcher`** (`git.rs`: the `git` command; logoxide-app's `NetFetcher`:
  smart-HTTP refs and archives for hosts without git; the FFI's rounds: whatever the host gives).
- **logoxide-ffi is a workspace of its own** (`crates/logoxide-ffi`, excluded from the root):
  BoltFFI only exports path dependencies' items, so it re-exports topos-ffi through paths into
  `../topos`. Its library is `logoxide`, so it depends on the core as `logoxide_core`. Build and
  test it from main, not a worktree. Python names each method `_boltffi_<class>_<method>` in
  snake case, so no two may meet that way.
- **logoxide-markup is a clean-room rewrite**: change `docs/spec/` first, then the code; golden
  files and `NO_PANIC_ROUNDS=100000 cargo test --release -p logoxide-markup --test no_panic`
  check it; never read libsword or other SWORD readers' code.

## Decisions

The owner's, dated in the plans; the ones an agent most needs:

- Its own repository (github.com/Logoxide/logoxide), depending on topos 0.6 and sword-modules
  0.1 from crates.io. Until they're published, `~/Development/.cargo/config.toml` (outside every
  repository) patches in `../topos` and `../sword-modules`. The CLI's package is `logoxide-cli`
  (`lx` on crates.io is taken), its binary `lx`. Nothing is pushed or published without the owner.
- The first release is for a reader with a vault (R1); publisher-side tools (registries, works,
  catalogs of catalogs, media publishing) stay as they are and don't grow until someone asks.
- Licenses are shown, never enforced. No default registry (the owner can't host mirrors). Five
  publishers' lists are searched by default (`DEFAULT_CATALOGS`); a package's `import` is found
  only in the project's catalogs.
- One manifest name, `logoxide.toml`, for packages and vaults (a vault's has no `[package]`).
  "Index" means only a registry. Cargo's shapes: features, `dep:`, `package =` for forks,
  `git`/`registry`/`path`/`import` dependencies, `enabled = false`.
- The package manager never reads text (2026-10-10); summaries with every verse's hash are
  published for every kind of package; the parsed cache is never distributed.
- Verse hashes are rule 1 forever (D3: `plain_text`, NFC, a few invisible characters dropped,
  whitespace collapsed; `tests/verse_hash.rs` pins it); a change means rule 2, never rewording.
- Imports are `1.0.0` at first, the publisher's version in `[package.upstream]` (D1); kept in the
  shared library by checksum (`<version>+i<8 hex>`); `--vendor` makes a path package instead.
- Audio and video are referenced, never stored (URL, size, SHA-256, WebVTT timings a file).
- `about-books`, `work`, and a dictionary's `keys` are said by the author, never derived.
- Door43 was written while git.door43.org didn't answer; check it live when it does.

## Conventions

- Like topos: no `unwrap` in library code; errors in `LogoxideError` (`#[non_exhaustive]`;
  callers branch on `kind()`); comments and docs in plain prose; `cargo fmt` and `cargo clippy
  --all-targets -- -D warnings` clean.
- Each change is one commit with its docs: the crate's paragraph in `doc/architecture.md`, the
  specification in `doc/` it touches, the crate's README. Commit messages end with a
  `Co-Authored-By` line. Commit when asked; never push or publish.
- Check on the real packages, not only unit tests: `lx check ~/Development/logoxide-packages/kjv`,
  `lx verify` on the KJV and BSB. The checks for every step: `cargo fmt --check`, `cargo test
  --workspace`, clippy, the no-`read` build, and the FFI's tests from main.
- After changing logoxide, reinstall what the owner runs: `cargo install --path crates/lx`,
  `crates/logoxide-lsp`, `crates/logoxide-mcp`.
- Everything lx prints but JSON goes through `outln!`/`errln!` (`text::printable`), so catalogs'
  and manifests' text can't drive the terminal; the MCP wraps untrusted text in `untrusted`
  objects. Keep that when adding output.
- The minimum Rust is 1.89 (`std::fs::File::lock`); edition 2024.
