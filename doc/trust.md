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
- **imports** (`lx import ebible:…`, `fetch:…`, `ccel:…`, `sword:…`): the publisher's download
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
edited in the library is caught. Updating (`lx update`) is a new first use: it reads the source
again and trusts its new answer.

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

## Caches and summaries

The parsed-book cache is per device, keyed by package version and checksum, and never shared:
it can always be built again from the text. Published summaries (`logoxide.summary.toml`,
`logoxide.verses.bin`) are part of the package and covered by its checksum; `lx verify` rebuilds
them from the text to check that they match.

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
