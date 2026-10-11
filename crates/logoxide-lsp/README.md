# logoxide language server

`logoxide-lsp` speaks the Language Server Protocol over stdin and stdout. It was topos's `topos-lsp`
until October 2026; it moved here so its Bible text can come from logoxide packages, while topos
stays references only:

- **Completion**: book names (`1 Co` → `1 Corinthians`), then chapters and verses from the
  book's versification; each completion rewrites the whole reference (`jn 3:1` → `John 3:16`).
  With a `bible`, chapters and verses resolve (`completionItem/resolve`) to their text
- **Hover**: the reference under the cursor, normalized, with what the `hover` setting lists:
  the passage's text (with a `bible`), its full name and abbreviation, OSIS id, book (and its
  number and chapters), testament, genres, verses, and where it is. On a reference that doesn't
  exist, what does (`John 3 has 36 verses`, `John has 21 chapters`). With `commentaries`, the
  project's commentaries' notes on the passage follow its text; with `strongs`, hovering a
  Strong's number (`G25`, `H430`) shows its dictionary entry (see
  [Commentaries and Strong's numbers](#commentaries-and-strongs-numbers))
- **Go to definition** and **declaration** (with a `bible`): the passage in its whole book, a read-only document
  like `logoxide://BSB/JHN` (see [Whole books](#whole-books))
- **Inlay hints**: after each reference, how it's written in your format (where that differs,
  like `jn 3:16` → `John 3:16`), or its OSIS id
- **Document symbols**: every reference in the file, for outlines and pickers
- **Go to references**: every reference in the workspace that is exactly the one under the
  cursor (`--exact-overlap`), however it's written. In a whole book, every reference that names
  the verse under the cursor (`--explicit-overlap`), or on a chapter's label, every reference
  that shares a verse with the chapter
- **Code actions**: on a reference, reformat it the way completion writes it (`Reformat as
  "John 3:16"`, and `Reformat all N references in this file` when several differ), or search the
  workspace for it: `Search "John 3:16" for explicit overlap`, `... for any overlap`,
  `... for exact overlap`, and `Search inside "John 3"`. With a `bible`, one action per
  `insertions` entry (`Insert Callout`) writes the passage's text below the reference, or in
  place of its line or of the reference
- **Diagnostics**, pushed, or pulled when the editor asks for them (`textDocument/diagnostic`, for
  editors that support it): an information diagnostic on each reference with how it's written in your
  format (`John 3:16`, code `reference`), and warnings for references that do not exist, like
  `John 3:99` (code `missing`)

Workspace searches cover every text file in the workspace folders, skipping hidden, binary, and
ignored files (`.gitignore`, `.ignore`, `.toposignore`), like the CLI. Open files are searched
as they are in the editor, including unsaved changes. With `epub` (and `pdf`), books are searched
too: each is read once, then its references come from topos's cache (the CLI's), and a result
opens the book's text at the reference (logoxide.nvim opens `logoxide-file://` documents).

Install with:

```sh
cargo install --path crates/logoxide-lsp     # in a logoxide checkout, next to a topos checkout
cargo install --path crates/logoxide-lsp --features pdf   # for the `pdf` setting (builds MuPDF)
```

## Settings

The server reads the CLI's `~/.config/topos/config.toml` (or `$XDG_CONFIG_HOME/topos/config.toml`),
so one file configures both. It uses these keys and ignores the others:

| Key | Effect |
|---|---|
| `format`, `psg-fmt`, `fmt-*` | How references are written in completions, hover, hints, symbols, and action titles (see the CLI's "Writing references") |
| `data`, `merge-data`, `remove-data` | Custom book data |
| `context-book`, `context-heading` | References without a book (`3:16`) |
| `ext` | Extensions searched in the workspace, like `"md,txt"` (all text files by default) |
| `epub` | Search EPUBs too, whatever `ext` says (`false` by default). Their references are kept in topos's cache, shared with the CLI, and are in `logoxide-file:///…/Gill.epub` documents: the book's text, from the `logoxide/fileText` request (`{ uri }`, giving `{ title, text }`) |
| `pdf` | Search PDFs the same way (`false` by default; only in a build with `--features pdf`) |
| `inlay-hints` | `"changed"` (default), `"always"`, `"osis"`, or `"never"` |
| `reference-diagnostics` | The diagnostic on each reference: `"info"` (default), `"hint"`, or `"never"` |
| `complete-typed` | Completion offers the chapter or verse already typed, first (`Matthew 1` offers `Matthew 1`, then `Matthew 10`, ...), so its passage shows; `true` by default. The CLI's `--complete` uses it too |
| `hover` | What a hover shows below the reference, in order (a list or `"osis,book"`; `[]` for none): `text` (the passage, from `bible`), `written`, `name`, `abbreviation`, `osis`, `book`, `testament`, `genres`, `verses`, `bcv`, `location`. The default is every one but `written` and `bcv`. `name` and `abbreviation` are left out when the reference is already written that way |
| `hover-trim` | Hover and completion documentation leave out the leading whitespace of a passage's text (a note's, an entry's), like the blank line a heading displayed as `"\n{text}"` starts with; `false` by default |
| `hover-max` | How many bytes of a passage's text (a note's, an entry's) hover and completion documentation show before cutting it with ` …`: `2000` by default, `0` for no limit |
| `bible` | A Bible's folder: a logoxide package (with a `logoxide.toml`; USX or USFM, named by its abbreviation), or a folder of USX files, one book each (`JHN.usx`, `44JHNBSB.usx`, or any name with a `<book code>` inside), as Paratext, the Digital Bible Library, and eBible.org publish them, named after the folder. Its files are left out of workspace searches |
| `bibles` | More Bibles: package ids from the project's `logoxide.lock` (`["bsb", "kjv"]`), or folders by name: `{ KJV = "~/Bibles/KJV" }` or `{ KJV = { path = "~/Bibles/KJV", aliases = ["AV"] } }` (see [Translations](#translations)). Without `bible` or `bibles`, the project's Bibles are used (not those turned off with `lx disable`) |
| `project` | The folder whose `logoxide.toml` and `logoxide.lock` name the Bibles: by default a workspace folder that has one (a vault), else the library's own (`lx add` without a project) |
| `library` | The logoxide library the packages are installed in (`LOGOXIDE_HOME`, else `$XDG_DATA_HOME/logoxide`) |
| `default-bible` | The Bible for references that name none (a name or alias): `bible`'s by default, else the first of `bibles` by name |
| `hover-bibles` | Bibles whose text hover also shows, after the one the reference names (`["BSB", "KJV"]`); one numbered differently shows the passage as it numbers it when the project has a versification map package between the two |
| `commentaries` | Commentaries whose notes on the passage hover shows after its text: `true` for every commentary the project uses (`kind = "commentary"`, not those turned off), or their ids (`["mhcc"]`); none by default |
| `strongs` | Hovering a Strong's number shows its entry in the project's Strong's dictionaries (`kind = "lexicon"` with `keys = "strongs-greek"` or `"strongs-hebrew"`); `false` by default |
| `markup` | How Bible text is written (see below) |
| `insertions` | Code actions that insert a passage's text (see below) |

The CLI skips `inlay-hints`, `reference-diagnostics`, `hover`, `bible`, `bibles`, `project`,
`library`, `default-bible`, `hover-bibles`, `markup`, and `insertions`, so they can go in the shared `config.toml`.

`markup` has a table for each kind of element, with `show` and `display` (the defaults are
shown; other keys, like an editor's `color`, are ignored):

```toml
[markup]
verse_numbers = { show = true, display = "[{verse}]" }
chapter_numbers = { show = true, display = "{name} {chapter}" } # in whole books, on their own line
headings = { show = true, display = "{text}" }
notes = { show = false, display = "[{text}]" } # footnotes
words_of_jesus = { show = true, display = "{text}" }
added = { show = true, display = "{text}" }        # translators' additions (KJV italics)
divine_name = { show = true, display = "{text}" }  # LORD in small capitals
quotes = { show = true, display = "{text}" }       # quotations other than words of Jesus
width = 80 # break lines longer than this at a space (none by default)
```

Labels (verse and chapter numbers) can use `{verse}` (`16`, or `4-5` for a bridge),
`{chapter}`, `{name}` (`John`), and `{book}` (the USX code, `JHN`). The other elements' `display`
is a template with `{text}` (and `{name}`), like `## {text}` or `“{text}”`, glued to the element's
words. Headings and notes that aren't shown are left out; other elements that aren't shown are
plain text, without their template or color.

Each insertion has a `name` (the action is `Insert {name}`, or `Insert {name} (KJV)` with several
Bibles), a `display` (default `{text}`; also `{reference}` as formatted, `{osis}`, and `{bible}`,
the Bible's name), a `line_prefix` written before each line of `{text}`,
its own `markup` over the one above, and `replace`: `"line"` (the reference's whole line),
`"inline"` (just the reference, keeping the rest of its line), or nothing (the lines below):

```toml
[[insertions]]
name = "Callout"
display = "> [!bible] {reference}\n{text}"
line_prefix = "> "
markup = { headings = { show = false } }
```

The editor's settings override the file: as initialization options, or as `settings` (changed
while the server runs, without a restart). They take the same keys, at the top level or under
`logoxide`. Two more choose the file: `config` (another path) and `no-config` (none). A `format`
can also be a whole object, like `psg-fmt`. Settings that can't be used are reported as a
warning, and the previous ones stay.

## Translations

Each Bible (`bible`, and each of `bibles`) is a translation: its name, or an alias, right after a
reference (`John 3:16 KJV`, `John 3:16, KJV`, `John 3:16 (AV)`) picks it, ignoring case, and
other references use `default-bible`. Then:

- Hover (on the reference or the name) shows that Bible's text, and the `hover-bibles` after it;
  with several Bibles, each text is labeled with its name
- Go to definition opens its book (`logoxide://KJV/JHN`)
- Completion offers the names after a reference (`John 3:16 K` → `KJV`), whose documentation is
  that Bible's text
- The reference is checked against that Bible's own verse counts, read from its USX markers when
  it loads (with an ESV, `3 John 1:15 ESV` exists, though the default counts, the KJV's, end 3 John
  at verse 14), and diagnostics name it: `3 John 1:16 does not exist in the ESV`
- Insertions use it; a reference without one offers an insertion per Bible

The `logoxide/bibles` request (no parameters) lists them, the default first: `{ "bibles": [{ "name":
"BSB", "aliases": [], "books": ["GEN", ...] }] }`. The `logoxide/sameVerse` request (`{ "uri":
"logoxide://BSB/JHN", "position": { "line": 3, "character": 0 }, "bible": "KJV" }`) gives the
location of the verse at that position in another Bible's book (else its chapter, else the book's
start), or `null` if that Bible lacks the book, for showing two Bibles side by side.

## Whole books

With a `bible`, go to definition returns a document named like `logoxide://BSB/JHN` (the
Bible's name, then the book's USX code), which no file backs. The editor asks for its text with
the `logoxide/book` request (`{ "uri": "logoxide://BSB/JHN" }` gives `{ "title": "John", "text":
"..." }`), shows it read-only, and opens it with the server like any document. The server then
provides:

- **Semantic tokens**: `verseNumber`, `chapterNumber`, `wordsOfJesus`, `heading`, `note`,
  `added`, `divineName`, and `quote` (other documents have none), for the editor to color
- **Go to references** from a verse, as above; **hover** on a verse says how many references
  name it and in how many files (`???` if searching takes more than half a second; files read
  until then are cached)
- Hover and go to definition on the references in its headings; no diagnostics, inlay hints, or
  reformat actions

## Passage text in hover

A passage's text in hover and completion documentation is a Markdown code block whose info
string, after the language, says what to color: `line:column:length:type` for each token, with
byte columns in the block's text and the types numbered as in the semantic tokens legend:

````markdown
```logoxide 0:0:19:3 1:0:4:0 1:33:4:0
Jesus and Nicodemus
[16] For God so loved the world. [17] ...
```
````

Editors show only the language; the Neovim plugin colors the block from it. Completion
documentation is the block without the token list, since completion menus (like blink.cmp) show
the info string; the list is in the resolved item's `data.tokens` instead. With `markup.width`, a line break replaces a space, so every byte offset (verses,
tokens) is the same as without it.

## Commentaries and Strong's numbers

Commentaries and dictionaries come from the project (`lx add` them as any package; see
`doc/entries.md` for the dictionary format). With `commentaries = true` (or a list of ids),
hovering a reference shows, after the Bible text, each note the commentaries have on any of its
verses, labeled with the commentary's name and the verses the note covers:

````markdown
*MHCC* (John 3:16-18)
```logoxide
God so loved the world, ...
```
````

A note is cut like passage text (after 2000 bytes), and after eight notes from one commentary
the rest are counted (`*MHCC*: 28 more notes`). The passage is looked up as written, in the
commentary's own numbering. A verse the commentary skips has no note, unless
`stretch-notes = true` (off by default): then a note also covers the verses after it up to the
commentary's next note in the chapter, so hovering John 3:19 shows a note on 3:17-18.

With `strongs = true`, hovering a Strong's number (an uppercase `G` or `H`, one to five digits,
perhaps a letter: `G25`, `G0025`, `H430a`; standing alone, so `h1` or `AG25` isn't one) shows its
entry in each of the project's Strong's dictionaries that has it (Greek for `G`, Hebrew for `H`),
under the number in normal form and its headword (`**G25** ἀγαπάω`), each labeled with its
dictionary's name when the project has several.

Each commentary and dictionary is opened once when the settings are read and kept: a
commentary's books are parsed when first needed (and kept in logoxide's cache, as a Bible's
are), a dictionary's files are indexed when it opens and an entry's text is read when it's
looked up. The `markup` settings write notes and entries, without verse or chapter labels.

## Neovim

The [logoxide.nvim](https://github.com/Logoxide/logoxide.nvim) plugin sets up the
server, builds it, opens whole books as read-only buffers colored by `markup`, opens the code
actions' results in Telescope (or the quickfix list), and adds search commands and pickers.

Without the plugin (Neovim 0.11+):

```lua
vim.lsp.config('logoxide', {
  cmd = { 'logoxide-lsp' },
  filetypes = { 'markdown', 'text' },
  root_markers = { '.git', '.obsidian' },
  init_options = { ['psg-fmt'] = { join_adjacent = true }, ['inlay-hints'] = 'always' },
})
vim.lsp.enable('logoxide')
```

Go to references and completion then work. The code actions need a `vim.lsp.commands['logoxide.search']`
handler to show their results (see below; the plugin's is in its `lua/logoxide/init.lua`), and
go to definition needs the editor to open `logoxide://` documents (the plugin's
`lua/logoxide/book.lua`).

## Code actions in other editors

Reformat actions in an unnamed buffer (whose URI, `file://`, names no file) carry their edits as the
`logoxide.applyEdits` command (`{ "edits": [TextEdit] }`) for the editor to apply to that buffer;
in named files they are ordinary workspace edits. Completion lists are marked incomplete, so
editors ask again as you type instead of filtering a stale list.

LSP has no standard way for a server to open the editor's references list. The code actions
run the `logoxide.search` command, which returns the matching locations
(`workspace/executeCommand` with `{ "mode": "explicit-overlap" | "any-overlap" |
"exact-overlap" | "inside", "passage": "John 3:16" }`); the editor shows them.
