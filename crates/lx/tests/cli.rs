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
