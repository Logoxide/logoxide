//! Books: a package made from an EPUB (`create`), opened without verse text, and the references
//! topos finds in it, published with its summary

use std::{
    io::{Cursor, Write},
    sync::Arc,
};

use biblix::{
    Features, MemoryStorage, Package, Storage,
    create::{NewPackage, new_from_file},
    manifest::PackageKind,
    summary::{Summary, summarize},
};

fn xhtml(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>x</title></head><body>{body}</body></html>"
    )
}

/// A small EPUB 3: two authors and an editor, a subtitle, and three references in two chapters
fn epub() -> Vec<u8> {
    let opf = r##"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="id">x</dc:identifier>
    <dc:title id="t">Freely Given</dc:title>
    <dc:title id="s">Essays on the Free Gospel</dc:title>
    <meta refines="#s" property="title-type">subtitle</meta>
    <dc:creator id="a1">First Author</dc:creator>
    <dc:creator id="a2">Second Author</dc:creator>
    <dc:creator id="e1">An Editor</dc:creator>
    <meta refines="#e1" property="role" scheme="marc:relators">edt</meta>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="one.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="two.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/><itemref idref="c2"/></spine>
</package>"##;
    let files = [
        ("mimetype", "application/epub+zip".to_string()),
        (
            "META-INF/container.xml",
            r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_string(),
        ),
        ("OEBPS/content.opf", opf.to_string()),
        (
            "OEBPS/nav.xhtml",
            xhtml("<nav><ol><li><a href=\"one.xhtml\">One</a></li><li><a href=\"two.xhtml\">Two</a></li></ol></nav>"),
        ),
        ("OEBPS/one.xhtml", xhtml("<p>Freely you received; freely give (Matthew 10:8).</p>")),
        ("OEBPS/two.xhtml", xhtml("<p>See 1 Corinthians 9 and John 3:16-18.</p>")),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn a_book_from_an_epub() {
    let files = MemoryStorage::new();
    let given = NewPackage {
        license: Some("CC0-1.0".into()),
        version: Some("1.0.0".parse().unwrap()),
        ..NewPackage::default()
    };
    let manifest = new_from_file("Freely-Given.epub", &epub(), &given, &files).unwrap();
    let info = manifest.package.unwrap();
    assert_eq!(
        (info.id.as_str(), info.abbreviation.as_str(), info.kind),
        ("freely-given", "FG", PackageKind::Book)
    );
    // The editor isn't an author
    assert_eq!(info.authors, ["First Author", "Second Author"]);
    assert_eq!(
        info.description.as_deref(),
        Some("Essays on the Free Gospel")
    );
    assert!(files.read("epub/Freely-Given.epub").is_ok());

    // It opens with no verse text, and its summary has its references
    let package = Package::open(Arc::new(files), Features::default()).unwrap();
    assert!(package.books().is_empty());
    // Read from the book: every reference, and where it is
    assert_eq!(
        biblix::references::epub_files(&package).unwrap(),
        ["epub/Freely-Given.epub"]
    );
    let found = biblix::references::find(&package).unwrap();
    let passages: Vec<&str> = found.iter().map(|r| r.passage.as_str()).collect();
    assert_eq!(passages, ["Matt.10.8", "1Cor.9", "John.3.16-John.3.18"]);
    let chapters: Vec<Option<&str>> = found.iter().map(|r| r.chapter.as_deref()).collect();
    assert_eq!(chapters, [Some("One"), Some("Two"), Some("Two")]);
    assert!(found.iter().all(|r| r.cfi.starts_with("epubcfi(")));
    // What topos's index needs: where each one is in the book's text, and its paragraph
    let matthew = &found[0];
    assert!(matthew.line_text.contains("(Matthew 10:8)"), "{matthew:?}");
    assert_eq!(matthew.end - matthew.start, "Matthew 10:8".len() as u32);
    assert_eq!(found[1].spine, 1);

    // Its summary: how many, and the passages cited, in Bible order
    let (summary, verses) = summarize(&package).unwrap();
    assert!(verses.is_empty());
    assert_eq!(summary.references, 3);
    let cited: Vec<&str> = summary.cited.iter().map(|c| c.passage.as_str()).collect();
    assert_eq!(cited, ["Matt.10.8", "John.3.16-John.3.18", "1Cor.9"]);

    // Published beside the summary, and read back
    let published = MemoryStorage::new();
    summary.write(&verses, &published).unwrap();
    assert!(published.read("biblix.references.bin").is_ok());
    let (read, _) = Summary::read(&published).unwrap();
    assert_eq!(read, summary);
}

#[test]
fn a_bible_without_text_is_refused() {
    let files = MemoryStorage::new();
    let given = NewPackage {
        kind: Some(PackageKind::Bible),
        ..NewPackage::default()
    };
    new_from_file("Freely-Given.epub", &epub(), &given, &files).unwrap();
    // A Bible must be read as verses, and biblix doesn't read an EPUB's
    assert!(Package::open(Arc::new(files), Features::default()).is_err());
}
