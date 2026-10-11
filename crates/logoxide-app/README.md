# logoxide-app

What every logoxide front end does, written once: lx, the bindings, and a language or MCP server
call these rather than keeping copies.

- **HTTP** (`http`): an `Http` trait (`get`, and `get_to` streaming to a sink); `UreqHttp` with
  the `ureq` feature (on by default: timeouts, the environment's proxy, three tries, HTTPS kept
  HTTPS). Without it the host gives its own, so WebAssembly builds without a network stack
- **Fetching** (`fetch`): `GitFetcher` through the `git` command, `NetFetcher` over HTTP alone
  (git's smart-HTTP refs and archives of commits) where there's no `git`, and `fetcher` to pick
- **Catalogs** (`catalogs`): a project's catalogs and registries, from URLs, files, or a folder of
  repositories; `registry_url` resolves `registry:name`
- **Imports** (`import`, `importer`): `find_entry`, `run` (an `import` dependency, made by
  `importer::Importing` as the resolver asks and kept in the library by its checksum),
  `run_vendored` (download, stage, move into place in `imports/<id>/`, add, install, or roll
  back), `plan_updates` and `apply_update`
- **Cleaning** (`clean`), **media** (`media::verify`, hashing as it downloads)
- **Reading** (`read`): `read(project, "John 3:16 KJV")` and `compare(passage, bibles)`, mapped
  through the project's versification maps, as logoxide-text's passages
- **`Logoxide`**: the facade holding the config, library, `Http`, fetcher, and text reader

Nothing here prints: downloads and plain-HTTP warnings go to a `Notes` callback, and what to warn
of comes back with each result.
