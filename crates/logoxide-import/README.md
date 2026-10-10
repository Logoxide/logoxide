# logoxide-import

Packages from other publishers' downloads, so a Bible that isn't a logoxide package can still be
installed: its files with a `logoxide.toml` around them, which a vault then uses as a path
package (or which can be pushed anywhere as a git repository).

- **Catalogs**: each publisher's own list read into a logoxide catalog (`catalog::parse`):
  eBible.org's `translations.csv`, fetch.bible's `manifest.json`, CCEL's page of its Bibles and
  its genre indexes (`gen-COM`, its commentaries), Door43's catalog API, and a SWORD
  repository's `mods.d.tar.gz`. Every entry says what to download
  (`CatalogSource::External`), and nothing else here needs to know the publisher
- **Imports**: `downloads(entry)` lists what to fetch, and `build(entry, files, dest)` makes the
  package from the bytes, whichever publisher it comes from (`ebible`, `fetch`, `ccel`, `sword`,
  `door43`: a SWORD module's entries become OSIS or ThML files, a book each; CCEL's works are
  Bibles, commentaries, or books, as their ThML's head says)
- **Updates**: an import made again from a newer download is compared with the one a project has
  (`update::compare_import`), and its version raised by what changed, whatever the publisher
  numbers it
- **Licenses**: the publishers' statements read as SPDX and rights (`license`)

[logoxide](../logoxide), the package manager, knows no publisher: it keeps only what an entry
says of itself (its import name, the download it came from, whether the publisher's versions
track its updates, and the downloads). This crate sits above it, as lx and the bindings do.
