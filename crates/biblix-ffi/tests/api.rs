//! The API as the bindings see it: packages from their files' bytes, the cache handed back to the
//! host, topos's translations and books, and a vault's manifest and lock file

use biblix::{
    Biblix, BiblixError, BookStyle, BookTextOptions, OffsetUnit, PackageFeatures, PackageFile,
    PackageKind, PackageLocation, Topos, ToposTranslations,
};

const USX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0"><book code="JHN" style="id">x</book>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world.<verse eid="JHN 3:16"/>
<verse number="17" style="v" sid="JHN 3:17"/>For God did not send His Son.<verse eid="JHN 3:17"/>
<verse number="18" style="v" sid="JHN 3:18"/>Whoever believes in Him is not condemned.<verse eid="JHN 3:18"/></para>
<chapter eid="JHN 3"/></usx>"#;

const MANIFEST: &str = r#"
[package]
id = "test"
name = "A Test Bible"
abbreviation = "TB"
aliases = ["Test"]
version = "1.0.0"
language = "he"
direction = "rtl"
license = "CC0-1.0"

[formats]
usx = { path = "usx/" }
"#;

fn file(path: &str, text: &str) -> PackageFile {
    PackageFile {
        path: path.into(),
        bytes: text.as_bytes().to_vec(),
    }
}

fn files() -> Vec<PackageFile> {
    vec![file("biblix.toml", MANIFEST), file("usx/JHN.usx", USX)]
}

fn defaults() -> PackageFeatures {
    PackageFeatures {
        features: vec![],
        default_features: true,
    }
}

#[test]
fn a_package_from_its_files() {
    let package = Biblix::new()
        .open_package(files(), defaults(), vec![])
        .unwrap();
    let info = package.info();
    assert_eq!(
        (info.id.as_str(), info.abbreviation.as_str(), info.kind),
        ("test", "TB", PackageKind::Bible)
    );
    assert!(info.right_to_left);
    assert_eq!(info.license.as_deref(), Some("CC0-1.0"));
    assert_eq!(package.format(), "usx");
    assert_eq!(package.books(), [43]);
    assert_eq!(package.chapter_verses(43), [0, 0, 18]);
    assert!(package.chapter_verses(1).is_empty());
    assert!(package.checksum().unwrap().starts_with("sha256:"));
    let missing = vec![file("biblix.toml", MANIFEST)];
    assert!(matches!(
        Biblix::new().open_package(missing, defaults(), vec![]),
        Err(BiblixError::NotFound { .. } | BiblixError::Files { .. })
    ));
}

#[test]
fn the_host_keeps_the_cache() {
    let package = Biblix::new()
        .open_package(files(), defaults(), vec![])
        .unwrap();
    // Opening wrote the verse counts; reading a book writes it
    let first = package.cache_writes();
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(first[0].path.ends_with("/manifest.postcard"));
    package.book(43).unwrap();
    let book = package.cache_writes();
    assert_eq!(book.len(), 1);
    assert!(book[0].path.ends_with("/books/JHN.postcard"));
    assert!(
        package.cache_writes().is_empty(),
        "each write is handed back once"
    );
    // Given back, nothing needs writing again
    let cache = [first, book].concat();
    let again = Biblix::new()
        .open_package(files(), defaults(), cache)
        .unwrap();
    again.book(43).unwrap();
    assert!(again.cache_writes().is_empty());
}

#[test]
fn topos_reads_its_books_and_verse_counts() {
    let topos = Topos::new();
    let package = Biblix::new()
        .open_package(files(), defaults(), vec![])
        .unwrap();
    let translations = package.add_to(&ToposTranslations::create());
    assert_eq!(translations.names(), ["TB"]);
    assert_eq!(translations.get("test".into()).as_deref(), Some("TB"));
    let parse = |s: &str| {
        topos.parse_with_translation(s.into(), OffsetUnit::Byte, &translations, BookStyle::Name)
    };
    assert!(parse("John 3:18 TB").is_some());
    // The package's John 3 ends at verse 18
    assert!(parse("John 3:19 TB").is_none());
    assert!(parse("John 3:19").is_some());

    let book = package.book(43).unwrap();
    let passage = topos.parse("John 3:16-17".into(), BookStyle::Name).unwrap();
    let options = BookTextOptions {
        notes: false,
        headings: false,
        class_prefix: "topos-".into(),
    };
    let text = book.text(passage, options);
    assert_eq!(
        text.iter().map(|t| t.trim()).collect::<Vec<_>>(),
        ["For God so loved the world. For God did not send His Son."]
    );
    assert!(matches!(package.book(1), Err(BiblixError::NotFound { .. })));
}

#[test]
fn a_vaults_packages() {
    let manifest = r#"
[dependencies]
bsb = { git = "https://example.org/bsb", version = "1" }
mine = { path = "bibles/mine" }
kjv = { git = "https://example.org/kjv", branch = "main" }

[settings]
default = "bsb"
compare = ["mine"]
"#;
    let lock = r#"
version = 1
dependencies = ["bsb 1.2.0", "mine 0.1.0"]

[[package]]
id = "bsb"
version = "1.2.0"
git = "https://example.org/bsb"
tag = "v1.2.0"
commit = "3f9c2a1b77"
checksum = "sha256:00"
features = ["sword"]

[[package]]
id = "mine"
version = "0.1.0"
path = "bibles/mine"
"#;
    let project = Biblix::new()
        .parse_project(manifest.into(), Some(lock.into()))
        .unwrap();
    assert_eq!(project.dependencies(), ["bsb", "kjv", "mine"]);
    assert_eq!(project.unlocked(), ["kjv"]);
    assert_eq!(project.default_bible().as_deref(), Some("bsb"));
    assert_eq!(project.compare(), ["mine"]);
    let packages = project.packages();
    assert_eq!(
        packages.iter().map(|p| &p.location).collect::<Vec<_>>(),
        [
            &PackageLocation::Library {
                dir: "packages/bsb/1.2.0".into()
            },
            &PackageLocation::Path {
                dir: "bibles/mine".into()
            },
        ]
    );
    assert_eq!(packages[0].features.features, ["sword"]);
    assert_eq!(packages[0].checksum.as_deref(), Some("sha256:00"));

    let no_lock = Biblix::new().parse_project(manifest.into(), None).unwrap();
    assert!(no_lock.packages().is_empty());
    assert_eq!(no_lock.unlocked().len(), 3);
    assert!(matches!(
        Biblix::new().parse_project("[dependencies".into(), None),
        Err(BiblixError::Manifest { .. })
    ));
}

#[test]
fn a_folder_natively() {
    let dir = std::env::temp_dir().join(format!("biblix-ffi-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("package/usx")).unwrap();
    std::fs::write(dir.join("package/biblix.toml"), MANIFEST).unwrap();
    std::fs::write(dir.join("package/usx/JHN.usx"), USX).unwrap();
    let path = |p: &str| dir.join(p).to_string_lossy().into_owned();
    let package = Biblix::new()
        .open_package_dir(path("package"), defaults(), path("cache"))
        .unwrap();
    package.book(43).unwrap();
    assert!(package.cache_writes().is_empty(), "it writes its own cache");
    assert!(dir.join("cache/test").is_dir());
    std::fs::remove_dir_all(&dir).unwrap();
}
