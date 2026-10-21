---
name: logoxide-app
description: Add Bible text to an app with logoxide (npm `logoxide`, PyPI `logoxide`, or the Rust crates logoxide, logoxide-import, logoxide-text) — finding translations in eBible.org's and fetch.bible's lists, downloading a book at a time or a whole Bible, reading files a user uploads (USX, USFM, USJ, OSIS, ThML, GBF, zips), turning books into verses, caching them, showing passages as HTML or text, linking references found in text, comparing translations, searching, and serving passages over HTTP. Use when someone wants Scripture, Bible verses, or translations in a web app, browser extension, Node or Bun tool, Python script or server, Rust program, Obsidian plugin, Tauri or Electron app, or bot, or mentions logoxide, topos, USFM, USX, OSIS, fetch.bible, or eBible.org in code.
---

# Bibles in an app with logoxide

logoxide reads Bibles in every common format and finds them in publishers' lists. One API in
three forms:

| | Install | Notes |
|---|---|---|
| TypeScript / JavaScript | `npm install logoxide` | WebAssembly (5.5 MB) in browsers, Node, and Bun; topos (references) included |
| Python 3.10+ | `pip install logoxide` | native; also reads folders itself (`openPackageDir`, `readPassage`) |
| Rust 1.89+ | `cargo add logoxide logoxide-import logoxide-text` | catalogs in logoxide-import, reading in logoxide-text; logoxide-app for a vault |

Names are camelCase in TypeScript, snake_case in Python (`verseTexts` / `verse_texts`).

## Workflow

1. **Find out where the app runs and what text it needs.** Ask only what the code doesn't show:
   a web page or a native/server host? which translations (a fixed few, or the user's choice)?
   may users upload their own files? Read the project's package manifest and build config.
2. **Choose the source** (details in [references/sources.md](references/sources.md)):
   - A web page: **fetch.bible**, a book at a time (CORS allowed). Not eBible.org: it sends no
     CORS header, so a browser can't download its zips.
   - Native, server, or a host with its own HTTP (Obsidian's `requestUrl`, Tauri, Electron's
     main process): eBible.org's whole-Bible zips too.
   - The user's own files: `openBible` (zips, any format, any folder layout).
   - The user already keeps Bibles with `lx` (a vault): read that library
     ([references/recipes.md](references/recipes.md), "Reading a vault").
3. **Install and load** (browser specifics below), then write the code from the matching
   pattern in [references/api.md](references/api.md). Keep logoxide in one module of the app
   (a "bible" service) so the rest of the app sees plain verses.
4. **Turn books into the app's own shape** with `verseTexts()`: handle bridged verses (`last`,
   `lastChapter`) and verses in parts (`part`) — see api.md.
5. **Cache**: keep downloaded files or the verses read from them, keyed by import name
   (`fetch:eng_bsb`) and book, recording `textVersion()` with parsed verses.
6. **Show each translation's `license` and `copyright`** from its catalog entry wherever its
   text appears. logoxide never enforces licenses; most modern translations forbid
   redistribution, so don't bundle text into the app unless its license allows it.
7. **Run it.** Check a known verse (John 3:16) comes out, a missing book is handled, and the
   build works (for browsers, a production build too).

## Browser setup

- Load it lazily: `const lx = await import("logoxide"); await lx.initialized;`. In Node, Bun,
  and Python it's ready on import.
- Read whole Bibles in a Web Worker (`new Worker(new URL("./bible.worker.ts", import.meta.url), { type: "module" })`)
  and post plain objects back (verses, not logoxide objects).
- Vite: `optimizeDeps: { exclude: ["logoxide"] }` and `worker: { format: "es" }` — the package
  loads its `.wasm` beside itself, which pre-bundling breaks. Other bundlers: make sure the
  `.wasm` file is emitted as an asset and resolvable from `import.meta.url`.
- `dispose()` logoxide objects (books, Bibles, catalogs) when done with them in long sessions;
  they hold WebAssembly memory until garbage-collected otherwise.

## Gotchas (each cost a real app time)

- v1.fetch.bible answers a book it doesn't have with **200 OK and an HTML page**, not 404. Use
  `bookDownloads(entry)` (lists only the books the translation has), or check `Content-Type`.
- eBible.org returns **403 to Python's default User-Agent**: send one naming the app.
- `openBible` with the **same book twice in one format** is an `Invalid` error ("keep one");
  in two formats, the better one is read and the other is `skipped` as `duplicate`.
- `openBible` with no book at all is a `NotFound` error; `LogoxideBook.parse` of text without
  verses is `null` in TypeScript and raises `LogoxideErrorInvalid` in Python.
- Only tell users about `skipped()` files they gave directly, not a zip's extras (copyright
  pages, glossaries, front matter, signatures).
- The KJV's text keeps its paragraph marks (`¶`); strip them for typing or matching, or compare
  with `lx.words(text)` (folded words).
- Offsets from topos come in the unit asked for: `OffsetUnit.Utf16` for JavaScript strings,
  `OffsetUnit.CHAR` in Python, bytes in Rust.
- Errors: TypeScript throws `LogoxideErrorException` (`error.value.tag`: `NotFound`, `Invalid`,
  `InvalidMarkup`, `Network`, `Io`, …); Python raises `LogoxideErrorException` (`error.error` is
  `LogoxideErrorNotFound`, …). Branch on the kind, show the message.

## More

- [references/api.md](references/api.md): the patterns, in TypeScript, Python, and Rust
- [references/sources.md](references/sources.md): fetch.bible, eBible.org, uploads, and the
  other publishers (CCEL, CrossWire's SWORD, Door43, Scripture Burritos)
- [references/recipes.md](references/recipes.md): where the full recipes are (showing passages,
  references in text, comparing translations, search, stable links, commentaries and
  dictionaries, audio with timings, a passage server, reading a vault)
- The full guide: https://github.com/Logoxide/logoxide/blob/main/doc/apps.md; every method:
  https://github.com/Logoxide/logoxide/blob/main/crates/logoxide-ffi/README.md
