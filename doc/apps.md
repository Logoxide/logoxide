# Bibles in your app

How an app gets a Bible's text with logoxide: found in a publisher's list, downloaded, read
into verses, and kept. Each step is shown in TypeScript (the npm package, WebAssembly in browsers,
Node, and Bun), Python (the PyPI package, native), and Rust (the crates). What's here is what
apps built on logoxide learned:
[scripture-slinger](https://github.com/MasterTemple/scripture-slinger) (a typing game, in the
browser, downloading a book as it's played), [scripture-hash](https://github.com/MasterTemple/scripture-hash)
(verses as hashes, in the browser, downloading or reading uploads in a worker), and a desktop app
importing whatever Bible files its user has, in Rust.

The [bindings' README](../crates/logoxide-ffi/README.md) is the reference for every method; this
is a walk through the ones an app needs. A vault of versioned, locked packages (what `lx`
makes) is another way, at the end.

- [Which way](#which-way)
- [Setting up](#setting-up)
- [Finding a translation](#finding-a-translation)
- [A book at a time](#a-book-at-a-time)
- [A whole Bible](#a-whole-bible)
- [Files your user brings](#files-your-user-brings)
- [From a book to verses](#from-a-book-to-verses)
- [Keeping what you read](#keeping-what-you-read)
- [References that name your translations](#references-that-name-your-translations)
- [Packages and vaults](#packages-and-vaults)

Once the text is in, the [recipes](recipes/README.md) build on this guide: uploads renamed and
added to, showing passages,
references in text, comparing translations, searching, links that survive corrections,
commentaries and dictionaries, audio with timings, a passage server, reading a vault, and
converting formats.

## Which way

| | What it is | Where it works |
|---|---|---|
| [A book at a time](#a-book-at-a-time) | fetch.bible's 1,400 translations, a USX file a book, downloaded as each is shown | everywhere: fetch.bible allows any site (CORS) |
| [A whole Bible](#a-whole-bible) | eBible.org's 1,300 translations, a `.zip` of USFM each (a few MB) | native apps, servers, and hosts with their own HTTP (Obsidian's `requestUrl`, Tauri); **not a web page**: eBible.org sends no CORS header, so a browser refuses to hand a page its files |
| [Files your user brings](#files-your-user-brings) | USX, USFM, USJ, OSIS, ThML, or GBF files, or `.zip`s of them (eBible.org's, a Paratext export, a Digital Bible Library bundle) | everywhere |
| [Packages and vaults](#packages-and-vaults) | versioned packages with a lock file, as `lx` keeps them | everywhere, with the host doing the HTTP (so a web page has the same limits) |

The licenses of most translations allow reading but not redistributing. Show each one's
`license` and `copyright` (every catalog entry has them) where the text is shown; logoxide shows
licenses and never enforces them, so what your app does with a text is up to you.

## Setting up

```sh
npm install logoxide          # TypeScript and JavaScript
pip install logoxide          # Python 3.10 or later
cargo add logoxide logoxide-import logoxide-text   # Rust: catalogs, downloads, and reading
```

In Node, Bun, and Python, the module is ready on import. In a browser, `await initialized` first.
The WebAssembly is 5.5 MB, so a page should load it only when a passage is wanted
(`await import("logoxide")`), and, since reading a whole Bible takes a moment, read it in a
worker so the page stays responsive. The package loads its `.wasm` from beside itself
(`new URL(…, import.meta.url)`), which Vite's pre-bundling breaks: exclude it.

```ts
// vite.config.ts
export default defineConfig({
  optimizeDeps: { exclude: ["logoxide"] },
  worker: { format: "es" },
});
```

```ts
// worker.ts: logoxide off the main thread, loaded on its first message
import { initialized, Logoxide, Topos } from "logoxide";

const ready = initialized.then(() => ({ lx: Logoxide.new(), topos: Topos.new() }));

self.onmessage = async (event) => {
  const { lx, topos } = await ready;
  // ... read what the page asked for, and post back plain objects (verses, not books)
};
```

The examples below run in order, as one program in each language, starting from these helpers:
an HTTP GET trying each of a download's URLs in turn. Send a `User-Agent` naming your app:
eBible.org answers Python's default with 403 Forbidden.

```ts
import { Logoxide, LogoxideBook, Topos, type PackageFile } from "logoxide";

const lx = Logoxide.new();
const topos = Topos.new();

/** The first of a download's URLs that answers */
async function get(urls: string[]): Promise<Uint8Array> {
  let failure: unknown;
  for (const url of urls) {
    try {
      const response = await fetch(url);
      if (response.ok) return new Uint8Array(await response.arrayBuffer());
      failure = new Error(`${url}: ${response.status} ${response.statusText}`);
    } catch (error) {
      failure = error;
    }
  }
  throw failure;
}
```

```python
import urllib.request

from logoxide import Logoxide, LogoxideBook, PackageFile, Topos

lx = Logoxide()
topos = Topos()


def get(urls: list[str]) -> bytes:
    """The first of a download's URLs that answers"""
    failure = OSError("no URLs")
    for url in urls:
        request = urllib.request.Request(url, headers={"User-Agent": "my-bible-app/1.0"})
        try:
            with urllib.request.urlopen(request) as response:
                return response.read()
        except OSError as error:
            failure = error
    raise failure
```

```rust
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The first of a download's URLs that answers
fn get(urls: &[String]) -> Result<Vec<u8>> {
    let mut failure: Box<dyn std::error::Error> = "no URLs".into();
    for url in urls {
        match ureq::get(url).header("User-Agent", "my-bible-app/1.0").call() {
            // eBible.org's zips are larger than ureq reads by default (10 MB)
            Ok(mut response) => return Ok(response.body_mut().with_config().limit(1 << 28).read_to_vec()?),
            Err(error) => failure = error.into(),
        }
    }
    Err(failure)
}
```

(`cargo add ureq` for this one; any HTTP client does.)

## Finding a translation

Each publisher's own list is read as a catalog: fetch.bible's `manifest.json` (750 KB) and
eBible.org's `translations.csv` (730 KB). Download one when your user browses for a translation,
not on every start, and keep it a day or so. `search` finds entries by name, abbreviation, or
language, best first (the second argument is how many, 0 for all; keep `kind === PackageKind.Bible`
for translations, since lists have commentaries and books too); `findImport` finds one by its
import name, `fetch:<id>` or `ebible:<id>`, which is what to save to remember a choice. Catalogs
`merge`, so one search covers both.

```ts
import { PackageKind } from "logoxide";

const FETCH_BIBLE = "https://v1.fetch.bible/manifest.json";
const fetchBible = lx.parseCatalogBytes(FETCH_BIBLE, await get([FETCH_BIBLE]));
const ebible = lx.parseCatalogBytes(lx.ebibleCatalogUrl(), await get([lx.ebibleCatalogUrl()]));
console.log(fetchBible.count(), ebible.count()); // → 1400 1294
for (const entry of fetchBible.merge(ebible).search("world english", 0).filter((e) => e.kind === PackageKind.Bible).slice(0, 2)) {
  console.log(entry.abbreviation, entry.name, entry.language, entry.source.tag === "External" && entry.source._import);
}
// → WEB World English Bible en fetch:eng_webu
// → WEBP World English Bible en ebible:engwebp

const bsb = fetchBible.findImport("fetch:eng_bsb")!;
console.log(bsb.name, "·", bsb.license, "·", bsb.copyright); // → Berean Standard Bible · LicenseRef-PublicDomain · BSB Publishing, LLC
```

```python
from logoxide import PackageKind

FETCH_BIBLE = "https://v1.fetch.bible/manifest.json"
fetch_bible = lx.parse_catalog_bytes(FETCH_BIBLE, get([FETCH_BIBLE]))
ebible = lx.parse_catalog_bytes(lx.ebible_catalog_url(), get([lx.ebible_catalog_url()]))
print(fetch_bible.count(), ebible.count())  # → 1400 1294
bibles = [e for e in fetch_bible.merge(ebible).search("world english", 0) if e.kind == PackageKind.BIBLE]  # 0: no limit
for entry in bibles[:2]:
    print(entry.abbreviation, entry.name, entry.language, entry.source.import_)
# → WEB World English Bible en fetch:eng_webu
# → WEBP World English Bible en ebible:engwebp

bsb = fetch_bible.find_import("fetch:eng_bsb")
print(bsb.name, "·", bsb.license, "·", bsb.copyright)  # → Berean Standard Bible · LicenseRef-PublicDomain · BSB Publishing, LLC
```

```rust
use logoxide::{
    catalog::{Catalog, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG},
    manifest::PackageKind,
};

fn catalogs() -> Result<(Catalog, Catalog)> {
    let fetch_bible = logoxide_import::catalog::parse(FETCH_BIBLE_CATALOG, &get(&[FETCH_BIBLE_CATALOG.into()])?)?;
    let ebible = logoxide_import::catalog::parse(EBIBLE_CATALOG, &get(&[EBIBLE_CATALOG.into()])?)?;
    for entry in fetch_bible.search("world english").into_iter().filter(|e| e.kind == PackageKind::Bible).take(1) {
        // WEB World English Bible en fetch:eng_webu
        println!("{} {} {} {}", entry.abbreviation.as_deref().unwrap_or(""), entry.name, entry.language, entry.source.import_name().unwrap_or(""));
    }
    let bsb = fetch_bible.find_import("fetch:eng_bsb").ok_or("no BSB")?;
    let license = [bsb.license.as_deref(), bsb.copyright.as_deref()].into_iter().flatten().collect::<Vec<_>>();
    println!("{} · {}", bsb.name, license.join(" · ")); // Berean Standard Bible · LicenseRef-PublicDomain · BSB Publishing, LLC
    Ok((fetch_bible, ebible))
}
```

The numbers change as the publishers add translations. A catalog that can't be read whole still
reads what it can: `warnings()` says what was passed over.

## A book at a time

fetch.bible serves each book as its own USX file, so an app can download only the books it shows:
John for John 3:16, then Romans when it's asked for. `bookDownloads(entry)` lists the books a
translation has, each with its URLs; `LogoxideBook.parse(topos, "usx", text)` reads one. A book a
translation doesn't have isn't listed, which matters: v1.fetch.bible answers a file it doesn't
have with `200 OK` and an HTML listing of the folder, not a 404, so don't build its URLs by hand
without checking the response's `Content-Type`.

```ts
const books = lx.bookDownloads(bsb); // a translation of the New Testament alone lists 27
const john = books.find((b) => b.bookId === 43)!;
console.log(books.length, john.urls); // → 66 [ 'https://v1.fetch.bible/bibles/eng_bsb/usx/jhn.usx' ]
const johnText = new TextDecoder().decode(await get(john.urls));
const book = LogoxideBook.parse(topos, "usx", johnText); // null if it isn't a book
const verses = book!.verseTexts();
console.log(verses.length, verses.find((v) => v.chapter === 3 && v.verse === 16)!.text);
// → 878 For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.
```

```python
books = lx.book_downloads(bsb)  # a translation of the New Testament alone lists 27
john = next(b for b in books if b.book_id == 43)
print(len(books), john.urls)  # → 66 ['https://v1.fetch.bible/bibles/eng_bsb/usx/jhn.usx']
book = LogoxideBook.parse(topos, "usx", get(john.urls).decode())  # raises if it isn't a book
verses = book.verse_texts()
print(len(verses), next(v for v in verses if (v.chapter, v.verse) == (3, 16)).text)
# → 878 For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.
```

In Rust, `logoxide_import::downloads` gives the same list (each book named by its lowercase USFM
code), and logoxide-text reads a book from files in memory:

```rust
use logoxide_text::{MemoryFiles, Source, passage::verse_texts, source::open_loose, topos_bible::data::books::BookId};

fn john(fetch_bible: &Catalog) -> Result<()> {
    let bsb = fetch_bible.find_import("fetch:eng_bsb").ok_or("no BSB")?;
    let downloads = logoxide_import::downloads(bsb)?;
    let john = downloads.iter().find(|d| d.name == "jhn").ok_or("no John")?;
    let files = MemoryFiles::with([("jhn.usx", get(&john.urls)?)]);
    let book = open_loose(&files, "")?.book(BookId(43))?;
    let verses = verse_texts(&book);
    let v = verses.iter().find(|v| (v.chapter, v.verse) == (3, 16)).ok_or("no John 3:16")?;
    println!("{} {}", verses.len(), v.text); // 878 For God so loved the world …
    Ok(())
}
```

## A whole Bible

eBible.org's translations come whole, as a `.zip` of a USFM file a book. `importDownloads(entry)`
says what to download, and `openBible` reads the downloaded bytes as they are: it opens the zip,
finds each file's format, and reads each book when it's asked for. `skipped()` says what it passed
over (the copyright page, a glossary, front matter), which needn't be shown to anyone. Opening is
quick (the WEB's 66 books in a fifth of a second); parsing a book is the slow part, a third of a
second for John in WebAssembly, so read the books you need, or all of them once in a worker and
keep their verses.

```ts
const web = ebible.findImport("ebible:engwebp")!;
const downloads = lx.importDownloads(web);
console.log(downloads); // → [ { name: 'zip', urls: [ 'https://ebible.org/Scriptures/engwebp_usfm.zip' ] } ]
const files: PackageFile[] = [];
for (const d of downloads) files.push({ path: d.urls[0].split("/").pop()!, bytes: await get(d.urls) });
const bible = lx.openBible(files);
console.log(bible.books().length, bible.bookFormat(43), bible.skipped()[0].path); // → 66 usfm engwebp_usfm.zip/copr.htm
const webJohn = bible.book(43);
console.log(webJohn.verseTexts()[0].text); // → In the beginning was the Word, and the Word was with God, and the Word was God.
```

```python
web = ebible.find_import("ebible:engwebp")
downloads = lx.import_downloads(web)
print(downloads)  # → [ImportDownload(name='zip', urls=['https://ebible.org/Scriptures/engwebp_usfm.zip'])]
files = [PackageFile(path=d.urls[0].rsplit("/", 1)[-1], bytes=get(d.urls)) for d in downloads]
bible = lx.open_bible(files)
print(len(bible.books()), bible.book_format(43), bible.skipped()[0].path)  # → 66 usfm engwebp_usfm.zip/copr.htm
web_john = bible.book(43)
print(web_john.verse_texts()[0].text)  # → In the beginning was the Word, and the Word was with God, and the Word was God.
```

```rust
fn whole(ebible: &Catalog) -> Result<()> {
    let web = ebible.find_import("ebible:engwebp").ok_or("no WEB")?;
    let mut files = vec![];
    for d in logoxide_import::downloads(web)? {
        let name = d.urls[0].rsplit('/').next().unwrap_or("download.zip").to_string();
        files.push((name, get(&d.urls)?));
    }
    let bible = open_loose(&MemoryFiles::with(files), "")?;
    for skipped in bible.skipped() {
        eprintln!("{skipped}"); // engwebp_usfm.zip/copr.htm: not a Bible's text (…), passed over
    }
    let mut verses = std::collections::BTreeMap::new();
    for id in bible.books() {
        verses.insert(id.0, verse_texts(&bible.book(id)?));
    }
    println!("{} books, {} verses in John", verses.len(), verses[&43].len()); // 66 books, 879 verses in John
    Ok(())
}
```

In a web page, download eBible.org's zips through your own server (or let your user download one
and drop it in: the next section), or use fetch.bible's copy of the same translation where it has
one.

## Files your user brings

`openBible` reads what a user drops in, as they come: a book a file in USX (`.usx`, or `.xml`),
USFM (`.usfm`, `.sfm`, `.SFM`), USJ, OSIS, ThML, or GBF; a whole Bible in one OSIS or ThML file;
or `.zip`s of any of them, with whatever folders inside. It finds each file's format from its name
and, failing that, from how its text starts. A book given twice is read from its best format
(USX, then USFM, USJ, OSIS, ThML, GBF), and the other file is `skipped` as a `duplicate`; a book
given twice in one format is an `Invalid` error naming both files, for your user to keep one. Files
that aren't a book at all (a license, a stylesheet, signatures) are `skipped` as `not-text`; the
ones worth telling your user about are the files they gave that gave no book, not a zip's
extras. When no file is a book of the Bible, it's a `NotFound` error.

Compare each book's chapters with how many it should have to say what's missing before saving a
translation (scripture-slinger lists the books and chapters a user's files lack).

```ts
import { LogoxideErrorException } from "logoxide";

function readUploads(uploads: PackageFile[]) {
  let uploaded: ReturnType<typeof lx.openBible>;
  try {
    uploaded = lx.openBible(uploads);
  } catch (error) {
    if (error instanceof LogoxideErrorException && error.value.tag === "NotFound") return null; // not one book
    throw error;
  }
  const given = new Set(uploads.map((f) => f.path));
  const notBooks = uploaded.skipped().filter((s) => given.has(s.path)).map((s) => s.path);
  const missing = [...uploaded.books()].flatMap((id) => {
    const expected = topos.books()[id - 1].chapters;
    const has = uploaded.chapterVerses(id).length;
    return has < expected ? [`${topos.books()[id - 1].name} has ${has} of ${expected} chapters`] : [];
  });
  return { bible: uploaded, notBooks, missing };
}

const usx = lx.usfmToUsx("\\id 3JN\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n");
const read = readUploads([files[0], { path: "3JN.xml", bytes: new TextEncoder().encode(usx) }, { path: "notes.txt", bytes: new Uint8Array(1) }])!;
console.log(read.bible.bookFormat(64), read.notBooks, read.missing); // → usx [ 'notes.txt' ] []
console.log(readUploads([{ path: "notes.txt", bytes: new Uint8Array(1) }])); // → null
```

```python
from logoxide import LogoxideErrorException, LogoxideErrorNotFound


def read_uploads(uploads: list[PackageFile]):
    try:
        uploaded = lx.open_bible(uploads)
    except LogoxideErrorException as error:
        if isinstance(error.error, LogoxideErrorNotFound):
            return None  # not one book
        raise
    given = {f.path for f in uploads}
    not_books = [s.path for s in uploaded.skipped() if s.path in given]
    missing = []
    for id in uploaded.books():
        info = topos.books()[id - 1]
        has = len(uploaded.chapter_verses(id))
        if has < info.chapters:
            missing.append(f"{info.name} has {has} of {info.chapters} chapters")
    return uploaded, not_books, missing


usx = lx.usfm_to_usx("\\id 3JN\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n")
uploaded, not_books, missing = read_uploads([files[0], PackageFile(path="3JN.xml", bytes=usx.encode()), PackageFile(path="notes.txt", bytes=b"\0")])
print(uploaded.book_format(64), not_books, missing)  # → usx ['notes.txt'] []
print(read_uploads([PackageFile(path="notes.txt", bytes=b"\0")]))  # → None
```

Here 3 John is read from the loose USX file, and the zip's USFM 3 John is skipped as a
`duplicate` (inside the zip, so not among the files to mention). In Rust it's the same `open_loose`, given the files
by name; `bible_data()` has how many chapters each book has:

```rust
fn uploads(paths: &[std::path::PathBuf]) -> Result<()> {
    let files = paths.iter().map(|p| Ok((p.to_string_lossy().into_owned(), std::fs::read(p)?)));
    let files = MemoryFiles::with(files.collect::<Result<Vec<_>>>()?);
    let bible = open_loose(&files, "")?; // an error when no file is a book
    let data = logoxide_text::bible_data();
    for id in bible.books() {
        let has = bible.chapter_verses(id)?.len();
        let expected = data.chapter_verses().get_chapter_verses(&id).map_or(0, |c| c.get_chapter_count());
        if has < expected as usize {
            let name = data.books().get_name(id).cloned().unwrap_or_default();
            println!("{name} has {has} of {expected} chapters");
        }
    }
    Ok(())
}
```

USFM can also be converted to USX (`lx.usfmToUsx`), and `lx.unzip(bytes)` lists a zip's files,
for an app that keeps books as USX itself.

To let your user rename an upload, add books to it, or replace one, keep it under an id of its
own and keep its files: the recipe [Translations your user uploads](recipes/uploaded-translations.md).

## From a book to verses

`verseTexts()` is every verse of a book as reading text, in order: its words without notes,
composed (NFC), with a few invisible characters dropped and whitespace collapsed (verse-hash rule
1, so its `hash` is the one a package's summary has for it); the `heading` just before it; and
whether it starts a `paragraph` (or a stanza) or a `line` of poetry, to lay it out again. It
keeps what a file prints in a verse, which for the KJV includes its paragraph marks (`¶`).

Two things every app runs into:

- **Bridged verses.** A file can print verses together (`\v 4-5`): one entry whose `verse` is 4
  and `last` is 5 (in `lastChapter`, for a bridge across chapters, rare). Its text is both
  verses'; look up verse 5 and you should find the same entry. scripture-hash stores the text
  under each verse a bridge covers, with the span; a desktop app that needs a text per verse gives
  the first all of it and the rest none.
- **Verses in parts.** A file can print a verse in parts (`20a`, `20b`), each an entry with
  `part` 1, 2, …; join them for the whole verse.

```ts
const romans = LogoxideBook.parse(topos, "usx", lx.usfmToUsx(
  "\\id ROM\n\\c 1\n\\p\n\\v 1 Paul.\n\\v 2-3 Promised before.\n\\v 4a Declared\n\\v 4b with power.\n",
))!;

/** Each verse's text, by "chapter:verse": a bridge's under each verse it covers, parts joined */
function byVerse(book: LogoxideBook): Map<string, string> {
  const out = new Map<string, string>();
  for (const v of book.verseTexts()) {
    const key = `${v.chapter}:${v.verse}`;
    if (v.part > 1 && out.has(key)) {
      out.set(key, `${out.get(key)} ${v.text}`);
      continue;
    }
    // A bridge across chapters needs topos for the verses between; within one, they're in order
    const last = v.lastChapter === v.chapter ? v.last : v.verse;
    for (let n = v.verse; n <= last; n++) out.set(`${v.chapter}:${n}`, v.text);
  }
  return out;
}
console.log(byVerse(romans)); // → Map(4) { '1:1' => 'Paul.', '1:2' => 'Promised before.', '1:3' => 'Promised before.', '1:4' => 'Declared with power.' }
```

```python
romans = LogoxideBook.parse(topos, "usx", lx.usfm_to_usx(
    "\\id ROM\n\\c 1\n\\p\n\\v 1 Paul.\n\\v 2-3 Promised before.\n\\v 4a Declared\n\\v 4b with power.\n"
))


def by_verse(book: LogoxideBook) -> dict[str, str]:
    """Each verse's text, by "chapter:verse": a bridge's under each verse it covers, parts joined"""
    out: dict[str, str] = {}
    for v in book.verse_texts():
        key = f"{v.chapter}:{v.verse}"
        if v.part > 1 and key in out:
            out[key] += " " + v.text
            continue
        # A bridge across chapters needs topos for the verses between; within one, they're in order
        last = v.last if v.last_chapter == v.chapter else v.verse
        for n in range(v.verse, last + 1):
            out[f"{v.chapter}:{n}"] = v.text
    return out


print(by_verse(romans))  # → {'1:1': 'Paul.', '1:2': 'Promised before.', '1:3': 'Promised before.', '1:4': 'Declared with power.'}
```

In Rust the entries are `logoxide_text::passage::VerseText`, with the same fields, and
`logoxide_text::hash::verse_hash(&v.text)` is the hash.

For comparing what someone typed or said with a verse, `lx.words(text)` splits text into words as
matched loosely (lowercase, accents and punctuation dropped): `words("“Éphraïm’s” word—so")` is
`["ephraims", "word", "so"]`. For showing a passage rather than storing verses, a book also writes
passages as text or HTML (`text`, `html`, `styledText`), with headings, notes, and verse labels as
you choose: the bindings' README, under "A book's text".

In TypeScript, a book, a Bible, and a catalog hold memory in WebAssembly until they're
garbage-collected; `dispose()` frees it now, which matters when reading a whole Bible.

```ts
webJohn.dispose();
```

## Keeping what you read

Download each book or zip once. Two things can be kept:

- **The downloaded files**, as they came (scripture-slinger keeps each book's USX in the
  browser's Cache Storage, keyed by its URL). They never go stale, and parsing a book again is
  quick enough when it's opened.
- **The verses read from them** (scripture-hash keeps each book's verses in IndexedDB). Then the
  app needn't load logoxide at all for a book it has read. Record `lx.textVersion()` with them:
  it changes only when a release of logoxide reads files differently, and a book read by an
  older version should be read again from its file (downloaded again if it wasn't kept;
  while offline, the old verses are better than none). Something a user uploaded can't be read
  again unless you kept its file, so keep uploads' files too.

```ts
console.log(lx.textVersion()); // → 4 0.1.2
```

```python
print(lx.text_version())  # → 4 0.1.2
```

```rust
fn text_version() {
    println!("{}", logoxide_text::cache::text_version()); // 4 0.1.2
}
```

Key what you keep by the import name (`fetch:eng_bsb`) and book, not by the abbreviation: two
publishers' copies of one translation can share an abbreviation and differ in their text.
fetch.bible's copies don't change version as they're corrected (eBible.org's do: each entry's
`version` is the date of its last update), so an app that wants corrections can download a
translation again now and then and compare verses' hashes to see what changed.

## References that name your translations

topos, in the same package, finds references in text and parses them (`John 3:16`, `Jn 3.16`,
`1 Cor 13`). To have `John 3:16 BSB` name a translation, give topos your translations' names
(and, once a book is read, its own verse counts, so `3 John 1:15` is caught in a Bible that
numbers 3 John to 14):

```ts
import { BookStyle, OffsetUnit, ToposQuery, ToposTranslations } from "logoxide";

let names = ToposTranslations.create().add("BSB", ["Berean"]).add("WEB", []);
names = names.chapterVerses("WEB", 64, bible.chapterVerses(64));
const found = topos.parseWithTranslation("John 3:16 Berean", OffsetUnit.Utf16, names, BookStyle.Name)!;
console.log(found.passage.reference, found.translation?.name, found.passage.bookId); // → John 3:16 BSB 43
const inText = topos.searchWithTranslations("Read John 3:16 WEB, then Rom 8:28.", OffsetUnit.Utf16, ToposQuery.create(), names);
console.log(inText.map((m) => [m.reference.passage.reference, m.translation?.name ?? null])); // → [ [ 'John 3:16', 'WEB' ], [ 'Romans 8:28', null ] ]
```

```python
from logoxide import BookStyle, OffsetUnit, ToposQuery, ToposTranslations

names = ToposTranslations.create().add("BSB", ["Berean"]).add("WEB", [])
names = names.chapter_verses("WEB", 64, bible.chapter_verses(64))
found = topos.parse_with_translation("John 3:16 Berean", OffsetUnit.CHAR, names, BookStyle.NAME)
print(found.passage.reference, found.translation.name, found.passage.book_id)  # → John 3:16 BSB 43
in_text = topos.search_with_translations("Read John 3:16 WEB, then Rom 8:28.", OffsetUnit.CHAR, ToposQuery.create(), names)
print([(m.reference.passage.reference, m.translation.name if m.translation else None) for m in in_text])  # → [('John 3:16', 'WEB'), ('Romans 8:28', None)]
```

Offsets come in the unit your strings index by: UTF-16 code units in JavaScript, characters in
Python, bytes in Rust. topos's own guide has the rest (completion, queries, formats).

## Packages and vaults

Everything above reads a publisher's files directly, which is all most apps need. logoxide's
packages add what a library of Bibles needs over time: a `logoxide.toml` saying what each is
(name, abbreviation, language, license, versification), a summary with every verse's hash,
versions that rise by what changed, and a lock file pinning the exact text.

- **One import as a package**: `importDownloaded(entry, files)` makes a package's files from the
  same downloads (write them into a folder, or keep them in memory), and `openPackage` opens it,
  with its `info()`, `verseHashes`, and a cache of what it has read. `updateImport` says what a
  newer download changed. The bindings' README: "Imports".
- **A vault**: a folder with a `logoxide.toml` listing packages and a `logoxide.lock`, as `lx`
  makes and reads them (`lx import ebible:engwebp`, then `lx read "John 3:16 WEB"`). An app can
  share one with `lx`, the language server, and the Obsidian plugin: the bindings' `installer()`
  resolves and installs as `lx install` does, asking the host for each download, and in Rust,
  logoxide-app's `Logoxide::import` and `Logoxide::read` are what `lx import` and `lx read` call.
  The bindings' README: "A vault's project" and "Installing"; [workflow.md](workflow.md) for `lx`.
