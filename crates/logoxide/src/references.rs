/*!
The Bible references in a package's books, found by topos in each EPUB its enabled formats hold.

- [`find`] reads the books: every reference, with where it is (an EPUB CFI readers open straight
  to, its place in the book's text, its chapter, its paragraph). For anyone with the book: it
  takes a tenth of a second a book
- [`cited`] is what a release publishes (`logoxide.references.bin`, beside the summary): only which
  passages the book cites and how often, so which books cite a passage is known without
  downloading them. Locations aren't published: they're only of use with the book, which gives
  them again

`logoxide.references.bin`: `LXR1`, then deflate over postcard of each cited passage (its OSIS
reference and count) in Bible order. `LXR1` names this layout for good: a change is `LXR2`.
*/

use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use topos_bible::matcher::{BibleMatcher, matches::ComplexFilter};
use topos_bible_formats::epub::{CfiOptions, search_epub_reader};

use crate::{
    error::{LogoxideError, Result},
    manifest::Format,
    package::Package,
};

/// The references file, beside the summary
pub const REFERENCES: &str = "logoxide.references.bin";

const MAGIC: &[u8; 4] = b"LXR1";

/// The most a references file may inflate to (64 MiB; 937 references are 2.6 KB deflated), so a
/// small file can't fill memory
pub const MAX_REFERENCES_BYTES: u64 = 64 << 20;

/// A reference found in a book
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundReference {
    /// The passage, as an OSIS reference (`John.3.16`, `John.3.16-John.3.18`)
    pub passage: String,
    /// The file it's in, in the package (`epub/the-dorean-principle.epub`)
    pub file: String,
    /// Where it is in the file: an EPUB CFI range
    pub cfi: String,
    /// The chapter it's in, when the book's table of contents names one
    pub chapter: Option<String>,
    /// Its UTF-16 range in the book's text (each content document's text, a blank line apart,
    /// as topos's index and EPUB++ count it)
    pub start: u32,
    pub end: u32,
    /// The 1-based line of the book's text it starts on, and its 1-based UTF-16 column there
    pub line: u32,
    pub column: u32,
    /// Its content document's place in the spine
    pub spine: u32,
    /// The line (paragraph) it starts in
    pub line_text: String,
}

/// A passage a book cites, and how many times
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cited {
    /// An OSIS reference (`Matt.10.8`, `Phil.4.10-Phil.4.20`, `1Cor.9`), as it was written
    pub passage: String,
    pub count: u32,
}

/// The EPUBs the package's enabled formats hold, in path order (`epub/the-dorean-principle.epub`)
pub fn epub_files(package: &Package) -> Result<Vec<String>> {
    let declared = crate::package::declared_files(package.storage().as_ref())?;
    Ok((declared.into_iter())
        .filter(|f| f.to_ascii_lowercase().ends_with(".epub"))
        .filter(|f| {
            (package.manifest().formats(package.enabled()).iter()).any(|(name, entry)| {
                let dir = entry.path.trim_end_matches('/');
                Format::from_name(name) == Some(Format::Epub)
                    && (f.as_str() == dir || f.starts_with(&format!("{dir}/")))
            })
        })
        .collect())
}

/// Every reference in the EPUBs the package's enabled formats hold, file by file in path order,
/// each file's in the order they're in it. Empty for a package without EPUBs
pub fn find(package: &Package) -> Result<Vec<FoundReference>> {
    let storage = package.storage();
    let files = epub_files(package)?;
    if files.is_empty() {
        return Ok(vec![]);
    }
    let data = crate::bible_data();
    let matcher = BibleMatcher::new(data.clone(), ComplexFilter::default());
    let mut found = vec![];
    for file in files {
        let bytes = storage.read(&file)?;
        let matches =
            search_epub_reader(&matcher, std::io::Cursor::new(bytes), CfiOptions::default())
                .map_err(|e| LogoxideError::Book {
                    book: file.clone(),
                    message: e.to_string(),
                })?;
        for m in matches {
            let Some(passage) = m.psg.to_osis(data.books()) else {
                continue;
            };
            let at = m.location;
            let whole = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
            found.push(FoundReference {
                passage,
                file: file.clone(),
                cfi: at.cfi,
                chapter: at.chapter,
                start: whole(at.text_utf16.start),
                end: whole(at.text_utf16.end),
                line: whole(at.line),
                column: whole(at.utf16_column),
                spine: whole(at.spine_index),
                line_text: at.line_text,
            });
        }
    }
    Ok(found)
}

/// The passages references cite, each once with how many times, in Bible order (then as
/// written: `Matt.10.8` before `Matt.10.8-Matt.10.10`)
pub fn cited(references: &[FoundReference]) -> Vec<Cited> {
    let books = crate::bible_data().books();
    let mut counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for r in references {
        *counts.entry(r.passage.as_str()).or_default() += 1;
    }
    let mut cited: Vec<Cited> = (counts.into_iter())
        .map(|(passage, count)| Cited {
            passage: passage.to_string(),
            count,
        })
        .collect();
    // Bible order: the book, then its chapter and verse as numbers
    let key = |passage: &str| {
        let mut parts = passage.split(['.', '-']);
        let book = parts.next().and_then(|b| books.search_osis(b)).map(|b| b.0);
        let numbers: Vec<u32> = parts.take(2).map(|n| n.parse().unwrap_or(0)).collect();
        (book.unwrap_or(u8::MAX), numbers, passage.to_string())
    };
    cited.sort_by_cached_key(|c| key(&c.passage));
    cited
}

/// The references file's bytes
pub fn encode(cited: &[Cited]) -> Result<Vec<u8>> {
    let body = postcard::to_stdvec(cited).map_err(|e| LogoxideError::Cache(e.to_string()))?;
    let mut out = MAGIC.to_vec();
    let mut deflate = flate2::write::DeflateEncoder::new(&mut out, flate2::Compression::best());
    deflate
        .write_all(&body)
        .and_then(|()| deflate.finish().map(|_| ()))
        .map_err(|e| LogoxideError::Cache(e.to_string()))?;
    Ok(out)
}

/// The passages in a references file's bytes
pub fn decode(bytes: &[u8]) -> Result<Vec<Cited>> {
    let bad = |message: String| LogoxideError::Manifest(format!("{REFERENCES}: {message}"));
    let body = bytes
        .strip_prefix(MAGIC.as_slice())
        .ok_or_else(|| bad(String::from("not a LXR1 references file")))?;
    let mut inflated = vec![];
    flate2::read::DeflateDecoder::new(body)
        .take(MAX_REFERENCES_BYTES + 1)
        .read_to_end(&mut inflated)
        .map_err(|e| bad(e.to_string()))?;
    if inflated.len() as u64 > MAX_REFERENCES_BYTES {
        return Err(bad(format!(
            "inflates past {MAX_REFERENCES_BYTES} bytes, more than any book cites"
        )));
    }
    postcard::from_bytes(&inflated).map_err(|e| bad(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_what_is_cited() {
        let found = |passage: &str| FoundReference {
            passage: passage.into(),
            file: "epub/book.epub".into(),
            cfi: "epubcfi(/6/14!/4/2,/1:5,/1:15)".into(),
            chapter: None,
            start: 0,
            end: 9,
            line: 1,
            column: 1,
            spine: 0,
            line_text: String::new(),
        };
        let references: Vec<FoundReference> = [
            "1Cor.9",
            "Matt.10.8",
            "Matt.10.8-Matt.10.10",
            "Matt.10.8",
            "Matt.9.2",
            "Gen.1.1",
        ]
        .into_iter()
        .map(found)
        .collect();
        let cited = cited(&references);
        let listed: Vec<(&str, u32)> = cited
            .iter()
            .map(|c| (c.passage.as_str(), c.count))
            .collect();
        assert_eq!(
            listed,
            [
                ("Gen.1.1", 1),
                ("Matt.9.2", 1),
                ("Matt.10.8", 2),
                ("Matt.10.8-Matt.10.10", 1),
                ("1Cor.9", 1)
            ]
        );
        let bytes = encode(&cited).unwrap();
        assert!(bytes.starts_with(b"LXR1"));
        assert_eq!(decode(&bytes).unwrap(), cited);
        assert!(decode(b"LXR9").is_err());
        assert!(decode(b"LXR1 not deflate").is_err());
        // A bomb: a little deflate that inflates past the limit
        let mut bomb = MAGIC.to_vec();
        let mut deflate =
            flate2::write::DeflateEncoder::new(&mut bomb, flate2::Compression::fast());
        let zeros = vec![0u8; 1 << 20];
        for _ in 0..=(MAX_REFERENCES_BYTES >> 20) {
            deflate.write_all(&zeros).unwrap();
        }
        deflate.finish().unwrap();
        assert!(bomb.len() < 1 << 20);
        let error = decode(&bomb).unwrap_err().to_string();
        assert!(error.contains("inflates past"), "{error}");
    }
}
