# Commentaries, dictionaries, and cross-references

Beside a passage: what a commentary says on it, what its words are in Greek or Hebrew, and where
else Scripture speaks of the same thing. The publishers' lists logoxide reads have hundreds of
these, all public domain or free: CrossWire's SWORD repository (Matthew Henry, the Treasury of
Scripture Knowledge, Strong's Greek and Hebrew dictionaries, the ISBE, Torrey's topics) and CCEL's
commentaries (Calvin's among them). Each is an import made into a package and opened, and its kind
says what it holds: notes on verses, entries by key, or cross-references.

The examples run in order, starting from the helpers in
[Bibles in your app](../apps.md#setting-up) (`lx`, `topos`, `get`). They download about 12 MB
from crosswire.org and ccel.org. The bindings' README has each method in full, under
"Commentaries, dictionaries, and cross-references".

- [Finding them](#finding-them)
- [Opening one](#opening-one)
- [A commentary's notes on a passage](#a-commentarys-notes-on-a-passage)
- [References in a note, as links](#references-in-a-note-as-links)
- [A word's Strong's number and its entry](#a-words-strongs-number-and-its-entry)
- [A topic's entry](#a-topics-entry)
- [Cross-references](#cross-references)
- [What doesn't work yet](#what-doesnt-work-yet)

## Finding them

CrossWire's list is a SWORD repository's `mods.d.tar.gz` (200 KB, read as bytes);
`swordCatalogUrl("crosswire")` is where it is. CCEL lists its commentaries on a page of their own.
Each entry's `kind` says what it is (`PackageKind.Commentary`, `Lexicon` for dictionaries,
`CrossReferences`), and `license` what its publisher says of it: show it beside what you show.
CCEL's list doesn't say; the package made from a download does (`info().license`, below).

```ts
import { PackageKind } from "logoxide";

const SWORD = lx.swordCatalogUrl("crosswire")!;
const sword = lx.parseCatalogBytes(SWORD, await get([SWORD]));
const COMMENTARIES = "https://ccel.org/index/genre/gen-COM";
const ccel = lx.parseCatalogBytes(COMMENTARIES, await get([COMMENTARIES]));
console.log(sword.count(), ccel.count()); // → 386 203

const mhcc = sword.findImport("sword:crosswire/MHCC")!;
console.log(mhcc.name, mhcc.kind === PackageKind.Commentary, mhcc.license); // → Matthew Henry's Concise Commentary on the Whole Bible true LicenseRef-PublicDomain
const strongs = sword.findImport("sword:crosswire/StrongsGreek")!;
console.log(strongs.name, strongs.kind === PackageKind.Lexicon, strongs.keys); // → Strong's Greek Bible Dictionary true strongs-greek
for (const e of ccel.search("romans", 0).filter((e) => e.kind === PackageKind.Commentary).slice(0, 2)) {
  console.log(e.id, e.name, e.license);
}
// → ccel-barnes-romans Notes on the New Testament Explanatory and Practical: Romans null
// → ccel-newell-romans Romans Verse-by-Verse null
```

```python
from logoxide import PackageKind

SWORD = lx.sword_catalog_url("crosswire")
sword = lx.parse_catalog_bytes(SWORD, get([SWORD]))
COMMENTARIES = "https://ccel.org/index/genre/gen-COM"
ccel = lx.parse_catalog_bytes(COMMENTARIES, get([COMMENTARIES]))
print(sword.count(), ccel.count())  # → 386 203

mhcc = sword.find_import("sword:crosswire/MHCC")
print(mhcc.name, mhcc.kind == PackageKind.COMMENTARY, mhcc.license)  # → Matthew Henry's Concise Commentary on the Whole Bible True LicenseRef-PublicDomain
strongs = sword.find_import("sword:crosswire/StrongsGreek")
print(strongs.name, strongs.kind == PackageKind.LEXICON, strongs.keys)  # → Strong's Greek Bible Dictionary True strongs-greek
for e in [e for e in ccel.search("romans", 0) if e.kind == PackageKind.COMMENTARY][:2]:
    print(e.id, e.name, e.license)
# → ccel-barnes-romans Notes on the New Testament Explanatory and Practical: Romans None
# → ccel-newell-romans Romans Verse-by-Verse None
```

```rust
use logoxide::{
    catalog::{Catalog, CatalogEntry},
    manifest::PackageKind,
};

fn sword_and_ccel() -> Result<(Catalog, Catalog)> {
    // logoxide::catalog::sword_catalog_url("crosswire") in the bindings is this
    let sword_url = "https://crosswire.org/ftpmirror/pub/sword/raw/mods.d.tar.gz";
    let sword = logoxide_import::catalog::parse(sword_url, &get(&[sword_url.into()])?)?;
    let ccel_url = "https://ccel.org/index/genre/gen-COM";
    let ccel = logoxide_import::catalog::parse(ccel_url, &get(&[ccel_url.into()])?)?;
    let mhcc = sword.find_import("sword:crosswire/MHCC").ok_or("no MHCC")?;
    println!("{} {}", mhcc.name, mhcc.kind == PackageKind::Commentary); // Matthew Henry's Concise Commentary on the Whole Bible true
    Ok((sword, ccel))
}
```

The catalogs list SWORD modules of every license: check `license` (and `rights`) before
offering one, since many are under copyright and allow only personal use. A few SWORD
repositories other than CrossWire's are served over plain HTTP; `lx.isInsecureUrl(url)` says so,
for an app that won't download without encryption.

## Opening one

The same three steps as any import: `importDownloads` says what to download (a SWORD module's
`.zip`, a CCEL work's ThML), `importDownloaded` makes a package's files from the bytes, and
`openPackage` opens them. Making the package is the slow part (Strong's Greek takes three seconds
in WebAssembly), so keep the files it gives and open those next time, with the cache files the
package wrote (`cacheWrites()`), as [Bibles in your app](../apps.md#packages-and-vaults) says.

```ts
import { type CatalogEntry, type LogoxidePackage } from "logoxide";

const features = { features: [], defaultFeatures: true };

/** An import's package: downloaded, made, and opened */
async function openImport(entry: CatalogEntry): Promise<LogoxidePackage> {
  const files: PackageFile[] = [];
  for (const d of lx.importDownloads(entry)) files.push({ path: d.name, bytes: await get(d.urls) });
  const made = lx.importDownloaded(entry, files); // keep these: they're the package
  return lx.openPackage(made, features, []);
}

const henry = await openImport(mhcc);
const calvin = await openImport(ccel.findImport("ccel:calvin/calcom38")!);
console.log(henry.format(), calvin.info().name, calvin.info().license, [...calvin.books()]); // → osis Commentary on Romans LicenseRef-PublicDomain [ 45 ]
```

```python
from logoxide import CatalogEntry, LogoxidePackage, PackageFeatures

features = PackageFeatures(features=[], default_features=True)


def open_import(entry: CatalogEntry) -> LogoxidePackage:
    """An import's package: downloaded, made, and opened"""
    files = [PackageFile(path=d.name, bytes=get(d.urls)) for d in lx.import_downloads(entry)]
    made = lx.import_downloaded(entry, files)  # keep these: they're the package
    return lx.open_package(made, features, [])


henry = open_import(mhcc)
calvin = open_import(ccel.find_import("ccel:calvin/calcom38"))
print(henry.format(), calvin.info().name, calvin.info().license, list(calvin.books()))  # → osis Commentary on Romans LicenseRef-PublicDomain [45]
```

In Rust, `logoxide_import::build` makes the package into any `Storage` (memory here, or a folder),
and `logoxide::package::Package` opens it; its `source()` has the notes, entries, and
cross-references.

```rust
use std::sync::Arc;

use logoxide::{
    package::{Features, Package},
    storage::MemoryStorage,
};

/// An import's package: downloaded, made, and opened
fn open_import(entry: &CatalogEntry) -> Result<Package> {
    let mut files = vec![];
    for d in logoxide_import::downloads(entry)? {
        files.push((d.name.clone(), get(&d.urls)?));
    }
    let made = Arc::new(MemoryStorage::new());
    logoxide_import::build(entry, &files, made.as_ref())?;
    Ok(Package::open(made, Features::default())?)
}
```

## A commentary's notes on a passage

`notes(passage, options, stretch)` gives each of a commentary's notes covering any verse of the
passage, as text and HTML, with the verses it's on (`chapter`:`verse` to `lastChapter`:`last`).
Matthew Henry writes a note on a few verses at a time; Calvin, on nearly every verse. With
`stretch`, a note also covers the verses after it up to the next one, so a verse a commentary
skips still finds the note it falls under.

```ts
import { BookStyle } from "logoxide";

const romans828 = topos.parse("Romans 8:28", BookStyle.Name)!;
const options = { notes: false, headings: true, classPrefix: "lx-" };
for (const commentary of [henry, calvin]) {
  for (const note of commentary.notes(romans828, options, true)) {
    console.log(commentary.info().abbreviation, `${note.chapter}:${note.verse}-${note.last}`, note.text.slice(0, 50).replace(/\s+/g, " "));
  }
}
// → MHCC 8:28-31 Verses 28-31 That is good for the saints which doe
// → CALCOM38 8:28-28 28. And we know, etc. He now draws this conclusion
```

```python
from logoxide import BookStyle, BookTextOptions

romans828 = topos.parse("Romans 8:28", BookStyle.NAME)
options = BookTextOptions(notes=False, headings=True, class_prefix="lx-")
for commentary in [henry, calvin]:
    for note in commentary.notes(romans828, options, True):
        print(commentary.info().abbreviation, f"{note.chapter}:{note.verse}-{note.last}", " ".join(note.text[:50].split()))
# → MHCC 8:28-31 Verses 28-31 That is good for the saints which doe
# → CALCOM38 8:28-28 28. And we know, etc. He now draws this conclusion
```

`note.html` is the same note as HTML, with logoxide-markup's classes (each with `classPrefix`),
for a side panel.

```rust
use logoxide_text::{
    markup::text::{TextOptions, plain_text},
    source::book_notes,
};

fn notes(commentary: &Package) -> Result<()> {
    let passage = logoxide_text::bible_data().parse("Romans 8:28").ok_or("not a reference")?;
    let book = commentary.source().book(passage.book)?;
    // `source().notes(&passage)` is the same without stretching
    for note in book_notes(&book, &passage, true) {
        let text = plain_text(&note.nodes, &TextOptions::default());
        println!("{}:{}-{} {}", note.chapter, note.verse, note.last, text.chars().take(50).collect::<String>());
    }
    Ok(())
}
```

## References in a note, as links

The Treasury of Scripture Knowledge is, in SWORD, a commentary whose notes are lists of
references, by the words of the verse they're for. topos finds the references in any text
(`search`), each with where it is, so a reader can make them links; the Treasury's abbreviations
(`Ro 5:8`, `1Jo 4:9`) are read as well as full names. A reference that doesn't name its book
(`35-39`, `5:3,4`) is the note's own book's (Romans 8:35-39, Romans 5:3-4), which topos can't
know: it finds only those that name one.

```ts
import { OffsetUnit } from "logoxide";

const tsk = await openImport(sword.findImport("sword:crosswire/TSK")!);
const [treasury] = tsk.notes(romans828, options, false);
console.log(treasury.text.split("\n").slice(0, 2)); // → [ 'we know.', '35-39; 5:3,4; Ge 50:20; De 8:2,3,16; Ps 46:1,2; Jer 24:5-7; Zec 13:9' ]
const found = topos.search(treasury.text, OffsetUnit.Utf16);
console.log(found.length, found.slice(0, 2).map((m) => [m.passage.reference, treasury.text.slice(m.start, m.end)])); // → 29 [ [ 'Genesis 50:20', 'Ge 50:20' ], [ 'Deuteronomy 8:2,3,16', 'De 8:2,3,16' ] ]
```

```python
from logoxide import OffsetUnit

tsk = open_import(sword.find_import("sword:crosswire/TSK"))
[treasury] = tsk.notes(romans828, options, False)
print(treasury.text.split("\n")[:2])  # → ['we know.', '35-39; 5:3,4; Ge 50:20; De 8:2,3,16; Ps 46:1,2; Jer 24:5-7; Zec 13:9']
found = topos.search(treasury.text, OffsetUnit.CHAR)
print(len(found), [(m.passage.reference, treasury.text[m.start : m.end]) for m in found[:2]])  # → 29 [('Genesis 50:20', 'Ge 50:20'), ('Deuteronomy 8:2,3,16', 'De 8:2,3,16')]
```

In Rust, topos-bible (re-exported as `logoxide_text::topos_bible`) finds them the same way.

## A word's Strong's number and its entry

A SWORD Bible with Strong's numbers (CrossWire's KJV, among others) marks each word with its
number in a `lemma` attribute (`strong:G25`); the tree a book gives (`nodes()`, `verseNodes()`)
has it, and Strong's dictionary has the entry. A dictionary's `entry(key)` finds `G25`, `g25`,
and `G0025` alike, and gives `null` (`None`) for a key it lacks. CrossWire's KJV is under the GPL
(its markup is CrossWire's); the KJV's words are public domain.

```ts
import { NodeKind } from "logoxide";

const kjvStrongs = await openImport(sword.findImport("sword:crosswire/KJV")!);
const strongsGreek = await openImport(strongs);
console.log(kjvStrongs.info().license, strongsGreek.keys().length); // → GPL-2.0-only 5742

/** Each word of a verse the Bible tags, with its Strong's numbers */
function strongsWords(book: LogoxideBook, chapter: number, verse: number) {
  const nodes = book.nodes();
  const at = book.verseNodes().find((v) => v.chapter === chapter && v.verse === verse)!;
  const textOf = (i: number): string =>
    nodes[i].kind === NodeKind.Text ? nodes[i].text ?? "" : nodes.flatMap((n, j) => (n.parent === i ? [textOf(j)] : [])).join("");
  const words = [];
  for (let i = at.start; i < at.end; i++) {
    if (nodes[i].kind !== NodeKind.Word) continue;
    const lemma = nodes[i].attributes.find((a) => a.name === "lemma")?.value ?? "";
    words.push({ word: textOf(i), numbers: [...lemma.matchAll(/strong:([GH]\d+)/g)].map((m) => m[1]) });
  }
  return words;
}

const words = strongsWords(kjvStrongs.book(45), 8, 28);
console.log(words.slice(6, 9).map((w) => `${w.word} ${w.numbers.join(" ")}`)); // → [ 'good G18', 'to them that love G3588 G25', 'God G3588 G2316' ]
// "to them that love" is two Greek words: the article (G3588) and the verb
const love = words[7].numbers.at(-1)!;
const entry = strongsGreek.entry(love, options)!;
console.log(entry.key, entry.headword, entry.text.split("\n")[1].slice(0, 54)); // → G25 ἀγαπάω ajgapavw agapao {ag-ap-ah'-o} perhaps from agan (much)
```

```python
import re

from logoxide import LogoxideBook, NodeKind

kjv_strongs = open_import(sword.find_import("sword:crosswire/KJV"))
strongs_greek = open_import(strongs)
print(kjv_strongs.info().license, len(strongs_greek.keys()))  # → GPL-2.0-only 5742


def strongs_words(book: LogoxideBook, chapter: int, verse: int) -> list[tuple[str, list[str]]]:
    """Each word of a verse the Bible tags, with its Strong's numbers"""
    nodes = book.nodes()
    at = next(v for v in book.verse_nodes() if (v.chapter, v.verse) == (chapter, verse))

    def text_of(i: int) -> str:
        if nodes[i].kind == NodeKind.TEXT:
            return nodes[i].text or ""
        return "".join(text_of(j) for j, n in enumerate(nodes) if n.parent == i)

    words = []
    for i in range(at.start, at.end):
        if nodes[i].kind == NodeKind.WORD:
            lemma = next((a.value for a in nodes[i].attributes if a.name == "lemma"), "")
            words.append((text_of(i), re.findall(r"strong:([GH]\d+)", lemma)))
    return words


words = strongs_words(kjv_strongs.book(45), 8, 28)
print([f"{w} {' '.join(n)}" for w, n in words[6:9]])  # → ['good G18', 'to them that love G3588 G25', 'God G3588 G2316']
# "to them that love" is two Greek words: the article (G3588) and the verb
love = words[7][1][-1]
entry = strongs_greek.entry(love, options)
print(entry.key, entry.headword, entry.text.split("\n")[1][:54])  # → G25 ἀγαπάω ajgapavw agapao {ag-ap-ah'-o} perhaps from agan (much)
```

In Rust, a dictionary's source gives the entry, as markup nodes:

```rust
fn strongs_entry(strongs_greek: &Package) -> Result<()> {
    let entry = strongs_greek.source().entry("g25")?.ok_or("no G25")?;
    let text = plain_text(&entry.nodes, &TextOptions::default());
    println!("{} {:?} {}", entry.key, entry.headword, text.lines().nth(1).unwrap_or("")); // G25 Some("ἀγαπάω") ajgapavw agapao …
    Ok(())
}
```

## A topic's entry

A dictionary keyed by headword (`info().keys` is `headword`) is looked up by its words, as
`keys()` lists them, in any case. Torrey's New Topical Textbook is small; the ISBE (10 MB) is a
full encyclopedia, read the same way.

```ts
const torrey = await openImport(sword.findImport("sword:crosswire/Torrey")!);
console.log(torrey.info().keys, torrey.keys().filter((k) => k.startsWith("LOVE TO"))); // → headword [ 'LOVE TO CHRIST', 'LOVE TO GOD', 'LOVE TO MAN' ]
const topic = torrey.entry("love to god", options)!;
console.log(topic.key, topic.text.split("\n").slice(0, 4)); // → LOVE TO GOD [ '- Commanded', 'De 11:1; Jos 22:5', '- The first great commandment', 'Mt 22:38' ]
```

```python
torrey = open_import(sword.find_import("sword:crosswire/Torrey"))
print(torrey.info().keys, [k for k in torrey.keys() if k.startswith("LOVE TO")])  # → headword ['LOVE TO CHRIST', 'LOVE TO GOD', 'LOVE TO MAN']
topic = torrey.entry("love to god", options)
print(topic.key, topic.text.split("\n")[:4])  # → LOVE TO GOD ['- Commanded', 'De 11:1; Jos 22:5', '- The first great commandment', 'Mt 22:38']
```

Its references are found as the Treasury's are, above.

## Cross-references

A cross-reference package (`kind = "cross-references"`, lines of `from`, `to`, and a weight in
`xref/*.tsv`) gives `crossReferences(verse)`: its lines covering any verse of it, as OSIS, which
topos turns into references to show. None of the publishers' lists has one yet (SWORD's TSK is a
commentary, above); a package of your own, made from OpenBible.info's cross-references or any
other list, is read this way. The bindings' README has an example, and
[doc/entries.md](../entries.md) the format.

## What doesn't work yet

In 0.1.1, importing CrossWire's Easton, Smith, and Nave dictionaries fails with an `Invalid`
error (`key KADESH is also in entries/Easton.xml: keep one`): each lists some headwords twice, and
logoxide refuses a dictionary with a key twice rather than choosing one. Strong's, the ISBE, and
Torrey's import.
