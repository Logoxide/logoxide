# Showing passages

A reader view: what someone typed (`jn 3:16-17`, `Psalm 23`) parsed into a passage, its book
downloaded once, and the passage written as HTML with verse numbers, headings, and the words of
Jesus marked, or as plain text with its citation for copying. Then a whole chapter with the
chapters before and after it, and a verse for the day.

The examples start from the helpers in [Bibles in your app](../apps.md#setting-up) (`lx`,
`topos`, `get`) and run in order, as one program in each language. Books come from fetch.bible:
the BSB, which has headings, and the WEB, which marks the words of Jesus (no translation there
has both).

- [Books, downloaded once](#books-downloaded-once)
- [What someone typed](#what-someone-typed)
- [A passage as HTML](#a-passage-as-html)
- [A stylesheet](#a-stylesheet)
- [Plain text to copy, with its citation](#plain-text-to-copy-with-its-citation)
- [A chapter, and the ones around it](#a-chapter-and-the-ones-around-it)
- [A verse for the day](#a-verse-for-the-day)
- [In Rust](#in-rust)

## Books, downloaded once

A book is parsed once and kept: asking again for John gives the same `LogoxideBook`. Keep the
promise, not the book, so two passages from John asked for at once download it once.
`bookDownloads` says which books a translation has (a New Testament alone has 27), so a book
it lacks is `null` rather than a download of fetch.bible's folder listing.

```ts
import { BookStyle, OffsetUnit, ToposFormat, type BookTextOptions, type BookTextStyle } from "logoxide";

const FETCH_BIBLE = "https://v1.fetch.bible/manifest.json";
const fetchBible = lx.parseCatalogBytes(FETCH_BIBLE, await get([FETCH_BIBLE]));

const opened = new Map<string, Promise<LogoxideBook | null>>();

/** A book of a translation on fetch.bible (`fetch:eng_bsb`), downloaded and parsed once; null if it hasn't the book */
function book(translation: string, bookId: number): Promise<LogoxideBook | null> {
  const key = `${translation}/${bookId}`;
  if (!opened.has(key)) {
    const entry = fetchBible.findImport(translation);
    const download = entry && lx.bookDownloads(entry).find((b) => b.bookId === bookId);
    const loading = download
      ? get(download.urls).then((bytes) => LogoxideBook.parse(topos, "usx", new TextDecoder().decode(bytes)))
      : Promise.resolve(null);
    loading.catch(() => opened.delete(key)); // a failed download can be tried again
    opened.set(key, loading);
  }
  return opened.get(key)!;
}
```

```python
import dataclasses
import re

from logoxide import BookStyle, BookTextOptions, BookTextStyle, ChapterVerse, OffsetUnit, ToposFormat

FETCH_BIBLE = "https://v1.fetch.bible/manifest.json"
fetch_bible = lx.parse_catalog_bytes(FETCH_BIBLE, get([FETCH_BIBLE]))

opened: dict[tuple[str, int], LogoxideBook | None] = {}


def book(translation: str, book_id: int) -> LogoxideBook | None:
    """A book of a translation on fetch.bible (fetch:eng_bsb), downloaded and parsed once; None if it hasn't the book"""
    key = (translation, book_id)
    if key not in opened:
        entry = fetch_bible.find_import(translation)
        download = next((b for b in lx.book_downloads(entry) if b.book_id == book_id), None) if entry else None
        opened[key] = LogoxideBook.parse(topos, "usx", get(download.urls).decode()) if download else None
    return opened[key]
```

## What someone typed

`topos.parse` reads one reference however it's written (`jn 3:16-17`, `John 3.16`, `Ps 23`) into
a `Passage`: its `bookId`, its `reference` written out, and its OSIS and USFM forms. It's `null`
for text that isn't a reference, and for a verse the Bible doesn't have (`John 22:1`, John
having 21 chapters). A reference can have several parts in one book (`John 3:16; 4:1`); for
references to several books, or references inside a sentence, `topos.parts` and `topos.search`
find each one.

```ts
for (const typed of ["jn 3:16-17", "John 3:16; 4:1", "Ps 23", "John 22:1", "hello"]) {
  console.log(typed, "→", topos.parse(typed, BookStyle.Name)?.reference ?? null);
}
// → jn 3:16-17 → John 3:16-17
// → John 3:16; 4:1 → John 3:16; 4:1
// → Ps 23 → Psalm 23
// → John 22:1 → null
// → hello → null
console.log(topos.parts("John 3:16; Rom 8:28", OffsetUnit.Utf16).map((m) => m.passage.reference)); // → [ 'John 3:16', 'Romans 8:28' ]
console.log(topos.search("as Paul says in Rom 8:28-30, and", OffsetUnit.Utf16).map((m) => [m.passage.reference, m.start, m.end])); // → [ [ 'Romans 8:28-30', 16, 27 ] ]
```

```python
for typed in ["jn 3:16-17", "John 3:16; 4:1", "Ps 23", "John 22:1", "hello"]:
    passage = topos.parse(typed, BookStyle.NAME)
    print(typed, "→", passage.reference if passage else None)
# → jn 3:16-17 → John 3:16-17
# → John 3:16; 4:1 → John 3:16; 4:1
# → Ps 23 → Psalm 23
# → John 22:1 → None
# → hello → None
print([m.passage.reference for m in topos.parts("John 3:16; Rom 8:28", OffsetUnit.CHAR)])  # → ['John 3:16', 'Romans 8:28']
print([(m.passage.reference, m.start, m.end) for m in topos.search("as Paul says in Rom 8:28-30, and", OffsetUnit.CHAR)])  # → [('Romans 8:28-30', 16, 27)]
```

## A passage as HTML

`html(passage, options)` writes each part of the passage (`John 3:16; 4:1` has two) as HTML:
paragraphs as `<p class="lx-p">`, headings as `<div class="lx-title">`, lines of poetry as
`<span class="lx-l" data-style="q1">`, the words of Jesus as `<span class="lx-woj">`, and each
verse's start as an empty `<span class="lx-verse" data-n="16">` for a stylesheet to number.
Every class starts with `classPrefix`; `data-style` keeps the USX marker (`q2`, `s1`), so a
stylesheet can tell them apart. A heading belongs to the verse after it, so `John 3:1-2` starts
with "Jesus and Nicodemus". `notes: true` keeps the footnotes, as `<span class="lx-note">` where
they stand in the text.

```ts
const options: BookTextOptions = { notes: false, headings: true, classPrefix: "lx-" };
const nicodemus = topos.parse("John 3:1-2", BookStyle.Name)!;
const bsbJohn = (await book("fetch:eng_bsb", nicodemus.bookId))!;
console.log(bsbJohn.html(nicodemus, options)[0]);
// →   <div class="lx-title lx-s1" data-title-type="s1" data-style="s1">Jesus and Nicodemus</div>
// →   <p class="lx-p" data-style="p">
// →     <span class="lx-verse" data-osis-id="JHN 3:1" data-n="1"></span>Now there was a man of the Pharisees named Nicodemus, … <span class="lx-verse" data-osis-id="JHN 3:2" data-n="2"></span>He came to Jesus at night and said, … if God were not with him.”</p>
const parts = bsbJohn.html(topos.parse("John 3:16; 4:1", BookStyle.Name)!, options);
console.log(parts.length, `<div class="lx">${parts.join("")}</div>`.length > 0); // → 2 true
const withNotes = bsbJohn.html(topos.parse("John 3:16", BookStyle.Name)!, { ...options, notes: true })[0];
console.log(withNotes.match(/<span class="lx-note[^>]*>/)![0]); // → <span class="lx-note lx-footnote" data-note-type="footnote" data-n="+" data-style="f">

const webJohn = (await book("fetch:eng_web", 43))!;
const jesus = webJohn.html(topos.parse("John 3:3", BookStyle.Name)!, options)[0];
console.log(jesus.match(/<span class="lx-woj"/g)!.length, jesus.includes('data-strong="G2424"')); // → 2 true
```

```python
options = BookTextOptions(notes=False, headings=True, class_prefix="lx-")
nicodemus = topos.parse("John 3:1-2", BookStyle.NAME)
bsb_john = book("fetch:eng_bsb", nicodemus.book_id)
print(bsb_john.html(nicodemus, options)[0])
# →   <div class="lx-title lx-s1" data-title-type="s1" data-style="s1">Jesus and Nicodemus</div>
# →   <p class="lx-p" data-style="p">
# →     <span class="lx-verse" data-osis-id="JHN 3:1" data-n="1"></span>Now there was a man of the Pharisees named Nicodemus, … <span class="lx-verse" data-osis-id="JHN 3:2" data-n="2"></span>He came to Jesus at night and said, … if God were not with him.”</p>
parts = bsb_john.html(topos.parse("John 3:16; 4:1", BookStyle.NAME), options)
print(len(parts), len(f'<div class="lx">{"".join(parts)}</div>') > 0)  # → 2 True
with_notes = bsb_john.html(topos.parse("John 3:16", BookStyle.NAME), dataclasses.replace(options, notes=True))[0]
print(re.search(r'<span class="lx-note[^>]*>', with_notes)[0])  # → <span class="lx-note lx-footnote" data-note-type="footnote" data-n="+" data-style="f">

web_john = book("fetch:eng_web", 43)
jesus = web_john.html(topos.parse("John 3:3", BookStyle.NAME), options)[0]
print(jesus.count('<span class="lx-woj"'), 'data-strong="G2424"' in jesus)  # → 2 True
```

fetch.bible's WEB also marks each word with its Strong's number (`<span class="lx-w"
data-strong="G2424">Jesus</span>`), for a lexicon on hover. The HTML is the book's own text,
escaped, so it's safe to put in a page as it is.

## A stylesheet

The verse numbers come from `data-n`, so they're never part of the text a reader selects and
copies:

```css
.lx { line-height: 1.6; }
.lx-verse::before {
  content: attr(data-n);
  font-size: 0.7em;
  vertical-align: super;
  margin: 0 0.25em 0 0.4em;
  color: #888;
  user-select: none;
}
.lx-title { font-weight: 600; margin: 1.2em 0 0.4em; }
.lx-title[data-style="d"] { font-weight: normal; font-style: italic; } /* a psalm's superscription */
.lx-woj { color: #a11; }
.lx-l { display: inline-block; padding-left: 1em; text-indent: -1em; }
.lx-l[data-style="q2"] { margin-left: 1.5em; }
.lx-note { font-size: 0.8em; color: #666; }
.lx-note::before { content: " ["; }
.lx-note::after { content: "] "; }
@media (prefers-color-scheme: dark) {
  .lx-verse::before { color: #999; }
  .lx-woj { color: #e66; }
}
```

## Plain text to copy, with its citation

`text(passage, options)` is each part as plain text, without verse numbers; `styledText` writes
verse labels in a form of your own (`{verse}`, `{chapter}`, `{name}`), and wraps or cuts the
text if asked. `topos.formatPassage` writes the citation in the style you choose: the book's
name, its abbreviation, OSIS, or USFM.

```ts
const john316 = topos.parse("John 3:16-17", BookStyle.Name)!;
const plain = bsbJohn.text(john316, { notes: false, headings: false, classPrefix: "" }).join("\n\n");
console.log(`“${plain}” (${topos.formatPassage(john316, ToposFormat.create())}, BSB)`);
// → “For God so loved the world that He gave His one and only Son, … but to save the world through Him.” (John 3:16-17, BSB)
const numbered: BookTextStyle = { verseLabel: "{verse}", chapterLabel: null, notes: false, headings: false, templates: [], width: null, limit: null };
console.log(bsbJohn.styledText(john316, numbered, "John", OffsetUnit.Utf16)!.text);
// → 16 For God so loved the world … eternal life. 17 For God did not send His Son … through Him.
console.log(topos.formatPassage(john316, ToposFormat.create().book(BookStyle.Abbreviation)), john316.osis); // → Jn 3:16-17 John.3.16-John.3.17
```

```python
john316 = topos.parse("John 3:16-17", BookStyle.NAME)
plain = "\n\n".join(bsb_john.text(john316, BookTextOptions(notes=False, headings=False, class_prefix="")))
print(f"“{plain}” ({topos.format_passage(john316, ToposFormat.create())}, BSB)")
# → “For God so loved the world that He gave His one and only Son, … but to save the world through Him.” (John 3:16-17, BSB)
numbered = BookTextStyle(verse_label="{verse}", chapter_label=None, notes=False, headings=False, templates=[], width=None, limit=None)
print(bsb_john.styled_text(john316, numbered, "John", OffsetUnit.CHAR).text)
# → 16 For God so loved the world … eternal life. 17 For God did not send His Son … through Him.
print(topos.format_passage(john316, ToposFormat.create().book(BookStyle.ABBREVIATION)), john316.osis)  # → Jn 3:16-17 John.3.16-John.3.17
```

The bindings' README has the rest of `styledText` (templates for the words of Jesus, a width to
wrap at, a limit to cut at, and marks saying what each run of the text is): [A book's
text](../../crates/logoxide-ffi/README.md#a-books-text).

## A chapter, and the ones around it

`chapterHtml(chapter, options)` is a whole chapter, its headings included; it's empty for a
chapter the book doesn't have. topos's `books()` says how many chapters each book has, so the
next chapter after the last of a book is the next book's first. A translation may lack books
(the New Testament alone, or a work in progress): skip to the next one `bookDownloads` lists.

```ts
type Place = { bookId: number; chapter: number };
const bible = topos.books();

/** The chapter after this one, into the next book after a book's last; null after Revelation 22 */
function nextChapter({ bookId, chapter }: Place): Place | null {
  if (chapter < bible[bookId - 1].chapters) return { bookId, chapter: chapter + 1 };
  return bookId < bible.length ? { bookId: bookId + 1, chapter: 1 } : null;
}

/** The chapter before this one; null before Genesis 1 */
function previousChapter({ bookId, chapter }: Place): Place | null {
  if (chapter > 1) return { bookId, chapter: chapter - 1 };
  return bookId > 1 ? { bookId: bookId - 1, chapter: bible[bookId - 2].chapters } : null;
}

const title = ({ bookId, chapter }: Place) => `${bible[bookId - 1].name} ${chapter}`;
const here = { bookId: 43, chapter: 21 };
console.log(title(previousChapter(here)!), "←", title(here), "→", title(nextChapter(here)!)); // → John 20 ← John 21 → Acts 1
console.log(nextChapter({ bookId: 66, chapter: 22 })); // → null
const chapter = bsbJohn.chapterHtml(21, options);
console.log(chapter.length > 0, bsbJohn.chapterHtml(22, options)); // → true
```

```python
bible = topos.books()


def next_chapter(book_id: int, chapter: int) -> tuple[int, int] | None:
    """The chapter after this one, into the next book after a book's last; None after Revelation 22"""
    if chapter < bible[book_id - 1].chapters:
        return book_id, chapter + 1
    return (book_id + 1, 1) if book_id < len(bible) else None


def previous_chapter(book_id: int, chapter: int) -> tuple[int, int] | None:
    """The chapter before this one; None before Genesis 1"""
    if chapter > 1:
        return book_id, chapter - 1
    return (book_id - 1, bible[book_id - 2].chapters) if book_id > 1 else None


title = lambda book_id, chapter: f"{bible[book_id - 1].name} {chapter}"
print(title(*previous_chapter(43, 21)), "←", title(43, 21), "→", title(*next_chapter(43, 21)))  # → John 20 ← John 21 → Acts 1
print(next_chapter(66, 22))  # → None
chapter = bsb_john.chapter_html(21, options)
print(len(chapter) > 0, repr(bsb_john.chapter_html(22, options)))  # → True ''
```

## A verse for the day

A verse for the day is best chosen from a list of your own: drawn from the whole Bible, most
days would land in a genealogy or a census. Index the list by the day's number, so every reader
sees the same verse on the same day. For a verse at random from anywhere, topos's `verseCount`
gives each chapter's verses.

```ts
const DAILY = ["Psalm 23:1", "John 3:16", "Romans 8:28", "Philippians 4:6-7", "Isaiah 40:31", "Proverbs 3:5-6", "Matthew 11:28"];

/** The verse for a day: the same for everyone that day (in UTC) */
function verseOfTheDay(day: Date) {
  const number = Math.floor(day.getTime() / 86_400_000);
  return topos.parse(DAILY[number % DAILY.length], BookStyle.Name)!;
}

const today = verseOfTheDay(new Date("2026-10-20"));
const todaysBook = (await book("fetch:eng_bsb", today.bookId))!;
console.log(today.reference, todaysBook.text(today, { ...options, headings: false })[0]);
// → Proverbs 3:5-6 Trust in the LORD with all your heart,
// → and lean not on your own understanding;
// → …

/** A verse anywhere in the Bible, by topos's numbering */
function randomVerse() {
  const { id, chapters } = bible[Math.floor(Math.random() * bible.length)];
  const chapter = 1 + Math.floor(Math.random() * chapters);
  const verse = 1 + Math.floor(Math.random() * topos.verseCount(id, chapter)!);
  return topos.passageFromVerses(id, [{ chapter, verse }])!;
}
console.log(typeof randomVerse().reference); // → string
```

```python
import datetime
import random

DAILY = ["Psalm 23:1", "John 3:16", "Romans 8:28", "Philippians 4:6-7", "Isaiah 40:31", "Proverbs 3:5-6", "Matthew 11:28"]


def verse_of_the_day(day: datetime.date):
    """The verse for a day: the same for everyone that day"""
    number = (day - datetime.date(1970, 1, 1)).days
    return topos.parse(DAILY[number % len(DAILY)], BookStyle.NAME)


today = verse_of_the_day(datetime.date(2026, 10, 20))
todays_book = book("fetch:eng_bsb", today.book_id)
print(today.reference, todays_book.text(today, dataclasses.replace(options, headings=False))[0])
# → Proverbs 3:5-6 Trust in the LORD with all your heart,
# → and lean not on your own understanding;
# → …


def random_verse():
    """A verse anywhere in the Bible, by topos's numbering"""
    info = random.choice(bible)
    chapter = random.randint(1, info.chapters)
    verse = random.randint(1, topos.verse_count(info.id, chapter))
    return topos.passage_from_verses(info.id, [ChapterVerse(chapter=chapter, verse=verse)])


print(type(random_verse().reference).__name__)  # → str
```

Count days the same way everywhere your readers are (here, days since 1970 in UTC), or two
of them will see different verses. Poetry keeps its lines: a verse's text has a newline where
each line of it starts.

Weighting a random verse by each book's length (rather than each book equally) means summing
`verseCount` over every chapter once: 31,102 verses in topos's numbering.

## In Rust

logoxide-text writes passages: `passage::text` as plain text in a `Style` (verse and chapter
labels, what's hidden, templates for the rest), and logoxide-markup's `HtmlRenderer` as HTML,
the same HTML the bindings write (behind its `html` feature: `cargo add logoxide-markup
--features html`). A reference is parsed with `bible_data()`, the numbering
logoxide-text reads with, and written back with topos's `FormatOptions`. The book is read as in
[Bibles in your app](../apps.md#a-book-at-a-time).

```rust
use logoxide_markup::{Filter, HtmlOptions, HtmlRenderer, Kind, verses::{passage_ranges, slice}};
use logoxide_text::{
    Book,
    passage::{Style, text},
    topos_bible::segments::formatter::FormatOptions,
};

/// A passage of `book` as HTML, a string for each of its parts
fn html(book: &Book, passage: &logoxide_text::topos_bible::segments::Passage, notes: bool) -> Vec<String> {
    let mut filter = Filter::all().drop(Kind::Metadata);
    if !notes {
        filter = filter.drop(Kind::Note);
    }
    // One renderer for the passage, so words of Jesus across verses stay marked
    let mut renderer = HtmlRenderer::with_options(HtmlOptions { class_prefix: "lx-".into(), filter });
    passage_ranges(&book.spans, passage)
        .into_iter()
        .map(|r| renderer.render(&slice(&book.nodes, &book.spans[r.start].start, Some(&book.spans[r.end - 1].end))))
        .collect()
}

fn show(book: &Book) -> Result<()> {
    let data = logoxide_text::bible_data();
    let passage = data.parse("jn 3:16-17").ok_or("not a reference")?;
    let citation = FormatOptions::default().passage(&passage, data).unwrap_or_default();
    let style = Style { verse_label: Some("{verse}".into()), chapter_label: None, ..Style::reading() };
    let plain = text(book, &passage, &style, "John", None).ok_or("not in this book")?;
    println!("{} ({citation}, BSB)", plain.text); // 16 For God so loved the world … through Him. (John 3:16-17, BSB)
    println!("{}", html(book, &passage, false).join("\n")); // <p class="lx-p" data-style="p"> …
    Ok(())
}
```

`Style::reading()` is verse numbers as `[16]`, chapters as `John 3`, headings shown, and notes
hidden; `passage::render` writes a whole book with where each verse is in it, for a reader that
scrolls through a book rather than showing passages.
