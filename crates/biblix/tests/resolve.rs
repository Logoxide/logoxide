//! Resolving and installing from git repositories (made here with the `git` command)

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use biblix::{
    BiblixError, Library, Project, git::GitCli, manifest::DetailedDependency, resolve::Update,
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
    std::fs::write(repo.join("biblix.toml"), manifest).unwrap();
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
            std::env::temp_dir().join(format!("biblix-resolve-{name}-{}", std::process::id()));
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
        .map(|p| p.name())
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
    assert_eq!(
        library.installed(),
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
    assert!(matches!(error, BiblixError::Checksum { .. }), "{error}");

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
    assert_eq!(lock.dependencies, ["bible 1.0.0"]);
    assert_eq!(lock.get("bible").unwrap().dependencies, ["lexicon 2.0.0"]);

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
    let translations = project.translations(&library).unwrap();
    let names: Vec<String> = translations.names().map(String::from).collect();
    assert_eq!(names, ["BIBLE", "MINE"]);

    // A dependency named differently from its package
    project.add("other", &git_dep(&lexicon, None)).unwrap();
    let error = project
        .install(&library, &fetcher, Update::Nothing)
        .unwrap_err();
    assert!(error.to_string().contains("name it `lexicon`"), "{error}");
}
