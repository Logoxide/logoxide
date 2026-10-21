# A passage server

A small HTTP API that answers `GET /passage?ref=John+3:16&t=bsb` with the passage as JSON, for a
site, a chat bot, or a mobile app that shouldn't carry logoxide itself. It downloads each book
from fetch.bible the first time it's asked for and keeps the books it has read in memory, parses
references with topos (so `ref=Jn+3.16+KJV` names its own translation), answers what it can't
with a 400 or 404 and logoxide's error kind, and lets browsers and caches keep its answers.

Each step is in Python (the standard library's `http.server`, so it runs with nothing but
`pip install logoxide`), TypeScript (Node's `node:http`), and Rust (`tiny_http`), starting from
the helpers in [Bibles in your app](../apps.md#setting-up): the blocks there and here, in order,
make one program in each language. In Python, FastAPI is the usual choice for a real service; the
functions here are the same under it.

- [What it answers](#what-it-answers)
- [Translations, at startup](#translations-at-startup)
- [Books, kept in memory](#books-kept-in-memory)
- [A reference, read](#a-reference-read)
- [Serving it](#serving-it)
- [Before it's public](#before-its-public)

## What it answers

```sh
curl -s 'localhost:8000/passage?ref=John+3:16&t=bsb'
```

```json
{
  "reference": "John 3:16",
  "translation": {
    "id": "bsb",
    "abbreviation": "BSB",
    "name": "Berean Standard Bible",
    "license": "LicenseRef-PublicDomain",
    "copyright": "BSB Publishing, LLC"
  },
  "verses": [
    {
      "chapter": 3,
      "verse": 16,
      "last": 16,
      "text": "For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life."
    }
  ],
  "text": "For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life."
}
```

A translation named in the reference wins over `t`, and what can't be answered says why (the
Python server's answers; the others differ only in how they quote):

| Request | Status | Body |
|---|---|---|
| `ref=Jn+11:35+KJV` | 200 | `{"reference": "John 11:35", "translation": {"id": "kjv", …}, "verses": […], "text": "Jesus wept."}` |
| `ref=hello` | 400 | `{"error": "Invalid", "message": "not a reference: 'hello'"}` |
| `ref=John+3:16&t=esv` | 404 | `{"error": "NotFound", "message": "no translation 'esv' (one of bsb, web, kjv)"}` |
| `ref=John+3:16+ESV` | 404 | `{"error": "NotFound", "message": "no translation 'ESV' (one of bsb, web, kjv)"}` |
| `ref=John+5:4&t=bsb` | 404 | `{"error": "NotFound", "message": "John 5:4 isn't in the BSB"}` (the BSB leaves the verse out) |

A 200 carries `Access-Control-Allow-Origin: *`, `Cache-Control: public, max-age=86400`, and an
`ETag`; asked again with `If-None-Match`, the server answers `304 Not Modified`.

## Translations, at startup

The server reads fetch.bible's list once and keeps, for each translation it serves, its catalog
entry (name, license, copyright) and each book's URLs. The names a request can give are topos's
translations, so `John 3:16 KJV` is read as a reference and a translation. `textVersion()` goes
into each answer's `ETag`, so a new logoxide that reads files differently changes them.

```ts
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { BookStyle, LogoxideErrorException, OffsetUnit, ToposTranslations, type CatalogEntry, type TranslatedPassage } from "logoxide";

/** The translations served, by the name a request gives (`t=bsb`), and their import names */
const SERVED: Record<string, string> = { bsb: "fetch:eng_bsb", web: "fetch:eng_webu", kjv: "fetch:eng_kjv" };
const DEFAULT = "bsb";
const FETCH_BIBLE = "https://v1.fetch.bible/manifest.json";
const catalog = lx.parseCatalogBytes(FETCH_BIBLE, await get([FETCH_BIBLE]));
const served = new Map<string, { entry: CatalogEntry; books: Map<number, string[]> }>();
let names = ToposTranslations.create();
for (const [name, source] of Object.entries(SERVED)) {
  const entry = catalog.findImport(source)!;
  served.set(name, { entry, books: new Map(lx.bookDownloads(entry).map((b) => [b.bookId, b.urls])) });
  names = names.add(name.toUpperCase(), entry.abbreviation && entry.abbreviation.toLowerCase() !== name ? [entry.abbreviation] : []);
}
catalog.dispose();
const VERSION = lx.textVersion();
```

```python
import hashlib
import json
from collections import OrderedDict
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock
from urllib.parse import parse_qs, urlparse

from logoxide import BookStyle, LogoxideErrorException, OffsetUnit, ToposTranslations

# The translations served, by the name a request gives (`t=bsb`), and their import names
SERVED = {"bsb": "fetch:eng_bsb", "web": "fetch:eng_webu", "kjv": "fetch:eng_kjv"}
DEFAULT = "bsb"
FETCH_BIBLE = "https://v1.fetch.bible/manifest.json"
catalog = lx.parse_catalog_bytes(FETCH_BIBLE, get([FETCH_BIBLE]))
entries = {name: catalog.find_import(source) for name, source in SERVED.items()}
downloads = {name: {b.book_id: b.urls for b in lx.book_downloads(entry)} for name, entry in entries.items()}
names = ToposTranslations.create()
for name, entry in entries.items():
    names = names.add(name.upper(), [entry.abbreviation] if entry.abbreviation.lower() != name else [])
VERSION = lx.text_version()
```

```rust
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

use logoxide::catalog::{CatalogEntry, FETCH_BIBLE_CATALOG};
use logoxide_text::{
    MemoryFiles, Source,
    passage::{VerseText, verse_texts},
    source::open_loose,
    topos_bible::{
        data::{
            books::BookId,
            translations::{TranslatedPassage, Translations},
        },
        segments::formatter::FormatOptions,
    },
};
use serde_json::{Value, json};

/// The translations served, by the name a request gives (`t=bsb`), and their import names
const SERVED: [(&str, &str); 3] = [("bsb", "fetch:eng_bsb"), ("web", "fetch:eng_webu"), ("kjv", "fetch:eng_kjv")];
const DEFAULT: &str = "bsb";

/// A book's verses, as read
type Verses = Arc<Vec<VerseText>>;

struct Served {
    entry: CatalogEntry,
    /// Each book's URLs, by topos's book number
    books: BTreeMap<u8, Vec<String>>,
}

struct Server {
    served: BTreeMap<String, Served>,
    names: Translations,
    version: String,
    /// Books' verses, the most recently used last
    books: Mutex<Vec<((String, u8), Verses)>>,
}

impl Server {
    fn new() -> Result<Server> {
        let catalog = logoxide_import::catalog::parse(FETCH_BIBLE_CATALOG, &get(&[FETCH_BIBLE_CATALOG.into()])?)?;
        let usfm = logoxide_text::bible_data().books();
        let mut served = BTreeMap::new();
        for (name, source) in SERVED {
            let entry = catalog.find_import(source).ok_or(source)?.clone();
            let books = (logoxide_import::downloads(&entry)?.into_iter())
                .filter_map(|d| Some((usfm.search_usfm(&d.name.to_ascii_uppercase())?.0, d.urls)))
                .collect();
            served.insert(name.to_string(), Served { entry, books });
        }
        Ok(Server {
            names: Translations::new(SERVED.map(|(name, _)| name.to_ascii_uppercase())),
            served,
            version: logoxide_text::cache::text_version(),
            books: Mutex::new(vec![]),
        })
    }
}
```

In Rust the dependencies are `logoxide`, `logoxide-import`, `logoxide-text`, `ureq` (for the
helpers), `serde_json`, `tiny_http`, and `url`.

Starting takes a few seconds, most of it fetch.bible's 750 KB list. A server that restarts often
can keep the list on disk, or keep only the entries it serves.

## Books, kept in memory

A book is downloaded and parsed the first time a passage in it is asked for, then kept: the 30
most recently used (the BSB's first 30 books took 63 MB in Python). In Rust only their verses are
kept, which is less. A book the
translation doesn't have (a New Testament alone has no Genesis) is `null`, and the request becomes
a 404. In TypeScript the promise is kept, so two requests for one book download it once, and a
book pushed out is `dispose`d.

```ts
/** Parsed books, the most recently used last (a Map keeps the order its keys were set in) */
const books = new Map<string, Promise<LogoxideBook | null>>();

function book(name: string, bookId: number): Promise<LogoxideBook | null> {
  const key = `${name}/${bookId}`;
  let found = books.get(key);
  if (found) books.delete(key); // set again below, as the newest
  else {
    const urls = served.get(name)!.books.get(bookId);
    found = urls ? get(urls).then((bytes) => LogoxideBook.parse(topos, "usx", new TextDecoder().decode(bytes))) : Promise.resolve(null);
    found.catch(() => books.delete(key)); // a failed download is tried again
  }
  books.set(key, found);
  for (const [oldest, evicted] of books) {
    if (books.size <= 30) break;
    books.delete(oldest);
    evicted.then((b) => b?.dispose(), () => {});
  }
  return found;
}
```

```python
# Parsed books, the most recently used last
books: OrderedDict[tuple[str, int], LogoxideBook] = OrderedDict()
books_lock = Lock()


def book(name: str, book_id: int) -> LogoxideBook | None:
    key = (name, book_id)
    with books_lock:
        if key in books:
            books.move_to_end(key)
            return books[key]
    urls = downloads[name].get(book_id)
    if urls is None:
        return None  # the translation hasn't this book
    parsed = LogoxideBook.parse(topos, "usx", get(urls).decode())
    with books_lock:
        books[key] = parsed
        while len(books) > 30:
            books.popitem(last=False)
    return parsed
```

```rust
impl Server {
    /// A book's verses, downloaded and read the first time; `None` if the translation hasn't it
    fn book(&self, name: &str, book: u8) -> Result<Option<Verses>> {
        let key = (name.to_string(), book);
        {
            let mut books = self.books.lock().map_err(|_| "poisoned")?;
            if let Some(at) = books.iter().position(|(k, _)| *k == key) {
                let found = books.remove(at);
                books.push(found.clone());
                return Ok(Some(found.1));
            }
        }
        let Some(urls) = self.served[name].books.get(&book) else { return Ok(None) };
        let files = MemoryFiles::with([("book.usx", get(urls)?)]);
        let verses = Arc::new(verse_texts(&open_loose(&files, "")?.book(BookId(book))?));
        let mut books = self.books.lock().map_err(|_| "poisoned")?;
        books.push((key, verses.clone()));
        if books.len() > 30 {
            books.remove(0);
        }
        Ok(Some(verses))
    }
}
```

## A reference, read

topos parses the reference and the translation named after it, if it's one of the names it was
given. A name it wasn't given, like `ESV` in `John 3:16 ESV`, is left over rather than failing,
so `unknownName` catches it: without the last word, the reference still reads as the same
passage. A passage's verses come from `topos.verses`. A verse a file prints with others (`4-5`)
is one entry, kept when any verse of it is in the passage.

```ts
/**
 * A word after the reference that names none of the translations (`ESV` in `John 3:16 ESV`),
 * which topos leaves over rather than failing on
 */
function unknownName(reference: string, found: TranslatedPassage): string | null {
  const words = reference.trim().split(/\s+/);
  const last = words.pop()!;
  const shorter = words.length && !found.translation ? topos.parse(words.join(" "), BookStyle.Name) : null;
  return shorter?.reference === found.passage.reference ? last : null;
}

async function passage(query: URLSearchParams): Promise<[number, object]> {
  const reference = query.get("ref") ?? "";
  const found = topos.parseWithTranslation(reference, OffsetUnit.Utf16, names, BookStyle.Name);
  if (!found) return [400, { error: "Invalid", message: `not a reference: ${JSON.stringify(reference)}` }];
  const unknown = unknownName(reference, found);
  if (unknown) return [404, { error: "NotFound", message: `no translation "${unknown}" (one of ${Object.keys(SERVED).join(", ")})` }];
  // The translation the reference names wins over `t`
  const name = (found.translation?.name ?? query.get("t") ?? DEFAULT).toLowerCase();
  const translation = served.get(name);
  if (!translation) return [404, { error: "NotFound", message: `no translation "${name}" (one of ${Object.keys(SERVED).join(", ")})` }];
  const { entry } = translation;
  const text = await book(name, found.passage.bookId);
  const wanted = new Set(topos.verses(found.passage).map((v) => `${v.chapter}:${v.verse}`));
  // A bridge (4-5) is one entry; it's in the passage if any verse of it is
  const inPassage = (chapter: number, first: number, last: number) =>
    Array.from({ length: Math.max(last, first) - first + 1 }, (_, i) => `${chapter}:${first + i}`).some((v) => wanted.has(v));
  const verses = (text?.verseTexts() ?? [])
    .filter((v) => inPassage(v.chapter, v.verse, v.last))
    .map((v) => ({ chapter: v.chapter, verse: v.verse, last: v.last, text: v.text }));
  if (!verses.length) return [404, { error: "NotFound", message: `${found.passage.reference} isn't in the ${entry.abbreviation}` }];
  return [200, {
    reference: found.passage.reference,
    translation: { id: name, abbreviation: entry.abbreviation, name: entry.name, license: entry.license, copyright: entry.copyright },
    verses,
    text: verses.map((v) => v.text).join(" "),
  }];
}
```

```python
def unknown_name(reference: str, found) -> str | None:
    """A word after the reference that names none of the translations (`ESV` in `John 3:16 ESV`),
    which topos leaves over rather than failing on"""
    head, _, last = reference.strip().rpartition(" ")
    shorter = topos.parse(head, BookStyle.NAME) if head and not found.translation else None
    return last if shorter and shorter.reference == found.passage.reference else None


def passage(query: dict[str, list[str]]) -> tuple[int, dict]:
    reference = query.get("ref", [""])[0]
    found = topos.parse_with_translation(reference, OffsetUnit.CHAR, names, BookStyle.NAME)
    if found is None:
        return 400, {"error": "Invalid", "message": f"not a reference: {reference!r}"}
    if unknown := unknown_name(reference, found):
        return 404, {"error": "NotFound", "message": f"no translation {unknown!r} (one of {', '.join(SERVED)})"}
    # The translation the reference names wins over `t`
    name = (found.translation.name if found.translation else query.get("t", [DEFAULT])[0]).lower()
    if name not in entries:
        return 404, {"error": "NotFound", "message": f"no translation {name!r} (one of {', '.join(SERVED)})"}
    entry = entries[name]
    text = book(name, found.passage.book_id)
    wanted = {(v.chapter, v.verse) for v in topos.verses(found.passage)}
    # A bridge (4-5) is one entry; it's in the passage if any verse of it is
    verses = [
        {"chapter": v.chapter, "verse": v.verse, "last": v.last, "text": v.text}
        for v in (text.verse_texts() if text else [])
        if any((v.chapter, n) in wanted for n in range(v.verse, max(v.last, v.verse) + 1))
    ]
    if not verses:
        return 404, {"error": "NotFound", "message": f"{found.passage.reference} isn't in the {entry.abbreviation}"}
    return 200, {
        "reference": found.passage.reference,
        "translation": {"id": name, "abbreviation": entry.abbreviation, "name": entry.name, "license": entry.license, "copyright": entry.copyright},
        "verses": verses,
        "text": " ".join(v["text"] for v in verses),
    }
```

In Rust, topos's own API is lower-level than the bindings': `parse_with_translation` on
logoxide-text's `bible_data()`, `Passage::verses` with the book's verse counts, and
`FormatOptions` to write the reference out. logoxide-text's book data has the deuterocanon, so
`Tobit 1:1` is read there and answered with a 404 (none of these Bibles has it), where the
bindings' topos doesn't read it as a reference (a 400).

```rust
impl Server {
    /// A word after the reference that names none of the translations (`ESV` in `John 3:16
    /// ESV`), which topos leaves over rather than failing on
    fn unknown_name<'a>(reference: &'a str, found: &TranslatedPassage) -> Option<&'a str> {
        let (head, last) = reference.trim().rsplit_once(char::is_whitespace)?;
        let shorter = (found.translation.is_none()).then(|| logoxide_text::bible_data().parse(head))??;
        (shorter == found.passage).then_some(last)
    }

    fn passage(&self, query: &BTreeMap<String, String>) -> Result<(u16, Value)> {
        let reference = query.get("ref").map_or("", String::as_str);
        let data = logoxide_text::bible_data();
        let Some(found) = data.parse_with_translation(reference, &self.names) else {
            return Ok((400, json!({ "error": "Invalid", "message": format!("not a reference: {reference:?}") })));
        };
        if let Some(unknown) = Server::unknown_name(reference, &found) {
            let names = SERVED.map(|(n, _)| n).join(", ");
            return Ok((404, json!({ "error": "NotFound", "message": format!("no translation {unknown:?} (one of {names})") })));
        }
        // The translation the reference names wins over `t`
        let name = (found.translation.map(|t| t.name))
            .or_else(|| query.get("t").cloned())
            .unwrap_or_else(|| DEFAULT.into())
            .to_ascii_lowercase();
        let Some(served) = self.served.get(&name) else {
            let names = SERVED.map(|(n, _)| n).join(", ");
            return Ok((404, json!({ "error": "NotFound", "message": format!("no translation {name:?} (one of {names})") })));
        };
        let passage = found.passage;
        let written = FormatOptions::default().passage(&passage, data).unwrap_or_default();
        let versification = data.chapter_verses().get_chapter_verses(&passage.book);
        let wanted: HashSet<(u8, u8)> = passage.verses(versification).iter().map(|v| (v.chapter, v.verse)).collect();
        let book = self.book(&name, passage.book.0)?;
        // A bridge (4-5) is one entry; it's in the passage if any verse of it is
        let verses: Vec<&VerseText> = (book.iter().flat_map(|b| b.iter()))
            .filter(|v| (v.verse..=v.last.max(v.verse)).any(|n| wanted.contains(&(v.chapter, n))))
            .collect();
        let entry = &served.entry;
        if verses.is_empty() {
            let abbreviation = entry.abbreviation.as_deref().unwrap_or(&name);
            return Ok((404, json!({ "error": "NotFound", "message": format!("{written} isn't in the {abbreviation}") })));
        }
        Ok((200, json!({
            "reference": written,
            "translation": { "id": name, "abbreviation": entry.abbreviation, "name": entry.name, "license": entry.license, "copyright": entry.copyright },
            "verses": verses.iter().map(|v| json!({ "chapter": v.chapter, "verse": v.verse, "last": v.last, "text": v.text })).collect::<Vec<_>>(),
            "text": verses.iter().map(|v| v.text.as_str()).collect::<Vec<_>>().join(" "),
        })))
    }
}
```

## Serving it

Each answer is JSON. A download that failed (or any of logoxide's errors) is a 502 with its kind
(`Network`, and in the bindings `Invalid`, `NotFound`, and the rest: [Errors](../../crates/logoxide-ffi/README.md#errors)),
so a client can tell a bad request from the server's trouble. Every answer may be read by any
site (`Access-Control-Allow-Origin: *`). A 200 may be kept a day, and its `ETag` (a hash of
`textVersion()` and the request) lets a client ask whether it changed.

```ts
createServer(async (request, response) => {
  const url = new URL(request.url ?? "/", "http://localhost");
  let status: number, body: object;
  if (url.pathname !== "/passage") [status, body] = [404, { error: "NotFound", message: "GET /passage?ref=John+3:16&t=bsb" }];
  else {
    try {
      [status, body] = await passage(url.searchParams);
    } catch (error) {
      // A download that failed, or logoxide's error with its kind
      const kind = error instanceof LogoxideErrorException ? error.value.tag : "Network";
      [status, body] = [502, { error: kind, message: error instanceof Error ? error.message : String(error) }];
    }
  }
  const etag = `"${createHash("sha256").update(`${VERSION} ${request.url}`).digest("hex").slice(0, 16)}"`;
  if (status === 200 && request.headers["if-none-match"] === etag) {
    response.writeHead(304, { ETag: etag }).end();
    return;
  }
  response.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Access-Control-Allow-Origin": "*",
    // The text changes only with a new logoxide (VERSION) or a translation's corrections
    ...(status === 200 ? { "Cache-Control": "public, max-age=86400", ETag: etag } : {}),
  });
  response.end(JSON.stringify(body));
}).listen(8000, "127.0.0.1");
```

```python
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        url = urlparse(self.path)
        if url.path != "/passage":
            return self.send(404, {"error": "NotFound", "message": "GET /passage?ref=John+3:16&t=bsb"})
        try:
            status, body = passage(parse_qs(url.query))
        except LogoxideErrorException as error:
            status, body = 502, {"error": type(error.error).__name__.removeprefix("LogoxideError"), "message": error.error.message}
        except OSError as error:  # a download failed
            status, body = 502, {"error": "Network", "message": str(error)}
        self.send(status, body)

    def send(self, status: int, body: dict):
        data = json.dumps(body, ensure_ascii=False).encode()
        etag = '"' + hashlib.sha256(f"{VERSION} {self.path}".encode()).hexdigest()[:16] + '"'
        if status == 200 and self.headers.get("If-None-Match") == etag:
            self.send_response(304)
            self.send_header("ETag", etag)
            self.end_headers()
            return
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Access-Control-Allow-Origin", "*")
        if status == 200:
            # The text changes only with a new logoxide (VERSION) or a translation's corrections
            self.send_header("Cache-Control", "public, max-age=86400")
            self.send_header("ETag", etag)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


ThreadingHTTPServer(("127.0.0.1", 8000), Handler).serve_forever()
```

```rust
impl Server {
    fn respond(&self, request: tiny_http::Request) -> std::io::Result<()> {
        let url = url::Url::parse(&format!("http://localhost{}", request.url())).ok();
        let (status, body) = match &url {
            Some(url) if url.path() == "/passage" => {
                let query = url.query_pairs().into_owned().collect();
                // A download that failed, or logoxide's error with its kind
                self.passage(&query).unwrap_or_else(|error| (502, json!({ "error": "Network", "message": error.to_string() })))
            }
            _ => (404, json!({ "error": "NotFound", "message": "GET /passage?ref=John+3:16&t=bsb" })),
        };
        // Any stable hash does; this is the first bytes of a SHA-256
        let hash = logoxide_text::hash::verse_hash(&format!("{} {}", self.version, request.url()));
        let etag = format!("\"{}\"", hash.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let header = |name: &str, value: &str| tiny_http::Header::from_bytes(name, value).expect("a valid header");
        let matches = request.headers().iter().any(|h| h.field.equiv("If-None-Match") && h.value == etag);
        if status == 200 && matches {
            return request.respond(tiny_http::Response::empty(304).with_header(header("ETag", &etag)));
        }
        let mut response = tiny_http::Response::from_string(body.to_string())
            .with_status_code(status)
            .with_header(header("Content-Type", "application/json; charset=utf-8"))
            .with_header(header("Access-Control-Allow-Origin", "*"));
        if status == 200 {
            // The text changes only with a new logoxide (version) or a translation's corrections
            response.add_header(header("Cache-Control", "public, max-age=86400"));
            response.add_header(header("ETag", &etag));
        }
        request.respond(response)
    }
}

fn main() -> Result<()> {
    let server = Arc::new(Server::new()?);
    let http = Arc::new(tiny_http::Server::http("127.0.0.1:8000").map_err(|e| e.to_string())?);
    // Four threads answering, so one slow download doesn't hold up the rest
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let (server, http) = (server.clone(), http.clone());
            std::thread::spawn(move || {
                for request in http.incoming_requests() {
                    let _ = server.respond(request);
                }
            })
        })
        .collect();
    for worker in workers {
        let _ = worker.join();
    }
    Ok(())
}
```

## Before it's public

- **Licenses.** The three translations here are public domain. Most aren't: a translation's
  `rights` (in its catalog entry) say whether it may be redistributed, and a server handing out
  text is redistributing it. Serve only what you may, and keep `license` and `copyright` in each
  answer.
- **Startup and memory.** To answer the first request for each book quickly, read the books you
  expect at startup (all 66 of the BSB took 87 MB in Python), or keep each book's verses
  rather than its parsed tree, as the Rust server does.
- **Whole Bibles.** For eBible.org's translations, download each `.zip` once at startup and read
  it with `openBible` ([A whole Bible](../apps.md#a-whole-bible)); a server has no CORS limits.
- **Your own copies.** fetch.bible serves anyone, but a busy server should keep the files it
  downloads (on disk, by URL) rather than download a book again after a restart.
- **References in text.** To find every reference in a message (a chat bot's), use
  `searchWithTranslations` rather than `parseWithTranslation`: the bindings' README, under
  [References, with topos](../../crates/logoxide-ffi/README.md#references-with-topos).
