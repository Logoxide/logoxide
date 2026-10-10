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
| `language` | string | yes | A BCP 47 tag (`en`, `grc`, `pt-BR`) |
| `kind` | string | no | One of the [kinds](#kinds); `bible` when absent |
| `authors` | array of strings | no | Who wrote it (a book's authors, a translation's translators), in order |
| `description` | string | no | A sentence or two (a book's subtitle) |
| `aliases` | array of strings | no | Other names references may use (`AV`, `King James`) |
| `direction` | string | no | `ltr` (the default) or `rtl` |
| `license` | string | no | An SPDX expression (`CC0-1.0`, `CC-BY-SA-4.0`, `LicenseRef-PublicDomain`). Shown to users, never enforced |
| `license-file` | string | no | The license's path in the package. It is part of the package |
| `publisher` | string | no | Who publishes the text |
| `homepage` | string | no | A URL |
| `repository` | string | no | Where its files are kept (its git repository's page) |
| `documentation` | string | no | A URL, if not its README |
| `readme` | string | no | Its README's path in the package. It is part of the package |
| `keywords` | array of strings | no | Words a search should find it by. Each without surrounding spaces, not empty, and not repeated (compared case-insensitively) |
| `categories` | array of strings | no | At most 5, each from the [fixed list](#categories), not repeated |
| `about-books` | array of strings | no | The books of the Bible it is mainly about, as uppercase USFM codes (`["ROM", "GAL"]` for a commentary on Romans and Galatians), not repeated. Said by its author, never worked out from its text; not the books a Bible contains |
| `upstream` | table | no | Where the text was imported from (written by importers; see [Imports](#imports)) |
| `versification` | string | no | `own` (the default: the text's own verse markers say) or a named versification (`English`, `KJV`, `Original`, …) |
| `maps-to` | string | only for a map | For `kind = "versification-map"`: the versification it maps `versification` to. Any other kind must not have it |
| `include` | array of strings | no | More files the package is made of, as [patterns](#what-a-package-is-made-of) (`README.md`, `images/**`) |

A package (other than a collection) must have a non-empty `[formats]`. A collection must have
none. A versification map must give both `versification` (not `own`) and `maps-to`.

Versification names are matched ignoring case. A Bible imported from a SWORD module keeps the
module's own (its conf's `Versification`, `KJV` when it gives none), spelled as SWORD spells it:
`KJV`, `KJVA`, `NRSV`, `NRSVA`, `MT`, `Leningrad`, `Synodal`, `SynodalProt`, `Vulg`, `LXX`,
`Orthodox`, `German`, `Luther`, `Catholic`, `Catholic2`, `Segond`, `Calvin`, `DarbyFr`. A map
between one of them and another versification names it the same way. Other imports keep `own`.

Esther's lettered chapters (A to F, the Greek additions some Bibles number apart) aren't in the
format: chapters are numbers. No import makes them (SWORD numbers chapters by its versification,
and eBible.org and fetch.bible give Greek Esther, `ESG`, numbered chapters). In OSIS and ThML, a
verse marker with a lettered chapter (`Esth.A.1`, `|Esth|A|1|`) is passed over with a warning.

### Kinds

| `kind` | What it is |
|---|---|
| `bible` | A translation, read as verses (the default). It needs a readable format |
| `commentary` | Notes on passages |
| `lexicon` | A dictionary of words |
| `cross-references` | Links between passages |
| `versification-map` | How one versification's verses map to another's (a `vrs` format) |
| `book` | Any other book, kept as files (an EPUB); its Bible references are found in it |
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
| `usj` | `*.json` | not yet |
| `zefania` | `*.xml` | not yet |
| `epub` | `*.epub` | no: kept as files; references are found in them |
| `vrs` | `*.vrs` | no: a versification map's lines |
| `sword` | every file under `path` | no: kept as files |

Other names are allowed and kept, but nothing reads them. Opening a Bible picks, among the formats
its enabled features turn on, the first readable one in the order above.

A file a book is named by its book's USFM code (`JHN.usx`, `43-JHNeng-kjv.usfm`), else by the code
it gives (`\id JHN`, `<book code="JHN">`); a whole file's books are the ones its verses name.
What isn't a book of the Bible is passed over and noted: the peripherals `FRT`, `INT`, `BAK`, `CNC`,
`GLO`, `TDX`, `NDX`, `OTH`, and `XXA` to `XXG`. A book the reader doesn't know (topos's book data
lacks it) is passed over with a warning, its verses left out, and so is a file that names no book.
A book in two files is an error naming both.

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
| `default-catalogs` | boolean | Whether the four default catalogs (eBible.org, fetch.bible, CCEL, CrossWire) are searched after these. Default `true` |
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
| `url` | string | yes | The download it was made from (a SWORD module's zip, CCEL's ThML, fetch.bible's folder of books) |
| `import` | string | no | What `lx import` takes for it (`ebible:eng-kjv2006`, `fetch:eng_kjv`, `ccel:kjv`, `sword:crosswire/KJV`) |
| `version` | string | no | The publisher's version of what was downloaded, as the publisher gives it (eBible.org's update date `2026-10-08`, a SWORD module's `Version` `3.1`, CCEL's edition `1.0` or its file's date, fetch.bible's year) |

An update finds the catalog entry again by `import`, else by `url`. The package's own `version`
follows the rules above like any package's: a new import is `1.0.0`, and each update raises it by
what changed (a reworded verse is minor, a removed one major, a new copyright notice or a new
publisher's version with the same text a patch), whatever the publisher numbers its releases,
so a dependency on `^1` stays safe across the publisher's years. Whether the publisher has a
newer release is judged by comparing its version in the catalog with `upstream.version`
(numbers between dots and dashes compared as numbers, so `2026-10-08` is before `2026-11-01`),
never with the package's version.
