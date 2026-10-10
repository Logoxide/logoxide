# `logoxide.toml`

The manifest of a package, and of a vault (a project that uses packages without being one), like
`Cargo.toml`. This describes format as logoxide 0.1 reads it, for anyone writing another
implementation; the reference is `crates/logoxide/src/manifest.rs`.

The file is TOML 1.0, UTF-8, at the root of the package's folder (or repository). Keys are
kebab-case. A manifest with `[package]` is a package's; without one, it is a vault's (only
`[dependencies]`, `[features]`, and `[settings]` mean anything there).

**Unknown keys** are passed over with a warning in every table described here (so a manifest
written for a newer logoxide still reads), and implementations should do the same: ignore what
they don't know, and say so. Values of known keys that don't make sense are still errors.

```toml
[package]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
version = "1.0.0"
language = "en"
license = "LicenseRef-PublicDomain"
versification = "English"

[formats]
usx = { path = "usx/" }
usfm = { path = "usfm/", feature = "usfm" }

[features]
default = []
usfm = []
strongs = ["dep:strongs-greek"]

[dependencies]
strongs-greek = { git = "https://example.org/strongs-greek", version = "2", optional = true }
```

## `[package]`

| Key | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | What dependencies and lock files call it. Only `a`-`z`, `0`-`9`, `-`, and `_`, not empty |
| `name` | string | yes | Its full name |
| `abbreviation` | string | yes | What references name it by (`John 3:16 BSB`) |
| `version` | string | yes | A [semantic version](https://semver.org); see [Versions](#versions) |
| `language` | string | yes | A BCP 47 tag (`en`, `grc`, `pt-BR`, `sr-Latn`; `und` when unknown), well-formed by RFC 5646's grammar (`en_US` is refused). It should be canonical, as BCP 47 asks: the language by its shortest ISO 639 code (`en`, not ISO 639-3's `eng` or 639-2/B's `ger` for `de`; `grc` and `hbo`, without a two-letter code, stay), deprecated subtags replaced as the IANA registry says (`iw` is `he`), and each subtag in its case (language lowercase, script title case, region uppercase: `en-Latn-US`). A tag that isn't canonical is read, and warned of (`lx check` says what it should be). Imports write canonical tags |
| `language-name` | string | no | The language's name (`English`); an import keeps the publisher's own text here when it isn't a tag, and says `und` |
| `script` | string | no | Its script, an ISO 15924 code (`Latn`, `Hebr`, `Grek`: four letters, the first a capital). Without it, the language tag's script subtag (`sr-Latn`), if any |
| `kind` | string | no | One of the [kinds](#kinds); `bible` when absent |
| `authors` | array of strings | no | Who wrote it (a book's authors, a translation's translators), in order |
| `description` | string | no | A sentence or two (a book's subtitle) |
| `aliases` | array of strings | no | Other names references may use (`AV`, `King James`) |
| `direction` | string | no | `ltr` or `rtl`. Without it, the script's: `rtl` for the scripts written right to left (`Arab`, `Hebr`, `Syrc`, `Thaa`, `Nkoo`, `Adlm`, `Samr`, `Mand`, and the other historic ones Unicode gives that class), else `ltr` |
| `license` | string | no | An SPDX expression (`CC0-1.0`, `CC-BY-SA-4.0`, `MIT OR Apache-2.0`, `LicenseRef-PublicDomain`), checked as one (leniently: `GPL-2.0+`, `/` for `OR`, deprecated ids), with `LicenseRef-<name>` for a license SPDX has no id for. Shown to users, never enforced |
| `rights` | table | no | What the license allows, where it's said rather than derived: see [`[package.rights]`](#packagerights) |
| `license-file` | string | no | The license's path in the package. It is part of the package |
| `publisher` | string | no | Who publishes the text |
| `homepage` | string | no | A URL |
| `repository` | string | no | Where its files are kept (its git repository's page) |
| `documentation` | string | no | A URL, if not its README |
| `readme` | string | no | Its README's path in the package. It is part of the package |
| `keywords` | array of strings | no | Words a search should find it by. Each without surrounding spaces, not empty, and not repeated (compared case-insensitively) |
| `categories` | array of strings | no | At most 5, each from the [fixed list](#categories), not repeated |
| `about-books` | array of strings | no | The books of the Bible it is mainly about, as uppercase USFM codes (`["ROM", "GAL"]` for a commentary on Romans and Galatians), not repeated. Said by its author, never worked out from its text; not the books a Bible contains |
| `work` | string | no | The work it's an edition of (`kjv`), as an id (like `id`'s): see [Works and editions](#works-and-editions) |
| `edition` | string | no | Which edition of its work it is (`1769-oxford`), as an id |
| `edition-year` | integer | no | That edition's year (`1769`; negative before the common era) |
| `upstream` | table | no | Where the text was imported from (written by importers; see [Imports](#imports)) |
| `versification` | string | no | `own` (the default: the text's own verse markers say) or a named versification (`English`, `KJV`, `Original`, …) |
| `maps-to` | string | only for a map | For `kind = "versification-map"`: the versification it maps `versification` to. Any other kind must not have it |
| `keys` | string | only for a dictionary | For a package with a `dictionary` format: what its entries are keyed by, `strongs-greek`, `strongs-hebrew`, or `headword`, which says how a key is matched ([entries.md](entries.md#keys)). A package without that format must not have it |
| `include` | array of strings | no | More files the package is made of, as [patterns](#what-a-package-is-made-of) (`README.md`, `images/**`) |

A package (other than a collection) must have a non-empty `[formats]`, unless it's a recording
that reads another package's text (`[media] text` or `versification`: see [Audio
only](#audio-only)). A collection must have none. A versification map must give both `versification` (not `own`) and `maps-to`.

Versification names are matched ignoring case. A Bible imported from a SWORD module keeps the
module's own (its conf's `Versification`, `KJV` when it gives none), spelled as SWORD spells it:
`KJV`, `KJVA`, `NRSV`, `NRSVA`, `MT`, `Leningrad`, `Synodal`, `SynodalProt`, `Vulg`, `LXX`,
`Orthodox`, `German`, `Luther`, `Catholic`, `Catholic2`, `Segond`, `Calvin`, `DarbyFr`. A map
between one of them and another versification names it the same way. Other imports keep `own`.

Esther's lettered chapters (A to F, the Greek additions some Bibles number apart) aren't in the
format: chapters are numbers. No import makes them (SWORD numbers chapters by its versification,
and eBible.org and fetch.bible give Greek Esther, `ESG`, numbered chapters). In OSIS and ThML, a
verse marker with a lettered chapter (`Esth.A.1`, `|Esth|A|1|`) is passed over with a warning.

### `[package.rights]`

What the license allows, in four words, each optional:

```toml
[package.rights]
redistributable = false   # may be shared as it is, by anyone
derivatives = "none"      # "allowed", "share-alike" (under the same terms), or "none"
commercial = false        # may be sold or used commercially
attribution = true        # sharing it must name its source
```

For a license logoxide knows they're derived from `license`: the public domain
(`LicenseRef-PublicDomain`, `CC0-1.0`, `CC-PDDC`, `PDDL-1.0`, `Unlicense`, `0BSD`, `MIT-0`,
`WTFPL`: everything allowed, no attribution), Creative Commons (`CC-BY-*`: redistributable with
attribution; `-NC` no commercial use, `-ND` no derivatives, `-SA` derivatives under the same
terms), permissive licenses (`MIT`, `ISC`, `Zlib`, `BSL-1.0`, `Apache-*`, `BSD-*`, …: everything
allowed, with attribution), and copyleft ones (`GPL-*`, `LGPL-*`, `AGPL-*`, `GFDL-*`, `MPL-*`,
`EUPL-*`, `EPL-*`, `OFL-*`, `ODbL-*`: derivatives under the same terms). Only a single license is
derived from (not an expression of several). What's said here wins over what's derived; what
neither says is unknown. Importers write what a publisher says beyond its license (a SWORD module
CrossWire alone may distribute is `redistributable = false`, `derivatives = "none"`).

Like the license, rights are shown, never enforced. `lx publish` warns of a package that has no
license or one not known to be free (unless `redistributable = true` is said), and when its
rights say it isn't redistributable, warns loudly and asks first (`--yes` answers);
see [trust.md](trust.md#licenses).

### Works and editions

A work is a text whatever its editions, formats, and publishers: the King James Version is one
work, and the 1769 Oxford text, a 1611 facsimile, and the Pure Cambridge Edition are editions of
it. `work` names it by an id, so packages of one work can be found together (`lx search` gathers
each work's editions under its name) and a vault using two of them is told so (`lx add`, `lx
install`, and `lx import` warn: not wrong, but perhaps not meant).

logoxide keeps a table of works (`crates/logoxide/data/works.toml`): each work's id, name,
aliases (which searches match), its Wikidata item when known, and the import names known to be it
(`ebible:eng-kjv2006`, `sword:crosswire/KJV`). An import whose name is listed is given that
`work`; nothing is ever guessed from a title. Other ids may be used for works the table doesn't
have. A package that doesn't say its work takes the work of another of the project's Bibles with
its `id` (a fork, added with `package = "kjv"`, is its original's work).

### Kinds

| `kind` | What it is |
|---|---|
| `bible` | A translation, read as verses (the default). It needs a readable format |
| `commentary` | Notes on passages: OSIS or ThML files whose verse markers say which verses each note is on, read by passage |
| `lexicon` | A dictionary: Strong's numbers or headwords and their entries (the `dictionary` format, [entries.md](entries.md)) |
| `cross-references` | Links between passages (the `xref` format, [entries.md](entries.md)) |
| `versification-map` | How one versification's verses map to another's (a `vrs` format) |
| `book` | Any other book, kept as files (an EPUB, or ThML like CCEL's books); its Bible references are found in it. A book's formats are never read as verses, even one that could be (`thml`) |
| `collection` | Other packages, as its dependencies, and no text of its own |

Only `bible` requires a format logoxide reads as verses. Other kinds may have none; they are
kept and summarized as files.

### Categories

Subjects; what kind of package it is (a commentary, a lexicon) is its `kind`, and traditions,
eras, and narrower topics are keywords. The list is fixed:

| Category | For |
|---|---|
| `biblical-theology` | Themes traced through Scripture (covenant, kingdom) |
| `introduction` | Old and New Testament introductions and surveys, authorship, canon |
| `background` | The Bible's history, archaeology, geography, and culture |
| `hermeneutics` | How to interpret Scripture |
| `biblical-languages` | Greek, Hebrew, and Aramaic grammars and readers |
| `systematic-theology` | Doctrine by topic |
| `creeds-and-confessions` | Creeds, confessions, and catechisms |
| `apologetics` | Defending the faith |
| `ethics` | Christian ethics and moral theology |
| `church-history` | The church through the centuries |
| `biography` | Lives, memoirs, and letters |
| `preaching` | Homiletics: how to preach |
| `sermons` | Collected sermons |
| `pastoral` | Pastoral ministry, counseling, and church leadership |
| `worship` | Liturgy, hymns, and prayer books |
| `missions` | Missions and evangelism |
| `spiritual-life` | Discipleship, prayer, and holiness |
| `devotional` | Daily readings and meditations |
| `family` | Marriage, parenting, and the home |
| `children` | Written for children |
| `reference` | Dictionaries, encyclopedias, atlases, and concordances |

## `[formats]`

Each format of the text, by name, as an inline table:

| Key | Type | Meaning |
|---|---|---|
| `path` | string | A folder in the package (`usx/`), or a single file (`eng.vrs`) |
| `feature` | string | The feature that turns this format on. It must be a key of `[features]`. Without it, the format is always on |

Format names (case-insensitive) and the files each is made of:

| Name | Files under `path` | Read as verses |
|---|---|---|
| `usx` | `*.usx`, a book a file | yes (preferred first) |
| `usfm` | `*.usfm`, `*.sfm`, a book a file | yes (second) |
| `osis` | `*.xml`, `*.osis`: a book a file, or the whole Bible in one | yes (third) |
| `thml` | `*.xml`, `*.thml`: a book a file, or the whole Bible in one | yes (fourth) |
| `gbf` | `*.gbf`, `*.txt`: a book a file, or the whole Bible in one, with GBF's sync marks | yes (fifth) |
| `usj` | `*.json` | not yet |
| `zefania` | `*.xml` | not yet |
| `epub` | `*.epub` | no: kept as files; references are found in them |
| `vrs` | `*.vrs` | no: a versification map's lines |
| `sword` | every file under `path` | no: kept as files |
| `dictionary` | `*.xml`: a dictionary's entries, `<entry key="G25">…</entry>` in the markup each file declares ([entries.md](entries.md)) | no: read as entries by key |
| `xref` | `*.tsv`: `from`, `to`, and an optional weight a line, OSIS references ([entries.md](entries.md)) | no: read as a verse's cross-references |

Other names are allowed and kept, but nothing reads them. Opening a Bible picks, among the formats
its enabled features turn on, the first readable one in the order above.

A verse's marker may name a range of verses: a bridge (`n="4-5"`, `osisID="John.3.4 John.3.5"`),
or in OSIS a range into the next chapter (`osisID="John.3.36-John.4.2"`; in ThML
`parsed="|John|3|36|4|2"`), one verse from its first to its last: what a SWORD module links to one
entry (a commentary's note on both chapters' verses).

ThML has no verse elements: its verses are empty `<scripture parsed="|Gen|1|2|0|0"/>` markers
(CCEL's Bibles) or `<scripCom parsed="|Gen|1|1|1|2"/>` (CCEL's commentaries), each running to the
next. A chapter's own marker (`parsed="|Gen|1|0|0|0"`) is its verse 0: what a commentary says of
the whole chapter is a note on verse 0. A division (`div1` to `div6`) whose first marker is in
another chapter ends the verse before it, so an introduction isn't read as the last note of the
chapter before; when that first marker is the chapter's verse 0, the chapter starts at the
division's start, so an introduction written before the marker is part of verse 0's note.

A file a book is named by its book's USFM code (`JHN.usx`, `43-JHNeng-kjv.usfm`), else by the code
it gives (`\id JHN`, `<book code="JHN">`); a whole file's books are the ones its verses name.
What isn't a book of the Bible is passed over and noted: the peripherals `FRT`, `INT`, `BAK`, `CNC`,
`GLO`, `TDX`, `NDX`, `OTH`, and `XXA` to `XXG`. A book the reader doesn't know (topos's book data
lacks it) is passed over with a warning, its verses left out, and so is a file that names no book.
A book in two files is an error naming both.

**GBF** (the General Bible Format, which SWORD's older modules are written in) has no XML: its
markup is `<XY…>` tokens (logoxide-markup reads them), and its structure is sync marks: `<SBJohn>`
starts a book (named by its OSIS id, USFM code, or English name; GBF's own numbers 1 to 39 and 64
to 90 are read too), `<SC3>` a chapter, and `<SV16>` a verse (`<SC>` or `<SV>` without a number is
one more than the last). A bridge is its first and last marks with nothing between (`<SV16><SV17>`),
as GBF writes them. A file's books are the ones its `<SB>` marks name; a file without one is a
book a file, named by its file name as the other formats' are. A chapter's text before its first
verse (a heading, a commentary's introduction) belongs to no verse, like USX's. A SWORD import
writes a GBF module this way, a book a file (`gbf/John.gbf`); a note SWORD links across a chapter
(John 3:36 to 4:2), which OSIS and ThML keep as one range, is written at its first verse, with
the verses it covers in the next chapter as an empty bridge (`<SC4><SV1><SV2>`).

## `[features]`

Cargo's features. Each key is a feature; its value is an array of what it turns on:

- another feature's name
- `dep:<name>`: an optional dependency (one with `optional = true`)
- `<name>/<feature>`: a feature of a dependency (which also turns that dependency on)

`default` is turned on unless the user asks for no default features. Every name a feature lists
must exist. Features choose formats (`usfm = []` and `feature = "usfm"`) and optional
dependencies (a collection whose packages are each behind a feature).

## `[dependencies]`

Each dependency by name. The name is the package's id, unless `package` says otherwise.

```toml
[dependencies]
bsb = { git = "https://github.com/someone/bsb", version = "1" }
kjv = { git = "github:someone/kjv", tag = "v1.0.1", features = ["usx"], default-features = false }
kjv-bob = { git = "github:bob/kjv", package = "kjv", abbreviation = "KJVB" }
romans = { registry = "https://example.org/registry/catalog.toml", version = "1" }
mine = { path = "Bibles/My Translation" }
strongs = { git = "https://example.org/strongs", optional = true, enabled = false }
```

| Key | Type | Meaning |
|---|---|---|
| `git` | string | A git repository's URL (never starting with `-`). `github:owner/repo`, `gitlab:owner/repo`, and `codeberg:owner/repo` are short for the hosts' HTTPS URLs |
| `registry` | string | A registry's catalog (its URL, or a file's path), whose releases are archives ([catalog.md](catalog.md)) |
| `path` | string | A folder, relative to this manifest, used in place. `lx add --path` writes it relative even when given absolute; an absolute one is warned of, since the lock file names it too and another machine won't have it |
| `version` | string | A semver requirement (`1`, `^1.2`, `>=1, <3`): the newest release matching it. For git, releases are tags `v<version>` with a whole semver version (`v1.0.0`; `v1.0` is no release, and is named when nothing matches) |
| `tag` | string | For `git` only: this tag |
| `rev` | string | For `git` only: this commit, by its full object id (40 hex digits, or 64 in a SHA-256 repository); a shorter prefix is refused |
| `branch` | string | For `git` only: this branch's head |
| `folder` | string | For `git` only: the package's folder in the repository, when it isn't at the top. Its releases are tags `<folder>/v<version>` (others are passed over), and only that folder is installed |
| `optional` | boolean | Only used when a feature turns it on (`dep:<name>`). Default `false` |
| `features` | array of strings | Its features to turn on |
| `default-features` | boolean | Whether its `default` feature is on. Default `true` |
| `package` | string | The package's id, when the dependency is named something else (forks side by side) |
| `abbreviation` | string | What references name it by here, in place of its own. Only this copy takes it: the same package at another version, brought in by a collection, keeps its own. Two Bibles answering to one name are warned of |
| `aliases` | array of strings | Its aliases here, in place of its own |
| `enabled` | boolean | `false` turns it off without removing it: it stays locked and installed, but nothing reads it. Default `true` |

Exactly one of `git`, `registry`, and `path` must be given, and at most one of `tag`, `rev`, and
`branch` (only with `git`). A bare requirement (`bsb = "1"`) parses, but there is no default
registry, so installing it is an error.

A package fetched from git or a registry can't have `path` dependencies (they would name files
on the publisher's machine). The resolver is also gaining checks that a fetched package's
dependencies don't name local paths or `file://` URLs, and that git URLs and refs that would be
read as command-line options (starting with `-`) are refused; see [trust.md](trust.md).

## `[settings]`

A vault's choices (also allowed in a package, where they mean nothing to its users):

| Key | Type | Meaning |
|---|---|---|
| `default` | string | The Bible references use when they name none |
| `compare` | array of strings | Bibles shown after the named one |
| `catalogs` | array of strings | Catalogs to search (URLs or paths), before the default ones |
| `default-catalogs` | boolean | Whether the five default catalogs (eBible.org, fetch.bible, CCEL's Bibles and commentaries, CrossWire) are searched after these. Default `true` |
| `registries` | table of strings | Registries by name: `dorean-local = "~/logoxide-packages"` lets `dorean-local:kjv` name a package. Each is a catalog's URL or path, or a folder of package repositories |

Registry names are `a`-`z`, `0`-`9`, `-`, and `_`, and can't be `github`, `gitlab`, `codeberg`,
`ebible`, `fetch`, `ccel`, `sword`, `door43`, `http`, `https`, `file`, `ssh`, or `git`.

## `[media]`

Audio and video, listed by URL and hash, never stored in the package.

```toml
[media]
timings = "timings/"

[[media.files]]
path = "audio/JHN.mp3"
url = "https://example.org/audio/JHN.mp3"
mirrors = ["https://archive.org/download/bsb-audio/JHN.mp3"]
size = 31457280
sha256 = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
book = "JHN"
# timings = "subtitles/john.vtt"
```

| Key | Type | Meaning |
|---|---|---|
| `timings` | string | A folder in the package holding each file's verse timings |
| `text` | string | For a recording without a text of its own: the dependency (a key of `[dependencies]`) whose text it reads |
| `versification` | string | The versification it's numbered by (`KJV`), whose verse counts its timings are checked against when there's no text |
| `files` | array of tables | Each file |
| `files.path` | string | Its name in the package (what its timings are named after) |
| `files.url` | string | Where it is served |
| `files.mirrors` | array of strings | Other places serving the same bytes, tried in order after `url` |
| `files.size` | integer | Its length in bytes |
| `files.sha256` | string | Its SHA-256 as lowercase hex (no prefix) |
| `files.book` | string | The book it reads (a USFM code), if one |
| `files.timings` | string | Its timings' path in the package, in place of the timings folder's |

A download is good when its length is `size` and its SHA-256 is `sha256` (compared
case-insensitively); since the hash says which bytes are meant, any copy is as good as the first.
Hosts keep files by hash, at `media/<first two hex digits>/<hex>` in their cache, so versions that
list the same file share it.

**Timings** are WebVTT, one file per media file: its own `timings` when it has one, else
`<timings>/<stem>.vtt`, where `<stem>` is the media file's `path` (folders kept) without its last
extension (`audio/JHN.mp3` → `timings/audio/JHN.vtt`, so `video/JHN.mp4`'s are
`timings/video/JHN.vtt`). A file with neither has no timings. Each cue's identifier is the verse it reads, a USFM reference: `JHN 3:16`, or `JHN 3:17-18` for a
bridge. Its time range (`hh:mm:ss.mmm` or `mm:ss.mmm`, milliseconds exactly three digits) is the
verse's; its text may be the verse's or empty. Cues not named by a verse (an introduction, music)
are skipped. The file starts with `WEBVTT`. Checks (`lx check`, `lx media check`, `lx release`):
each verse exists in the package's text (verse 0, a psalm's title, only where the text numbers
one 0; a bridge's every verse), is in the file's `book` if it has one, verses come in the text's
order without overlapping, every listed file that should have timings has them, and every
`*.vtt` anywhere under the timings folder belongs to a listed file.

### Audio only

A recording needn't carry a text of its own. `text` names the dependency whose text it reads,
and `versification` the versification it's numbered by; a package with either may have no
`[formats]`, and opens with no verses (it isn't one of a project's Bibles, so references never
name it):

```toml
[package]
id = "bsb-audio"
name = "The Berean Standard Bible, read aloud"
abbreviation = "BSBA"
version = "1.0.0"
language = "en"

[dependencies]
bsb = { git = "https://example.org/bsb", version = "1" }

[media]
text = "bsb"
versification = "KJV"
timings = "timings/"
```

Its timings are checked (`lx check`, `lx media check`) against the text `text` names, as
installed for the package's folder (`lx install` there; when it isn't, that's said and the
verses go unchecked), else against its own text, else against `versification`'s verse counts
(those logoxide has: SWORD's systems, `KJV`, `NRSV`, `Synodal`, …; another name is said). `lx
release` checks only what it can without installing: the files, and the verses' order.

## `[titles]`

Each file's title (what a reader sees for it, apart from its file name), by path in the package,
in place of what the file says:

```toml
[titles]
"usx/JHN.usx" = "Juan"
```

Without one, a file's title is what it says (a Bible book's `\toc2`, else `\h`, else `\toc1`; in
USX the paragraphs of those styles; an EPUB's `dc:title`), else a Bible book's English name, else
for a file holding many books, the package's `name`. A file with no title is refused by `lx check`
and `lx release`. Titles are published in the summary, and changing one is a minor change.

## What a package is made of

A package is the files its manifest declares, so other files in its folder (a reader's notes
beside a book, `.DS_Store`) are never part of it. A path is declared when it is:

- `logoxide.toml`
- a summary file: `logoxide.summary.toml`, `logoxide.verses.bin`, `logoxide.references.bin`
- `license-file` or `readme`
- under a format's `path` with one of that format's extensions (case-insensitive), or the
  format's `path` itself when that is a file; for formats with no extension list (`sword`, and
  names logoxide doesn't know), every file under its `path`
- a `*.vtt` under `[media] timings`, or a media file's own `timings`
- matched by an `include` pattern

`.git/` is never part of a package. Paths use `/`, relative to the package's root.

**Patterns**: matched against the whole path. `*` is any characters but `/`, `?` is one
character but `/`, a `**` segment is any number of folders (or none), and a pattern ending in
`/` matches everything under that folder. `README.md` matches only at the top; `**/*.png`
matches PNGs anywhere.

The package's **checksum** is computed over these files; see [catalog.md](catalog.md#checksums).

## Versions

A package's version says what an update can break. `lx release` compares the new text with the
last release's ([summaries.md](summaries.md#changes-and-versions)) and refuses a version number
smaller than the change.

## Imports

Packages made from other publishers' downloads (`lx import`) are ordinary packages that say where
their text came from in `[package.upstream]`:

```toml
[package.upstream]
url = "https://ebible.org/Scriptures/eng-kjv2006_usfm.zip"
import = "ebible:eng-kjv2006"
version = "2026-10-08"
```

| Key | Type | Required | Meaning |
|---|---|---|---|
| `url` | string | no | The download it was made from (a SWORD module's zip, CCEL's ThML, fetch.bible's folder of books). Every import writes it; a package made by hand from a source with no URL says where in `note` |
| `import` | string | no | What `lx import` takes for it (`ebible:eng-kjv2006`, `fetch:eng_kjv`, `ccel:kjv`, `ccel:henry/mhc1`, `sword:crosswire/KJV`, `door43:Door43-Catalog/hi_irv`) |
| `version` | string | no | The publisher's version of what was downloaded, as the publisher gives it (eBible.org's update date `2026-10-08`, a SWORD module's `Version` `3.1`, CCEL's edition `1.0` or its file's date, fetch.bible's year) |
| `note` | string | no | Where the text came from, in words, when `url` and `import` can't name it (`Digital Bible Library text release bundle 317e03834065a236, revision 1`) |
| `converted` | boolean | no | The import rewrote the publisher's markup, its words unchanged (a SWORD module's entries made into OSIS or ThML files). Not a derivative to logoxide, but `lx publish` says so when the license allows no derivatives. Default `false` |

An update finds the catalog entry again by `import`, else by `url`. The package's own `version`
follows the rules above like any package's: a new import is `1.0.0`, and each update raises it by
what changed (a reworded verse is minor, a removed one major, a new copyright notice or a new
publisher's version with the same text a patch), whatever the publisher numbers its releases,
so a dependency on `^1` stays safe across the publisher's years. Whether the publisher has a
newer release is judged by comparing its version in the catalog with `upstream.version`
(numbers between dots and dashes compared as numbers, so `2026-10-08` is before `2026-11-01`),
never with the package's version.
