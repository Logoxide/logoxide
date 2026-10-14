//! The API as the bindings see it: packages from their files' bytes, the cache handed back to the
//! host, topos's translations and books, and a vault's manifest and lock file

use std::collections::HashMap;

use crate::{
    BookStyle, BookTextOptions, BookTextStyle, CatalogsStep, Dependency, FetchRequest,
    ImportDownload, InspectStep, InstallStep, Logoxide, LogoxideError, LogoxideInstaller,
    LogoxideProject, MappedVerse, NameClash, NamedBible, NodeKind, OffsetUnit, PackageFeatures,
    PackageFile, PackageKind, PackageLocation, PackageManifest, Topos, ToposTranslations,
    UpdateMode, VerseTiming,
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
    vec![file("logoxide.toml", MANIFEST), file("usx/JHN.usx", USX)]
}

fn defaults() -> PackageFeatures {
    PackageFeatures {
        features: vec![],
        default_features: true,
    }
}

#[test]
fn a_package_from_its_files() {
    let package = Logoxide::new()
        .open_package(files(), defaults(), vec![])
        .unwrap();
    let info = package.info();
    assert_eq!(
        (info.id.as_str(), info.abbreviation.as_deref(), info.kind),
        ("test", Some("TB"), PackageKind::Bible)
    );
    assert!(info.right_to_left);
    assert_eq!(info.license.as_deref(), Some("CC0-1.0"));
    assert_eq!(package.format(), "usx");
    assert_eq!(package.books(), [43]);
    assert_eq!(
        package.book_files(),
        [crate::BookFile {
            book_id: 43,
            path: "usx/JHN.usx".into(),
            // The file names no title: the book's name in English
            title: Some("John".into()),
        }]
    );
    let usx = Logoxide::new()
        .usfm_to_usx("\\id JHN\n\\c 3\n\\p\n\\v 16 For God so loved the world.\n".into())
        .unwrap();
    assert!(usx.contains("<verse number=\"16\""), "{usx}");
    assert_eq!(package.chapter_verses(43), [0, 0, 18]);
    // Each verse's words hashed: what links record to find a verse again
    let hashes = package.verse_hashes(43).unwrap();
    assert_eq!(
        hashes
            .iter()
            .map(|v| (v.chapter, v.verse))
            .collect::<Vec<_>>(),
        [(3, 16), (3, 17), (3, 18)]
    );
    assert!(hashes.iter().all(|v| v.hash.len() == 14 && v.part == 0));
    assert_ne!(hashes[0].hash, hashes[1].hash);
    assert!(package.chapter_verses(1).is_empty());
    assert!(package.checksum().unwrap().starts_with("sha256:"));
    let missing = vec![file("logoxide.toml", MANIFEST)];
    assert!(matches!(
        Logoxide::new().open_package(missing, defaults(), vec![]),
        Err(LogoxideError::NotFound { .. } | LogoxideError::Io { .. })
    ));
}

#[test]
fn the_host_keeps_the_cache() {
    let package = Logoxide::new()
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
    let again = Logoxide::new()
        .open_package(files(), defaults(), cache)
        .unwrap();
    again.book(43).unwrap();
    assert!(again.cache_writes().is_empty());
}

#[test]
fn topos_reads_its_books_and_verse_counts() {
    let topos = Topos::new();
    let package = Logoxide::new()
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
    assert!(matches!(
        package.book(1),
        Err(LogoxideError::NotFound { .. })
    ));
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
    let project = Logoxide::new()
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

    // Turned off: still listed, but not in use, and neither is what it brings in (romans is
    // asked for itself, so it is)
    let off = project.set_enabled("books".into(), false).unwrap();
    assert!(!off.is_enabled("books".into()));
    assert_eq!(off.enabled_dependencies(), ["romans"]);
    assert_eq!(off.disabled_dependencies(), ["books"]);
    assert!(off.manifest_text().contains("enabled = false"));
    let enabled: Vec<(String, bool)> = (off.packages().into_iter())
        .map(|p| (p.id, p.enabled))
        .collect();
    assert_eq!(
        enabled,
        [
            (String::from("books"), false),
            (String::from("acts"), false),
            (String::from("romans"), true),
        ]
    );
    assert!(project.set_enabled("nope".into(), false).is_err());
    // A package that's off is named from its logoxide.toml alone
    let info = Logoxide::new()
        .read_package_info("[package]\nid = \"gill\"\nname = \"Gill's Exposition\"\nversion = \"1.0.0\"\nabbreviation = \"Gill\"\nlanguage = \"en\"\nkind = \"collection\"\n".into())
        .unwrap();
    assert_eq!(
        (info.id.as_str(), info.name.as_str()),
        ("gill", "Gill's Exposition")
    );
    assert!(
        Logoxide::new()
            .read_package_info("[dependencies]\n".into())
            .is_err()
    );
    let on = off.set_enabled("books".into(), true).unwrap();
    assert!(on.packages().iter().all(|p| p.enabled));
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
dependencies = ["bsb 1.2.0+g3f9c2a1b", "mine 0.1.0"]

[[package]]
id = "bsb"
version = "1.2.0"
git = "https://example.org/bsb"
tag = "v1.2.0"
commit = "3f9c2a1b770123456789abcdef0123456789abcd"
checksum = "sha256:00"
features = ["sword"]

[[package]]
id = "mine"
version = "0.1.0"
path = "bibles/mine"
"#;
    let project = Logoxide::new()
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

    // Two Bibles answering to one name are said
    let named = |label: &str, abbreviation: &str, aliases: &[&str]| NamedBible {
        label: label.into(),
        abbreviation: abbreviation.into(),
        aliases: aliases.iter().map(|a| a.to_string()).collect(),
    };
    let clashes = project.name_clashes(vec![
        named("kjv 2.0.0", "KJV", &["AV"]),
        named("kjv 1.0.0", "KJV", &["av", "AV1611"]),
        named("bsb 1.2.0", "BSB", &[]),
    ]);
    assert_eq!(
        clashes,
        [
            NameClash {
                name: "KJV".into(),
                packages: vec!["kjv 2.0.0".into(), "kjv 1.0.0".into()]
            },
            NameClash {
                name: "AV".into(),
                packages: vec!["kjv 2.0.0".into(), "kjv 1.0.0".into()]
            },
        ]
    );

    let no_lock = Logoxide::new()
        .parse_project(Some(manifest.into()), None)
        .unwrap();
    assert!(no_lock.packages().is_empty());
    assert_eq!(no_lock.unlocked().len(), 3);
    assert!(matches!(
        Logoxide::new().parse_project(Some("[dependencies".into()), None),
        Err(LogoxideError::Invalid { .. })
    ));
}

#[test]
fn a_folder_natively() {
    let dir = std::env::temp_dir().join(format!("logoxide-ffi-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("package/usx")).unwrap();
    std::fs::write(dir.join("package/logoxide.toml"), MANIFEST).unwrap();
    std::fs::write(dir.join("package/usx/JHN.usx"), USX).unwrap();
    let path = |p: &str| dir.join(p).to_string_lossy().into_owned();
    let package = Logoxide::new()
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
        installer: &LogoxideInstaller,
        project: &LogoxideProject,
    ) -> (String, Vec<PackageFile>) {
        loop {
            match installer.install(project, UpdateMode::Nothing).unwrap() {
                InstallStep::Done { lock, files } => return (lock, files),
                InstallStep::Fetch { request } => self.answer(installer, request),
            }
        }
    }

    fn answer(&mut self, installer: &LogoxideInstaller, request: FetchRequest) {
        assert!(
            !self.requests.contains(&request),
            "asked twice: {request:?}"
        );
        self.requests.push(request.clone());
        match request {
            FetchRequest::ImportEntry { import } => panic!("no imports here: {import}"),
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
            FetchRequest::Download { url, .. } => {
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
    use logoxide_core::{Storage, catalog::CatalogRelease, publish};
    let storage = logoxide_core::MemoryStorage::new();
    for f in files() {
        storage.write(&f.path, &f.bytes).unwrap();
    }
    let info = logoxide_core::Manifest::parse(MANIFEST)
        .unwrap()
        .package
        .unwrap();
    let bytes = publish::pack(&storage, &info).unwrap();
    let release = CatalogRelease {
        version: "1.0.0".into(),
        url: "test/test-1.0.0.tar.gz".into(),
        mirrors: vec!["https://mirror.example.org/test-1.0.0.tar.gz".into()],
        sha256: publish::sha256(&bytes),
        checksum: Some(logoxide_core::package::checksum(&storage).unwrap()),
        size: None,
        yanked: false,
    };
    let text = publish::add_release(None, "r", &info, None, &release)
        .unwrap()
        .unwrap();
    (text, bytes)
}

#[test]
fn installing_from_a_registry_of_archives() {
    let logoxide = Logoxide::new();
    let catalog = logoxide
        .parse_catalog(REGISTRY.into(), registry().0)
        .unwrap();
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

    let project = logoxide.parse_project(None, None).unwrap();
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
    let installer = logoxide.installer();
    let (lock, files) = host.install(&installer, &project);
    // The catalog, whether it's installed, then the release's archive: nothing else
    assert!(matches!(
        host.requests.as_slice(),
        [
            FetchRequest::Download { url: catalog, .. },
            FetchRequest::Files { .. },
            FetchRequest::Download { url: archive, urls },
        ] if catalog == REGISTRY
            && archive.ends_with(".tar.gz")
            && urls[1] == "https://mirror.example.org/test-1.0.0.tar.gz"
    ));
    assert!(lock.contains("mirrors = [\"https://mirror.example.org/test-1.0.0.tar.gz\"]"));
    assert!(lock.contains("archive = \"https://example.org/registry/test/test-1.0.0.tar.gz\""));
    let dir = files[0]
        .path
        .split('/')
        .take(3)
        .collect::<Vec<_>>()
        .join("/");
    assert!(dir.starts_with("packages/test/1.0.0+a"), "{dir}");

    // Another device with the lock: the archive alone, by the lock's URL and hash
    let locked = logoxide
        .parse_project(Some(project.manifest_text()), Some(lock))
        .unwrap();
    let mut other = Host {
        library: HashMap::new(),
        requests: vec![],
    };
    other.install(&logoxide.installer(), &locked);
    assert!(matches!(
        other.requests.as_slice(),
        [FetchRequest::Files { .. }, FetchRequest::Download { url, urls }]
            if url.ends_with(".tar.gz") && urls.len() == 2
    ));
}

fn added(project: &LogoxideProject) -> LogoxideProject {
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
    let logoxide = Logoxide::new();
    let mut host = Host {
        library: HashMap::new(),
        requests: vec![],
    };

    // Inspecting reads the newest release's manifest
    let installer = logoxide.installer();
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
    let project = added(&logoxide.parse_project(None, None).unwrap());
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
            format!("{TEST_DIR}/logoxide.toml"),
            format!("{TEST_DIR}/usx/JHN.usx")
        ]
    );
    let project = logoxide
        .parse_project(Some(project.manifest_text()), Some(lock.clone()))
        .unwrap();
    let entry = &project.packages()[0];
    assert_eq!(entry.commit.as_deref(), Some(COMMIT));
    let package = logoxide
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
    let (again, files) = host.install(&logoxide.installer(), &project);
    assert_eq!(again, lock);
    assert!(files.is_empty());
    assert_eq!(host.requests.len(), 1, "{:?}", host.requests);

    // Changed files are caught
    let mut changed = files_under(&[], "");
    changed.push(file("logoxide.toml", MANIFEST));
    changed.push(file("usx/JHN.usx", &USX.replace("loved", "LOVED")));
    host.library.insert(TEST_DIR.into(), changed);
    host.requests.clear();
    let installer = logoxide.installer();
    let error = loop {
        match installer.install(&project, UpdateMode::Nothing) {
            Ok(InstallStep::Fetch { request }) => host.answer(&installer, request),
            Ok(InstallStep::Done { .. }) => panic!("a changed file was installed"),
            Err(error) => break error,
        }
    };
    assert!(
        matches!(error, LogoxideError::Integrity { .. }),
        "{error:?}"
    );

    // Repairing hands back its files to write over the changed one; an annotation beside them
    // isn't the package's, so it changes nothing
    let mut annotated = files_under(&[], "");
    annotated.push(file("logoxide.toml", MANIFEST));
    annotated.push(file("usx/JHN.usx", &USX.replace("loved", "LOVED")));
    annotated.push(file("usx/JHN - Annotations.md", "my note"));
    host.library.insert(TEST_DIR.into(), annotated);
    host.requests.clear();
    let installer = logoxide.installer();
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

    let logoxide = Logoxide::new();
    let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
    let ebible = logoxide
        .parse_catalog(logoxide.ebible_catalog_url(), csv.into())
        .unwrap();
    let mine = logoxide
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
    // The work logoxide's table knows ebible:eng-asv to be, and its group
    assert_eq!(entry.work.as_deref(), Some("asv"));
    let groups = all.search_grouped("american".into(), 0);
    assert_eq!(
        (groups.len(), groups[0].name.as_deref()),
        (1, Some("American Standard Version"))
    );
    // Every work, with the packages claiming it
    let asv = all.works("american".into());
    assert_eq!(asv.len(), 1);
    assert_eq!(asv[0].from, crate::WorkFrom::Logoxide);
    assert!(asv[0].entries.iter().any(|e| e.id == "ebible-eng-asv"));
    let zip_url = "https://ebible.org/Scriptures/eng-asv_usfm.zip";
    assert_eq!(
        entry.source,
        crate::CatalogSource::External {
            import: "ebible:eng-asv".into(),
            upstream: Some(zip_url.into()),
            tracks_versions: true,
            downloads: vec![ImportDownload {
                name: "zip".into(),
                urls: vec![zip_url.into()]
            }],
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
    // The import of any publisher: what it downloads (the entry's own list), then its bytes by
    // name
    let downloads = logoxide.import_downloads(entry.clone()).unwrap();
    assert_eq!(
        downloads,
        [ImportDownload {
            name: "zip".into(),
            urls: vec![zip_url.into()]
        }]
    );
    let built = (logoxide.import_downloaded(entry.clone(), vec![file("zip", "")]))
        .expect_err("an empty zip isn't one");
    assert!(matches!(built, LogoxideError::Invalid { .. }), "{built:?}");
    let by_name = vec![PackageFile {
        path: "zip".into(),
        bytes: zip,
    }];
    let files = logoxide.import_downloaded(entry, by_name).unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["logoxide.toml", "usfm/73-JHNeng-asv.usfm"]);
    let package = logoxide.open_package(files, defaults(), vec![]).unwrap();
    assert_eq!(
        (package.info().abbreviation.as_deref(), package.books()),
        (Some("ASV"), vec![43])
    );

    let project = logoxide.parse_project(None, None).unwrap();
    assert_eq!(project.catalogs()[0], logoxide.ebible_catalog_url());
    assert_eq!(project.catalogs(), logoxide.default_catalog_urls());
    assert!(logoxide.is_insecure_url("http://ftp.xiphos.org/".into()));
    assert_eq!(project.catalogs().len(), 5);
    let mine = project.add_catalog("catalog.toml".into()).unwrap();
    assert_eq!(mine.catalogs()[0], "catalog.toml");
    assert!(
        mine.manifest_text()
            .contains("catalogs = [\"catalog.toml\"]")
    );
    let ebible = logoxide.ebible_catalog_url();
    assert!(mine.is_default_catalog(ebible.clone()));
    let fewer = mine.remove_catalog(ebible.clone()).unwrap();
    assert_eq!(fewer.catalogs().len(), 5);
    assert!(!fewer.catalogs().contains(&ebible));
}

/// An `import` dependency installed through the host: its entry and download asked for, made,
/// kept in the library by its checksum, and made again from the lock on another device
#[test]
fn import_dependencies_install_through_the_host() {
    use std::io::Write;

    let logoxide = Logoxide::new();
    let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
    let catalog = logoxide
        .parse_catalog(logoxide.ebible_catalog_url(), csv.into())
        .unwrap();
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
    let zip_url = "https://ebible.org/Scriptures/eng-asv_usfm.zip";
    let project = (logoxide.parse_project(None, None).unwrap())
        .add_dependency(
            "ebible-eng-asv".into(),
            Dependency {
                import: Some("ebible:eng-asv".into()),
                default_features: true,
                ..Dependency::default()
            },
        )
        .unwrap();
    assert!(
        project
            .manifest_text()
            .contains("ebible-eng-asv = { import = \"ebible:eng-asv\" }")
    );
    // Answers what an install asks, counting the requests
    let run = |installer: &LogoxideInstaller, project: &LogoxideProject| {
        let mut asked = vec![];
        loop {
            match installer.install(project, UpdateMode::Nothing).unwrap() {
                InstallStep::Done { lock, files } => return (lock, files, asked),
                InstallStep::Fetch { request } => {
                    asked.push(request.clone());
                    match request {
                        FetchRequest::ImportEntry { import } => {
                            let entry = catalog.find_import(import).unwrap();
                            installer.provide_import_entry(entry);
                        }
                        FetchRequest::Download { url, .. } => {
                            assert_eq!(url, zip_url);
                            installer.provide_download(url, zip.clone());
                        }
                        FetchRequest::Files { location } => installer.provide_missing(location),
                        other => panic!("{other:?}"),
                    }
                }
            }
        }
    };
    let installer = logoxide.installer();
    let (lock, files, asked) = run(&installer, &project);
    assert!(asked.contains(&FetchRequest::ImportEntry {
        import: "ebible:eng-asv".into()
    }));
    assert!(lock.contains("import = \"ebible:eng-asv\""), "{lock}");
    assert!(lock.contains("[[package.download]]"), "{lock}");
    let dir = files[0]
        .path
        .split('/')
        .take(3)
        .collect::<Vec<_>>()
        .join("/");
    assert!(dir.starts_with("packages/ebible-eng-asv/1.0.0+i"), "{dir}");
    let entry = &(logoxide.parse_project(Some(project.manifest_text()), Some(lock.clone())))
        .unwrap()
        .packages()[0];
    assert_eq!(entry.import.as_deref(), Some("ebible:eng-asv"));
    assert_eq!(
        entry.location,
        PackageLocation::Library { dir: dir.clone() }
    );

    // Another device with the lock: made again from the locked download, the same files
    let locked =
        (logoxide.parse_project(Some(project.manifest_text()), Some(lock.clone()))).unwrap();
    let (again, files_again, _) = run(&logoxide.installer(), &locked);
    assert_eq!(again, lock);
    assert_eq!(files_again, files);
}

#[test]
fn ccel_catalog_and_import() {
    let logoxide = Logoxide::new();
    let page = r#"<h2><a href="https://ccel.org/ccel/bible/kjv/kjv">King James Version of the Holy Bible</a></h2>"#;
    let catalog = logoxide
        .parse_catalog("https://ccel.org/ccel/bible/".into(), page.into())
        .unwrap();
    let entry = catalog.search("king james".into(), 0)[0].clone();
    let url = "https://ccel.org/ccel/bible/kjv.xml";
    assert_eq!(
        entry.source,
        crate::CatalogSource::External {
            import: "ccel:kjv".into(),
            upstream: Some(url.into()),
            tracks_versions: false,
            downloads: vec![ImportDownload {
                name: "thml".into(),
                urls: vec![url.into()]
            }],
        }
    );
    let xml = r#"<?xml version="1.0"?><ThML><ThML.head><electronicEdInfo><version>1.0</version>
<DC><DC.Title>The King James Version</DC.Title><DC.Language>eng</DC.Language><DC.Rights>Public Domain</DC.Rights></DC>
</electronicEdInfo></ThML.head><ThML.body><div1><p><scripture passage="John 3.16" parsed="|John|3|16|0|0" osisRef="Bible:John.3.16" />For God so loved the world.</p></div1></ThML.body></ThML>"#;
    let files = logoxide
        .import_downloaded(entry.clone(), vec![file("thml", xml)])
        .unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["COPYRIGHT.txt", "logoxide.toml", "thml/kjv.xml"]);
    let package = logoxide.open_package(files, defaults(), vec![]).unwrap();
    let info = package.info();
    assert_eq!(
        (
            info.language.as_str(),
            info.version.as_str(),
            package.books()
        ),
        ("en", "1.0.0", vec![43])
    );
    // A page in the file's place (a work CCEL restricts)
    assert!(
        logoxide
            .import_downloaded(entry, vec![file("thml", "<!DOCTYPE html><html></html>")])
            .is_err()
    );
}

#[test]
fn sword_catalog_and_import() {
    use std::io::Write;
    use sword_modules::{Testament, VerseKey, Versification};

    let logoxide = Logoxide::new();
    let conf = "[Test]\nDataPath=./modules/texts/rawtext/test/\nModDrv=RawText\nSourceType=OSIS\nEncoding=UTF-8\nLang=en\nVersion=2.1\nDescription=Test Bible\nDistributionLicense=Public Domain\n";
    // The repository's catalog: every module's conf in a mods.d.tar.gz
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(conf.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "mods.d/test.conf", conf.as_bytes())
        .unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar.into_inner().unwrap()).unwrap();
    let location = "https://crosswire.org/ftpmirror/pub/sword/raw/mods.d.tar.gz";
    let catalog = logoxide
        .parse_catalog_bytes(location.into(), gz.finish().unwrap())
        .unwrap();
    let entry = catalog.search("test".into(), 0)[0].clone();
    let crate::CatalogSource::External {
        import,
        upstream,
        tracks_versions,
        downloads,
    } = &entry.source
    else {
        panic!("{:?}", entry.source)
    };
    let zip_url = "https://crosswire.org/ftpmirror/pub/sword/raw/packages/Test.zip";
    assert_eq!(import, "sword:crosswire/Test");
    assert_eq!(upstream.as_deref(), Some(zip_url));
    assert!(tracks_versions);
    // The module's zip, by its name or lowercase: the first that answers
    assert_eq!((downloads.len(), downloads[0].name.as_str()), (1, "zip"));
    assert_eq!(downloads[0].urls[0], zip_url);
    assert_eq!(downloads[0].urls.len(), 2);

    // The module's zip: a RawText module with John 3:16
    let v11n = Versification::kjv();
    let mut index = vec![0u8; v11n.slot_count(Testament::New) as usize * 6];
    let text = "For God so loved the world.";
    let key = VerseKey::from_osis("John", 3, 16).unwrap();
    let at = v11n.index_of(&key).unwrap().slot as usize * 6;
    index[at + 4..at + 6].copy_from_slice(&(text.len() as u16).to_le_bytes());
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let dir = "modules/texts/rawtext/test";
    for (path, bytes) in [
        ("mods.d/test.conf".to_string(), conf.as_bytes()),
        (format!("{dir}/nt.vss"), &index[..]),
        (format!("{dir}/nt"), text.as_bytes()),
    ] {
        writer
            .start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    let zip = writer.finish().unwrap().into_inner();
    let files = logoxide
        .import_downloaded(
            entry,
            vec![PackageFile {
                path: "zip".into(),
                bytes: zip,
            }],
        )
        .unwrap();
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["COPYRIGHT.txt", "logoxide.toml", "osis/John.xml"]);
    let package = logoxide.open_package(files, defaults(), vec![]).unwrap();
    let info = package.info();
    assert_eq!(
        (
            info.version.as_str(),
            info.license.as_deref(),
            package.books()
        ),
        ("1.0.0", Some("LicenseRef-PublicDomain"), vec![43])
    );
    let upstream = info.upstream.clone().unwrap();
    assert_eq!(upstream.version.as_deref(), Some("2.1"));
    // Converted into OSIS files; public domain, so nothing to say before sharing it
    assert!(upstream.converted);
    assert_eq!(info.rights.redistributable, Some(true));
    assert_eq!(info.rights.derivatives, Some(crate::Derivatives::Allowed));
    assert_eq!(package.license_concerns(), vec![]);
    // A text catalog reads as bytes too
    let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n";
    let ebible =
        logoxide.parse_catalog_bytes(logoxide.ebible_catalog_url(), csv.as_bytes().to_vec());
    assert_eq!(ebible.unwrap().count(), 0);
}

#[test]
fn updating_an_import() {
    use std::io::Write;
    let logoxide = Logoxide::new();
    let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
    let catalog = logoxide
        .parse_catalog(logoxide.ebible_catalog_url(), csv.into())
        .unwrap();
    let zip = |books: &[(&str, &str)]| {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, text) in books {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    };
    let john = |text: &str| format!("\\id JHN\n\\c 3\n\\p\n\\v 16 {text}\n");
    let entry = catalog.search("asv".into(), 1)[0].clone();
    let downloaded = |bytes: Vec<u8>| {
        vec![PackageFile {
            path: "zip".into(),
            bytes,
        }]
    };
    let current = logoxide
        .import_downloaded(
            entry.clone(),
            downloaded(zip(&[
                ("73-JHNeng-asv.usfm", &john("For God so loved the world.")),
                (
                    "44-ACTeng-asv.usfm",
                    "\\id ACT\n\\c 1\n\\p\n\\v 1 The former.\n",
                ),
            ])),
        )
        .unwrap();
    // The installed import names its entry
    let package = logoxide
        .open_package(current.clone(), defaults(), vec![])
        .unwrap();
    let upstream = package.info().upstream.clone().unwrap();
    assert_eq!(catalog.find_upstream(upstream.clone()), Some(entry.clone()));
    assert_eq!(upstream.version.as_deref(), Some("2026-10-08"));
    assert!(logoxide.import_is_current(entry.clone(), upstream.version.clone()));
    assert!(!logoxide.import_is_current(entry.clone(), Some("2026-01-01".into())));
    assert!(!logoxide.import_is_current(entry.clone(), None));

    assert_eq!(
        logoxide
            .update_import(current.clone(), current.clone())
            .unwrap(),
        None
    );
    let fresh = logoxide
        .import_downloaded(
            entry,
            downloaded(zip(&[(
                "73-JHNeng-asv.usfm",
                &john("For God so much loved the world."),
            )])),
        )
        .unwrap();
    let update = logoxide.update_import(current, fresh).unwrap().unwrap();
    assert_eq!(
        (update.from.as_str(), update.to.as_str()),
        ("1.0.0", "2.0.0")
    );
    assert_eq!(update.upstream_to.as_deref(), Some("2026-10-08"));
    assert_eq!(update.change, crate::ChangeKind::Major);
    assert_eq!(update.removed, ["usfm/44-ACTeng-asv.usfm"]);
    assert!(
        update.description.contains("1 verse reworded"),
        "{}",
        update.description
    );
    let manifest = update
        .files
        .iter()
        .find(|f| f.path == "logoxide.toml")
        .unwrap();
    assert!(String::from_utf8_lossy(&manifest.bytes).contains("version = \"2.0.0\""));
}

#[test]
fn media_by_url_and_hash() {
    let manifest = format!(
        "{MANIFEST}\n[media]\ntimings = \"timings/\"\n\n[[media.files]]\npath = \"JHN.mp3\"\nurl = \"https://example.org/JHN.mp3\"\nsize = 5\nsha256 = \"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\"\n"
    );
    let vtt = "WEBVTT\n\nJHN 3:16\n00:04.000 --> 00:09.500\nFor God so loved the world\n";
    let files = vec![
        file("logoxide.toml", &manifest),
        file("usx/JHN.usx", USX),
        file("timings/JHN.vtt", vtt),
    ];
    let logoxide = Logoxide::new();
    let package = logoxide.open_package(files, defaults(), vec![]).unwrap();
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
    assert_eq!(logoxide.parse_timings(vtt.into()).unwrap(), timings);
    assert!(matches!(
        logoxide.parse_timings("JHN 3:16".into()),
        Err(LogoxideError::Invalid { .. })
    ));
    assert_eq!(media.len(), 1);
    assert!(
        logoxide
            .verify_media(media[0].clone(), b"hello".to_vec())
            .is_ok()
    );
    assert!(matches!(
        logoxide.verify_media(media[0].clone(), b"hellO".to_vec()),
        Err(LogoxideError::Integrity { .. })
    ));
    assert!(
        logoxide
            .media_store_path(media[0].clone())
            .starts_with("media/2c/2cf24dba")
    );
}

#[test]
fn making_packages() {
    let logoxide = Logoxide::new();
    // A Bible's USX says nothing about itself, so a name and a language are given
    let given = crate::NewPackage {
        name: Some("My Gospel".into()),
        language: Some("en".into()),
        authors: vec!["Me".into(), "You".into()],
        version: Some("1.0.0".into()),
        ..crate::NewPackage::default()
    };
    assert_eq!(
        logoxide
            .file_metadata("JHN.usx".into(), USX.as_bytes().to_vec())
            .unwrap(),
        crate::FileMetadata::default()
    );
    let files = logoxide
        .new_package("JHN.usx".into(), USX.as_bytes().to_vec(), given.clone())
        .unwrap();
    let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["logoxide.toml", "usx/JHN.usx"]);
    // It opens, as the host would after writing it
    let package = logoxide.open_package(files, defaults(), vec![]).unwrap();
    let info = package.info();
    assert_eq!(
        (info.id.as_str(), info.abbreviation.as_deref(), info.kind),
        ("my-gospel", Some("MG"), PackageKind::Bible)
    );
    assert_eq!(info.authors, ["Me", "You"]);
    assert!(package.references().unwrap().is_empty());
    assert!(package.epub_files().unwrap().is_empty());
    // Without a language, the error says what to give
    let no_language = crate::NewPackage {
        language: None,
        ..given.clone()
    };
    let error = logoxide
        .new_package("JHN.usx".into(), USX.as_bytes().to_vec(), no_language)
        .unwrap_err();
    assert!(matches!(error, LogoxideError::Invalid { .. }), "{error:?}");

    // A collection is its manifest alone
    let collection = logoxide
        .new_collection(crate::NewPackage {
            id: Some("library".into()),
            ..given
        })
        .unwrap();
    assert_eq!(collection.len(), 1);
    let text = String::from_utf8(collection[0].bytes.clone()).unwrap();
    assert!(text.contains("kind = \"collection\""), "{text}");

    // A published references file: the passages cited
    let published = logoxide_core::references::encode(&[logoxide_core::references::Cited {
        passage: "John.3.16".into(),
        count: 3,
    }])
    .unwrap();
    let cited = logoxide.parse_references(published).unwrap();
    assert_eq!(
        (cited[0].passage.as_str(), cited[0].count),
        ("John.3.16", 3)
    );
    assert!(logoxide.parse_references(b"not one".to_vec()).is_err());
}

#[test]
fn versification_maps_map_references() {
    let fixture = |name: &str| {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../logoxide/tests/fixtures/english-original/"
        );
        std::fs::read_to_string(format!("{dir}{name}")).unwrap()
    };
    let logoxide = Logoxide::new();
    let files = vec![
        file("logoxide.toml", &fixture("logoxide.toml")),
        file("eng-org.vrs", &fixture("eng-org.vrs")),
    ];
    let map = logoxide.open_package(files, defaults(), vec![]).unwrap();
    let info = map.info();
    assert_eq!(info.kind, PackageKind::VersificationMap);
    assert_eq!(
        (info.versification.as_str(), info.maps_to.as_deref()),
        ("English", Some("Original"))
    );
    let maps = logoxide.versifications().with_map(&map).unwrap();
    assert_eq!(maps.names(), ["English", "Original"]);
    let mapped = maps
        .map(
            "Psalm 51:1-2".into(),
            "English".into(),
            "Original".into(),
            None,
        )
        .unwrap();
    assert_eq!(
        (mapped.text.as_str(), mapped.osis.as_str()),
        ("Psalm 51:3-4", "Ps.51.3-Ps.51.4")
    );
    assert_eq!(
        mapped.verses[0],
        MappedVerse {
            book: 19,
            chapter: 51,
            verse: 3
        }
    );
    // Hebrew Malachi has no chapter 4
    let none = maps
        .map(
            "Malachi 4:1".into(),
            "Original".into(),
            "English".into(),
            None,
        )
        .unwrap();
    assert!(none.verses.is_empty());
    assert_eq!(none.missing_text, "Malachi 4:1");
    // From a .vrs file's text, and through both maps
    let maps = maps
        .with_vrs(
            "Vulgate".into(),
            "Original".into(),
            "PSA 50:1-21 = PSA 51:1-21".into(),
        )
        .unwrap();
    let vulgate = maps
        .map(
            "Psalm 51:1".into(),
            "english".into(),
            "vulgate".into(),
            None,
        )
        .unwrap();
    assert_eq!(vulgate.text, "Psalm 50:3");
    assert!(matches!(
        maps.map("Psalm 51:1".into(), "English".into(), "LXX".into(), None),
        Err(LogoxideError::Invalid { .. })
    ));
}

#[test]
fn lists_the_categories() {
    let logoxide = crate::Logoxide::new();
    let categories = logoxide.categories();
    assert_eq!(categories.len(), 21);
    assert_eq!(categories[0].name, "biblical-theology");
    assert!(categories.iter().any(|c| c.name == "church-history"));
    assert_eq!(logoxide.max_categories(), 5);
}

const COMMENTARY: &str = "[package]\nid = \"mhcc\"\nname = \"A Commentary\"\nabbreviation = \"MHCC\"\nversion = \"1.0.0\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\"\nkind = \"commentary\"\n\n[formats]\nosis = { path = \"osis/\" }\n";

const DICTIONARY: &str = "[package]\nid = \"strongs\"\nname = \"Strong's Greek\"\nabbreviation = \"STR\"\nversion = \"1.0.0\"\nlanguage = \"grc\"\nlicense = \"CC0-1.0\"\nkind = \"lexicon\"\nkeys = \"strongs-greek\"\n\n[formats]\ndictionary = { path = \"entries/\" }\n";

const XREF: &str = "[package]\nid = \"tsk\"\nname = \"Treasury\"\nabbreviation = \"TSK\"\nversion = \"1.0.0\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\"\nkind = \"cross-references\"\n\n[formats]\nxref = { path = \"xref/\" }\n";

#[test]
fn commentaries_dictionaries_and_cross_references() {
    let logoxide = Logoxide::new();
    let topos = Topos::new();
    let options = BookTextOptions {
        notes: false,
        headings: false,
        class_prefix: "topos-".into(),
    };
    let passage = |s: &str| topos.parse(s.into(), BookStyle::Name).unwrap();

    // A commentary in OSIS as SWORD imports write it: a note on 3:16 and one on 3:17-18
    let commentary = logoxide
        .open_package(
            vec![
                file("logoxide.toml", COMMENTARY),
                file(
                    "osis/John.xml",
                    r#"<osis><osisText osisIDWork="MHCC"><div type="book" osisID="John"><chapter osisID="John.3" sID="John.3"/><verse osisID="John.3.16" sID="John.3.16" n="16"/>God so loved: the <hi type="italic">world</hi>.<verse eID="John.3.16"/><verse osisID="John.3.17 John.3.18" sID="John.3.17" n="17-18"/>Not to condemn.<verse eID="John.3.17"/><chapter eID="John.3"/></div></osisText></osis>"#,
                ),
            ],
            defaults(),
            vec![],
        )
        .unwrap();
    let notes = commentary
        .notes(passage("John 3:16-17"), options.clone(), false)
        .unwrap();
    let verses: Vec<(u8, u8, u8, u8, &str)> = (notes.iter())
        .map(|n| (n.chapter, n.verse, n.last_chapter, n.last, n.text.as_str()))
        .collect();
    assert_eq!(
        verses,
        [
            (3, 16, 3, 16, "God so loved: the world."),
            (3, 17, 3, 18, "Not to condemn."),
        ]
    );
    assert!(notes[0].html.contains("world"), "{}", notes[0].html);
    assert!(notes[0].html.contains("topos-"), "{}", notes[0].html);
    // A verse it lacks, or a book: nothing
    assert!(
        commentary
            .notes(passage("John 4:1"), options.clone(), false)
            .unwrap()
            .is_empty()
    );
    assert!(
        commentary
            .notes(passage("Romans 1:1"), options.clone(), false)
            .unwrap()
            .is_empty()
    );
    // A verse it skips gets the note before it in the chapter only when stretched
    let stretched = (commentary.notes(passage("John 3:19"), options.clone(), true)).unwrap();
    assert_eq!(stretched.len(), 1);
    assert_eq!((stretched[0].verse, stretched[0].last), (17, 18));
    // A commentary has no entries or keys
    assert!(commentary.keys().is_empty());
    assert_eq!(
        commentary.entry("G25".into(), options.clone()).unwrap(),
        None
    );

    // A Strong's dictionary: a TEI entry with a headword, and a plain one
    let dictionary = logoxide
        .open_package(
            vec![
                file("logoxide.toml", DICTIONARY),
                file(
                    "entries/greek.xml",
                    "<entries keys=\"strongs-greek\" markup=\"tei\">\n<entry key=\"G0025\" headword=\"ἀγαπάω\"><entryFree><orth>ἀγαπάω</orth> to love</entryFree></entry>\n<entry key=\"G26\">love</entry>\n</entries>\n",
                ),
            ],
            defaults(),
            vec![],
        )
        .unwrap();
    assert_eq!(dictionary.info().keys.as_deref(), Some("strongs-greek"));
    assert_eq!(commentary.info().keys, None);
    assert_eq!(dictionary.keys(), ["G25", "G26"]);
    // By its key in any form
    let entry = dictionary
        .entry("g0025".into(), options.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        (entry.key.as_str(), entry.headword.as_deref()),
        ("G25", Some("ἀγαπάω"))
    );
    assert!(entry.text.contains("to love"), "{}", entry.text);
    assert!(entry.html.contains("to love"), "{}", entry.html);
    assert_eq!(
        dictionary.entry("G9999".into(), options.clone()).unwrap(),
        None
    );
    assert!(
        dictionary
            .notes(passage("John 3:16"), options, false)
            .unwrap()
            .is_empty()
    );

    // Cross-references: lines covering any verse of the one asked for, OSIS or not
    let xref = logoxide
        .open_package(
            vec![
                file("logoxide.toml", XREF),
                file(
                    "xref/gen.tsv",
                    "# Genesis\nGen.1.1\tJohn.1.1-John.1.3\t1\nGen.1.1\tHeb.11.3\t0.8\nGen.1.1-Gen.1.5\tPs.33.6\nGen.2.1\tExod.20.11\n",
                ),
            ],
            defaults(),
            vec![],
        )
        .unwrap();
    let found = xref.cross_references("Gen.1.1".into()).unwrap();
    let lines: Vec<(&str, &str, Option<f64>)> = (found.iter())
        .map(|x| (x.from.as_str(), x.to.as_str(), x.weight))
        .collect();
    assert_eq!(
        lines,
        [
            ("Gen.1.1", "John.1.1-John.1.3", Some(1.0)),
            ("Gen.1.1", "Heb.11.3", Some(0.8)),
            ("Gen.1.1-Gen.1.5", "Ps.33.6", None),
        ]
    );
    assert_eq!(
        xref.cross_references("Genesis 1:3".into()).unwrap().len(),
        1
    );
    assert!(
        xref.cross_references("Exodus 1:1".into())
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        xref.cross_references("nothing at all".into()),
        Err(LogoxideError::Invalid { .. })
    ));
    assert_eq!(xref.keys(), ["Gen.1.1", "Gen.1.1-Gen.1.5", "Gen.2.1"]);

    // A project's commentaries, dictionaries, and cross-references, by the manifests the host
    // read: one turned off isn't in use
    let manifest = r#"
[dependencies]
test = { path = "bibles/test" }
mhcc = { path = "mhcc" }
strongs = { path = "strongs" }
tsk = { path = "tsk", enabled = false }
"#;
    let lock = r#"
version = 1
dependencies = ["test 1.0.0", "mhcc 1.0.0", "strongs 1.0.0", "tsk 1.0.0"]

[[package]]
id = "test"
version = "1.0.0"
path = "bibles/test"

[[package]]
id = "mhcc"
version = "1.0.0"
path = "mhcc"

[[package]]
id = "strongs"
version = "1.0.0"
path = "strongs"

[[package]]
id = "tsk"
version = "1.0.0"
path = "tsk"
"#;
    let project = logoxide
        .parse_project(Some(manifest.into()), Some(lock.into()))
        .unwrap();
    let manifests = || {
        [
            ("test", MANIFEST),
            ("mhcc", COMMENTARY),
            ("strongs", DICTIONARY),
            ("tsk", XREF),
        ]
        .map(|(name, manifest)| PackageManifest {
            name: name.into(),
            manifest: manifest.into(),
        })
        .to_vec()
    };
    let names = |entries: Vec<crate::LockedEntry>| -> Vec<String> {
        entries.into_iter().map(|e| e.name).collect()
    };
    assert_eq!(names(project.commentaries(manifests()).unwrap()), ["mhcc"]);
    assert_eq!(
        names(project.dictionaries(manifests()).unwrap()),
        ["strongs"]
    );
    assert!(project.cross_references(manifests()).unwrap().is_empty());
    let on = project.set_enabled("tsk".into(), true).unwrap();
    assert_eq!(names(on.cross_references(manifests()).unwrap()), ["tsk"]);
    // Without its manifest, an entry is left out; a manifest that can't be read is an error
    assert!(project.commentaries(vec![]).unwrap().is_empty());
    let bad = vec![PackageManifest {
        name: "mhcc".into(),
        manifest: "[package\n".into(),
    }];
    assert!(matches!(
        project.commentaries(bad),
        Err(LogoxideError::Invalid { .. })
    ));
}

#[test]
fn loading_catalogs_with_the_host_downloading() {
    let logoxide = Logoxide::new();
    let manifest = format!(
        "[settings]\ncatalogs = [\"{REGISTRY}\", \"https://example.org/down.toml\", \"mine.toml\"]\ndefault-catalogs = false\n"
    );
    let project = logoxide.parse_project(Some(manifest), None).unwrap();
    let installer = logoxide.installer();
    assert!(installer.catalog().is_err(), "nothing read yet");
    let mut asked = vec![];
    let warnings = loop {
        match installer.load_catalogs(&project).unwrap() {
            CatalogsStep::Done { warnings } => break warnings,
            CatalogsStep::Fetch {
                request: FetchRequest::Download { url, .. },
            } => {
                assert!(!asked.contains(&url), "asked twice for {url}");
                asked.push(url.clone());
                match url.as_str() {
                    REGISTRY => installer.provide_download(url, registry().0.into_bytes()),
                    _ => installer.provide_download_failed(url, "404 Not Found".into()),
                }
            }
            CatalogsStep::Fetch { request } => panic!("asked for {request:?}"),
        }
    };
    assert_eq!(asked, [REGISTRY, "https://example.org/down.toml"]);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("down.toml") && w.contains("404")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("mine.toml")),
        "{warnings:?}"
    );
    let catalog = installer.catalog().unwrap();
    assert_eq!(catalog.search("TB".into(), 0)[0].id, "test");
    assert_eq!(catalog.warnings(), warnings);

    // None readable is an error
    let manifest =
        "[settings]\ncatalogs = [\"https://example.org/down.toml\"]\ndefault-catalogs = false\n";
    let project = logoxide.parse_project(Some(manifest.into()), None).unwrap();
    assert!(installer.load_catalogs(&project).is_err());
}

#[test]
fn reading_and_comparing_passages_natively() {
    let dir = std::env::temp_dir().join(format!("logoxide-ffi-read-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (folder, manifest) in [
        ("test", MANIFEST.to_string()),
        (
            "other",
            (MANIFEST.replace("id = \"test\"", "id = \"other\""))
                .replace("abbreviation = \"TB\"", "abbreviation = \"OB\"")
                .replace("aliases = [\"Test\"]", "aliases = []"),
        ),
    ] {
        std::fs::create_dir_all(dir.join(format!("vault/{folder}/usx"))).unwrap();
        std::fs::write(dir.join(format!("vault/{folder}/logoxide.toml")), manifest).unwrap();
        std::fs::write(dir.join(format!("vault/{folder}/usx/JHN.usx")), USX).unwrap();
    }
    std::fs::write(
        dir.join("vault/logoxide.toml"),
        "[dependencies]\ntest = { path = \"test\" }\nother = { path = \"other\" }\n",
    )
    .unwrap();
    let library = logoxide_core::Library::new(dir.join("library"), dir.join("cache"));
    let mut project = logoxide_core::Project::open(dir.join("vault")).unwrap();
    (project.install(
        &library,
        &logoxide_core::git::GitCli::default(),
        logoxide_core::resolve::Update::Nothing,
    ))
    .unwrap();

    let path = |p: &str| dir.join(p).to_string_lossy().into_owned();
    let style = BookTextStyle {
        verse_label: Some("[{verse}]".into()),
        chapter_label: None,
        notes: false,
        headings: false,
        templates: vec![],
        width: None,
        limit: None,
    };
    let logoxide = Logoxide::new();
    let read = logoxide
        .read_passage(
            path("vault"),
            path("library"),
            path("cache"),
            "John 3:16 OB".into(),
            style.clone(),
        )
        .unwrap();
    assert_eq!(
        (read.bible.as_str(), read.abbreviation.as_str()),
        ("other", "OB")
    );
    assert_eq!(read.passage.reference, "John 3:16");
    assert_eq!(read.passage.osis, "John.3.16");
    let text = read.text.unwrap();
    assert_eq!(text.text, "[16] For God so loved the world.");
    assert!(text.marks.iter().any(|m| m.kind == NodeKind::Verse));

    let cut = BookTextStyle {
        limit: Some(12),
        ..style.clone()
    };
    let short = logoxide
        .read_passage(
            path("vault"),
            path("library"),
            path("cache"),
            "John 3:16 TB".into(),
            cut,
        )
        .unwrap();
    assert_eq!(short.bible, "test");
    assert_eq!(short.text.unwrap().text, "[16] For God …");

    let compared = logoxide
        .compare_passage(
            path("vault"),
            path("library"),
            path("cache"),
            "John 3:17".into(),
            vec!["OB".into(), "TB".into()],
            style.clone(),
        )
        .unwrap();
    let bibles: Vec<_> = compared.iter().map(|c| c.abbreviation.as_str()).collect();
    assert_eq!(bibles, ["OB", "TB"]);
    assert!(compared.iter().all(|c| !c.mapped && c.missing.is_empty()));
    assert_eq!(compared[1].passages[0].reference, "John 3:17");
    assert_eq!(
        compared[1].text.as_ref().unwrap().text,
        "[17] For God did not send His Son."
    );
    assert!(
        logoxide
            .compare_passage(
                path("vault"),
                path("library"),
                path("cache"),
                "John 3:17".into(),
                vec!["XYZ".into()],
                style,
            )
            .is_err()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
