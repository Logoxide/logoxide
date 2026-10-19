# logoxide-text

What's in a Bible or a book: verses, hashes, titles, numbering. It knows no packages.

- **Sources**: USX, USFM (through usfm3), USJ, OSIS, ThML, and GBF read a book at a time into
  [logoxide-markup](../logoxide-markup)'s tree (`source::open`, `Source`, `Book`), from any
  `Files` (a folder, memory, or a host's files, so WebAssembly too). Peripherals, books topos
  doesn't know, and lettered chapters are passed over and said (`Skipped`)
- **Conversion**: a book in USFM, USX, or USJ as any of them (`source::convert`; one way at a
  time: `usfm_to_usx` and `usx_to_usfm` through usfm3, `usx_to_usj` and `usj_to_usx`; what's
  written is normalized, not the publisher's layout)
- **Hashes**: each verse's words hashed by a rule that never changes (`hash::HashRule`,
  `book_hashes`), so a verse can be found again in a later version of its text
- **Titles**: a file's own title (`\toc2`, `\h`, `\toc1`; an EPUB's `dc:title`)
- **Versification maps**: Paratext `.vrs` lines, and references mapped through them both ways
  and through each other (`versification`)
- **Passages as text**: a passage or a whole book as plain text in a style (labels, hidden
  elements, templates, a width), with where each verse is and what marks each run (`passage`)
- **Timings**: WebVTT cues named by verse, checked against a text (`timings`)
- **Cache**: parsed books kept between runs under any `BookStore` (a folder, memory, a host's
  files), keyed by the files' checksum and format, each file naming the version that wrote it
  so another's is parsed again, never misread (`cache`)
- **Citations**: the passages an EPUB cites, found by topos (`citations`); an EPUB's metadata
  (`metadata`); the script a text is written in (`script`)

[logoxide](../logoxide), the package manager, never depends on this crate: whoever runs both
(lx, the bindings, the language server) hands it the facts this crate finds, through
`logoxide::reader::TextReader`. With logoxide's default `read` feature, that reader is built in.
