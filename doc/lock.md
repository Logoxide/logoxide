# `logoxide.lock`

The exact text a vault (or a machine's library) uses, like `Cargo.lock`: each package's version
and where it came from, pinned by commit or archive hash and by checksum, so every device installs
the same text and links mean the same thing everywhere. It sits beside `logoxide.toml`; commit or
sync both. This describes the format as logoxide 0.1 reads and writes it; the reference is
`crates/logoxide/src/lock.rs` and `crates/logoxide/src/resolve.rs`.

```toml
# Written by lx: the exact text this uses. Commit it with logoxide.toml.
version = 1
dependencies = ["bsb 1.0.0+gb0d4882a", "romans 1.0.2+a9f86d081"]

[[package]]
id = "bsb"
version = "1.0.0"
git = "https://github.com/someone/bsb"
tag = "v1.0.0"
commit = "b0d4882a51c3…"
checksum = "sha256:44cb…"
features = ["usx"]
dependencies = ["strongs-greek 2.0.0+g3f9c2a1b"]

[[package]]
id = "romans"
version = "1.0.2"
registry = "https://example.org/registry/catalog.toml"
archive = "https://example.org/registry/romans/romans-1.0.2.tar.gz"
mirrors = ["https://archive.org/download/registry/romans-1.0.2.tar.gz"]
sha256 = "sha256:9f86d081884c…"
checksum = "sha256:41c0…"
```

## Top level

| Key | Type | Meaning |
|---|---|---|
| `version` | integer | The lock format's version: `1`. A reader must refuse any other |
| `dependencies` | array of strings | The packages the manifest asks for, each as its [name](#names) (the rest of `package` are their dependencies) |
| `package` | array of tables | Every package installed for the manifest, its dependencies' dependencies included |

## `[[package]]`

| Key | Type | Meaning |
|---|---|---|
| `id` | string | The package's id |
| `rename` | string | What the manifest calls it, when that isn't its id (`kjv-bob = { package = "kjv", … }`) |
| `version` | string | Its version |
| `git` | string | The repository it came from (the full URL, never a `github:` short form) |
| `tag`, `branch`, `rev` | string | How it was asked for (the commit is what is used) |
| `commit` | string | The commit installed (git packages): 7 to 64 lowercase hex digits |
| `folder` | string | The package's folder in its repository, when it isn't at the top |
| `path` | string | A folder used in place, relative to the manifest (path packages) |
| `registry` | string | The registry's catalog it was chosen from |
| `archive` | string | That release's archive URL |
| `mirrors` | array of strings | Other places serving the same archive, tried in order after `archive` |
| `sha256` | string | The SHA-256 of the archive's bytes (`sha256:` and hex) |
| `checksum` | string | The package's [checksum](catalog.md#checksums). Absent for path packages, whose files change as they're written |
| `features` | array of strings | Features turned on, besides the defaults |
| `default-features` | boolean | `false` when its defaults are off. Default `true` |
| `dependencies` | array of strings | The packages it depends on, as their names |

A git package has `git` and `commit`; a registry package has `registry`, `archive`, and `sha256`;
a path package has `path`.

## Names

A locked package's **key** is its version with what pins it, as semver build metadata:

- git: `<version>+g<first 8 hex digits of the commit>` (`1.2.0+g3f9c2a1b`)
- registry archive: `<version>+a<first 8 hex digits of the sha256>` (`1.2.0+a9f86d081`)
- path: `<version>`

Its **name** is `<id> <key>`, which `dependencies` lists; a package the manifest calls something
else (`rename`) is `<rename> (<id>) <key>` (`kjv-two (kjv) 1.0.0+g3f9c2a1b`). Two names for one
commit are two entries, each turned on and off and named in references apart, sharing one folder.
Lock files from before forks were supported name packages `<id> <version>`, and those from before
renames had their own names `<id> <key>`; readers should still accept both, matching by id and
key, then by id and version. The key is also the package's folder in the library
(`packages/<id>/<key>/`), so forks of a package at one version, or two registries' releases of it,
never share a folder.

## Resolving and installing

- A locked package is kept while the manifest still allows it (its requirement matches, and it
  comes from the same place), unless the user asks to update it; otherwise the newest allowed
  version is chosen: the newest `v<version>` tag matching `version` for git, or the newest
  matching unyanked release for a registry ([catalog.md](catalog.md)). Updating by name takes
  only names the manifest has or packages the lock holds (as a package brings them in); any other
  is refused, rather than updating nothing.
- A lock can hold one package at two versions (a collection asks for `kjv` 1, the manifest for
  `kjv` 2). The entry kept for a dependency is first the one the same asker locked (the
  top-level `dependencies` for the manifest's, the naming package's `dependencies` for its own),
  then any entry that still fits.
- What is fetched is checked against the lock: an archive must hash to `sha256`, and the
  package's files to `checksum`. An installed copy whose checksum no longer matches (its files
  were edited) is an error unless the user asks to repair it, which puts back its declared files
  and leaves other files alone.
- A locked commit is fetched by itself; one the host no longer has (a moved tag, a rewritten
  branch) is an error saying to update that package. Updating warns of a release that came back
  changed: a tag naming another commit, or a registry version with another archive or checksum
  (`Lockfile::moved_since`).
- A git tag's version must equal the `version` in its `logoxide.toml`.
- A git or registry package can't have `path` dependencies.
- Dependencies with `enabled = false` stay locked and installed; nothing reads them.
