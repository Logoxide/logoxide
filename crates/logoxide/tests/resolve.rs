#![cfg(feature = "read")]

//! Resolving and installing from git repositories (made here with the `git` command)

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use logoxide::reader::DefaultReader;
use logoxide::{
    Library, LogoxideError, Project,
    git::GitCli,
    manifest::DetailedDependency,
    resolve::{Frozen, Update},
};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

const JHN: &str = r#"<usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c"/><para style="p"><verse number="16" style="v"/>WORDS</para></usx>"#;

/// Commits a package version (and tags it `v{version}`)
fn release(repo: &Path, id: &str, version: &str, words: &str, extra: &str) {
    std::fs::create_dir_all(repo.join("usx")).unwrap();
    let manifest = format!(
        "[package]\nid = \"{id}\"\nname = \"{id}\"\nabbreviation = \"{}\"\nversion = \"{version}\"\nlanguage = \"en\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n{extra}",
        id.to_uppercase()
    );
    std::fs::write(repo.join("logoxide.toml"), manifest).unwrap();
    std::fs::write(repo.join("usx/JHN.usx"), JHN.replace("WORDS", words)).unwrap();
    if !repo.join(".git").exists() {
        git(repo, &["init", "-q", "-b", "main"]);
    }
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", version]);
    git(repo, &["tag", &format!("v{version}")]);
}

struct World {
    root: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        let root =
            std::env::temp_dir().join(format!("logoxide-resolve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        World { root }
    }
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }
    fn library(&self, name: &str) -> Library {
        Library::new(self.path(name), self.path(&format!("{name}-cache")))
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git_dep(url: &Path, version: Option<&str>) -> DetailedDependency {
    DetailedDependency {
        git: Some(url.display().to_string()),
        version: version.map(|v| v.parse().unwrap()),
        default_features: true,
        ..DetailedDependency::default()
    }
}

fn locked(project: &Project) -> Vec<String> {
    project
        .lock()
        .unwrap()
        .packages
        .iter()
        .map(|p| p.label())
        .collect()
}

/// Lock names (`id version+gcommit`) without their commits
fn versions(names: &[String]) -> Vec<String> {
    (names.iter())
        .map(|n| n.split('+').next().unwrap_or(n).to_string())
        .collect()
}

#[test]
fn versions_locks_and_updates() {
    let world = World::new("versions");
    let repo = world.path("repos/tb");
    std::fs::create_dir_all(&repo).unwrap();
    release(&repo, "tb", "1.0.0", "one", "");
    release(&repo, "tb", "1.1.0", "one point one", "");
    release(&repo, "tb", "2.0.0", "two", "");

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    // `1` allows 1.x: the newest is 1.1.0
    project.add("tb", &git_dep(&repo, Some("1"))).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(locked(&project), ["tb 1.1.0"]);
    let installed = library
        .open(project.lock().unwrap().get("tb").unwrap(), project.dir())
        .unwrap();
    assert!(
        installed
            .storage()
            .read_string("usx/JHN.usx")
            .unwrap()
            .contains("one point one")
    );

    // A new release within the requirement: kept until updated
    release(&repo, "tb", "1.2.0", "one point two", "");
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(locked(&project), ["tb 1.1.0"]);
    project.install(&library, &fetcher, Update::All).unwrap();
    assert_eq!(locked(&project), ["tb 1.2.0"]);
    // Both versions stay in the library, side by side
    let installed: Vec<(String, String)> = (library.installed().into_iter())
        .map(|(id, key)| (id, key.split('+').next().unwrap_or(&key).to_string()))
        .collect();
    assert_eq!(
        installed,
        [("tb".into(), "1.1.0".into()), ("tb".into(), "1.2.0".into())]
    );

    // Another device: a new library, the same lock file, the same commit and checksum
    project.save_manifest().unwrap();
    let other_library = world.library("other");
    let mut other = Project::open(world.path("vault")).unwrap();
    other
        .install(&other_library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(other.lock(), project.lock());

    // A changed installed file is caught
    let dir = library.package_dir(project.lock().unwrap().get("tb").unwrap(), project.dir());
    std::fs::write(dir.join("usx/JHN.usx"), "tampered").unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(matches!(error, LogoxideError::Changed { .. }), "{error}");

    // A requirement no tag matches
    project.add("tb", &git_dep(&repo, Some("3"))).unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(error.to_string().contains("no tag"), "{error}");
}

#[test]
fn dependencies_features_and_paths() {
    let world = World::new("deps");
    let lexicon = world.path("repos/lexicon");
    let bible = world.path("repos/bible");
    std::fs::create_dir_all(&lexicon).unwrap();
    std::fs::create_dir_all(&bible).unwrap();
    release(&lexicon, "lexicon", "2.0.0", "lexicon", "");
    // The lexicon only comes with the `strongs` feature
    let extra = format!(
        "\n[features]\nstrongs = [\"dep:lexicon\"]\n\n[dependencies]\nlexicon = {{ git = \"{}\", version = \"2\", optional = true }}\n",
        lexicon.display()
    );
    release(&bible, "bible", "1.0.0", "bible", &extra);

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    project.add("bible", &git_dep(&bible, None)).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(locked(&project), ["bible 1.0.0"]);

    let mut with_strongs = git_dep(&bible, None);
    with_strongs.features = vec!["strongs".into()];
    project.add("bible", &with_strongs).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(locked(&project), ["bible 1.0.0", "lexicon 2.0.0"]);
    let lock = project.lock().unwrap();
    assert_eq!(versions(&lock.dependencies), ["bible 1.0.0"]);
    assert_eq!(
        versions(&lock.get("bible").unwrap().dependencies),
        ["lexicon 2.0.0"]
    );

    // A path package, used in place (not copied into the library)
    let mine = world.path("vault/Bibles/Mine");
    std::fs::create_dir_all(&mine).unwrap();
    release(&mine, "mine", "0.1.0", "mine", "");
    std::fs::remove_dir_all(mine.join(".git")).unwrap();
    project
        .add(
            "mine",
            &DetailedDependency {
                path: Some("Bibles/Mine".into()),
                default_features: true,
                ..DetailedDependency::default()
            },
        )
        .unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let mine_locked = project.lock().unwrap().get("mine").unwrap().clone();
    assert_eq!(mine_locked.path.as_deref(), Some("Bibles/Mine"));
    assert!(library.is_installed(&mine_locked, project.dir()));
    assert!(!library.installed().iter().any(|(id, _)| id == "mine"));

    // The vault's Bibles become topos translations (the lexicon, a dependency, doesn't)
    let translations = project.translations(&library, &DefaultReader).unwrap();
    let names: Vec<String> = translations.names().map(String::from).collect();
    assert_eq!(names, ["BIBLE", "MINE"]);

    // A dependency named differently from its package
    project.add("other", &git_dep(&lexicon, None)).unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(error.to_string().contains("name it `lexicon`"), "{error}");
}

#[test]
fn forks_side_by_side() {
    let world = World::new("forks");
    let alice = world.path("repos/alice/kjv");
    let bob = world.path("repos/bob/kjv");
    std::fs::create_dir_all(&alice).unwrap();
    std::fs::create_dir_all(&bob).unwrap();
    // The same package and version, with different text
    release(&alice, "kjv", "1.0.0", "alice's words", "");
    release(&bob, "kjv", "1.0.0", "bob's words", "");

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    project.add("kjv", &git_dep(&alice, None)).unwrap();
    // Bob's under another name, with another abbreviation and no aliases
    let mut fork = git_dep(&bob, None);
    fork.package = Some("kjv".into());
    fork.abbreviation = Some("KJVB".into());
    fork.aliases = Some(vec![]);
    project.add("kjv-bob", &fork).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let mut labels = locked(&project);
    labels.sort();
    assert_eq!(labels, ["kjv 1.0.0", "kjv-bob (kjv) 1.0.0"]);

    // Each in its own folder, with its own text
    let lock = project.lock().unwrap();
    let text = |name: &str| {
        let package = library
            .open(lock.get(name).unwrap(), project.dir())
            .unwrap();
        package.storage().read_string("usx/JHN.usx").unwrap()
    };
    assert!(text("kjv").contains("alice's words"));
    assert!(text("kjv-bob").contains("bob's words"));
    assert_eq!(library.installed().len(), 2);

    // References name them apart
    let translations = project.translations(&library, &DefaultReader).unwrap();
    assert_eq!(translations.get("KJV"), Some("KJV"));
    assert_eq!(translations.get("KJVB"), Some("KJVB"));

    // Naming a fork as the package it isn't is caught
    let mut wrong = git_dep(&bob, None);
    wrong.package = Some("bsb".into());
    project.add("bsb", &wrong).unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(
        error.to_string().contains("asks for the package `bsb`"),
        "{error}"
    );
}

#[test]
fn two_names_for_one_commit_stay_two() {
    let world = World::new("two-names");
    let repo = world.path("repos/kjv");
    std::fs::create_dir_all(&repo).unwrap();
    release(&repo, "kjv", "1.0.0", "words", "");

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    project.add("kjv", &git_dep(&repo, None)).unwrap();
    let mut two = git_dep(&repo, None);
    two.package = Some("kjv".into());
    two.abbreviation = Some("KJV2".into());
    two.aliases = Some(vec![]);
    project.add("kjv-two", &two).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let lock = project.lock().unwrap().clone();
    assert_eq!(lock.packages.len(), 2);
    assert_eq!(lock.dependencies.len(), 2);
    let (kjv, kjv_two) = (lock.get("kjv").unwrap(), lock.get("kjv-two").unwrap());
    assert_eq!(kjv.commit, kjv_two.commit);
    assert!(kjv_two.name().starts_with("kjv-two (kjv) 1.0.0+g"));
    // One folder for both
    assert_eq!(library.installed().len(), 1);
    assert_eq!(
        library.package_dir(kjv, project.dir()),
        library.package_dir(kjv_two, project.dir())
    );

    let translations = project.translations(&library, &DefaultReader).unwrap();
    assert_eq!(translations.get("KJV"), Some("KJV"));
    assert_eq!(translations.get("KJV2"), Some("KJV2"));

    // Turning one off leaves the other on
    project.set_enabled("kjv", false).unwrap();
    let in_use: Vec<&str> = project
        .in_use()
        .iter()
        .map(|p| p.dependency_name())
        .collect();
    assert_eq!(in_use, ["kjv-two"]);
    let translations = project.translations(&library, &DefaultReader).unwrap();
    assert_eq!(translations.get("KJV"), None);
    assert_eq!(translations.get("KJV2"), Some("KJV2"));

    // Installing again keeps both, as they were
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(project.lock().unwrap(), &lock);
}

#[test]
fn collections_bring_in_their_packages() {
    let world = World::new("collections");
    let bible = world.path("repos/tb");
    std::fs::create_dir_all(&bible).unwrap();
    release(&bible, "tb", "1.0.0", "words", "");
    // A collection: no text, only its packages
    let collection = world.path("repos/library");
    std::fs::create_dir_all(&collection).unwrap();
    std::fs::write(
        collection.join("logoxide.toml"),
        format!(
            "[package]\nid = \"library\"\nname = \"A Library\"\nabbreviation = \"LIB\"\nversion = \"1.0.0\"\nkind = \"collection\"\nlanguage = \"en\"\n\n[dependencies]\ntb = {{ git = \"{}\", version = \"1\" }}\n",
            bible.display()
        ),
    )
    .unwrap();
    git(&collection, &["init", "-q", "-b", "main"]);
    git(&collection, &["add", "-A"]);
    git(&collection, &["commit", "-q", "-m", "1.0.0"]);
    git(&collection, &["tag", "v1.0.0"]);

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    project.add("library", &git_dep(&collection, None)).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(locked(&project), ["library 1.0.0", "tb 1.0.0"]);
    // Its Bible is the project's, though only the collection is named
    let translations = project.translations(&library, &DefaultReader).unwrap();
    assert_eq!(translations.get("TB"), Some("TB"));
    assert_eq!(translations.get("LIB"), None);
}

/// A collection (released `v1.0.0`) of one package, with its requirement
fn collection(dir: &Path, package: &str, url: &Path, version: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("logoxide.toml"),
        format!(
            "[package]\nid = \"library\"\nname = \"A Library\"\nabbreviation = \"LIB\"\nversion = \"1.0.0\"\nkind = \"collection\"\nlanguage = \"en\"\n\n[dependencies]\n{package} = {{ git = \"{}\", version = \"{version}\" }}\n",
            url.display()
        ),
    )
    .unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "1.0.0"]);
    git(dir, &["tag", "v1.0.0"]);
}

#[test]
fn a_locked_install_keeps_the_lock_as_it_is() {
    let world = World::new("locked");
    let mine = world.path("vault/mine");
    std::fs::create_dir_all(&mine).unwrap();
    release(&mine, "mine", "0.1.0", "mine", "");
    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    let locked = Frozen {
        locked: true,
        offline: false,
    };
    // No lock file to keep
    project
        .add(
            "mine",
            &DetailedDependency {
                path: Some("mine".into()),
                default_features: true,
                ..DetailedDependency::default()
            },
        )
        .unwrap();
    assert!(project.install_frozen(&library, &fetcher, locked).is_err());
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let lock = project.lock().unwrap().clone();
    project.install_frozen(&library, &fetcher, locked).unwrap();
    // A path package's new version would change it
    release(&mine, "mine", "0.2.0", "mine", "");
    let error = project
        .install_frozen(&library, &fetcher, locked)
        .unwrap_err();
    assert!(error.to_string().contains("would change"), "{error}");
    assert_eq!(project.lock().unwrap(), &lock);
}

#[test]
fn each_asker_keeps_its_own_version() {
    let world = World::new("askers");
    let bible = world.path("repos/tb");
    std::fs::create_dir_all(&bible).unwrap();
    release(&bible, "tb", "1.0.0", "one", "");
    release(&bible, "tb", "2.0.0", "two", "");
    let shelf = world.path("repos/library");
    collection(&shelf, "tb", &bible, "1");

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    project.add("library", &git_dep(&shelf, None)).unwrap();
    project.add("tb", &git_dep(&bible, Some(">=1"))).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let lock = project.lock().unwrap().clone();
    assert_eq!(locked(&project), ["library 1.0.0", "tb 1.0.0", "tb 2.0.0"]);
    assert_eq!(lock.get("tb").unwrap().version.to_string(), "2.0.0");

    // Installing again: the manifest's tb stays 2.0.0, though 1.0.0 (the collection's) also
    // matches `>=1` and comes first in the lock
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(project.lock().unwrap(), &lock);
    // And on another device, from the same lock file
    project.save_manifest().unwrap();
    let mut other = Project::open(world.path("vault")).unwrap();
    other
        .install(&world.library("other"), &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(other.lock().unwrap(), &lock);

    // Both are Bibles answering to TB: said, not hidden
    let clashes = project.name_clashes(&library).unwrap();
    assert_eq!(clashes.len(), 1);
    assert_eq!(clashes[0].name, "TB");
    let mut packages = clashes[0].packages.clone();
    packages.sort();
    assert_eq!(packages, ["tb 1.0.0", "tb 2.0.0"]);

    // The manifest's abbreviation is the manifest's copy's alone: the collection's keeps TB
    let mut renamed = git_dep(&bible, Some(">=1"));
    renamed.abbreviation = Some("TB2".into());
    project.add("tb", &renamed).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert!(project.name_clashes(&library).unwrap().is_empty());
    let translations = project.translations(&library, &DefaultReader).unwrap();
    assert_eq!(translations.get("TB2"), Some("TB2"));
    assert_eq!(translations.get("TB"), Some("TB"));
    let lock = project.lock().unwrap();
    let collections = lock
        .packages
        .iter()
        .find(|p| p.version.major == 1 && p.id == "tb");
    let package = library.open(collections.unwrap(), project.dir()).unwrap();
    assert_eq!(project.names(collections.unwrap(), package.info()).0, "TB");
}

#[test]
fn annotations_stay_and_edits_are_repaired() {
    let world = World::new("annotations");
    let repo = world.path("repos/tb");
    std::fs::create_dir_all(&repo).unwrap();
    // A file in the repository that the package doesn't declare isn't installed
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("README.md"), "about the repository").unwrap();
    release(&repo, "tb", "1.0.0", "words", "");

    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    project.add("tb", &git_dep(&repo, None)).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let locked = project.lock().unwrap().get("tb").unwrap().clone();
    let dir = library.package_dir(&locked, project.dir());
    assert!(!dir.join("README.md").exists());

    // A reader's annotation beside a book: still the same package
    let note = dir.join("usx/JHN - Annotations.md");
    std::fs::write(&note, "my note").unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();

    // An edited book is caught, and repairing puts it back, keeping the annotation
    std::fs::write(dir.join("usx/JHN.usx"), "edited").unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(error.to_string().contains("--repair"), "{error}");
    project.repair(&library, &fetcher).unwrap();
    let book = std::fs::read_to_string(dir.join("usx/JHN.usx")).unwrap();
    assert!(book.contains("words"), "{book}");
    assert_eq!(std::fs::read_to_string(&note).unwrap(), "my note");
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
}

/// Publishes a package's folder into a registry folder: its archive, and its release in the
/// registry's catalog.toml (as `lx publish` does)
fn publish(package: &Path, registry: &Path) {
    publish_mirrored(package, registry, vec![]);
}

/// The same, with mirrors listed for the archive (relative to the catalog, like its URL)
fn publish_mirrored(package: &Path, registry: &Path, mirrors: Vec<String>) {
    use logoxide::{FsStorage, Storage, catalog::CatalogRelease, publish};
    let storage = FsStorage::new(package);
    let manifest =
        logoxide::Manifest::parse(&storage.read_string("logoxide.toml").unwrap()).unwrap();
    let info = manifest.package.unwrap();
    let bytes = publish::pack(&storage, &info).unwrap();
    let file = format!("{}/{}", info.id, publish::archive_name(&info));
    std::fs::create_dir_all(registry.join(&info.id)).unwrap();
    std::fs::write(registry.join(&file), &bytes).unwrap();
    let catalog = registry.join("catalog.toml");
    let text = std::fs::read_to_string(&catalog).ok();
    let release = CatalogRelease {
        version: info.version.to_string(),
        url: file,
        mirrors,
        sha256: publish::sha256(&bytes),
        checksum: Some(logoxide::package::checksum(&storage).unwrap()),
        size: Some(bytes.len() as u64),
        yanked: false,
    };
    let text = publish::add_release(text.as_deref(), "test", &info, None, &release)
        .unwrap()
        .unwrap();
    std::fs::write(catalog, text).unwrap();
}

#[test]
fn registry_releases_must_give_their_checksum() {
    let world = World::new("no-checksum");
    let repo = world.path("authoring/tb");
    let registry = world.path("registry");
    std::fs::create_dir_all(&repo).unwrap();
    release(&repo, "tb", "1.0.0", "one", "");
    publish(&repo, &registry);
    let catalog = registry.join("catalog.toml");
    let text = std::fs::read_to_string(&catalog).unwrap();
    let without: String = (text.lines())
        .filter(|l| !l.starts_with("checksum"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_ne!(without, text);
    std::fs::write(&catalog, without).unwrap();

    let library = world.library("lib");
    let mut project = Project::init(world.path("vault")).unwrap();
    project
        .add(
            "tb",
            &DetailedDependency {
                registry: Some(catalog.display().to_string()),
                default_features: true,
                ..DetailedDependency::default()
            },
        )
        .unwrap();
    let error = project
        .install(&library, &GitCli::default(), Update::Nothing)
        .unwrap_err();
    assert!(
        error.to_string().contains("without a `checksum`"),
        "{error}"
    );
    assert!(library.installed().is_empty());
}

#[test]
fn registry_releases_are_archives() {
    let world = World::new("registry");
    let repo = world.path("authoring/tb");
    let registry = world.path("registry");
    std::fs::create_dir_all(&repo).unwrap();
    release(&repo, "tb", "1.0.0", "one", "");
    publish(&repo, &registry);
    release(&repo, "tb", "1.1.0", "one point one", "");
    publish(&repo, &registry);
    // The authoring repository is gone: only the registry's files are used
    std::fs::remove_dir_all(world.path("authoring")).unwrap();

    let catalog = registry.join("catalog.toml").display().to_string();
    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    let dependency = |version: &str| DetailedDependency {
        registry: Some(catalog.clone()),
        version: Some(version.parse().unwrap()),
        default_features: true,
        ..DetailedDependency::default()
    };
    project.add("tb", &dependency("~1.0")).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    let lock = project.lock().unwrap();
    let tb = lock.get("tb").unwrap();
    assert_eq!(tb.version.to_string(), "1.0.0");
    assert!(tb.key().starts_with("1.0.0+a"));
    assert!(
        tb.archive
            .as_deref()
            .unwrap()
            .ends_with("tb/tb-1.0.0.tar.gz")
    );
    assert_eq!(tb.registry.as_deref(), Some(catalog.as_str()));
    let installed = library.open(tb, project.dir()).unwrap();
    assert!(
        installed
            .storage()
            .read_string("usx/JHN.usx")
            .unwrap()
            .contains("one")
    );

    // A newer release the requirement allows only comes with an update
    project.add("tb", &dependency("1")).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(
        project
            .lock()
            .unwrap()
            .get("tb")
            .unwrap()
            .version
            .to_string(),
        "1.0.0"
    );
    project.install(&library, &fetcher, Update::All).unwrap();
    assert_eq!(
        project
            .lock()
            .unwrap()
            .get("tb")
            .unwrap()
            .version
            .to_string(),
        "1.1.0"
    );

    // Another device installs from the lock alone; an archive that changed is refused
    project.save_manifest().unwrap();
    std::fs::remove_file(registry.join("catalog.toml")).unwrap();
    let third = world.library("third");
    let mut again = Project::open(world.path("vault")).unwrap();
    again.install(&third, &fetcher, Update::Nothing).unwrap();
    assert_eq!(third.installed().len(), 1);
    let other = world.library("other");
    std::fs::write(registry.join("tb/tb-1.1.0.tar.gz"), b"something else").unwrap();
    let result = again.install(&other, &fetcher, Update::Nothing).map(|_| ());

    assert!(
        matches!(result, Err(LogoxideError::Checksum { .. })),
        "{result:?}"
    );
}

#[test]
fn archives_come_from_mirrors_when_their_url_fails() {
    let world = World::new("mirrors");
    let repo = world.path("authoring/tb");
    let registry = world.path("registry");
    std::fs::create_dir_all(&repo).unwrap();
    release(&repo, "tb", "1.0.0", "one", "");
    let mirrors = vec!["gone/tb-1.0.0.tar.gz".into(), "copy/tb-1.0.0.tar.gz".into()];
    publish_mirrored(&repo, &registry, mirrors);
    // The archive only at the second mirror
    std::fs::create_dir_all(registry.join("copy")).unwrap();
    std::fs::rename(
        registry.join("tb/tb-1.0.0.tar.gz"),
        registry.join("copy/tb-1.0.0.tar.gz"),
    )
    .unwrap();

    let catalog = registry.join("catalog.toml").display().to_string();
    let fetcher = GitCli::default();
    let mut project = Project::init(world.path("vault")).unwrap();
    let dependency = DetailedDependency {
        registry: Some(catalog),
        default_features: true,
        ..DetailedDependency::default()
    };
    project.add("tb", &dependency).unwrap();
    project
        .install(&world.library("lib"), &fetcher, Update::Nothing)
        .unwrap();
    let lock = project.lock().unwrap();
    let tb = lock.get("tb").unwrap();
    assert_eq!(tb.mirrors.len(), 2);
    assert!(tb.mirrors[1].ends_with("registry/copy/tb-1.0.0.tar.gz"));

    // A mirror whose bytes don't hash as listed is passed over too, and when none serves it,
    // the error names each
    std::fs::write(registry.join("tb/tb-1.0.0.tar.gz"), b"something else").unwrap();
    project
        .install(&world.library("second"), &fetcher, Update::Nothing)
        .unwrap();
    std::fs::remove_file(registry.join("copy/tb-1.0.0.tar.gz")).unwrap();
    let error = project
        .install(&world.library("third"), &fetcher, Update::Nothing)
        .unwrap_err()
        .to_string();
    for place in ["tb/tb-1.0.0.tar.gz", "gone/", "copy/"] {
        assert!(error.contains(place), "{error}");
    }
}

/// A package's files in a folder of a repository, without committing
fn package_in(dir: &Path, id: &str, version: &str, words: &str) {
    std::fs::create_dir_all(dir.join("usx")).unwrap();
    let manifest = format!(
        "[package]\nid = \"{id}\"\nname = \"{id}\"\nabbreviation = \"{}\"\nversion = \"{version}\"\nlanguage = \"en\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n",
        id.to_uppercase()
    );
    std::fs::write(dir.join("logoxide.toml"), manifest).unwrap();
    std::fs::write(dir.join("usx/JHN.usx"), JHN.replace("WORDS", words)).unwrap();
}

#[test]
fn packages_in_folders_of_a_repository() {
    let world = World::new("folders");
    let repo = world.path("repos/books");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    // Two packages, each released on its own (`lx release` tags `<folder>/v<version>`)
    let commit = |folder: &str, id: &str, version: &str, words: &str| {
        package_in(&repo.join(folder), id, version, words);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", &format!("{id} {version}")]);
        git(&repo, &["tag", &format!("{folder}/v{version}")]);
    };
    commit("romans", "romans", "1.0.0", "romans one");
    commit("galatians", "galatians", "3.0.0", "galatians three");
    commit("romans", "romans", "1.1.0", "romans one point one");
    commit("romans", "romans", "2.0.0", "romans two");

    let vault = world.path("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(
        vault.join("logoxide.toml"),
        format!(
            "[dependencies]\nromans = {{ git = \"{}\", folder = \"romans\", version = \"1\" }}\n",
            repo.display()
        ),
    )
    .unwrap();
    let library = world.library("lib");
    let fetcher = GitCli::default();
    let mut project = Project::open(&vault).unwrap();
    project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap();
    // The newest of its own tags the requirement allows; only its folder's files
    assert_eq!(locked(&project), ["romans 1.1.0"]);
    let lock = project.lock().unwrap();
    let romans = lock.get("romans").unwrap();
    assert_eq!(romans.folder.as_deref(), Some("romans"));
    assert_eq!(romans.tag.as_deref(), Some("romans/v1.1.0"));
    let installed = library.open(romans, project.dir()).unwrap();
    let files = installed.storage().walk("").unwrap();
    assert_eq!(files, ["logoxide.toml", "usx/JHN.usx"]);
    assert!(
        (installed.storage().read_string("usx/JHN.usx").unwrap()).contains("romans one point one")
    );

    // Locked: the same again, here and on another device
    project.save_manifest().unwrap();
    let mut other = Project::open(&vault).unwrap();
    other
        .install(&world.library("other"), &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(other.lock(), project.lock());

    // Another folder's tags are none of its versions
    std::fs::write(
        vault.join("logoxide.toml"),
        format!(
            "[dependencies]\nromans = {{ git = \"{}\", folder = \"romans\", version = \"3\" }}\n",
            repo.display()
        ),
    )
    .unwrap();
    let mut project = Project::open(&vault).unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(error.to_string().contains("for romans/"), "{error}");
}

/// Fetches `https://example.org/<name>` from the folder `root/<name>`: a stand-in for a remote
/// host
struct Remote {
    root: PathBuf,
    git: GitCli,
}

impl Remote {
    fn local(&self, url: &str) -> String {
        match url.strip_prefix("https://example.org/") {
            Some(name) => self.root.join(name).display().to_string(),
            None => url.to_string(),
        }
    }
}

impl logoxide::git::Fetcher for Remote {
    fn tags(&self, url: &str) -> logoxide::Result<Vec<(String, String)>> {
        self.git.tags(&self.local(url))
    }

    fn fetch(
        &self,
        url: &str,
        reference: &logoxide::git::GitRef,
        dest: &dyn logoxide::Storage,
    ) -> logoxide::Result<String> {
        self.git.fetch(&self.local(url), reference, dest)
    }
}

#[test]
fn packages_from_elsewhere_name_nothing_here() {
    let world = World::new("remote");
    let lexicon = world.path("repos/lexicon");
    std::fs::create_dir_all(&lexicon).unwrap();
    release(&lexicon, "lexicon", "1.0.0", "lexicon", "");
    let fetcher = Remote {
        root: world.path("repos"),
        git: GitCli::default(),
    };
    // A package fetched from elsewhere may depend on another from elsewhere
    let fine = world.path("repos/fine");
    std::fs::create_dir_all(&fine).unwrap();
    let extra = "\n[dependencies]\nlexicon = { git = \"https://example.org/lexicon\" }\n";
    release(&fine, "fine", "1.0.0", "fine", extra);
    let mut project = Project::init(world.path("vault")).unwrap();
    let remote = |name: &str| DetailedDependency {
        git: Some(format!("https://example.org/{name}")),
        default_features: true,
        ..DetailedDependency::default()
    };
    project.add("fine", &remote("fine")).unwrap();
    project
        .install(&world.library("lib"), &fetcher, Update::Nothing)
        .unwrap();
    assert_eq!(locked(&project), ["fine 1.0.0", "lexicon 1.0.0"]);

    // But not on this machine: a local repository, a file:// one, or a local registry; nor over
    // git's unencrypted protocol
    for (i, dependency) in [
        format!("lexicon = {{ git = \"{}\" }}", lexicon.display()),
        format!("lexicon = {{ git = \"file://{}\" }}", lexicon.display()),
        String::from("lexicon = { registry = \"/srv/registry/catalog.toml\" }"),
        String::from("lexicon = { git = \"git://example.org/lexicon\" }"),
    ]
    .iter()
    .enumerate()
    {
        let name = format!("sneaky{i}");
        let repo = world.path(&format!("repos/{name}"));
        std::fs::create_dir_all(&repo).unwrap();
        release(
            &repo,
            &name,
            "1.0.0",
            "sneaky",
            &format!("\n[dependencies]\n{dependency}\n"),
        );
        let mut project = Project::init(world.path(&format!("vault-{name}"))).unwrap();
        project.add(&name, &remote(&name)).unwrap();
        let error = project
            .install(&world.library("lib"), &fetcher, Update::Nothing)
            .unwrap_err();
        let error = error.to_string();
        assert!(
            error.contains("on this one") || error.contains("git's unencrypted protocol"),
            "{error}"
        );
    }
}
