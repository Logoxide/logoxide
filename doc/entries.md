# Dictionaries and cross-references: `entries/*.xml`, `xref/*.tsv`

Two formats of logoxide's own for what isn't verse text: a dictionary's entries (a lexicon's,
Strong's numbers or headwords) and cross-references between passages. This describes them as
logoxide 0.1 reads them, for anyone writing another implementation; the reference is
`crates/logoxide-text/src/source/dictionary.rs` and `xref.rs`. Commentaries need no format of
their own: they are OSIS or ThML files whose verse markers say which verses each note is on
([manifest.md](manifest.md#formats), and `Source::notes` in logoxide-text reads them by passage).

Both are declared in `[formats]` like any other ([manifest.md](manifest.md#formats)):

```toml
[package]
id = "strongs-greek"
name = "Strong's Greek Dictionary"
abbreviation = "StrongsGreek"
version = "1.0.0"
language = "grc"
kind = "lexicon"
keys = "strongs-greek"

[formats]
dictionary = { path = "entries/" }
```

```toml
[package]
id = "tsk"
name = "Treasury of Scripture Knowledge"
abbreviation = "TSK"
version = "1.0.0"
language = "en"
kind = "cross-references"

[formats]
xref = { path = "xref/" }
```

## `dictionary`: `entries/*.xml`

Every `*.xml` file under the format's `path` is a file of entries. A package's entries are those
of all its files, in path order; a key found in two files is an error naming both. The file is
UTF-8 XML in form, but its entries' bodies are read verbatim, not as XML (so GBF and plain text
fit too):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<entries keys="strongs-greek" markup="tei">
<entry key="G25" headword="ἀγαπάω"><entryFree n="25"><orth>ἀγαπάω</orth> … </entryFree></entry>
<entry key="G26" headword="ἀγάπη">…</entry>
<entry key="G25a" link="G25"/>
</entries>
```

### The root: `<entries>`

| Attribute | Required | Meaning |
|---|---|---|
| `markup` | yes | The markup the bodies are written in, as logoxide-markup names them: `tei`, `osis`, `thml`, `gbf`, or `plain` (`plaintext`). Each body is parsed by it |
| `keys` | no | What the entries are keyed by (`strongs-greek`, `strongs-hebrew`, `headword`), the same as the manifest's `[package] keys`, for a reader with only the file. The manifest's is what counts |

Any other attribute is passed over. The root may be preceded by an XML declaration and
whitespace; nothing but whitespace stands between entries.

### Each entry: `<entry>`

| Attribute | Required | Meaning |
|---|---|---|
| `key` | yes | The entry's key, as the dictionary gives it (`G25`, `G0025`, `Aaron`). Its [normal form](#keys) is what the entry is found and hashed by; two entries whose keys have one normal form are an error naming the key |
| `headword` | no | The word the entry is about, for showing (`ἀγαπάω`); for a headword dictionary, usually the key itself |
| `link` | no | For an alias: the key of the entry whose text this one shares (SWORD's `@LINK`). An alias has no body of its own; looking it up gives the target's. One step only: an alias of an alias finds nothing |

Attribute values are XML attribute values: `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`, and
numeric character references are decoded; the delimiter is `"` or `'`.

The **body** is everything between the end of the `<entry …>` tag and the first `</entry>` after
it, taken as written: the entry's markup as its source gives it, parsed with logoxide-markup as
the root's `markup` says. So a body may not contain the text `</entry>`, and an XML body's own
entities are the markup's business (a TEI body's `&amp;` is decoded by the TEI parser; a plain
body's isn't, since plain text decodes nothing). An empty element (`<entry key="…"/>`) has an
empty body.

### Keys

A key's **normal form** is what it's found and hashed by:

- A **Strong's number** (`keys = "strongs-greek"` or `"strongs-hebrew"`): one letter `G` or `H`
  in either case, digits, and an optional suffix (letters, `!a`, …) is written as the uppercase
  letter, the number without leading zeros (`0` stays `0`), and the suffix as given: `G0025`,
  `g25`, and `G25` are one key, `G25`; `H0430a` is `H430a`. A key of a Strong's dictionary that
  isn't of that form (an introduction, `A`) is kept as given.
- A **headword** (`keys = "headword"`): kept as given, with the whitespace around it trimmed.

A key is **matched** by its normal form, exactly first; else ignoring case, every combining mark
(accents, breathings, Hebrew points: NFD, the marks dropped), and final sigma, as catalog search
matches names (`ιησους` finds `Ἰησοῦς`), the first entry in path and file order that matches so.
Two entries with one normal form are an error; two that only match loosely (`Mary` and `MARY`)
are allowed, each found by its own form.

### What a summary records

A dictionary package's summary ([summaries.md](summaries.md#logoxideentriesbin-lxe1)) has
`entries`, how many entries it has, and `logoxide.entries.bin` holds a hash of each entry: the
first 8 bytes of SHA-256 of its key's normal form (UTF-8), and a hash of its words by the verse
hash rule (`verse-hash`: the body's plain text with notes left out, normalized, whitespace
collapsed), so a changed entry is told from a changed key. An alias's text is its target's.

## `xref`: `xref/*.tsv`

Every `*.tsv` file under the format's `path` holds lines of three tab-separated fields:

```text
# Treasury of Scripture Knowledge, Genesis
Gen.1.1	John.1.1-John.1.3	1
Gen.1.1	Heb.11.3	0.8
Gen.1.1	Ps.33.6
```

| Field | Required | Meaning |
|---|---|---|
| `from` | yes | The passage the references are for: an OSIS reference, a verse (`John.3.16`) or a range (`John.3.16-John.3.18`, `Gen.1`, a whole chapter) |
| `to` | yes | The passage referred to, an OSIS reference the same way |
| `weight` | no | How strong the link is, a number (`1`, `0.8`), for a reader to rank by. Without it, no weight |

Fields are separated by one tab; a line's trailing whitespace is dropped. A line starting with
`#` is a comment, and an empty line is passed over. One line per pair: the same `from` and `to`
twice is an error naming the line. A line with fewer than two fields, a field that isn't an OSIS
reference of a book logoxide knows, or a weight that isn't a number, is an error naming the file
and line. Book names are OSIS's (`Gen`, `1Sam`, `John`), matched ignoring case.

A verse's **cross-references** are the lines whose `from` covers it (`John.3.16-John.3.18`
covers `John.3.17`), in file and line order.

### What a summary records

The summary's `entries` counts the distinct `from` passages, and `logoxide.entries.bin` has a
record for each: the key hash is the first 8 bytes of SHA-256 of the `from` reference in its
normal OSIS form (`John.3.16`, or `John.3.16-John.3.18`: the book as logoxide's book data spells
it, chapter and verse as numbers), and the text hash is the hash of its lines' targets in file
order, each `to` followed by `=` and its weight when it has one (`John.1.1-John.1.3=1
Heb.11.3=0.8 Ps.33.6`), joined with single spaces, by the same verse hash rule. So adding or
removing a line for a verse rewords its entry, and dropping every line for a verse removes it.
