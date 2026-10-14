# Plan: catalogs of catalogs

**Deferred** (2026-10-14, `PLAN-REVIEW.md` R2): the design below is kept as written, and none of
it is to be built until a second catalog exists anywhere for one to include. Its two questions
for the owner wait with it.

A `catalog.toml` that names other catalogs, which a search follows, so that one URL can stand for
a community's or a publisher's whole set of lists. The owner's idea of 2026-10-09, deferred then
(PLAN-NEXT.md's "Later", N8) with its questions to settle first; this plan answers each with a
recommendation and orders the work. Written 2026-10-14 against logoxide main d0a689b.

## What it is, and what it isn't

- **For finding, not installing.** A catalog of catalogs adds places `lx search`, `lx works`,
  `lx import`, the MCP's `search_catalogs`, and the bindings' `LogoxideCatalog` look. Nothing is
  installed because a catalog includes another, and locks never name a catalog that only led to
  a package: a git dependency locks its URL, a registry dependency its catalog's own URL and the
  archive's (as now).
- **Not a collection.** A `collection` package is a bundle a project depends on: installed,
  versioned, locked. An included catalog is a list someone keeps up to date, which changes
  whenever its host changes it. The two stay separate. "Everything a church uses" is a
  collection; "every Bible publisher Wycliffe knows of" is a catalog of catalogs.
- **Not the defaults.** `DEFAULT_CATALOGS` stays in the core. The owner can't host a list of
  lists, and the defaults shouldn't change because a file somewhere did.

## The format

```toml
format = 1
name = "Bibles for the Pacific"

[[catalog]]
url = "https://example.org/png/catalog.toml"
name = "Papua New Guinea"
description = "Translations from SIL PNG"

[[catalog]]
url = "fiji/catalog.toml"            # relative: beside this file

[[package]]
# … a catalog can list packages too, as now
```

| Key | Type | Required | Meaning |
|---|---|---|---|
| `url` | string | yes | Where the catalog is: a URL, or relative to this catalog's location (as a release's `url` is). Any form logoxide reads (a `catalog.toml`, eBible.org's `.csv`, a SWORD repository's `.tar.gz`, …) |
| `name` | string | no | What to call it when it has no `name` of its own (a publisher's list never does) |
| `description` | string | no | Shown by `lx catalog list` |

Only a `catalog.toml` can include; other publishers' lists are read as they are now.

`format` stays 1. A reader of format 1 ignores keys at the top level that it doesn't know (as
logoxide 0.2 does), so an older lx reads the packages and doesn't follow the includes. Nothing is
misread, so nothing needs refusing.

## Decisions

Each is taken as recommended until the owner says otherwise.

| # | Question | Recommendation |
|---|---|---|
| CC1 | Depth and cycles | Followed breadth-first, to `MAX_DEPTH` 3 levels below a catalog the project names, and at most `MAX_INCLUDED` 64 catalogs reached through includes in one load. Each catalog is read once a load, by its whole URL after `relative_url` (no other normalization: two spellings of one URL are two catalogs, and the second's ids are shadowed anyway). A cycle, or an include past the depth or the count, isn't followed and is warned of once, naming the catalog that included it |
| CC2 | Who wins when ids collide | Breadth-first order: the project's catalogs in their order, then everything they include one level down (in the same order), then the next level. Merging keeps the first id it sees (as now), so a catalog the user named always wins over anything reached through an include, and a nearer include wins over a deeper one. `Catalog::shadowed`'s warnings cover it unchanged: they name both catalogs |
| CC3 | Registries | An included catalog never creates a registry name (`foo:<id>`): only `[settings.registries]` and the config do, by the user's hand. Its `[[package.release]]` entries can still be installed, since a registry dependency is the catalog's URL (`{ registry = "<url>" }`), not its name. `lx catalog add <url> --name <registry>` turns an included one into a named registry, as it would any URL |
| CC4 | Where included catalogs may point | The rule entries follow now: a catalog read from elsewhere names nothing on this machine. A remote catalog's include of a path or `file://` is refused (warned, passed over); a local catalog (the project's own, or a folder registry's) may include local ones. `http://` is followed and warned of, as `git::is_insecure` warns of other URLs |
| CC5 | Caching | Each included catalog at a URL is kept in `CatalogCache` by its own URL, with the same `catalog-max-age` and `Freshness` as the catalog that named it. `--offline` follows what's kept; an include not kept is warned of and skipped. `lx catalog refresh` follows includes and downloads each again; `lx catalog refresh <url>` refreshes only that one (its includes are refreshed only if named too). A download that fails uses the kept copy, warned, as now |
| CC6 | Turning it off | `[settings] follow-catalogs = false` in a vault (and `follow-catalogs` in the config, the vault's winning) stops following includes; on by default, since the user chose the catalog that includes them. No per-include switch: removing the catalog, or adding the one wanted directly with following off, covers it |
| CC7 | What a result says of its catalog | Each `Catalog` carries `via: Option<String>`: the name of the catalog that included it (the first, when more than one did). `lx search` shows the catalog's name as now, plus ` (via <name>)` for an included one; JSON adds `via`. `lx catalog list` shows the included catalogs indented under the one that named them, with their description, and `--format json` gives each `included` list |
| CC8 | Works | An included catalog's `[works]` merge as any catalog's do (`add_works`: the first kept, a differing one warned of), in the order of CC2 |
| CC9 | `lx catalog build` | A folder registry's catalog keeps the `[[catalog]]` entries of the catalog it replaces (as `[works]` is kept without a `works.toml`), so they're written by hand once. A `catalogs.toml` in the folder, like `works.toml`, is the alternative; recommended only if the owner wants it, since a registry that includes others is rare |

Questions for the owner beyond these:

1. Whether following should be on by default (CC6). Recommended on; off is safer, but then
   adding a catalog of catalogs does nothing until a second setting is set, which surprises.
2. Whether `lx search` should say ` (via …)` at all, or only in `--format json` and `lx catalog
   list`. Recommended: say it, since an included catalog is one the user didn't add.

## Phases

Each phase builds, tests, and lints clean on its own, with its docs (`doc/catalog.md`,
`doc/manifest.md` for the setting, `doc/architecture.md`'s crate blocks) in the same commit, and the usual
checks: `cargo fmt`, `cargo clippy --all-targets -D warnings`, the workspace's tests, `cargo check
-p logoxide --no-default-features --all-targets`, and the FFI's tests from main.

### CC-A: the format, in the core

- `catalog.rs`: `TomlCatalog` gains `#[serde(rename = "catalog")] includes: Vec<TomlInclude>`;
  `Catalog` gains `includes: Vec<Include { url, name, description }>`, each `url` made whole
  with `relative_url` against the catalog's location (left as written when it has none) and
  `via: Option<String>`, which the loader sets. An include with an empty `url` is passed over
  with a warning; one whose `url` is local in a remote catalog too (CC4), into
  `Catalog::warnings` rather than refusing the catalog, since its packages are still good.
- `merge` keeps `includes` out of the merged catalog's search (they're already followed by
  then) but appends them, so a merged catalog can still say what it included.
- `Manifest`'s `[settings] follow-catalogs` (`Option<bool>`) and `Config`'s `follow-catalogs`;
  `Config::follow_catalogs(manifest)`, the vault's winning, true when neither says.
- Tests: parsing, relative URLs from a URL and from a path, the remote/local rule, an older
  catalog with no includes unchanged, a catalog of only `[[catalog]]` entries.
- `doc/catalog.md`: the `[[catalog]]` table, the order rule, the depth and count limits, and the
  local rule.

### CC-B: following, in logoxide-app

- `catalogs.rs`: `load_catalogs(fetch, locations, base, follow, warn)` reads the named catalogs
  as now (level 0), then, when `follow`, each level's includes in order, through `load_catalog`
  (so the cache and freshness apply unchanged), with a `HashSet` of URLs read, `MAX_DEPTH`, and
  `MAX_INCLUDED`. An include that can't be read is a warning (never the whole load's error: the
  catalogs the user named were read). Each included `Catalog` gets `via` and, when it has no
  name, the include's `name`, else its URL.
- `Logoxide::catalogs`, `import::find_entry`, and `import::plan_updates` pass the setting
  through; `lx`'s `cmd/catalog.rs` `gather` too.
- `refresh` follows includes when no names are given (each included URL a `Refreshed` with
  `via`); with names, only those.
- Tests with the `Table` HTTP: two levels, a cycle (A includes B includes A), a self-include,
  depth past the limit, the count limit, an include that fails, offline with one kept and one
  not, a remote catalog including a path (passed over), id collisions resolved breadth-first,
  `follow = false`.

### CC-C: lx

- `lx search`: ` (via <name>)` after an included catalog's name; JSON `via`.
- `lx catalog list`: included catalogs under their includer, indented, with descriptions, read
  from what's kept (a list never downloads; `--refresh` does); JSON `included`.
- `lx catalog refresh`: says each included catalog refreshed or failed, under its includer.
- `lx catalog build` keeps the replaced catalog's `[[catalog]]` entries (CC9).
- `tests/cli.rs`: a vault whose one catalog (a file) includes another (a file beside it), found
  by search and by `lx import`; `follow-catalogs = false` in the vault.

### CC-D: the bindings and the MCP

- FFI: `LogoxideInstaller.loadCatalogs` follows includes through the round's `HostHttp`, so each
  new URL is one more `CatalogsStep::Fetch` for the host to answer (the host's loop doesn't
  change). `CatalogEntry` gains nothing; `LogoxideCatalog.includes()` lists them (`url`, `name`,
  `description`, `via`). A test that a two-level load asks for both URLs in turn. Then `npm run
  pack:wasm`.
- MCP: `search_catalogs` gets includes through `Logoxide::catalogs` unchanged; each result's
  `catalog` gains `via` (through `Untrusted`, like every catalog field).
- `doc/catalog.md`'s final pass; `doc/architecture.md`'s `catalog.rs`, logoxide-app, lx, and FFI entries.

## Not in this plan

- Signing catalogs, or trusting one more than another: catalogs are pointers, and what's
  installed is checked by `sha256` and `checksum` as now (`doc/trust.md`).
- A hosted list of catalogs from logoxide itself (the owner can't host one).
- Including a catalog conditionally (by language or kind): searches filter results already, so
  a filter on the include would only save downloads.
