//! Each tool on the fixture project (`tests/fixtures/vault`: two Bibles numbered differently and
//! a map between them, a commentary, a dictionary, cross-references), copied into a folder of
//! the test's own with its own library and cache

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use logoxide::{Library, Project, config::Config, git::GitCli, resolve::Update};
use logoxide_app::{Logoxide, http::NoHttp};
use serde_json::{Value, json};

use crate::tools::{self, Context};

/// A test's folder: the fixtures copied, a library and cache, removed afterwards
struct World {
    root: PathBuf,
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        match entry.file_type().unwrap().is_dir() {
            true => copy(&entry.path(), &target),
            false => {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

impl World {
    /// The fixtures, with `more` added to the vault's manifest, installed
    fn new(name: &str, more: impl FnOnce(&Path) -> String) -> World {
        let root = std::env::temp_dir().join(format!("logoxide-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
            &root,
        );
        let extra = more(&root);
        if !extra.is_empty() {
            let manifest = root.join("vault/logoxide.toml");
            let text = std::fs::read_to_string(&manifest).unwrap();
            std::fs::write(&manifest, text + &extra).unwrap();
        }
        let world = World { root };
        let mut project = Project::open(world.root.join("vault")).unwrap();
        project
            .install(&world.library(), &GitCli::default(), Update::Nothing)
            .unwrap();
        world
    }

    fn library(&self) -> Library {
        Library::new(self.root.join("library"), self.root.join("cache"))
    }

    fn context(&self, allow_writes: bool) -> Context {
        let http = Arc::new(NoHttp);
        let fetcher =
            logoxide_app::fetch::fetcher(GitCli::default(), http.clone(), logoxide_app::quiet());
        let app = Logoxide::new(
            Config::default(),
            self.library(),
            http,
            fetcher,
            Box::new(logoxide::reader::DefaultReader),
        );
        Context::new(app, self.root.join("vault"), allow_writes)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn params<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

#[test]
fn packages_are_listed_and_described() {
    let world = World::new("list", |_| String::new());
    let context = world.context(false);
    let listed = tools::list_packages(&context, params(json!({}))).unwrap();
    assert_eq!(listed["total"], 6);
    let one = (listed["packages"].as_array().unwrap().iter())
        .find(|p| p["name"] == "one")
        .unwrap();
    assert_eq!(one["kind"], "bible");
    assert_eq!(one["free"], true);
    assert_eq!(one["installed"], true);
    assert_eq!(one["untrusted"]["title"], "A Free Bible");
    assert_eq!(one["untrusted"]["source"], "the lock file");
    // By its name, id, or abbreviation, in any case
    let info = tools::package_info(&context, params(json!({ "package": "arr" }))).unwrap();
    assert_eq!(info["free"], false);
    assert_eq!(info["books"], json!(["JHN"]));
    // Counted by its chapters' verse numbers: its John 3 ends at verse 18
    assert_eq!(info["verses"], 18);
    assert_eq!(info["untrusted"]["license"], "LicenseRef-AllRightsReserved");
    assert_eq!(info["untrusted"]["source"], "the manifest of arr 1.0.0");
    assert!(
        info["untrusted"]["licenseConcerns"][0]
            .as_str()
            .unwrap()
            .contains("isn't a free or open license")
    );
    let by_abbreviation = tools::package_info(&context, params(json!({ "package": "str" })));
    assert_eq!(by_abbreviation.unwrap()["id"], "strongs");
    let missing = tools::package_info(&context, params(json!({ "package": "nope" })));
    assert!(missing.unwrap_err().contains("no package `nope`"));
}

#[test]
fn catalogs_are_searched_with_their_text_marked_and_cleaned() {
    let world = World::new("search", |_| String::new());
    let context = world.context(false);
    let found = tools::search_catalogs(&context, params(json!({ "query": "king james" }))).unwrap();
    assert_eq!(found["total"], 1);
    let kjv = &found["results"][0];
    assert_eq!(kjv["id"], "kjv");
    assert_eq!(
        kjv["source"],
        json!({ "type": "git", "url": "https://example.org/kjv" })
    );
    assert_eq!(kjv["rights"]["redistributable"], true);
    // A catalog's words can't drive a terminal or reorder text
    assert_eq!(
        kjv["untrusted"]["description"],
        "The Authorized Version  of 1611"
    );
    assert!(
        kjv["untrusted"]["source"]
            .as_str()
            .unwrap()
            .contains("Test catalog")
    );
    // Pages of `limit`, then a cursor; at most 100 a page
    let first =
        tools::search_catalogs(&context, params(json!({ "query": "en", "limit": 1 }))).unwrap();
    assert_eq!(
        (
            first["total"].as_u64(),
            first["results"].as_array().map(Vec::len)
        ),
        (Some(2), Some(1))
    );
    let cursor = first["nextCursor"].as_str().unwrap().to_string();
    let second = tools::search_catalogs(
        &context,
        params(json!({ "query": "en", "limit": 1, "cursor": cursor })),
    )
    .unwrap();
    assert_ne!(second["results"][0]["id"], first["results"][0]["id"]);
    assert_eq!(second["nextCursor"], Value::Null);
    // A cursor from another question is refused
    let other = tools::search_catalogs(
        &context,
        params(json!({ "query": "bsb", "cursor": cursor })),
    );
    assert!(other.is_err());
}

#[test]
fn a_passage_is_read_with_its_license() {
    let world = World::new("read", |_| String::new());
    let context = world.context(false);
    let read =
        tools::read_passage(&context, params(json!({ "reference": "John 3:16 ONE" }))).unwrap();
    assert_eq!(read["bible"], "one");
    assert_eq!(read["osis"], "John.3.16");
    assert_eq!(
        read["untrusted"]["text"],
        "[16] For God so loved the world."
    );
    assert_eq!(read["license"]["free"], true);
    assert_eq!(read["license"]["untrusted"]["license"], "CC0-1.0");
    assert_eq!(read["nextCursor"], Value::Null);
    // The Bible named, without labels
    let arr = tools::read_passage(
        &context,
        params(json!({ "reference": "John 3:17", "bible": "ARR", "labels": false })),
    )
    .unwrap();
    assert_eq!(arr["untrusted"]["text"], "God loved the world.");
    assert_eq!(arr["license"]["free"], false);
    assert_eq!(arr["license"]["untrusted"]["publisher"], "A Publisher");
    // Text that isn't free: never more than half a book a call (its John is chapter 3's 18)
    let refused = tools::read_passage(&context, params(json!({ "reference": "John 3 ARR" })));
    assert!(refused.unwrap_err().contains("more than half of John"));
    // The free one, all of it
    let all = tools::read_passage(&context, params(json!({ "reference": "John 3 ONE" })));
    assert_eq!(all.unwrap()["verses"], 18);
}

#[test]
fn passages_are_compared_in_each_numbering() {
    let world = World::new("compare", |_| String::new());
    let context = world.context(false);
    let compared = tools::compare_passage(
        &context,
        params(json!({ "reference": "John 3:16", "bibles": ["ONE", "ARR"], "labels": false })),
    )
    .unwrap();
    let passages = compared["passages"].as_array().unwrap();
    assert_eq!(passages.len(), 2);
    assert_eq!(
        passages[0]["untrusted"]["text"],
        "For God so loved the world."
    );
    assert_eq!(passages[1]["reference"], "John 3:17");
    assert_eq!(passages[1]["mapped"], true);
    assert_eq!(passages[1]["untrusted"]["text"], "God loved the world.");
    assert_eq!(compared["licenses"]["arr"]["free"], false);
    assert_eq!(compared["licenses"]["one"]["free"], true);
    // More than half of the copyrighted one's John: its text withheld, the free one's given
    let compared = tools::compare_passage(
        &context,
        params(json!({ "reference": "John 3 ARR", "bibles": ["ONE", "ARR"] })),
    )
    .unwrap();
    let passages = compared["passages"].as_array().unwrap();
    assert!(passages[0]["untrusted"]["text"].is_string());
    assert!(
        passages[1]["withheld"]
            .as_str()
            .unwrap()
            .contains("half of John")
    );
    assert_eq!(passages[1]["untrusted"]["text"], Value::Null);
}

#[test]
fn references_are_mapped() {
    let world = World::new("map", |_| String::new());
    let context = world.context(false);
    let mapped = tools::map_reference(
        &context,
        params(json!({ "reference": "John 3:16", "from": "ONE", "to": "ARR" })),
    )
    .unwrap();
    assert_eq!(mapped["reference"], "John 3:17");
    assert_eq!(
        (mapped["from"].as_str(), mapped["to"].as_str()),
        (Some("English"), Some("Shifted"))
    );
    let back = tools::map_reference(
        &context,
        params(json!({ "reference": "John 3:18", "from": "Shifted", "to": "English" })),
    )
    .unwrap();
    assert_eq!(back["reference"], "John 3:17");
}

#[test]
fn commentaries_dictionaries_and_cross_references() {
    let world = World::new("lookups", |_| String::new());
    let context = world.context(false);
    let notes =
        tools::read_notes(&context, params(json!({ "reference": "John 3:16-18" }))).unwrap();
    assert_eq!(notes["total"], 2);
    assert_eq!(notes["notes"][0]["reference"], "John 3:16");
    assert_eq!(
        notes["notes"][0]["untrusted"]["text"],
        "God so loved: the world."
    );
    assert_eq!(notes["notes"][1]["reference"], "John 3:17-18");
    assert_eq!(notes["licenses"]["mhcc"]["free"], true);
    let none = tools::read_notes(&context, params(json!({ "reference": "Romans 1:1" }))).unwrap();
    assert_eq!(none["total"], 0);
    // A verse it skips gets the note before it in the chapter with `stretch`
    let skipped = json!({ "reference": "John 3:19" });
    assert_eq!(
        tools::read_notes(&context, params(skipped)).unwrap()["total"],
        0
    );
    let stretched = json!({ "reference": "John 3:19", "stretch": true });
    let stretched = tools::read_notes(&context, params(stretched)).unwrap();
    assert_eq!(stretched["notes"][0]["reference"], "John 3:17-18");
    // A package of another kind is refused
    let wrong = tools::read_notes(
        &context,
        params(json!({ "reference": "John 3:16", "commentary": "one" })),
    );
    assert!(wrong.unwrap_err().contains("not a Commentary"));

    let entry = tools::lookup_entry(&context, params(json!({ "key": "g0025" }))).unwrap();
    assert_eq!(entry["entries"][0]["key"], "G25");
    assert_eq!(entry["entries"][0]["untrusted"]["headword"], "ἀγαπάω");
    assert_eq!(entry["entries"][0]["untrusted"]["text"], "ἀγαπάω to love");
    let missing = tools::lookup_entry(&context, params(json!({ "key": "G27" }))).unwrap();
    assert_eq!(
        (missing["total"].as_u64(), missing["note"].as_str()),
        (Some(0), Some("no entry `G27`"))
    );

    let x =
        tools::cross_references(&context, params(json!({ "reference": "John 3:16-17" }))).unwrap();
    assert_eq!(x["osis"], "John.3.16-John.3.17");
    assert_eq!(x["total"], 3);
    assert_eq!(
        x["crossReferences"][0],
        json!({ "package": "tsk", "from": "John.3.16", "to": "Rom.5.8", "weight": 3.0 })
    );
}

/// A package `tagged` released twice in its own repository, John 3:16 reworded in 1.1.0
fn tagged(root: &Path) -> String {
    let dir = root.join("tagged");
    copy(&root.join("one"), &dir);
    let manifest = |version: &str| {
        std::fs::read_to_string(root.join("one/logoxide.toml"))
            .unwrap()
            .replace("id = \"one\"", "id = \"tagged\"")
            .replace("abbreviation = \"ONE\"", "abbreviation = \"TAG\"")
            .replace("version = \"1.0.0\"", &format!("version = \"{version}\""))
    };
    std::fs::write(dir.join("logoxide.toml"), manifest("1.0.0")).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "1.0.0"]);
    git(&dir, &["tag", "v1.0.0"]);
    std::fs::write(dir.join("logoxide.toml"), manifest("1.1.0")).unwrap();
    let usx = std::fs::read_to_string(dir.join("usx/JHN.usx")).unwrap();
    std::fs::write(dir.join("usx/JHN.usx"), usx.replace("so loved", "loved")).unwrap();
    git(&dir, &["commit", "-q", "-am", "1.1.0"]);
    git(&dir, &["tag", "v1.1.0"]);
    format!("tagged = {{ git = \"{}\" }}\n", dir.display())
}

#[test]
fn summaries_and_what_changed_between_versions() {
    let world = World::new("diff", tagged);
    let context = world.context(false);
    let summary = tools::show_summary(&context, params(json!({ "package": "one" }))).unwrap();
    assert_eq!(summary["published"], false);
    assert_eq!(summary["verses"], 4);
    assert_eq!(
        summary["books"],
        json!([{ "book": "JHN", "chapters": 3, "verses": 18 }])
    );
    let old = tools::show_summary(
        &context,
        params(json!({ "package": "TAG", "version": "1.0.0" })),
    )
    .unwrap();
    assert_eq!(old["version"], "1.0.0");
    // A released version against the installed one, or two released ones
    let d = tools::diff_versions(
        &context,
        params(json!({ "package": "tagged", "from": "1.0.0" })),
    )
    .unwrap();
    assert_eq!(
        (d["from"].as_str(), d["to"].as_str()),
        (Some("1.0.0"), Some("1.1.0"))
    );
    assert_eq!(
        d["changes"],
        json!([{ "change": "reworded", "verse": "JHN 3:16", "to": null }])
    );
    assert_eq!(d["change"], "minor");
    let same = tools::diff_versions(
        &context,
        params(json!({ "package": "tagged", "from": "v1.1.0", "to": "1.1.0" })),
    )
    .unwrap();
    assert_eq!(same["total"], 0);
    // A path package has no releases to read
    let path = tools::diff_versions(
        &context,
        params(json!({ "package": "one", "from": "1.0.0" })),
    );
    assert!(path.unwrap_err().contains("isn't from a git repository"));
}

/// A book package of one EPUB citing three passages
fn book(root: &Path) -> String {
    let xhtml = |body: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>x</title></head><body>{body}</body></html>"
        )
    };
    let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">x</dc:identifier><dc:title>Freely Given</dc:title><dc:language>en</dc:language></metadata>
  <manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="one.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let files = [
        ("mimetype", String::from("application/epub+zip")),
        (
            "META-INF/container.xml",
            String::from(
                r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
            ),
        ),
        ("OEBPS/content.opf", String::from(opf)),
        (
            "OEBPS/nav.xhtml",
            xhtml("<nav><ol><li><a href=\"one.xhtml\">One</a></li></ol></nav>"),
        ),
        (
            "OEBPS/one.xhtml",
            xhtml("<p>Freely give (Matthew 10:8), as in Matthew 10:8 and John 3:16.</p>"),
        ),
    ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    let dir = root.join("book");
    std::fs::create_dir_all(dir.join("epub")).unwrap();
    std::fs::write(
        dir.join("epub/freely.epub"),
        zip.finish().unwrap().into_inner(),
    )
    .unwrap();
    std::fs::write(
        dir.join("logoxide.toml"),
        "[package]\nid = \"freely\"\nname = \"Freely Given\"\nabbreviation = \"FG\"\nversion = \"1.0.0\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\"\nkind = \"book\"\n\n[formats]\nepub = { path = \"epub/\" }\n",
    )
    .unwrap();
    String::from("freely = { path = \"../book\" }\n")
}

#[test]
fn a_books_cited_passages() {
    let world = World::new("cited", book);
    let context = world.context(false);
    let cited = tools::cited_references(&context, params(json!({ "package": "freely" }))).unwrap();
    assert_eq!(cited["references"], 3);
    assert_eq!(
        cited["cited"],
        json!([{ "passage": "Matt.10.8", "count": 2 }, { "passage": "John.3.16", "count": 1 }])
    );
}

#[test]
fn writes_need_allow_writes() {
    let world = World::new("writes", |_| String::new());
    let read_only = world.context(false);
    let refused = tools::install(&read_only, params(json!({})));
    assert!(refused.unwrap_err().contains("--allow-writes"));
    let refused = tools::import(
        &read_only,
        params(json!({ "source": "ebible:eng-kjv2006" })),
    );
    assert!(refused.unwrap_err().contains("--allow-writes"));
    // Unlisted, too
    let names: Vec<String> = (crate::Server::new(world.context(false)).tools().into_iter())
        .map(|t| t.name.to_string())
        .collect();
    assert!(
        !names.iter().any(|n| n == "install" || n == "import"),
        "{names:?}"
    );
    assert_eq!(names.len(), 12);
    let writable = crate::Server::new(world.context(true)).tools();
    assert_eq!(writable.len(), 14);
    // Allowed: installing as locked; an import only from the project's catalogs, never a URL
    let context = world.context(true);
    let installed = tools::install(&context, params(json!({ "locked": true }))).unwrap();
    assert_eq!(installed["installed"].as_array().map(Vec::len), Some(6));
    let url = tools::import(
        &context,
        params(json!({ "source": "https://example.org/x.zip" })),
    );
    assert!(url.is_err());
    let unlisted = tools::import(&context, params(json!({ "source": "ebible:eng-kjv2006" })));
    assert!(
        unlisted
            .unwrap_err()
            .contains("in none of this project's catalogs")
    );
}
