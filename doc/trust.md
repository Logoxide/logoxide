# Trust

What logoxide checks when it installs a package, what it relies on without checking, and what it
never does. For users deciding what to add, and for implementers who want the same guarantees.

## In short

- **A source is trusted the first time it's used.** Adding a package trusts its git repository or
  its registry's catalog to give the text its publisher meant. Nothing is signed.
- **After that, the lock file pins it.** `logoxide.lock` records the git commit, or the archive's
  SHA-256, and the checksum of every file. Every later install, on any device, gets exactly those
  bytes or fails.
- **Licenses are shown, never enforced.** Checking that you may use or redistribute a text is up
  to you.

## First install: trust on first use

When a dependency is first resolved, logoxide takes what its source says:

- **git**: the newest `v<version>` tag matching the requirement (or the named tag, branch, or
  commit) of the repository at that URL. Whoever controls the repository, or can impersonate its
  host, controls the text. The tag's `logoxide.toml` must have the same version, and the
  package's id must be the one asked for.
- **registry**: the catalog at that URL names a release, its archive URL, and the archive's
  SHA-256 and files' checksum. Whoever controls the catalog controls which archive is accepted:
  the archive is checked against the catalog, and the catalog against nothing.
- **imports** (`lx import ebible:…`, `fetch:…`, `ccel:…`, `sword:…`, `door43:…`): the publisher's download
  (from its URL as the catalog gives it), made into a package in the project's folder. It is
  then a path package, which the project holds itself.

Catalogs and archives fetched over **plain HTTP** (any `http://` URL, including one SWORD
repository in CrossWire's list) give no integrity at all on first use: anyone on the network
path can change them. The lock still protects later installs. Prefer HTTPS sources. An HTTPS
URL stays HTTPS: `lx` refuses a redirect from one to plain HTTP, and its warning of plain HTTP
names where a download was redirected.

There are **no signatures**: no package, catalog, tag, or archive is signed, and logoxide doesn't
check git commit or tag signatures. Trust in a publisher is trust in their hosting account.

## Later installs: the lock pins everything

`logoxide.lock` ([lock.md](lock.md)) is meant to be committed or synced with the vault. For each
package it records:

- for git, the **commit** (not just the tag, which can be moved): the commit is what is fetched
- for a registry, the **archive URL** and its **sha256**: downloads (from the URL or any listed
  mirror) whose bytes hash differently are passed over, and if none match, installing fails
- for both, the **checksum** of the package's declared files ([catalog.md](catalog.md#checksums)):
  what is unpacked must hash to it, and installed copies are checked against it on every install

So a catalog changed after you locked a release, a moved tag, a compromised mirror, or a file
edited in the library is caught. A locked commit the host no longer has (its tag was moved, its
branch rewritten) fails to install, saying so. Updating (`lx update`) is a new first use: it reads
the source again and trusts its new answer, but warns when a release comes back changed (a tag now
naming another commit, a registry version with another archive or checksum: "v2.0.0 was moved").

Path packages (`path = "…"`) are the project's own files and aren't checksummed in the lock.

## What a fetched package may ask for

A package's own `[dependencies]` are resolved too, so a package from the internet names further
sources. To keep a fetched manifest from reaching into the machine it is installed on:

- a git or registry package **can't have `path` dependencies** (refused today)
- its dependencies **can't name local paths or `file://` URLs** in `git` or `registry` either,
  and a catalog fetched from the web can't list a local path or `file://` URL for a release
- its dependencies **can't use `git://`**, git's unencrypted protocol (your own manifest may,
  with a warning, as for any `http://` URL)
- git URLs and refs (tags, branches, revisions) that `git` would read as **command-line options**
  (starting with `-`) are refused, since `lx` runs the `git` command (which also runs with the
  `ext::` transport off, and never follows a symlink in a checkout)
- git has a **deadline**: a fetch is stopped after 10 minutes and listing a repository's tags
  after 60 seconds (`git-timeout` and `git-list-timeout`, in seconds, in logoxide's config), so a
  host that never answers can't hold `lx` forever
- git **runs nothing a repository asks for**: no hooks (lx's clones use an empty
  `core.hooksPath`, so not even your own), no Git LFS filter (LFS's small pointer files are
  kept as they are, so a package hashes the same whether or not LFS is installed, and nothing is
  downloaded from what a `.lfsconfig` names), no `core.fsmonitor` (also off in repositories lx
  didn't make: a package being released, a registry's folder), and SSH never asks for a
  passphrase or to trust a host (`BatchMode`), unless you set `GIT_SSH_COMMAND` or
  `core.sshCommand` yourself
- links in your own folders aren't followed where lx writes over or publishes what's there:
  `lx import --update` won't write or remove a file through a link, and `lx release` and
  `lx publish` pass links over, so a link can't put a file from elsewhere on the machine into a
  published package

What a package can still do is name other public repositories and catalogs, which are then
trusted on first use like your own dependencies. `lx` never runs anything a package contains:
packages are text, read by parsers.

## What lx prints

Names, descriptions, licenses, titles, and other text from catalogs and manifests, and what `git`
says when it fails, are printed with control characters (but newlines and tabs), terminal escape
sequences (colors, cursor movement, window titles, links), and bidirectional controls (U+202A to
U+202E, U+2066 to U+2069) taken out, so a catalog can't rewrite the terminal or make text read
in another order than it's shown. JSON output (`--format json`) is left as it is: JSON escapes
control characters itself. Other implementations that show such text in a terminal should do the
same (`logoxide::text::printable`).

## The MCP server

`logoxide-mcp` gives an assistant a project's catalogs, packages, and text over the Model
Context Protocol. An assistant follows instructions it reads, so what a catalog or a package
says must never pass for the server's own words:

- **Untrusted text is marked.** Every field from a catalog, a manifest, a lock file's sources,
  or a package's text (names, descriptions, licenses, titles, passages, notes, entries, the
  warnings loading a catalog gives) is inside an `untrusted` object naming its `source` (`the
  catalog eBible.org`, `the manifest of kjv 1.0.3`, `the text of kjv 1.0.3`). The server's
  instructions tell the client that these are data to quote or summarize, never instructions.
  What the server works out itself (ids checked as ids, versions, counts, OSIS references,
  rights as true and false) is outside it.
- **It's cleaned and capped.** Untrusted text goes through `logoxide::text::printable`, as lx's
  output does, and each field is cut at a length (300 characters for a name or a short field,
  2,000 for a description or a license statement, 16,000 for one note or entry), ending
  `… [cut]`. Errors, which may quote a manifest, are cleaned and capped too.
- **Results come in pages** of about 20,000 characters, each with a `nextCursor` for the next;
  searches give 20 results a page by default and at most 100. A cursor names the question it
  came from, and one from another question is refused.
- **Every passage carries its license**: the package's license, rights, publisher, and authors,
  and whether it's free (a free SPDX license, or rights saying it may be shared, sold, and
  changed). Text that isn't free is limited to **500 verses a call, and never more than half a
  book** (counted by the package's own verse numbers); past that its text is withheld with the
  reason. This is a limit on how much one call returns, not enforcement: licenses are shown,
  never enforced, and checking that you may use the text is still up to you.
- **It's read-only** unless started with `--allow-writes`. Only then are `install` (what the
  project's manifest asks for, as its lock pins it) and `import` (an entry of the project's own
  catalogs, by its import name: never a URL, a git repository, or a path the assistant gives)
  listed. They take the same locks as lx. Reading a release of a package's own git repository
  (`show_summary` with a version, `diff_versions`) fetches from the URL the lock already has.
- Downloads and their warnings go to the server's stderr, never stdout, which is the protocol's.

## Caches and summaries

The parsed-book cache is per device, keyed by package version, checksum, and format, and never
shared: it can always be built again from the text. Each cached book names the version that
wrote it, and one written by another version is parsed again rather than decoded. To find that
checksum without hashing every file on every open, the cache remembers it with each file's size
and modification time (`stamps/`), and uses it only while every file keeps both and none was
changed within two seconds of the stamps being taken (git's rule for racy files). A file
rewritten with the same size and time is not seen there; `lx verify`, `lx install --repair`,
`lx release`, and `lx info`'s checksum always hash. Published summaries (`logoxide.summary.toml`,
`logoxide.verses.bin`) are part of the package and covered by its checksum; `lx verify` rebuilds
them from the text to check that they match.

## Writes

`logoxide.toml`, `logoxide.lock`, a registry's `catalog.toml` and archives, and every file in
the library and the cache are written whole or not at all: to a temporary file beside the
target, synced to disk, renamed over it, then the folder synced, so a reader never sees half a
file and a crash leaves the old one. A new version is built in a staging folder beside its place
and renamed in, so a half-installed version is never taken for an installed one; a repair writes
each file atomically and removes what a version no longer has last. `lx` reads `logoxide.toml`
again before writing it and refuses (`Conflict`) if it changed since the project was opened, so
an edit made meanwhile isn't written over.

## Locks

Two `lx` processes writing at once (an install and a clean, two vaults installing into one
library) could each undo the other's work, so `lx` takes an exclusive lock on the library while
it installs, repairs, or cleans, and on a project while it writes its manifest or lock. The
locks are the operating system's advisory file locks, in `$XDG_RUNTIME_DIR/logoxide/` (else
`locks/` in logoxide's cache folder), each named by a hash of the folder's canonical path; never
in the library itself, which a sync service may copy, and a process that dies releases its lock.
Readers take none: a version is renamed into place whole, and the manifest and lock are written
whole. Taking a lock waits five seconds, then fails (`Busy`) naming the process that holds it.
Nothing is locked in WebAssembly, where the host owns the files. A version an install was writing
when it died (`packages/<id>/.<key>.staging-…`) is removed by `lx clean --remove` only under the
library's lock and only once it is a day old.

## Media

Audio and video are listed by URL, size, and SHA-256 in the package (so the lock's checksum
covers the list). A download is accepted only if its size and hash match, so any mirror is as good
as the first.

## Licenses

Each package's `license` (an SPDX expression) and `license-file` are shown by `lx show`, `lx
search`, and the bindings, and copied from publishers' own statements by importers. logoxide never
enforces them: it will install, cache, and read a text whatever its license says, and it can't
know whether a catalog's claim is true. **You must check that you have the right to use, and
especially to redistribute, a text** (in a published vault, a registry, a mirror, or an app)
before you do. Many Bible translations are under copyright and allow only personal use.

`license` must be an SPDX expression (read leniently, `LicenseRef-<name>` for the rest), and
`[package.rights]` says what it allows in four words (`redistributable`, `derivatives`,
`commercial`, `attribution`; [manifest.md](manifest.md#packagerights)), derived for licenses
logoxide knows. Importers read them from the publisher: eBible.org's license from the Creative
Commons link or name in its `copr.htm`, fetch.bible's terms, and SWORD's `DistributionLicense`
(permission given to CrossWire, or only in SWORD's format, is `redistributable = false` and
`derivatives = "none"`). A statement that denies the public domain ("not public domain") is never
read as it.

`lx publish` says what may keep a package from being shared (`logoxide::license::assess`, which
the bindings give as `LogoxidePackage.licenseConcerns()`): no license, one not known to be free,
and an import whose markup was converted when the license allows no derivatives (logoxide
doesn't count converting markup, its words unchanged, as a derivative, but the publisher might).
When the rights say a package isn't redistributable, it warns loudly and asks before publishing
(`--yes` answers; outside a terminal it must be given). This is still a warning, not
enforcement: the choice, and the responsibility, are the publisher's.
