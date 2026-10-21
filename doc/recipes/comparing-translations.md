# Comparing translations

A passage in two or three translations side by side, verse by verse, saying what one lacks; and,
when two Bibles number their verses differently (Malachi 4 in English Bibles is Malachi 3:19-24
in the Hebrew, Joel 2:28 is Joel 3:1, a psalm's title is its verse 1), finding each verse's
counterpart through a versification map. The examples start from the helpers in
[Bibles in your app](../apps.md#setting-up) (`lx`, `topos`, `get`) and run in order, as one
program in each language.

- [Side by side](#side-by-side)
- [What one lacks](#what-one-lacks)
- [When the numbering differs](#when-the-numbering-differs)
- [In a vault](#in-a-vault)

## Side by side

Read each translation's book once, as each verse's text by its number (a bridge's text under
each verse it covers, as [Bibles in your app](../apps.md#from-a-book-to-verses) does), then walk
the passage's verses (`topos.verses`) and take each translation's text for each. A verse a
translation hasn't is `null`; a verse it has with no words (the WEB keeps Acts 8:37's number and
puts its words in a footnote) is an empty string. (fetch.bible calls its WEB `WEBC`.)

```ts
import { BookStyle } from "logoxide";

const FETCH_BIBLE = "https://v1.fetch.bible/manifest.json";
const fetchBible = lx.parseCatalogBytes(FETCH_BIBLE, await get([FETCH_BIBLE]));
const bibles = ["fetch:eng_kjv", "fetch:eng_bsb", "fetch:eng_web"].map((name) => fetchBible.findImport(name)!);

/** Each verse's text in a translation's book, by "chapter:verse" (none if it hasn't the book) */
async function versesOf(bible: (typeof bibles)[number], bookId: number): Promise<Map<string, string>> {
  const out = new Map<string, string>();
  const download = lx.bookDownloads(bible).find((b) => b.bookId === bookId);
  if (!download) return out;
  const book = LogoxideBook.parse(topos, "usx", new TextDecoder().decode(await get(download.urls)));
  for (const v of book?.verseTexts() ?? []) {
    const last = v.lastChapter === v.chapter ? v.last : v.verse;
    for (let n = v.verse; n <= last; n++) {
      const key = `${v.chapter}:${n}`;
      out.set(key, v.part > 1 && out.has(key) ? `${out.get(key)} ${v.text}` : v.text);
    }
  }
  return out;
}

/** A passage's verses, each with every translation's text (null where one hasn't the verse) */
async function sideBySide(reference: string) {
  const passage = topos.parse(reference, BookStyle.Name)!;
  const columns = await Promise.all(bibles.map((b) => versesOf(b, passage.bookId)));
  return topos.verses(passage).map(({ chapter, verse }) => ({
    verse: `${chapter}:${verse}`,
    texts: columns.map((c) => c.get(`${chapter}:${verse}`) ?? null),
  }));
}

for (const row of await sideBySide("Acts 8:36-38")) {
  console.log(row.verse, row.texts.map((t, i) => `${bibles[i].abbreviation}: ${t === null ? "—" : t.slice(0, 24)}`).join(" | "));
}
// → 8:36 KJV: And as they went on thei | BSB: As they traveled along t | WEBC: As they went on the way,
// → 8:37 KJV: And Philip said, If thou | BSB: — | WEBC:
// → 8:38 KJV: And he commanded the cha | BSB: And he gave orders to st | WEBC: He commanded the chariot
```

```python
from logoxide import BookStyle

FETCH_BIBLE = "https://v1.fetch.bible/manifest.json"
fetch_bible = lx.parse_catalog_bytes(FETCH_BIBLE, get([FETCH_BIBLE]))
bibles = [fetch_bible.find_import(name) for name in ["fetch:eng_kjv", "fetch:eng_bsb", "fetch:eng_web"]]


def verses_of(bible, book_id: int) -> dict[str, str]:
    """Each verse's text in a translation's book, by "chapter:verse" (none if it hasn't the book)"""
    out: dict[str, str] = {}
    download = next((b for b in lx.book_downloads(bible) if b.book_id == book_id), None)
    if download is None:
        return out
    book = LogoxideBook.parse(topos, "usx", get(download.urls).decode())
    for v in book.verse_texts():
        last = v.last if v.last_chapter == v.chapter else v.verse
        for n in range(v.verse, last + 1):
            key = f"{v.chapter}:{n}"
            out[key] = f"{out[key]} {v.text}" if v.part > 1 and key in out else v.text
    return out


def side_by_side(reference: str):
    """A passage's verses, each with every translation's text (None where one hasn't the verse)"""
    passage = topos.parse(reference, BookStyle.NAME)
    columns = [verses_of(b, passage.book_id) for b in bibles]
    return [(f"{cv.chapter}:{cv.verse}", [c.get(f"{cv.chapter}:{cv.verse}") for c in columns]) for cv in topos.verses(passage)]


for verse, texts in side_by_side("Acts 8:36-38"):
    print(verse, " | ".join(f"{b.abbreviation}: {'—' if t is None else t[:24]}" for b, t in zip(bibles, texts)))
# → 8:36 KJV: And as they went on thei | BSB: As they traveled along t | WEBC: As they went on the way,
# → 8:37 KJV: And Philip said, If thou | BSB: — | WEBC:
# → 8:38 KJV: And he commanded the cha | BSB: And he gave orders to st | WEBC: He commanded the chariot
```

In Rust, each book is read as in [Bibles in your app](../apps.md#a-book-at-a-time), and
`logoxide_text::passage::verse_texts` gives the same verses; topos-bible's `Passage::verses`
lists a passage's verses.

The three line up verse for verse because fetch.bible numbers every translation it serves as
English Bibles do, even its Hebrew Old Testament (`fetch:hbo_wlc`, whose Malachi has four
chapters). Bibles from elsewhere keep their own numbering: see
[When the numbering differs](#when-the-numbering-differs).

## What one lacks

Comparing whole books says which verses one translation leaves out (or prints without words) that
another has: in the Gospels, verses most modern translations put in footnotes.

```ts
const [kjvMatthew, bsbMatthew, webMatthew] = await Promise.all(bibles.map((b) => versesOf(b, 40)));
const lacks = (has: Map<string, string>, other: Map<string, string>) => [...other.keys()].filter((v) => !has.get(v));
console.log(lacks(bsbMatthew, kjvMatthew), lacks(webMatthew, kjvMatthew)); // → [ '17:21', '18:11', '23:14' ] []
```

```python
kjv_matthew, bsb_matthew, web_matthew = (verses_of(b, 40) for b in bibles)
lacks = lambda has, other: [v for v in other if not has.get(v)]
print(lacks(bsb_matthew, kjv_matthew), lacks(web_matthew, kjv_matthew))  # → ['17:21', '18:11', '23:14'] []
```

## When the numbering differs

Bibles published in another tradition's numbering keep it: eBible.org's Westminster Leningrad
Codex (`ebible:hebwlc`) numbers Malachi in three chapters, Joel in four, and a psalm's title as
its first verses, as the Hebrew does. A versification map says where each verse of one numbering
is in another. Paratext's `.vrs` files are the usual form: `eng.vrs` maps English Bibles'
numbering to the original languages' (`MAL 4:1-6 = MAL 3:19-24`); SIL's libpalaso publishes it
under the MIT license.

`versifications()` holds no maps; `withVrs(from, to, text)` adds one, and `map(reference, from,
to, counts)` gives where a passage numbered `from` is in `to`: its `verses` there, `text` and
`osis` to show and store them, and what has no counterpart (`missing`). `counts` are the book's
verse counts in `from`, which a whole chapter needs when `from` isn't English (`Malachi 3` in the
Hebrew has 24 verses, in topos's English counts 18): pass that Bible's `chapterVerses(book)`.
Verse 0 is a psalm's title, which English Bibles show as a heading, not a verse.

```ts
const ENG_VRS = "https://raw.githubusercontent.com/sillsdev/libpalaso/master/SIL.Scripture/Resources/eng.vrs.txt";
const maps = lx.versifications().withVrs("English", "Original", new TextDecoder().decode(await get([ENG_VRS])));
for (const reference of ["Malachi 4:1-2", "Joel 2:28", "Psalm 51:1"]) {
  console.log(reference, "→", maps.map(reference, "English", "Original", null).text);
}
// → Malachi 4:1-2 → Malachi 3:19-20
// → Joel 2:28 → Joel 3:1
// → Psalm 51:1 → Psalm 51:3
console.log(maps.map("Psalm 51:1-2", "Original", "English", null).text); // → Psalm 51:0

const ebible = lx.parseCatalogBytes(lx.ebibleCatalogUrl(), await get([lx.ebibleCatalogUrl()]));
const [wlcZip] = lx.importDownloads(ebible.findImport("ebible:hebwlc")!);
const wlc = lx.openBible([{ path: "hebwlc_usfm.zip", bytes: await get(wlcZip.urls) }]);
console.log([...wlc.chapterVerses(39)]); // → [ 14, 17, 24 ]
console.log(maps.map("Malachi 3", "Original", "English", wlc.chapterVerses(39)).text); // → Malachi 3:1-18; 4:1-6

// Malachi 4:1 in the WEB and the WLC, side by side
const wlcMalachi = wlc.book(39).verseTexts();
const there = maps.map("Malachi 4:1", "English", "Original", null).verses;
const webMalachi = await versesOf(bibles[2], 39);
console.log("WEB 4:1", webMalachi.get("4:1")!.slice(0, 30));
for (const v of there) console.log(`WLC ${v.chapter}:${v.verse}`, wlcMalachi.find((w) => w.chapter === v.chapter && w.verse === v.verse)!.text.slice(0, 30));
// → WEB 4:1 “For behold, the day comes, bu
// → WLC 3:19 כִּֽי־הִנֵּ֤ה הַיּוֹם֙ בָּ֔א ב
```

```python
ENG_VRS = "https://raw.githubusercontent.com/sillsdev/libpalaso/master/SIL.Scripture/Resources/eng.vrs.txt"
maps = lx.versifications().with_vrs("English", "Original", get([ENG_VRS]).decode())
for reference in ["Malachi 4:1-2", "Joel 2:28", "Psalm 51:1"]:
    print(reference, "→", maps.map(reference, "English", "Original", None).text)
# → Malachi 4:1-2 → Malachi 3:19-20
# → Joel 2:28 → Joel 3:1
# → Psalm 51:1 → Psalm 51:3
print(maps.map("Psalm 51:1-2", "Original", "English", None).text)  # → Psalm 51:0

ebible = lx.parse_catalog_bytes(lx.ebible_catalog_url(), get([lx.ebible_catalog_url()]))
[wlc_zip] = lx.import_downloads(ebible.find_import("ebible:hebwlc"))
wlc = lx.open_bible([PackageFile(path="hebwlc_usfm.zip", bytes=get(wlc_zip.urls))])
print(list(wlc.chapter_verses(39)))  # → [14, 17, 24]
print(maps.map("Malachi 3", "Original", "English", wlc.chapter_verses(39)).text)  # → Malachi 3:1-18; 4:1-6

# Malachi 4:1 in the WEB and the WLC, side by side
wlc_malachi = wlc.book(39).verse_texts()
there = maps.map("Malachi 4:1", "English", "Original", None).verses
web_malachi = verses_of(bibles[2], 39)
print("WEB 4:1", web_malachi["4:1"][:30])
for v in there:
    print(f"WLC {v.chapter}:{v.verse}", next(w for w in wlc_malachi if (w.chapter, w.verse) == (v.chapter, v.verse)).text[:30])
# → WEB 4:1 “For behold, the day comes, bu
# → WLC 3:19 כִּֽי־הִנֵּ֤ה הַיּוֹם֙ בָּ֔א ב
```

Look the mapped verses up by number in the other Bible's `verseTexts()`, as here, rather than
through topos: topos checks references against English counts, so it has no Malachi 3:19.

eBible.org's zips can't be downloaded by a web page ([Bibles in your app](../apps.md#which-way)):
in a browser, map with the same `.vrs` (raw.githubusercontent.com allows any site) and read the
other Bible from files your user brings.

In Rust, `logoxide_text::versification` reads the same files:

```rust
use logoxide_text::versification::{VersificationMap, Versifications};

fn map_malachi() -> Result<()> {
    let url = "https://raw.githubusercontent.com/sillsdev/libpalaso/master/SIL.Scripture/Resources/eng.vrs.txt";
    let text = String::from_utf8(get(&[url.to_string()])?)?;
    let maps = Versifications::new(vec![VersificationMap::parse("English", "Original", [("eng.vrs", text.as_str())])?]);
    let mapped = maps.map_reference("Malachi 4:1-2", "English", "Original", None)?;
    println!("{} {:?}", mapped.text(), mapped.missing_text()); // Malachi 3:19-20 ""
    Ok(())
}
```

A map can also be a package of its own (`kind = "versification-map"`, with `versification` and
`maps-to`), which `withMap` adds, and maps chain: English to Original, then Original to the
Vulgate's numbering. The bindings' README, under "Versification maps", has both.

## In a vault

In a vault, each Bible's `logoxide.toml` names its numbering (`versification = "English"`) and
the vault's map packages connect them, so comparing needs no code of your own: `lx compare "Joel
2:28" KJV WLC` prints each Bible's text in its own numbering, saying what one lacks. Natively
(Python, Swift, Kotlin), `comparePassage` does the same from the vault's folder; in WebAssembly,
open each package and map as above. The bindings' README, under "Reading natively".
