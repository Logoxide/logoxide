//! Releasing: summaries, version checks, and reading a release's summary alone

use std::{path::Path, process::Command, sync::Arc};

use biblix::{
    Features, FsStorage, Package, Storage,
    git::{Fetcher, GitCli, GitRef},
    release::{check, plan, release},
    summary::{Change, SUMMARY, Summary, VERSES, summarize},
};

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

fn write(repo: &Path, version: &str, verses: &[(u8, &str)]) {
    let body: String = verses
        .iter()
        .map(|(n, text)| format!("<verse number=\"{n}\" style=\"v\"/>{text} "))
        .collect();
    std::fs::create_dir_all(repo.join("usx")).unwrap();
    std::fs::write(
        repo.join("usx/JHN.usx"),
        format!("<usx version=\"3.0\"><book code=\"JHN\" style=\"id\"/><chapter number=\"3\" style=\"c\"/><para style=\"p\">{body}<note style=\"f\">a note</note></para></usx>"),
    )
    .unwrap();
    std::fs::write(
        repo.join("biblix.toml"),
        format!("[package]\nid = \"tb\"\nname = \"tb\"\nabbreviation = \"TB\"\nversion = \"{version}\"\nlanguage = \"en\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n"),
    )
    .unwrap();
}

#[test]
fn releases_follow_what_changed() {
    let root = std::env::temp_dir().join(format!("biblix-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("tb");
    std::fs::create_dir_all(&repo).unwrap();
    let cli = GitCli::default();

    write(
        &repo,
        "1.0.0",
        &[(16, "For God so loved"), (17, "For God sent")],
    );
    git(&repo, &["init", "-q", "-b", "main"]);
    // The first release: nothing to compare with
    let first = plan(&repo, &cli).unwrap();
    assert!(first.previous.is_none());
    release(&repo, &first, &cli).unwrap();
    assert!(repo.join(SUMMARY).is_file() && repo.join(VERSES).is_file());
    let (summary, verses) = Summary::read(&FsStorage::new(&repo)).unwrap();
    assert_eq!((summary.verses, verses.len()), (2, 2));
    assert_eq!(summary.books["JHN"], [0, 0, 17]);
    assert_eq!(summary.contents["note"], 1);

    // A note changed: a patch. The same version again isn't allowed
    let text = std::fs::read_to_string(repo.join("usx/JHN.usx")).unwrap();
    std::fs::write(
        repo.join("usx/JHN.usx"),
        text.replace("a note", "another note"),
    )
    .unwrap();
    let p = plan(&repo, &cli).unwrap();
    assert_eq!(p.change(), Change::Patch);
    assert!(
        check(&p)
            .unwrap_err()
            .to_string()
            .contains("1.0.1 or later")
    );

    // A word changed: a minor change, so 1.0.1 is too small
    write(
        &repo,
        "1.0.1",
        &[(16, "For God so loved the world"), (17, "For God sent")],
    );
    let p = plan(&repo, &cli).unwrap();
    assert_eq!(p.change(), Change::Minor);
    assert!(
        check(&p)
            .unwrap_err()
            .to_string()
            .contains("1.1.0 or later")
    );
    write(
        &repo,
        "1.1.0",
        &[(16, "For God so loved the world"), (17, "For God sent")],
    );
    release(&repo, &plan(&repo, &cli).unwrap(), &cli).unwrap();

    // A verse removed: major
    write(&repo, "1.2.0", &[(16, "For God so loved the world")]);
    let p = plan(&repo, &cli).unwrap();
    assert_eq!(
        (p.change(), p.minimum.clone().unwrap().to_string()),
        (Change::Major, "2.0.0".to_string())
    );
    write(&repo, "2.0.0", &[(16, "For God so loved the world")]);
    release(&repo, &plan(&repo, &cli).unwrap(), &cli).unwrap();

    // The published summary matches the text
    let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(&repo));
    let package = Package::open(storage.clone(), Features::default()).unwrap();
    assert_eq!(
        summarize(&package).unwrap(),
        Summary::read(storage.as_ref()).unwrap()
    );

    // A release's summary, fetched alone
    let dest = root.join("shown");
    let url = format!("file://{}", repo.display());
    cli.fetch_files(
        &url,
        &GitRef::Tag("v1.1.0".into()),
        &[SUMMARY, VERSES],
        &FsStorage::new(&dest),
    )
    .unwrap();
    assert!(!dest.join("usx").exists());
    let (old, _) = Summary::read(&FsStorage::new(&dest)).unwrap();
    assert_eq!(
        (old.version.to_string(), old.verses),
        ("1.1.0".to_string(), 2)
    );
    let _ = std::fs::remove_dir_all(&root);
}
