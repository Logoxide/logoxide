/*!
Golden tests of verse-hash rule 1: the text each verse is hashed as, and its hash, for small USFM,
USX, and OSIS books (nested `\w`, `\add`, a note inside a word, Hebrew whose marks are out of
canonical order and the same in NFC, a zero-width space, a no-break space, a bridge, and a
partial verse).

A rule never changes once published: these pin logoxide-markup's `plain_text` (how words in
nested elements join, what notes leave behind) as rule 1 uses it. If a topos upgrade breaks them,
don't change the expected values: the text rule 1 hashes has changed, so `HashRule` needs a new
rule (rule 2) beside rule 1, written by the new topos, and the files written with rule 1 are built
again from their text when compared.
*/

use std::sync::Arc;

use logoxide_text::markup::verses::slice;
use logoxide_text::topos_bible::data::books::BookId;
use logoxide_text::{
    MemoryFiles, Source,
    hash::{HashRule, VerseRecord, book_hashes, verse_text},
    source::{TextFormat, open, usfm_to_usx, usx_to_usfm},
};

/// One format's files, read by its source
fn package(format: &str, files: &[(&str, &str)]) -> Box<dyn Source> {
    let all: Vec<(String, String)> = (files.iter())
        .map(|(p, t)| (format!("{format}/{p}"), t.to_string()))
        .collect();
    let files = Arc::new(MemoryFiles::with(all));
    open(files, TextFormat::from_name(format).unwrap(), format).unwrap()
}

/// `JHN 3:16` (`JHN 3:4-5` for a bridge, `JHN 3:20a` for a part)
fn reference(r: &VerseRecord) -> String {
    let code = logoxide_text::bible_data()
        .books()
        .get_usfm(r.book)
        .cloned()
        .unwrap();
    let part = match r.part {
        1..=26 => char::from(b'a' + r.part - 1).to_string(),
        _ => String::new(),
    };
    match r.last > r.verse {
        true => format!("{code} {}:{}{part}-{}", r.chapter, r.verse, r.last),
        false => format!("{code} {}:{}{part}", r.chapter, r.verse),
    }
}

/// Each verse of the source's only book: its reference, its text as rule 1 hashes it, and its
/// hash as hex
fn hashed(package: &dyn Source) -> Vec<(String, String, String)> {
    let books = package.books();
    assert_eq!(books.len(), 1);
    let book = package.book(books[0]).unwrap();
    let hashes = book_hashes(&book, HashRule::One);
    assert_eq!(hashes.len(), book.spans.len());
    (book.spans.iter().zip(hashes))
        .map(|(span, hash)| {
            let text = verse_text(
                &slice(&book.nodes, &span.start, Some(&span.end)),
                HashRule::One,
            );
            let hex = hash.hash.iter().map(|b| format!("{b:02x}")).collect();
            (reference(&hash), text, hex)
        })
        .collect()
}

/// Hebrew with its marks out of canonical order (dagesh before shewa, shin dot before hiriq), as
/// texts are often typed
const HEBREW_TYPED: &str =
    "\u{5D1}\u{5BC}\u{5B0}\u{5E8}\u{5B5}\u{5D0}\u{5E9}\u{5C1}\u{5B4}\u{5D9}\u{5EA}";
/// The same word in NFC
const HEBREW_NFC: &str =
    "\u{5D1}\u{5B0}\u{5BC}\u{5E8}\u{5B5}\u{5D0}\u{5E9}\u{5B4}\u{5C1}\u{5D9}\u{5EA}";

fn usfm(hebrew: &str) -> String {
    format!(
        "\\id JHN\n\\c 3\n\\p\n\
\\v 16 For God so \\w loved|strong=\"G25\"\\w* the \\add world\\add*, that he gave his \\w only begot\\f + \\fr 3:16 \\ft Or, unique.\\f*ten\\w* Son.\n\
\\v 17 Of \\w the \\+nd Lord\\+nd*\\w* and {hebrew}, the caf\u{65}\u{301}.\n\
\\v 18-19 In the\u{200B} be\u{AD}ginning\u{A0}was   the\tWord.\n\
\\v 20a The first part,\n\
\\v 20b and the second.\n"
    )
}

/// What every format's verses hash as: the same text whichever markup it came from
fn expected() -> Vec<(String, String, String)> {
    let row = |r: &str, t: &str, h: &str| (r.to_string(), t.to_string(), h.to_string());
    vec![
        row(
            "JHN 3:16",
            "For God so loved the world, that he gave his only begotten Son.",
            "29fc0c4dce1347",
        ),
        row(
            "JHN 3:17",
            &format!("Of the Lord and {HEBREW_NFC}, the caf\u{e9}."),
            "fef2d3f9f710ee",
        ),
        row(
            "JHN 3:18-19",
            "In the beginning was the Word.",
            "e05ad1a76b7a0c",
        ),
        row("JHN 3:20a", "The first part,", "a31a3a471a8025"),
        row("JHN 3:20b", "and the second.", "01e18f2a9dfa1a"),
    ]
}

#[test]
fn usfm_verses() {
    let typed = hashed(&*package("usfm", &[("JHN.usfm", &usfm(HEBREW_TYPED))]));
    assert_eq!(typed, expected());
    // Typed out of order or in NFC, the Hebrew hashes alike
    let nfc = hashed(&*package("usfm", &[("JHN.usfm", &usfm(HEBREW_NFC))]));
    assert_eq!(nfc, typed);
}

fn usx() -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0"><book code="JHN" style="id"/>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so <char style="w" strong="G25">loved</char> the <char style="add">world</char>, that he gave his <char style="w">only begot<note caller="+" style="f"><char style="fr">3:16 </char><char style="ft">Or, unique.</char></note>ten</char> Son.<verse eid="JHN 3:16"/>
<verse number="17" style="v" sid="JHN 3:17"/>Of <char style="w">the <char style="nd">Lord</char></char> and {HEBREW_TYPED}, the caf&#x65;&#x301;.<verse eid="JHN 3:17"/>
<verse number="18-19" style="v" sid="JHN 3:18-19"/>In the&#x200B; be&#xAD;ginning&#xA0;was   the	Word.<verse eid="JHN 3:18-19"/>
<verse number="20a" style="v" sid="JHN 3:20a"/>The first part,<verse eid="JHN 3:20a"/>
<verse number="20b" style="v" sid="JHN 3:20b"/>and the second.<verse eid="JHN 3:20b"/></para>
<chapter eid="JHN 3"/>
</usx>"#
    )
}

#[test]
fn usx_verses() {
    assert_eq!(hashed(&*package("usx", &[("JHN.usx", &usx())])), expected());
}

#[test]
fn usx_as_usfm_verses() {
    // USX into USFM keeps every verse's words: read as USFM, they hash as the USX does
    let usfm = usx_to_usfm(&usx()).unwrap();
    assert_eq!(
        hashed(&*package("usfm", &[("JHN.usfm", &usfm)])),
        expected()
    );
    // and back into USX again
    let usx = usfm_to_usx(&usfm).unwrap();
    assert_eq!(hashed(&*package("usx", &[("JHN.usx", &usx)])), expected());
}

#[test]
fn osis_verses() {
    let osis = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<osis><osisText osisIDWork="T"><header><work osisWork="T"><title>T</title></work></header>
<div type="book" osisID="John">
<chapter osisID="John.3" sID="John.3" n="3"/>
<p><verse osisID="John.3.16" sID="John.3.16" n="16"/>For God so <w lemma="strong:G25">loved</w> the <transChange type="added">world</transChange>, that he gave his <w>only begot<note type="explanation">Or, unique.</note>ten</w> Son.<verse eID="John.3.16"/>
<verse osisID="John.3.17" sID="John.3.17" n="17"/>Of <w>the <divineName>Lord</divineName></w> and {HEBREW_TYPED}, the caf&#x65;&#x301;.<verse eID="John.3.17"/>
<verse osisID="John.3.18 John.3.19" sID="John.3.18" n="18-19"/>In the&#x200B; be&#xAD;ginning&#xA0;was   the	Word.<verse eID="John.3.18"/>
<verse osisID="John.3.20" sID="John.3.20a" n="20a"/>The first part,<verse eID="John.3.20a"/>
<verse osisID="John.3.20" sID="John.3.20b" n="20b"/>and the second.<verse eID="John.3.20b"/></p>
<chapter eID="John.3"/>
</div></osisText></osis>"#
    );
    assert_eq!(
        hashed(&*package("osis", &[("John.xml", &osis)])),
        expected()
    );
}

/// The BSB's text with every file in NFD hashes as it does in NFC: rule 1 normalizes. Reads
/// `LOGOXIDE_PACKAGES` (by default `~/Development/logoxide-packages`)
#[test]
#[ignore]
fn the_bsb_in_nfd_hashes_the_same() {
    use unicode_normalization::UnicodeNormalization;

    let dir = std::path::PathBuf::from(format!("{}/bsb/usx", packages()));
    let (mut nfc, mut nfd) = (vec![], vec![]);
    let mut changed = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = format!("usx/{}", path.file_name().unwrap().to_string_lossy());
        let bytes = std::fs::read(&path).unwrap();
        let decomposed = match (name.ends_with(".usx"), String::from_utf8(bytes.clone())) {
            (true, Ok(text)) => text.nfd().collect::<String>().into_bytes(),
            _ => bytes.clone(),
        };
        changed += usize::from(decomposed != bytes);
        nfc.push((name.clone(), bytes));
        nfd.push((name, decomposed));
    }
    assert!(changed > 0, "no file of the BSB changed in NFD");
    let hashes = |files: Vec<(String, Vec<u8>)>| {
        let source = open(Arc::new(MemoryFiles::with(files)), TextFormat::Usx, "usx").unwrap();
        (source.books().into_iter())
            .flat_map(|id| book_hashes(&source.book(id).unwrap(), HashRule::One))
            .collect::<Vec<_>>()
    };
    let (nfc, nfd) = (hashes(nfc), hashes(nfd));
    assert_eq!(nfc.len(), 31_086);
    assert_eq!(nfc, nfd);
}

/// `LOGOXIDE_PACKAGES`, by default `~/Development/logoxide-packages`
fn packages() -> String {
    std::env::var("LOGOXIDE_PACKAGES").unwrap_or_else(|_| {
        format!(
            "{}/Development/logoxide-packages",
            std::env::var("HOME").unwrap()
        )
    })
}

/// Every verse's hash in one format's folder of a package, read with `files` changed by `convert`
/// into `to`'s files
fn converted_hashes(
    dir: &str,
    from: TextFormat,
    to: TextFormat,
    convert: impl Fn(&str) -> String,
) -> (Vec<VerseRecord>, Vec<VerseRecord>) {
    let (mut before, mut after) = (vec![], vec![]);
    let (from_name, to_name) = (from.name(), to.name());
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let file = path.file_stem().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        after.push((format!("{to_name}/{file}.{to_name}"), convert(&text)));
        before.push((format!("{from_name}/{file}.{from_name}"), text));
    }
    let hashes = |files: Vec<(String, String)>, format: TextFormat| {
        let source = open(Arc::new(MemoryFiles::with(files)), format, format.name()).unwrap();
        (source.books().into_iter())
            .flat_map(|id| book_hashes(&source.book(id).unwrap(), HashRule::One))
            .collect::<Vec<_>>()
    };
    (hashes(before, from), hashes(after, to))
}

/// The BSB's USX made USFM, and the KJV's USFM made USX and USFM again, hash as they did. Reads
/// `LOGOXIDE_PACKAGES` (by default `~/Development/logoxide-packages`)
#[test]
#[ignore]
fn real_bibles_through_usx_to_usfm() {
    let root = packages();
    let (usx, usfm) = converted_hashes(
        &format!("{root}/bsb/usx"),
        TextFormat::Usx,
        TextFormat::Usfm,
        |text| usx_to_usfm(text).unwrap(),
    );
    assert_eq!(usx.len(), 31_086);
    // But for Psalm 119's 21 acrostic headings (`<para style="qa">ב</para>`): USX's reader gives
    // one after a verse's end milestone to the next verse, and USFM, which has no end, to the
    // verse before it, so the heading's words move from the second verse to the first
    let headings: Vec<u8> = (1..=21u8).flat_map(|k| [8 * k, 8 * k + 1]).collect();
    let moved =
        |r: &VerseRecord| r.book == BookId(19) && r.chapter == 119 && headings.contains(&r.verse);
    let keep = |all: Vec<VerseRecord>| all.into_iter().filter(|r| !moved(r)).collect::<Vec<_>>();
    let differ = (usx.iter().zip(&usfm)).filter(|(a, b)| a != b).count();
    assert_eq!(differ, headings.len());
    same(&keep(usx), &keep(usfm));
    let (usfm, again) = converted_hashes(
        &format!("{root}/kjv/usfm"),
        TextFormat::Usfm,
        TextFormat::Usfm,
        |text| usx_to_usfm(&usfm_to_usx(text).unwrap()).unwrap(),
    );
    assert_eq!(usfm.len(), 31_102);
    same(&usfm, &again);
}

/// The same verses, or the first few that differ
fn same(before: &[VerseRecord], after: &[VerseRecord]) {
    let differ: Vec<_> = (before.iter().zip(after))
        .filter(|(b, a)| b != a)
        .map(|(b, a)| format!("{} → {}", reference(b), reference(a)))
        .collect();
    assert!(
        differ.is_empty(),
        "{} verses differ: {differ:?}",
        differ.len()
    );
    assert_eq!(before.len(), after.len());
}
