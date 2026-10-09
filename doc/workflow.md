# Working with bx

An example of how biblix is meant to be used, from both sides: someone reading Bibles in a vault
(notes, Obsidian, an editor), and someone publishing a package. Every command here runs today,
except those under "Commentaries and other formats", which are a sketch. The URLs are
examples, since no package is hosted yet (see the end).

## Completion

`bx` completes its own arguments, so it knows the project's registries and packages:
`bx add dorean-local:<Tab>` lists that registry's packages (by path, and by id when no other
package there has it), and `bx remove <Tab>` the project's dependencies. In `~/.bashrc`:

```sh
source <(COMPLETE=bash bx)
# Bash splits words at `:`; without this, `dorean-local:<Tab>` can't complete
COMP_WORDBREAKS=${COMP_WORDBREAKS//:/}
```

(zsh: `source <(COMPLETE=zsh bx)`; fish: `COMPLETE=fish bx | source`.) Registries on the web
aren't downloaded to complete.

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

### A registry of your own

A registry is a catalog with a name, so its packages can be named through it. A folder of
package repositories is one: every git repository with a `biblix.toml` in it, at any depth.

```sh
bx catalog add ~/Documents/biblix-dorean-repos --name dorean-local
bx search --registry dorean-local         # every package in it, and how to add each
bx add dorean-local:dorean-collection     # by its id
bx add dorean-local:books/romans          # by its path in the folder
```

To host a registry, or use one on a phone, build its folder into a `catalog.toml`: each
package's newest release (the `v1.2.3` tag, not its working files), with its path and commit, so
a package whose release hasn't changed keeps its entry next time (`--force` reads them all):

```sh
bx catalog build ~/Documents/biblix-dorean-repos     # writes catalog.toml there
bx catalog build . --url 'https://github.com/someone/{name}' --output hosted.toml
bx catalog add ~/Documents/biblix-dorean-repos/catalog.toml --name dorean-local
```

A folder registry reads each package's newest release too; one not released yet is left out.
An id two packages share (a fork beside the original) has to be named by its path. The
manifest keeps the package's full `file://` URL, so the name is only for typing. A registry can
also be a `catalog.toml` (`--name mine https://example.org/catalog.toml`), whose packages are
named by id. Registries are in `[settings.registries]`, searched before the catalogs.

### Publishing many packages: a registry of archives

A repository per package doesn't scale to thousands of books that rarely change. Instead, keep
them together in one repository (a folder each) and publish each release as an archive: one
file, its SHA-256 in the registry's `catalog.toml`, downloaded alone when installed.

```sh
# One repository, a folder per package; each is released on its own
cd dorean-epubs/the-dorean-principle && bx release       # tags the-dorean-principle/v1.0.2

# Once: a registry, on GitHub's releases (the repository holds no files) or in a folder
bx registry init github:MasterTemple/dorean-epubs        # or: bx registry init ~/registry
# Each release: its files at its tag, packed (the same files always make the same archive)
bx publish the-dorean-principle --to github:MasterTemple/dorean-epubs
```

`bx registry init` prints the catalog's URL. Readers name the registry once and add by id:

```sh
bx catalog add https://github.com/MasterTemple/dorean-epubs/releases/download/index/catalog.toml --name dorean
bx add dorean:the-dorean-principle      # { registry = "https://…/catalog.toml", version = "1.0.2" }
bx add --registry https://…/catalog.toml the-dorean-principle   # without naming it
```

The lock pins the release's archive and hash, so another device downloads that file alone (not
even the catalog), and an archive that changed is refused. A published version never changes:
publishing it again with other files is an error. A folder registry can be served by any static
host (a web server, GitHub Pages, an S3 bucket). The Obsidian plugin installs from registries
on the web too, phones included; a collection's packages can come from a registry
(`{ registry = …, version = … }` in its `[dependencies]`).

### Audio

Recordings are listed, never stored: each file's URL, size, and SHA-256, and a WebVTT file of
where each verse is (`timings/JHN.vtt`, cues named `JHN 3:16`):

```sh
bx media add JHN.mp3 --url https://example.org/audio/JHN.mp3 --book JHN
bx media check --offline                  # the timings against the text
bx media check                            # and download and check every file
```

## Books and collections

A book (an EPUB) is a package like a Bible, but its text isn't read as verses: biblix keeps its
file, and topos finds the references in it (each located by an EPUB CFI). `bx new --from` makes
one from the file, filling in what the EPUB says about itself (its title, subtitle, every author,
language, and publisher); flags give the rest, and win over the file:

```sh
cd ~/Documents/biblix-dorean-repos
bx new --from epubs/the-dorean-principle.epub --version 1.0.0 --license CC0-1.0
# created the-dorean-principle (Book) in the-dorean-principle
```

```toml
# Made by biblix from the-dorean-principle.epub
[package]
id = "the-dorean-principle"
name = "The Dorean Principle"
abbreviation = "TDP"
authors = ["Conley Owens"]
version = "1.0.0"
kind = "book"
language = "en-US"
license = "CC0-1.0"

[formats]
epub = { path = "epub/" }
```

```sh
cd the-dorean-principle && bx release     # commits and tags v1.0.0
```

The id comes from the title (`--id`, or the folder's name: `bx new my-folder --from …`), the
abbreviation from its initials (`--abbreviation`), and `--author` (repeated, in order),
`--name`, `--description`, `--language`, `--kind`, `--publisher`, and `--homepage` set the
others. An EPUB names no license biblix can trust, so give one (`--license`). A book has no verses
to compare, so `bx release` compares its files: any change is a minor version.

Its release also publishes the passages topos finds it citing, with how often
(`biblix.references.bin`, a few KB), so which books cite a passage can be known without
downloading them. Where each reference is (its chapter, paragraph, and an EPUB CFI readers open
straight to) comes from the book itself, in a tenth of a second:

```sh
bx show --git github:someone/the-dorean-principle --references | grep "^Matt.10.8"
# Matt.10.8 (7)
# Matt.10.8-Matt.10.10 (5)
bx show the-dorean-principle --references --locations | grep "^Matt.10.8 "
# Matt.10.8  [Foreword]  Since 2006, FirstLove Publications has given away over 300,000 copies of publica…
# Matt.10.8  [1. The Command of Christ]  Jesus forbids ministerial reciprocity in Mat­thew 10:8 when he commands his disc…
```

`--format tsv` or `json` gives every field (with `--locations`: the CFI, offsets, line, and
column).

### Your files beside a package's

A package is the files its manifest declares: `biblix.toml`, each format's own files (`usx/*.usx`,
`epub/*.epub`), the license file, the summary, media timings, and what `include = ["…"]` names.
Anything else in its folder (an annotation File++ writes beside a book, `JHN - Annotations.md`)
isn't part of it: it doesn't change the checksum, isn't removed by `bx clean` (which keeps that
version's folder, and says why), and stays through a repair. Editing a package's own files is
caught:

```sh
bx install
# bx: bsb 1.0.1: its files in packages/bsb/1.0.1+gcd07745e changed since it was locked (…): `bx install --repair` puts them back
bx info bsb
# checksum: sha256:abe3… (CHANGED: locked as sha256:0e61…; bx install --repair puts its files back)
# not the package's (kept): usx/JHN - Annotations.md
bx install --repair
# repaired bsb 1.0.1
```

A **collection** is a package of packages: no text of its own, only dependencies. Each work stays
its own package, versioned on its own, and the collection gathers them:

```sh
bx new dorean-collection --kind collection --name "The Dorean Collection" \
  --abbreviation DOREAN --language en --license CC0-1.0 --version 1.0.0
bx add --project dorean-collection github:someone/the-dorean-principle
bx add --project dorean-collection github:someone/the-doctrine-of-simony
# … and the others
cd dorean-collection && bx release
```

A vault that adds the collection gets every package in it, locked one by one, and the Bibles in a
collection are the vault's Bibles:

```sh
bx add github:someone/dorean-collection
bx list
# abolish-the-jesus-trade 1.0.0  ATJT (Abolish the Jesus Trade), CC0-1.0  …  (a dependency)
# dorean-collection 1.0.0  DOREAN (The Dorean Collection), CC0-1.0  …
# …
```

To let readers take only some of a collection, make its dependencies optional, each turned on by
a feature (edit its `biblix.toml`; `bx add` writes plain dependencies):

```toml
[features]
default = ["dorean-principle"]
dorean-principle = ["dep:the-dorean-principle"]
simony = ["dep:the-doctrine-of-simony"]
all = ["dorean-principle", "simony"]

[dependencies]
the-dorean-principle = { git = "https://github.com/someone/the-dorean-principle", version = "1", optional = true }
the-doctrine-of-simony = { git = "https://github.com/someone/the-doctrine-of-simony", version = "1", optional = true }
```

```sh
bx add github:someone/dorean-collection -F simony --no-default-features
```

## Commentaries and other formats (a sketch)

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
  sword-modules.

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

## What isn't there yet

- **Nothing is hosted**: biblix, the KJV and BSB packages, and a catalog of them are local
  repositories, so the examples above use `github:someone/...` where today only
  `file://` URLs exist.
- **Installing bx** needs a topos checkout next to biblix (`cargo install --path crates/bx`)
  until topos 0.6 is released.
- `bx update` doesn't yet say when a newer major version exists; `ver=` links,
  SWORD, Door43, and CCEL imports, and an audio player are later steps (`doc/biblix.md` in
  topos).
