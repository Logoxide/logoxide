//! Versification maps: the sample map package (English to Original, from Paratext's eng.vrs),
//! checked, summarized, released, and used by a project to map between its Bibles' numberings

use std::{path::PathBuf, sync::Arc};

use biblix::{
    Features, FsStorage, Library, MemoryStorage, Package, Project, Storage,
    git::GitCli,
    manifest::{DetailedDependency, PackageKind},
    package::declared_files,
    resolve::Update,
    summary::{Change, diff_summaries, summarize},
    versification::VersificationMap,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/english-original")
}

fn open(storage: Arc<dyn Storage>) -> Package {
    Package::open(storage, Features::default()).unwrap()
}

#[test]
fn a_map_package_is_checked_and_summarized() {
    let package = open(Arc::new(FsStorage::new(fixture())));
    assert_eq!(package.info().kind, PackageKind::VersificationMap);
    assert!(package.books().is_empty());
    let map = VersificationMap::from_package(&package).unwrap();
    assert_eq!(
        (map.from.as_str(), map.to.as_str()),
        ("English", "Original")
    );
    assert!(map.problems().is_empty());
    assert_eq!(map.mappings().len(), 19);

    // Its summary counts what it maps, and has no verses
    let (summary, verses) = summarize(&package).unwrap();
    assert!(verses.is_empty());
    assert_eq!(summary.mappings, map.len() as u64);
    assert!(summary.to_toml().unwrap().contains("mappings = 179"));

    // A changed mapping is a minor change at least
    let copy = Arc::new(MemoryStorage::new());
    for path in declared_files(package.storage().as_ref()).unwrap() {
        copy.write(&path, &package.storage().read(&path).unwrap())
            .unwrap();
    }
    let text = copy.read_string("eng-org.vrs").unwrap();
    let changed = text.replace("MAL 4:1-6 = MAL 3:19-24", "MAL 4:1-5 = MAL 3:19-23");
    copy.write("eng-org.vrs", changed.as_bytes()).unwrap();
    let (new_summary, new_verses) = summarize(&open(copy.clone())).unwrap();
    let diff = diff_summaries((&summary, &verses), (&new_summary, &new_verses));
    assert!(diff.change() >= Change::Minor);

    // Overlapping lines are refused, by `bx check` and by releases alike
    let overlapping = format!("{changed}\nMAL 4:2 = MAL 3:22\n");
    copy.write("eng-org.vrs", overlapping.as_bytes()).unwrap();
    let broken = open(copy);
    let error = VersificationMap::from_package(&broken)
        .unwrap_err()
        .to_string();
    assert!(error.contains("MAL 4:2 is on the left"), "{error}");
    assert!(summarize(&broken).is_err());
}

#[test]
fn a_map_names_both_versifications() {
    let manifest = std::fs::read_to_string(fixture().join("biblix.toml")).unwrap();
    let without = manifest.replace("maps-to = \"Original\"\n", "");
    assert!(biblix::Manifest::parse(&without).is_err());
    let bible = manifest.replace("kind = \"versification-map\"\n", "");
    assert!(biblix::Manifest::parse(&bible).is_err());
}

const PSALMS: &str = r#"<usx version="3.0"><book code="PSA" style="id"/><chapter number="51" style="c"/><para style="p"><verse number="1" style="v"/>Have mercy upon me, O God</para></usx>"#;

#[test]
fn a_project_maps_between_its_bibles() {
    let root = std::env::temp_dir().join(format!("biblix-versification-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let vault = root.join("vault");
    // A Bible numbered the English way, and one numbered its own way
    for (id, versification) in [("eb", "versification = \"English\"\n"), ("ob", "")] {
        let dir = vault.join(id);
        std::fs::create_dir_all(dir.join("usx")).unwrap();
        let manifest = format!(
            "[package]\nid = \"{id}\"\nname = \"{id}\"\nabbreviation = \"{}\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n{versification}\n[formats]\nusx = {{ path = \"usx/\" }}\n",
            id.to_uppercase()
        );
        std::fs::write(dir.join("biblix.toml"), manifest).unwrap();
        std::fs::write(dir.join("usx/PSA.usx"), PSALMS).unwrap();
    }
    let map = vault.join("english-original");
    std::fs::create_dir_all(&map).unwrap();
    for file in ["biblix.toml", "eng-org.vrs"] {
        std::fs::copy(fixture().join(file), map.join(file)).unwrap();
    }

    let library = Library::new(root.join("lib"), root.join("cache"));
    let mut project = Project::init(&vault).unwrap();
    for name in ["eb", "ob", "english-original"] {
        let dependency = DetailedDependency {
            path: Some(name.to_string()),
            default_features: true,
            ..DetailedDependency::default()
        };
        project.add(name, &dependency).unwrap();
    }
    project
        .install(&library, &GitCli::default(), Update::Nothing)
        .unwrap();

    // A Bible (by name or abbreviation) is its versification; a name alone is a versification
    let to_original = project
        .map_reference(&library, "Psalm 51:1", "EB", "Original")
        .unwrap();
    assert_eq!(to_original.text(), "Psalm 51:3");
    let back = project
        .map_reference(&library, "Psalm 51:3-4", "Original", "eb")
        .unwrap();
    assert_eq!(back.text(), "Psalm 51:1-2");
    let malachi = project
        .map_reference(&library, "Malachi 4:1-6", "English", "Original")
        .unwrap();
    assert_eq!(malachi.text(), "Malachi 3:19-24");
    // A Bible numbered its own way can't be mapped
    let own = project.map_reference(&library, "Psalm 51:1", "OB", "Original");
    assert!(
        own.unwrap_err()
            .to_string()
            .contains("names no versification")
    );
    let _ = std::fs::remove_dir_all(&root);
}
