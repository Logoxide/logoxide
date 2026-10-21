# logoxide patterns for apps

Every snippet here was run against logoxide 0.1.1 (npm, PyPI, crates.io). Outputs follow `→`.

## Contents

- [Setup and HTTP](#setup-and-http)
- [Catalogs: finding a translation](#catalogs-finding-a-translation)
- [A book at a time (fetch.bible)](#a-book-at-a-time-fetchbible)
- [A whole Bible (eBible.org)](#a-whole-bible-ebibleorg)
- [Uploaded files](#uploaded-files)
- [Verses: bridges and parts](#verses-bridges-and-parts)
- [Caching](#caching)
- [References naming the app's translations](#references-naming-the-apps-translations)
- [Showing a passage](#showing-a-passage)

## Setup and HTTP

```ts
import { Logoxide, LogoxideBook, Topos, type PackageFile } from "logoxide";
// browser: const { Logoxide, ... , initialized } = await import("logoxide"); await initialized;
const lx = Logoxide.new();
const topos = Topos.new();

async function get(urls: string[]): Promise<Uint8Array> {
  let failure: unknown;
  for (const url of urls) {
    try {
      const response = await fetch(url);
      if (response.ok) return new Uint8Array(await response.arrayBuffer());
      failure = new Error(`${url}: ${response.status} ${response.statusText}`);
    } catch (error) { failure = error; }
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
    """The first of a download's URLs that answers (eBible.org 403s urllib's default User-Agent)"""
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
// Cargo.toml: logoxide = "0.1", logoxide-import = "0.1", logoxide-text = "0.1", ureq = "3"
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn get(urls: &[String]) -> Result<Vec<u8>> {
    let mut failure: Box<dyn std::error::Error> = "no URLs".into();
    for url in urls {
        match ureq::get(url).header("User-Agent", "my-bible-app/1.0").call() {
            Ok(mut response) => return Ok(response.body_mut().with_config().limit(1 << 28).read_to_vec()?),
            Err(error) => failure = error.into(),
        }
    }
    Err(failure)
}
```

## Catalogs: finding a translation

fetch.bible's `manifest.json` (750 KB, 1,400 translations) and eBible.org's `translations.csv`
(730 KB, about 1,300). Download when the user browses; keep a day. Save the import name
(`fetch:eng_bsb`, `ebible:engwebp`) to remember a choice.

```ts
import { PackageKind } from "logoxide";
const FETCH_BIBLE = "https://v1.fetch.bible/manifest.json";
const fetchBible = lx.parseCatalogBytes(FETCH_BIBLE, await get([FETCH_BIBLE]));
const ebible = lx.parseCatalogBytes(lx.ebibleCatalogUrl(), await get([lx.ebibleCatalogUrl()]));
for (const e of fetchBible.merge(ebible).search("world english", 0).filter((e) => e.kind === PackageKind.Bible).slice(0, 2))
  console.log(e.abbreviation, e.name, e.language, e.source.tag === "External" && e.source._import);
// → WEB World English Bible en fetch:eng_webu
// → WEBP World English Bible en ebible:engwebp
const bsb = fetchBible.findImport("fetch:eng_bsb")!;
console.log(bsb.name, bsb.license, bsb.copyright); // → Berean Standard Bible LicenseRef-PublicDomain BSB Publishing, LLC
```

```python
from logoxide import PackageKind
FETCH_BIBLE = "https://v1.fetch.bible/manifest.json"
fetch_bible = lx.parse_catalog_bytes(FETCH_BIBLE, get([FETCH_BIBLE]))
ebible = lx.parse_catalog_bytes(lx.ebible_catalog_url(), get([lx.ebible_catalog_url()]))
bibles = [e for e in fetch_bible.merge(ebible).search("world english", 0) if e.kind == PackageKind.BIBLE]  # 0: no limit
for e in bibles[:2]:
    print(e.abbreviation, e.name, e.language, e.source.import_)
bsb = fetch_bible.find_import("fetch:eng_bsb")
```

```rust
use logoxide::{catalog::{Catalog, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG}, manifest::PackageKind};

let fetch_bible: Catalog = logoxide_import::catalog::parse(FETCH_BIBLE_CATALOG, &get(&[FETCH_BIBLE_CATALOG.into()])?)?;
let bibles: Vec<_> = fetch_bible.search("world english").into_iter().filter(|e| e.kind == PackageKind::Bible).collect();
let bsb = fetch_bible.find_import("fetch:eng_bsb").ok_or("no BSB")?; // .name, .license, .copyright: Option<String>
```

## A book at a time (fetch.bible)

```ts
const books = lx.bookDownloads(bsb);                 // only the books it has: { bookId, name: "jhn", urls }
const john = books.find((b) => b.bookId === 43)!;
const book = LogoxideBook.parse(topos, "usx", new TextDecoder().decode(await get(john.urls))); // null if not a book
const verses = book!.verseTexts();
console.log(verses.length, verses.find((v) => v.chapter === 3 && v.verse === 16)!.text); // → 878 For God so loved the world …
```

```python
john = next(b for b in lx.book_downloads(bsb) if b.book_id == 43)
book = LogoxideBook.parse(topos, "usx", get(john.urls).decode())  # raises LogoxideErrorException if not a book
verses = book.verse_texts()
```

```rust
use logoxide_text::{MemoryFiles, Source, passage::verse_texts, source::open_loose, topos_bible::data::books::BookId};

let downloads = logoxide_import::downloads(bsb)?;                 // each book named by lowercase USFM code
let john = downloads.iter().find(|d| d.name == "jhn").ok_or("no John")?;
let files = MemoryFiles::with([("jhn.usx", get(&john.urls)?)]);
let book = open_loose(&files, "")?.book(BookId(43))?;
let verses = verse_texts(&book);                                   // Vec<logoxide_text::passage::VerseText>
```

Book ids are topos's: 1 Genesis … 66 Revelation (`topos.books()[id - 1]` has `name`, `usfm`,
`osis`, `chapters`); the deuterocanon follows (Tobit 67) in logoxide-text's data.

## A whole Bible (eBible.org)

Not from a web page (no CORS). Opening is quick; parsing a book takes ~0.3 s in WebAssembly, so
parse what's needed, or everything once in a worker and keep the verses.

```ts
const web = ebible.findImport("ebible:engwebp")!;
const files: PackageFile[] = [];
for (const d of lx.importDownloads(web)) files.push({ path: d.urls[0].split("/").pop()!, bytes: await get(d.urls) });
const bible = lx.openBible(files);
console.log(bible.books().length, bible.bookFormat(43), bible.skipped()[0].path); // → 66 usfm engwebp_usfm.zip/copr.htm
const verses = bible.book(43).verseTexts();
```

```python
web = ebible.find_import("ebible:engwebp")
files = [PackageFile(path=d.urls[0].rsplit("/", 1)[-1], bytes=get(d.urls)) for d in lx.import_downloads(web)]
bible = lx.open_bible(files)
verses = bible.book(43).verse_texts()
```

```rust
let web = ebible.find_import("ebible:engwebp").ok_or("no WEB")?;
let mut files = vec![];
for d in logoxide_import::downloads(web)? {
    files.push((d.urls[0].rsplit('/').next().unwrap_or("download.zip").to_string(), get(&d.urls)?));
}
let bible = open_loose(&MemoryFiles::with(files), "")?;
for id in bible.books() { let verses = verse_texts(&bible.book(id)?); /* keep */ }
```

## Uploaded files

`openBible` takes `{ path, bytes }` files: USX (`.usx`/`.xml`), USFM (`.usfm`/`.sfm`), USJ, OSIS,
ThML, GBF, a whole Bible in one OSIS/ThML file, or zips of any. Best format wins per book;
`skipped()` lists `not-text`, `duplicate`, `peripheral`, … Same book twice in one format:
`Invalid` error. No book at all: `NotFound` error.

```ts
import { LogoxideErrorException } from "logoxide";
function readUploads(uploads: PackageFile[]) {
  let bible;
  try { bible = lx.openBible(uploads); }
  catch (error) {
    if (error instanceof LogoxideErrorException && error.value.tag === "NotFound") return null;
    throw error; // Invalid: e.g. "64: in two files, a.usfm and b.usfm: keep one"
  }
  const given = new Set(uploads.map((f) => f.path));
  const notBooks = bible.skipped().filter((s) => given.has(s.path)).map((s) => s.path);
  const missing = [...bible.books()].flatMap((id) => {
    const info = topos.books()[id - 1], has = bible.chapterVerses(id).length;
    return has < info.chapters ? [`${info.name} has ${has} of ${info.chapters} chapters`] : [];
  });
  return { bible, notBooks, missing };
}
```

In the browser, read `File`s as `{ path: file.name, bytes: new Uint8Array(await file.arrayBuffer()) }`.
Keep the uploaded bytes if verses may need reading again later (a new `textVersion()`).

## Verses: bridges and parts

`verseTexts()` entries: `chapter`, `verse`, `last`, `lastChapter`, `part`, `text` (notes left
out; NFC; invisible characters dropped; whitespace collapsed), `hash` (verse-hash rule 1, 14 hex),
`heading` (or null), `paragraph`, `line`.

```ts
/** Each verse's text, by "chapter:verse": a bridge's under each verse it covers, parts joined */
function byVerse(book: LogoxideBook): Map<string, string> {
  const out = new Map<string, string>();
  for (const v of book.verseTexts()) {
    const key = `${v.chapter}:${v.verse}`;
    if (v.part > 1 && out.has(key)) { out.set(key, `${out.get(key)} ${v.text}`); continue; }
    const last = v.lastChapter === v.chapter ? v.last : v.verse; // across chapters: rare; topos can list the verses
    for (let n = v.verse; n <= last; n++) out.set(`${v.chapter}:${n}`, v.text);
  }
  return out;
}
// \v 2-3 Promised before. / \v 4a Declared / \v 4b with power.
// → Map { '1:1' => 'Paul.', '1:2' => 'Promised before.', '1:3' => 'Promised before.', '1:4' => 'Declared with power.' }
```

An app that needs one text per verse (audio, typing) can give a bridge's text to its first verse
and leave the others empty.

## Caching

- Raw files (USX books, zips) keyed by URL or import name + book: never stale. Browser: Cache
  Storage (`caches.open`, https or localhost only) or IndexedDB.
- Parsed verses keyed by import name + book, stored with `lx.textVersion()` (`"4 0.1.1"`); read
  again from the file when it differs; offline, old verses beat none.
- Key by import name, not abbreviation: two publishers' copies can share an abbreviation.
- eBible.org entries' `version` is the date of the last update; fetch.bible's copies don't
  version, so compare verse `hash`es after a fresh download to see what changed.

## References naming the app's translations

```ts
import { BookStyle, OffsetUnit, ToposQuery, ToposTranslations } from "logoxide";
let names = ToposTranslations.create().add("BSB", ["Berean"]).add("WEB", []);
names = names.chapterVerses("WEB", 64, bible.chapterVerses(64)); // its own numbering, per book
const found = topos.parseWithTranslation("John 3:16 Berean", OffsetUnit.Utf16, names, BookStyle.Name)!;
console.log(found.passage.reference, found.translation?.name, found.passage.bookId); // → John 3:16 BSB 43
const inText = topos.searchWithTranslations("Read John 3:16 WEB, then Rom 8:28.", OffsetUnit.Utf16, ToposQuery.create(), names);
// → [ ['John 3:16', 'WEB'], ['Romans 8:28', null] ] via m.reference.passage.reference, m.translation?.name
```

Python: same names in snake_case, `OffsetUnit.CHAR`, `BookStyle.NAME`.

## Showing a passage

A book writes passages itself; `passage` comes from `topos.parse("John 3:16-18", BookStyle.Name)`.

```ts
import { type BookTextOptions } from "logoxide";
const options: BookTextOptions = { notes: false, headings: true, classPrefix: "bible-" };
book.text(passage, options);      // string[] (one per part of the reference)
book.html(passage, options);      // string[]: verse markers are <span class="bible-verse" data-n="16"></span>
book.chapterHtml(3, options);     // a whole chapter
```

`text` has no verse numbers; `styledText(passage, style, bookName, unit)` adds labels, chapter
labels where a passage crosses chapters, templates for words of Jesus, wrapping (`width`), and a
byte `limit`, with `marks` saying where each verse and heading is. Every field of the style is
required. Placeholders: `{verse}` (`4-5` for a bridge), `{chapter}`, `{name}` (the `bookName`
argument), `{book}` (USX code). A passage the translation lacks (BSB's Acts 8:37) gives `null` /
`None` (or empty text): say so rather than print nothing.

```python
from logoxide import BookStyle, BookTextStyle, OffsetUnit

passage = topos.parse("Romans 8:39-9:1", BookStyle.NAME)   # .book_id 45, .book "Romans", .reference
style = BookTextStyle(verse_label="[{verse}]", chapter_label="{name} {chapter}", notes=False,
                      headings=False, templates=[], width=None, limit=None)
styled = book.styled_text(passage, style, passage.book, OffsetUnit.CHAR)
print(styled.text)
# → [39] neither height nor depth, … in Christ Jesus our Lord.
# → Romans 9
# → [1] I speak the truth in Christ; …
```

`topos.parse` returns `null`/`None` for an unknown book, a chapter or verse out of range, and
junk alike. To tell them apart, parse the book name alone (`topos.parse("Romans")` is the whole
book; `topos.parse("Hezekiah")` is `None`), or check the book's `chapters` in `topos.books()`.
