/*!
Cross-references (`xref/` and its `.tsv` files, `doc/entries.md`): a line a pair, `from`, `to`, and an optional
weight, tab-separated, as OSIS references. Every file is read when opened; a verse's
cross-references are the lines whose `from` covers it.
*/

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use logoxide_markup::Node;
use topos_bible::{data::books::BookId, segments::Passage};

use super::{CrossReference, Entry, Source, TextFormat};
use crate::{
    error::{Result, TextError},
    files::{Files, join},
};

/// One line, by the passages' places in [`XrefSource`]'s tables (a million lines are a few
/// megabytes)
struct Line {
    from: u32,
    to: u32,
    /// NaN for none
    weight: f64,
}

/// A distinct `from`: as parsed, in normal OSIS form, and its lines in order
struct From {
    passage: Passage,
    osis: String,
    lines: Vec<u32>,
}

pub struct XrefSource {
    lines: Vec<Line>,
    /// Each distinct `from`, in file and line order
    froms: Vec<From>,
    from_ids: HashMap<String, u32>,
    /// Each distinct `to`, in normal form
    tos: Vec<String>,
    /// The `from`s in each book, in order
    by_book: BTreeMap<BookId, Vec<u32>>,
    no_books: BTreeMap<BookId, String>,
}

/// What's needed only while the files are read
#[derive(Default)]
struct Reading {
    to_ids: HashMap<String, u32>,
    /// Each `from` and `to` pair read, for lines listed twice
    pairs: HashSet<(u32, u32)>,
}

impl XrefSource {
    /// Reads every `*.tsv` file under `dir`, in path order
    pub fn open(storage: Arc<dyn Files>, dir: &str) -> Result<Self> {
        let mut names = storage.list(dir)?;
        names.sort();
        let mut source = Self {
            lines: vec![],
            froms: vec![],
            from_ids: HashMap::new(),
            tos: vec![],
            by_book: BTreeMap::new(),
            no_books: BTreeMap::new(),
        };
        let mut reading = Reading::default();
        for name in names {
            let wanted =
                (name.rsplit_once('.')).is_some_and(|(_, e)| e.eq_ignore_ascii_case("tsv"));
            if !wanted {
                continue;
            }
            let path = join(dir, &name);
            source.read(&path, &storage.read_string(&path)?, &mut reading)?;
        }
        source.lines.shrink_to_fit();
        Ok(source)
    }

    fn read(&mut self, path: &str, text: &str, reading: &mut Reading) -> Result<()> {
        for (i, line) in text.lines().enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = |message: String| TextError::Entries(format!("{path}:{}: {message}", i + 1));
            let mut fields = line.split('\t');
            let (Some(from), Some(to)) = (fields.next(), fields.next()) else {
                return Err(bad(String::from(
                    "a line is `from`, `to`, and an optional weight, separated by tabs",
                )));
            };
            let (from, from_osis) = parse_reference(from).ok_or_else(|| {
                bad(format!(
                    "`{from}` isn't an OSIS reference of a book logoxide knows"
                ))
            })?;
            let (_, to_osis) = parse_reference(to).ok_or_else(|| {
                bad(format!(
                    "`{to}` isn't an OSIS reference of a book logoxide knows"
                ))
            })?;
            let weight = match fields.next().map(str::trim).filter(|w| !w.is_empty()) {
                Some(weight) => (weight.parse::<f64>().ok())
                    .filter(|w| w.is_finite())
                    .ok_or_else(|| bad(format!("weight `{weight}` isn't a number")))?,
                None => f64::NAN,
            };
            let from_id = match self.from_ids.get(&from_osis) {
                Some(&id) => id,
                None => {
                    let id = self.froms.len() as u32;
                    self.by_book.entry(from.book).or_default().push(id);
                    self.from_ids.insert(from_osis.clone(), id);
                    self.froms.push(From {
                        passage: from,
                        osis: from_osis,
                        lines: vec![],
                    });
                    id
                }
            };
            let to_id = match reading.to_ids.get(&to_osis) {
                Some(&id) => id,
                None => {
                    let id = self.tos.len() as u32;
                    reading.to_ids.insert(to_osis.clone(), id);
                    self.tos.push(to_osis);
                    id
                }
            };
            if !reading.pairs.insert((from_id, to_id)) {
                let (from, to) = (
                    &self.froms[from_id as usize].osis,
                    &self.tos[to_id as usize],
                );
                return Err(bad(format!("{from} to {to} is listed twice")));
            }
            let index = self.lines.len() as u32;
            self.froms[from_id as usize].lines.push(index);
            self.lines.push(Line {
                from: from_id,
                to: to_id,
                weight,
            });
        }
        Ok(())
    }

    /// How many lines the files have
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn weight(line: &Line) -> Option<f64> {
        (!line.weight.is_nan()).then_some(line.weight)
    }
}

/// An OSIS reference as a passage, with its normal form (`John.3.16-John.3.18`)
fn parse_reference(reference: &str) -> Option<(Passage, String)> {
    let books = crate::bible_data().books();
    let passage = books.parse_osis(reference.trim()).ok()?;
    let osis = passage.to_osis(books)?;
    Some((passage, osis))
}

/// Whether any verse of `a` is in `b` (both in one book)
fn overlap(a: &Passage, b: &Passage) -> bool {
    if a.book != b.book {
        return false;
    }
    let ranges = |p: &Passage| -> Vec<((u8, u16), (u8, u16))> {
        (p.ranges(None).iter())
            .map(|r| {
                (
                    (r.start.chapter, r.start.verse.into()),
                    (r.end.chapter, r.end.verse.into()),
                )
            })
            .collect()
    };
    let (a, b) = (ranges(a), ranges(b));
    a.iter().any(|(a_start, a_end)| {
        (b.iter()).any(|(b_start, b_end)| a_start <= b_end && b_start <= a_end)
    })
}

impl Source for XrefSource {
    fn format(&self) -> Option<TextFormat> {
        Some(TextFormat::Xref)
    }

    fn files(&self) -> &BTreeMap<BookId, String> {
        &self.no_books
    }

    fn book(&self, id: BookId) -> Result<super::Book> {
        Err(TextError::NotFound(format!(
            "book {}: cross-references have no verse text",
            id.0
        )))
    }

    fn keys(&self) -> Vec<String> {
        self.froms.iter().map(|f| f.osis.clone()).collect()
    }

    /// A passage's targets as one text: each `to` with `=` and its weight when it has one,
    /// in line order, joined with spaces (what a summary hashes, `doc/entries.md`)
    fn entry(&self, key: &str) -> Result<Option<Entry>> {
        let Some((_, from)) = parse_reference(key) else {
            return Ok(None);
        };
        let lines = (self.from_ids.get(&from))
            .map(|&id| self.froms[id as usize].lines.as_slice())
            .unwrap_or_default();
        let targets: Vec<String> = (lines.iter())
            .map(|&i| &self.lines[i as usize])
            .map(|l| {
                let to = &self.tos[l.to as usize];
                match Self::weight(l) {
                    Some(weight) => format!("{to}={weight}"),
                    None => to.clone(),
                }
            })
            .collect();
        if targets.is_empty() {
            return Ok(None);
        }
        Ok(Some(Entry {
            key: from,
            headword: None,
            nodes: vec![Node::Text(targets.join(" "))],
        }))
    }

    fn cross_references(&self, verse: &str) -> Result<Vec<CrossReference>> {
        let Some((passage, _)) = parse_reference(verse) else {
            return Ok(vec![]);
        };
        let froms = (self.by_book.get(&passage.book))
            .map(Vec::as_slice)
            .unwrap_or_default();
        // In line order, as the files have them
        let mut lines: Vec<u32> = (froms.iter())
            .map(|&id| &self.froms[id as usize])
            .filter(|f| overlap(&f.passage, &passage))
            .flat_map(|f| f.lines.iter().copied())
            .collect();
        lines.sort_unstable();
        Ok((lines.iter())
            .map(|&i| &self.lines[i as usize])
            .map(|l| CrossReference {
                from: self.froms[l.from as usize].osis.clone(),
                to: self.tos[l.to as usize].clone(),
                weight: Self::weight(l),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        files::MemoryFiles,
        hash::{HashRule, verse_text},
        source::open,
    };

    const TSK: &str = "# Genesis\nGen.1.1\tJohn.1.1-John.1.3\t1\nGen.1.1\tHeb.11.3\t0.8\n\nGen.1.1\tPs.33.6\nGen.1.2-Gen.1.5\tJohn.1.5\n";

    #[test]
    fn a_verses_cross_references() {
        let files = Arc::new(MemoryFiles::with([
            ("xref/gen.tsv", TSK),
            ("xref/exo.tsv", "Exod.3.14\tJohn.8.58\t1\n"),
            ("xref/README.md", "not lines"),
        ]));
        let source = open(files, TextFormat::Xref, "xref").unwrap();
        assert!(source.books().is_empty());
        assert_eq!(source.keys(), ["Exod.3.14", "Gen.1.1", "Gen.1.2-Gen.1.5"]);
        let refs = source.cross_references("Gen.1.1").unwrap();
        let to: Vec<(&str, Option<f64>)> = refs.iter().map(|r| (r.to.as_str(), r.weight)).collect();
        assert_eq!(
            to,
            [
                ("John.1.1-John.1.3", Some(1.0)),
                ("Heb.11.3", Some(0.8)),
                ("Ps.33.6", None)
            ]
        );
        // A range covers each of its verses; the normal form of its book name
        let inside = source.cross_references("Gen.1.4").unwrap();
        assert_eq!(
            (inside[0].from.as_str(), inside[0].to.as_str()),
            ("Gen.1.2-Gen.1.5", "John.1.5")
        );
        assert!(source.cross_references("Gen.1.6").unwrap().is_empty());
        assert!(
            source
                .cross_references("not a reference")
                .unwrap()
                .is_empty()
        );
        // The text a summary hashes
        let entry = source.entry("Gen.1.1").unwrap().unwrap();
        assert_eq!(
            verse_text(&entry.nodes, HashRule::CURRENT),
            "John.1.1-John.1.3=1 Heb.11.3=0.8 Ps.33.6"
        );
        assert_eq!(source.entry("Gen.1.1").unwrap().unwrap().key, "Gen.1.1");
        assert!(source.entry("Gen.2.1").unwrap().is_none());
    }

    #[test]
    fn mistakes_are_named() {
        let error = |text: &str| {
            let files = Arc::new(MemoryFiles::with([("xref/a.tsv", text)]));
            XrefSource::open(files, "xref")
                .err()
                .expect("refused")
                .to_string()
        };
        assert!(error("Gen.1.1\n").contains("a.tsv:1: a line is"));
        assert!(error("Zzz.1.1\tGen.1.1\n").contains("`Zzz.1.1` isn't an OSIS reference"));
        assert!(error("Gen.1.1\tGen 1:1\n").contains("`Gen 1:1`"));
        assert!(error("Gen.1.1\tGen.1.2\tmuch\n").contains("weight `much`"));
        assert!(
            error("# ok\nGen.1.1\tGen.1.2\nGen.1.1\tGen.1.2\t1\n")
                .contains("a.tsv:3: Gen.1.1 to Gen.1.2 is listed twice")
        );
    }
}
