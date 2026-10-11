//! Imports kept in the library: shared between projects, pinned by their lock, made again on
//! another machine, refused when the publisher changed them, updated beside the old version, and
//! vendored into a project on request

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use logoxide::{
    Library, Project, Result,
    config::Config,
    git::GitCli,
    reader::DefaultReader,
    resolve::{Frozen, Update},
};
use logoxide_app::{Logoxide, http::Http, import::Outcome};

const CATALOG: &str = "https://ebible.org/Scriptures/translations.csv";
const ZIP: &str = "https://ebible.org/Scriptures/eng-asv_usfm.zip";
const ID: &str = "ebible-eng-asv";

/// eBible.org's list with the ASV updated on `date`
fn catalog(date: &str) -> Vec<u8> {
    format!(
        "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"{date}\",\"ltr\",\"True\",\"ASV\"\n"
    )
    .into_bytes()
}

/// The ASV's zip with John 3:16 saying `words`
fn zip(words: &str) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, text) in [
        (
            "73-JHNeng-asv.usfm",
            format!("\\id JHN\n\\toc2 John\n\\c 3\n\\p\n\\v 16 {words}\n"),
        ),
        ("copr.htm", String::from("<p>public domain</p>")),
    ] {
        writer
            .start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A web of fixed files, counting how often each is asked for
#[derive(Default)]
struct Web {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    gets: Mutex<BTreeMap<String, usize>>,
}

impl Web {
    fn serve(&self, url: &str, bytes: Vec<u8>) {
        self.files.lock().unwrap().insert(url.to_string(), bytes);
    }
    fn gets(&self, url: &str) -> usize {
        self.gets.lock().unwrap().get(url).copied().unwrap_or(0)
    }
}

impl Http for Web {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        *self
            .gets
            .lock()
            .unwrap()
            .entry(url.to_string())
            .or_default() += 1;
        (self.files.lock().unwrap().get(url).cloned())
            .ok_or_else(|| logoxide::LogoxideError::network(url, "not found"))
    }
}

struct World {
    root: PathBuf,
    web: Arc<Web>,
}

impl World {
    fn new(name: &str) -> World {
        let root =
            std::env::temp_dir().join(format!("logoxide-imports-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let web = Arc::new(Web::default());
        web.serve(CATALOG, catalog("2026-10-08"));
        web.serve(ZIP, zip("For God so loved the world."));
        World { root, web }
    }

    /// A logoxide on its own library (`name`), downloading from the web
    fn app(&self, name: &str) -> Logoxide {
        let library = Library::new(
            self.root.join(name),
            self.root.join(format!("{name}-cache")),
        );
        let config = Config {
            catalog_max_age: Some(0),
            ..Config::default()
        };
        Logoxide::new(
            config,
            library,
            self.web.clone(),
            Box::new(GitCli::default()),
            Box::new(DefaultReader),
        )
    }

    /// A project searching only eBible.org's list
    fn project(&self, name: &str) -> Project {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("logoxide.toml"),
            format!("[settings]\ncatalogs = [\"{CATALOG}\"]\ndefault-catalogs = false\n"),
        )
        .unwrap();
        Project::open(dir).unwrap()
    }

    /// Another project with `from`'s manifest and lock (a clone of its repository)
    fn clone_of(&self, from: &Project, name: &str) -> Project {
        let dir = self.root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for file in ["logoxide.toml", "logoxide.lock"] {
            std::fs::copy(from.dir().join(file), dir.join(file)).unwrap();
        }
        Project::open(dir).unwrap()
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The library folder a project's import is locked to (`1.0.0+i…`)
fn locked_key(project: &Project) -> String {
    project.lock().unwrap().get(ID).unwrap().key()
}

fn no_warnings() -> impl FnMut(String) {
    |w| panic!("warned: {w}")
}

fn library_dir(app: &Logoxide, key: &str) -> PathBuf {
    app.library().root().join("packages").join(ID).join(key)
}

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

#[test]
fn an_import_is_kept_in_the_library_and_shared() {
    let world = World::new("shared");
    let app = world.app("library");
    let mut first = world.project("first");
    let imported = (app.import(&mut first, "ebible:eng-asv", false, &mut no_warnings())).unwrap();
    assert_eq!(imported.version, "1.0.0");
    // In the library, by its files' checksum; nothing in the project but its manifest and lock
    let key = locked_key(&first);
    assert!(key.starts_with("1.0.0+i"), "{key}");
    assert_eq!(imported.dir, library_dir(&app, &key));
    assert!(read(&imported.dir, "usfm/73-JHNeng-asv.usfm").contains("so loved"));
    assert!(!first.dir().join("imports").exists());
    let manifest = read(first.dir(), "logoxide.toml");
    assert!(
        manifest.contains("ebible-eng-asv = { import = \"ebible:eng-asv\" }"),
        "{manifest}"
    );
    // Pinned: what it was made from, and the publisher's version
    let locked = first.lock().unwrap().get(ID).unwrap().clone();
    assert_eq!(locked.import.as_deref(), Some("ebible:eng-asv"));
    assert_eq!(locked.upstream_version.as_deref(), Some("2026-10-08"));
    assert_eq!(locked.downloads.len(), 1);
    assert_eq!(locked.downloads[0].url, ZIP);
    assert_eq!(
        locked.downloads[0].sha256.as_deref(),
        Some(logoxide::publish::sha256(&zip("For God so loved the world.")).as_str())
    );
    assert_eq!(world.web.gets(ZIP), 1);

    // Another project on the same library uses the same folder, without downloading: the catalog
    // lists the version the library has
    let mut second = world.project("second");
    (app.import(&mut second, "ebible:eng-asv", false, &mut no_warnings())).unwrap();
    assert_eq!(locked_key(&second), key);
    assert_eq!(world.web.gets(ZIP), 1);
    let installed: Vec<_> = (app.library().installed().into_iter())
        .filter(|(id, _)| id == ID)
        .collect();
    assert_eq!(installed.len(), 1);

    // Installing again reads nothing from the web
    let gets = world.web.gets(CATALOG);
    (app.install(
        &mut second,
        Update::Nothing,
        Frozen::default(),
        &mut no_warnings(),
    ))
    .unwrap();
    assert_eq!((world.web.gets(ZIP), world.web.gets(CATALOG)), (1, gets));

    // Importing it twice is refused
    let again = app.import(&mut first, "ebible:eng-asv", false, &mut no_warnings());
    assert!(again.unwrap_err().to_string().contains("already"));
}

#[test]
fn a_clone_makes_the_locked_import_again() {
    let world = World::new("clone");
    let app = world.app("library");
    let mut first = world.project("first");
    (app.import(&mut first, "ebible:eng-asv", false, &mut no_warnings())).unwrap();
    let key = locked_key(&first);

    // Another machine (its own library): offline, it can't
    let other = world.app("elsewhere");
    let mut clone = world.clone_of(&first, "clone");
    let offline = Frozen {
        locked: false,
        offline: true,
    };
    let error = other.install(&mut clone, Update::Nothing, offline, &mut no_warnings());
    assert!(error.unwrap_err().to_string().contains("offline"));
    // Online, from the locked download, to the same files, the lock unchanged
    let lock = (other.install(
        &mut clone,
        Update::Nothing,
        Frozen::default(),
        &mut no_warnings(),
    ))
    .unwrap();
    assert_eq!(&lock, first.lock().unwrap());
    assert!(library_dir(&other, &key).join("logoxide.toml").is_file());
    assert_eq!(world.web.gets(ZIP), 2);
}

#[test]
fn a_changed_download_is_refused_until_updated() {
    let world = World::new("changed");
    let app = world.app("library");
    let mut first = world.project("first");
    (app.import(&mut first, "ebible:eng-asv", false, &mut no_warnings())).unwrap();
    let key = locked_key(&first);

    // The publisher changed its zip without changing its date
    world
        .web
        .serve(ZIP, zip("For God so loved the whole world."));
    let other = world.app("elsewhere");
    let mut clone = world.clone_of(&first, "clone");
    let error = (other.install(
        &mut clone,
        Update::Nothing,
        Frozen::default(),
        &mut no_warnings(),
    ))
    .unwrap_err()
    .to_string();
    assert!(error.contains("changed it since"), "{error}");
    assert!(error.contains("lx update ebible-eng-asv"), "{error}");
    assert!(!library_dir(&other, &key).exists());
    // Updating it locks what the publisher has now
    let only = Update::Only([ID.to_string()].into());
    (other.install(&mut clone, only, Frozen::default(), &mut no_warnings())).unwrap();
    let now = locked_key(&clone);
    assert_ne!(now, key);
    assert!(read(&library_dir(&other, &now), "usfm/73-JHNeng-asv.usfm").contains("whole"));
}

#[test]
fn an_update_goes_beside_the_old_version() {
    let world = World::new("update");
    let app = world.app("library");
    let mut first = world.project("first");
    (app.import(&mut first, "ebible:eng-asv", false, &mut no_warnings())).unwrap();
    let mut second = world.project("second");
    (app.import(&mut second, "ebible:eng-asv", false, &mut no_warnings())).unwrap();
    let old = locked_key(&first);

    // eBible.org rewords John 3:16 and dates it
    world.web.serve(CATALOG, catalog("2026-11-01"));
    world
        .web
        .serve(ZIP, zip("For God so greatly loved the world."));
    let plans = app.plan_updates(&first, None, &mut no_warnings()).unwrap();
    assert_eq!(plans.len(), 1);
    let plan = &plans[0];
    assert!(plan.in_library);
    assert_eq!(plan.dir, library_dir(&app, &old));
    let Outcome::Changed(change) = &plan.outcome else {
        panic!("{:?}", plan.outcome);
    };
    // Rewording is a minor change
    assert_eq!(change.update.to.to_string(), "1.1.0");
    assert_eq!(change.upstream_version.as_deref(), Some("2026-11-01"));
    let importing = app.importing(&first);
    importing.prepare(&plan.import, change.build().unwrap());
    let downloads = world.web.gets(ZIP);
    let only = Update::Only([ID.to_string()].into());
    (app.install_with(&mut first, &importing, only, Frozen::default())).unwrap();
    // Made once, by the plan
    assert_eq!(world.web.gets(ZIP), downloads);
    let new = locked_key(&first);
    assert!(new.starts_with("1.1.0+i"), "{new}");
    let locked = first.lock().unwrap().get(ID).unwrap();
    assert_eq!(locked.upstream_version.as_deref(), Some("2026-11-01"));
    // The old version stays for the project still locking it
    assert_eq!(locked_key(&second), old);
    assert!(library_dir(&app, &old).join("logoxide.toml").is_file());
    assert!(read(&library_dir(&app, &new), "usfm/73-JHNeng-asv.usfm").contains("greatly"));

    // The second project's update takes the new version from the library, without downloading
    let only = Update::Only([ID.to_string()].into());
    (app.install(&mut second, only, Frozen::default(), &mut no_warnings())).unwrap();
    assert_eq!(locked_key(&second), new);
    assert_eq!(world.web.gets(ZIP), downloads);
}

#[test]
fn a_vendored_import_is_a_path_package() {
    let world = World::new("vendored");
    let app = world.app("library");
    let mut project = world.project("project");
    let imported = (app.import(&mut project, "ebible:eng-asv", true, &mut no_warnings())).unwrap();
    assert_eq!(imported.dir, project.dir().join("imports").join(ID));
    let manifest = read(project.dir(), "logoxide.toml");
    assert!(
        manifest.contains("path = \"imports/ebible-eng-asv\""),
        "{manifest}"
    );
    let locked = project.lock().unwrap().get(ID).unwrap();
    assert_eq!(locked.path.as_deref(), Some("imports/ebible-eng-asv"));
    assert!(locked.import.is_none());
}

#[test]
fn an_import_made_differently_is_refused() {
    let world = World::new("remade");
    let app = world.app("library");
    let mut first = world.project("first");
    (app.import(&mut first, "ebible:eng-asv", false, &mut no_warnings())).unwrap();

    // The same download, but the catalog now titles it otherwise: made again, it isn't the
    // locked text
    let renamed = String::from_utf8(catalog("2026-10-08")).unwrap().replace(
        "American Standard Version (1901)",
        "American Standard Version",
    );
    world.web.serve(CATALOG, renamed.into_bytes());
    let other = world.app("elsewhere");
    let mut clone = world.clone_of(&first, "clone");
    let error = (other.install(
        &mut clone,
        Update::Nothing,
        Frozen::default(),
        &mut no_warnings(),
    ))
    .unwrap_err()
    .to_string();
    assert!(error.contains("made again isn't the locked"), "{error}");
    // --locked can't take what it makes now either
    let locked = Frozen {
        locked: true,
        offline: false,
    };
    let only = Update::Only([ID.to_string()].into());
    assert!(
        other
            .install(&mut clone, only.clone(), locked, &mut no_warnings())
            .is_err()
    );
    (other.install(&mut clone, only, Frozen::default(), &mut no_warnings())).unwrap();
    assert_ne!(locked_key(&clone), locked_key(&first));
}
