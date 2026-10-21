# lx

logoxide's command line: Bibles, commentaries, lexicons, and books as versioned packages, kept
beside your notes. `lx` finds a text in publishers' lists, makes it a package, pins its exact
words in a lock file, and reads, compares, and maps passages from it.

```sh
cargo install logoxide-cli    # the package is logoxide-cli (`lx` on crates.io is taken); the command is lx
```

Rust 1.89 or later. Packages published as git repositories are fetched with the `git` command
where it's installed, else over HTTPS (their refs, and archives of commits).

## A vault in five commands

A vault is any folder of notes with a `logoxide.toml` saying which packages it uses, and a
`logoxide.lock` pinning their exact text. Commit or sync both with the notes.

```sh
cd ~/Notes
lx init                       # a logoxide.toml here
lx search "king james"        # eBible.org, fetch.bible, CCEL, and CrossWire's SWORD lists
lx import ebible:eng-kjv      # a publisher's download, made into a package and locked
lx read "John 3:16 KJV"       # its text
lx install                    # on another device: exactly what logoxide.lock says, or an error
```

Without a `logoxide.toml` in the current folder, `lx` uses the library's own (as `--global` does),
so it works outside a vault too.

**Licenses are shown, never enforced.** `lx search`, `lx show`, and `lx info` print each text's
license as its publisher states it, but `lx` installs and reads a text whatever it says. Check
that you may before you share a text: many translations allow only personal use.

## Finding and adding texts

```sh
lx search kjv                              # every word must match: name, abbreviation, or language
lx search --refresh spanish                # catalogs are kept a week; --offline uses only what's kept
lx import fetch:eng_bsb                    # also ebible:<id>, ccel:<id>, sword:<repository>/<module>
lx import ./web-burrito.zip                # a Scripture Burrito or DBL bundle, by URL or path
lx import --update                         # make the imports again; replace the ones that changed
lx add github:someone/bsb                  # a package published as a git repository (newest release)
lx add github:someone/bsb --version 1      # or --tag, --branch, --rev
lx add github:bob/kjv --name kjv-bob --abbreviation KJVB   # a fork beside the KJV
lx add --path "Bibles/My Translation"      # a folder used in place
lx catalog add https://example.org/catalog.toml            # search another list too
```

An import is kept in the shared library by a checksum of its files, so every vault importing the
same download shares one copy. `lx import --vendor` puts it in the vault's `imports/` instead.

## Keeping a vault

```sh
lx list                       # what logoxide.toml asks for, as locked; --all: every version in the library
lx info kjv                   # one package: where, how big, its books, whether its files are still locked
lx update [id]                # newer versions the requirements allow
lx disable kjv-bob            # off without removing it (enabled = false); lx enable turns it back on
lx is-enabled kjv-bob         # prints on or off; exits 0 if on, 1 if off (for scripts)
lx remove kjv-bob             # out of logoxide.toml; its files stay in the library
lx clean                      # versions no lock here uses; --keep <project> spares others; --remove deletes
```

## Reading

```sh
lx read "Psalm 23 BSB"                     # the Bible named after it, else the first
lx compare "Psalm 51:1" --bibles KJV,BSB   # each in its own numbering, saying what one lacks
lx map "Psalm 51:1" --from English --to Original   # through the vault's versification maps
lx show path/to/mhcc --notes "John 3:16"   # a commentary's notes on a passage
lx show path/to/strongs --entry G25        # a dictionary's entry; --keys lists its keys
lx show path/to/tsk --xref "John 3:16"     # a verse's cross-references, with their weights
lx show path/to/book --references          # the passages a book's EPUBs cite, and how often
```

`lx show` takes a package's folder (the current one by default); `lx info <id>` says where a
locked package's folder is.

## Making packages

Packages, registries, and media work and are specified, but nobody hosts them yet, so they stay
as they are until someone asks.

```sh
lx new --from book.epub --license CC0-1.0  # a package from one file (an EPUB's title and authors fill it in)
lx new my-library --kind collection        # a package of packages
lx check                                   # read every book, and say what was found
lx release --set-version                   # write the summary, check the version against what changed, tag
lx diff v1.0.0                             # what changed since a release
lx verify                                  # build the summary again and compare it with the published one
lx show --git https://example.org/bsb      # a release's summary, without downloading its text
lx convert JHN.usfm --to usx               # a book (or a folder of them) between USFM, USX, and USJ
```

`lx registry`, `lx publish`, `lx yank`, `lx catalog build`, `lx works`, and `lx media` are for
hosting releases as archives, grouping editions of one work, and listing audio and video by URL
and hash. `lx <command> --help` says what each takes.

## Where things are

| | Set by | Default |
|---|---|---|
| The project | `--project <folder>`, `--global` | the current folder if it has a `logoxide.toml`, else the library's |
| The library (packages, shared) | `--library`, `LOGOXIDE_HOME`, the config's `library` | `$XDG_DATA_HOME/logoxide` |
| The cache (per device) | `--cache-dir`, `LOGOXIDE_CACHE`, the config's `cache` | `$XDG_CACHE_HOME/logoxide` |
| The config | | `~/.config/logoxide/config.toml` |

The library can be any folder, a vault's `.obsidian/` included, so it syncs with the notes. The
config can also name catalogs and registries for every project, and `catalog-max-age` (`"7d"`
by default; `"12h"`, `"30m"`, or seconds; 0 keeps none).

## Output

Text by default; `--format json` for `list`, `info`, `check`, `diff`, `verify`, `search`,
`catalog list`, `import --update`, `read`, `compare`, and `show`'s `--references`, `--notes`,
`--entry`, `--keys`, and `--xref` (`tsv` too for `--references`). Text from catalogs and
manifests is printed with control characters removed, so a package can't drive the terminal.

Exit status: 0 when it worked, 1 on an error (`lx is-enabled`: 0 on, 1 off, 2 an error).

Completion for bash, zsh, fish, and others: `source <(COMPLETE=bash lx)`, with the shell's name
in place of `bash`.

## More

- [logoxide](https://github.com/Logoxide/logoxide): what packages are, and the rest of the
  project (the language server, the MCP server, the npm and PyPI bindings)
- [A walk-through](https://github.com/Logoxide/logoxide/blob/main/doc/workflow.md): a vault and a
  package of your own, start to finish
- [`logoxide.toml`](https://github.com/Logoxide/logoxide/blob/main/doc/manifest.md),
  [`logoxide.lock`](https://github.com/Logoxide/logoxide/blob/main/doc/lock.md),
  [catalogs and registries](https://github.com/Logoxide/logoxide/blob/main/doc/catalog.md), and
  [what is trusted](https://github.com/Logoxide/logoxide/blob/main/doc/trust.md)

License: CC0-1.0
