# Searching a Bible's words

A search box over a whole translation: verses with all the words typed, a phrase, and the words
found highlighted in each verse. logoxide doesn't keep a word index itself (topos's `ToposIndex`
indexes the references in many files, such as a vault's notes, not a Bible's words), but it
gives the two pieces one needs: every verse as reading text (`verseTexts`) and the same word
splitting everywhere (`words`), so a search matches whatever the translation's punctuation,
accents, or capitals. The rest is a small inverted index of your own: each word, and the verses
it's in.

This starts from the helpers in [Bibles in your app](../apps.md#setting-up) (`lx`, `topos`,
`get`) and the BSB's catalog entry from [Finding a translation](../apps.md#finding-a-translation)
(`bsb`). The examples run in order, as one program in each language.

## The index

Read every book (here the BSB from fetch.bible, a book a download), and for each verse, add it to
the list of each word it has. `lx.words` splits text at whitespace, dashes, and slashes, and
folds each word: lowercase, accents and other marks dropped, only letters and digits kept, so
`“Éphraïm’s”` is `ephraims`, as is `Ephraim's` typed in a search box.

```ts
/** A verse as the search keeps it */
interface Verse { bookId: number; chapter: number; verse: number; text: string }

const verses: Verse[] = [];
/** Each word, folded, and the verses it's in (their places in `verses`, ascending) */
const index = new Map<string, number[]>();

const usx = await Promise.all(lx.bookDownloads(bsb).map(async (d) => new TextDecoder().decode(await get(d.urls))));
for (const text of usx) {
  const book = LogoxideBook.parse(topos, "usx", text)!;
  const bookId = book.bookId()!;
  for (const v of book.verseTexts()) {
    const at = verses.push({ bookId, chapter: v.chapter, verse: v.verse, text: v.text }) - 1;
    for (const word of new Set(lx.words(v.text))) {
      let list = index.get(word);
      if (!list) index.set(word, (list = []));
      list.push(at);
    }
  }
  book.dispose();
}
console.log(verses.length, index.size); // → 31086 14477
```

```python
import bisect
import re
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass


@dataclass
class Verse:
    """A verse as the search keeps it"""
    book_id: int
    chapter: int
    verse: int
    text: str


verses: list[Verse] = []
index: dict[str, list[int]] = {}  # each word, folded, and the verses it's in (places in `verses`, ascending)

with ThreadPoolExecutor(8) as pool:
    usx = list(pool.map(lambda d: get(d.urls).decode(), lx.book_downloads(bsb)))
for text in usx:
    book = LogoxideBook.parse(topos, "usx", text)
    book_id = book.book_id()
    for v in book.verse_texts():
        verses.append(Verse(book_id, v.chapter, v.verse, v.text))
        for word in set(lx.words(v.text)):
            index.setdefault(word, []).append(len(verses) - 1)
print(len(verses), len(index))  # → 31086 14477
```

In Rust, `logoxide_text::keys::words` is the same function (and `keys::fold` folds without
splitting):

```rust
use std::collections::{HashMap, HashSet};

use logoxide_text::{MemoryFiles, Source, keys::words, passage::verse_texts, source::open_loose};

/// A verse as the search keeps it
struct Verse {
    book: u8,
    chapter: u8,
    verse: u8,
    text: String,
}

/// Every verse of a Bible, and each word (folded) with the verses it's in, ascending
struct Search {
    verses: Vec<Verse>,
    index: HashMap<String, Vec<u32>>,
}

impl Search {
    fn new(bible: &dyn Source) -> Result<Search> {
        let (mut verses, mut index) = (vec![], HashMap::<String, Vec<u32>>::new());
        for id in bible.books() {
            for v in verse_texts(&bible.book(id)?) {
                for word in words(&v.text).into_iter().collect::<HashSet<_>>() {
                    index.entry(word).or_default().push(verses.len() as u32);
                }
                verses.push(Verse { book: id.0, chapter: v.chapter, verse: v.verse, text: v.text });
            }
        }
        Ok(Search { verses, index })
    }
}

fn whole_bsb(fetch_bible: &logoxide::catalog::Catalog) -> Result<Search> {
    let bsb = fetch_bible.find_import("fetch:eng_bsb").ok_or("no BSB")?;
    let mut files = vec![];
    for d in logoxide_import::downloads(bsb)? {
        files.push((format!("{}.usx", d.name), get(&d.urls)?));
    }
    let search = Search::new(&open_loose(&MemoryFiles::with(files), "")?)?;
    println!("{} verses, {} words", search.verses.len(), search.index.len()); // 31086 verses, 14477 words
    Ok(search)
}
```

A verse printed in parts (`20a`, `20b`) is an entry per part, and a bridge (`4-5`) one entry for
both verses: see [From a book to verses](../apps.md#from-a-book-to-verses) for joining them.

## Words and phrases

Verses with every word typed: the shortest of the words' lists, kept where every other list has
it too (the lists are in Bible order, so a binary search finds each). A phrase is those verses
whose words have the phrase's, together and in order.

```ts
/** Whether a list in ascending order has `n` */
function has(list: number[], n: number): boolean {
  let [lo, hi] = [0, list.length];
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (list[mid] < n) lo = mid + 1;
    else hi = mid;
  }
  return list[lo] === n;
}

/** The verses with every word of `query`, in Bible order */
function search(query: string): number[] {
  const words = [...new Set(lx.words(query))];
  if (!words.length) return [];
  const lists = words.map((w) => index.get(w) ?? []).sort((a, b) => a.length - b.length);
  return lists[0].filter((n) => lists.every((list) => has(list, n)));
}

/** The verses with `query`'s words together, in order */
function searchPhrase(query: string): number[] {
  const wanted = lx.words(query);
  return search(query).filter((n) => {
    const words = lx.words(verses[n].text);
    return words.some((_, i) => wanted.every((w, j) => words[i + j] === w));
  });
}

const name = (v: Verse) => `${topos.books()[v.bookId - 1].name} ${v.chapter}:${v.verse}`;
const found = search("loved world");
console.log(found.length, found.slice(0, 3).map((n) => name(verses[n]))); // → 5 [ 'John 3:16', 'John 3:19', 'John 13:1' ]
console.log(searchPhrase("God so loved").map((n) => name(verses[n]))); // → [ 'John 3:16', '1 John 4:11' ]
console.log(search("Éphraïm’s").length, search("ephraims").length); // → 8 8
```

```python
def search(query: str) -> list[int]:
    """The verses with every word of `query`, in Bible order"""
    words = set(lx.words(query))
    if not words:
        return []
    lists = sorted((index.get(w, []) for w in words), key=len)

    def has(lst: list[int], n: int) -> bool:
        i = bisect.bisect_left(lst, n)
        return i < len(lst) and lst[i] == n

    return [n for n in lists[0] if all(has(lst, n) for lst in lists[1:])]


def search_phrase(query: str) -> list[int]:
    """The verses with `query`'s words together, in order"""
    wanted = lx.words(query)
    found = []
    for n in search(query):
        words = lx.words(verses[n].text)
        if any(words[i : i + len(wanted)] == wanted for i in range(len(words))):
            found.append(n)
    return found


def name(v: Verse) -> str:
    return f"{topos.books()[v.book_id - 1].name} {v.chapter}:{v.verse}"


found = search("loved world")
print(len(found), [name(verses[n]) for n in found[:3]])  # → 5 ['John 3:16', 'John 3:19', 'John 13:1']
print([name(verses[n]) for n in search_phrase("God so loved")])  # → ['John 3:16', '1 John 4:11']
print(len(search("Éphraïm’s")), len(search("ephraims")))  # → 8 8
```

```rust
impl Search {
    /// The verses with every word of `query`, in Bible order
    fn find(&self, query: &str) -> Vec<&Verse> {
        let wanted: HashSet<String> = words(query).into_iter().collect();
        let mut lists: Vec<&[u32]> = wanted.iter().map(|w| self.index.get(w).map_or(&[][..], Vec::as_slice)).collect();
        lists.sort_by_key(|l| l.len());
        let Some((first, rest)) = lists.split_first() else { return vec![] };
        (first.iter())
            .filter(|n| rest.iter().all(|l| l.binary_search(n).is_ok()))
            .map(|&n| &self.verses[n as usize])
            .collect()
    }

    /// The verses with `query`'s words together, in order
    fn phrase(&self, query: &str) -> Vec<&Verse> {
        let wanted = words(query);
        (self.find(query).into_iter())
            .filter(|v| words(&v.text).windows(wanted.len()).any(|w| w == wanted.as_slice()))
            .collect()
    }
}
```

Words match whole: `loved` doesn't find `love`. A search box that searches as its user types can
take the last word as the start of a word, from the index's words:

```ts
/** `search`, with the last word of `query` the start of a word */
function searchAsTyped(query: string): number[] {
  const words = lx.words(query);
  const last = words.pop();
  if (!last) return [];
  const starting = [...index.keys()].filter((w) => w.startsWith(last));
  const done = words.length ? new Set(search(words.join(" "))) : null;
  const found = new Set(starting.flatMap((w) => index.get(w)!).filter((n) => !done || done.has(n)));
  return [...found].sort((a, b) => a - b);
}
console.log(searchAsTyped("eternal lif").length, searchAsTyped("eternal life").length); // → 45 45
```

```python
def search_as_typed(query: str) -> list[int]:
    """`search`, with the last word of `query` the start of a word"""
    words = lx.words(query)
    if not words:
        return []
    last = words.pop()
    done = set(search(" ".join(words))) if words else None
    found = {n for w, lst in index.items() if w.startswith(last) for n in lst if done is None or n in done}
    return sorted(found)


print(len(search_as_typed("eternal lif")), len(search_as_typed("eternal life")))  # → 45 45
```

A search box usually takes references too: try `topos.parse(query, BookStyle.Name)` first and go
to the passage it gives (`Jn 3:16` is John 3:16, `ps 23` Psalm 23). A book's name alone parses as
the whole book (`john` is John), so offer that beside the word results rather than in place of
them.

## Highlighting the words found

The words matched are found again in a verse's text by splitting it at the same separators
`words` splits at, and keeping each run from its first letter or digit to its last whose folded
word is wanted. Offsets are in the unit your strings are indexed by: UTF-16 code units in
JavaScript, characters in Python, bytes in Rust.

```ts
/** Where `query`'s words are in a verse's text: offsets in UTF-16 code units, as a string is indexed */
function highlights(text: string, query: string): { start: number; end: number }[] {
  const wanted = new Set(lx.words(query));
  const found = [];
  // Runs between the separators `words` splits at, from their first letter or digit to their last
  for (const m of text.matchAll(/[\p{L}\p{N}](?:[^\s\-‐–—/]*[\p{L}\p{N}\p{M}])?/gu)) {
    if (lx.words(m[0]).some((w) => wanted.has(w))) found.push({ start: m.index, end: m.index + m[0].length });
  }
  return found;
}

const john316 = verses[searchPhrase("God so loved")[0]];
const marks = highlights(john316.text, "loved world");
console.log(marks); // → [ { start: 11, end: 16 }, { start: 21, end: 26 } ]
let html = john316.text;
for (const m of [...marks].reverse()) html = `${html.slice(0, m.start)}<mark>${html.slice(m.start, m.end)}</mark>${html.slice(m.end)}`;
console.log(html); // → For God so <mark>loved</mark> the <mark>world</mark> that He gave His one and only Son, …
```

```python
def highlights(text: str, query: str) -> list[tuple[int, int]]:
    """Where `query`'s words are in a verse's text, in characters, as a str is indexed"""
    wanted = set(lx.words(query))
    # Runs between the separators `words` splits at, from their first letter or digit to their last
    return [
        (m.start(), m.end())
        for m in re.finditer(r"[^\W_](?:[^\s\-‐–—/]*[^\W_])?", text)
        if wanted & set(lx.words(m.group()))
    ]


john316 = verses[search_phrase("God so loved")[0]]
marks = highlights(john316.text, "loved world")
print(marks, [john316.text[a:b] for a, b in marks])  # → [(11, 16), (21, 26)] ['loved', 'world']
```

```rust
/// Where `query`'s words are in `text`, as byte ranges
fn highlights(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    let wanted: HashSet<String> = words(query).into_iter().collect();
    let separator = |c: char| c.is_whitespace() || matches!(c, '-' | '‐' | '–' | '—' | '/');
    let mut found = vec![];
    let mut start = 0;
    for (at, c) in text.char_indices().chain([(text.len(), ' ')]) {
        if !separator(c) {
            continue;
        }
        // The run from its first letter or digit to its last
        let run = &text[start..at];
        if let (Some(a), Some(b)) = (run.find(char::is_alphanumeric), run.rfind(char::is_alphanumeric)) {
            let end = b + run[b..].chars().next().map_or(0, char::len_utf8);
            if words(&run[a..end]).iter().any(|w| wanted.contains(w)) {
                found.push(start + a..start + end);
            }
        }
        start = at + c.len_utf8();
    }
    found
}

fn find_and_highlight(search: &Search) {
    let john316 = search.phrase("God so loved")[0];
    let marked: Vec<&str> = highlights(&john316.text, "loved world").into_iter().map(|r| &john316.text[r]).collect();
    println!("{marked:?}"); // ["loved", "world"]
}
```

## Fast enough, and kept

Measured on the BSB's 66 books (31,086 verses) with the npm package in Node, as a browser's
worker runs it:

| | Time | Size |
|---|---|---|
| Parsing the books (`LogoxideBook.parse`, `verseTexts`) | 1.7 s | |
| Indexing the verses (`words` on each) | 1 s | |
| A search (`search`, `searchPhrase`) | about 1 ms | |
| The verses, as JSON | | 5.3 MB (1.4 MB gzipped) |
| The index, as JSON | | 3.6 MB (1.5 MB gzipped) |
| Reading both back (`JSON.parse`) | 45 ms | |

So build it once, in a worker (see [Setting up](../apps.md#setting-up)), and keep the verses,
with the index or without it (building it again from the verses takes the second). Keep them
under the translation's import name and `lx.textVersion()`, as
[Keeping what you read](../apps.md#keeping-what-you-read) says: a release of logoxide that reads
files differently changes `textVersion`, and verses read by an older one are read again from
their files and indexed again.
A search over several translations is an index each, searched in turn.

The index holds a verse's place in `verses`, not its reference, so a smaller form is easy if it
matters: the lists as differences from the number before (most are small), or one typed array of
all of them with each word's start. The verses' text needn't be kept with it at all if your app
keeps the books' files: a result's reference is enough to read its verse again.
