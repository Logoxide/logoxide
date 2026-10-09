//! The API as the bindings see it: packages from their files' bytes, the cache handed back to the
//! host, topos's translations and books, and a vault's manifest and lock file

use std::collections::HashMap;

use crate::{
    Biblix, BiblixError, BiblixInstaller, BiblixProject, BookStyle, BookTextOptions, Dependency,
    FetchRequest, InspectStep, InstallStep, OffsetUnit, PackageFeatures, PackageFile, PackageKind,
    PackageLocation, Topos, ToposTranslations, UpdateMode, VerseTiming,
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
    assert_eq!(
        package.book_files(),
        [crate::BookFile {
            book_id: 43,
            path: "usx/JHN.usx".into()
        }]
    );
    let usx = Biblix::new()
        .usfm_to_usx("\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n".into())
        .unwrap();
    assert!(usx.contains("<verse number=\"16\""), "{usx}");
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
fn a_collections_packages_follow_it() {
    let manifest = r#"
[dependencies]
books = { git = "https://example.org/books", version = "1" }
romans = { git = "https://example.org/romans", version = "1" }
"#;
    let lock = r#"
version = 1
dependencies = ["books 1.0.0", "romans 1.0.0"]

[[package]]
id = "books"
version = "1.0.0"
git = "https://example.org/books"
dependencies = ["acts 1.0.0", "romans 1.0.0"]

[[package]]
id = "acts"
version = "1.0.0"
git = "https://example.org/acts"

[[package]]
id = "romans"
version = "1.0.0"
git = "https://example.org/romans"
"#;
    let project = Biblix::new()
        .parse_project(Some(manifest.into()), Some(lock.into()))
        .unwrap();
    let listed: Vec<(String, Option<String>)> = (project.packages().into_iter())
        .map(|p| (p.id, p.dependency_of))
        .collect();
    // romans is asked for too: listed once, as asked for
    assert_eq!(
        listed,
        [
            (String::from("books"), None),
            (String::from("acts"), Some(String::from("books"))),
            (String::from("romans"), None),
        ]
    );
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
        .parse_project(Some(manifest.into()), Some(lock.into()))
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
                dir: "packages/bsb/1.2.0+g3f9c2a1b".into()
            },
            &PackageLocation::Path {
                dir: "bibles/mine".into()
            },
        ]
    );
    assert_eq!(packages[0].features.features, ["sword"]);
    assert_eq!(packages[0].checksum.as_deref(), Some("sha256:00"));

    assert!(packages.iter().all(|p| p.dependency_of.is_none()));

    let no_lock = Biblix::new()
        .parse_project(Some(manifest.into()), None)
        .unwrap();
    assert!(no_lock.packages().is_empty());
    assert_eq!(no_lock.unlocked().len(), 3);
    assert!(matches!(
        Biblix::new().parse_project(Some("[dependencies".into()), None),
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

const URL: &str = "https://example.org/someone/test";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
/// Where the test package's version goes in the library: its version and commit
const TEST_DIR: &str = "packages/test/1.0.0+g01234567";

/// A host with one repository (tag v1.0.0 at COMMIT) and a library of files
struct Host {
    library: HashMap<String, Vec<PackageFile>>,
    requests: Vec<FetchRequest>,
}

impl Host {
    fn refs() -> Vec<u8> {
        let pkt = |line: String| format!("{:04x}{line}", line.len() + 4);
        [
            pkt("# service=git-upload-pack\n".into()),
            "0000".into(),
            pkt(format!("{COMMIT} HEAD\0side-band\n")),
            pkt(format!("{COMMIT} refs/tags/v1.0.0\n")),
            "0000".into(),
        ]
        .concat()
        .into_bytes()
    }

    fn archive() -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(gz);
        for file in files() {
            let mut header = tar::Header::new_gnu();
            header.set_size(file.bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            let path = format!("test-{COMMIT}/{}", file.path);
            builder
                .append_data(&mut header, path, &file.bytes[..])
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    /// Answers what the installer asks until it's done
    fn install(
        &mut self,
        installer: &BiblixInstaller,
        project: &BiblixProject,
    ) -> (String, Vec<PackageFile>) {
        loop {
            match installer.install(project, UpdateMode::Nothing).unwrap() {
                InstallStep::Done { lock, files } => return (lock, files),
                InstallStep::Fetch { request } => self.answer(installer, request),
            }
        }
    }

    fn answer(&mut self, installer: &BiblixInstaller, request: FetchRequest) {
        assert!(
            !self.requests.contains(&request),
            "asked twice: {request:?}"
        );
        self.requests.push(request.clone());
        match request {
            FetchRequest::Refs { repository, url } => {
                assert_eq!(url, format!("{URL}.git/info/refs?service=git-upload-pack"));
                installer.provide_refs(repository, Self::refs()).unwrap();
            }
            FetchRequest::Archive {
                repository,
                commit,
                urls,
            } => {
                assert_eq!(urls[0], format!("{URL}/archive/{COMMIT}.tar.gz"));
                installer
                    .provide_archive(repository, commit, Self::archive())
                    .unwrap();
            }
            FetchRequest::Download { url } => {
                let bytes = match url.as_str() {
                    REGISTRY => registry().0.into_bytes(),
                    _ if url == format!("{REGISTRY_DIR}/test/test-1.0.0.tar.gz") => registry().1,
                    _ => panic!("no {url}"),
                };
                installer.provide_download(url, bytes);
            }
            FetchRequest::Files { location } => {
                let PackageLocation::Library { dir } = &location else {
                    panic!("no path packages here");
                };
                match self.library.get(dir) {
                    Some(files) => installer.provide_files(location, files.clone()),
                    None => installer.provide_missing(location),
                }
            }
        }
    }
}

const REGISTRY_DIR: &str = "https://example.org/registry";
const REGISTRY: &str = "https://example.org/registry/catalog.toml";

/// A registry publishing the test package: its catalog's text, and the release's archive
fn registry() -> (String, Vec<u8>) {
    use biblix_core::{Storage, catalog::CatalogRelease, publish};
    let storage = biblix_core::MemoryStorage::new();
    for f in files() {
        storage.write(&f.path, &f.bytes).unwrap();
    }
    let info = biblix_core::Manifest::parse(MANIFEST)
        .unwrap()
        .package
        .unwrap();
    let bytes = publish::pack(&storage, &info).unwrap();
    let release = CatalogRelease {
        version: "1.0.0".into(),
        url: "test/test-1.0.0.tar.gz".into(),
        sha256: publish::sha256(&bytes),
        checksum: Some(biblix_core::package::checksum(&storage).unwrap()),
        size: None,
    };
    let text = publish::add_release(None, "r", &info, None, &release)
        .unwrap()
        .unwrap();
    (text, bytes)
}

#[test]
fn installing_from_a_registry_of_archives() {
    let biblix = Biblix::new();
    let catalog = biblix.parse_catalog(REGISTRY.into(), registry().0).unwrap();
    let entry = &catalog.search("TB".into(), 5)[0];
    assert_eq!(
        entry.source,
        crate::CatalogSource::Registry {
            catalog: REGISTRY.into()
        }
    );
    assert_eq!(
        entry.releases[0].url,
        format!("{REGISTRY_DIR}/test/test-1.0.0.tar.gz")
    );

    let project = biblix.parse_project(None, None).unwrap();
    let dependency = Dependency {
        registry: Some(REGISTRY.into()),
        version: Some("1".into()),
        default_features: true,
        ..Dependency::default()
    };
    let project = project.add_dependency("test".into(), dependency).unwrap();
    let mut host = Host {
        library: HashMap::new(),
        requests: vec![],
    };
    let installer = biblix.installer();
    let (lock, files) = host.install(&installer, &project);
    // The catalog, whether it's installed, then the release's archive: nothing else
    assert!(matches!(
        host.requests.as_slice(),
        [
            FetchRequest::Download { url: catalog },
            FetchRequest::Files { .. },
            FetchRequest::Download { url: archive },
        ] if catalog == REGISTRY && archive.ends_with(".tar.gz")
    ));
    assert!(lock.contains("archive = \"https://example.org/registry/test/test-1.0.0.tar.gz\""));
    let dir = files[0]
        .path
        .split('/')
        .take(3)
        .collect::<Vec<_>>()
        .join("/");
    assert!(dir.starts_with("packages/test/1.0.0+a"), "{dir}");

    // Another device with the lock: the archive alone, by the lock's URL and hash
    let locked = biblix
        .parse_project(Some(project.manifest_text()), Some(lock))
        .unwrap();
    let mut other = Host {
        library: HashMap::new(),
        requests: vec![],
    };
    other.install(&biblix.installer(), &locked);
    assert!(matches!(
        other.requests.as_slice(),
        [FetchRequest::Files { .. }, FetchRequest::Download { url }] if url.ends_with(".tar.gz")
    ));
}

fn added(project: &BiblixProject) -> BiblixProject {
    let dependency = Dependency {
        git: Some(URL.into()),
        version: Some("1.0.0".into()),
        default_features: true,
        ..Dependency::default()
    };
    project.add_dependency("test".into(), dependency).unwrap()
}

#[test]
fn installing_with_the_host_downloading() {
    let biblix = Biblix::new();
    let mut host = Host {
        library: HashMap::new(),
        requests: vec![],
    };

    // Inspecting reads the newest release's manifest
    let installer = biblix.installer();
    let found = loop {
        match installer.inspect(URL.into(), None, None, None).unwrap() {
            InspectStep::Found { inspected } => break inspected,
            InspectStep::Fetch { request } => host.answer(&installer, request),
        }
    };
    assert_eq!(
        (found.info.id.as_str(), found.tag.as_deref()),
        ("test", Some("v1.0.0"))
    );
    assert_eq!(found.commit, COMMIT);

    // A new vault: its manifest gets the dependency, and installing gives the files and the lock
    let project = added(&biblix.parse_project(None, None).unwrap());
    assert!(
        project
            .manifest_text()
            .contains("test = { git = \"https://example.org/someone/test\", version = \"1.0.0\" }")
    );
    assert_eq!(project.unlocked(), ["test"]);
    // The same installer has the refs and archive already: it only asks where the version goes
    let (lock, files) = host.install(&installer, &project);
    assert!(matches!(
        host.requests.last(),
        Some(FetchRequest::Files { .. })
    ));
    assert_eq!(host.requests.len(), 3);
    let mut paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            format!("{TEST_DIR}/biblix.toml"),
            format!("{TEST_DIR}/usx/JHN.usx")
        ]
    );
    let project = biblix
        .parse_project(Some(project.manifest_text()), Some(lock.clone()))
        .unwrap();
    let entry = &project.packages()[0];
    assert_eq!(entry.commit.as_deref(), Some(COMMIT));
    let package = biblix
        .open_package(
            files_under(&files, &format!("{TEST_DIR}/")),
            entry.features.clone(),
            vec![],
        )
        .unwrap();
    assert_eq!(entry.checksum, Some(package.checksum().unwrap()));

    // Another device with the lock file: installed already, it's only checked
    host.library.insert(
        TEST_DIR.into(),
        files_under(&files, &format!("{TEST_DIR}/")),
    );
    host.requests.clear();
    let (again, files) = host.install(&biblix.installer(), &project);
    assert_eq!(again, lock);
    assert!(files.is_empty());
    assert_eq!(host.requests.len(), 1, "{:?}", host.requests);

    // Changed files are caught
    let mut changed = files_under(&[], "");
    changed.push(file("biblix.toml", MANIFEST));
    changed.push(file("usx/JHN.usx", &USX.replace("loved", "LOVED")));
    host.library.insert(TEST_DIR.into(), changed);
    host.requests.clear();
    let installer = biblix.installer();
    let error = loop {
        match installer.install(&project, UpdateMode::Nothing) {
            Ok(InstallStep::Fetch { request }) => host.answer(&installer, request),
            Ok(InstallStep::Done { .. }) => panic!("a changed file was installed"),
            Err(error) => break error,
        }
    };
    assert!(matches!(error, BiblixError::Files { .. }), "{error:?}");

    // Repairing hands back its files to write over the changed one; an annotation beside them
    // isn't the package's, so it changes nothing
    let mut annotated = files_under(&[], "");
    annotated.push(file("biblix.toml", MANIFEST));
    annotated.push(file("usx/JHN.usx", &USX.replace("loved", "LOVED")));
    annotated.push(file("usx/JHN - Annotations.md", "my note"));
    host.library.insert(TEST_DIR.into(), annotated);
    host.requests.clear();
    let installer = biblix.installer();
    let (repaired, files) = loop {
        match installer.install(&project, UpdateMode::Repair).unwrap() {
            InstallStep::Done { lock, files } => break (lock, files),
            InstallStep::Fetch { request } => host.answer(&installer, request),
        }
    };
    assert_eq!(repaired, lock);
    let book = files
        .iter()
        .find(|f| f.path == format!("{TEST_DIR}/usx/JHN.usx"))
        .expect("the changed book is handed back");
    assert_eq!(String::from_utf8(book.bytes.clone()).unwrap(), USX);
    assert!(!files.iter().any(|f| f.path.ends_with(".md")));

    // Removing it leaves the manifest without it
    let removed = project.remove_dependency("test".into()).unwrap();
    assert!(removed.dependencies().is_empty());
}

fn files_under(files: &[PackageFile], dir: &str) -> Vec<PackageFile> {
    (files.iter())
        .filter_map(|f| {
            Some(PackageFile {
                path: f.path.strip_prefix(dir)?.to_string(),
                bytes: f.bytes.clone(),
            })
        })
        .collect()
}

#[test]
fn catalogs_and_ebible_imports() {
    use std::io::Write;

    let biblix = Biblix::new();
    let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
    let ebible = biblix
        .parse_catalog(biblix.ebible_catalog_url(), csv.into())
        .unwrap();
    let mine = biblix
        .parse_catalog(
            "catalog.toml".into(),
            "[[package]]\nid = \"bsb\"\nname = \"Berean Standard Bible\"\nabbreviation = \"BSB\"\nlanguage = \"en\"\ngit = \"https://example.org/bsb\"\n".into(),
        )
        .unwrap();
    let all = mine.merge(&ebible);
    assert_eq!(all.count(), 2);
    let found = all.search("american".into(), 0);
    assert_eq!(found.len(), 1);
    let entry = found[0].clone();
    assert_eq!(
        entry.source,
        crate::CatalogSource::Ebible {
            id: "eng-asv".into(),
            zip_url: "https://ebible.org/Scriptures/eng-asv_usfm.zip".into()
        }
    );
    assert!(matches!(
        &all.search("BSB".into(), 1)[0].source,
        crate::CatalogSource::Git { .. }
    ));

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file(
            "73-JHNeng-asv.usfm",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer
        .write_all(b"\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n")
        .unwrap();
    let zip = writer.finish().unwrap().into_inner();
    let files = biblix.import_ebible(entry, zip).unwrap();
    let package = biblix.open_package(files, defaults(), vec![]).unwrap();
    assert_eq!(
        (package.info().abbreviation.as_str(), package.books()),
        ("ASV", vec![43])
    );

    let project = biblix.parse_project(None, None).unwrap();
    assert_eq!(project.catalogs()[0], biblix.ebible_catalog_url());
    assert_eq!(project.catalogs().len(), 2);
    let mine = project.add_catalog("catalog.toml".into()).unwrap();
    assert_eq!(mine.catalogs()[0], "catalog.toml");
    assert!(
        mine.manifest_text()
            .contains("catalogs = [\"catalog.toml\"]")
    );
    let ebible = biblix.ebible_catalog_url();
    assert!(mine.is_default_catalog(ebible.clone()));
    let fewer = mine.remove_catalog(ebible.clone()).unwrap();
    assert_eq!(fewer.catalogs().len(), 2);
    assert!(!fewer.catalogs().contains(&ebible));
}

#[test]
fn media_by_url_and_hash() {
    let manifest = format!(
        "{MANIFEST}\n[media]\ntimings = \"timings/\"\n\n[[media.files]]\npath = \"JHN.mp3\"\nurl = \"https://example.org/JHN.mp3\"\nsize = 5\nsha256 = \"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\"\n"
    );
    let vtt = "WEBVTT\n\nJHN 3:16\n00:04.000 --> 00:09.500\nFor God so loved the world\n";
    let files = vec![
        file("biblix.toml", &manifest),
        file("usx/JHN.usx", USX),
        file("timings/JHN.vtt", vtt),
    ];
    let biblix = Biblix::new();
    let package = biblix.open_package(files, defaults(), vec![]).unwrap();
    let media = package.media();
    let timings = package.timings(media[0].clone()).unwrap();
    assert_eq!(
        timings,
        [VerseTiming {
            book: "JHN".into(),
            book_id: 43,
            chapter: 3,
            verse: 16,
            last: 16,
            start_ms: 4_000,
            end_ms: 9_500
        }]
    );
    assert_eq!(biblix.parse_timings(vtt.into()).unwrap(), timings);
    assert!(matches!(
        biblix.parse_timings("JHN 3:16".into()),
        Err(BiblixError::Media { .. })
    ));
    assert_eq!(media.len(), 1);
    assert!(
        biblix
            .verify_media(media[0].clone(), b"hello".to_vec())
            .is_ok()
    );
    assert!(matches!(
        biblix.verify_media(media[0].clone(), b"hellO".to_vec()),
        Err(BiblixError::Files { .. })
    ));
    assert!(
        biblix
            .media_store_path(media[0].clone())
            .starts_with("media/2c/2cf24dba")
    );
}

#[test]
fn making_packages() {
    let biblix = Biblix::new();
    // A Bible's USX says nothing about itself, so a name and a language are given
    let given = crate::NewPackage {
        name: Some("My Gospel".into()),
        language: Some("en".into()),
        authors: vec!["Me".into(), "You".into()],
        version: Some("1.0.0".into()),
        ..crate::NewPackage::default()
    };
    assert_eq!(
        biblix
            .file_metadata("JHN.usx".into(), USX.as_bytes().to_vec())
            .unwrap(),
        crate::FileMetadata::default()
    );
    let files = biblix
        .new_package("JHN.usx".into(), USX.as_bytes().to_vec(), given.clone())
        .unwrap();
    let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["biblix.toml", "usx/JHN.usx"]);
    // It opens, as the host would after writing it
    let package = biblix.open_package(files, defaults(), vec![]).unwrap();
    let info = package.info();
    assert_eq!(
        (info.id.as_str(), info.abbreviation.as_str(), info.kind),
        ("my-gospel", "MG", PackageKind::Bible)
    );
    assert_eq!(info.authors, ["Me", "You"]);
    assert!(package.references().unwrap().is_empty());
    assert!(package.epub_files().unwrap().is_empty());
    // Without a language, the error says what to give
    let no_language = crate::NewPackage {
        language: None,
        ..given.clone()
    };
    let error = biblix
        .new_package("JHN.usx".into(), USX.as_bytes().to_vec(), no_language)
        .unwrap_err();
    assert!(matches!(error, BiblixError::Import { .. }), "{error:?}");

    // A collection is its manifest alone
    let collection = biblix
        .new_collection(crate::NewPackage {
            id: Some("library".into()),
            ..given
        })
        .unwrap();
    assert_eq!(collection.len(), 1);
    let text = String::from_utf8(collection[0].bytes.clone()).unwrap();
    assert!(text.contains("kind = \"collection\""), "{text}");

    // A published references file: the passages cited
    let published = biblix_core::references::encode(&[biblix_core::references::Cited {
        passage: "John.3.16".into(),
        count: 3,
    }])
    .unwrap();
    let cited = biblix.parse_references(published).unwrap();
    assert_eq!(
        (cited[0].passage.as_str(), cited[0].count),
        ("John.3.16", 3)
    );
    assert!(biblix.parse_references(b"not one".to_vec()).is_err());
}
