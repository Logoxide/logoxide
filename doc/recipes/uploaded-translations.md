# Translations your user uploads

Once an app reads the Bibles its user uploads ([Files your user brings](../apps.md#files-your-user-brings)),
the user soon wants to fix what they called one, add the books it lacked, correct a book, or take
one out. Three things make that easy, as [scripture-hash](https://github.com/MasterTemple/scripture-hash),
[scripture-retention](https://github.com/MasterTemple/scripture-retention), and
[scripture-slinger](https://github.com/MasterTemple/scripture-slinger) found:

- **An id of its own**, given once and never shown or changed. Keep its books under the id, not
  its name or abbreviation, and renaming it changes nothing kept. (scripture-slinger's ids were its
  abbreviations at first, so renaming meant moving every book.)
- **Its files, as they came, an upload at a time**, with the books read from each. Then a book
  read by an earlier release of logoxide can be read again, and books added later needn't be read
  with the ones there already. `openBible` refuses a book given twice in one format, so reading
  the new files alone is also what lets a book uploaded again replace the one there.
- **The abbreviations it had**, so references written with an old one (`3 John 1 MT` in a note)
  still name it.

This starts from the helpers in [Bibles in your app](../apps.md#setting-up) (`lx`, `topos`). The
examples run in order, as one program in each language, with books made by hand in USFM.

## What to keep

A translation, each upload's files and the books read from them, and each book's verses with
the `textVersion()` of the logoxide that read them. In an app these go in IndexedDB (which keeps
a `Uint8Array` as it is), a database, or files; here they're maps.

```ts
import { BookStyle, OffsetUnit, ToposTranslations, type PackageFile } from "logoxide";

/** A translation the user uploaded, as the app keeps it */
interface Uploaded {
  id: string; // given once and never changed: its books are kept under it
  name: string;
  abbreviation: string;
  formerly: string[]; // abbreviations it had, still understood in what was written with them
  copyright: string | null;
  uploads: Upload[]; // each upload's files, and the books read from them
}

interface Upload {
  files: PackageFile[];
  books: number[];
}

/** A book's verses, as the logoxide that read them read them */
interface Kept {
  textVersion: string;
  verses: { chapter: number; verse: number; text: string }[];
}

// In an app, IndexedDB (both hold Uint8Arrays as they are), a database, or files
const translations = new Map<string, Uploaded>();
const kept = new Map<string, Kept>(); // by `${id}/${bookId}`

/** "RT", "rt", and "R.T." are one abbreviation */
const fold = (abbreviation: string) => abbreviation.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");

/** Another translation with this abbreviation */
const taken = (abbreviation: string, except?: string) =>
  [...translations.values()].find((t) => t.id !== except && fold(t.abbreviation) === fold(abbreviation));

/** Reads files and keeps their books' verses under `id` (only the books in `only`, if given); the books kept */
function keep(id: string, files: PackageFile[], only?: number[]): number[] {
  const bible = lx.openBible(files); // a NotFound error when no file is a book
  const books = [...bible.books()].filter((b) => !only || only.includes(b));
  for (const bookId of books) {
    const book = bible.book(bookId);
    const verses = book.verseTexts().map(({ chapter, verse, text }) => ({ chapter, verse, text }));
    kept.set(`${id}/${bookId}`, { textVersion: lx.textVersion(), verses });
    book.dispose();
  }
  bible.dispose();
  return books;
}
```

```python
import re
from dataclasses import dataclass, field

from logoxide import BookStyle, OffsetUnit, ToposTranslations


@dataclass
class Upload:
    files: list[PackageFile]
    books: list[int]


@dataclass
class Uploaded:
    """A translation the user uploaded, as the app keeps it"""
    id: str  # given once and never changed: its books are kept under it
    name: str
    abbreviation: str
    formerly: list[str] = field(default_factory=list)  # abbreviations it had, still understood in what was written with them
    copyright: str | None = None
    uploads: list[Upload] = field(default_factory=list)  # each upload's files, and the books read from them


@dataclass
class Kept:
    """A book's verses, as the logoxide that read them read them"""
    text_version: str
    verses: list[tuple[int, int, str]]  # chapter, verse, text


# In an app, a database or files
translations: dict[str, Uploaded] = {}
kept: dict[tuple[str, int], Kept] = {}  # by (id, book_id)


def fold(abbreviation: str) -> str:
    """ "RT", "rt", and "R.T." are one abbreviation"""
    return re.sub(r"[\W_]+", "", abbreviation.lower())


def taken(abbreviation: str, but: str | None = None) -> Uploaded | None:
    """Another translation with this abbreviation"""
    return next((t for t in translations.values() if t.id != but and fold(t.abbreviation) == fold(abbreviation)), None)


def keep(id: str, files: list[PackageFile], only: list[int] | None = None) -> list[int]:
    """Reads files and keeps their books' verses under `id` (only the books in `only`, if given); the books kept"""
    bible = lx.open_bible(files)  # a NotFound error when no file is a book
    books = [b for b in bible.books() if only is None or b in only]
    for book_id in books:
        verses = [(v.chapter, v.verse, v.text) for v in bible.book(book_id).verse_texts()]
        kept[(id, book_id)] = Kept(lx.text_version(), verses)
    return books
```

Keep the verses as your app uses them: [From a book to verses](../apps.md#from-a-book-to-verses)
has bridged verses and verses in parts, and the same section of the guide says which of the
files a user gave to tell them were passed over, and which chapters a book lacks.

## Adding one

The id here is made from the name the translation was first given (`upload:mytranslation`), as
scripture-hash's are, and stays when the name changes; a random one (`crypto.randomUUID()`,
`uuid.uuid4()`) does as well. Two of the user's translations can't share an abbreviation, letter
case and punctuation aside, or a reference naming one would be ambiguous; check your app's own
translations' abbreviations too.

```ts
/** A new translation from the files a user uploaded */
function add(name: string, abbreviation: string, files: PackageFile[]): Uploaded {
  const other = taken(abbreviation);
  if (other) throw new Error(`${other.name} is ${other.abbreviation} already`);
  const base = `upload:${fold(name) || "bible"}`;
  let id = base;
  for (let n = 2; translations.has(id); n++) id = `${base}-${n}`;
  const t: Uploaded = { id, name, abbreviation, formerly: [], copyright: null, uploads: [] };
  t.uploads.push({ files, books: keep(id, files) });
  translations.set(id, t);
  return t;
}

const usfm = (text: string) => new TextEncoder().encode(text);
const mine = add("My Translation", "MT", [
  { path: "3JN.usfm", bytes: usfm("\\id 3JN\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n") },
  { path: "JUD.usfm", bytes: usfm("\\id JUD\n\\c 1\n\\p\n\\v 1 Jude, a servent of Jesus Christ.\n") },
]);
console.log(mine.id, mine.uploads.map((u) => u.books)); // → upload:mytranslation [ [ 64, 65 ] ]
```

```python
def add(name: str, abbreviation: str, files: list[PackageFile]) -> Uploaded:
    """A new translation from the files a user uploaded"""
    if other := taken(abbreviation):
        raise ValueError(f"{other.name} is {other.abbreviation} already")
    base = f"upload:{fold(name) or 'bible'}"
    id, n = base, 2
    while id in translations:
        id, n = f"{base}-{n}", n + 1
    t = Uploaded(id, name, abbreviation)
    t.uploads.append(Upload(files, keep(id, files)))
    translations[id] = t
    return t


mine = add("My Translation", "MT", [
    PackageFile(path="3JN.usfm", bytes=b"\\id 3JN\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n"),
    PackageFile(path="JUD.usfm", bytes=b"\\id JUD\n\\c 1\n\\p\n\\v 1 Jude, a servent of Jesus Christ.\n"),
])
print(mine.id, [u.books for u in mine.uploads])  # → upload:mytranslation [[64, 65]]
```

In a browser, a `File` the user chose is `{ path: file.name, bytes: new Uint8Array(await file.arrayBuffer()) }`.

## Renaming it

A name, an abbreviation, and a copyright line are all a rename touches. The abbreviation it had
goes in `formerly`, and topos is given it as another name for the translation, so `3 John 1 MT`
in a note written before still finds it, until another translation takes `MT`. What the app
recorded by id (a reading position, a chosen translation, progress) stays attached; what it
recorded by abbreviation (a history line saying `MT`) keeps saying it, which is right for history.

```ts
/** Renames a translation; nothing kept under its id changes */
function rename(id: string, change: { name?: string; abbreviation?: string; copyright?: string | null }) {
  const t = translations.get(id)!;
  const abbreviation = change.abbreviation?.trim();
  if (abbreviation && fold(abbreviation) !== fold(t.abbreviation)) {
    const other = taken(abbreviation, id);
    if (other) throw new Error(`${other.name} is ${other.abbreviation} already`);
    t.formerly = [...t.formerly.filter((a) => fold(a) !== fold(abbreviation)), t.abbreviation];
    t.abbreviation = abbreviation;
  }
  if (change.name?.trim()) t.name = change.name.trim();
  if (change.copyright !== undefined) t.copyright = change.copyright;
}

/** The translations' names for topos, with the abbreviations they had and no one else has now */
function names(): ToposTranslations {
  const now = new Set([...translations.values()].map((t) => fold(t.abbreviation)));
  let names = ToposTranslations.create();
  for (const t of translations.values()) {
    names = names.add(t.abbreviation, [t.name, ...t.formerly.filter((a) => !now.has(fold(a)))]);
  }
  return names;
}

rename(mine.id, { name: "Revised Translation", abbreviation: "RT", copyright: "Mine, 2026" });
console.log(mine.abbreviation, mine.formerly, kept.has(`${mine.id}/64`)); // → RT [ 'MT' ] true
try {
  add("Another", "r.t.", [{ path: "PHM.usfm", bytes: usfm("\\id PHM\n\\c 1\n\\p\n\\v 1 Paul.\n") }]);
} catch (error) {
  console.log(String(error)); // → Error: Revised Translation is RT already
}
```

```python
def rename(id: str, name: str | None = None, abbreviation: str | None = None, copyright: str | None = None):
    """Renames a translation; nothing kept under its id changes"""
    t = translations[id]
    if abbreviation and abbreviation.strip() and fold(abbreviation) != fold(t.abbreviation):
        if other := taken(abbreviation, id):
            raise ValueError(f"{other.name} is {other.abbreviation} already")
        t.formerly = [a for a in t.formerly if fold(a) != fold(abbreviation)] + [t.abbreviation]
        t.abbreviation = abbreviation.strip()
    if name and name.strip():
        t.name = name.strip()
    if copyright is not None:
        t.copyright = copyright


def names() -> ToposTranslations:
    """The translations' names for topos, with the abbreviations they had and no one else has now"""
    now = {fold(t.abbreviation) for t in translations.values()}
    names = ToposTranslations.create()
    for t in translations.values():
        names = names.add(t.abbreviation, [t.name] + [a for a in t.formerly if fold(a) not in now])
    return names


rename(mine.id, name="Revised Translation", abbreviation="RT", copyright="Mine, 2026")
print(mine.abbreviation, mine.formerly, (mine.id, 64) in kept)  # → RT ['MT'] True
try:
    add("Another", "r.t.", [PackageFile(path="PHM.usfm", bytes=b"\\id PHM\n\\c 1\n\\p\n\\v 1 Paul.\n")])
except ValueError as error:
    print(error)  # → Revised Translation is RT already
```

Build `names()` again after every rename, add, and removal; [References that name your
translations](../apps.md#references-that-name-your-translations) adds each book's own verse
counts to it.

## Adding and replacing books

Read only the new files, and keep their books under the same id: a book the translation had is
replaced (here, Jude with its spelling corrected), and the others are added. Each book is then
read from the last upload that has it, so take the replaced books out of the earlier uploads' lists,
and let go of an upload left with none (its files aren't needed again).

```ts
/** Adds the books in `files` to a translation, replacing any it has; the books added */
function addBooks(id: string, files: PackageFile[]): number[] {
  const t = translations.get(id)!;
  const added = keep(id, files);
  // Each book is read from the last upload that has it; an upload left with none is let go
  const earlier = t.uploads.map((u) => ({ ...u, books: u.books.filter((b) => !added.includes(b)) }));
  t.uploads = [...earlier.filter((u) => u.books.length), { files, books: added }];
  return added;
}

/** Takes a book out of a translation */
function removeBook(id: string, bookId: number) {
  const t = translations.get(id)!;
  const left = t.uploads.map((u) => ({ ...u, books: u.books.filter((b) => b !== bookId) }));
  t.uploads = left.filter((u) => u.books.length);
  kept.delete(`${id}/${bookId}`);
}

addBooks(mine.id, [
  { path: "JUD.usfm", bytes: usfm("\\id JUD\n\\c 1\n\\p\n\\v 1 Jude, a servant of Jesus Christ.\n") },
  { path: "PHM.usfm", bytes: usfm("\\id PHM\n\\c 1\n\\p\n\\v 1 Paul, a prisoner of Christ Jesus.\n") },
]);
console.log(mine.uploads.map((u) => u.books), kept.get(`${mine.id}/65`)!.verses[0].text); // → [ [ 64 ], [ 57, 65 ] ] Jude, a servant of Jesus Christ.

const found = topos.parseWithTranslation("3 John 1:1 MT", OffsetUnit.Utf16, names(), BookStyle.Name)!;
console.log(found.passage.reference, found.translation?.name); // → 3 John 1:1 RT

removeBook(mine.id, 64);
console.log(mine.uploads.map((u) => u.books), kept.has(`${mine.id}/64`)); // → [ [ 57, 65 ] ] false
```

```python
def add_books(id: str, files: list[PackageFile]) -> list[int]:
    """Adds the books in `files` to a translation, replacing any it has; the books added"""
    t = translations[id]
    added = keep(id, files)
    # Each book is read from the last upload that has it; an upload left with none is let go
    earlier = [Upload(u.files, [b for b in u.books if b not in added]) for u in t.uploads]
    t.uploads = [u for u in earlier if u.books] + [Upload(files, added)]
    return added


def remove_book(id: str, book_id: int):
    """Takes a book out of a translation"""
    t = translations[id]
    left = [Upload(u.files, [b for b in u.books if b != book_id]) for u in t.uploads]
    t.uploads = [u for u in left if u.books]
    kept.pop((id, book_id), None)


add_books(mine.id, [
    PackageFile(path="JUD.usfm", bytes=b"\\id JUD\n\\c 1\n\\p\n\\v 1 Jude, a servant of Jesus Christ.\n"),
    PackageFile(path="PHM.usfm", bytes=b"\\id PHM\n\\c 1\n\\p\n\\v 1 Paul, a prisoner of Christ Jesus.\n"),
])
print([u.books for u in mine.uploads], kept[(mine.id, 65)].verses[0][2])  # → [[64], [57, 65]] Jude, a servant of Jesus Christ.

found = topos.parse_with_translation("3 John 1:1 MT", OffsetUnit.CHAR, names(), BookStyle.NAME)
print(found.passage.reference, found.translation.name)  # → 3 John 1:1 RT

remove_book(mine.id, 64)
print([u.books for u in mine.uploads], (mine.id, 64) in kept)  # → [[57, 65]] False
```

An upload's books are what `openBible` found in its files, so one file holding a whole Bible (an
OSIS file, a zip) stays while any of its books is read from it. To keep less, `bookFiles()` names
each book's file (in a zip, `bible.zip/JHN.usfm`); `lx.unzip(bytes)` gives the zip's files to keep
the ones still read.

## Reading them again

When a release of logoxide reads files differently, `textVersion()` changes. Read the books kept
by an earlier one again from their uploads' files, each upload for the books still read from it.
Offline, or with an upload's files gone, the old verses are better than none.

```ts
/** Reads again, from their files, the books read by an earlier logoxide; how many were */
function readAgain(id: string): number {
  let read = 0;
  for (const upload of translations.get(id)!.uploads) {
    const stale = upload.books.filter((b) => kept.get(`${id}/${b}`)?.textVersion !== lx.textVersion());
    if (stale.length) read += keep(id, upload.files, stale).length;
  }
  return read;
}

kept.get(`${mine.id}/57`)!.textVersion = "3 0.1.0"; // as if kept by an earlier logoxide
console.log(readAgain(mine.id), kept.get(`${mine.id}/57`)!.textVersion); // → 1 4 0.1.1
```

```python
def read_again(id: str) -> int:
    """Reads again, from their files, the books read by an earlier logoxide; how many were"""
    read = 0
    for upload in translations[id].uploads:
        stale = [b for b in upload.books if (k := kept.get((id, b))) is None or k.text_version != lx.text_version()]
        if stale:
            read += len(keep(id, upload.files, stale))
    return read


kept[(mine.id, 57)].text_version = "3 0.1.0"  # as if kept by an earlier logoxide
print(read_again(mine.id), kept[(mine.id, 57)].text_version)  # → 1 4 0.1.1
```

## In Rust

The same, with `open_loose` over the files in memory, and `text_version()` from logoxide-text's
cache. A translation's names for references are topos-bible's `Translations`
(`logoxide_text::topos_bible::data::translations`).

```rust
use std::collections::HashMap;

use logoxide_text::{MemoryFiles, Source, cache::text_version, passage::verse_texts, source::open_loose};

/// A translation the user uploaded, as the app keeps it
struct Uploaded {
    id: String, // given once and never changed: its books are kept under it
    name: String,
    abbreviation: String,
    formerly: Vec<String>, // abbreviations it had, still understood in what was written with them
    uploads: Vec<Upload>,  // each upload's files, and the books read from them
}

struct Upload {
    files: Vec<(String, Vec<u8>)>,
    books: Vec<u8>,
}

/// A book's verses, as the logoxide that read them read them
struct Kept {
    text_version: String,
    verses: Vec<(u8, u8, String)>, // chapter, verse, text
}

#[derive(Default)]
struct Library {
    translations: HashMap<String, Uploaded>,
    kept: HashMap<(String, u8), Kept>,
}

/// "RT", "rt", and "R.T." are one abbreviation
fn fold(abbreviation: &str) -> String {
    abbreviation.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

impl Library {
    /// Another translation with this abbreviation
    fn taken(&self, abbreviation: &str, except: &str) -> Option<&Uploaded> {
        (self.translations.values()).find(|t| t.id != except && fold(&t.abbreviation) == fold(abbreviation))
    }

    /// Reads files and keeps their books' verses under `id` (only the books in `only`, if given); the books kept
    fn keep(&mut self, id: &str, files: &[(String, Vec<u8>)], only: Option<&[u8]>) -> Result<Vec<u8>> {
        let bible = open_loose(&MemoryFiles::with(files.iter().cloned()), "")?; // an error when no file is a book
        let mut books = vec![];
        for book_id in bible.books().into_iter().filter(|b| only.is_none_or(|only| only.contains(&b.0))) {
            let verses = verse_texts(&bible.book(book_id)?).into_iter().map(|v| (v.chapter, v.verse, v.text));
            let kept = Kept { text_version: text_version(), verses: verses.collect() };
            self.kept.insert((id.to_string(), book_id.0), kept);
            books.push(book_id.0);
        }
        Ok(books)
    }

    /// A new translation from the files a user uploaded; its id
    fn add(&mut self, name: &str, abbreviation: &str, files: Vec<(String, Vec<u8>)>) -> Result<String> {
        if let Some(other) = self.taken(abbreviation, "") {
            return Err(format!("{} is {} already", other.name, other.abbreviation).into());
        }
        let base = format!("upload:{}", fold(name));
        let mut id = base.clone();
        for n in 2.. {
            if !self.translations.contains_key(&id) {
                break;
            }
            id = format!("{base}-{n}");
        }
        let books = self.keep(&id, &files, None)?;
        let (name, abbreviation, formerly) = (name.to_string(), abbreviation.to_string(), vec![]);
        let uploads = vec![Upload { files, books }];
        self.translations.insert(id.clone(), Uploaded { id: id.clone(), name, abbreviation, formerly, uploads });
        Ok(id)
    }

    /// Gives a translation another abbreviation; nothing kept under its id changes
    fn rename(&mut self, id: &str, abbreviation: &str) -> Result<()> {
        if let Some(other) = self.taken(abbreviation, id) {
            return Err(format!("{} is {} already", other.name, other.abbreviation).into());
        }
        let t = self.translations.get_mut(id).ok_or("no such translation")?;
        if fold(abbreviation) != fold(&t.abbreviation) {
            t.formerly.retain(|a| fold(a) != fold(abbreviation));
            t.formerly.push(std::mem::replace(&mut t.abbreviation, abbreviation.to_string()));
        }
        Ok(())
    }

    /// Adds the books in `files` to a translation, replacing any it has; the books added
    fn add_books(&mut self, id: &str, files: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>> {
        let added = self.keep(id, &files, None)?;
        let t = self.translations.get_mut(id).ok_or("no such translation")?;
        // Each book is read from the last upload that has it; an upload left with none is let go
        for upload in &mut t.uploads {
            upload.books.retain(|b| !added.contains(b));
        }
        t.uploads.retain(|u| !u.books.is_empty());
        t.uploads.push(Upload { files, books: added.clone() });
        Ok(added)
    }

    /// Reads again, from their files, the books read by an earlier logoxide; how many were
    fn read_again(&mut self, id: &str) -> Result<usize> {
        let now = text_version();
        let mut stale = vec![];
        for upload in &self.translations.get(id).ok_or("no such translation")?.uploads {
            let kept = |b: &&u8| self.kept.get(&(id.to_string(), **b)).is_some_and(|k| k.text_version == now);
            let books: Vec<u8> = upload.books.iter().filter(|b| !kept(b)).copied().collect();
            if !books.is_empty() {
                stale.push((upload.files.clone(), books));
            }
        }
        let mut read = 0;
        for (files, books) in stale {
            read += self.keep(id, &files, Some(&books))?.len();
        }
        Ok(read)
    }
}

fn uploads() -> Result<()> {
    let usfm = |path: &str, text: &str| (path.to_string(), text.as_bytes().to_vec());
    let mut library = Library::default();
    let id = library.add("My Translation", "MT", vec![
        usfm("3JN.usfm", "\\id 3JN\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n"),
        usfm("JUD.usfm", "\\id JUD\n\\c 1\n\\p\n\\v 1 Jude, a servent of Jesus Christ.\n"),
    ])?;
    library.rename(&id, "RT")?;
    library.add_books(&id, vec![
        usfm("JUD.usfm", "\\id JUD\n\\c 1\n\\p\n\\v 1 Jude, a servant of Jesus Christ.\n"),
        usfm("PHM.usfm", "\\id PHM\n\\c 1\n\\p\n\\v 1 Paul, a prisoner of Christ Jesus.\n"),
    ])?;
    let t = &library.translations[&id];
    let books: Vec<_> = t.uploads.iter().map(|u| u.books.clone()).collect();
    let jude = &library.kept[&(id.clone(), 65)].verses[0].2;
    println!("{id} {} {:?} {books:?} {jude}", t.abbreviation, t.formerly); // upload:mytranslation RT ["MT"] [[64], [57, 65]] Jude, a servant of Jesus Christ.
    library.kept.get_mut(&(id.clone(), 57)).unwrap().text_version = "3 0.1.0".into();
    println!("{}", library.read_again(&id)?); // 1
    Ok(())
}
```

## In the apps

- **scripture-hash and scripture-retention** keep each book's verses in IndexedDB under the
  translation's id (`upload:my-translation`), and the translation (name, abbreviation, copyright
  line, books) in the app's settings. A pencil beside an uploaded translation opens its edit
  view: the name and abbreviation, the copyright line, and a drop zone for more books (a book
  uploaded again replaces the one it has). They don't keep the files yet, so an upload can't
  be read again by a newer logoxide; this recipe keeps them.
- **scripture-slinger** keeps each book's file in Cache Storage under the translation's id, and a
  list of the user's translations beside them. Its ids were made from abbreviations, so a
  translation keeps the id it was first given (`my-mt`) and one added later under that
  abbreviation gets another (`my-mt-2`); a rename to an abbreviation another of the user's
  translations has is refused. Adding books finds the translation by its abbreviation, as typed,
  letter case and punctuation aside.
