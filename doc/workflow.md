# Working with bx

An example of how biblix is meant to be used, from both sides: someone reading Bibles in a vault
(notes, Obsidian, an editor), and someone publishing a package. Every command here runs today,
except those under "Commentaries, books, and other formats", which are a sketch. The URLs are
examples, since no package is hosted yet (see the end).

## Reading: a vault with a few Bibles

A vault is any folder of notes. Its `biblix.toml` says which Bibles it uses, like `Cargo.toml`, and
`biblix.lock` pins their exact text, like `Cargo.lock`. Commit or sync both with the notes.

```sh
cd ~/Notes
bx init                                   # biblix.toml here, without [package]
```

Find a translation. The project's own catalogs are searched first, then eBible.org's and
fetch.bible's lists (about 2,900 translations):

```sh
bx search "american standard"
# ASV (eng-asv): American Standard Version (1901), English, LicenseRef-PublicDomain
#     bx import ebible:eng-asv
# ASV (eng_asv): American Standard Version (1901), English, LicenseRef-PublicDomain
#     bx import fetch:eng_asv
```

Before adding a package published as a git repository, look at its newest release without
downloading its text (a partial clone reads only its summary):

```sh
bx show --git github:someone/kjv
# KJV (King James Version (1769)), LicenseRef-PublicDomain
# kjv 1.0.1: 66 books, 31102 verses
# usfm: 12.5 MB
# contents: note 6959, strongs 349308, words-of-jesus 2038, ...
```

Add what the vault uses. Git packages are installed into the library (`BIBLIX_HOME`, else
`~/.local/share/biblix`) as plain files and locked; imports become path packages in `imports/`:

```sh
bx add github:someone/kjv                                 # the newest release, ^1.0.1
bx add github:someone/bsb --version 1                     # any 1.x
bx import ebible:eng-asv                                  # into imports/eng-asv
bx list
# bsb 1.0.1  BSB (Berean Standard Bible), LicenseRef-PublicDomain  https://github.com/someone/bsb v1.0.1
# eng-asv 2026.10.8  ASV (American Standard Version (1901)), LicenseRef-PublicDomain  path imports/eng-asv
# kjv 1.0.1  KJV (King James Version (1769)), LicenseRef-PublicDomain  https://github.com/someone/kjv v1.0.1
```

An import's prefix is its catalog's kind (`ebible:` for eBible.org's list, `fetch:` for
fetch.bible's), and `bx import` looks it up in the project's catalogs in order, the same ones
`bx search` reads (`bx catalog list`).

The manifest now reads:

```toml
[dependencies]
kjv = { git = "https://github.com/someone/kjv", version = "1.0.1" }
bsb = { git = "https://github.com/someone/bsb", version = "1" }
eng-asv = { path = "imports/eng-asv" }
```

`github:owner/repo` is short for `https://github.com/owner/repo` (`gitlab:` and `codeberg:` work
the same way, GitLab's nested groups too); the manifest and the lock keep the full URL. Any other
URL git reads works too (`--git URL`), and `--path DIR` uses a folder in place.

`bx info kjv` says where it's installed, its size, its books and verses, and whether its files are
still the ones locked.

### Using them

References that name a translation (`John 3:16 KJV`) are checked against that translation's own
verse counts, and its text is shown:

- **An editor** (Neovim through topos-bible.nvim, or any LSP client): `biblix-lsp` finds the
  workspace's `biblix.toml` and loads its Bibles; `bibles = ["bsb", "kjv"]` in
  `~/.config/topos/config.toml` picks some of them. Hover shows the passage, go to definition
  opens the whole book. In Neovim, `:ToposBibleSearch`, `:ToposBibleAdd`, and `:ToposBibleRemove`
  run `bx` for you.
- **Obsidian**: the plugin reads the same `biblix.toml` and `biblix.lock`, with its library in the
  vault (`Biblix/`), so it syncs to mobile. Its settings search, add, and remove packages.

### Forks

Two people's versions of a package (`github:alice/kjv` and `github:bob/kjv`) can be used side by
side. Both are the package `kjv`, so the second needs another name here, and since references
name each Bible by its abbreviation, another abbreviation too:

```sh
bx add github:alice/kjv
bx add github:bob/kjv
# bx: `kjv` is already https://github.com/alice/kjv: give this one another name (--name kjv-2)
bx add github:bob/kjv --name kjv-bob
# bx: `KJV` already names kjv here: give this one another (--abbreviation KJV2, --alias)
bx add github:bob/kjv --name kjv-bob --abbreviation KJVB --alias ""
# added kjv-bob (kjv) 1.0.1 to ./biblix.toml
```

```toml
kjv-bob = { git = "https://github.com/bob/kjv", version = "1.0.1", package = "kjv", abbreviation = "KJVB", aliases = [] }
```

Now `John 3:16 KJV` is Alice's and `John 3:16 KJVB` is Bob's. Each version is installed in its
own folder in the library (`packages/kjv/1.0.1+ga7ff59b2`, its version and commit), so forks at
the same version never mix, even across vaults sharing a library.

### Another device

The lock file says exactly which text the vault uses, so a second device (or a teammate) gets the
same verses, and a changed file is caught by its checksum:

```sh
cd ~/Notes                                # synced: biblix.toml, biblix.lock, imports/
bx install
# bsb 1.0.1
# eng-asv 2026.10.8
# kjv 1.0.1
# 3 packages, locked in ./biblix.lock
```

### Keeping up

```sh
bx update                                 # newer versions the requirements allow (kjv 1.0.1 → 1.1.0)
bx update kjv                             # just one
bx remove bsb                             # out of biblix.toml; its files stay in the library
bx clean                                  # versions no lock here uses (--keep ~/Other/Vault)
bx clean --remove                         # delete them
```

An update never crosses a major version: `version = "1"` stays on 1.x, because a major version
means verses were renumbered or removed, and verse links could point at other text.

## Publishing: a package of your own

A package is a git repository with a `biblix.toml` and its text, released by tags (`v1.0.0`).
Start from a folder of USX or USFM books, or from an import (`imports/eng-asv` is already one):

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
usfm = { path = "usfm/", feature = "usfm" }

[features]
default = ["usfm"]
usfm = []
```

```sh
git init && git add . && git commit -m "The KJV"
bx check .                                # reads every book: 66 books, 31102 verses
bx release                                # summary, version check, commit, tag v1.0.0
git remote add origin https://github.com/someone/kjv && git push --tags origin master
```

`bx release` writes `biblix.summary.toml` (verse counts, sizes, what the text holds) and
`biblix.verses.bin` (a hash of every verse), which is how readers see a release before
downloading it.

### A correction

Say a word in John 3:16 changes. `bx diff` compares the working files with the last release, and
`bx release` refuses a version smaller than the change:

```sh
bx diff v1.0.1
# 1 verse reworded: a Minor change
# reworded JHN 3:16
bx release
# bx: version 1.0.1 isn't newer than the last release, 1.0.1: release this as 1.1.0 or later
bx release --set-version                  # sets 1.1.0, commits, tags v1.1.0
git push --tags origin master
bx verify                                 # anyone can rebuild the summary and compare
```

Notes and markup are a patch, reworded or added verses a minor version, renumbered or removed
verses a major one (before 1.0, minor counts as major, like Cargo).

### Listing it in a catalog

A catalog is a `catalog.toml` anyone can host (a git repository, any URL). `bx catalog entry`
writes a package's entry from its newest release:

```sh
bx catalog entry github:someone/kjv >> catalog.toml
```

Readers add the catalog once, and `bx search` finds the package before eBible.org's and
fetch.bible's:

```sh
bx catalog add https://example.org/bibles/catalog.toml
bx search kjv
```

### Audio

Recordings are listed, never stored: each file's URL, size, and SHA-256, and a WebVTT file of
where each verse is (`timings/JHN.vtt`, cues named `JHN 3:16`):

```sh
bx media add JHN.mp3 --url https://example.org/audio/JHN.mp3 --book JHN
bx media check --offline                  # the timings against the text
bx media check                            # and download and check every file
```

## Commentaries, books, and other formats (a sketch)

What's missing here is biblix's side, not topos's:

- **topos-bible-markup already reads** OSIS, ThML, GBF, and TEI (every markup SWORD modules use)
  into the same tree as USX, and renders it as plain text and HTML. sword-modules reads SWORD
  Bibles, commentaries, lexicons, and general books into that tree.
- **topos already finds references** inside all of these, and inside EPUBs (each match located by
  an EPUB CFI) and PDFs, so a ThML commentary or an EPUB in a vault is searched today.
- **biblix reads Bibles in USX, USFM, OSIS, and ThML** (a whole Bible in one file works: the
  KJV from open-bibles' OSIS and from CCEL's ThML both give 31,102 verses in its 66 books). What
  it doesn't read yet is everything that isn't a Bible: a commentary needs its sections tied to
  the verses they're on, and its own summary (sections, not verse counts). SWORD needs wiring to
  sword-modules. EPUB has no tree in topos-bible-markup (it's XHTML, so it renders as it is).

What's below is how it could look, not what runs.

### One commentary in a format SWORD uses

A commentary is keyed by verse, like a Bible, so it gets the same summary (verse hashes, per
verse) and the same versioning:

```toml
[package]
id = "mhc"
name = "Matthew Henry's Commentary on the Whole Bible"
abbreviation = "MHC"
kind = "commentary"
version = "1.0.0"
language = "en"
license = "LicenseRef-PublicDomain"
upstream = "https://ccel.org/ccel/henry/mhc"
versification = "KJV"

[formats]
thml = { path = "thml/", feature = "thml" }      # CCEL's source, read by topos-bible-markup
sword = { path = "sword/", feature = "sword" }   # the same text as a SWORD module (zCom)

[features]
default = ["thml"]
thml = []
sword = []
```

```sh
bx import sword:MHC                       # from CrossWire's repository, through sword-modules
bx import ccel:henry/mhc                  # CCEL's ThML (downloads need a login: a file instead)
bx import ./mhc.thml --kind commentary    # a file someone already has
```

An EPUB isn't keyed by verse, so it's a `kind = "book"`: its chapters are the units, and the
references topos finds in its text (`John 3:16`) link it to Bibles instead of a verse key.

### Libraries of packages

Each work is its own package (Henry's, Gill's, and Poole's commentaries, each versioned on its own
as its text is corrected), and a library of them is a package of packages: no text, only
optional dependencies, each turned on by a feature, so readers take only what they want.

```toml
[package]
id = "puritan-commentaries"
name = "Puritan Commentaries"
abbreviation = "PURITAN"
kind = "collection"
version = "1.0.0"
language = "en"

[features]
default = ["henry"]
henry = ["dep:mhc"]
gill = ["dep:gill"]
poole = ["dep:poole"]
all = ["henry", "gill", "poole"]

[dependencies]
mhc = { git = "https://github.com/someone/mhc", version = "1", optional = true }
gill = { git = "https://github.com/someone/gill", version = "1", optional = true }
poole = { git = "https://github.com/someone/poole", version = "1", optional = true }
```

```sh
bx add github:someone/puritan-commentaries -F gill --no-default-features
bx add github:someone/puritan-commentaries -F all
```

Only the chosen works are downloaded, since each is its own repository. The lock records the
collection and each package it brought in, with its own commit and checksum. Not working yet: a
package must have `[formats]`, and a project's Bibles are only the packages it names, not what a
collection brings in.

## What isn't there yet

- **Nothing is hosted**: biblix, the KJV and BSB packages, and a catalog of them are local
  repositories, so the examples above use `github:someone/...` where today only
  `file://` URLs exist.
- **Installing bx** needs a topos checkout next to biblix (`cargo install --path crates/bx`)
  until topos 0.6 is released.
- `bx update` doesn't yet say when a newer major version exists; collections, `ver=` links,
  SWORD, Door43, and CCEL imports, and an audio player are later steps (`doc/biblix.md` in
  topos).
