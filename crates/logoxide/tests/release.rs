#![cfg(feature = "read")]

//! Releasing: summaries, version checks, and reading a release's summary alone

use std::{path::Path, process::Command, sync::Arc};

use logoxide::reader::DefaultReader;
use logoxide::{
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
        repo.join("logoxide.toml"),
        format!("[package]\nid = \"tb\"\nname = \"tb\"\nabbreviation = \"TB\"\nversion = \"{version}\"\nlanguage = \"en\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n"),
    )
    .unwrap();
}

#[test]
fn releases_follow_what_changed() {
    let root = std::env::temp_dir().join(format!("logoxide-release-{}", std::process::id()));
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
    let first = plan(&repo, &cli, &DefaultReader).unwrap();
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
    let p = plan(&repo, &cli, &DefaultReader).unwrap();
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
    let p = plan(&repo, &cli, &DefaultReader).unwrap();
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
    release(&repo, &plan(&repo, &cli, &DefaultReader).unwrap(), &cli).unwrap();

    // A verse removed: major
    write(&repo, "1.2.0", &[(16, "For God so loved the world")]);
    let p = plan(&repo, &cli, &DefaultReader).unwrap();
    assert_eq!(
        (p.change(), p.minimum.clone().unwrap().to_string()),
        (Change::Major, "2.0.0".to_string())
    );
    write(&repo, "2.0.0", &[(16, "For God so loved the world")]);
    release(&repo, &plan(&repo, &cli, &DefaultReader).unwrap(), &cli).unwrap();

    // The published summary matches the text
    let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(&repo));
    let package = Package::open(storage.clone(), Features::default()).unwrap();
    assert_eq!(
        summarize(&package, &DefaultReader).unwrap(),
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

/// A release whose verse hashes are in the layout from before hash rules (`LXV1`, no
/// `verse-hash` in its summary) is built again from its text: hashes made another way are never
/// taken for rewording
#[test]
fn an_older_hash_rule_is_built_again() {
    let root = std::env::temp_dir().join(format!("logoxide-rule-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("tb");
    std::fs::create_dir_all(&repo).unwrap();
    let cli = GitCli::default();
    write(
        &repo,
        "1.0.0",
        &[(16, "For God so lo\u{AD}ved"), (17, "For God sent")],
    );
    git(&repo, &["init", "-q", "-b", "main"]);
    let first = plan(&repo, &cli, &DefaultReader).unwrap();
    release(&repo, &first, &cli).unwrap();
    // Made as an older lx would have: 8 bytes of another hash a verse, no header
    let summary = std::fs::read_to_string(repo.join(SUMMARY)).unwrap();
    assert!(summary.contains("verse-hash = 1"), "{summary}");
    let old: String = (summary.lines())
        .filter(|l| !l.starts_with("verse-hash"))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(repo.join(SUMMARY), old).unwrap();
    let mut bin = b"LXV1".to_vec();
    for verse in [16u8, 17] {
        bin.extend_from_slice(&[43, 3, verse, verse, 1, 2, 3, 4, 5, 6, 7, 8]);
    }
    std::fs::write(repo.join(VERSES), bin).unwrap();
    git(&repo, &["commit", "-qam", "old layout"]);
    git(&repo, &["tag", "-f", "v1.0.0"]);
    assert!(Summary::read(&FsStorage::new(&repo)).is_err());

    // Nothing but the layout changed: a patch, not every verse reworded
    write(
        &repo,
        "1.0.1",
        &[(16, "For God so lo\u{AD}ved"), (17, "For God sent")],
    );
    let p = plan(&repo, &cli, &DefaultReader).unwrap();
    assert_eq!(p.change(), Change::Patch);
    let _ = std::fs::remove_dir_all(&root);
}

/// A dictionary package's `<entries>` file with these entries
fn write_dictionary(repo: &Path, version: &str, entries: &[(&str, &str)]) {
    let body: String = (entries.iter())
        .map(|(key, text)| format!("<entry key=\"{key}\">{text}</entry>\n"))
        .collect();
    std::fs::create_dir_all(repo.join("entries")).unwrap();
    std::fs::write(
        repo.join("entries/greek.xml"),
        format!("<entries keys=\"strongs-greek\" markup=\"plain\">\n{body}</entries>\n"),
    )
    .unwrap();
    std::fs::write(
        repo.join("logoxide.toml"),
        format!("[package]\nid = \"strongs\"\nname = \"Strong's\"\nabbreviation = \"STR\"\nversion = \"{version}\"\nlanguage = \"grc\"\nkind = \"lexicon\"\nkeys = \"strongs-greek\"\n\n[formats]\ndictionary = {{ path = \"entries/\" }}\n"),
    )
    .unwrap();
}

/// A dictionary's release writes `logoxide.entries.bin`, and the next release is judged by
/// its entries: reworded or added minor, removed major
#[test]
fn a_dictionarys_entries_are_released_and_compared() {
    use logoxide::summary::ENTRIES;
    let root = std::env::temp_dir().join(format!("logoxide-entries-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("strongs");
    std::fs::create_dir_all(&repo).unwrap();
    let cli = GitCli::default();
    write_dictionary(&repo, "1.0.0", &[("G0025", "to love"), ("G26", "love")]);
    git(&repo, &["init", "-q", "-b", "main"]);
    let first = plan(&repo, &cli, &DefaultReader).unwrap();
    assert_eq!((first.summary.entries, first.verses.len()), (2, 0));
    release(&repo, &first, &cli).unwrap();
    let bytes = std::fs::read(repo.join(ENTRIES)).unwrap();
    assert!(bytes.starts_with(b"LXE1"));
    assert_eq!(bytes.len(), 8 + 2 * 15);
    let (summary, _) = Summary::read(&FsStorage::new(&repo)).unwrap();
    assert_eq!(summary.entry_hashes.len(), 2);
    assert_eq!(
        summary.sizes["dictionary"] as usize,
        std::fs::read(repo.join("entries/greek.xml")).unwrap().len()
    );

    // Reworded (the key in another form is the same key): minor
    write_dictionary(
        &repo,
        "1.0.1",
        &[("G25", "to love, cherish"), ("G26", "love")],
    );
    let second = plan(&repo, &cli, &DefaultReader).unwrap();
    let (_, d) = second.previous.as_ref().unwrap();
    assert_eq!((d.entries_changed.len(), d.change()), (1, Change::Minor));
    assert_eq!(logoxide::release::describe(d), "1 entry reworded");
    assert!(check(&second).unwrap_err().to_string().contains("1.1.0"));
    write_dictionary(
        &repo,
        "1.1.0",
        &[("G25", "to love, cherish"), ("G26", "love")],
    );
    let second = plan(&repo, &cli, &DefaultReader).unwrap();
    release(&repo, &second, &cli).unwrap();

    // Removed: major; added: minor
    write_dictionary(
        &repo,
        "1.2.0",
        &[("G25", "to love, cherish"), ("G27", "beloved")],
    );
    let third = plan(&repo, &cli, &DefaultReader).unwrap();
    let (_, d) = third.previous.as_ref().unwrap();
    assert_eq!(
        (d.entries_removed.len(), d.entries_added.len(), d.change()),
        (1, 1, Change::Major)
    );
    assert_eq!(
        logoxide::release::describe(d),
        "1 entry removed, 1 entry added"
    );
    assert!(check(&third).is_err());
    let _ = std::fs::remove_dir_all(&root);
}
