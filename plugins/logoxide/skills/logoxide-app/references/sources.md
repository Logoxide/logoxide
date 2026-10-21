# Where Bibles come from

logoxide reads each publisher's own list as a catalog (`parseCatalogBytes(url, bytes)`); every
entry says what to download (`importDownloads(entry)`), and the download is read the same way
whoever published it. An entry's import name (`fetch:eng_bsb`) is the stable id to save.

| Publisher | List | What a translation is | Import name | Browser (CORS) |
|---|---|---|---|---|
| fetch.bible | `https://v1.fetch.bible/manifest.json` (1,400) | a USX file a book: `bookDownloads(entry)` | `fetch:<id>` (`fetch:eng_bsb`) | yes |
| eBible.org | `lx.ebibleCatalogUrl()` = `https://ebible.org/Scriptures/translations.csv` (~1,300) | a `.zip` of USFM, a few MB | `ebible:<id>` (`ebible:engwebp`) | **no** |
| CCEL | `https://ccel.org/ccel/bible/` and its genre indexes | ThML: Bibles, commentaries, books | `ccel:<id>` | no |
| CrossWire (SWORD) | `lx.swordCatalogUrl("crosswire")` (`mods.d.tar.gz`, read as bytes) | Bibles, commentaries, dictionaries (Strong's), cross-references | `sword:<repository>/<module>` | no; some repositories are plain http (`lx.isInsecureUrl`) |
| Door43 | its catalog API | USFM | `door43:<id>` | unknown: written while git.door43.org didn't answer; untested live |
| A Scripture Burrito or DBL bundle | none: `importBundleEntry(location, files)` | USX/USFM/USJ in a zip | `burrito:<URL>`, `dbl:<URL>` | — |

`lx.defaultCatalogUrls()` lists the five read by default; `lx.importPrefixes()` the publishers.

## Choosing

- **A web page with a fixed set of public-domain translations**: fetch.bible, a book at a time
  (ids: BSB `eng_bsb`, KJV `eng_kjv`, WEB `eng_webu` — `eng_web` is the WEB Classic,
  abbreviated WEBC — ASV `eng_asv`, YLT `eng_ylt`, LSV `eng_lsv`). Hard-code the import names
  (`fetch:eng_bsb`). `bookDownloads` and the license line both need the catalog entry, so
  download the 750 KB manifest once and keep it (a day to a week); a page that only needs text
  can instead build `https://v1.fetch.bible/bibles/<id>/usx/<usfm lowercase>.usx` and check the
  response is XML (a missing book is a 200 HTML page), with the license written in by hand.
- **Letting users pick any translation**: download and merge catalogs, `search(query, 0)` (the second argument is a limit, 0 for all; filter `kind === PackageKind.Bible`),
  show `name`, `abbreviation`, `languageName`, `license`, `copyright`.
- **Native or server apps wanting whole Bibles**: eBible.org (`importDownloads` → `openBible`).
- **Commentaries, Strong's, cross-references**: CrossWire's SWORD modules or CCEL, made into a
  package (`importDownloaded(entry, files)` → `openPackage(files, features, [])`), then `notes`,
  dictionary entries, cross-references. See the recipe "Commentaries and dictionaries".
- **Users' own files**: `openBible` — no catalog needed.

## Licenses

Every entry has `license` (SPDX or `LicenseRef-…`), `rights` (`redistributable`, `derivatives`,
`commercial`, `attribution`), and `copyright`. Show them with the text. Bundling a text into an
app, or serving it to others, is redistribution: only do it when the license allows. logoxide
reports licenses and never enforces them.

## Updates

eBible.org's and SWORD's entries carry the publisher's version (eBible.org: the date of the
last update); `lx.importIsCurrent(entry, version)` says whether a kept copy is current.
fetch.bible's and CCEL's don't version: download again occasionally and compare verse `hash`es.
