//! Packages read from memory: formats chosen by features, the cache, and topos's translations

use std::sync::Arc;

use biblix::{
    Cache, Features, MemoryStorage, Package, Storage, bible_data,
    topos_bible::{data::books::BookId, data::translations::Translations},
};

const JOHN: BookId = BookId(43);

const USX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0"><book code="JHN" style="id">x</book>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world.<verse eid="JHN 3:16"/>
<verse number="17" style="v" sid="JHN 3:17"/>For God did not send His Son.<verse eid="JHN 3:17"/>
<verse number="18" style="v" sid="JHN 3:18"/>Whoever believes in Him is not condemned.<verse eid="JHN 3:18"/></para>
<chapter eid="JHN 3"/></usx>"#;

const USFM: &str = "\\id JHN\n\\c 3\n\\p\n\\v 16 \\w For|strong=\"G1063\"\\w* \\w God|strong=\"G2316\"\\w* so loved the world.\n\\v 17 For God sent not his Son.\n";

const MANIFEST: &str = r#"
[package]
id = "test"
name = "A Test Bible"
abbreviation = "TB"
aliases = ["Test"]
version = "1.0.0"
language = "en"

[formats]
usx = { path = "usx/", feature = "usx" }
usfm = { path = "usfm/", feature = "usfm" }

[features]
default = ["usx"]
usx = []
usfm = []
"#;

fn storage() -> Arc<dyn Storage> {
    Arc::new(MemoryStorage::with([
        ("biblix.toml", MANIFEST),
        // A name that isn't the code: the file's <book code> says which book
        ("usx/book-four.usx", USX),
        ("usfm/43JHNTB.usfm", USFM),
    ]))
}

fn text(package: &Package, chapter: u8, verse: u8) -> String {
    let book = package.source().book(JOHN).unwrap();
    let span = book
        .spans
        .iter()
        .find(|s| s.chapter == chapter && s.verse == verse)
        .unwrap();
    let nodes = biblix::markup::verses::slice(&book.nodes, &span.start, Some(&span.end));
    biblix::markup::plain_text(&nodes, &Default::default())
        .trim()
        .to_string()
}

#[test]
fn features_choose_the_format() {
    let usx = Package::open(storage(), Features::default()).unwrap();
    assert_eq!(usx.books(), [JOHN]);
    assert_eq!(
        text(&usx, 3, 18),
        "Whoever believes in Him is not condemned."
    );
    // USX is preferred over USFM when both are on
    let both = Package::open(
        storage(),
        Features {
            features: &["usfm"],
            default: true,
        },
    )
    .unwrap();
    assert_eq!(format!("{:?}", both.source().format()), "Usx");
    let usfm = Package::open(
        storage(),
        Features {
            features: &["usfm"],
            default: false,
        },
    )
    .unwrap();
    // usfm3 keeps the spaces between \w words
    assert_eq!(text(&usfm, 3, 16), "For God so loved the world.");
    assert_eq!(usfm.source().chapter_verses(JOHN).unwrap(), [0, 0, 17]);
    // Nothing readable is on
    let none = Package::open(
        storage(),
        Features {
            features: &[],
            default: false,
        },
    );
    assert!(none.is_err());
}

#[test]
fn the_cache_keeps_counts_and_books() {
    let package = Package::open(storage(), Features::default()).unwrap();
    let cache_storage = Arc::new(MemoryStorage::new());
    let cache = Cache::new(cache_storage.clone());
    let cached = cache.open(&package).unwrap();
    assert_eq!(cached.manifest().books, [(JOHN, vec![0, 0, 18])]);
    let book = cached.book(JOHN).unwrap();
    assert_eq!(book.spans.len(), 3);
    // Written: the manifest, then the book when it was read
    let files = cache_storage.walk("").unwrap();
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(files.iter().any(|f| f.ends_with("/books/JHN.postcard")));
    // A second cache over the same files reads them back
    let again = Cache::new(cache_storage.clone()).open(&package).unwrap();
    assert_eq!(*again.book(JOHN).unwrap(), *book);
    // Other text is another checksum, so another cache
    let changed = MemoryStorage::with([
        ("biblix.toml", MANIFEST),
        ("usx/JHN.usx", &USX.replace("not condemned", "free")[..]),
    ]);
    let changed = Package::open(Arc::new(changed), Features::default()).unwrap();
    assert_ne!(changed.checksum().unwrap(), package.checksum().unwrap());
}

#[test]
fn topos_checks_references_against_the_package() {
    let package = Package::open(storage(), Features::default()).unwrap();
    let cache = Cache::new(Arc::new(MemoryStorage::new()));
    let translations = cache
        .open(&package)
        .unwrap()
        .add_to(Translations::default());
    let data = bible_data();
    let parse = |s: &str| {
        data.parse_with_translation(s, &translations)
            .map(|p| p.translation.map(|t| t.name))
    };
    assert_eq!(parse("John 3:18 TB"), Some(Some("TB".into())));
    assert_eq!(parse("John 3:18 (test)"), Some(Some("TB".into())));
    // This package's John 3 ends at verse 18
    assert_eq!(parse("John 3:19 TB"), None);
    assert_eq!(parse("John 3:19"), Some(None));
}
