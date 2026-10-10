/*!
The book numbers in logoxide's files (`logoxide.verses.bin`), which logoxide owns so they never
change with topos's data: 1 to 66 for `GEN` to `REV` in the Protestant canon's order, 67 to 90 as
topos 0.6 numbers its deuterocanonical and other books (70 and 78, which have no USFM code, are
reserved), 91 to 103 the books after them, 104 to 199 reserved, and 200 to 214 the peripherals
(front matter, glossaries, indexes). 0 is never a book.

A number is matched to topos's [`BookId`] by its USFM code, so a book topos doesn't know still has
a number, and topos renumbering its books changes nothing here. A number, once given, is never
changed or given to another book; new books take reserved numbers.
*/

use topos_bible::data::books::BookId;

/// A book, by its number in logoxide's files
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Book(pub u8);

/// Every book with a number, and its USFM code, in number order
pub const BOOKS: &[(u8, &str)] = &[
    (1, "GEN"),
    (2, "EXO"),
    (3, "LEV"),
    (4, "NUM"),
    (5, "DEU"),
    (6, "JOS"),
    (7, "JDG"),
    (8, "RUT"),
    (9, "1SA"),
    (10, "2SA"),
    (11, "1KI"),
    (12, "2KI"),
    (13, "1CH"),
    (14, "2CH"),
    (15, "EZR"),
    (16, "NEH"),
    (17, "EST"),
    (18, "JOB"),
    (19, "PSA"),
    (20, "PRO"),
    (21, "ECC"),
    (22, "SNG"),
    (23, "ISA"),
    (24, "JER"),
    (25, "LAM"),
    (26, "EZK"),
    (27, "DAN"),
    (28, "HOS"),
    (29, "JOL"),
    (30, "AMO"),
    (31, "OBA"),
    (32, "JON"),
    (33, "MIC"),
    (34, "NAM"),
    (35, "HAB"),
    (36, "ZEP"),
    (37, "HAG"),
    (38, "ZEC"),
    (39, "MAL"),
    (40, "MAT"),
    (41, "MRK"),
    (42, "LUK"),
    (43, "JHN"),
    (44, "ACT"),
    (45, "ROM"),
    (46, "1CO"),
    (47, "2CO"),
    (48, "GAL"),
    (49, "EPH"),
    (50, "PHP"),
    (51, "COL"),
    (52, "1TH"),
    (53, "2TH"),
    (54, "1TI"),
    (55, "2TI"),
    (56, "TIT"),
    (57, "PHM"),
    (58, "HEB"),
    (59, "JAS"),
    (60, "1PE"),
    (61, "2PE"),
    (62, "1JN"),
    (63, "2JN"),
    (64, "3JN"),
    (65, "JUD"),
    (66, "REV"),
    // topos 0.6's deuterocanonical and other books; 70 (Additions to Esther) and 78 (Additions
    // to Daniel) have no USFM code, and are reserved
    (67, "TOB"), // Tobit
    (68, "JDT"), // Judith
    (69, "ESG"), // Greek Esther
    (71, "WIS"), // Wisdom of Solomon
    (72, "SIR"), // Sirach
    (73, "BAR"), // Baruch
    (74, "LJE"), // Letter of Jeremiah
    (75, "S3Y"), // Prayer of Azariah
    (76, "SUS"), // Susanna
    (77, "BEL"), // Bel and the Dragon
    (79, "1MA"), // 1 Maccabees
    (80, "2MA"), // 2 Maccabees
    (81, "1ES"), // 1 Esdras
    (82, "MAN"), // Prayer of Manasseh
    (83, "PS2"), // Psalm 151
    (84, "3MA"), // 3 Maccabees
    (85, "2ES"), // 2 Esdras
    (86, "4MA"), // 4 Maccabees
    (87, "ODA"), // Odes
    (88, "PSS"), // Psalms of Solomon
    (89, "ENO"), // 1 Enoch
    (90, "LAO"), // Laodiceans
    // The other books USFM names
    (91, "DAG"),  // Daniel (Greek)
    (92, "PS3"),  // Psalms 152-155
    (93, "2BA"),  // 2 Baruch
    (94, "LBA"),  // Letter of Baruch
    (95, "JUB"),  // Jubilees
    (96, "1MQ"),  // 1 Meqabyan
    (97, "2MQ"),  // 2 Meqabyan
    (98, "3MQ"),  // 3 Meqabyan
    (99, "REP"),  // Reproof
    (100, "4BA"), // 4 Baruch
    (101, "EZA"), // Apocalypse of Ezra
    (102, "5EZ"), // 5 Ezra
    (103, "6EZ"), // 6 Ezra
    // 104 to 199 are reserved; then the peripherals
    (200, "FRT"), // front matter
    (201, "INT"), // introductions
    (202, "BAK"), // back matter
    (203, "CNC"), // concordance
    (204, "GLO"), // glossary
    (205, "TDX"), // topical index
    (206, "NDX"), // names index
    (207, "OTH"), // other matter
    (208, "XXA"), // extra matter
    (209, "XXB"),
    (210, "XXC"),
    (211, "XXD"),
    (212, "XXE"),
    (213, "XXF"),
    (214, "XXG"),
];

impl Book {
    /// The book a USFM code names (in any case)
    pub fn from_usfm(code: &str) -> Option<Book> {
        (BOOKS.iter())
            .find(|(_, c)| c.eq_ignore_ascii_case(code))
            .map(|&(n, _)| Book(n))
    }

    /// Its USFM code, if the number is a book's
    pub fn usfm(self) -> Option<&'static str> {
        (BOOKS.iter()).find(|&&(n, _)| n == self.0).map(|&(_, c)| c)
    }

    /// The book topos numbers `id`, by its USFM code (`None` for one without a code, like topos's
    /// Additions to Esther)
    pub fn from_book_id(id: BookId) -> Option<Book> {
        Book::from_usfm(crate::bible_data().books().get_usfm(id)?)
    }

    /// topos's number for it, by its USFM code (`None` when topos doesn't know the book)
    pub fn book_id(self) -> Option<BookId> {
        crate::bible_data().books().search_usfm(self.usfm()?)
    }

    /// Its USFM code, or its number when it has none
    pub fn name(self) -> String {
        self.usfm()
            .map_or_else(|| self.0.to_string(), str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table as it was first published. A number once given never changes, so the table is
    /// only ever added to: this fails if a number in it is edited
    const SNAPSHOT: &str = "\
        1 GEN 2 EXO 3 LEV 4 NUM 5 DEU 6 JOS 7 JDG 8 RUT 9 1SA 10 2SA 11 1KI 12 2KI 13 1CH 14 2CH \
        15 EZR 16 NEH 17 EST 18 JOB 19 PSA 20 PRO 21 ECC 22 SNG 23 ISA 24 JER 25 LAM 26 EZK \
        27 DAN 28 HOS 29 JOL 30 AMO 31 OBA 32 JON 33 MIC 34 NAM 35 HAB 36 ZEP 37 HAG 38 ZEC \
        39 MAL 40 MAT 41 MRK 42 LUK 43 JHN 44 ACT 45 ROM 46 1CO 47 2CO 48 GAL 49 EPH 50 PHP \
        51 COL 52 1TH 53 2TH 54 1TI 55 2TI 56 TIT 57 PHM 58 HEB 59 JAS 60 1PE 61 2PE 62 1JN \
        63 2JN 64 3JN 65 JUD 66 REV \
        67 TOB 68 JDT 69 ESG 71 WIS 72 SIR 73 BAR 74 LJE 75 S3Y 76 SUS 77 BEL 79 1MA 80 2MA \
        81 1ES 82 MAN 83 PS2 84 3MA 85 2ES 86 4MA 87 ODA 88 PSS 89 ENO 90 LAO \
        91 DAG 92 PS3 93 2BA 94 LBA 95 JUB 96 1MQ 97 2MQ 98 3MQ 99 REP 100 4BA 101 EZA 102 5EZ \
        103 6EZ \
        200 FRT 201 INT 202 BAK 203 CNC 204 GLO 205 TDX 206 NDX 207 OTH 208 XXA 209 XXB 210 XXC \
        211 XXD 212 XXE 213 XXF 214 XXG";

    #[test]
    fn numbers_never_change() {
        let snapshot: Vec<&str> = SNAPSHOT.split_whitespace().collect();
        let snapshot: Vec<String> = (snapshot.chunks(2)).map(|p| p.join(" ")).collect();
        let table: Vec<String> = (BOOKS.iter()).map(|(n, c)| format!("{n} {c}")).collect();
        for entry in &snapshot {
            assert!(table.contains(entry), "{entry} was changed or removed");
        }
        assert!(BOOKS.windows(2).all(|w| w[0].0 < w[1].0), "in number order");
        assert!(
            (BOOKS.iter()).all(|&(n, _)| n != 0 && n != 70 && n != 78 && !(104..200).contains(&n))
        );
        let mut codes: Vec<&str> = BOOKS.iter().map(|(_, c)| *c).collect();
        codes.sort();
        codes.dedup();
        assert_eq!(codes.len(), BOOKS.len(), "each code once");
    }

    #[test]
    fn every_book_topos_knows_has_a_number() {
        let books = crate::bible_data().books();
        for id in books.ids() {
            let Some(code) = books.get_usfm(id) else {
                continue;
            };
            let book = Book::from_usfm(code).unwrap_or_else(|| panic!("{code} has no number"));
            assert_eq!(book.book_id(), Some(id));
            assert_eq!(Book::from_book_id(id), Some(book));
        }
        // 1 to 66 are topos's numbers too, so verse files from before this table read the same
        for n in 1..=66 {
            assert_eq!(Book(n).book_id(), Some(BookId(n)));
        }
    }

    #[test]
    fn codes_and_numbers() {
        assert_eq!(Book::from_usfm("jhn"), Some(Book(43)));
        assert_eq!(Book(43).usfm(), Some("JHN"));
        assert_eq!(Book::from_usfm("JUB"), Some(Book(95)));
        assert_eq!(Book::from_usfm("XXG"), Some(Book(214)));
        assert_eq!(Book(70).usfm(), None);
        assert_eq!(Book(150).name(), "150");
        assert_eq!(Book::from_usfm("ZZZ"), None);
        // Each round trip
        for &(n, code) in BOOKS {
            assert_eq!(Book::from_usfm(code).and_then(|b| b.usfm()), Some(code));
            assert_eq!(Book(n).usfm().and_then(Book::from_usfm), Some(Book(n)));
        }
    }
}
