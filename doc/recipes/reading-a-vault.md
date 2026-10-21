# Reading a vault

An app that reads the Bibles its user already keeps with `lx`, rather than downloading its own:
a script, a desktop app, or a tool beside the user's notes. A vault is a folder with a
`logoxide.toml` (the packages it uses) and a `logoxide.lock` (their exact text); the packages
themselves are in the library, shared by every vault on the machine. Reading the same vault means
the app, `lx`, the language server, and the Obsidian plugin agree on which Bibles there are, what
they're called, and what John 3:16 says in each.

The app only reads: `lx` imports, installs, and updates. Making the vault from an app (adding a
dependency, installing it with the host's downloads) is in the bindings' README, under
[A vault's project](../../crates/logoxide-ffi/README.md#a-vaults-project) and
[Installing](../../crates/logoxide-ffi/README.md#installing).

- [A vault to read](#a-vault-to-read)
- [Where the library is](#where-the-library-is)
- [A passage, and the same passage in several Bibles](#a-passage-and-the-same-passage-in-several-bibles)
- [The vault's packages](#the-vaults-packages)
- [In WebAssembly](#in-webassembly)
- [What can go wrong](#what-can-go-wrong)

## A vault to read

The examples read a vault with two Bibles, made with `lx` ([its README](../../crates/lx/README.md),
and [workflow.md](../workflow.md) for the whole walk through):

```sh
cd ~/Notes
lx init                       # a logoxide.toml here
lx import ebible:engwebp      # the World English Bible, from eBible.org (WEBP)
lx import fetch:eng_bsb       # the Berean Standard Bible, from fetch.bible (BSB)
lx read "John 3:16 BSB"
```

The vault's `logoxide.toml` is then:

```toml
# The Bibles this uses: `lx add --git URL`
[dependencies]
ebible-engwebp = { import = "ebible:engwebp" }
fetch-eng_bsb = { import = "fetch:eng_bsb" }
```

## Where the library is

`lx` finds the library and the cache the same way everywhere, and so should an app:

| | First of |
|---|---|
| Library | `LOGOXIDE_HOME`; `library` in `~/.config/logoxide/config.toml` (`$XDG_CONFIG_HOME/logoxide`, or the file `LOGOXIDE_CONFIG` names); `$XDG_DATA_HOME/logoxide`; `~/.local/share/logoxide` |
| Cache | `LOGOXIDE_CACHE`; `cache` in that config; `$XDG_CACHE_HOME/logoxide`; `~/.cache/logoxide` |

These are the same on macOS as on Linux (under `$HOME`), not each system's own places. On
Windows, where `HOME` is usually unset, logoxide 0.1.1 falls back to the current folder, so set
`LOGOXIDE_HOME` and `LOGOXIDE_CACHE` there (Python's `Path.home()` and Node's `homedir()` would
look in the user's profile instead, and not find what `lx` made). In Rust, logoxide's `Config`
finds them as `lx` does. The bindings don't, so Python and TypeScript look for themselves:

```python
import os
import pathlib
import sys
import tomllib

from logoxide import BookTextStyle, Logoxide, LogoxideErrorException, PackageLocationLibrary

lx = Logoxide()


def env_dir(name: str) -> pathlib.Path | None:
    value = os.environ.get(name)
    return pathlib.Path(value) if value else None


def library_dirs() -> tuple[str, str]:
    """Where lx keeps its library and its cache"""
    path = env_dir("LOGOXIDE_CONFIG") or (env_dir("XDG_CONFIG_HOME") or pathlib.Path.home() / ".config") / "logoxide" / "config.toml"
    config = tomllib.loads(path.read_text()) if path.is_file() else {}
    configured = lambda key: path.parent / pathlib.Path(config[key]).expanduser() if key in config else None
    library = env_dir("LOGOXIDE_HOME") or configured("library") or (env_dir("XDG_DATA_HOME") or pathlib.Path.home() / ".local/share") / "logoxide"
    cache = env_dir("LOGOXIDE_CACHE") or configured("cache") or (env_dir("XDG_CACHE_HOME") or pathlib.Path.home() / ".cache") / "logoxide"
    return str(library), str(cache)


vault = sys.argv[1] if len(sys.argv) > 1 else str(pathlib.Path.home() / "Notes")
library, cache = library_dirs()
```

(`path.parent / …` keeps an absolute path as it is, and puts a relative one beside the config
file, as `lx` does.)

```rust
use std::path::PathBuf;

use logoxide::config::Config;
use logoxide_app::{Logoxide, Style};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// What `lx` works with: the same config, library, and cache
fn logoxide() -> Result<Logoxide> {
    let config = Config::load()?;
    let library = config.library(None, None);
    Ok(Logoxide::default_native(config, library, "my-bible-app/1.0", logoxide_app::quiet()))
}
```

In Rust the dependencies are `logoxide` and `logoxide-app`. `default_native` also sets up
downloads and `git` for an app that installs; for reading, nothing is downloaded.

## A passage, and the same passage in several Bibles

`readPassage` is `lx read`: the passage in the Bible its reference names (by abbreviation or
alias, as the vault names it), else the vault's default (`[settings] default`), else its first.
`comparePassage` is `lx compare`: one passage in several Bibles, each in its own numbering
through the vault's versification maps, saying what of it one lacks. Both read the books through
`lx`'s cache, so a book `lx` has read is quick for the app too, and the other way round.

A `versification` of `own` means the Bible numbers its verses its own way, as its files do,
which an import does unless its publisher says otherwise.

```python
plain = BookTextStyle(verse_label="[{verse}]", chapter_label=None, notes=False, headings=False, templates=[], width=None, limit=None)
read = lx.read_passage(vault, library, cache, "John 3:16 BSB", plain)
print(read.abbreviation, read.passage.reference, read.versification)  # → BSB John 3:16 own
print(read.text.text)  # → [16] For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.
print(lx.read_passage(vault, library, cache, "Romans 8:28", plain).abbreviation)  # → WEBP
for c in lx.compare_passage(vault, library, cache, "John 3:16", ["WEBP", "BSB"], plain):
    print(c.abbreviation, c.text.text[:40], c.mapped, repr(c.missing))
# → WEBP [16] For God so loved the world, that he False ''
# → BSB [16] For God so loved the world that He  False ''
```

```rust
fn read(vault: PathBuf) -> Result<()> {
    let lx = logoxide()?;
    let project = lx.project(Some(vault))?;
    let style = Style::reading();
    let read = lx.read(&project, "John 3:16 BSB", &style)?;
    println!("{} {} {}", read.abbreviation, read.reference, read.versification); // BSB John 3:16 own
    println!("{}", read.text.map(|t| t.text).unwrap_or_default()); // [16] For God so loved the world …
    for c in lx.compare(&project, "John 3:16", &["WEBP".into(), "BSB".into()], &style)? {
        println!("{} {}", c.abbreviation, c.text.map(|t| t.text).unwrap_or_default()); // WEBP [16] For God so loved the world, …
    }
    Ok(())
}
```

`text` is `null` (`None`) when the Bible hasn't the passage: `Tobit 1:1` in either of these.
The style is the language server's ([A book's text](../../crates/logoxide-ffi/README.md#a-books-text)):
headings, notes, labels, templates for words of Jesus, a width to wrap at.

## The vault's packages

`parseProject` reads the vault's two files; `packages()` lists each package as the lock has it,
with where its files are: in the library, or a path package's folder in the vault. The vault can
rename a package (`abbreviation = "WEB"` in its dependency), so `abbreviation` and `aliases` on an
entry win over the package's own. Opening one gives its `info()`: name, language, license, and
what its license allows (`rights`), to show beside its text.

```python
from logoxide import PackageFeatures

project = lx.parse_project(pathlib.Path(vault, "logoxide.toml").read_text(), pathlib.Path(vault, "logoxide.lock").read_text())
for entry in project.packages():
    folder = pathlib.Path(library if isinstance(entry.location, PackageLocationLibrary) else vault, entry.location.dir)
    package = lx.open_package_dir(str(folder), entry.features, cache)
    info = package.info()
    print(entry.name, entry.version, entry.enabled, info.abbreviation, info.license, info.rights.redistributable)
# → ebible-engwebp 1.0.0 True WEBP LicenseRef-PublicDomain True
# → fetch-eng_bsb 1.0.0 True BSB LicenseRef-PublicDomain True
```

A package turned off (`lx disable`) is still listed, with `enabled` false: leave it out.

## In WebAssembly

The npm package can't read folders, so `readPassage` and `openPackageDir` are `Io` errors there.
A Node or Electron app (or an Obsidian plugin, through the vault's file API) reads the files
itself: the lock's packages, each opened from its files, and topos told their names so a
reference can name one. Opening counts every book's verses the first time (two or three seconds
for a Bible), so keep what `cacheWrites()` hands back in a cache folder of the app's own and give
it back next time (a tenth of a second). It's the same layout `lx` uses, but writing into `lx`'s
cache from another program isn't needed.

```ts
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, relative, sep } from "node:path";
import { BookStyle, Logoxide, OffsetUnit, PackageKind, Topos, ToposTranslations, type LockedEntry, type LogoxidePackage, type PackageFile } from "logoxide";

const lx = Logoxide.new();
const topos = Topos.new();

/** Every file under a folder but git's, with `/`-separated paths relative to it */
function files(dir: string, root = dir): PackageFile[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.name !== ".git")
    .flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return files(path, root);
      return [{ path: relative(root, path).split(sep).join("/"), bytes: new Uint8Array(readFileSync(path)) }];
    });
}

/** Where lx keeps its library: LOGOXIDE_HOME, else config.toml's `library`, else $XDG_DATA_HOME/logoxide */
function libraryDir(): string {
  if (process.env.LOGOXIDE_HOME) return process.env.LOGOXIDE_HOME;
  const config = process.env.LOGOXIDE_CONFIG ?? join(process.env.XDG_CONFIG_HOME ?? join(homedir(), ".config"), "logoxide", "config.toml");
  const library = existsSync(config) ? /^library\s*=\s*"([^"]*)"/m.exec(readFileSync(config, "utf8"))?.[1] : undefined;
  if (library) return library.startsWith("~/") ? join(homedir(), library.slice(2)) : join(dirname(config), library);
  return join(process.env.XDG_DATA_HOME ?? join(homedir(), ".local", "share"), "logoxide");
}

const vault = process.argv[2] ?? join(homedir(), "Notes"); // the folder with the logoxide.toml
const library = libraryDir();
const cache = join(homedir(), ".cache", "my-bible-app"); // the app's own
const project = lx.parseProject(readFileSync(join(vault, "logoxide.toml"), "utf8"), readFileSync(join(vault, "logoxide.lock"), "utf8"));
const folder = (entry: LockedEntry) => join(entry.location.tag === "Library" ? library : vault, entry.location.dir);

const cached = files(cache); // what earlier runs saved
const bibles = new Map<string, LogoxidePackage>();
let names = ToposTranslations.create();
for (const entry of project.packages()) {
  if (!entry.enabled) continue;
  if (!existsSync(folder(entry))) {
    console.log(`${entry.name} isn't installed here: run lx install in ${vault}`);
    continue;
  }
  const pkg = lx.openPackage(files(folder(entry)), entry.features, cached);
  for (const written of pkg.cacheWrites()) {
    mkdirSync(dirname(join(cache, written.path)), { recursive: true });
    writeFileSync(join(cache, written.path), written.bytes);
  }
  const info = pkg.info();
  if (info.kind !== PackageKind.Bible) continue;
  // The vault can rename a package (`abbreviation = ` in its dependency)
  const abbreviation = entry.abbreviation ?? info.abbreviation ?? entry.name;
  bibles.set(abbreviation.toLowerCase(), pkg);
  names = pkg.addToAs(names, abbreviation, entry.aliases ?? info.aliases);
  console.log(entry.name, entry.version, abbreviation, info.license);
}
// → ebible-engwebp 1.0.0 WEBP LicenseRef-PublicDomain
// → fetch-eng_bsb 1.0.0 BSB LicenseRef-PublicDomain

/** A passage in the Bible its reference names, else the vault's default, else its first */
function read(reference: string): string | null {
  const found = topos.parseWithTranslation(reference, OffsetUnit.Utf16, names, BookStyle.Name);
  if (!found) return null;
  // topos leaves over a name that isn't one of these Bibles (`KJV` in `John 3:16 KJV`)
  const words = reference.trim().split(/\s+/);
  words.pop();
  if (!found.translation && words.length && topos.parse(words.join(" "), BookStyle.Name)?.reference === found.passage.reference) return null;
  const name = found.translation?.name ?? project.defaultBible() ?? [...bibles.keys()][0];
  const pkg = name ? bibles.get(name.toLowerCase()) : undefined;
  if (!pkg || !pkg.books().includes(found.passage.bookId)) return null;
  return pkg.book(found.passage.bookId).text(found.passage, { notes: false, headings: false, classPrefix: "" }).join("\n");
}

console.log(read("John 3:16 BSB")); // → For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.
console.log(read("John 3:16")); // → For God so loved the world, that he gave his only born Son, that whoever believes in him should not perish, but have eternal life.
console.log(read("John 3:16 KJV")); // → null
```

This leaves out what `readPassage` does besides: versification maps, so a passage numbered one way
is found in a Bible numbered another (the bindings' `versifications()`, in
[Versification maps](../../crates/logoxide-ffi/README.md#versification-maps)).

## What can go wrong

- **Not installed.** A vault synced to another machine has its `logoxide.lock`, but the library
  there may not have the packages yet: `readPassage` is a `NotFound` error saying
  `ebible-engwebp 1.0.0+if1bede20 (not installed: run `lx install`)`. Tell the user to run
  `lx install` in the vault, which gets exactly the locked text.
- **A Bible it hasn't.** A reference naming a Bible the vault hasn't is a `Resolve` error:
  `no Bible `KJV` in this project (lx list)`. In Rust, `error.kind()` is `Resolve`.
- **No vault.** A folder without a `logoxide.toml` has no Bibles: `readPassage` is a `Resolve`
  error, `this project has no Bibles (lx add, lx import)`.

```python
try:
    lx.read_passage(vault, library, cache, "John 3:16 KJV", plain)
except LogoxideErrorException as error:
    print(type(error.error).__name__, error.error.message)  # → LogoxideErrorResolve no Bible `KJV` in this project (lx list)
```

```rust
fn errors(vault: PathBuf) -> Result<()> {
    let lx = logoxide()?;
    let project = lx.project(Some(vault))?;
    if let Err(error) = lx.read(&project, "John 3:16 KJV", &Style::reading()) {
        println!("{:?}: {error}", error.kind()); // Resolve: no Bible `KJV` in this project (lx list)
    }
    Ok(())
}
```
