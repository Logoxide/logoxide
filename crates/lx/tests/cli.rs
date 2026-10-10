//! `lx` itself, run on packages made here: each test has its own folder, library, cache, and
//! config, so nothing touches the machine's

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::cargo::CommandCargoExt;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
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
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

const JHN: &str = r#"<usx version="3.0"><book code="JHN" style="id"/><chapter number="3" style="c"/><para style="p"><verse number="16" style="v"/>For God so loved <verse number="17" style="v"/>For God sent</para></usx>"#;

/// A package's files: John 3:16-17 as USX
fn package(dir: &Path, id: &str, version: &str) {
    std::fs::create_dir_all(dir.join("usx")).unwrap();
    let manifest = format!(
        "[package]\nid = \"{id}\"\nname = \"Test {id}\"\nabbreviation = \"{}\"\nversion = \"{version}\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n",
        id.to_uppercase()
    );
    std::fs::write(dir.join("logoxide.toml"), manifest).unwrap();
    std::fs::write(dir.join("usx/JHN.usx"), JHN).unwrap();
}

/// A package released in its own repository (tagged `v{version}`)
fn released(dir: &Path, id: &str, version: &str) {
    package(dir, id, version);
    if !dir.join(".git").exists() {
        git(dir, &["init", "-q", "-b", "main"]);
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", version]);
    git(dir, &["tag", &format!("v{version}")]);
}

/// A test's folder, with its own library, cache, and config
struct World {
    root: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        let root = std::env::temp_dir().join(format!("lx-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::write(root.join("config.toml"), "").unwrap();
        World { root }
    }

    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    /// `lx` in the vault, with this world's library, cache, and config
    fn lx(&self, args: &[&str]) -> std::process::Output {
        let vault = self.path("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let mut command = Command::cargo_bin("lx").unwrap();
        command
            .args(args)
            .current_dir(&vault)
            .env("LOGOXIDE_HOME", self.path("library"))
            .env("LOGOXIDE_CACHE", self.path("cache"))
            .env("LOGOXIDE_CONFIG", self.path("config.toml"))
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("home/.config"))
            .env("XDG_DATA_HOME", self.path("home/.local/share"))
            .env("XDG_CACHE_HOME", self.path("home/.cache"));
        command.output().unwrap()
    }

    /// `lx`, which must succeed; what it printed
    fn ok(&self, args: &[&str]) -> String {
        let out = self.lx(args);
        assert!(
            out.status.success(),
            "lx {args:?}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let text = self.ok(args);
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("lx {args:?}: {e}: {text}"))
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_vault_adds_lists_turns_off_and_cleans() {
    let world = World::new("vault");
    let repo = world.path("tb");
    released(&repo, "tb", "1.0.0");
    let repo = repo.display().to_string();

    assert!(world.ok(&["init"]).contains("created"));
    // Nothing yet
    assert!(world.ok(&["list"]).contains("nothing installed yet"));
    assert_eq!(
        world.json(&["list", "--format", "json"]),
        serde_json::json!([])
    );

    let added = world.ok(&["add", "--git", &repo]);
    assert!(added.contains("added tb 1.0.0"), "{added}");
    assert!(world.path("vault/logoxide.lock").is_file());
    let installed = world.ok(&["install"]);
    assert!(installed.contains("1 packages, locked"), "{installed}");

    let listed = world.ok(&["list"]);
    assert!(
        listed.contains("tb 1.0.0  TB (Test tb), CC0-1.0"),
        "{listed}"
    );
    let listed = world.json(&["list", "--format", "json"]);
    assert_eq!(listed[0]["id"], "tb");
    assert_eq!(listed[0]["version"], "1.0.0");
    assert_eq!(listed[0]["abbreviation"], "TB");
    assert_eq!(listed[0]["enabled"], true);
    assert_eq!(listed[0]["installed"], true);
    assert_eq!(listed[0]["dependency"], false);
    assert_eq!(listed[0]["git"], repo);

    let info = world.json(&["info", "tb", "--format", "json"]);
    // Counted as the cache does: each chapter up to its last verse
    assert_eq!(
        (info["books"].as_u64(), info["verses"].as_u64()),
        (Some(1), Some(17))
    );
    assert_eq!(info["unchanged"], true);

    // On and off: 0 and 1, and 2 for an error
    let on = world.lx(&["is-enabled", "tb"]);
    assert_eq!(
        (on.status.code(), on.stdout.as_slice()),
        (Some(0), &b"on\n"[..])
    );
    assert!(world.ok(&["disable", "tb"]).contains("turned tb off"));
    let off = world.lx(&["is-enabled", "tb"]);
    assert_eq!(
        (off.status.code(), off.stdout.as_slice()),
        (Some(1), &b"off\n"[..])
    );
    assert_eq!(world.lx(&["is-enabled", "nothing"]).status.code(), Some(2));
    assert!(world.ok(&["list"]).contains("(off)"));
    assert!(world.ok(&["list", "--enabled"]).is_empty());
    let disabled = world.json(&["list", "--disabled", "--format", "json"]);
    assert_eq!(disabled[0]["enabled"], false);
    world.ok(&["enable", "tb"]);
    assert_eq!(world.lx(&["is-enabled", "tb"]).status.code(), Some(0));

    // Every version is used until it's removed
    assert!(
        world
            .ok(&["clean"])
            .contains("every installed version is used")
    );
    world.ok(&["remove", "tb"]);
    let unused = world.ok(&["clean"]);
    assert!(unused.contains("unused: tb 1.0.0+g"), "{unused}");
    assert!(
        world
            .ok(&["clean", "--remove"])
            .contains("removed tb 1.0.0+g")
    );
    assert!(!world.path("library/packages/tb").exists());
    assert!(
        world
            .ok(&["clean"])
            .contains("every installed version is used")
    );
}

#[test]
fn path_packages_are_used_in_place() {
    let world = World::new("path");
    package(&world.path("local"), "local", "0.1.0");
    world.ok(&["init"]);
    let added = world.ok(&["add", "--path", "../local"]);
    assert!(added.contains("added local 0.1.0"), "{added}");
    let listed = world.ok(&["list"]);
    assert!(listed.contains("path ../local"), "{listed}");
    // Nothing goes into the library for it
    assert!(
        world
            .ok(&["clean"])
            .contains("every installed version is used")
    );
    let listed = world.json(&["list", "--format", "json"]);
    assert_eq!(listed[0]["path"], "../local");
}

#[test]
fn paths_are_kept_relative() {
    let world = World::new("relative");
    package(&world.path("local"), "local", "0.1.0");
    world.ok(&["init"]);
    // Given absolute, written relative to the manifest
    let absolute = world.path("local").display().to_string();
    let added = world.lx(&["add", "--path", &absolute]);
    assert!(added.status.success());
    assert!(!String::from_utf8_lossy(&added.stderr).contains("warning"));
    let manifest = std::fs::read_to_string(world.path("vault/logoxide.toml")).unwrap();
    assert!(manifest.contains("path = \"../local\""), "{manifest}");

    // One written absolute by hand is warned of: the lock can't be used elsewhere
    std::fs::write(
        world.path("vault/logoxide.toml"),
        format!("[dependencies]\nlocal = {{ path = \"{absolute}\" }}\n"),
    )
    .unwrap();
    let out = world.lx(&["install"]);
    assert!(out.status.success());
    let warned = String::from_utf8_lossy(&out.stderr);
    assert!(warned.contains("is absolute"), "{warned}");
}

#[test]
fn check_reads_every_book() {
    let world = World::new("check");
    let dir = world.path("tb");
    package(&dir, "tb", "1.0.0");
    let dir = dir.display().to_string();
    let checked = world.ok(&["check", &dir]);
    assert!(checked.contains("1 books, 2 verses"), "{checked}");
    let report = world.json(&["check", &dir, "--format", "json"]);
    assert_eq!(report["ok"], true);
    assert_eq!(
        (report["books"].as_u64(), report["verses"].as_u64()),
        (Some(1), Some(2))
    );
    assert_eq!(report["titles"][0]["path"], "usx/JHN.usx");
    assert_eq!(report["skipped"], serde_json::json!([]));

    // Front matter is noted, a book logoxide doesn't know warned of, and both passed over
    let usx = |code: &str| format!(r#"<usx version="3.0"><book code="{code}" style="id"/></usx>"#);
    std::fs::write(world.path("tb/usx/front.usx"), usx("FRT")).unwrap();
    std::fs::write(world.path("tb/usx/ZZZ.usx"), usx("ZZZ")).unwrap();
    let out = world.lx(&["check", &dir]);
    assert!(out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("note: usx/front.usx: FRT isn't a book"),
        "{said}"
    );
    assert!(
        said.contains("warning: usx/ZZZ.usx: ZZZ is a book"),
        "{said}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 books, 2 verses"));
    let report = world.json(&["check", &dir, "--format", "json"]);
    assert_eq!(report["ok"], true);
    assert_eq!(report["skipped"][0]["reason"], "unknown");
    assert_eq!(report["skipped"][1]["reason"], "peripheral");

    // John in two files is an error naming both
    std::fs::write(world.path("tb/usx/john.usx"), JHN).unwrap();
    let out = world.lx(&["check", &dir]);
    assert_eq!(out.status.code(), Some(1));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("usx/JHN.usx and usx/john.usx"), "{said}");
    std::fs::remove_file(world.path("tb/usx/john.usx")).unwrap();

    // The versification map fixture is read whole
    let map =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../logoxide/tests/fixtures/english-original");
    let checked = world.ok(&["check", &map.display().to_string()]);
    assert!(
        checked.contains("a versification map from English to Original"),
        "{checked}"
    );

    // A folder without a package is an error
    let out = world.lx(&["check", &world.path("nothing").display().to_string()]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn formats_are_only_where_a_command_has_them() {
    let world = World::new("format");
    world.ok(&["init"]);
    let out = world.lx(&["remove", "x", "--format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--format json"));
    assert_eq!(
        world.lx(&["list", "--format", "tsv"]).status.code(),
        Some(1)
    );
    let catalogs = world.json(&["catalog", "list", "--format", "json"]);
    assert!(
        catalogs["catalogs"]
            .as_array()
            .is_some_and(|c| !c.is_empty())
    );
}

#[test]
fn catalog_text_is_printed_without_escapes() {
    let world = World::new("escapes");
    world.ok(&["init"]);
    let catalog = world.path("catalog.toml");
    std::fs::write(
        &catalog,
        "[[package]]\nid = \"kjv\"\nname = \"King James\\u001b]0;owned\\u0007 \\u001b[2JVersion\\u202e\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\\r\\u001b[1A\"\ngit = \"https://example.com/kjv\"\n",
    )
    .unwrap();
    let out = world.ok(&["search", "kjv", "--catalog", catalog.to_str().unwrap()]);
    assert!(out.contains("KJV (kjv): King James Version, "), "{out}");
    assert!(
        !out.contains(['\u{1b}', '\u{7}', '\r', '\u{202e}']),
        "{out:?}"
    );
}

#[test]
fn catalog_ids_are_checked_and_shadowing_is_said() {
    let world = World::new("ids");
    world.ok(&["init"]);
    // An entry whose id isn't one is passed over, with a warning; the rest are read
    let catalog = world.path("catalog.toml");
    std::fs::write(
        &catalog,
        "[[package]]\nid = \"../kjv\"\nname = \"Bad\"\nabbreviation = \"BAD\"\nlanguage = \"en\"\ngit = \"https://example.com/bad\"\n\n[[package]]\nid = \"kjv\"\nname = \"King James Version\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\ngit = \"https://example.com/kjv\"\n",
    )
    .unwrap();
    let out = world.lx(&["search", "--catalog", catalog.to_str().unwrap()]);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{stderr}");
    assert!(
        stdout.contains("KJV (kjv)") && !stdout.contains("BAD"),
        "{stdout}"
    );
    assert!(
        stderr.contains("warning: ") && stderr.contains("passed over package `../kjv`"),
        "{stderr}"
    );
    // Two registries list `tb`: adding one's says so
    released(&world.path("a/tb"), "tb", "1.0.0");
    released(&world.path("b/tb"), "tb", "1.0.0");
    for name in ["a", "b"] {
        let folder = world.path(name).display().to_string();
        world.ok(&["catalog", "add", &folder, "--name", name]);
    }
    let out = world.lx(&["add", "b:tb"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("warning: `tb` is in b, a; b's is used"),
        "{stderr}"
    );
}

/// Commits what's in a package's folder and tags it (`tag`)
fn tagged(dir: &Path, tag: &str) {
    if !dir.join(".git").exists() {
        git(dir, &["init", "-q", "-b", "main"]);
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", tag]);
    git(dir, &["tag", tag]);
}

/// Adds lines to a package's `logoxide.toml`
fn append_manifest(dir: &Path, text: &str) {
    let manifest = std::fs::read_to_string(dir.join("logoxide.toml")).unwrap();
    std::fs::write(dir.join("logoxide.toml"), format!("{manifest}{text}")).unwrap();
}

/// The one installed version of a package in a world's library
fn installed_dir(world: &World, id: &str) -> PathBuf {
    let mut versions = std::fs::read_dir(world.path(&format!("library/packages/{id}"))).unwrap();
    versions.next().unwrap().unwrap().path()
}

#[test]
fn a_failed_add_or_remove_leaves_the_manifest_alone() {
    let world = World::new("failed-add");
    world.ok(&["init"]);
    let manifest = world.path("vault/logoxide.toml");
    let before = std::fs::read(&manifest).unwrap();

    // A repository that isn't there
    let missing = world.path("nothing").display().to_string();
    assert_eq!(world.lx(&["add", "--git", &missing]).status.code(), Some(1));
    assert_eq!(std::fs::read(&manifest).unwrap(), before);

    // A package that reads, but whose own dependency can't be fetched: installing fails after
    // the manifest was edited in memory
    let repo = world.path("tb");
    package(&repo, "tb", "1.0.0");
    append_manifest(
        &repo,
        &format!("\n[dependencies]\ngone = {{ git = \"{missing}\" }}\n"),
    );
    tagged(&repo, "v1.0.0");
    let out = world.lx(&["add", "--git", &repo.display().to_string()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(std::fs::read(&manifest).unwrap(), before);
    assert!(!world.path("vault/logoxide.lock").exists());

    // Removing, when what's left no longer installs (an installed copy was edited)
    let other = world.path("ob");
    released(&other, "ob", "1.0.0");
    let kept = world.path("kb");
    released(&kept, "kb", "1.0.0");
    world.ok(&["add", "--git", &other.display().to_string()]);
    world.ok(&["add", "--git", &kept.display().to_string()]);
    let before = std::fs::read(&manifest).unwrap();
    std::fs::write(installed_dir(&world, "kb").join("usx/JHN.usx"), "edited").unwrap();
    assert_eq!(world.lx(&["remove", "ob"]).status.code(), Some(1));
    assert_eq!(std::fs::read(&manifest).unwrap(), before);
}

/// A repository's current commit
fn head(dir: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn revisions_are_whole_commits() {
    let world = World::new("rev");
    world.ok(&["init"]);
    let repo = world.path("tb");
    released(&repo, "tb", "1.0.0");
    let commit = head(&repo);
    let url = repo.display().to_string();
    let out = world.lx(&["add", "--git", &url, "--rev", &commit[..8]]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("isn't a full commit id"), "{error}");
    let added = world.ok(&["add", "--git", &url, "--rev", &commit]);
    assert!(added.contains("added tb 1.0.0"), "{added}");
}

#[test]
fn update_names_only_what_is_there() {
    let world = World::new("update");
    world.ok(&["init"]);
    let repo = world.path("tb");
    released(&repo, "tb", "1.0.0");
    world.ok(&["add", "--git", &repo.display().to_string()]);
    let out = world.lx(&["update", "tbb"]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("nothing here is named tbb (the manifest has tb)"),
        "{error}"
    );
    world.ok(&["update", "tb"]);
    world.ok(&["update"]);
}

#[test]
fn a_gone_commit_and_a_moved_tag_are_said() {
    let world = World::new("moved");
    world.ok(&["init"]);
    let repo = world.path("tb");
    released(&repo, "tb", "1.0.0");
    world.ok(&["add", "--git", &repo.display().to_string()]);

    // The release is made again, its tag moved, and the old commit dropped from the repository
    std::fs::write(repo.join("usx/JHN.usx"), JHN.replace("loved", "LOVED")).unwrap();
    git(
        &repo,
        &["commit", "-q", "-a", "--amend", "-m", "1.0.0 again"],
    );
    git(&repo, &["tag", "-f", "v1.0.0"]);
    git(&repo, &["reflog", "expire", "--expire=now", "--all"]);
    git(&repo, &["gc", "-q", "--prune=now"]);

    // Another device (a new library) can't fetch what's locked, and is told what to do
    std::fs::remove_dir_all(world.path("library/packages")).unwrap();
    let out = world.lx(&["install"]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("may no longer be on the host"), "{error}");
    assert!(error.contains("`lx update tb`"), "{error}");

    let out = world.lx(&["update", "tb"]);
    assert!(out.status.success());
    let warned = String::from_utf8_lossy(&out.stderr);
    assert!(
        warned.contains("warning: tb 1.0.0: v1.0.0 was moved"),
        "{warned}"
    );
    // Once only: the lock now has the new commit
    assert!(!String::from_utf8_lossy(&world.lx(&["update"]).stderr).contains("moved"));
}

#[test]
fn tags_that_are_not_releases() {
    let world = World::new("tags");
    world.ok(&["init"]);
    let repo = world.path("tb");
    package(&repo, "tb", "1.0.0");
    tagged(&repo, "v1.0");
    let url = repo.display().to_string();
    // No release: refused, naming the tag that isn't one
    let out = world.lx(&["add", "--git", &url]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("has no releases"), "{error}");
    assert!(error.contains("v1.0 names no version"), "{error}");
    assert!(error.contains("--branch or --rev"), "{error}");
    // Following a branch is asked for by name
    let added = world.ok(&["add", "--git", &url, "--branch", "main"]);
    assert!(added.contains("added tb 1.0.0"), "{added}");

    // A release beside one that isn't: added, and the other warned of
    world.ok(&["remove", "tb"]);
    git(&repo, &["tag", "v1.0.0"]);
    let out = world.lx(&["add", "--git", &url]);
    assert!(out.status.success());
    let warned = String::from_utf8_lossy(&out.stderr);
    assert!(warned.contains("warning:"), "{warned}");
    assert!(warned.contains("v1.0 names no version"), "{warned}");
}

#[test]
fn locked_and_offline_installs() {
    let world = World::new("frozen");
    world.ok(&["init"]);
    let repo = world.path("tb");
    released(&repo, "tb", "1.0.0");
    world.ok(&["add", "--git", &repo.display().to_string()]);
    let lock_path = world.path("vault/logoxide.lock");
    let lock = std::fs::read(&lock_path).unwrap();
    world.ok(&["install", "--locked"]);
    world.ok(&["install", "--offline"]);
    world.ok(&["install", "--locked", "--offline"]);
    assert_eq!(std::fs::read(&lock_path).unwrap(), lock);

    // The manifest asks for something the lock doesn't have
    let other = world.path("ob");
    released(&other, "ob", "1.0.0");
    let manifest_path = world.path("vault/logoxide.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        &manifest_path,
        format!("{manifest}ob = {{ git = \"{}\" }}\n", other.display()),
    )
    .unwrap();
    let out = world.lx(&["install", "--locked"]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("`ob` isn't locked"), "{error}");
    assert!(error.contains("--locked"), "{error}");
    let out = world.lx(&["install", "--offline"]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("offline nothing is fetched"), "{error}");
    assert_eq!(std::fs::read(&lock_path).unwrap(), lock);
    assert!(!world.path("library/packages/ob").exists());

    // Locked, but not installed (another device): offline can't, locked can
    std::fs::write(&manifest_path, manifest).unwrap();
    std::fs::remove_dir_all(world.path("library/packages")).unwrap();
    let out = world.lx(&["install", "--offline"]);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        error.contains("tb 1.0.0 isn't installed, and offline nothing is fetched"),
        "{error}"
    );
    world.ok(&["install", "--locked"]);
    world.ok(&["install", "--offline"]);
    assert_eq!(std::fs::read(&lock_path).unwrap(), lock);

    // Nothing locked yet
    std::fs::remove_file(&lock_path).unwrap();
    assert_eq!(world.lx(&["install", "--locked"]).status.code(), Some(1));
    assert!(!lock_path.exists());
}

#[test]
fn two_bibles_with_one_name_are_warned_of() {
    let world = World::new("clash");
    world.ok(&["init"]);
    let repo = world.path("tb");
    released(&repo, "tb", "1.0.0");
    released(&repo, "tb", "2.0.0");
    // A collection of tb 1, beside the manifest's own tb 2
    let shelf = world.path("shelf");
    std::fs::create_dir_all(&shelf).unwrap();
    std::fs::write(
        shelf.join("logoxide.toml"),
        format!(
            "[package]\nid = \"shelf\"\nname = \"Shelf\"\nabbreviation = \"SH\"\nversion = \"1.0.0\"\nkind = \"collection\"\nlanguage = \"en\"\n\n[dependencies]\ntb = {{ git = \"{}\", version = \"1\" }}\n",
            repo.display()
        ),
    )
    .unwrap();
    tagged(&shelf, "v1.0.0");
    world.ok(&["add", "--git", &repo.display().to_string()]);
    world.ok(&["add", "--git", &shelf.display().to_string()]);
    let out = world.lx(&["install"]);
    assert!(out.status.success());
    let warned = String::from_utf8_lossy(&out.stderr);
    assert!(
        warned.contains("warning: `TB` names tb 1.0.0 and tb 2.0.0"),
        "{warned}"
    );
    // Its own abbreviation for the manifest's copy, and the warning goes
    world.ok(&["remove", "tb"]);
    let added = world.lx(&[
        "add",
        "--git",
        &repo.display().to_string(),
        "--abbreviation",
        "TB2",
    ]);
    assert!(added.status.success());
    let out = world.lx(&["install"]);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("warning"));
}
