# Recipes

Each is a full, tested walk-through in TypeScript, Python, and Rust; fetch the one that matches
the task and adapt it. They build on the helpers in the guide (`lx`, `topos`, `get`).

Base: `https://github.com/Logoxide/logoxide/blob/main/doc/` (raw text:
`https://raw.githubusercontent.com/Logoxide/logoxide/main/doc/`).

| Task | File | What to take from it |
|---|---|---|
| Get text into the app | `apps.md` | sources, catalogs, `bookDownloads`, `openBible`, uploads, verses, caching |
| Uploads the user edits | `recipes/uploaded-translations.md` | a stable id apart from name/abbreviation; keep each upload's files with the books read from them; rename (old abbreviation kept as a topos alias); add books by reading only the new files (same book replaces); remove a book; read again when `textVersion()` changes |
| A reader view | `recipes/showing-passages.md` | `html`/`chapterHtml` with `classPrefix`, a stylesheet numbering verses from `data-n`, `styledText` and `formatPassage` for copying with a citation, next/previous chapter, a verse of the day |
| References in user text | `recipes/references-in-text.md` | `search`/`parts` with the right `OffsetUnit`, rewriting Markdown from the end, hover text, storing as OSIS, `complete`, `overlaps` |
| Parallel translations | `recipes/comparing-translations.md` | verse-by-verse alignment, missing verses (BSB lacks Acts 8:37), `.vrs` maps (`versifications().withVrs`, `map`) |
| Full-text search | `recipes/searching-text.md` | an inverted index from `verseTexts` + `lx.words`; phrases; prefix search; highlight offsets; ~1 ms per search, index 1.5 MB gzipped for a whole Bible |
| Bookmarks/highlights | `recipes/stable-links.md` | store translation, version, verse, 8-hex hash, words; refind by hash after renumbering; `findFragment` (UTF-8 byte offsets) |
| Commentaries, Strong's, xrefs | `recipes/commentaries-and-dictionaries.md` | SWORD/CCEL imports → `importDownloaded` → `openPackage`; `notes` with stretch; entries by key; Easton/Smith/Nave fail to import in 0.1.1 |
| Audio Bible | `recipes/audio-with-timings.md` | WebVTT cues named by verse, `parseTimings`, binary search for the current verse, `verifyMedia`, `mediaStorePath` |
| HTTP API | `recipes/passage-server.md` | stdlib/`node:http`/`tiny_http` servers, LRU of books, ETag with `textVersion()`, 400/404/502 by error kind |
| The user's `lx` vault | `recipes/reading-a-vault.md` | library location lookup, Python `read_passage`/`compare_passage`, Rust logoxide-app, WebAssembly via `parseProject` + `openPackage` |
| Format conversion | `recipes/converting-formats.md` | `usfmToUsx`, `usxToUsj`, `convertBook`, `detectFormat`, `lx convert` |

Things these recipes found that matter beyond them:

- `parseWithTranslation` ignores a translation name it wasn't given: `John 3:16 ESV` parses as
  John 3:16 with no translation. Check the text after the reference when users name
  translations, and say the app doesn't have it.
- `topos.parse` returns `null` for verses past English counts (`Malachi 3:19`); look such verses
  up by number in the other Bible's `verseTexts()`. Don't pass a `null` passage to `book.text`
  (TypeScript throws a `TypeError`).
- fetch.bible renumbers every translation to English versification; eBible.org keeps each
  translation's own (its Hebrew WLC has Malachi 3:19-24).
- `topos.parse("john")` is the whole book: offer it beside search results, not instead.
- In Rust, HTML rendering needs `logoxide-markup` with its `html` feature.
- On Windows, set `LOGOXIDE_HOME` and `LOGOXIDE_CACHE` before reading a vault (0.1.1 has no
  default library there).
