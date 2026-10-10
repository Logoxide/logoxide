# Summaries: `logoxide.summary.toml`, `logoxide.verses.bin`, `logoxide.references.bin`

What a package version holds, published with it (committed at its release tag, and inside its
release archive) so it can be known without downloading the text: what's in it, how big each
format is, a short hash of every verse, and for books, the passages they cite. Two versions'
summaries say what changed between them, which says how the version number must change. `lx
release` writes them, `lx verify` builds them again and compares, `lx show --git` reads them from
a tag alone. This describes the formats as logoxide 0.1 writes them; the reference is
`crates/logoxide/src/summary.rs` and `crates/logoxide/src/references.rs`.

All three files are part of the package (declared), but are left out of `text-checksum`, which
they can't contain.

## `logoxide.summary.toml`

```toml
# Written by `lx release`: what this version holds. `lx verify` checks it.
summary = 1
verse-hash = 1
id = "kjv"
version = "1.0.3"
text-checksum = "sha256:8497…"
files-checksum = "sha256:ef03…"
verses = 31102

[sizes]
usfm = 12511641

[books]
GEN = [31, 25, 24, 26, 32, 22, …]
EXO = [22, 25, 22, 31, …]

[contents]
added = 21510
note = 6959
strongs = 349308
title = 220
word = 349308
words-of-jesus = 2038

[titles]
"usfm/01-GEN.usfm" = "Genesis"
```

| Key | Type | Meaning |
|---|---|---|
| `summary` | integer | The summary format's version: `1`. A reader must refuse any other |
| `verse-hash` | integer | The [hash rule](#the-verses-text-hash-rules) `logoxide.verses.bin` was made with, which its header names too: the two must agree. A reader refuses a rule it doesn't know (written by a newer lx). Absent in summaries from before rules, whose verse file is `LXV1` |
| `id` | string | The package's id |
| `version` | string | The package's version |
| `text-checksum` | string | The [checksum](catalog.md#checksums) of the package's declared files other than the three summary files |
| `files-checksum` | string | The same hash over the formats' files alone (not `logoxide.toml` or the license): for a package without verses, what says its text changed. Absent in summaries from before it existed |
| `verses` | integer | How many verse markers the text has (a bridge like `4-5` is one) |
| `sizes` | table of integers | Bytes of each format's files, by format name (every format in `[formats]`, 0 if empty) |
| `books` | table of arrays | Each book's verse count per chapter, by USFM code, in the text's book order: the translation's own versification. A chapter's count is the highest verse number its markers give (a bridge `4-5` counts to 5); a chapter without verses is 0 |
| `contents` | table of integers | How many of each kind of element the text has, by kebab-case kind (`note`, `title`, `words-of-jesus`, `divine-name`, …), leaving out markers and containers (verse, chapter, metadata, paragraph, line break, milestone, division, other); plus `strongs`, the words tagged with Strong's numbers |
| `media` | table | `files` and `bytes`: how many media files `[media]` lists, and their total size. Absent without media |
| `references` | integer | How many Bible references the package's EPUBs have (absent when 0) |
| `mappings` | integer | For a versification map, how many verses it maps (absent when 0) |
| `titles` | table of strings | Each file's title, by path ([manifest.md](manifest.md#titles)) |

The element kinds are topos-bible-markup's (`Kind`); a reader should treat `contents` as
informational and accept kinds it doesn't know.

## `logoxide.verses.bin` (`LXV2`)

A short hash of every verse's words, in the text's order (373 KB for the KJV): an 8-byte header,
then a record a verse.

```text
offset  size  field
0       4     magic: the ASCII bytes "LXV2"
4       2     hash rule: the rule the hashes were made with (u16, little-endian; 1)
6       1     record length: bytes a record (u8; 12)
7       1     reserved (0)
8       r*n   n records of r bytes, one per verse marker:
  +0    1     book: logoxide's book number (u8; the table below)
  +1    1     chapter (u8)
  +2    1     verse (u8; 0 is a psalm's title where the text numbers one)
  +3    1     last: the bridge's last verse (u8; the same as verse when not a bridge)
  +4    1     part: which part of the verse (u8; 0 the whole verse, 1 for `20a`, 2 for `20b`, …)
  +5    7     the first 7 bytes of SHA-256(verse text, as the rule makes it)
```

The file's length minus 8 must be a multiple of the record length. A reader takes the fields it
knows from the start of each record and skips the rest, so later versions can add fields at a
record's end (with a longer record length) without a new magic; a record length below 12 is an
error. A reader refuses a hash rule it doesn't know: the file was written by a newer logoxide.

The verse is found by book, chapter, verse, and part: `20a` and `20b` are two verses.

`LXV1` was the layout before hash rules (no header, no part, 8 bytes of a hash of text that
wasn't normalized). It's recognized only to be built again: a reader comparing versions builds an
`LXV1` release's hashes from its text, as for a release without a summary.

**Book numbers** are logoxide's own, fixed for good: a number is never changed or given to
another book, and new books take reserved numbers. They're matched to a book by its USFM code,
never by another library's numbering. 1 to 66 are the Protestant canon in its order (the same
numbers topos gives them), 67 to 90 follow topos 0.6's numbering of the deuterocanonical and
other books, 91 to 103 are the other books USFM names, and 200 to 214 its peripherals. 0 is never
a book; 70 and 78 (topos's Additions to Esther and to Daniel, which have no USFM code) and 104 to
199 are reserved. The reference is `crates/logoxide/src/books.rs`.

| Numbers | Books (USFM codes) |
|---|---|
| 1–39 | `GEN` `EXO` `LEV` `NUM` `DEU` `JOS` `JDG` `RUT` `1SA` `2SA` `1KI` `2KI` `1CH` `2CH` `EZR` `NEH` `EST` `JOB` `PSA` `PRO` `ECC` `SNG` `ISA` `JER` `LAM` `EZK` `DAN` `HOS` `JOL` `AMO` `OBA` `JON` `MIC` `NAM` `HAB` `ZEP` `HAG` `ZEC` `MAL` |
| 40–66 | `MAT` `MRK` `LUK` `JHN` `ACT` `ROM` `1CO` `2CO` `GAL` `EPH` `PHP` `COL` `1TH` `2TH` `1TI` `2TI` `TIT` `PHM` `HEB` `JAS` `1PE` `2PE` `1JN` `2JN` `3JN` `JUD` `REV` |
| 67–69 | `TOB` `JDT` `ESG` |
| 70 | reserved |
| 71–77 | `WIS` `SIR` `BAR` `LJE` `S3Y` `SUS` `BEL` |
| 78 | reserved |
| 79–90 | `1MA` `2MA` `1ES` `MAN` `PS2` `3MA` `2ES` `4MA` `ODA` `PSS` `ENO` `LAO` |
| 91–103 | `DAG` `PS3` `2BA` `LBA` `JUB` `1MQ` `2MQ` `3MQ` `REP` `4BA` `EZA` `5EZ` `6EZ` |
| 104–199 | reserved |
| 200–214 | `FRT` `INT` `BAK` `CNC` `GLO` `TDX` `NDX` `OTH` `XXA` `XXB` `XXC` `XXD` `XXE` `XXF` `XXG` |

### The verse's text: hash rules

**The verse's text**, as hashed: the plain text of everything between the verse's start and its
end (from right after the previous verse to the next verse's or chapter's marker, so a heading
between two verses belongs to the second), with notes, titles (headings), and metadata elements
left out, then normalized as the file's hash rule says, as UTF-8. Changing a note, a heading, or
markup therefore never changes a verse's hash.

**Rule 1** (the only one so far):

1. topos-bible-markup's `plain_text` with `Filter::text()` (how words in nested elements join,
   what a note inside a word leaves behind)
2. Unicode NFC (not NFKC, which would fold away distinctions a text makes), so text typed with
   its marks in another order, or decomposed, hashes the same
3. U+200B (zero-width space), U+FEFF (byte-order mark), U+00AD (soft hyphen), and U+2060 (word
   joiner) removed; zero-width joiners and non-joiners (U+200D, U+200C) are kept, since they
   change how words are written in some scripts
4. Split on Unicode whitespace (`White_Space`, so a no-break space too) and joined with single
   spaces, no leading or trailing space

A rule never changes once published. Anything that would change the text a rule hashes (another
normalization, or a topos-bible-markup that renders markup differently, which golden tests in
`crates/logoxide/tests/verse_hash.rs` catch) is a new rule with the next number. Hashes are
compared only when made by one rule: `lx diff` and `lx release` build an older rule's side again
from its tag's text, so a rule change is never reported as rewording, and `lx verify` says to
release again. Another implementation that renders text differently will compute different
hashes, so it should compare hashes only with its own, or with logoxide's files.

Hosts put a verse's hash in links (`&ver=1.0.2&vh=9f86d081`: the version, and the first verse's
hash as 8 hex digits, its first 4 bytes) so a link can find its words again in a later version.

## `logoxide.references.bin` (`LXR1`)

For packages with EPUBs (books): which passages they cite and how often, so which books cite a
passage is known without downloading them (2.6 KB for 937 references). Written only when there is
at least one reference.

```text
offset  size  field
0       4     magic: the ASCII bytes "LXR1"
4       …     raw DEFLATE (RFC 1951, no zlib or gzip header) of a postcard-encoded sequence
```

The inflated bytes are [postcard](https://postcard.jamesmunns.com/wire-format)'s encoding of a
sequence of `(passage: string, count: u32)`: a varint (LEB128) count of entries, then for each, a
varint byte length and the passage's UTF-8 bytes, then the count as a varint. Each passage is an
OSIS reference as it was written (`Matt.10.8`, `Phil.4.10-Phil.4.20`, `1Cor.9`), in Bible order.
Where each reference is in the book isn't published: it's only of use with the book, which gives
it again.

## Changes and versions

Two releases' verse hashes are compared by verse number (book, chapter, verse, part), the range
each verse covers (its `last`) before its words: a split verse's words are taken to change with
it. A whole verse cut into parts (`20` became `20a` and `20b`) is split; parts made one whole verse
are joined into it.

| Change | Meaning | Kind |
|---|---|---|
| removed | A verse number the new version lacks, whose words aren't anywhere else | **major** |
| renumbered (moved) | A verse's old words are now at another number | **major** |
| joined | A verse number now inside another verse's bridge (`4`, `5` became `4-5`, or `4-5`, `6` became `4-6`): a link to it finds other words with its own | **major** |
| bridged | A verse's bridge narrowed and lost a number (`4-5` became `4`, with no `5`) | **major** |
| split | A verse's bridge narrowed, each number it covered still a verse or in another bridge (`4-5` became `4` and `5`) | **minor** |
| reworded | A verse's words changed at the same number and range (or its bridge widened over numbers no verse had) | **minor** |
| added | A verse number the old version lacks (not one a split bridge covered) | **minor** |
| retitled | A file's title changed (hosts name what they show by it, but nothing they point at is gone) | **minor** |
| files changed | For packages without verses (books): `files-checksum` changed | **minor** |
| anything else | Notes, headings, markup, metadata, media timings | **patch** |

A version's change is the largest of its changes, and the new version number must be at least
that much bigger (`lx release` refuses a smaller one):

- From 1.0 on: a major change needs a new major version, a minor change a new minor version, a
  patch a new patch version.
- Before 1.0, like Cargo, the minor number counts as major: a major change needs 0.(x+1).0, and
  a patch bump (0.1.1 after 0.1.0) covers minor changes too.
- In 0.0.x, every change is breaking, matching Cargo's rule that each 0.0.x version is
  incompatible with the others: any change needs 0.0.(z+1).
- For packages without verses (books, collections, maps), any change to their files is minor.
- Imported packages whose version is the publisher's date (eBible.org's `2026.10.8`) or its own
  numbering (a SWORD module's) are an exception: their numbers say when, not what. An update
  still compares the summaries, and when the publisher's version isn't newer it takes the next
  version the change calls for.

A release from before summaries existed is compared by building its summary from its text at
the tag.
