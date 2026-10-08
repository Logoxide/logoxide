//! The API as the bindings see it: packages from their files' bytes, the cache handed back to the
//! host, topos's translations and books, and a vault's manifest and lock file

use std::collections::HashMap;

use crate::{
    Biblix, BiblixError, BiblixInstaller, BiblixProject, BookStyle, BookTextOptions, Dependency,
    FetchRequest, InspectStep, InstallStep, OffsetUnit, PackageFeatures, PackageFile, PackageKind,
    PackageLocation, Topos, ToposTranslations, UpdateMode,
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
                dir: "packages/bsb/1.2.0".into()
            },
            &PackageLocation::Path {
                dir: "bibles/mine".into()
            },
        ]
    );
    assert_eq!(packages[0].features.features, ["sword"]);
    assert_eq!(packages[0].checksum.as_deref(), Some("sha256:00"));

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
    let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "packages/test/1.0.0/biblix.toml",
            "packages/test/1.0.0/usx/JHN.usx"
        ]
    );
    let project = biblix
        .parse_project(Some(project.manifest_text()), Some(lock.clone()))
        .unwrap();
    let entry = &project.packages()[0];
    assert_eq!(entry.commit.as_deref(), Some(COMMIT));
    let package = biblix
        .open_package(
            files_under(&files, "packages/test/1.0.0/"),
            entry.features.clone(),
            vec![],
        )
        .unwrap();
    assert_eq!(entry.checksum, Some(package.checksum().unwrap()));

    // Another device with the lock file: installed already, it's only checked
    host.library.insert(
        "packages/test/1.0.0".into(),
        files_under(&files, "packages/test/1.0.0/"),
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
    host.library.insert("packages/test/1.0.0".into(), changed);
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
