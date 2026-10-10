/*!
Golden tests of verse-hash rule 1: the text each verse is hashed as, and its hash, for small USFM,
USX, and OSIS books (nested `\w`, `\add`, a note inside a word, Hebrew whose marks are out of
canonical order and the same in NFC, a zero-width space, a no-break space, a bridge, and a
partial verse).

A rule never changes once published: these pin topos-bible-markup's `plain_text` (how words in
nested elements join, what notes leave behind) as rule 1 uses it. If a topos upgrade breaks them,
don't change the expected values: the text rule 1 hashes has changed, so `HashRule` needs a new
rule (rule 2) beside rule 1, written by the new topos, and the files written with rule 1 are built
again from their text when compared.
*/

use std::sync::Arc;

use logoxide::markup::verses::slice;
use logoxide::{
    Features, MemoryStorage, Package,
    summary::{HashRule, book_hashes, verse_text},
};

/// A package of one format's files
fn package(format: &str, files: &[(&str, &str)]) -> Package {
    let manifest = format!(
        "[package]\nid = \"t\"\nname = \"T\"\nabbreviation = \"T\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n\n[formats]\n{format} = {{ path = \"{format}/\" }}\n"
    );
    let mut all = vec![(String::from("logoxide.toml"), manifest)];
    all.extend((files.iter()).map(|(p, t)| (format!("{format}/{p}"), t.to_string())));
    Package::open(Arc::new(MemoryStorage::with(all)), Features::default()).unwrap()
}

/// Each verse of the package's only book: its reference, its text as rule 1 hashes it, and its
/// hash as hex
fn hashed(package: &Package) -> Vec<(String, String, String)> {
    let books = package.books();
    assert_eq!(books.len(), 1);
    let book = package.source().book(books[0]).unwrap();
    let hashes = book_hashes(&book);
    assert_eq!(hashes.len(), book.spans.len());
    (book.spans.iter().zip(hashes))
        .map(|(span, hash)| {
            let text = verse_text(
                &slice(&book.nodes, &span.start, Some(&span.end)),
                HashRule::One,
            );
            let hex = hash.hash.iter().map(|b| format!("{b:02x}")).collect();
            (hash.reference(), text, hex)
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
    let typed = hashed(&package("usfm", &[("JHN.usfm", &usfm(HEBREW_TYPED))]));
    assert_eq!(typed, expected());
    // Typed out of order or in NFC, the Hebrew hashes alike
    let nfc = hashed(&package("usfm", &[("JHN.usfm", &usfm(HEBREW_NFC))]));
    assert_eq!(nfc, typed);
}

#[test]
fn usx_verses() {
    let usx = format!(
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
    );
    assert_eq!(hashed(&package("usx", &[("JHN.usx", &usx)])), expected());
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
    assert_eq!(hashed(&package("osis", &[("John.xml", &osis)])), expected());
}

/// The BSB's text with every file in NFD hashes as it does in NFC: rule 1 normalizes. Reads
/// `LOGOXIDE_PACKAGES` (by default `~/Development/logoxide-packages`)
#[test]
#[ignore]
fn the_bsb_in_nfd_hashes_the_same() {
    use logoxide::{FsStorage, Storage};
    use unicode_normalization::UnicodeNormalization;

    let root = std::env::var("LOGOXIDE_PACKAGES").unwrap_or_else(|_| {
        format!(
            "{}/Development/logoxide-packages",
            std::env::var("HOME").unwrap()
        )
    });
    let bsb = FsStorage::new(format!("{root}/bsb"));
    let open =
        |storage: MemoryStorage| Package::open(Arc::new(storage), Features::default()).unwrap();
    let (nfc, nfd) = (MemoryStorage::new(), MemoryStorage::new());
    let mut changed = 0;
    for path in bsb.walk("").unwrap() {
        if path.starts_with(".git/") {
            continue;
        }
        let bytes = bsb.read(&path).unwrap();
        nfc.write(&path, &bytes).unwrap();
        let decomposed = match (path.ends_with(".usx"), String::from_utf8(bytes.clone())) {
            (true, Ok(text)) => text.nfd().collect::<String>().into_bytes(),
            _ => bytes,
        };
        changed += usize::from(decomposed != nfc.read(&path).unwrap());
        nfd.write(&path, &decomposed).unwrap();
    }
    assert!(changed > 0, "no file of the BSB changed in NFD");
    let hashes = |package: &Package| {
        (package.books().into_iter())
            .flat_map(|id| book_hashes(&package.source().book(id).unwrap()))
            .collect::<Vec<_>>()
    };
    let (nfc, nfd) = (hashes(&open(nfc)), hashes(&open(nfd)));
    assert_eq!(nfc.len(), 31_086);
    assert_eq!(nfc, nfd);
}
