# References in text

Finding every Bible reference in what someone wrote (notes, Markdown, a blog post, a sermon
outline), linking each, showing its verses when it's hovered, storing it, and completing one as
it's typed. References are topos's work, in the same package as logoxide; the verses come from a
Bible read as in [Bibles in your app](../apps.md). The examples start from the helpers in
[Bibles in your app](../apps.md#setting-up) (`lx`, `topos`, `get`) and run in order, as one
program in each language.

topos's own guide, in [topos-ffi's README](https://github.com/MasterTemple/topos-bible/tree/main/crates/topos-ffi),
has the rest: queries (only the Gospels, only passages inside Romans 8), indexes of many files,
and every format option.

- [Finding them](#finding-them)
- [Linking them](#linking-them)
- [Showing their verses](#showing-their-verses)
- [Storing them](#storing-them)
- [Completing one as it's typed](#completing-one-as-its-typed)
- [Which notes cite a passage](#which-notes-cite-a-passage)

## Finding them

`search` finds each reference in a text, however it's written (`1 Cor 13:4–7`, `Jn 3:16, 17`,
`Rom. 8:28-39`, `Ps 23`), with where it is and the passage it names: `reference` to show,
`osis` and `usfm` to store or link. It checks each against the book's chapters and verses, so
`John 3:40` isn't one.

Where it is comes in the unit your strings are indexed by, which matters as soon as a text has
an emoji or an accent before a reference: UTF-16 code units in JavaScript (`OffsetUnit.Utf16`),
characters in Python (`OffsetUnit.CHAR`), bytes in Rust. `line` and `column` start at 1, and `column` is in the same unit: the emoji before `Gen 1:1` is two
UTF-16 code units and one character, so its column is 47 in JavaScript and 46 in Python.

```ts
import { BookStyle, OffsetUnit } from "logoxide";

const note = "Love is patient (1 Cor 13:4–7).\nSee Jn 3:16, 17 and Rom. 8:28-39 KJV. 💡 Then Gen 1:1, not John 3:40.";
for (const m of topos.search(note, OffsetUnit.Utf16)) {
  console.log(note.slice(m.start, m.end), "·", m.passage.reference, "·", m.passage.osis, "·", m.line, m.column);
}
// → 1 Cor 13:4–7 · 1 Corinthians 13:4-7 · 1Cor.13.4-1Cor.13.7 · 1 18
// → Jn 3:16, 17 · John 3:16,17 · John.3.16 John.3.17 · 2 5
// → Rom. 8:28-39 · Romans 8:28-39 · Rom.8.28-Rom.8.39 · 2 21
// → Gen 1:1 · Genesis 1:1 · Gen.1.1 · 2 47
```

```python
from logoxide import BookStyle, OffsetUnit

note = "Love is patient (1 Cor 13:4–7).\nSee Jn 3:16, 17 and Rom. 8:28-39 KJV. 💡 Then Gen 1:1, not John 3:40."
for m in topos.search(note, OffsetUnit.CHAR):
    print(note[m.start : m.end], "·", m.passage.reference, "·", m.passage.osis, "·", m.line, m.column)
# → 1 Cor 13:4–7 · 1 Corinthians 13:4-7 · 1Cor.13.4-1Cor.13.7 · 1 18
# → Jn 3:16, 17 · John 3:16,17 · John.3.16 John.3.17 · 2 5
# → Rom. 8:28-39 · Romans 8:28-39 · Rom.8.28-Rom.8.39 · 2 21
# → Gen 1:1 · Genesis 1:1 · Gen.1.1 · 2 46
```

In Rust, topos-bible comes with logoxide-text (`logoxide_text::topos_bible`); its matcher gives
byte ranges, and lines and columns in bytes, characters, and UTF-16:

```rust
use logoxide_text::topos_bible::{
    matcher::BibleMatcher,
    segments::formatter::{BookStyle, FormatOptions},
};

fn find(note: &str) {
    let matcher = BibleMatcher::default();
    let data = matcher.data();
    for m in matcher.search(note) {
        let reference = FormatOptions::default().passage(&m.psg, data).unwrap_or_default();
        let osis = m.psg.to_osis(data.books()).unwrap_or_default();
        // 1 Cor 13:4–7 · 1 Corinthians 13:4-7 · 1Cor.13.4-1Cor.13.7
        let bytes = &m.location.bytes;
        println!("{} · {reference} · {osis}", &note[bytes.start..bytes.end]);
    }
}
```

A reference followed by a translation's abbreviation (`Rom. 8:28-39 KJV`) can name it: see
[Showing their verses](#showing-their-verses).

## Linking them

A list (`Jn 3:16, 17`) is one reference with several parts; `parts` gives each part its own
place in the text, so each can be its own link. Replace them from the last to the first, so the
places before each stay right, and leave alone what's already a link or code.

```ts
/** Markdown with each reference a link (`href` gives each part's URL), except in code and links */
function linkify(markdown: string, href: (osis: string) => string): string {
  const kept = [...markdown.matchAll(/`[^`]*`|\[[^\]]*\]\([^)]*\)/g)].map((m) => [m.index, m.index + m[0].length]);
  let out = markdown;
  for (const p of topos.parts(markdown, OffsetUnit.Utf16).reverse()) {
    if (kept.some(([start, end]) => p.start < end && start < p.end)) continue;
    out = `${out.slice(0, p.start)}[${out.slice(p.start, p.end)}](${href(p.passage.osis)})${out.slice(p.end)}`;
  }
  return out;
}

console.log(linkify("Read Jn 3:16, 17, not `Gen 1:1` or [Ps 23](https://example.org).", (osis) => `/bible/${osis}`));
// → Read [Jn 3:16](/bible/John.3.16), [17](/bible/John.3.17), not `Gen 1:1` or [Ps 23](https://example.org).
```

```python
import re


def linkify(markdown: str, href) -> str:
    """Markdown with each reference a link (href gives each part's URL), except in code and links"""
    kept = [m.span() for m in re.finditer(r"`[^`]*`|\[[^\]]*\]\([^)]*\)", markdown)]
    out = markdown
    for p in reversed(topos.parts(markdown, OffsetUnit.CHAR)):
        if any(p.start < end and start < p.end for start, end in kept):
            continue
        out = f"{out[: p.start]}[{out[p.start : p.end]}]({href(p.passage.osis)}){out[p.end :]}"
    return out


print(linkify("Read Jn 3:16, 17, not `Gen 1:1` or [Ps 23](https://example.org).", lambda osis: f"/bible/{osis}"))
# → Read [Jn 3:16](/bible/John.3.16), [17](/bible/John.3.17), not `Gen 1:1` or [Ps 23](https://example.org).
```

A part's `first` and `last` are its first and last verse, and `lastStart` where the last verse's
number is written (in `1 Cor 13:4–7`, the `7`), for an editor that links each end.

## Showing their verses

To show a reference's verses (a tooltip, a hover in an editor, an embed), read its book in the
translation it names, else your default, and write the passage as text. Give topos your
translations' names, as [Bibles in your app](../apps.md#references-that-name-your-translations)
does, and `searchWithTranslations` says which a reference names. Keep each book once it's read:
a page of notes cites the same few books again and again.

```ts
import { ToposQuery, ToposTranslations, type BookTextOptions } from "logoxide";

const FETCH_BIBLE = "https://v1.fetch.bible/manifest.json";
const fetchBible = lx.parseCatalogBytes(FETCH_BIBLE, await get([FETCH_BIBLE]));
const translations = { BSB: fetchBible.findImport("fetch:eng_bsb")!, KJV: fetchBible.findImport("fetch:eng_kjv")! };
const names = ToposTranslations.create().add("BSB", ["Berean"]).add("KJV", ["AV"]);

const books = new Map<string, Promise<LogoxideBook | null>>();
/** A translation's book, downloaded once; null if the translation hasn't it */
function bookOf(translation: keyof typeof translations, bookId: number) {
  const key = `${translation}/${bookId}`;
  if (!books.has(key)) {
    const download = lx.bookDownloads(translations[translation]).find((b) => b.bookId === bookId);
    books.set(key, download ? get(download.urls).then((bytes) => LogoxideBook.parse(topos, "usx", new TextDecoder().decode(bytes))) : Promise.resolve(null));
  }
  return books.get(key)!;
}

const plain: BookTextOptions = { notes: false, headings: false, classPrefix: "" };
for (const m of topos.searchWithTranslations(note, OffsetUnit.Utf16, ToposQuery.create(), names)) {
  const translation = (m.translation?.name ?? "BSB") as keyof typeof translations;
  const book = await bookOf(translation, m.reference.passage.bookId);
  const text = book?.text(m.reference.passage, plain).join(" … ") ?? "";
  console.log(`${m.reference.passage.reference} ${translation}: ${text.slice(0, 50)}`);
}
// → 1 Corinthians 13:4-7 BSB: Love is patient, love is kind. It does not envy, i
// → John 3:16,17 BSB: For God so loved the world that He gave His one an
// → Romans 8:28-39 KJV: And we know that all things work together for good
// → Genesis 1:1 BSB: In the beginning God created the heavens and the e
```

```python
from logoxide import BookTextOptions, ToposQuery, ToposTranslations

FETCH_BIBLE = "https://v1.fetch.bible/manifest.json"
fetch_bible = lx.parse_catalog_bytes(FETCH_BIBLE, get([FETCH_BIBLE]))
translations = {"BSB": fetch_bible.find_import("fetch:eng_bsb"), "KJV": fetch_bible.find_import("fetch:eng_kjv")}
names = ToposTranslations.create().add("BSB", ["Berean"]).add("KJV", ["AV"])

books: dict[tuple[str, int], LogoxideBook | None] = {}


def book_of(translation: str, book_id: int) -> LogoxideBook | None:
    """A translation's book, downloaded once; None if the translation hasn't it"""
    if (translation, book_id) not in books:
        download = next((b for b in lx.book_downloads(translations[translation]) if b.book_id == book_id), None)
        books[translation, book_id] = LogoxideBook.parse(topos, "usx", get(download.urls).decode()) if download else None
    return books[translation, book_id]


plain = BookTextOptions(notes=False, headings=False, class_prefix="")
for m in topos.search_with_translations(note, OffsetUnit.CHAR, ToposQuery.create(), names):
    translation = m.translation.name if m.translation else "BSB"
    book = book_of(translation, m.reference.passage.book_id)
    text = " … ".join(book.text(m.reference.passage, plain)) if book else ""
    print(f"{m.reference.passage.reference} {translation}: {text[:50]}")
# → 1 Corinthians 13:4-7 BSB: Love is patient, love is kind. It does not envy, i
# → John 3:16,17 BSB: For God so loved the world that He gave His one an
# → Romans 8:28-39 KJV: And we know that all things work together for good
# → Genesis 1:1 BSB: In the beginning God created the heavens and the e
```

`text` gives one string for each part of the passage (`John 3:16,17` has two); `html` gives the
same as HTML, with verse numbers a stylesheet can show, and `styledText` with verse labels of
your own: the bindings' README, under "A book's text". In Rust, `logoxide_text::passage::text`
writes a passage of a book read as in [Bibles in your app](../apps.md#a-book-at-a-time):

```rust
use logoxide_text::{
    Book,
    passage::{Style, text},
    topos_bible::segments::passage::Passage,
};

/// A passage of a book as plain text: no verse numbers, headings, or notes
fn verses_of(book: &Book, passage: &Passage) -> String {
    let style = Style { hidden: vec![logoxide_text::markup::Kind::Note, logoxide_text::markup::Kind::Title], ..Style::default() };
    text(book, passage, &style, "", None).map(|t| t.text).unwrap_or_default()
}
```

## Storing them

Store a reference as OSIS (`John.3.16-John.3.18 John.3.20`): it's one string, it never depends
on how someone wrote it, and topos reads it back. Show it with `formatPassage`, in the style your
app uses.

```ts
import { ToposFormat } from "logoxide";

const passage = topos.parse("Jn 3:16-18, 20", BookStyle.Name)!;
const stored = passage.osis;
const again = topos.parse(stored, BookStyle.Name)!;
console.log(stored, "·", again.reference); // → John.3.16-John.3.18 John.3.20 · John 3:16-18,20
console.log(topos.formatPassage(again, ToposFormat.create().book(BookStyle.Abbreviation).verseSeparator(", "))); // → Jn 3:16-18, 20
console.log(topos.formatPassage(again, ToposFormat.create().book(BookStyle.Usfm))); // → JHN 3:16-18,20
```

```python
from logoxide import ToposFormat

passage = topos.parse("Jn 3:16-18, 20", BookStyle.NAME)
stored = passage.osis
again = topos.parse(stored, BookStyle.NAME)
print(stored, "·", again.reference)  # → John.3.16-John.3.18 John.3.20 · John 3:16-18,20
print(topos.format_passage(again, ToposFormat.create().book(BookStyle.ABBREVIATION).verse_separator(", ")))  # → Jn 3:16-18, 20
print(topos.format_passage(again, ToposFormat.create().book(BookStyle.USFM)))  # → JHN 3:16-18,20
```

```rust
fn store() -> Option<()> {
    let matcher = BibleMatcher::default();
    let data = matcher.data();
    let passage = matcher.find("Jn 3:16-18, 20")?.psg;
    let stored = passage.to_osis(data.books())?; // John.3.16-John.3.18 John.3.20
    let again = data.books().parse_osis(&stored).ok()?;
    let short = FormatOptions { book: BookStyle::Abbreviation, ..FormatOptions::default() };
    println!("{stored} · {}", short.passage(&again, data)?); // John.3.16-John.3.18 John.3.20 · Jn 3:16-18,20
    Some(())
}
```

To link a verse so the link still finds it after the translation is re-edited, keep its hash
with it (`verseTexts()`'s `hash`): [Bibles in your app](../apps.md#from-a-book-to-verses).

## Completing one as it's typed

`complete` offers what a half-typed reference can become, from the cursor back: books
(`1 Co` → `1 Corinthians`), then chapters, then verses. Each completion replaces `start` to
`end` (in the unit asked for) with its `text`, which ends a book's name with a space, ready for
the chapter. `completeWithTranslations` also completes a
translation's name after a reference.

```ts
const typed = "See 1 Co";
const offered = topos.complete(typed, typed.length, OffsetUnit.Utf16, BookStyle.Name, 3);
console.log(offered.map((c) => c.label)); // → [ '1 Corinthians', '1 Corinthians 1', '1 Corinthians 2' ]
const chosen = offered[0];
console.log(JSON.stringify(typed.slice(0, chosen.start) + chosen.text + typed.slice(chosen.end))); // → "See 1 Corinthians "
const verse = "See John 3:";
console.log(topos.complete(verse, verse.length, OffsetUnit.Utf16, BookStyle.Name, 40).length); // → 36
const named = "John 3:16 K";
console.log(topos.completeWithTranslations(named, named.length, OffsetUnit.Utf16, ToposFormat.create(), 5, names).map((c) => c.label)); // → [ 'KJV' ]
```

```python
typed = "See 1 Co"
offered = topos.complete(typed, len(typed), OffsetUnit.CHAR, BookStyle.NAME, 3)
print([c.label for c in offered])  # → ['1 Corinthians', '1 Corinthians 1', '1 Corinthians 2']
chosen = offered[0]
print(repr(typed[: chosen.start] + chosen.text + typed[chosen.end :]))  # → 'See 1 Corinthians '
verse = "See John 3:"
print(len(topos.complete(verse, len(verse), OffsetUnit.CHAR, BookStyle.NAME, 40)))  # → 36
named = "John 3:16 K"
print([c.label for c in topos.complete_with_translations(named, len(named), OffsetUnit.CHAR, ToposFormat.create(), 5, names)])  # → ['KJV']
```

```rust
use logoxide_text::topos_bible::segments::autocomplete::CompleteOptions;

fn complete(typed: &str) -> Vec<String> {
    let matcher = BibleMatcher::default();
    let options = CompleteOptions { limit: Some(3), ..CompleteOptions::default() };
    let offered = matcher.complete(typed, typed.len(), &options);
    if let Some(first) = offered.first() {
        let mut done = typed.to_string();
        done.replace_range(first.edit.range.clone(), &first.edit.text); // "See 1 Corinthians "
    }
    offered.into_iter().map(|c| c.label).collect() // ["1 Corinthians", "1 Corinthians 1", "1 Corinthians 2"]
}
```

A book picker (a menu of books, not running text) wants `suggestBooks("john", 5)`: John, then
1, 2, and 3 John.

## Which notes cite a passage

With each note's references stored, `overlaps` finds the notes touching a passage (John 3:30-4:2
touches John 3), `contains` those inside it, and `merge` joins a note's references into one
passage per book, to list what it covers.

```ts
const notes = { "On love": "1 Cor 13:4-7; Jn 3:16", "Nicodemus": "John 3:1-21", "Witness": "Jn 3:30-4:2", "Romans": "Rom 8:28" };
const john3 = topos.parse("John 3", BookStyle.Name)!;
for (const [title, text] of Object.entries(notes)) {
  const cited = topos.search(text, OffsetUnit.Utf16).map((m) => m.passage);
  const touches = cited.some((p) => topos.overlaps(john3, p));
  const inside = touches && cited.every((p) => p.bookId !== john3.bookId || topos.contains(john3, p));
  console.log(title, touches, inside);
}
// → On love true true
// → Nicodemus true true
// → Witness true false
// → Romans false false
const together = topos.merge(["John 3:16", "Jn 3:17-18", "John 3:20"].map((r) => topos.parse(r, BookStyle.Name)!));
console.log(together.map((p) => p.reference)); // → [ 'John 3:16-18,20' ]
```

```python
notes = {"On love": "1 Cor 13:4-7; Jn 3:16", "Nicodemus": "John 3:1-21", "Witness": "Jn 3:30-4:2", "Romans": "Rom 8:28"}
john3 = topos.parse("John 3", BookStyle.NAME)
for title, text in notes.items():
    cited = [m.passage for m in topos.search(text, OffsetUnit.CHAR)]
    touches = any(topos.overlaps(john3, p) for p in cited)
    inside = touches and all(p.book_id != john3.book_id or topos.contains(john3, p) for p in cited)
    print(title, touches, inside)
# → On love True True
# → Nicodemus True True
# → Witness True False
# → Romans False False
together = topos.merge([topos.parse(r, BookStyle.NAME) for r in ["John 3:16", "Jn 3:17-18", "John 3:20"]])
print([p.reference for p in together])  # → ['John 3:16-18,20']
```

In Rust, a `Passage` has `overlaps_with` and `contains`, and `Passage::merge(passages, data)`
joins them.

For many files (a vault of notes), topos keeps an index of every reference in them, updated as
files change (`ToposIndex`); and the language server, logoxide-lsp, does all of this in any
editor.
