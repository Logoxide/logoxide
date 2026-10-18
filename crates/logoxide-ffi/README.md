# logoxide (bindings)

logoxide's packages with topos's whole API, in one package for TypeScript/WebAssembly, Python,
Swift, and Kotlin (through [BoltFFI](https://github.com/boltffi/boltffi)). `Topos`,
`ToposTranslations`, and the rest are the same as in topos's own package (`topos-bible`). A book's
text is logoxide's own `LogoxideBook` (`pkg.book(id)`, or `LogoxideBook.parse(topos, "usx", text)`
for a file of your own): its verses, and passages as HTML or plain text. It was topos's
`ToposBook`, with the same methods; its `BookVerse` and `BookTextOptions` keep their names.

The host reads and writes the files: an app's file API is often asynchronous (Obsidian's vault),
and WebAssembly has none. logoxide reads the text from the bytes it's given, and hands back the
cache files it wrote for the host to save. Only the Python (and Swift and Kotlin) packages are
native, so only they have `openPackageDir`, `readPassage`, and `comparePassage`, which read
folders themselves; the npm package is WebAssembly in Node too.

## The examples

Each feature below has an example in TypeScript and in Python. They run in order, as one program:
each uses what the ones before it made (`kjv`, `bsb`, `project`, `installer`, ...). What a line
prints is written after it, following `→` (`…` stands for text left out).

They're checked: `examples/node/readme.ts` and `examples/python/readme.py` run every example here
and compare what it prints with its `→` comments. They run in a folder holding the KJV and BSB
packages (`kjv/` and `bsb/`) and the files in `examples/fixtures`, and answer the downloads the
examples make offline: `https://example.org/kjv` and `https://example.org/bsb` are those packages'
repositories, and `https://example.org/catalog.toml` and eBible.org's list and zip are fixtures,
so what's printed is theirs. With real URLs the code is the same.

- [Loading](#loading)
- [Opening a package](#opening-a-package)
- [References, with topos](#references-with-topos)
- [A book's text](#a-books-text)
- [Commentaries, dictionaries, and cross-references](#commentaries-dictionaries-and-cross-references)
- [Audio and video](#audio-and-video)
- [Versification maps](#versification-maps)
- [A vault's project](#a-vaults-project)
- [Catalogs](#catalogs)
- [Installing](#installing)
- [The vault's packages](#the-vaults-packages)
- [Imports](#imports)
- [Making packages](#making-packages)
- [Reading natively](#reading-natively)
- [Errors](#errors)
- [Every export](#every-export)
- [Building](#building)

## Loading

In Node and Bun the WebAssembly module is ready on import (in a browser, `await initialized`
first). Names are camelCase in TypeScript and snake_case in Python. Records are plain objects in
TypeScript and frozen dataclasses in Python, every field given; enums without data are numbers in
TypeScript (`PackageKind.Bible` is `0`) and `IntEnum`s in Python (`PackageKind.BIBLE`); enums with
data are `{ tag: "Library", dir }` objects in TypeScript and a class each in Python
(`PackageLocationLibrary(dir=...)`). A field or argument named `import` is `_import` in
TypeScript and `import_` in Python, and `from` is `from_` in Python. Bytes are `Uint8Array` and
`bytes`, and so are lists of small numbers (`books()`, verse counts); 64-bit numbers (a file's
`size`) are `bigint`s in TypeScript.

The host's own helpers, used by the examples below: a folder's files, writing files, and HTTP.

```ts
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { Logoxide, Topos, type PackageFile } from "logoxide";

/** Every file under a folder but git's, with `/`-separated paths relative to it */
function files(dir: string, root = dir): PackageFile[] {
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.name !== ".git")
    .flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return files(path, root);
      return [{ path: relative(root, path).split(sep).join("/"), bytes: new Uint8Array(readFileSync(path)) }];
    });
}

/** Writes files under a folder */
function write(dir: string, written: PackageFile[]) {
  for (const file of written) {
    mkdirSync(dirname(join(dir, file.path)), { recursive: true });
    writeFileSync(join(dir, file.path), file.bytes);
  }
}

/** A file from its text */
const file = (path: string, text: string): PackageFile => ({ path, bytes: new TextEncoder().encode(text) });

/** The host's HTTP GET (`requestUrl` in Obsidian) */
async function get(url: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  return new Uint8Array(await response.arrayBuffer());
}

/** The first of several URLs that answers */
async function getFirst(urls: string[]): Promise<Uint8Array> {
  for (const url of urls.slice(0, -1)) {
    try { return await get(url); } catch { /* the next */ }
  }
  return get(urls[urls.length - 1]);
}

const logoxide = Logoxide.new();
const topos = Topos.new();
console.log(logoxide.version()); // → 0.1.0
```

```python
import dataclasses
import pathlib
import urllib.request

from logoxide import Logoxide, PackageFile, Topos


def files(folder: str) -> list[PackageFile]:
    """Every file under a folder but git's, with /-separated paths relative to it"""
    root = pathlib.Path(folder)
    return [
        PackageFile(path=p.relative_to(root).as_posix(), bytes=p.read_bytes())
        for p in sorted(root.rglob("*"))
        if p.is_file() and ".git" not in p.relative_to(root).parts
    ]


def write(folder: str, written: list[PackageFile]) -> None:
    """Writes files under a folder"""
    for f in written:
        path = pathlib.Path(folder, f.path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(f.bytes)


def file(path: str, text: str) -> PackageFile:
    """A file from its text"""
    return PackageFile(path=path, bytes=text.encode())


def get(url: str) -> bytes:
    """The host's HTTP GET"""
    with urllib.request.urlopen(url) as response:
        return response.read()


def get_first(urls: list[str]) -> bytes:
    """The first of several URLs that answers"""
    for url in urls[:-1]:
        try:
            return get(url)
        except OSError:
            pass
    return get(urls[-1])


logoxide = Logoxide()
topos = Topos()
print(logoxide.version())  # → 0.1.0
```

## Opening a package

`openPackage` takes every file under the package's folder (but `.git`), with paths like
`usx/JHN.usx`, the features to turn on, and the cache files saved before. Opening counts every
book's verses unless the cache has them (for USFM that means parsing the whole text, under a
second for the KJV), so the host saves what `cacheWrites()` hands back and gives it back next
time. `info()` is the package's `[package]`; `checksum()` is what a lock file's `checksum` should
be, to catch changed files.

```ts
import { PackageKind } from "logoxide";

const features = { features: [], defaultFeatures: true }; // Cargo's --features and --no-default-features
const cache: PackageFile[] = []; // the host's cache folder
const kjv = logoxide.openPackage(files("kjv"), features, cache);
cache.push(...kjv.cacheWrites()); // save these, and give them back next time
const info = kjv.info();
console.log(info.id, info.abbreviation, info.aliases, info.version); // → kjv KJV [ 'AV', 'King James' ] 1.0.3
console.log(info.name, info.kind === PackageKind.Bible, info.language, info.rightToLeft); // → King James Version (1769) true en false
console.log(info.license, info.rights); // → LicenseRef-PublicDomain { redistributable: true, derivatives: 0, commercial: true, attribution: false }
console.log(info.work, info.edition, info.editionYear, info.versification); // → kjv 1769 1769 KJV
console.log(info.upstream?.note); // → fetch.bible's copy of eBible.org's eng-kjv2006
console.log(kjv.format(), kjv.books().length, kjv.books()[0], kjv.chapterVerses(43).length); // → usfm 66 1 21
console.log(kjv.checksum()); // → sha256:…
console.log(logoxide.openPackage(files("kjv"), features, cache).cacheWrites().length); // → 0
```

```python
from logoxide import PackageFeatures, PackageKind

features = PackageFeatures(features=[], default_features=True)
cache: list[PackageFile] = []  # the host's cache folder
kjv = logoxide.open_package(files("kjv"), features, cache)
cache += kjv.cache_writes()  # save these, and give them back next time
info = kjv.info()
print(info.id, info.abbreviation, info.aliases, info.version)  # → kjv KJV ['AV', 'King James'] 1.0.3
print(info.name, info.kind == PackageKind.BIBLE, info.language, info.right_to_left)  # → King James Version (1769) True en False
print(info.license, info.rights)  # → LicenseRef-PublicDomain Rights(redistributable=True, derivatives=<Derivatives.ALLOWED: 0>, commercial=True, attribution=False)
print(info.work, info.edition, info.edition_year, info.versification)  # → kjv 1769 1769 KJV
print(info.upstream.note)  # → fetch.bible's copy of eBible.org's eng-kjv2006
print(kjv.format(), len(kjv.books()), kjv.books()[0], len(kjv.chapter_verses(43)))  # → usfm 66 1 21
print(kjv.checksum())  # → sha256:…
print(len(logoxide.open_package(files("kjv"), features, cache).cache_writes()))  # → 0
```

Its files: each book's file and title (`bookFiles`, `titles`), what its format passed over
(`skipped`: front matter, books logoxide doesn't know), and the hash of each verse's words
(`verseHashes`: verse-hash rule 1, the first 7 bytes of a SHA-256, which links keep to find a
verse again in a later version).

```ts
console.log(kjv.bookFiles().find((f) => f.bookId === 43)); // → { bookId: 43, path: 'usfm/jhn.usfm', title: 'John' }
console.log(kjv.titles()[0], kjv.skipped()); // → { path: 'usfm/1ch.usfm', title: '1 Chronicles' } []
const hashes = kjv.verseHashes(43);
console.log(hashes.length, hashes.find((v) => v.chapter === 3 && v.verse === 16)); // → 879 { chapter: 3, verse: 16, last: 16, part: 0, hash: '…' }
```

```python
print(next(f for f in kjv.book_files() if f.book_id == 43))  # → BookFile(book_id=43, path='usfm/jhn.usfm', title='John')
print(kjv.titles()[0], kjv.skipped())  # → TitledFile(path='usfm/1ch.usfm', title='1 Chronicles') []
hashes = kjv.verse_hashes(43)
print(len(hashes), next(v for v in hashes if (v.chapter, v.verse) == (3, 16)))  # → 879 VerseHashEntry(chapter=3, verse=16, last=16, part=0, hash='…')
```

Natively, `openPackageDir` reads a folder itself and keeps its cache in a folder of its own (so
`cacheWrites()` is empty); in WebAssembly it's an `Io` error. `readPackageInfo` reads a
package's `logoxide.toml` alone, without its text (to name a package that's turned off).

```ts
import { LogoxideErrorException } from "logoxide";

const bsb = logoxide.openPackage(files("bsb"), features, cache);
cache.push(...bsb.cacheWrites());
try {
  logoxide.openPackageDir("bsb", features, "cache");
} catch (error) {
  console.log((error as LogoxideErrorException).value); // → { tag: 'Io', message: 'no file access in WebAssembly: use openPackage' }
}
console.log(logoxide.readPackageInfo(readFileSync("bsb/logoxide.toml", "utf8")).name); // → Berean Standard Bible
```

```python
bsb = logoxide.open_package_dir("bsb", features, "cache")
print(bsb.info().abbreviation, bsb.format(), bsb.cache_writes())  # → BSB usx []
print(logoxide.read_package_info(pathlib.Path("bsb/logoxide.toml").read_text()).name)  # → Berean Standard Bible
```

## References, with topos

topos finds and parses references; everything it does (searching text, completing references,
queries, indexes, formats) is in this package as in topos's own, and is described in [topos's
bindings' guide](https://github.com/MasterTemple/topos-bible/tree/main/crates/topos-ffi). A
package adds itself to topos's `ToposTranslations` (`addTo`: its abbreviation, aliases, and verse
counts), so a reference naming it is checked against its own verses; `addToAs` adds it by the
names a project gives it instead.

```ts
import { BookStyle, OffsetUnit, ToposTranslations } from "logoxide";

let translations = ToposTranslations.create();
translations = kjv.addTo(translations);
translations = bsb.addToAs(translations, "B", ["Berean"]);
for (const reference of ["John 3:16 AV", "3 John 1:15 B", "3 John 1:14 Berean"]) {
  const found = topos.parseWithTranslation(reference, OffsetUnit.Utf16, translations, BookStyle.Name);
  console.log(found ? `${found.passage.reference} in ${found.translation?.name}` : "no such verse");
}
// → John 3:16 in KJV
// → no such verse
// → 3 John 1:14 in B
```

```python
from logoxide import BookStyle, OffsetUnit, ToposTranslations

translations = ToposTranslations.create()
translations = kjv.add_to(translations)
translations = bsb.add_to_as(translations, "B", ["Berean"])
for reference in ["John 3:16 AV", "3 John 1:15 B", "3 John 1:14 Berean"]:
    found = topos.parse_with_translation(reference, OffsetUnit.CHAR, translations, BookStyle.NAME)
    print(f"{found.passage.reference} in {found.translation.name}" if found else "no such verse")
# → John 3:16 in KJV
# → no such verse
# → 3 John 1:14 in B
```

## A book's text

`book(id)` parses a book once (cached) for passages as plain text or HTML, one string for each
part of a reference (`John 3:16; 4` has two). HTML uses logoxide-markup's classes, each with
`classPrefix`: verse and chapter markers are empty `<span class="topos-verse" data-n="16">`s a
stylesheet can number (`data-osis-id` is the verse as the book names it). Each verse runs from the end of the one before it, so a heading belongs to
the verse after it.

```ts
import { type BookTextOptions } from "logoxide";

const john = topos.parse("John 3:16-17", BookStyle.Name)!;
const options: BookTextOptions = { notes: false, headings: true, classPrefix: "topos-" };
const kjvJohn = kjv.book(john.bookId);
console.log(kjvJohn.bookId(), kjvJohn.chapters().length, kjvJohn.chapterVerses()[2], kjvJohn.verses()[0]); // → 43 21 36 { chapter: 1, verse: 1, last: 1, lastChapter: 1, part: 0 }
console.log(kjvJohn.text(john, options)); // → [ '¶ For God so loved the world, …' ]
const html = bsb.book(43).html(john, options)[0];
console.log(html.slice(html.indexOf("<span"), html.indexOf("</span>") + 7)); // → <span class="topos-verse" data-osis-id="JHN 3:16" data-n="16"></span>
console.log(bsb.book(43).chapterHtml(3, options).length > 0); // → true
```

```python
from logoxide import BookTextOptions

john = topos.parse("John 3:16-17", BookStyle.NAME)
options = BookTextOptions(notes=False, headings=True, class_prefix="topos-")
kjv_john = kjv.book(john.book_id)
print(kjv_john.book_id(), len(kjv_john.chapters()), kjv_john.chapter_verses()[2], kjv_john.verses()[0])  # → 43 21 36 BookVerse(chapter=1, verse=1, last=1, last_chapter=1, part=0)
print(kjv_john.text(john, options))  # → ['¶ For God so loved the world, …']
html = bsb.book(43).html(john, options)[0]
print(html[html.index("<span") : html.index("</span>") + 7])  # → <span class="topos-verse" data-osis-id="JHN 3:16" data-n="16"></span>
print(len(bsb.book(43).chapter_html(3, options)) > 0)  # → True
```

`styledText` writes a passage as the language server shows it: verse labels, a chapter label on
its own line where a chapter starts inside the passage, words of Jesus and the like in
templates, lines wrapped at `width`, and cut at `limit` bytes. Its `marks`
say what each run of it is (a verse number, a heading), by byte offsets into the UTF-8 text.

```ts
import { NodeKind, type BookTextStyle } from "logoxide";

const style: BookTextStyle = {
  verseLabel: "[{verse}]", chapterLabel: "{name} {chapter}", notes: false, headings: false,
  templates: [{ kind: NodeKind.WordsOfJesus, template: "*{text}*" }], width: null, limit: null,
};
const styled = kjvJohn.styledText(topos.parse("John 3:2-3", BookStyle.Name)!, style, "John")!;
console.log(styled.text); // → [2] The same came to Jesus by night, … [3] Jesus answered and said unto him, *Verily, verily, … kingdom of God.*
const utf8 = new TextEncoder().encode(styled.text);
const runs = (kind: NodeKind) => styled.marks.filter((m) => m.kind === kind).map((m) => new TextDecoder().decode(utf8.slice(m.start, m.end)));
console.log(runs(NodeKind.Verse), runs(NodeKind.WordsOfJesus)); // → [ '[2]', '[3]' ] [ '*Verily, verily, I say unto thee, Except a man be born again, he cannot see the kingdom of God.*' ]
console.log(kjvJohn.styledText(topos.parse("John 2:25-3:1", BookStyle.Name)!, { ...style, width: 40, limit: 120 }, "John")!.text);
// → [25] And needed not that any should
// → testify of man: for he knew what was in
// → man.
// → John 3
// → [1] There was a man of the Phari …
```

```python
from logoxide import BookTextStyle, KindTemplate, NodeKind

style = BookTextStyle(
    verse_label="[{verse}]", chapter_label="{name} {chapter}", notes=False, headings=False,
    templates=[KindTemplate(kind=NodeKind.WORDS_OF_JESUS, template="*{text}*")], width=None, limit=None,
)
styled = kjv_john.styled_text(topos.parse("John 3:2-3", BookStyle.NAME), style, "John")
print(styled.text)  # → [2] The same came to Jesus by night, … [3] Jesus answered and said unto him, *Verily, verily, … kingdom of God.*
utf8 = styled.text.encode()
runs = lambda kind: [utf8[m.start : m.end].decode() for m in styled.marks if m.kind == kind]
print(runs(NodeKind.VERSE), runs(NodeKind.WORDS_OF_JESUS))  # → ['[2]', '[3]'] ['*Verily, verily, I say unto thee, Except a man be born again, he cannot see the kingdom of God.*']
print(kjv_john.styled_text(topos.parse("John 2:25-3:1", BookStyle.NAME), dataclasses.replace(style, width=40, limit=120), "John").text)
# → [25] And needed not that any should
# → testify of man: for he knew what was in
# → man.
# → John 3
# → [1] There was a man of the Phari …
```

The tree itself, for a host that renders with its own engine: `nodes()` lists every node in
document order, each naming its parent by index, with its kind, tag name, attributes, and text;
`verseNodes()` says where each verse is in that list (its marker; `start`, its first
node; and `end`, the first node after it, so `nodes.slice(start, end)` is the verse: a heading
before it belongs to it). A milestone (a USX verse marker, an OSIS `sID`/`eID` pair) is a `Start` or an `End`.

```ts
import { MilestoneKind, type BookNode } from "logoxide";

const nodes = bsb.book(43).nodes();
const at = bsb.book(43).verseNodes().find((v) => v.chapter === 3 && v.verse === 16)!;
const marker = nodes[at.marker];
console.log(marker.kind === NodeKind.Verse, marker.name, marker.milestone === MilestoneKind.Start, marker.attributes[0]); // → true verse true { name: 'number', value: '16' }
const inNote = (n: BookNode): boolean => n.parent !== null && (nodes[n.parent].kind === NodeKind.Note || inNote(nodes[n.parent]));
const words = nodes.slice(at.start, at.end).filter((n) => n.kind === NodeKind.Text && !inNote(n));
console.log(words.map((n) => n.text).join("")); // → For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.
```

```python
from logoxide import MilestoneKind

nodes = bsb.book(43).nodes()
at = next(v for v in bsb.book(43).verse_nodes() if (v.chapter, v.verse) == (3, 16))
marker = nodes[at.marker]
print(marker.kind == NodeKind.VERSE, marker.name, marker.milestone == MilestoneKind.START, marker.attributes[0])  # → True verse True NodeAttribute(name='number', value='16')
in_note = lambda n: n.parent is not None and (nodes[n.parent].kind == NodeKind.NOTE or in_note(nodes[n.parent]))
words = [n for n in nodes[at.start : at.end] if n.kind == NodeKind.TEXT and not in_note(n)]
print("".join(n.text for n in words))  # → For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.
```

A book file of your own: `usfmToUsx` turns USFM into USX, `LogoxideBook.parse` reads `usx` or
`osis` (an unknown markup is `InvalidMarkup`, which is `null` in TypeScript), and
`LogoxideBook.usxChapterVerses` counts a USX book's verses from its markers alone, fast enough
for a whole Bible. `LogoxideBook.findFragment` finds a link's words (`:~:text=…`) in a verse
whose words changed a little since, by byte offsets.

```ts
import { LogoxideBook } from "logoxide";

const usx = logoxide.usfmToUsx(readFileSync("kjv/usfm/3jn.usfm", "utf8"));
const thirdJohn = LogoxideBook.parse(topos, "usx", usx)!;
console.log(thirdJohn.bookId(), [...thirdJohn.chapterVerses()], [...LogoxideBook.usxChapterVerses(usx)]); // → 64 [ 14 ] [ 14 ]
console.log(LogoxideBook.parse(topos, "rtf", usx)); // → null
console.log(LogoxideBook.findFragment("For God so greatly loved the world", "so loved the world")); // → { start: 8, end: 34 }
```

```python
from logoxide import LogoxideBook, LogoxideErrorException

usx = logoxide.usfm_to_usx(pathlib.Path("kjv/usfm/3jn.usfm").read_text())
third_john = LogoxideBook.parse(topos, "usx", usx)
print(third_john.book_id(), list(third_john.chapter_verses()), list(LogoxideBook.usx_chapter_verses(usx)))  # → 64 [14] [14]
try:
    LogoxideBook.parse(topos, "rtf", usx)
except LogoxideErrorException as error:
    print(error.error)  # → LogoxideErrorInvalidMarkup(message='unknown markup "rtf" (usx, osis)')
print(LogoxideBook.find_fragment("For God so greatly loved the world", "so loved the world"))  # → TextRange(start=8, end=34)
```

## Commentaries, dictionaries, and cross-references

A commentary's `notes` on a passage are each of its notes covering any verse of it, as text and
HTML (`chapter`:`verse` to `lastChapter`:`last`); with `stretch`, a note also covers the verses
after it up to the next, so a verse the commentary skips gets the note before it.

```ts
const commentaryFiles = [
  file("logoxide.toml", `[package]
id = "notes"
name = "Short Notes"
abbreviation = "SN"
version = "1.0.0"
language = "en"
license = "CC0-1.0"
kind = "commentary"

[formats]
osis = { path = "osis/" }
`),
  file("osis/John.xml", `<osis><osisText osisIDWork="SN"><div type="book" osisID="John"><chapter osisID="John.3" sID="John.3"/>\
<verse osisID="John.3.16" sID="John.3.16" n="16"/>God so loved: the <hi type="italic">world</hi>.<verse eID="John.3.16"/>\
<verse osisID="John.3.17 John.3.18" sID="John.3.17" n="17-18"/>Not to condemn.<verse eID="John.3.17"/>\
<chapter eID="John.3"/></div></osisText></osis>`),
];
const commentary = logoxide.openPackage(commentaryFiles, features, []);
for (const note of commentary.notes(john, options, false)) console.log(`${note.chapter}:${note.verse}-${note.last}`, note.text);
// → 3:16-16 God so loved: the world.
// → 3:17-18 Not to condemn.
console.log(commentary.notes(john, options, false)[0].html); // → <span class="topos-verse" data-osis-id="John.3.16" data-n="16"></span>God so loved: the <i class="topos-hi topos-italic">world</i>.
const after = topos.parse("John 3:19", BookStyle.Name)!;
console.log(commentary.notes(after, options, false).length, commentary.notes(after, options, true)[0].text); // → 0 Not to condemn.
```

```python
commentary_files = [
    file("logoxide.toml", """[package]
id = "notes"
name = "Short Notes"
abbreviation = "SN"
version = "1.0.0"
language = "en"
license = "CC0-1.0"
kind = "commentary"

[formats]
osis = { path = "osis/" }
"""),
    file("osis/John.xml", """<osis><osisText osisIDWork="SN"><div type="book" osisID="John"><chapter osisID="John.3" sID="John.3"/>\
<verse osisID="John.3.16" sID="John.3.16" n="16"/>God so loved: the <hi type="italic">world</hi>.<verse eID="John.3.16"/>\
<verse osisID="John.3.17 John.3.18" sID="John.3.17" n="17-18"/>Not to condemn.<verse eID="John.3.17"/>\
<chapter eID="John.3"/></div></osisText></osis>"""),
]
commentary = logoxide.open_package(commentary_files, features, [])
for note in commentary.notes(john, options, False):
    print(f"{note.chapter}:{note.verse}-{note.last}", note.text)
# → 3:16-16 God so loved: the world.
# → 3:17-18 Not to condemn.
print(commentary.notes(john, options, False)[0].html)  # → <span class="topos-verse" data-osis-id="John.3.16" data-n="16"></span>God so loved: the <i class="topos-hi topos-italic">world</i>.
after = topos.parse("John 3:19", BookStyle.NAME)
print(len(commentary.notes(after, options, False)), commentary.notes(after, options, True)[0].text)  # → 0 Not to condemn.
```

A dictionary's `entry` for a key (`G0025`, `g25`, and `G25` are one key; `αγαπαω` finds
`ἀγαπάω`), `null` for a key it lacks; `keys()` lists them. `info().keys` says what it's keyed by.

```ts
const dictionary = logoxide.openPackage([
  file("logoxide.toml", `[package]
id = "strongs"
name = "Strong's Greek"
abbreviation = "STR"
version = "1.0.0"
language = "grc"
license = "CC0-1.0"
kind = "lexicon"
keys = "strongs-greek"

[formats]
dictionary = { path = "entries/" }
`),
  file("entries/greek.xml", `<entries keys="strongs-greek" markup="tei">
<entry key="G0025" headword="ἀγαπάω"><entryFree><orth>ἀγαπάω</orth> to love</entryFree></entry>
<entry key="G26">love</entry>
</entries>
`),
], features, []);
console.log(dictionary.info().keys, dictionary.keys()); // → strongs-greek [ 'G25', 'G26' ]
const entry = dictionary.entry("g25", options)!;
console.log(entry.key, entry.headword, entry.text); // → G25 ἀγαπάω ἀγαπάω to love
console.log(dictionary.entry("G9999", options)); // → null
```

```python
dictionary = logoxide.open_package([
    file("logoxide.toml", """[package]
id = "strongs"
name = "Strong's Greek"
abbreviation = "STR"
version = "1.0.0"
language = "grc"
license = "CC0-1.0"
kind = "lexicon"
keys = "strongs-greek"

[formats]
dictionary = { path = "entries/" }
"""),
    file("entries/greek.xml", """<entries keys="strongs-greek" markup="tei">
<entry key="G0025" headword="ἀγαπάω"><entryFree><orth>ἀγαπάω</orth> to love</entryFree></entry>
<entry key="G26">love</entry>
</entries>
"""),
], features, [])
print(dictionary.info().keys, dictionary.keys())  # → strongs-greek ['G25', 'G26']
entry = dictionary.entry("g25", options)
print(entry.key, entry.headword, entry.text)  # → G25 ἀγαπάω ἀγαπάω to love
print(dictionary.entry("G9999", options))  # → None
```

A cross-reference package's `crossReferences` for a verse (OSIS, or a reference topos reads) are
its lines covering any verse of it, as OSIS, each with its weight when the package gives one.

```ts
const crossReferences = logoxide.openPackage([
  file("logoxide.toml", `[package]
id = "tsk"
name = "Treasury"
abbreviation = "TSK"
version = "1.0.0"
language = "en"
license = "CC0-1.0"
kind = "cross-references"

[formats]
xref = { path = "xref/" }
`),
  file("xref/gen.tsv", "Gen.1.1\tJohn.1.1-John.1.3\t1\nGen.1.1\tHeb.11.3\t0.8\nGen.1.1-Gen.1.5\tPs.33.6\n"),
], features, []);
for (const x of crossReferences.crossReferences("Genesis 1:1")) console.log(x.from, x.to, x.weight);
// → Gen.1.1 John.1.1-John.1.3 1
// → Gen.1.1 Heb.11.3 0.8
// → Gen.1.1-Gen.1.5 Ps.33.6 null
console.log(crossReferences.keys()); // → [ 'Gen.1.1', 'Gen.1.1-Gen.1.5' ]
```

```python
cross_references = logoxide.open_package([
    file("logoxide.toml", """[package]
id = "tsk"
name = "Treasury"
abbreviation = "TSK"
version = "1.0.0"
language = "en"
license = "CC0-1.0"
kind = "cross-references"

[formats]
xref = { path = "xref/" }
"""),
    file("xref/gen.tsv", "Gen.1.1\tJohn.1.1-John.1.3\t1\nGen.1.1\tHeb.11.3\t0.8\nGen.1.1-Gen.1.5\tPs.33.6\n"),
], features, [])
for x in cross_references.cross_references("Genesis 1:1"):
    print(x.from_, x.to, x.weight)
# → Gen.1.1 John.1.1-John.1.3 1.0
# → Gen.1.1 Heb.11.3 0.8
# → Gen.1.1-Gen.1.5 Ps.33.6 None
print(cross_references.keys())  # → ['Gen.1.1', 'Gen.1.1-Gen.1.5']
```

## Audio and video

A package lists its recordings by URL, size, and SHA-256 (`media()`), never storing them; a
recording without a text of its own names the dependency whose text it reads (`mediaText`) or
the versification it's numbered by (`mediaVersification`). `timings` are where each verse is in
a file, from its WebVTT timings (`parseTimings` reads a timings file that isn't in a package).
Download a file when it's played (its `url`, else each of its `mirrors`), check it with
`verifyMedia` (an `Integrity` error says what differs), and keep it at `mediaStorePath`, by its
hash, so versions listing the same file share it.

```ts
const audio = logoxide.openPackage([
  file("logoxide.toml", `[package]
id = "bsb-audio"
name = "The Berean Standard Bible, read aloud"
abbreviation = "BSBA"
version = "1.0.0"
language = "en"

[dependencies]
bsb = { git = "https://example.org/bsb", version = "1" }

[media]
text = "bsb"
versification = "KJV"
timings = "timings/"

[[media.files]]
path = "JHN.mp3"
url = "https://example.org/audio/JHN.mp3"
size = 5
sha256 = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
book = "JHN"
`),
  file("timings/JHN.vtt", "WEBVTT\n\nJHN 3:16\n00:04.000 --> 00:09.500\nFor God so loved the world\n"),
], features, []);
const [recording] = audio.media();
console.log(recording.path, recording.size, recording.book, audio.mediaText(), audio.mediaVersification()); // → JHN.mp3 5n JHN bsb KJV
console.log(audio.timings(recording)); // → [ { book: 'JHN', bookId: 43, chapter: 3, verse: 16, last: 16, startMs: 4000, endMs: 9500 } ]
console.log(logoxide.parseTimings("WEBVTT\n\nJHN 3:17-18\n00:10.000 --> 00:15.000\n")[0].last); // → 18
const downloaded = new TextEncoder().encode("hello"); // what the host downloaded from recording.url
logoxide.verifyMedia(recording, downloaded);
try {
  logoxide.verifyMedia(recording, new TextEncoder().encode("hellO"));
} catch (error) {
  console.log((error as LogoxideErrorException).value.tag); // → Integrity
}
console.log(logoxide.mediaStorePath(recording)); // → media/2c/2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
```

```python
audio = logoxide.open_package([
    file("logoxide.toml", """[package]
id = "bsb-audio"
name = "The Berean Standard Bible, read aloud"
abbreviation = "BSBA"
version = "1.0.0"
language = "en"

[dependencies]
bsb = { git = "https://example.org/bsb", version = "1" }

[media]
text = "bsb"
versification = "KJV"
timings = "timings/"

[[media.files]]
path = "JHN.mp3"
url = "https://example.org/audio/JHN.mp3"
size = 5
sha256 = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
book = "JHN"
"""),
    file("timings/JHN.vtt", "WEBVTT\n\nJHN 3:16\n00:04.000 --> 00:09.500\nFor God so loved the world\n"),
], features, [])
[recording] = audio.media()
print(recording.path, recording.size, recording.book, audio.media_text(), audio.media_versification())  # → JHN.mp3 5 JHN bsb KJV
print(audio.timings(recording))  # → [VerseTiming(book='JHN', book_id=43, chapter=3, verse=16, last=16, start_ms=4000, end_ms=9500)]
print(logoxide.parse_timings("WEBVTT\n\nJHN 3:17-18\n00:10.000 --> 00:15.000\n")[0].last)  # → 18
downloaded = b"hello"  # what the host downloaded from recording.url
logoxide.verify_media(recording, downloaded)
try:
    logoxide.verify_media(recording, b"hellO")
except LogoxideErrorException as error:
    print(type(error.error).__name__)  # → LogoxideErrorIntegrity
print(logoxide.media_store_path(recording))  # → media/2c/2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
```

## Versification maps

`versifications()` holds no maps; `withMap` adds a map package's (`kind = "versification-map"`)
and `withVrs` a `.vrs` file's text, and `map` gives where a reference numbered one way is in
another, with what of it has no counterpart there. A Bible's numbering is its
`info().versification`; `counts` are the book's verse counts in the first numbering, for whole
chapters (`chapterVerses(book)`; `null` uses topos's).

```ts
const map = logoxide.openPackage([
  file("logoxide.toml", `[package]
id = "english-original"
name = "English to Original"
version = "1.0.0"
kind = "versification-map"
language = "en"
license = "CC0-1.0"
versification = "English"
maps-to = "Original"

[formats]
vrs = { path = "eng-org.vrs" }
`),
  file("eng-org.vrs", "PSA 51:0 = PSA 51:1\nPSA 51:0 = PSA 51:2\nPSA 51:1-19 = PSA 51:3-21\nMAL 4:1-6 = MAL 3:19-24\n"),
], features, []);
console.log(map.info().kind === PackageKind.VersificationMap, map.info().mapsTo); // → true Original
let maps = logoxide.versifications().withMap(map).withVrs("Vulgate", "Original", "PSA 50:1-21 = PSA 51:1-21");
console.log(maps.names()); // → [ 'English', 'Original', 'Vulgate' ]
const mapped = maps.map("Psalm 51:1-2", "English", "Original", null);
console.log(mapped.text, mapped.osis, mapped.verses[0]); // → Psalm 51:3-4 Ps.51.3-Ps.51.4 { book: 19, chapter: 51, verse: 3 }
console.log(maps.map("Psalm 51:1", "English", "Vulgate", kjv.chapterVerses(19)).text); // → Psalm 50:3
const missing = maps.map("Malachi 4:1", "Original", "English", null);
console.log(missing.verses.length, missing.missingText, missing.missing); // → 0 Malachi 4:1 [ { book: 39, chapter: 4, verse: 1 } ]
```

```python
mapping = logoxide.open_package([
    file("logoxide.toml", """[package]
id = "english-original"
name = "English to Original"
version = "1.0.0"
kind = "versification-map"
language = "en"
license = "CC0-1.0"
versification = "English"
maps-to = "Original"

[formats]
vrs = { path = "eng-org.vrs" }
"""),
    file("eng-org.vrs", "PSA 51:0 = PSA 51:1\nPSA 51:0 = PSA 51:2\nPSA 51:1-19 = PSA 51:3-21\nMAL 4:1-6 = MAL 3:19-24\n"),
], features, [])
print(mapping.info().kind == PackageKind.VERSIFICATION_MAP, mapping.info().maps_to)  # → True Original
maps = logoxide.versifications().with_map(mapping).with_vrs("Vulgate", "Original", "PSA 50:1-21 = PSA 51:1-21")
print(maps.names())  # → ['English', 'Original', 'Vulgate']
mapped = maps.map("Psalm 51:1-2", "English", "Original", None)
print(mapped.text, mapped.osis, mapped.verses[0])  # → Psalm 51:3-4 Ps.51.3-Ps.51.4 MappedVerse(book=19, chapter=51, verse=3)
print(maps.map("Psalm 51:1", "English", "Vulgate", kjv.chapter_verses(19)).text)  # → Psalm 50:3
missing = maps.map("Malachi 4:1", "Original", "English", None)
print(len(missing.verses), missing.missing_text, missing.missing)  # → 0 Malachi 4:1 [MappedVerse(book=39, chapter=4, verse=1)]
```

## A vault's project

A vault's `logoxide.toml` and `logoxide.lock` say which packages it uses (`parseProject`, either
`null` for a new vault). Changing a project gives a new one, its `manifestText()` (comments
kept, dependencies in the order added) to save, and installing it gives the lock file;
`dependencies()` are in name order. A dependency is `git` (with at most one of `version`, `tag`,
`branch`, or `rev`), `registry`, `path`, or `_import`.

```ts
import { type Dependency } from "logoxide";

const dependency: Dependency = {
  git: null, registry: null, path: null, _import: null, version: null, tag: null, branch: null, rev: null,
  features: [], defaultFeatures: true, packageId: null, abbreviation: null, aliases: null,
};
let project = logoxide.parseProject(`# My vault
[dependencies]
kjv = { git = "https://example.org/kjv", version = "1" }

[settings]
default = "kjv"
compare = ["bsb"]
catalogs = ["https://example.org/catalog.toml"]
`, null);
project = project.addDependency("bsb", { ...dependency, git: "https://example.org/bsb", version: "1" });
console.log(project.dependencies(), project.unlocked(), project.packages()); // → [ 'bsb', 'kjv' ] [ 'bsb', 'kjv' ] []
console.log(project.defaultBible(), project.compare()); // → kjv [ 'bsb' ]
console.log(project.catalogs().length, project.catalogs()[0], project.isDefaultCatalog(project.catalogs()[1])); // → 6 https://example.org/catalog.toml true
console.log(project.removeCatalog(logoxide.ebibleCatalogUrl()).catalogs().length); // → 5
console.log(project.addCatalog("https://example.org/more.toml").catalogs().length); // → 7
const named = project.addRegistry("example", "https://example.org/catalog.toml");
console.log(named.registries(), named.removeRegistry("example").registries()); // → [ { name: 'example', location: 'https://example.org/catalog.toml' } ] []
console.log(project.removeDependency("bsb").dependencies()); // → [ 'kjv' ]
mkdirSync("vault", { recursive: true });
writeFileSync("vault/logoxide.toml", project.manifestText());
console.log(project.manifestText());
// → # My vault
// → [dependencies]
// → kjv = { git = "https://example.org/kjv", version = "1" }
// → bsb = { git = "https://example.org/bsb", version = "1" }
// → [settings]
// → default = "kjv"
// → compare = ["bsb"]
// → catalogs = ["https://example.org/catalog.toml"]
```

```python
from logoxide import Dependency

dependency = Dependency(
    git=None, registry=None, path=None, import_=None, version=None, tag=None, branch=None, rev=None,
    features=[], default_features=True, package_id=None, abbreviation=None, aliases=None,
)
project = logoxide.parse_project("""# My vault
[dependencies]
kjv = { git = "https://example.org/kjv", version = "1" }

[settings]
default = "kjv"
compare = ["bsb"]
catalogs = ["https://example.org/catalog.toml"]
""", None)
project = project.add_dependency("bsb", dataclasses.replace(dependency, git="https://example.org/bsb", version="1"))
print(project.dependencies(), project.unlocked(), project.packages())  # → ['bsb', 'kjv'] ['bsb', 'kjv'] []
print(project.default_bible(), project.compare())  # → kjv ['bsb']
print(len(project.catalogs()), project.catalogs()[0], project.is_default_catalog(project.catalogs()[1]))  # → 6 https://example.org/catalog.toml True
print(len(project.remove_catalog(logoxide.ebible_catalog_url()).catalogs()))  # → 5
print(len(project.add_catalog("https://example.org/more.toml").catalogs()))  # → 7
named = project.add_registry("example", "https://example.org/catalog.toml")
print(named.registries(), named.remove_registry("example").registries())  # → [Registry(name='example', location='https://example.org/catalog.toml')] []
print(project.remove_dependency("bsb").dependencies())  # → ['kjv']
pathlib.Path("vault").mkdir(exist_ok=True)
pathlib.Path("vault/logoxide.toml").write_text(project.manifest_text())
print(project.manifest_text())
# → # My vault
# → [dependencies]
# → kjv = { git = "https://example.org/kjv", version = "1" }
# → bsb = { git = "https://example.org/bsb", version = "1" }
# → [settings]
# → default = "kjv"
# → compare = ["bsb"]
# → catalogs = ["https://example.org/catalog.toml"]
```

## Catalogs

A catalog lists packages to find by name or language: a `catalog.toml` anyone hosts, or a
publisher's own list (eBible.org's `translations.csv`, fetch.bible's `manifest.json`, CCEL's page,
a SWORD repository's `mods.d.tar.gz`, which is read as bytes). `parseCatalog` reads one's text and
`parseCatalogBytes` its bytes, by its location; catalogs `merge`, so one search covers them all.
Each entry's `source` says how to get it: a `Git` repository, a `Registry` of release archives
(its `releases`), or an `External` publisher's download to import.

```ts
import { type LogoxideCatalog } from "logoxide";

const examples = logoxide.parseCatalog("https://example.org/catalog.toml", new TextDecoder().decode(await get("https://example.org/catalog.toml")));
const ebible = logoxide.parseCatalogBytes(logoxide.ebibleCatalogUrl(), await get(logoxide.ebibleCatalogUrl()));
console.log(examples.name(), examples.count(), examples.warnings(), ebible.name(), ebible.count()); // → Example Bibles 3 [] eBible.org 2
const both: LogoxideCatalog = examples.merge(ebible);
for (const found of both.search("english bible", 0)) console.log(found.id, found.abbreviation, found.source.tag);
// → ebible-eng-asv ASV External
// → ebible-engwebp WEBP External
const sermons = both.search("sermons", 1)[0];
console.log(sermons.kind === PackageKind.Book, sermons.source, sermons.releases[0].version, sermons.releases[0].size); // → true { tag: 'Registry', catalog: 'https://example.org/catalog.toml' } 1.0.0 2504n
console.log(both.searchGrouped("king james", 0).map((group) => [group.work, group.name, group.entries.length])); // → [ [ 'kjv', 'King James Version', 1 ] ]
const [kjvWork] = both.works("king james");
console.log(kjvWork.id, kjvWork.from, kjvWork.entries.map((e) => e.id)); // → kjv { tag: 'Logoxide' } [ 'kjv' ]
```

```python
examples = logoxide.parse_catalog("https://example.org/catalog.toml", get("https://example.org/catalog.toml").decode())
ebible = logoxide.parse_catalog_bytes(logoxide.ebible_catalog_url(), get(logoxide.ebible_catalog_url()))
print(examples.name(), examples.count(), examples.warnings(), ebible.name(), ebible.count())  # → Example Bibles 3 [] eBible.org 2
both = examples.merge(ebible)
for found in both.search("english bible", 0):
    print(found.id, found.abbreviation, type(found.source).__name__)
# → ebible-eng-asv ASV CatalogSourceExternal
# → ebible-engwebp WEBP CatalogSourceExternal
sermons = both.search("sermons", 1)[0]
print(sermons.kind == PackageKind.BOOK, sermons.source, sermons.releases[0].version, sermons.releases[0].size)  # → True CatalogSourceRegistry(catalog='https://example.org/catalog.toml') 1.0.0 2504
print([(group.work, group.name, len(group.entries)) for group in both.search_grouped("king james", 0)])  # → [('kjv', 'King James Version', 1)]
[kjv_work] = both.works("king james")
print(kjv_work.id, kjv_work.from_, [e.id for e in kjv_work.entries])  # → kjv WorkFromLogoxide() ['kjv']
```

An installer reads a project's catalogs (its `[settings] catalogs`, then the default ones) as
`lx search` does, asking for each as a `Download`. A download the host can't make is a warning
(`provideDownloadFailed`), unless none can be read; `catalog()` is then all of them merged. The
same `answer` serves installing, below. A catalog given by path isn't read there (a warning says
so): read it and merge `parseCatalogBytes`'s.

```ts
import { type FetchRequest, type LogoxideInstaller, type PackageLocation } from "logoxide";

/** Where a package's files are: in the library, or a path package's in the vault */
const folder = (location: PackageLocation) => join(location.tag === "Library" ? "library" : "vault", location.dir);

/** Answers what an installer asks for */
async function answer(installer: LogoxideInstaller, request: FetchRequest, catalog: LogoxideCatalog | null = null) {
  if (request.tag === "Refs") installer.provideRefs(request.repository, await get(request.url));
  else if (request.tag === "Archive") installer.provideArchive(request.repository, request.commit, await getFirst(request.urls));
  else if (request.tag === "Download") {
    try { installer.provideDownload(request.url, await getFirst(request.urls)); }
    catch (error) { installer.provideDownloadFailed(request.url, String(error)); }
  } else if (request.tag === "ImportEntry") {
    const entry = catalog?.findImport(request._import);
    if (!entry) throw new Error(`no catalog lists ${request._import}`);
    installer.provideImportEntry(entry);
  } else if (existsSync(folder(request.location))) installer.provideFiles(request.location, files(folder(request.location)));
  else installer.provideMissing(request.location);
}

const installer = logoxide.installer(); // keep one: what it's given is kept
let loading = installer.loadCatalogs(project);
while (loading.tag === "Fetch") {
  await answer(installer, loading.request);
  loading = installer.loadCatalogs(project);
}
console.log(loading.warnings.length, loading.warnings[0]); // → 4 …fetch.bible…404…
const catalog = installer.catalog();
console.log(catalog.count(), catalog.warnings().length, catalog.findImport("ebible:eng-asv")?.name); // → 5 4 American Standard Version (1901)
```

```python
from logoxide import (
    FetchRequestArchive, FetchRequestDownload, FetchRequestFiles, FetchRequestImportEntry,
    FetchRequestRefs, LogoxideCatalog, LogoxideInstaller, PackageLocation, PackageLocationLibrary,
    CatalogsStepFetch,
)


def folder(location: PackageLocation) -> str:
    """Where a package's files are: in the library, or a path package's in the vault"""
    return str(pathlib.Path("library" if isinstance(location, PackageLocationLibrary) else "vault", location.dir))


def answer(installer: LogoxideInstaller, request, catalog: LogoxideCatalog | None = None) -> None:
    """Answers what an installer asks for"""
    match request:
        case FetchRequestRefs(repository=repository, url=url):
            installer.provide_refs(repository, get(url))
        case FetchRequestArchive(repository=repository, commit=commit, urls=urls):
            installer.provide_archive(repository, commit, get_first(urls))
        case FetchRequestDownload(url=url, urls=urls):
            try:
                installer.provide_download(url, get_first(urls))
            except OSError as error:
                installer.provide_download_failed(url, str(error))
        case FetchRequestImportEntry(import_=name):
            entry = catalog.find_import(name) if catalog else None
            if entry is None:
                raise LookupError(f"no catalog lists {name}")
            installer.provide_import_entry(entry)
        case FetchRequestFiles(location=location):
            if pathlib.Path(folder(location)).is_dir():
                installer.provide_files(location, files(folder(location)))
            else:
                installer.provide_missing(location)


installer = logoxide.installer()  # keep one: what it's given is kept
loading = installer.load_catalogs(project)
while isinstance(loading, CatalogsStepFetch):
    answer(installer, loading.request)
    loading = installer.load_catalogs(project)
print(len(loading.warnings), loading.warnings[0])  # → 4 …fetch.bible…404…
catalog = installer.catalog()
print(catalog.count(), len(catalog.warnings()), catalog.find_import("ebible:eng-asv").name)  # → 5 4 American Standard Version (1901)
```

What logoxide knows of catalogs, imports, and categories: the default catalogs, eBible.org's
list, whether a URL is fetched without encryption, a SWORD repository CrossWire lists, the
publishers imports come from, and the categories a package can be in (at most
`maxCategories()`).

```ts
console.log(logoxide.defaultCatalogUrls().length, logoxide.ebibleCatalogUrl()); // → 5 https://ebible.org/Scriptures/translations.csv
console.log(logoxide.isInsecureUrl("http://ftp.xiphos.org/"), logoxide.isInsecureUrl("https://example.org/kjv")); // → true false
console.log(logoxide.swordCatalogUrl("crosswire"), logoxide.swordCatalogUrl("nowhere")); // → https://crosswire.org/ftpmirror/pub/sword/raw/mods.d.tar.gz null
console.log(logoxide.importPrefixes()); // → [ 'ebible', 'fetch', 'ccel', 'sword', 'door43' ]
console.log(logoxide.categories()[0], logoxide.maxCategories()); // → { name: 'biblical-theology', description: 'Themes traced through Scripture (covenant, kingdom)' } 5
```

```python
print(len(logoxide.default_catalog_urls()), logoxide.ebible_catalog_url())  # → 5 https://ebible.org/Scriptures/translations.csv
print(logoxide.is_insecure_url("http://ftp.xiphos.org/"), logoxide.is_insecure_url("https://example.org/kjv"))  # → True False
print(logoxide.sword_catalog_url("crosswire"), logoxide.sword_catalog_url("nowhere"))  # → https://crosswire.org/ftpmirror/pub/sword/raw/mods.d.tar.gz None
print(logoxide.import_prefixes())  # → ['ebible', 'fetch', 'ccel', 'sword', 'door43']
print(logoxide.categories()[0], logoxide.max_categories())  # → Category(name='biblical-theology', description='Themes traced through Scripture (covenant, kingdom)') 5
```

## Installing

There's no `git` in WebAssembly, so the host downloads over HTTP and logoxide asks for what it
needs: a repository's refs (git's smart HTTP, which GitHub, GitLab, Gitea, Forgejo, and Bitbucket
serve), an archive of a commit (`.tar.gz`, from the first of `urls` that answers: the forms hosts
use), a registry's downloads, or the files of a package already installed. It resolves exactly as
`lx install` does, so a vault's `logoxide.lock` works with `lx` and back. `inspect` reads a
repository's newest release (or a `tag`, `branch`, or `rev`) before it's added, to name it by its
id.

```ts
let inspecting = installer.inspect("https://example.org/bsb", null, null, null);
while (inspecting.tag === "Fetch") {
  await answer(installer, inspecting.request);
  inspecting = installer.inspect("https://example.org/bsb", null, null, null);
}
const { info: found, tag, commit } = inspecting.inspected;
console.log(found.id, found.version, tag, commit.length); // → bsb 1.0.3 v1.0.3 40
```

```python
from logoxide import InspectStepFetch

inspecting = installer.inspect("https://example.org/bsb", None, None, None)
while isinstance(inspecting, InspectStepFetch):
    answer(installer, inspecting.request)
    inspecting = installer.inspect("https://example.org/bsb", None, None, None)
found, tag, commit = inspecting.inspected.info, inspecting.inspected.tag, inspecting.inspected.commit
print(found.id, found.version, tag, len(commit))  # → bsb 1.0.3 v1.0.3 40
```

`install(project, update)` resolves the project's dependencies, keeping its lock file's versions
(`{ tag: "Nothing" }`), or moving all of them (`All`) or some (`Only`) to the newest their
requirements allow, and says what it needs until it's done: then write `files` into the library
and `lock` as the vault's `logoxide.lock`. A book from a registry is added by the registry's
catalog.

```ts
import { type InstallStep, type LogoxideProject, type UpdateMode } from "logoxide";

/** Installs a project, answering what's asked; what was asked, by kind */
async function install(installer: LogoxideInstaller, project: LogoxideProject, update: UpdateMode, catalog: LogoxideCatalog | null = null) {
  const asked: string[] = [];
  let step: InstallStep = installer.install(project, update);
  while (step.tag === "Fetch") {
    asked.push(step.request.tag);
    await answer(installer, step.request, catalog);
    step = installer.install(project, update);
  }
  write("library", step.files);
  writeFileSync("vault/logoxide.toml", project.manifestText());
  writeFileSync("vault/logoxide.lock", step.lock);
  return asked;
}

project = project.addDependency("sermons-on-love", { ...dependency, registry: "https://example.org/catalog.toml", version: "1" });
console.log(await install(installer, project, { tag: "Nothing" })); // → [ 'Files', 'Refs', 'Archive', 'Files', 'Files', 'Download' ]
project = logoxide.parseProject(readFileSync("vault/logoxide.toml", "utf8"), readFileSync("vault/logoxide.lock", "utf8"));
console.log(await install(logoxide.installer(), project, { tag: "All" })); // → [ 'Refs', 'Archive', 'Files', 'Refs', 'Archive', 'Files', 'Download', 'Files' ]
console.log(await install(logoxide.installer(), project, { tag: "Only", ids: ["bsb"] })); // → [ 'Refs', 'Archive', 'Files', 'Files', 'Files' ]
```

```python
from logoxide import InstallStepFetch, LogoxideProject, UpdateMode, UpdateModeAll, UpdateModeNothing, UpdateModeOnly


def install(installer: LogoxideInstaller, project: LogoxideProject, update: UpdateMode, catalog: LogoxideCatalog | None = None) -> list[str]:
    """Installs a project, answering what's asked; what was asked, by kind"""
    asked = []
    step = installer.install(project, update)
    while isinstance(step, InstallStepFetch):
        asked.append(type(step.request).__name__.removeprefix("FetchRequest"))
        answer(installer, step.request, catalog)
        step = installer.install(project, update)
    write("library", step.files)
    pathlib.Path("vault/logoxide.toml").write_text(project.manifest_text())
    pathlib.Path("vault/logoxide.lock").write_text(step.lock)
    return asked


project = project.add_dependency("sermons-on-love", dataclasses.replace(dependency, registry="https://example.org/catalog.toml", version="1"))
print(install(installer, project, UpdateModeNothing()))  # → ['Files', 'Refs', 'Archive', 'Files', 'Files', 'Download']
project = logoxide.parse_project(pathlib.Path("vault/logoxide.toml").read_text(), pathlib.Path("vault/logoxide.lock").read_text())
print(install(logoxide.installer(), project, UpdateModeAll()))  # → ['Refs', 'Archive', 'Files', 'Refs', 'Archive', 'Files', 'Download', 'Files']
print(install(logoxide.installer(), project, UpdateModeOnly(ids=["bsb"])))  # → ['Refs', 'Archive', 'Files', 'Files', 'Files']
```

A package is what its manifest declares, so files a reader adds beside it (annotations) don't
change its checksum, but changing its own files does: installed versions are checked against the
lock file's checksum, and `{ tag: "Repair" }` hands back the files of any whose own files changed,
to write over them.

```ts
const kjvEntry = project.packages().find((e) => e.name === "kjv")!;
writeFileSync(join(folder(kjvEntry.location), "usfm/jhn.usfm"), "changed");
try {
  await install(logoxide.installer(), project, { tag: "Nothing" });
} catch (error) {
  console.log((error as LogoxideErrorException).value.tag); // → Integrity
}
console.log(await install(logoxide.installer(), project, { tag: "Repair" })); // → [ 'Files', 'Files', 'Archive', 'Files' ]
console.log(logoxide.openPackage(files(folder(kjvEntry.location)), kjvEntry.features, cache).checksum() === kjvEntry.checksum); // → true
```

```python
from logoxide import UpdateModeRepair

kjv_entry = next(e for e in project.packages() if e.name == "kjv")
pathlib.Path(folder(kjv_entry.location), "usfm/jhn.usfm").write_text("changed")
try:
    install(logoxide.installer(), project, UpdateModeNothing())
except LogoxideErrorException as error:
    print(type(error.error).__name__)  # → LogoxideErrorIntegrity
print(install(logoxide.installer(), project, UpdateModeRepair()))  # → ['Files', 'Files', 'Archive', 'Files']
print(logoxide.open_package(files(folder(kjv_entry.location)), kjv_entry.features, cache).checksum() == kjv_entry.checksum)  # → True
```

## The vault's packages

`packages()` lists each package the manifest asks for, as the lock file has it, followed by what
it brings in (`dependencyOf`), with where its files are: in the library, or a path package's
folder in the vault. Open each from there with its `features`. A dependency turned off
(`setEnabled`) stays locked and installed, and is `enabled: false` for hosts to leave out.

```ts
for (const entry of project.packages()) console.log(entry.name, entry.version, entry.location, entry.git, entry.enabled);
// → bsb 1.0.3 { tag: 'Library', dir: 'packages/bsb/1.0.3+g…' } https://example.org/bsb true
// → kjv 1.0.3 { tag: 'Library', dir: 'packages/kjv/1.0.3+g…' } https://example.org/kjv true
// → sermons-on-love 1.0.0 { tag: 'Library', dir: 'packages/sermons-on-love/1.0.0+a…' } null true
const off = project.setEnabled("bsb", false);
console.log(off.isEnabled("bsb"), off.enabledDependencies(), off.disabledDependencies(), off.packages()[0].enabled); // → false [ 'kjv', 'sermons-on-love' ] [ 'bsb' ] false
```

```python
for entry in project.packages():
    print(entry.name, entry.version, entry.location, entry.git, entry.enabled)
# → bsb 1.0.3 PackageLocationLibrary(dir='packages/bsb/1.0.3+g…') https://example.org/bsb True
# → kjv 1.0.3 PackageLocationLibrary(dir='packages/kjv/1.0.3+g…') https://example.org/kjv True
# → sermons-on-love 1.0.0 PackageLocationLibrary(dir='packages/sermons-on-love/1.0.0+a…') None True
off = project.set_enabled("bsb", False)
print(off.is_enabled("bsb"), off.enabled_dependencies(), off.disabled_dependencies(), off.packages()[0].enabled)  # → False ['kjv', 'sermons-on-love'] ['bsb'] False
```

The lock file doesn't say what a package is, so `commentaries`, `dictionaries`, and
`crossReferences` take each entry's `logoxide.toml` from the host (`PackageManifest`, by the
entry's name) and give those in use of that kind. `nameClashes` says which names two or more of
the Bibles a host shows answer to, so a reference naming one can't say which.

```ts
write("vault/notes", commentaryFiles);
project = project.addDependency("notes", { ...dependency, path: "notes" });
await install(installer, project, { tag: "Nothing" });
project = logoxide.parseProject(readFileSync("vault/logoxide.toml", "utf8"), readFileSync("vault/logoxide.lock", "utf8"));
const manifests = project.packages().map((entry) => ({ name: entry.name, manifest: readFileSync(join(folder(entry.location), "logoxide.toml"), "utf8") }));
console.log(project.commentaries(manifests).map((e) => [e.name, e.location]), project.dictionaries(manifests), project.crossReferences(manifests)); // → [ [ 'notes', { tag: 'Path', dir: 'notes' } ] ] [] []
console.log(project.nameClashes([
  { label: "kjv 1.0.3", abbreviation: "KJV", aliases: ["AV"] },
  { label: "my-av", abbreviation: "AV", aliases: [] },
])); // → [ { name: 'AV', packages: [ 'kjv 1.0.3', 'my-av' ] } ]
```

```python
from logoxide import NamedBible, PackageManifest

write("vault/notes", commentary_files)
project = project.add_dependency("notes", dataclasses.replace(dependency, path="notes"))
install(installer, project, UpdateModeNothing())
project = logoxide.parse_project(pathlib.Path("vault/logoxide.toml").read_text(), pathlib.Path("vault/logoxide.lock").read_text())
manifests = [PackageManifest(name=e.name, manifest=pathlib.Path(folder(e.location), "logoxide.toml").read_text()) for e in project.packages()]
print([(e.name, e.location) for e in project.commentaries(manifests)], project.dictionaries(manifests), project.cross_references(manifests))  # → [('notes', PackageLocationPath(dir='notes'))] [] []
print(project.name_clashes([
    NamedBible(label="kjv 1.0.3", abbreviation="KJV", aliases=["AV"]),
    NamedBible(label="my-av", abbreviation="AV", aliases=[]),
]))  # → [NameClash(name='AV', packages=['kjv 1.0.3', 'my-av'])]
```

## Imports

Another publisher's download is made into a package the same way for every publisher:
`importDownloads(entry)` says what to fetch (each from the first of its `urls` that answers),
and `importDownloaded(entry, files)` makes the package's files from their bytes, by name. Write
them into a folder in the vault and add it as a path package, or add `{ _import }` and let
installing make it (kept in the library by its checksum, as `lx import` does).

```ts
const asv = catalog.findImport("ebible:eng-asv")!;
if (asv.source.tag !== "External") throw new Error("not an import");
console.log(asv.source._import, asv.source.tracksVersions, asv.version); // → ebible:eng-asv true 2026-10-08
const downloads = logoxide.importDownloads(asv);
console.log(downloads); // → [ { name: 'zip', urls: [ 'https://ebible.org/Scriptures/eng-asv_usfm.zip' ] } ]
const fetched: PackageFile[] = [];
for (const download of downloads) fetched.push({ path: download.name, bytes: await getFirst(download.urls) });
const imported = logoxide.importDownloaded(asv, fetched);
console.log(imported.map((f) => f.path)); // → [ 'copr.htm', 'logoxide.toml', 'usfm/00-FRTeng-asv.usfm', 'usfm/73-JHNeng-asv.usfm', 'usfm/95-3JNeng-asv.usfm' ]
const asvPackage = logoxide.openPackage(imported, features, []);
console.log(asvPackage.info().abbreviation, [...asvPackage.books()], asvPackage.skipped()[0].reason); // → ASV [ 43, 64 ] peripheral
```

```python
from logoxide import CatalogSourceExternal

asv = catalog.find_import("ebible:eng-asv")
assert isinstance(asv.source, CatalogSourceExternal)
print(asv.source.import_, asv.source.tracks_versions, asv.version)  # → ebible:eng-asv True 2026-10-08
downloads = logoxide.import_downloads(asv)
print(downloads)  # → [ImportDownload(name='zip', urls=['https://ebible.org/Scriptures/eng-asv_usfm.zip'])]
fetched = [PackageFile(path=d.name, bytes=get_first(d.urls)) for d in downloads]
imported = logoxide.import_downloaded(asv, fetched)
print([f.path for f in imported])  # → ['copr.htm', 'logoxide.toml', 'usfm/00-FRTeng-asv.usfm', 'usfm/73-JHNeng-asv.usfm', 'usfm/95-3JNeng-asv.usfm']
asv_package = logoxide.open_package(imported, features, [])
print(asv_package.info().abbreviation, list(asv_package.books()), asv_package.skipped()[0].reason)  # → ASV [43, 64] peripheral
```

Updating an import: `findUpstream` finds its entry by `info().upstream`; `importIsCurrent` says
whether the publisher's version there is the one imported (eBible.org's and SWORD's versions
follow their updates; fetch.bible's and CCEL's don't, so those are always `false`); else download
it again and `updateImport(current, fresh)` gives what changed (`null` for nothing), the version
raised by what changed, the files to write, and those to delete.

```ts
import { ChangeKind } from "logoxide";

const upstream = asvPackage.info().upstream!;
console.log(upstream._import, upstream.version, catalog.findUpstream(upstream)?.id, logoxide.importIsCurrent(asv, upstream.version)); // → ebible:eng-asv 2026-10-08 ebible-eng-asv true
console.log(logoxide.updateImport(imported, imported)); // → null
// What a new download makes: here, John 3:16 reworded and 3 John dropped
const fresh = imported
  .filter((f) => !f.path.includes("3JN"))
  .map((f) => (f.path.includes("JHN") ? file(f.path, new TextDecoder().decode(f.bytes).replace("so loved", "so much loved")) : f));
const update = logoxide.updateImport(imported, fresh)!;
console.log(update.from, update.to, update.change === ChangeKind.Major, update.description, update.removed); // → 1.0.0 2.0.0 true 1 verse removed, 1 verse reworded [ 'usfm/95-3JNeng-asv.usfm' ]
```

```python
from logoxide import ChangeKind

upstream = asv_package.info().upstream
print(upstream.import_, upstream.version, catalog.find_upstream(upstream).id, logoxide.import_is_current(asv, upstream.version))  # → ebible:eng-asv 2026-10-08 ebible-eng-asv True
print(logoxide.update_import(imported, imported))  # → None
# What a new download makes: here, John 3:16 reworded and 3 John dropped
fresh = [
    file(f.path, f.bytes.decode().replace("so loved", "so much loved")) if "JHN" in f.path else f
    for f in imported
    if "3JN" not in f.path
]
update = logoxide.update_import(imported, fresh)
print(update.from_, update.to, update.change == ChangeKind.MAJOR, update.description, update.removed)  # → 1.0.0 2.0.0 True 1 verse removed, 1 verse reworded ['usfm/95-3JNeng-asv.usfm']
```

An `import` dependency is made as it installs: the installer asks for its catalog entry
(`ImportEntry`, answered from the catalogs with `provideImportEntry`) unless `loadCatalogs` listed
it, then for its downloads. Named other than its id (`ebible-eng-asv`), a dependency says which
package it is with `packageId`.

```ts
project = project.addDependency("asv", { ...dependency, _import: "ebible:eng-asv", packageId: "ebible-eng-asv" });
console.log(await install(logoxide.installer(), project, { tag: "Nothing" }, catalog)); // → [ 'ImportEntry', 'Download', 'Files', 'Files', 'Files', 'Files', 'Files' ]
project = logoxide.parseProject(readFileSync("vault/logoxide.toml", "utf8"), readFileSync("vault/logoxide.lock", "utf8"));
const asvEntry = project.packages().find((e) => e.name === "asv")!;
console.log(asvEntry._import, asvEntry.version, asvEntry.location.dir); // → ebible:eng-asv 1.0.0 packages/ebible-eng-asv/1.0.0+i…
```

```python
project = project.add_dependency("asv", dataclasses.replace(dependency, import_="ebible:eng-asv", package_id="ebible-eng-asv"))
print(install(logoxide.installer(), project, UpdateModeNothing(), catalog))  # → ['ImportEntry', 'Download', 'Files', 'Files', 'Files', 'Files', 'Files']
project = logoxide.parse_project(pathlib.Path("vault/logoxide.toml").read_text(), pathlib.Path("vault/logoxide.lock").read_text())
asv_entry = next(e for e in project.packages() if e.name == "asv")
print(asv_entry.import_, asv_entry.version, asv_entry.location.dir)  # → ebible:eng-asv 1.0.0 packages/ebible-eng-asv/1.0.0+i…
```

## Making packages

A package can be made from one file the person picks: an EPUB fills in its own title, subtitle,
authors, language, and publisher (`fileMetadata`, to fill a form); a Bible's USX or USFM needs a
name and a language. What's given wins over the file; the id comes from the name, a Bible's or
commentary's abbreviation from its initials (the other kinds have one only when given), and the
version is 0.1.0 unless given. logoxide gives back the package's files to write into a new folder
(then a git repository, to release it). `licenseConcerns` says what to know before
redistributing one (shown, never enforced).

```ts
import { ConcernKind, type NewPackage } from "logoxide";

const epub = new Uint8Array(readFileSync("sermons.epub"));
console.log(logoxide.fileMetadata("sermons.epub", epub)); // → { title: 'Sermons on Love', subtitle: 'Two Short Sermons', authors: [ 'A. Preacher' ], language: 'en', publisher: 'Example Press', rights: 'Public domain' }
const given: NewPackage = {
  id: null, name: null, abbreviation: null, authors: [], description: null, version: "1.0.0", kind: null,
  language: null, license: null, publisher: null, homepage: null, repository: null, keywords: [],
  categories: [], aboutBooks: ["1 John"], // a book's code or name
};
const made = logoxide.newPackage("sermons.epub", epub, given);
console.log(made.map((f) => f.path)); // → [ 'epub/sermons.epub', 'logoxide.toml' ]
const book = logoxide.openPackage(made, features, []);
console.log(book.info().id, book.info().kind === PackageKind.Book, book.info().aboutBooks, book.epubFiles()); // → sermons-on-love true [ '1JN' ] [ 'epub/sermons.epub' ]
const [concern] = book.licenseConcerns();
console.log(concern.kind === ConcernKind.NoLicense, concern.serious); // → true false
```

```python
from logoxide import ConcernKind, NewPackage

epub = pathlib.Path("sermons.epub").read_bytes()
print(logoxide.file_metadata("sermons.epub", epub))  # → FileMetadata(title='Sermons on Love', subtitle='Two Short Sermons', authors=['A. Preacher'], language='en', publisher='Example Press', rights='Public domain')
given = NewPackage(
    id=None, name=None, abbreviation=None, authors=[], description=None, version="1.0.0", kind=None,
    language=None, license=None, publisher=None, homepage=None, repository=None, keywords=[],
    categories=[], about_books=["1 John"],  # a book's code or name
)
made = logoxide.new_package("sermons.epub", epub, given)
print([f.path for f in made])  # → ['epub/sermons.epub', 'logoxide.toml']
book = logoxide.open_package(made, features, [])
print(book.info().id, book.info().kind == PackageKind.BOOK, book.info().about_books, book.epub_files())  # → sermons-on-love True ['1JN'] ['epub/sermons.epub']
[concern] = book.license_concerns()
print(concern.kind == ConcernKind.NO_LICENSE, concern.serious)  # → True False
```

A book has no verse text: `references()` reads its EPUBs for the Bible references in them, each
a `passage` in OSIS (`topos.parse` reads it), the `file`, its `chapter`, a `cfi` EPUB readers
open, and its UTF-16 range, line, column, spine item, and paragraph (what `ToposIndex.setEpub`
takes). `parseReferences` reads a release's published `logoxide.references.bin` without its
books: the passages cited, and how often.

```ts
const [first] = book.references();
console.log(first.passage, first.chapter, first.cfi, first.line, first.column, first.spineIndex); // → John.3.16 Love epubcfi(/6/2!/4/4,/1:28,/1:37) 3 29 0
console.log(topos.parse(first.passage, BookStyle.Name)?.reference); // → John 3:16
console.log(logoxide.parseReferences(new Uint8Array(readFileSync("logoxide.references.bin")))); // → [ { passage: 'John.3.16', count: 2 }, … ]
```

```python
first = book.references()[0]
print(first.passage, first.chapter, first.cfi, first.line, first.column, first.spine_index)  # → John.3.16 Love epubcfi(/6/2!/4/4,/1:28,/1:37) 3 29 0
print(topos.parse(first.passage, BookStyle.NAME).reference)  # → John 3:16
print(logoxide.parse_references(pathlib.Path("logoxide.references.bin").read_bytes()))  # → [CitedPassage(passage='John.3.16', count=2), …]
```

`unzip` gives a `.zip`'s files (folders left out), so a zip of USFM or USX books someone picks is
read without a zip library: pass a book's bytes to `newPackage`. Bytes that aren't a zip, or more
than 200,000 entries or 1 GiB, are an error. A collection is a package of packages, its manifest
alone (`newCollection`); add its packages with `addDependency` on it.

```ts
const zipped = logoxide.unzip(new Uint8Array(readFileSync("eng-asv_usfm.zip")));
console.log(zipped.map((f) => f.path)); // → [ '00-FRTeng-asv.usfm', '73-JHNeng-asv.usfm', '95-3JNeng-asv.usfm', 'copr.htm' ]
const gospel = zipped.find((f) => f.path.includes("JHN"))!;
const mine = logoxide.newPackage(gospel.path, gospel.bytes, { ...given, name: "My Gospel", language: "en", license: "CC0-1.0", aboutBooks: [] });
console.log(mine.map((f) => f.path), logoxide.openPackage(mine, features, []).info().abbreviation); // → [ 'logoxide.toml', 'usfm/73-JHNeng-asv.usfm' ] MG
const collection = logoxide.newCollection({ ...given, name: "My Library", aboutBooks: [] });
console.log(new TextDecoder().decode(collection[0].bytes).includes('kind = "collection"')); // → true
```

```python
zipped = logoxide.unzip(pathlib.Path("eng-asv_usfm.zip").read_bytes())
print([f.path for f in zipped])  # → ['00-FRTeng-asv.usfm', '73-JHNeng-asv.usfm', '95-3JNeng-asv.usfm', 'copr.htm']
gospel = next(f for f in zipped if "JHN" in f.path)
mine = logoxide.new_package(gospel.path, gospel.bytes, dataclasses.replace(given, name="My Gospel", language="en", license="CC0-1.0", about_books=[]))
print([f.path for f in mine], logoxide.open_package(mine, features, []).info().abbreviation)  # → ['logoxide.toml', 'usfm/73-JHNeng-asv.usfm'] MG
collection = logoxide.new_collection(dataclasses.replace(given, name="My Library", about_books=[]))
print('kind = "collection"' in collection[0].bytes.decode())  # → True
```

## Reading natively

In Python (and Swift and Kotlin), `readPassage` and `comparePassage` read a project's Bibles from
its folder and the library's, as `lx read` and `lx compare` do: a passage in the Bible the
reference names (else the project's first), and a passage in several Bibles, each in its own
numbering through the project's versification maps, with what of it one lacks. In WebAssembly
(the npm package, in Node too) they're an `Io` error: open each package and read its book instead.

```ts
const plain: BookTextStyle = { verseLabel: "[{verse}]", chapterLabel: null, notes: false, headings: false, templates: [], width: null, limit: null };
try {
  logoxide.readPassage("vault", "library", "cache", "John 3:16 KJV", plain);
} catch (error) {
  console.log((error as LogoxideErrorException).value); // → { tag: 'Io', message: 'no file access in WebAssembly: open each package with openPackage' }
}
try {
  logoxide.comparePassage("vault", "library", "cache", "John 3:16", ["KJV", "BSB", "ASV"], plain);
} catch (error) {
  console.log((error as LogoxideErrorException).value.tag); // → Io
}
```

```python
plain = BookTextStyle(verse_label="[{verse}]", chapter_label=None, notes=False, headings=False, templates=[], width=None, limit=None)
read = logoxide.read_passage("vault", "library", "cache", "John 3:16 KJV", plain)
print(read.bible, read.abbreviation, read.passage.reference, read.versification)  # → kjv KJV John 3:16 KJV
print(read.text.text)  # → [16] ¶ For God so loved the world, …
cut = dataclasses.replace(plain, limit=12)
for c in logoxide.compare_passage("vault", "library", "cache", "John 3:16", ["KJV", "BSB", "ASV"], cut):
    print(c.abbreviation, c.reference, c.text.text, c.mapped, repr(c.missing))
# → KJV John 3:16 [16] ¶ For … False ''
# → BSB John 3:16 [16] For God … False ''
# → ASV John 3:16 [16] For God … False ''
```

## Errors

Errors are thrown as `LogoxideErrorException` in TypeScript (`error.value` is `{ tag, message }`)
and raised as `LogoxideErrorException` in Python (`error.error` is a `LogoxideErrorNotFound`,
`LogoxideErrorIo`, and so on, with its `message`). The tag is the core's kind: `NotFound`, `Io`,
`Invalid`, `InvalidMarkup`, `Network`, `Git`, `Archive`, `Integrity`, `Resolve`, `Busy`, and
`Conflict`.

```ts
try {
  logoxide.openPackage([file("notes.md", "")], features, []);
} catch (error) {
  if (!(error instanceof LogoxideErrorException)) throw error;
  console.log(error.value); // → { tag: 'NotFound', message: 'logoxide.toml: not found' }
}
```

```python
from logoxide import LogoxideErrorNotFound

try:
    logoxide.open_package([file("notes.md", "")], features, [])
except LogoxideErrorException as error:
    print(isinstance(error.error, LogoxideErrorNotFound), error.error.message)  # → True logoxide.toml: not found
```

## Every export

Each of logoxide's classes, functions, records, and enums (by their TypeScript names), and where
an example uses it. The rest
(`Topos`, `ToposTranslations`, `ToposQuery`, `ToposFormat`, `ToposOptions`, `ToposIndex`,
`ToposFiles`, `Passage`, and their records and enums) are topos's: see [References, with
topos](#references-with-topos).

| Export | Example |
|---|---|
| `Logoxide.new`, `version` | [Loading](#loading) |
| `Logoxide.openPackage`, `openPackageDir`, `readPackageInfo` | [Opening a package](#opening-a-package) |
| `Logoxide.usfmToUsx` | [A book's text](#a-books-text) |
| `Logoxide.parseTimings`, `verifyMedia`, `mediaStorePath` | [Audio and video](#audio-and-video) |
| `Logoxide.versifications` | [Versification maps](#versification-maps) |
| `Logoxide.parseProject` | [A vault's project](#a-vaults-project) |
| `Logoxide.parseCatalog`, `parseCatalogBytes`, `installer`, `defaultCatalogUrls`, `ebibleCatalogUrl`, `isInsecureUrl`, `swordCatalogUrl`, `importPrefixes`, `categories`, `maxCategories` | [Catalogs](#catalogs) |
| `Logoxide.importDownloads`, `importDownloaded`, `importIsCurrent`, `updateImport` | [Imports](#imports) |
| `Logoxide.fileMetadata`, `newPackage`, `newCollection`, `parseReferences`, `unzip` | [Making packages](#making-packages) |
| `Logoxide.readPassage`, `comparePassage` | [Reading natively](#reading-natively) |
| `LogoxidePackage.info`, `checksum`, `format`, `books`, `chapterVerses`, `cacheWrites`, `bookFiles`, `titles`, `skipped`, `verseHashes` | [Opening a package](#opening-a-package) |
| `LogoxidePackage.addTo`, `addToAs` | [References, with topos](#references-with-topos) |
| `LogoxidePackage.book` | [A book's text](#a-books-text) |
| `LogoxidePackage.notes`, `entry`, `keys`, `crossReferences` | [Commentaries, dictionaries, and cross-references](#commentaries-dictionaries-and-cross-references) |
| `LogoxidePackage.media`, `mediaText`, `mediaVersification`, `timings` | [Audio and video](#audio-and-video) |
| `LogoxidePackage.licenseConcerns`, `references`, `epubFiles` | [Making packages](#making-packages) |
| `LogoxideBook.bookId`, `verses`, `chapters`, `chapterVerses`, `text`, `html`, `chapterHtml`, `styledText`, `nodes`, `verseNodes`, `parse`, `usxChapterVerses`, `findFragment` | [A book's text](#a-books-text) |
| `LogoxideVersifications.withMap`, `withVrs`, `names`, `map` | [Versification maps](#versification-maps) |
| `LogoxideProject.manifestText`, `dependencies`, `unlocked`, `defaultBible`, `compare`, `catalogs`, `isDefaultCatalog`, `addCatalog`, `removeCatalog`, `registries`, `addRegistry`, `removeRegistry`, `addDependency`, `removeDependency` | [A vault's project](#a-vaults-project) |
| `LogoxideProject.packages`, `setEnabled`, `isEnabled`, `enabledDependencies`, `disabledDependencies`, `commentaries`, `dictionaries`, `crossReferences`, `nameClashes` | [The vault's packages](#the-vaults-packages) |
| `LogoxideCatalog.name`, `count`, `warnings`, `merge`, `search`, `searchGrouped`, `works`, `findImport` | [Catalogs](#catalogs) |
| `LogoxideCatalog.findUpstream` | [Imports](#imports) |
| `LogoxideInstaller.loadCatalogs`, `catalog`, `provideDownload`, `provideDownloadFailed`, `provideRefs`, `provideArchive`, `provideImportEntry`, `provideFiles`, `provideMissing` | [Catalogs](#catalogs) (`answer`) |
| `LogoxideInstaller.inspect`, `install` | [Installing](#installing) |
| `PackageFile`, `PackageFeatures`, `PackageInfo`, `PackageKind`, `Rights`, `Derivatives`, `Upstream`, `BookFile`, `TitledFile`, `SkippedFile`, `VerseHashEntry` | [Opening a package](#opening-a-package), [Imports](#imports) (`SkippedFile`, `Upstream`) |
| `BookTextOptions`, `BookVerse`, `BookTextStyle`, `KindTemplate`, `StyledText`, `TextMark`, `NodeKind`, `BookNode`, `NodeAttribute`, `MilestoneKind`, `VerseNodes`, `TextRange` | [A book's text](#a-books-text) |
| `Note`, `Entry`, `CrossReference` | [Commentaries, dictionaries, and cross-references](#commentaries-dictionaries-and-cross-references) |
| `MediaFile`, `VerseTiming` | [Audio and video](#audio-and-video) |
| `MappedPassage`, `MappedVerse` | [Versification maps](#versification-maps) |
| `Dependency`, `Registry` | [A vault's project](#a-vaults-project) |
| `CatalogEntry`, `CatalogSource`, `CatalogRelease`, `ImportDownload`, `WorkGroup`, `WorkInfo`, `WorkFrom`, `Category`, `CatalogsStep`, `FetchRequest` | [Catalogs](#catalogs) |
| `InspectStep`, `Inspected`, `InstallStep`, `UpdateMode` | [Installing](#installing) |
| `LockedEntry`, `PackageLocation`, `PackageManifest`, `NamedBible`, `NameClash` | [The vault's packages](#the-vaults-packages) |
| `ImportUpdate`, `ChangeKind` | [Imports](#imports) |
| `FileMetadata`, `NewPackage`, `LicenseConcern`, `ConcernKind`, `FoundReference`, `CitedPassage` | [Making packages](#making-packages) |
| `ReadPassage`, `ComparedPassage` | [Reading natively](#reading-natively) |
| `LogoxideError`, `LogoxideErrorException` | [Errors](#errors) |

## Building

```sh
npm install && npm run pack:wasm   # dist/wasm/pkg (npm: logoxide)
npm run pack:python                # dist/python/wheelhouse (PyPI: logoxide)
```

The examples run against the KJV and BSB packages (git repositories with release tags; the
owner's are in `~/Development/logoxide-packages`, the default) and `examples/fixtures`: a made-up
two-chapter EPUB (`sermons.epub`) with its release archive and `logoxide.references.bin` (made by
`lx new` and `lx release`), a few ASV books in eBible.org's zip and the list naming them, and a
`catalog.toml` listing the KJV, the BSB, and the EPUB's release.

```sh
cd examples/node && npm install
node readme.ts ~/Development/logoxide-packages   # every TypeScript example above (--show prints what each printed)
node index.ts ~/Development/logoxide-packages/bsb ~/Development/logoxide-packages/kjv
node install.ts file://$HOME/Development/logoxide-packages/kjv file://$HOME/Development/logoxide-packages/bsb
node book.ts ../fixtures/sermons.epub
cd ../.. && pip install dist/python/wheelhouse/*.whl
python examples/python/readme.py ~/Development/logoxide-packages   # every Python example above
python examples/python/packages.py ~/Development/logoxide-packages/bsb ~/Development/logoxide-packages/kjv
```

topos's crate is a path dependency (`../topos`), as for the rest of logoxide.
