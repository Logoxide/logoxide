# Working with lx

An example of how logoxide is meant to be used, from both sides: someone reading Bibles in a vault
(notes, Obsidian, an editor), and someone publishing a package. Every command here runs today,
except those under "Commentaries and other formats", which are a sketch. The URLs are
examples, since no package is hosted yet (see the end).

## Completion

`lx` completes its own arguments, so it knows the project's registries and packages:
`lx add dorean-local:<Tab>` lists that registry's packages (by path, and by id when no other
package there has it), and `lx remove <Tab>` the project's dependencies. In `~/.bashrc`:

```sh
source <(COMPLETE=bash lx)
# Bash splits words at `:`; without this, `dorean-local:<Tab>` can't complete
COMP_WORDBREAKS=${COMP_WORDBREAKS//:/}
```

(zsh: `source <(COMPLETE=zsh lx)`; fish: `COMPLETE=fish lx | source`.) Registries on the web
aren't downloaded to complete.

## Reading: a vault with a few Bibles

A vault is any folder of notes. Its `logoxide.toml` says which Bibles it uses, like `Cargo.toml`, and
`logoxide.lock` pins their exact text, like `Cargo.lock`. Commit or sync both with the notes.

```sh
cd ~/Notes
lx init                                   # logoxide.toml here, without [package]
```

Find a translation. The project's own catalogs are searched first, then four publishers' own
lists of their downloads: eBible.org's and fetch.bible's (about 2,900 translations), CCEL's
Bibles (about 50, as ThML), and CrossWire's SWORD repository (about 250 Bibles and 40
commentaries whose markup is OSIS or ThML):

```sh
lx search "american standard"
# ASV (eng-asv): American Standard Version (1901), English, LicenseRef-PublicDomain
#     lx import ebible:eng-asv
# ASV (eng_asv): American Standard Version (1901), English, LicenseRef-PublicDomain
#     lx import fetch:eng_asv
# ASV (sword-asv): American Standard Version (1901), en, LicenseRef-PublicDomain
#     lx import sword:crosswire/ASV
# …
# ASV (ccel-asv): American Standard Version of the Holy Bible (1901), und, no license given
#     lx import ccel:asv
```

CCEL's page of its Bibles gives only their titles (`und` for the language until it's imported:
each file's head says its language, rights, and version).

Before adding a package published as a git repository, look at its newest release without
downloading its text (a partial clone reads only its summary):

```sh
lx show --git github:someone/kjv
# KJV (King James Version (1769)), LicenseRef-PublicDomain
# kjv 1.0.1: 66 books, 31102 verses
# usfm: 12.5 MB
# contents: note 6959, strongs 349308, words-of-jesus 2038, ...
```

Add what the vault uses. Git packages are installed into the library (`LOGOXIDE_HOME`, else
`~/.local/share/logoxide`) as plain files and locked; imports become path packages in `imports/`:

```sh
lx add github:someone/kjv                                 # the newest release, ^1.0.1
lx add github:someone/bsb --version 1                     # any 1.x
lx import ebible:eng-asv                                  # into imports/eng-asv
lx list
# bsb 1.0.1  BSB (Berean Standard Bible), LicenseRef-PublicDomain  https://github.com/someone/bsb v1.0.1
# eng-asv 2026.10.8  ASV (American Standard Version (1901)), LicenseRef-PublicDomain  path imports/eng-asv
# kjv 1.0.1  KJV (King James Version (1769)), LicenseRef-PublicDomain  https://github.com/someone/kjv v1.0.1
```

An import's prefix is its catalog's kind (`ebible:` for eBible.org's list, `fetch:` for
fetch.bible's, `ccel:` for CCEL's, `sword:<repository>/<module>` for a SWORD repository's), and
`lx import` looks it up in the project's catalogs in order, the same ones `lx search` reads
(`lx catalog list`). The other SWORD repositories CrossWire lists are known by name, so
`lx import sword:crosswire-beta/<module>` works without adding their catalog; any other is added
by its `mods.d.tar.gz` (`lx catalog add https://example.org/sword/mods.d.tar.gz`).

```sh
lx import ccel:kjv              # CCEL's ThML, a whole Bible in one file, with the Apocrypha
lx import sword:crosswire/KJV   # the module's OSIS entries, a file a book (osis/John.xml)
lx import sword:crosswire/MHCC  # a commentary: kind = "commentary", an entry a span of verses
```

Each import's version is its publisher's: eBible.org's update date (`2026.10.8`), CCEL's edition
(`1.0.0`, else the file's date), a SWORD module's `Version` (`3.1.0`). Its license is shown as
the publisher states it (public domain, a Creative Commons license, the GPL), else
`LicenseRef-<publisher>` with the statement in the package's license file. Locked SWORD modules
and those in GBF or plain text aren't listed: logoxide reads OSIS and ThML.

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

`lx info kjv` says where it's installed, its size, its books and verses, and whether its files are
still the ones locked.

### Using them

References that name a translation (`John 3:16 KJV`) are checked against that translation's own
verse counts, and its text is shown:

- **An editor** (Neovim through logoxide.nvim, or any LSP client): `logoxide-lsp` finds the
  workspace's `logoxide.toml` and loads its Bibles; `bibles = ["bsb", "kjv"]` in
  `~/.config/topos/config.toml` picks some of them. Hover shows the passage, go to definition
  opens the whole book. In Neovim, `:ToposBibleSearch`, `:ToposBibleAdd`, and `:ToposBibleRemove`
  run `lx` for you.
- **Obsidian**: the plugin reads the same `logoxide.toml` and `logoxide.lock`, with its library in the
  vault (`Logoxide/`), so it syncs to mobile. Its settings search, add, and remove packages.

### Forks

Two people's versions of a package (`github:alice/kjv` and `github:bob/kjv`) can be used side by
side. Both are the package `kjv`, so the second needs another name here, and since references
name each Bible by its abbreviation, another abbreviation too:

```sh
lx add github:alice/kjv
lx add github:bob/kjv
# lx: `kjv` is already https://github.com/alice/kjv: give this one another name (--name kjv-2)
lx add github:bob/kjv --name kjv-bob
# lx: `KJV` already names kjv here: give this one another (--abbreviation KJV2, --alias)
lx add github:bob/kjv --name kjv-bob --abbreviation KJVB --alias ""
# added kjv-bob (kjv) 1.0.1 to ./logoxide.toml
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
cd ~/Notes                                # synced: logoxide.toml, logoxide.lock, imports/
lx install
# bsb 1.0.1
# eng-asv 2026.10.8
# kjv 1.0.1
# 3 packages, locked in ./logoxide.lock
```

### Keeping up

```sh
lx update                                 # newer versions the requirements allow (kjv 1.0.1 → 1.1.0)
lx update kjv                             # just one
lx remove bsb                             # out of logoxide.toml; its files stay in the library
lx clean                                  # versions no lock here uses (--keep ~/Other/Vault)
lx clean --remove                         # delete them
```

An update never crosses a major version: `version = "1"` stays on 1.x, because a major version
means verses were renumbered or removed, and verse links could point at other text.

Imports aren't followed by `lx update` (they're path packages: nothing upstream is locked).
`lx import --update` makes each one again from its publisher's download and replaces those that
changed, saying how (a reader's files beside them stay):

```sh
lx import --update --dry-run              # what would change
# ccel-darby 1.0.0 → 1.1.0: 1 verse reworded (a Minor change, the version raised for it: the publisher's isn't newer)
# eng-asv 2026.10.8: up to date (ebible:eng-asv)
lx import --update                        # replace the changed ones and lock them again
lx import --update eng-asv                # just one (its name, or ebible:eng-asv)
```

eBible.org's and SWORD imports are downloaded only when the catalog lists a newer version.
fetch.bible's and CCEL's are downloaded each time, since their texts change without a new
version; when one did, its version is raised by what changed (a reworded verse is minor), so
one version never names two texts.

## Publishing: a package of your own

A package is a git repository with a `logoxide.toml` and its text, released by tags (`v1.0.0`).
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
lx check .                                # reads every book: 66 books, 31102 verses
lx release                                # summary, version check, commit, tag v1.0.0
git remote add origin https://github.com/someone/kjv && git push --tags origin master
```

`lx release` writes `logoxide.summary.toml` (verse counts, sizes, what the text holds) and
`logoxide.verses.bin` (a hash of every verse), which is how readers see a release before
downloading it.

### A correction

Say a word in John 3:16 changes. `lx diff` compares the working files with the last release, and
`lx release` refuses a version smaller than the change:

```sh
lx diff v1.0.1
# 1 verse reworded: a Minor change
# reworded JHN 3:16
lx release
# lx: version 1.0.1 isn't newer than the last release, 1.0.1: release this as 1.1.0 or later
lx release --set-version                  # sets 1.1.0, commits, tags v1.1.0
git push --tags origin master
lx verify                                 # anyone can rebuild the summary and compare
```

Notes and markup are a patch, reworded or added verses a minor version, renumbered or removed
verses a major one (before 1.0, minor counts as major, like Cargo).

### Listing it in a catalog

A catalog is a `catalog.toml` anyone can host (a git repository, any URL). `lx catalog entry`
writes a package's entry from its newest release:

```sh
lx catalog entry github:someone/kjv >> catalog.toml
```

Readers add the catalog once, and `lx search` finds the package before eBible.org's and
fetch.bible's:

```sh
lx catalog add https://example.org/bibles/catalog.toml
lx search kjv
```

### A registry of your own

A registry is a catalog with a name, so its packages can be named through it. A folder of
package repositories is one: every git repository with a `logoxide.toml` in it, at any depth.

```sh
lx catalog add ~/Documents/logoxide-dorean-repos --name dorean-local
lx search --registry dorean-local         # every package in it, and how to add each
lx add dorean-local:dorean-collection     # by its id
lx add dorean-local:books/romans          # by its path in the folder
```

To host a registry, or use one on a phone, build its folder into a `catalog.toml`: each
package's newest release (the `v1.2.3` tag, not its working files), with its path and commit, so
a package whose release hasn't changed keeps its entry next time (`--force` reads them all):

```sh
lx catalog build ~/Documents/logoxide-dorean-repos     # writes catalog.toml there
lx catalog build . --url 'https://github.com/someone/{name}' --output hosted.toml
lx catalog add ~/Documents/logoxide-dorean-repos/catalog.toml --name dorean-local
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
cd dorean-epubs/the-dorean-principle && lx release       # tags the-dorean-principle/v1.0.2

# Once: a registry, on GitHub's releases (the repository holds no files) or in a folder
lx registry init github:MasterTemple/dorean-epubs        # or: lx registry init ~/registry
# Each release: its files at its tag, packed (the same files always make the same archive)
lx publish the-dorean-principle --to github:MasterTemple/dorean-epubs
```

`lx registry init` prints the catalog's URL. Readers name the registry once and add by id:

```sh
lx catalog add https://github.com/MasterTemple/dorean-epubs/releases/download/index/catalog.toml --name dorean
lx add dorean:the-dorean-principle      # { registry = "https://…/catalog.toml", version = "1.0.2" }
lx add --registry https://…/catalog.toml the-dorean-principle   # without naming it
```

`lx publish --all` publishes every package in the repository whose version is released and
isn't published yet. A bad release can be yanked (`lx yank the-dorean-principle 1.0.3 --from
github:MasterTemple/dorean-epubs`): new installs and updates pass over it, while lock files that
have it still install it; `--undo` puts it back.

The lock pins the release's archive and hash, so another device downloads that file alone (not
even the catalog), and an archive that changed is refused. A published version never changes:
publishing it again with other files is an error. A folder registry can be served by any static
host (a web server, GitHub Pages, an S3 bucket). The Obsidian plugin installs from registries
on the web too, phones included; a collection's packages can come from a registry
(`{ registry = …, version = … }` in its `[dependencies]`).

Anyone can mirror a release: add `mirrors = ["https://…/the-dorean-principle-1.0.2.tar.gz"]` to
its `[[package.release]]` in `catalog.toml` (relative to the catalog, like `url`). Installing
tries `url`, then each mirror, and takes the first whose bytes hash as listed; the lock keeps the
mirrors too.

### Titles

Each file a reader sees has a title, which apps name it by (the Obsidian plugin's
`Logoxide/bsb/John.bible`): a Bible book's short name in the file (`\toc2`, else `\h`), an EPUB's
title, else the book's name in English. `[titles]` in `logoxide.toml` gives one in its place
(`"usx/JHN.usx" = "Juan"`). `lx check` lists them and refuses a file without one, and a changed
title is a major change, since links name the file by it.

### Audio

Recordings are listed, never stored: each file's URL, size, and SHA-256, and a WebVTT file of
where each verse is (`timings/JHN.vtt`, cues named `JHN 3:16`):

```sh
lx media add JHN.mp3 --url https://example.org/audio/JHN.mp3 --url https://archive.org/download/bsb-audio/JHN.mp3 --book JHN
lx media check --offline                  # the timings against the text
lx media check                            # and download and check every file, from every URL
```

A second `--url` (and more) is a mirror (`mirrors = […]`): players try `url`, then each mirror in
turn. The hash makes any copy as good as the first, so a file can be served from anywhere.

## Books and collections

A book (an EPUB) is a package like a Bible, but its text isn't read as verses: logoxide keeps its
file, and topos finds the references in it (each located by an EPUB CFI). `lx new --from` makes
one from the file, filling in what the EPUB says about itself (its title, subtitle, every author,
language, and publisher); flags give the rest, and win over the file:

```sh
cd ~/Documents/logoxide-dorean-repos
lx new --from epubs/the-dorean-principle.epub --version 1.0.0 --license CC0-1.0
# created the-dorean-principle (Book) in the-dorean-principle
```

```toml
# Made by logoxide from the-dorean-principle.epub
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
cd the-dorean-principle && lx release     # commits and tags v1.0.0
```

The id comes from the title (`--id`, or the folder's name: `lx new my-folder --from …`), the
abbreviation from its initials (`--abbreviation`), and `--author` (repeated, in order),
`--name`, `--description`, `--language`, `--kind`, `--publisher`, and `--homepage` set the
others. An EPUB names no license logoxide can trust, so give one (`--license`). A book has no verses
to compare, so `lx release` compares its files: any change is a minor version.

Like `Cargo.toml`, `[package]` can say more to help people find it: `keywords` (words a search
finds it by), `categories` (up to 5 of a fixed list: `lx new --help`), `repository`,
`documentation`, `readme` (a file in the package, which is then part of it), and `about-books`,
the books of the Bible it's mainly about, as USFM codes. These are its author's to say: logoxide
never works them out from the text (a book citing Romans often isn't about Romans).
`lx new` takes `--keyword`, `--category`, `--repository`, and `--about` (a book's name or code,
repeated):

```toml
[package]
id = "gill-romans"
kind = "commentary"
keywords = ["justification", "puritan", "reformed"]
categories = ["church-history", "biography"]
about-books = ["ROM"]
repository = "https://github.com/someone/gill-romans"
readme = "README.md"
```

Catalogs (`lx catalog build`, `lx catalog entry`) carry `keywords`, `categories`, and
`about-books`, so a search finds them: `lx search romans` lists what's about Romans, and
`--category church-history` keeps that category's. Books filter as they do in topos, with the
genres and names your topos `config.toml` gives (`merge-data`, ...): including a testament limits
to it, included genres and books add up, and exclusions win. A package is kept when one of the
books it's about is:

```sh
lx search -b romans                      # about Romans
lx search --nt -g epistles commentary    # commentaries on a New Testament epistle
lx search -g pauline --exclude-book Philemon
```

Its release also publishes the passages topos finds it citing, with how often
(`logoxide.references.bin`, a few KB), so which books cite a passage can be known without
downloading them. Where each reference is (its chapter, paragraph, and an EPUB CFI readers open
straight to) comes from the book itself, in a tenth of a second:

```sh
lx show --git github:someone/the-dorean-principle --references | grep "^Matt.10.8"
# Matt.10.8 (7)
# Matt.10.8-Matt.10.10 (5)
lx show the-dorean-principle --references --locations | grep "^Matt.10.8 "
# Matt.10.8  [Foreword]  Since 2006, FirstLove Publications has given away over 300,000 copies of publica…
# Matt.10.8  [1. The Command of Christ]  Jesus forbids ministerial reciprocity in Mat­thew 10:8 when he commands his disc…
```

`--format tsv` or `json` gives every field (with `--locations`: the CFI, offsets, line, and
column).

### Your files beside a package's

A package is the files its manifest declares: `logoxide.toml`, each format's own files (`usx/*.usx`,
`epub/*.epub`), the license file, the summary, media timings, and what `include = ["…"]` names.
Anything else in its folder (an annotation File++ writes beside a book, `JHN - Annotations.md`)
isn't part of it: it doesn't change the checksum, isn't removed by `lx clean` (which keeps that
version's folder, and says why), and stays through a repair. Editing a package's own files is
caught:

```sh
lx install
# lx: bsb 1.0.1: its files in packages/bsb/1.0.1+gcd07745e changed since it was locked (…): `lx install --repair` puts them back
lx info bsb
# checksum: sha256:abe3… (CHANGED: locked as sha256:0e61…; lx install --repair puts its files back)
# not the package's (kept): usx/JHN - Annotations.md
lx install --repair
# repaired bsb 1.0.1
```

A **collection** is a package of packages: no text of its own, only dependencies. Each work stays
its own package, versioned on its own, and the collection gathers them:

```sh
lx new dorean-collection --kind collection --name "The Dorean Collection" \
  --abbreviation DOREAN --language en --license CC0-1.0 --version 1.0.0
lx add --project dorean-collection github:someone/the-dorean-principle
lx add --project dorean-collection github:someone/the-doctrine-of-simony
# … and the others
cd dorean-collection && lx release
```

A vault that adds the collection gets every package in it, locked one by one, and the Bibles in a
collection are the vault's Bibles:

```sh
lx add github:someone/dorean-collection
lx list
# abolish-the-jesus-trade 1.0.0  ATJT (Abolish the Jesus Trade), CC0-1.0  …  (a dependency)
# dorean-collection 1.0.0  DOREAN (The Dorean Collection), CC0-1.0  …
# …
```

To let readers take only some of a collection, make its dependencies optional, each turned on by
a feature (edit its `logoxide.toml`; `lx add` writes plain dependencies):

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
lx add github:someone/dorean-collection -F simony --no-default-features
```

## Versification maps

`Psalm 51:1` in an English Bible is `Psalm 51:3` in the Hebrew (whose psalm titles are
numbered), and English Malachi 4:1 is Hebrew 3:19. A Bible names how it numbers verses
(`[package] versification = "English"`; `own`, the default, names nothing a map can use), and a
map package says where one numbering's verses are in another, in Paratext's `.vrs` form, so
Paratext's own `eng.vrs` works as it is:

```toml
[package]
id = "english-original"
name = "English to Original versification"
abbreviation = "ENG-ORG"
version = "1.0.0"
kind = "versification-map"
language = "en"
versification = "English"      # the left side of each line
maps-to = "Original"           # the right side

[formats]
vrs = { path = "eng.vrs" }
```

```text
PSA 51:0 = PSA 51:1            # verse 0: the title, which English Bibles don't number
PSA 51:0 = PSA 51:2
PSA 51:1-19 = PSA 51:3-21      # ranges within a chapter, verse by verse
MAL 4:1-6 = MAL 3:19-24
```

`lx check` reads every line (known books, no verse on two lines of one side unless each names it
alone), the summary counts the verses mapped, and any change to the lines is at least a minor
release. Once a project has one (`lx add`, or a Bible's own dependency), references map between
its Bibles, by name, abbreviation, or alias, or between versifications by name, both ways and
through several maps:

```sh
lx map "Psalm 51:1" --from kjv --to Original
# Psalm 51:3 (Original)
lx map "Malachi 3:19-4:2" --from Original --to English
# Malachi 4:1-6 (English)
# not in English: Malachi 4:1-2 (Original)
lx map "Joel 3" --from English --to Original --with ./english-original   # a map not added
# Joel 4:1-21 (Original)
```

## Commentaries and other formats (a sketch)

What's missing here is logoxide's side, not topos's:

- **topos-bible-markup already reads** OSIS, ThML, GBF, and TEI (every markup SWORD modules use)
  into the same tree as USX, and renders it as plain text and HTML. sword-modules reads SWORD
  Bibles, commentaries, lexicons, and general books into that tree.
- **topos already finds references** inside all of these, and inside EPUBs (each match located by
  an EPUB CFI) and PDFs, so a ThML commentary or an EPUB in a vault is searched today.
- **logoxide reads Bibles in USX, USFM, OSIS, and ThML** (a whole Bible in one file works: the
  KJV from open-bibles' OSIS and from CCEL's ThML both give 31,102 verses in its 66 books).
- **SWORD commentaries import** (`lx import sword:crosswire/MHCC`): each entry is a span of the
  verses it's on (an entry SWORD links over a range is one bridged span), in OSIS or ThML a book
  a file, so the package opens and versions like a Bible. What isn't there yet: a commentary's
  own summary (sections, not verse counts), reading one in the Obsidian plugin (it installs, but
  gets no reader), and CCEL's commentaries (its import takes Bibles).

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
lx import sword:crosswire/MHC             # from CrossWire's repository, through sword-modules (works)
lx import ccel:henry/mhc                  # CCEL's ThML commentaries (not yet: CCEL's import takes Bibles)
lx import ./mhc.thml --kind commentary    # a file someone already has (not yet)
```

## What isn't there yet

- **Nothing is hosted**: logoxide, the KJV and BSB packages, and a catalog of them are local
  repositories, so the examples above use `github:someone/...` where today only
  `file://` URLs exist.
- `lx update` doesn't yet say when a newer major version exists; Door43 imports and an audio
  player are later steps (`doc/logoxide.md` in topos).
