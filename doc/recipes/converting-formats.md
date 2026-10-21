# Converting formats

A book of the Bible in USFM, USX, or USJ written as another of them: for a tool that takes only
one (a USX reader, a USJ editor, Paratext's USFM), or to keep every book in one format. What's
written is normalized (one layout, attributes in one order), not the publisher's layout, so
converting a file to its own format tidies it. OSIS, ThML, and GBF are read (by `openBible` and
packages) but not converted.

The examples start from the helpers in [Bibles in your app](../apps.md#setting-up) (`lx`, `get`)
and run in order, as one program in each language.

## One book

`usfmToUsx` and `usxToUsfm` go between USFM and USX (through usfm3), `usxToUsj` and `usjToUsx`
between USX and USJ (USX as JSON), and `convertBook` from any of the three into another.

```ts
import { BookFormat } from "logoxide";

const usfm = "\\id 3JN\n\\h 3 John\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n";
const usx = lx.usfmToUsx(usfm);
console.log(usx); // → <?xml version="1.0" encoding="utf-8"?><usx version="3.0"><book code="3JN" style="id"/><para style="h">3 John</para><chapter number="1" style="c" sid="3JN 1"/>…</usx>
const usj = lx.usxToUsj(usx);
console.log(JSON.parse(usj).content[0]); // → { type: 'book', marker: 'id', code: '3JN' }
console.log(lx.usxToUsfm(lx.usjToUsx(usj))); // → \id 3JN … \p \v 1 The elder to Gaius the beloved.
console.log(lx.convertBook(usj, BookFormat.Usj, BookFormat.Usfm) === lx.usxToUsfm(usx)); // → true
```

```python
import json
import re

from logoxide import BookFormat

usfm = "\\id 3JN\n\\h 3 John\n\\c 1\n\\p\n\\v 1 The elder to Gaius the beloved.\n"
usx = lx.usfm_to_usx(usfm)
print(usx)  # → <?xml version="1.0" encoding="utf-8"?><usx version="3.0"><book code="3JN" style="id"/><para style="h">3 John</para><chapter number="1" style="c" sid="3JN 1"/>…</usx>
usj = lx.usx_to_usj(usx)
print(json.loads(usj)["content"][0])  # → {'type': 'book', 'marker': 'id', 'code': '3JN'}
print(lx.usx_to_usfm(lx.usj_to_usx(usj)))  # → \id 3JN … \p \v 1 The elder to Gaius the beloved.
print(lx.convert_book(usj, BookFormat.USJ, BookFormat.USFM) == lx.usx_to_usfm(usx))  # → True
```

Text that isn't the format named isn't always an error: USFM without markers is an empty USX
book, and USX that isn't USX is empty USFM. Check what a file is first (next), and that what
comes out has a book in it (`LogoxideBook.parse` of the USX).

```ts
console.log(lx.usfmToUsx("hello")); // → <?xml version="1.0" encoding="utf-8"?><usx version="3.0"></usx>
console.log(JSON.stringify(lx.convertBook("<osis/>", BookFormat.Usx, BookFormat.Usfm))); // → ""
```

```python
print(lx.usfm_to_usx("hello"))  # → <?xml version="1.0" encoding="utf-8"?><usx version="3.0"></usx>
print(repr(lx.convert_book("<osis/>", BookFormat.USX, BookFormat.USFM)))  # → ''
```

## What format a file is

`detectFormat(name, head)` names a file's format (`usx`, `usfm`, `usj`, `osis`, `thml`, `gbf`)
from its extension, else from how its text starts (the first few hundred bytes are enough), so
a `.xml`, `.json`, or `.txt` file is found too; `null` for anything else.

```ts
console.log(lx.detectFormat("3JN.SFM", usfm), lx.detectFormat("book.txt", usfm), lx.detectFormat("JHN.xml", usx), lx.detectFormat("book.json", usj)); // → usfm usfm usx usj
console.log(lx.detectFormat("bible.xml", "<osis>"), lx.detectFormat("notes.txt", "hello")); // → osis null
```

```python
print(lx.detect_format("3JN.SFM", usfm), lx.detect_format("book.txt", usfm), lx.detect_format("JHN.xml", usx), lx.detect_format("book.json", usj))  # → usfm usfm usx usj
print(lx.detect_format("bible.xml", "<osis>"), lx.detect_format("notes.txt", "hello"))  # → osis None
```

## A whole Bible

eBible.org's zip of USFM as USX, a file a book: `unzip` lists a zip's files, and each USFM file
is converted (front matter and glossaries too; leave them out by name if you don't want them).
A book takes a tenth of a second or so, so a whole Bible is a few seconds: in a browser, do it
in a worker.

```ts
const zip = await get(["https://ebible.org/Scriptures/engwebp_usfm.zip"]);
const converted: { path: string; text: string }[] = [];
for (const file of lx.unzip(zip)) {
  const text = new TextDecoder().decode(file.bytes);
  if (lx.detectFormat(file.path, text.slice(0, 1024)) !== "usfm") continue; // its copyright page, stylesheet, signatures
  converted.push({ path: file.path.replace(/\.(usfm|sfm)$/i, ".usx"), text: lx.usfmToUsx(text) });
}
console.log(converted.length, converted.find((f) => f.path.includes("JHN"))?.path); // → 68 73-JHNengwebp.usx
```

```python
zip_bytes = get(["https://ebible.org/Scriptures/engwebp_usfm.zip"])
converted: dict[str, str] = {}
for file in lx.unzip(zip_bytes):
    text = file.bytes.decode()
    if lx.detect_format(file.path, text[:1024]) != "usfm":
        continue  # its copyright page, stylesheet, signatures
    converted[re.sub(r"\.(usfm|sfm)$", ".usx", file.path, flags=re.I)] = lx.usfm_to_usx(text)
print(len(converted), next(p for p in converted if "JHN" in p))  # → 68 73-JHNengwebp.usx
```

To read the books rather than convert them, `openBible`
takes the zip as it is: [Bibles in your app](../apps.md#a-whole-bible).

## On the command line

`lx convert` does the same to a file or a folder, naming each file as it was (USJ as `.json`):

```sh
lx convert JHN.usfm --to usx                 # to stdout
lx convert JHN.usx --to usj -o JHN.json
lx convert usfm/ --to usx -o usx/            # every book file in a folder
lx convert usx/ --from usx --to usfm -o usfm/   # only the folder's USX files
```

## In Rust

logoxide-text's `source::convert` takes a `TextFormat` each way (and the four functions it's
made of, `usfm_to_usx` and the rest, are there too); `TextFormat::detect` names a file's format,
and `archive::unzip` lists a zip's files.

```rust
use logoxide_text::{TextFormat, archive::unzip, source::convert};

/// eBible.org's zip of USFM as USX files, by name
fn to_usx(zip: &[u8]) -> Result<Vec<(String, String)>> {
    let mut converted = vec![];
    for (path, bytes) in unzip(zip)? {
        let text = String::from_utf8_lossy(&bytes);
        if TextFormat::detect(&path, &text) != Some(TextFormat::Usfm) {
            continue; // its copyright page, stylesheet, signatures
        }
        let name = path.rsplit_once('.').map_or(path.as_str(), |(stem, _)| stem);
        converted.push((format!("{name}.usx"), convert(&text, TextFormat::Usfm, TextFormat::Usx)?));
    }
    Ok(converted)
}
```

`convert` is an error for OSIS, ThML, and GBF: only USFM, USX, and USJ convert into each other.
