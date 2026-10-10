# `catalog.toml`, registries, and release archives

A catalog lists packages, so they can be found by name, abbreviation, or language instead of by
URL. It holds what each package is and where it comes from, never its text, so anyone can host
one: a file in a git repository or on any static host is enough. A **registry** is a catalog
whose packages are published as release archives (`[[package.release]]`), so thousands of books
need no repository each. This describes the format as logoxide 0.1 reads and writes it; the
reference is `crates/logoxide/src/catalog.rs` and `crates/logoxide/src/publish.rs`.

logoxide's `logoxide-import` crate (and so lx and the bindings) also reads other publishers' own
lists as catalogs (eBible.org's `translations.csv`, fetch.bible's `manifest.json`, CCEL's page of
its Bibles and its genre indexes, Door43's catalog API, a SWORD repository's `mods.d.tar.gz`); their entries are imported into packages rather
than installed. They aren't described here.

## The file

TOML 1.0, UTF-8. A catalog's location decides how it is read: a name ending in `.csv` is
eBible.org's form, `.json` fetch.bible's, `.tar.gz` a SWORD repository's, a page on `ccel.org/ccel/`
CCEL's page of Bibles, and `ccel.org/index/genre/gen-XXX` one of CCEL's genre indexes
(`gen-COM`'s works are commentaries, `gen-BIB`'s Bibles, any other genre's books), and a
location containing `door43.org/api/v1/catalog` Door43's catalog API (all read by
`logoxide-import`); anything else is a `catalog.toml`.

```toml
format = 1
name = "Bibles in English"

[[package]]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
language = "en"
version = "1.0.0"
license = "LicenseRef-PublicDomain"
git = "https://github.com/someone/bsb"

[[package]]
id = "the-dorean-principle"
name = "The Dorean Principle"
abbreviation = "TDP"
language = "en"
version = "1.0.2"
kind = "book"
categories = ["pastoral"]
path = "the-dorean-principle"

[[package.release]]
version = "1.0.2"
url = "the-dorean-principle/the-dorean-principle-1.0.2.tar.gz"
mirrors = ["https://archive.org/download/dorean/the-dorean-principle-1.0.2.tar.gz"]
sha256 = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
checksum = "sha256:41c0…"
size = 1834201
```

### Top level

| Key | Type | Meaning |
|---|---|---|
| `format` | integer | The catalog format's version. Absent means `1`. A reader that finds a higher number than it knows must refuse the catalog and say a newer logoxide is needed |
| `name` | string | The catalog's name, shown in search results |
| `package` | array of tables | Its packages |

### `[[package]]`

| Key | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | The package's id (`[package] id`): lowercase ASCII letters, digits, `-`, and `_` |
| `name` | string | yes | Its name |
| `abbreviation` | string | yes | What references name it by |
| `language` | string | yes | BCP 47, canonical as in the manifest (`en`, not `eng`). Searches match a language by any of its codes: `eng` finds `en`, and `en` an entry that says `eng` |
| `aliases` | array of strings | no | |
| `language-name` | string | no | The language's name (`English`) |
| `script` | string | no | ISO 15924 (`Latn`, `Hebr`). Absent, the language tag's script subtag, if any |
| `description` | string | no | |
| `license` | string | no | SPDX expression, shown, never enforced |
| `rights` | table | no | What the license allows, where said rather than derived (`redistributable`, `derivatives`, `commercial`, `attribution`), as in the manifest's [`[package.rights]`](manifest.md#packagerights) |
| `kind` | string | no | As in the manifest; `bible` when absent |
| `direction` | string | no | `ltr` or `rtl`. Absent, the script's (as in the manifest), else `ltr` |
| `version` | string | no | The newest version, for display |
| `keywords`, `categories`, `about-books` | arrays of strings | no | As in the manifest; searches match them (a word naming a book of the Bible matches what's about it) |
| `work`, `edition`, `edition-year` | string, string, integer | no | As in the manifest ([works and editions](manifest.md#works-and-editions)): searches match the work's names, and `lx search` gathers a work's editions (`--flat` doesn't). Imports from other publishers' lists are given the work logoxide's table knows their import name to be |
| `git` | string | one of `git` or `release` | The package's git repository (`github:owner/repo` is accepted and expanded) |
| `release` | array of tables | | The archives a registry publishes, oldest first (below) |
| `path` | string | no | Its path in the folder the catalog was built from (`books/romans`), which names it there as its id does |
| `commit` | string | no | Written by `lx catalog build` (the commit of the newest release); informational |

An entry must have `git` or at least one `release`. Unknown keys in an entry are ignored. An
entry whose `id` isn't one (`../kjv`, `KJV`, empty) is passed over with a warning, and the rest
of the catalog read: an id names a folder in the library. Searches match without regard to case
or combining marks (both sides are lowercased, decomposed to NFD, stripped of every mark,
Unicode's `General_Category=Mark`, and a final sigma read as `σ`), so `ιησους` finds `Ἰησοῦς`,
`בראשית` finds `בְּרֵאשִׁית`, and `jose` finds `José`. When catalogs are searched together, the
first entry with a given id wins, so catalogs with overlapping ids should prefix them
(logoxide's importers use `ebible-`, `fetch-`, `ccel-`, `sword-`); `lx search` and `lx import` warn when an
id is listed by more than one of the project's catalogs, and `lx add` when another of its
registries lists the id it adds.

The fields describe the package as its newest release's `logoxide.toml` does (`lx catalog entry
<git URL>` writes one), but a catalog is only a pointer: what is installed is what the
repository or archive holds, checked as below.

### `[[package.release]]`

| Key | Type | Required | Meaning |
|---|---|---|---|
| `version` | string | yes | The release's version (semver) |
| `url` | string | yes | Its `.tar.gz`. A relative URL is relative to the catalog's own location (beside it, like a relative link); a URL with `://` or a path starting with `/` is taken as it is |
| `mirrors` | array of strings | no | Other places serving the same archive, tried in order after `url` (relative the same way) |
| `sha256` | string | yes | The SHA-256 of the archive's bytes: `sha256:` and lowercase hex |
| `checksum` | string | yes | The package's [checksum](#checksums) once unpacked. A release without one is refused: an installed copy could never be checked |
| `size` | integer | no | The archive's length in bytes |
| `yanked` | boolean | no | Withdrawn: new installs pass over it, but a lock file that has it still installs it |

**A published version never changes.** Adding a release whose version is already listed with
another `sha256` is an error; release a new version instead.

Choosing a release for a requirement: among releases not yanked whose version parses, the
newest that matches the requirement; with no requirement, the newest that isn't a pre-release.

## Registries

A project names registries in `[settings.registries]` (or a machine's config), so
`<registry>:<id>` or `<registry>:<path>` names a package in one. A registry is either a
`catalog.toml` (its URL or path) or a **folder of package repositories**: every git repository
in it, at any depth, with a `logoxide.toml` that has `[package]`, is a package named by its path
in the folder and found at its `file://` URL. A name matches an entry's `path` first, then its
`id`; an id two entries share names neither, and the path must be given.

A dependency `{ registry = "<catalog URL>", version = "1" }` downloads only the chosen release's
archive. Installing:

1. Read the catalog, find the entry by id (or path), choose the release.
2. Download `url`, else each mirror in turn, until one's bytes hash to `sha256`; bytes that don't
   are passed over.
3. Unpack it ([below](#archives)), keep only the package's declared files, and check that its
   `logoxide.toml` has the entry's id and the release's version, and that its checksum is the
   release's `checksum` (a release that lists none is refused).
4. Lock the release's `archive` URL, `mirrors`, `sha256`, and `checksum` ([lock.md](lock.md)), so
   later installs need no catalog.

## Archives

A release archive is a gzip-compressed tar of the package's declared files
([manifest.md](manifest.md#what-a-package-is-made-of)), made so that the same files always give
the same bytes (`crates/logoxide/src/publish.rs`, `pack`):

- every file under one top folder, `<id>-<version>/` (`the-dorean-principle-1.0.2/logoxide.toml`)
- files in byte order of their paths, regular files only (no folder entries, no links)
- GNU tar headers in deterministic mode: mode `0644`, modification time 0, uid and gid 0, no
  owner names
- gzip at the best compression level, with modification time 0 in its header
- its name `<id>-<version>.tar.gz`

Readers don't depend on the packing: unpacking drops the archive's single top folder (whatever
its name, as git hosts' archives have `<repo>-<commit>/`) and keeps regular files only, and what
is checked is the `sha256` of the bytes and the `checksum` of the files. Another implementation
may pack differently; its archive just hashes differently, and a version, once published with a
hash, keeps it.

logoxide refuses an archive of more than 200,000 entries, a file of more than 1 GiB, or more than
1 GiB of files in all, judging by the tar headers before it writes anything and again by what it
reads; other readers should have limits of their own.

## Checksums

A package's checksum names its exact files, for lock files, caches, and checking downloads. It is
`sha256:` and the lowercase hex of the SHA-256 of, for each declared file in byte order of its
path:

1. the path's UTF-8 bytes (`/`-separated, relative to the package's root)
2. one zero byte
3. the file's length as an unsigned 64-bit little-endian integer
4. the file's bytes

Files that aren't declared (notes beside a book, `.git/`) don't change it. The summary's own
files are declared, so a released package's checksum includes them; the summary's
`text-checksum` is the same hash without them ([summaries.md](summaries.md)).

The archive's `sha256` is simply the SHA-256 of the `.tar.gz` bytes, in the same `sha256:`-hex
form.
