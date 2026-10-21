# Links that survive corrections

Bookmarks, highlights, and notes on verses, kept so they still point at the right words after a
translation is corrected: a verse reworded, or verses numbered differently in a new edition. A
link that keeps only `John 3:16` silently moves to other words when the numbering changes, and
a highlight kept as offsets lands on the wrong letters when a word is added. So keep, with the
reference, a short hash of the verse's words and the words highlighted, and find them again when
the translation is downloaded again.

This starts from the helpers in [Bibles in your app](../apps.md#setting-up) (`lx`, `topos`,
`get`) and the BSB's catalog entry from [Finding a translation](../apps.md#finding-a-translation)
(`bsb`). The examples run in order, as one program in each language.

## What to keep

Every verse `verseTexts()` gives has a `hash`: the first 7 bytes (14 hex digits) of the SHA-256
of its words, as verse-hash rule 1 makes them (notes and headings left out, NFC, a few invisible
characters dropped, whitespace collapsed; [doc/summaries.md](../summaries.md) says exactly). The
rule never changes, so a hash made today matches one made by a later release. Correcting a
footnote, a heading, or the markup leaves a verse's hash alone; changing its words changes it.

A link keeps the first 8 hex digits (4 bytes), which is plenty to tell the verses of one book
apart, with the translation, its version where it has one, the verse, and the words highlighted,
if any. In a URL, logoxide's form is `&ver=1.0.2&vh=9f86d081`: the version and the first verse's
hash.

```ts
import { type BookVerseText } from "logoxide";

const johnDownload = lx.bookDownloads(bsb).find((b) => b.bookId === 43)!;
const before = new TextDecoder().decode(await get(johnDownload.urls));

/** What an app keeps of a bookmark or a highlight */
interface Link {
  import: string; // the translation: "fetch:eng_bsb"
  version: string | null; // the translation's version, where it has one
  bookId: number;
  chapter: number;
  verse: number;
  vh: string; // the verse's hash, its first 8 hex digits
  fragment: string | null; // the words highlighted
}

/** A link to a verse, with the words highlighted in it */
function link(bookId: number, v: BookVerseText, fragment: string | null = null): Link {
  return { import: "fetch:eng_bsb", version: bsb.version, bookId, chapter: v.chapter, verse: v.verse, vh: v.hash.slice(0, 8), fragment };
}

const old = LogoxideBook.parse(topos, "usx", before)!.verseTexts();
const at = (verses: BookVerseText[], chapter: number, verse: number) => verses.find((v) => v.chapter === chapter && v.verse === verse)!;
const links = [link(43, at(old, 3, 16), "loved the world"), link(43, at(old, 3, 18)), link(43, at(old, 3, 36), "eternal life")];
console.log(links[0]); // → { import: 'fetch:eng_bsb', version: '2024', bookId: 43, chapter: 3, verse: 16, vh: 'c6a0fb4f', fragment: 'loved the world' }
console.log(`?ref=John.3.16&ver=${links[0].version}&vh=${links[0].vh}`); // → ?ref=John.3.16&ver=2024&vh=c6a0fb4f
```

```python
import re
from dataclasses import dataclass

from logoxide import BookVerseText

john_download = next(b for b in lx.book_downloads(bsb) if b.book_id == 43)
before = get(john_download.urls).decode()


@dataclass
class Link:
    """What an app keeps of a bookmark or a highlight"""
    import_: str  # the translation: "fetch:eng_bsb"
    version: str | None  # the translation's version, where it has one
    book_id: int
    chapter: int
    verse: int
    vh: str  # the verse's hash, its first 8 hex digits
    fragment: str | None = None  # the words highlighted


def link(book_id: int, v: BookVerseText, fragment: str | None = None) -> Link:
    """A link to a verse, with the words highlighted in it"""
    return Link("fetch:eng_bsb", bsb.version, book_id, v.chapter, v.verse, v.hash[:8], fragment)


old = LogoxideBook.parse(topos, "usx", before).verse_texts()
at = lambda verses, chapter, verse: next(v for v in verses if (v.chapter, v.verse) == (chapter, verse))
links = [link(43, at(old, 3, 16), "loved the world"), link(43, at(old, 3, 18)), link(43, at(old, 3, 36), "eternal life")]
print(links[0])  # → Link(import_='fetch:eng_bsb', version='2024', book_id=43, chapter=3, verse=16, vh='c6a0fb4f', fragment='loved the world')
print(f"?ref=John.3.16&ver={links[0].version}&vh={links[0].vh}")  # → ?ref=John.3.16&ver=2024&vh=c6a0fb4f
```

fetch.bible's entries have a version that names the edition (`2024`) and doesn't change as it's
corrected; eBible.org's is the date of its last update; a package's is its own semantic version
(`info().version`). The hash is what finds the verse; the version only tells the user which text
the link was made against.

## Finding them again

When the translation is downloaded again, look for each link's hash in the book's verses: in the
same verse, nothing changed; in another, the verse was renumbered (take the nearest, since a
verse can repeat another's words); in none, the words changed, and the verse at the same number
is the best guess, with the words highlighted found again in it by `LogoxideBook.findFragment`.
It finds the run of words that matches the fragment best, ignoring case, accents, and punctuation,
when at least three in five of its words are still there in order; otherwise it gives none, and
the highlight is lost. Its offsets are UTF-8 bytes.

To show it, a corrected edition of John made by hand: 3:16 reworded, and chapter 3 numbered one
higher from verse 18 on.

```ts
/** A corrected edition: John 3:16 reworded, and John 3 from verse 18 on numbered one higher */
function corrected(usx: string): string {
  const renumber = (_: string, start: string, n: string) => `${start}${Number(n) >= 18 ? Number(n) + 1 : n}`;
  return usx
    .replace("For God so loved the world that He gave", "For God so greatly loved the world, He gave")
    .replace(/sid="JHN 3" \/>[\s\S]*?<chapter eid="JHN 3" \/>/, (chapter) =>
      chapter.replace(/(number=")(\d+)/g, renumber).replace(/(JHN 3:)(\d+)/g, renumber));
}
const after = LogoxideBook.parse(topos, "usx", corrected(before))!.verseTexts();

/** A link's highlighted words in a verse's text now (findFragment's offsets are UTF-8 bytes) */
function highlight(text: string, fragment: string | null): string | null {
  const found = fragment ? LogoxideBook.findFragment(text, fragment) : null;
  return found && new TextDecoder().decode(new TextEncoder().encode(text).slice(found.start, found.end));
}

/** A link found again in a book's verses now */
function resolve(link: Link, verses: BookVerseText[]) {
  const there = verses.find((v) => v.chapter === link.chapter && v.verse === link.verse);
  // Its words, unchanged: where it was, else the nearest verse with them (a verse can repeat another's words)
  const distance = (v: BookVerseText) => Math.abs(v.chapter * 1000 + v.verse - (link.chapter * 1000 + link.verse));
  const same = verses.filter((v) => v.hash.startsWith(link.vh)).sort((a, b) => distance(a) - distance(b))[0];
  const verse = same ?? there;
  if (!verse) return ["gone"];
  const status = !same ? "reworded" : same === there ? "same" : "moved";
  return [status, verse.chapter, verse.verse, highlight(verse.text, link.fragment)];
}

for (const l of links) console.log(`3:${l.verse}`, resolve(l, after));
// → 3:16 [ 'reworded', 3, 16, 'loved the world' ]
// → 3:18 [ 'moved', 3, 19, null ]
// → 3:36 [ 'moved', 3, 37, 'eternal life' ]
```

```python
def corrected(usx: str) -> str:
    """A corrected edition: John 3:16 reworded, and John 3 from verse 18 on numbered one higher"""
    renumber = lambda m: m[1] + str(int(m[2]) + 1 if int(m[2]) >= 18 else int(m[2]))
    usx = usx.replace("For God so loved the world that He gave", "For God so greatly loved the world, He gave")
    return re.sub(
        r'sid="JHN 3" />.*?<chapter eid="JHN 3" />',
        lambda chapter: re.sub(r"(JHN 3:)(\d+)", renumber, re.sub(r'(number=")(\d+)', renumber, chapter[0])),
        usx,
        flags=re.S,
    )


after = LogoxideBook.parse(topos, "usx", corrected(before)).verse_texts()


def highlight(text: str, fragment: str | None) -> str | None:
    """A link's highlighted words in a verse's text now (find_fragment's offsets are UTF-8 bytes)"""
    found = LogoxideBook.find_fragment(text, fragment) if fragment else None
    return text.encode()[found.start : found.end].decode() if found else None


def resolve(link: Link, verses: list[BookVerseText]):
    """A link found again in a book's verses now"""
    there = next((v for v in verses if (v.chapter, v.verse) == (link.chapter, link.verse)), None)
    # Its words, unchanged: where it was, else the nearest verse with them (a verse can repeat another's words)
    distance = lambda v: abs(v.chapter * 1000 + v.verse - (link.chapter * 1000 + link.verse))
    same = min((v for v in verses if v.hash.startswith(link.vh)), key=distance, default=None)
    verse = same or there
    if verse is None:
        return ("gone",)
    status = "reworded" if same is None else "same" if same is there else "moved"
    return (status, verse.chapter, verse.verse, highlight(verse.text, link.fragment))


for l in links:
    print(f"3:{l.verse}", resolve(l, after))
# → 3:16 ('reworded', 3, 16, 'loved the world')
# → 3:18 ('moved', 3, 19, None)
# → 3:36 ('moved', 3, 37, 'eternal life')
```

In Rust, `logoxide_text::hash::verse_hash` hashes a verse's text (`verse_texts` gives it), and
`logoxide_text::passage::find_fragment` finds a fragment, as byte ranges:

```rust
use logoxide_text::{
    MemoryFiles, Source,
    hash::verse_hash,
    passage::{VerseText, find_fragment, verse_texts},
    source::open_loose,
    topos_bible::data::books::BookId,
};

/// A verse's hash as links carry it: its first 8 hex digits (4 bytes)
fn vh(text: &str) -> String {
    verse_hash(text)[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Where a link (a verse's chapter, verse, and `vh`) is in a book's verses now, and whether its
/// words changed
fn resolve<'a>(verses: &'a [VerseText], chapter: u8, verse: u8, hash: &str) -> Option<(&'a VerseText, bool)> {
    let place = |c: u8, v: u8| i32::from(c) * 1000 + i32::from(v);
    let same = (verses.iter().filter(|v| vh(&v.text) == hash)).min_by_key(|v| (place(v.chapter, v.verse) - place(chapter, verse)).abs());
    let there = verses.iter().find(|v| (v.chapter, v.verse) == (chapter, verse));
    same.map(|v| (v, false)).or(there.map(|v| (v, true)))
}

fn relink(john_usx: &str) -> Result<()> {
    let read = |usx: &str| -> Result<Vec<VerseText>> {
        let files = MemoryFiles::with([("jhn.usx", usx.as_bytes().to_vec())]);
        Ok(verse_texts(&open_loose(&files, "")?.book(BookId(43))?))
    };
    let old = read(john_usx)?;
    let john316 = old.iter().find(|v| (v.chapter, v.verse) == (3, 16)).ok_or("no John 3:16")?;
    let hash = vh(&john316.text);
    println!("{hash}"); // c6a0fb4f
    let new = read(&john_usx.replace("For God so loved the world that He gave", "For God so greatly loved the world, He gave"))?;
    let (now, reworded) = resolve(&new, 3, 16, &hash).ok_or("gone")?;
    let words = find_fragment(&now.text, "loved the world").map(|r| &now.text[r]);
    println!("{}:{} {reworded} {words:?}", now.chapter, now.verse); // 3:16 true Some("loved the world")
    Ok(())
}
```

Say what happened rather than fixing it silently: a moved link can be updated as it is, but a
reworded verse's note may no longer fit its words, so show it beside the new text. A note on a
verse that's gone (dropped, or joined to another) keeps its old reference, the old version, and
whatever text was kept with it.

## With packages

A package (or an import made into one, `importDownloaded`) gives the same hashes for a book:
`verseHashes(bookId)`, from the book it has parsed (and cached), and each release publishes them
in its `logoxide.verses.bin`. Its releases are versioned by what changed (minor for wording, major for verses removed
or renumbered), so an app can skip the search above for a patch release, and `lx diff` and
`lx verify` compare two versions' hashes. The bindings' README has
[`verseHashes`](../../crates/logoxide-ffi/README.md#opening-a-package) and
[`updateImport`](../../crates/logoxide-ffi/README.md#imports), which says what a newer download
of an import changed.
