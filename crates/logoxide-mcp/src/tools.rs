/*!
The tools, each a plain function of the [`Context`] and its parameters giving JSON (or an error in
words), so they're tested without a client; [`crate::server`] calls them off the async runtime.
Text from catalogs, manifests, and packages is in `untrusted` objects ([`Untrusted`]); passages,
notes, entries, and cross-references carry their package's license, and text that isn't free is
capped (the owner's D15: [`MAX_VERSES`] verses a call, never more than half a book)
*/

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use logoxide::{
    CachedPackage, Library, LockedPackage, LogoxideError, Package, Project, bible_data,
    catalog::{Catalog, CatalogEntry, CatalogSource},
    license::Derivatives,
    manifest::PackageKind,
    resolve::{Frozen, Update},
    summary::{Summary, VerseHash},
};
use logoxide_app::{Logoxide, Style};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use topos_bible::{data::chapter_verses::ChapterVerses, segments::passage::Passage};

use crate::limits::{
    LONG, MAX_VERSES, PAGE_CHARS, SEARCH_DEFAULT, SEARCH_MAX, SHORT, TEXT, Untrusted, chunks,
    clean, cursor, offset, page,
};

/// What a tool gives: its result, or what went wrong in words
pub type Out = Result<Value, String>;

/// An error's words, cleaned and capped like any untrusted text (they may quote a manifest)
fn error(e: impl std::fmt::Display) -> String {
    clean(&e.to_string(), LONG)
}

/// What every tool works with: logoxide-app's facade, the project's folder, whether writes are
/// allowed, and the catalogs once they're loaded
pub struct Context {
    app: Logoxide,
    project: PathBuf,
    allow_writes: bool,
    catalogs: Mutex<Option<Arc<Loaded>>>,
}

/// The project's registries and catalogs, loaded once a session, with what loading them warned of
struct Loaded {
    /// Each catalog's name and entries, registries first
    catalogs: Vec<(String, bool, Catalog)>,
    warnings: Vec<String>,
}

impl Context {
    pub fn new(app: Logoxide, project: PathBuf, allow_writes: bool) -> Context {
        Context {
            app,
            project,
            allow_writes,
            catalogs: Mutex::new(None),
        }
    }

    pub fn allows_writes(&self) -> bool {
        self.allow_writes
    }

    pub fn project_dir(&self) -> &Path {
        &self.project
    }

    fn library(&self) -> &Library {
        self.app.library()
    }

    /// The project, read again for each call (so an edit made meanwhile is seen)
    fn project(&self) -> Result<Project, String> {
        Project::open(&self.project).map_err(error)
    }

    /// The project's registries, then its catalogs, read the first time they're asked for
    fn loaded(&self, project: &Project) -> Result<Arc<Loaded>, String> {
        let mut held = (self.catalogs.lock()).map_err(|_| String::from("catalogs unavailable"))?;
        if let Some(loaded) = held.as_ref() {
            return Ok(loaded.clone());
        }
        let mut warnings = vec![];
        let mut catalogs = vec![];
        let registries = self.app.config().registries(project.manifest());
        for name in registries.keys() {
            match self.app.registry(project, name) {
                Ok(catalog) => {
                    warnings.extend(catalog.warnings.iter().cloned());
                    catalogs.push((name.clone(), true, catalog));
                }
                Err(e) => warnings.push(format!("{name}: {e}")),
            }
        }
        let loaded = self
            .app
            .catalogs(project, &mut |warning| warnings.push(warning))
            .map_err(error)?;
        for catalog in loaded {
            catalogs.push((catalog.name.clone(), false, catalog));
        }
        let loaded = Arc::new(Loaded { catalogs, warnings });
        *held = Some(loaded.clone());
        Ok(loaded)
    }
}

/// What a name means among the project's locked packages: the manifest's name for it, its id,
/// its abbreviation, or an alias (ignoring case), the manifest's own first
fn find<'a>(
    project: &'a Project,
    library: &Library,
    name: &str,
) -> Result<(&'a LockedPackage, Package), String> {
    let lock = project.lock().ok_or_else(|| {
        String::from("nothing is installed in this project yet (lx install, or the install tool)")
    })?;
    let exact = (lock.get(name))
        .or_else(|| lock.packages.iter().find(|p| p.id == name))
        .or_else(|| {
            (lock.packages.iter()).find(|p| {
                p.dependency_name().eq_ignore_ascii_case(name) || p.id.eq_ignore_ascii_case(name)
            })
        });
    if let Some(locked) = exact {
        return Ok((locked, library.open(locked, project.dir()).map_err(error)?));
    }
    for locked in &lock.packages {
        let Ok(package) = library.open(locked, project.dir()) else {
            continue;
        };
        let (abbreviation, aliases) = project.names(locked, package.info());
        if abbreviation.eq_ignore_ascii_case(name)
            || aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
        {
            return Ok((locked, package));
        }
    }
    Err(format!(
        "no package `{}` in this project (list_packages)",
        clean(name, SHORT)
    ))
}

/// Where a package's manifest text is from, for an `untrusted` object's `source`
fn manifest_source(info: &logoxide::manifest::Package) -> String {
    format!("the manifest of {} {}", info.id, info.version)
}

/// Where a package's text is from
fn text_source(info: &logoxide::manifest::Package) -> String {
    format!("the text of {} {}", info.id, info.version)
}

/// Whether a package's text is free to share and change: a free SPDX license, or rights saying
/// it's redistributable, commercially, with derivatives
fn is_free(info: &logoxide::manifest::Package) -> bool {
    let rights = info.text_rights();
    info.license
        .as_deref()
        .is_some_and(logoxide::license::is_free)
        || (rights.redistributable == Some(true)
            && rights.commercial == Some(true)
            && matches!(
                rights.derivatives,
                Some(Derivatives::Allowed | Derivatives::ShareAlike)
            ))
}

/// What a package's license allows, as every passage carries it
fn license(info: &logoxide::manifest::Package) -> Value {
    json!({
        "package": info.id,
        "version": info.version.to_string(),
        "free": is_free(info),
        "rights": info.text_rights(),
        "untrusted": Untrusted::new(&manifest_source(info))
            .maybe("license", info.license.as_deref(), SHORT)
            .maybe("licenseFile", info.license_file.as_deref(), SHORT)
            .maybe("publisher", info.publisher.as_deref(), SHORT)
            .list("authors", &info.authors)
            .value(),
        "note": "licenses are shown, never enforced: check that you may use or share this text",
    })
}

/// A package's verse counts, by book, from the library's cache
fn counts(cached: &CachedPackage) -> HashMap<topos_bible::data::books::BookId, Vec<u8>> {
    (cached.manifest().books.iter())
        .map(|(book, counts)| (*book, counts.clone()))
        .collect()
}

/// How many verses `passages` are in a package numbered by `counts`, and, if any book's share is
/// more than half of it, which
fn verses_in(
    counts: &HashMap<topos_bible::data::books::BookId, Vec<u8>>,
    passages: &[Passage],
) -> (usize, Option<String>) {
    let mut by_book: BTreeMap<u8, (usize, usize, String)> = BTreeMap::new();
    for passage in passages {
        let Some(chapters) = counts.get(&passage.book) else {
            continue;
        };
        let book_verses: usize = chapters.iter().map(|&c| c as usize).sum();
        let verses = passage.verses(Some(&ChapterVerses::new(chapters.clone())));
        let name = (bible_data().books().get_name(passage.book).cloned()).unwrap_or_default();
        let entry = by_book
            .entry(passage.book.0)
            .or_insert((0, book_verses, name));
        entry.0 += verses.len();
    }
    let total = by_book.values().map(|(n, _, _)| n).sum();
    let over = (by_book.values())
        .find(|(n, of, _)| *n * 2 > *of)
        .map(|(_, _, name)| name.clone());
    (total, over)
}

/// Why text that isn't free can't be returned (D15), if it can't: past the verses left in this
/// call, or more than half a book
fn withheld(
    info: &logoxide::manifest::Package,
    verses: usize,
    half_of: Option<String>,
    left: usize,
) -> Option<String> {
    if is_free(info) {
        return None;
    }
    let license = info.license.as_deref().unwrap_or("no license given");
    let why = match half_of {
        Some(book) => format!("more than half of {book}"),
        None if verses > left => format!("{verses} verses, past the {MAX_VERSES} a call allows"),
        None => return None,
    };
    Some(clean(
        &format!(
            "{} isn't known to be free ({license}): logoxide-mcp returns at most {MAX_VERSES} \
             verses of such a text a call, never more than half a book, and this is {why}. Ask \
             for less",
            info.id
        ),
        LONG,
    ))
}

/// A style from what's asked: verse and chapter labels (the default), or none
fn style(labels: Option<bool>) -> Style {
    let mut style = Style::reading();
    if labels == Some(false) {
        style.verse_label = None;
        style.chapter_label = None;
    }
    style
}

/// A reference as topos reads it (`John 3:16-18`, `Jn 3`)
fn passage(reference: &str) -> Result<Passage, String> {
    bible_data()
        .parse(reference)
        .ok_or_else(|| format!("`{}` isn't a Bible reference", clean(reference, SHORT)))
}

/// The plain text of a note's or an entry's tree: everything but metadata
fn tree_text(nodes: &[logoxide_text::markup::Node]) -> String {
    use logoxide_text::markup::{Filter, Kind, TextOptions, plain_text};
    let options = TextOptions {
        filter: Filter::all().drop(Kind::Metadata),
        ..TextOptions::default()
    };
    plain_text(nodes, &options).trim().to_string()
}

/// Packages of a kind in use, or the one `name` names (which must be of that kind)
fn of_kind<'a>(
    project: &'a Project,
    library: &Library,
    kind: PackageKind,
    name: Option<&str>,
) -> Result<Vec<(&'a LockedPackage, Package)>, String> {
    if let Some(name) = name {
        let (locked, package) = find(project, library, name)?;
        if package.info().kind != kind {
            return Err(format!(
                "{} is a {:?}, not a {kind:?}",
                package.info().id,
                package.info().kind
            ));
        }
        return Ok(vec![(locked, package)]);
    }
    let found = project.packages_of(library, kind).map_err(error)?;
    if found.is_empty() {
        return Err(format!("this project uses no {kind:?} packages"));
    }
    Ok(found)
}

// ---------------------------------------------------------------------------------------------
// The tools' parameters

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Words to find in packages' ids, names, abbreviations, languages, keywords, categories,
    /// works, descriptions, or the books they're about (`kjv`, `greek new testament`, `romans`)
    pub query: String,
    /// Only this registry of the project's (by name)
    #[serde(default)]
    pub registry: Option<String>,
    /// Results a page: 20 by default, at most 100
    #[serde(default)]
    pub limit: Option<usize>,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PackageParams {
    /// A package of the project: its name in the manifest, id, abbreviation, or alias
    pub package: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListParams {
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    /// A Bible reference (`John 3:16-18`, `Ps 23`), perhaps naming the Bible after it
    /// (`John 3:16 KJV`)
    pub reference: String,
    /// The Bible to read it from (its name, abbreviation, or alias); else the one the reference
    /// names, else the project's first
    #[serde(default)]
    pub bible: Option<String>,
    /// Verse and chapter labels (`[16]`, `John 3`) in the text: true by default
    #[serde(default)]
    pub labels: Option<bool>,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompareParams {
    /// A Bible reference (`John 3:16`), numbered by the Bible it names (`Psalm 51:1 KJV`), else
    /// by the first of `bibles`
    pub reference: String,
    /// The Bibles to compare, in order (names, abbreviations, or aliases); all of the project's
    /// when left out
    #[serde(default)]
    pub bibles: Vec<String>,
    /// Verse and chapter labels in the text: true by default
    #[serde(default)]
    pub labels: Option<bool>,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MapParams {
    /// A Bible reference (`Psalm 51:1`)
    pub reference: String,
    /// The numbering it's in: one of the project's Bibles, or a versification's name (`English`)
    pub from: String,
    /// The numbering wanted, the same way
    pub to: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SummaryParams {
    /// A package of the project
    pub package: String,
    /// A released version (`1.0.0`) of its git repository; the installed one when left out
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiffParams {
    /// A package of the project (one from a git repository)
    pub package: String,
    /// The older released version (`1.0.0`)
    pub from: String,
    /// The newer released version; the installed one when left out
    #[serde(default)]
    pub to: Option<String>,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CitedParams {
    /// A book package of the project (EPUBs)
    pub package: String,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NotesParams {
    /// A Bible reference (`John 3:16`)
    pub reference: String,
    /// One commentary of the project; all it uses when left out
    #[serde(default)]
    pub commentary: Option<String>,
    /// A note also covers the verses after it up to the commentary's next note in the chapter,
    /// so a verse it skips gets the note before it (off when left out)
    #[serde(default)]
    pub stretch: bool,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntryParams {
    /// The entry's key: a Strong's number (`G25`, `H430`, any zeros) or a headword
    pub key: String,
    /// One dictionary of the project; all it uses when left out
    #[serde(default)]
    pub dictionary: Option<String>,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CrossParams {
    /// A Bible reference (`Genesis 1:1`, `John 3:16-18`): every line covering any verse of it
    pub reference: String,
    /// One cross-reference package of the project; all it uses when left out
    #[serde(default)]
    pub package: Option<String>,
    /// The `nextCursor` of the page before
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InstallParams {
    /// Fail if the lock file would change, rather than resolving again
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ImportParams {
    /// An import one of the project's catalogs lists (`ebible:eng-kjv2006`, `fetch:eng_kjv`,
    /// `ccel:kjv`, `sword:crosswire/KJV`), as search_catalogs gives it
    pub source: String,
}

// ---------------------------------------------------------------------------------------------
// Catalogs

/// A catalog entry, its own words marked untrusted
fn catalog_entry(catalog: &str, entry: &CatalogEntry) -> Value {
    let source = match &entry.source {
        CatalogSource::Git { url } => json!({ "type": "git", "url": clean(url, LONG) }),
        CatalogSource::Registry { catalog } => {
            json!({ "type": "registry", "catalog": clean(catalog, LONG) })
        }
        other => json!({
            "type": "import",
            "import": other.import_name().map(|i| clean(i, SHORT)),
            "upstream": other.upstream_url().map(|u| clean(u, LONG)),
        }),
    };
    json!({
        "id": entry.id,
        "kind": entry.kind,
        "source": source,
        "rights": entry.text_rights(),
        "work": entry.work,
        "untrusted": Untrusted::new(&format!("the catalog {catalog}"))
            .text("name", &entry.name, SHORT)
            .text("abbreviation", &entry.abbreviation, SHORT)
            .text("language", &entry.language, SHORT)
            .maybe("languageName", entry.language_name.as_deref(), SHORT)
            .maybe("version", entry.version.as_deref(), SHORT)
            .maybe("description", entry.description.as_deref(), LONG)
            .maybe("license", entry.license.as_deref(), SHORT)
            .maybe("copyright", entry.copyright.as_deref(), LONG)
            .maybe("edition", entry.edition.as_deref(), SHORT)
            .list("keywords", &entry.keywords)
            .list("categories", &entry.categories)
            .list("aboutBooks", &entry.about_books)
            .value(),
    })
}

/// What a catalog entry is wherever it's listed (its repository, its registry and id, or its
/// import), so a registry's packages with one id are all kept
fn source_key(entry: &CatalogEntry) -> String {
    match &entry.source {
        CatalogSource::Git { url } => url.clone(),
        CatalogSource::Registry { catalog } => format!("{catalog}#{}", entry.id),
        other => other.import_name().unwrap_or_default().to_string(),
    }
}

/// `search_catalogs`: packages the project's registries and catalogs list
pub fn search_catalogs(context: &Context, params: SearchParams) -> Out {
    let project = context.project()?;
    let loaded = context.loaded(&project)?;
    if let Some(name) = &params.registry
        && !loaded
            .catalogs
            .iter()
            .any(|(n, registry, _)| *registry && n == name)
    {
        return Err(format!("no registry `{}` here", clean(name, SHORT)));
    }
    // Registries' packages all; a catalog's only if no catalog before it lists the id
    let mut names: HashMap<String, String> = HashMap::new();
    let mut all = Catalog {
        name: String::new(),
        entries: vec![],
        works: vec![],
        warnings: vec![],
    };
    let mut seen = std::collections::HashSet::new();
    for (name, registry, catalog) in &loaded.catalogs {
        if params
            .registry
            .as_ref()
            .is_some_and(|r| r != name || !registry)
        {
            continue;
        }
        for entry in &catalog.entries {
            if !registry && !seen.insert(entry.id.clone()) {
                continue;
            }
            names
                .entry(source_key(entry))
                .or_insert_with(|| name.clone());
            all.entries.push(entry.clone());
        }
    }
    let found: Vec<Value> = (all.search(&params.query).into_iter())
        .map(|entry| {
            let catalog = names.get(&source_key(entry)).map_or("", String::as_str);
            catalog_entry(catalog, entry)
        })
        .collect();
    let limit = params.limit.unwrap_or(SEARCH_DEFAULT).clamp(1, SEARCH_MAX);
    let what = format!("search|{}|{:?}|{limit}", params.query, params.registry);
    let page = page(&what, found, params.cursor.as_deref(), limit)?;
    let warnings: Vec<String> = (loaded.warnings.iter().take(20))
        .map(|w| clean(w, LONG))
        .collect();
    let mut object = Map::new();
    object.insert(
        "searched".into(),
        json!(
            (loaded.catalogs.iter())
                .map(|(n, _, _)| clean(n, LONG))
                .collect::<Vec<_>>()
        ),
    );
    if !warnings.is_empty() {
        object.insert(
            "untrusted".into(),
            json!({ "source": "loading the catalogs", "warnings": warnings }),
        );
    }
    Ok(page.into_object("results", object))
}

// ---------------------------------------------------------------------------------------------
// The project's packages

/// Where a locked package comes from, in untrusted words (a fetched package's dependencies name
/// their own sources)
fn locked_source(locked: &LockedPackage) -> Untrusted {
    Untrusted::new("the lock file")
        .maybe("git", locked.git.as_deref(), LONG)
        .maybe("tag", locked.tag.as_deref(), SHORT)
        .maybe("branch", locked.branch.as_deref(), SHORT)
        .maybe("path", locked.path.as_deref(), LONG)
        .maybe("registry", locked.registry.as_deref(), LONG)
}

/// `list_packages`: the project's locked packages
pub fn list_packages(context: &Context, params: ListParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let Some(lock) = project.lock() else {
        return Ok(json!({ "packages": [], "total": 0, "nextCursor": null,
            "note": "nothing installed yet" }));
    };
    let in_use: std::collections::BTreeSet<String> =
        project.in_use().iter().map(|p| p.name()).collect();
    let mut items = vec![];
    for locked in &lock.packages {
        let package = library.open(locked, project.dir()).ok();
        let info = package.as_ref().map(|p| p.info());
        let mut untrusted = locked_source(locked);
        if let Some(info) = info {
            let (abbreviation, aliases) = project.names(locked, info);
            untrusted = untrusted
                .text("title", &info.name, SHORT)
                .text("abbreviation", &abbreviation, SHORT)
                .list("aliases", &aliases)
                .maybe("license", info.license.as_deref(), SHORT);
        }
        items.push(json!({
            "name": locked.dependency_name(),
            "id": locked.id,
            "version": locked.version.to_string(),
            "kind": info.map(|i| i.kind),
            "language": info.map(|i| i.language.clone()),
            "enabled": in_use.contains(&locked.name()),
            "installed": library.is_installed(locked, project.dir()),
            "dependency": !lock.roots().any(|r| std::ptr::eq(r, locked)),
            "free": info.map(is_free),
            "commit": locked.commit,
            "untrusted": untrusted.value(),
        }));
    }
    let page = page("list", items, params.cursor.as_deref(), usize::MAX)?;
    Ok(page.into_object("packages", Map::new()))
}

/// `package_info`: one package's details
pub fn package_info(context: &Context, params: PackageParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let (locked, package) = find(&project, library, &params.package)?;
    let info = package.info().clone();
    let (abbreviation, aliases) = project.names(locked, &info);
    let concerns: Vec<String> = (logoxide::license::assess(&info).iter())
        .map(|c| clean(&c.message, LONG))
        .collect();
    let format = package.text_format().map(|(f, _)| format!("{f:?}"));
    let books: Vec<&str> = (package.books().iter())
        .filter_map(|b| bible_data().books().get_usfm(*b).map(String::as_str))
        .collect();
    let books: Vec<String> = books.into_iter().map(String::from).collect();
    let verses = match books.is_empty() {
        true => 0,
        false => {
            let cached =
                (library.book_cache().open(package, context.app.reader())).map_err(error)?;
            (cached.manifest().books.iter())
                .flat_map(|(_, c)| c.iter().map(|&n| n as usize))
                .sum()
        }
    };
    let mut untrusted = Untrusted::new(&manifest_source(&info))
        .text("title", &info.name, SHORT)
        .text("abbreviation", &abbreviation, SHORT)
        .list("aliases", &aliases)
        .maybe("description", info.description.as_deref(), LONG)
        .list("authors", &info.authors)
        .maybe("license", info.license.as_deref(), SHORT)
        .maybe("licenseFile", info.license_file.as_deref(), SHORT)
        .maybe("publisher", info.publisher.as_deref(), SHORT)
        .maybe("languageName", info.language_name.as_deref(), SHORT)
        .text("versification", &info.versification, SHORT)
        .maybe("homepage", info.homepage.as_deref(), LONG)
        .maybe("repository", info.repository.as_deref(), LONG)
        .maybe("documentation", info.documentation.as_deref(), LONG)
        .list("keywords", &info.keywords)
        .list("categories", &info.categories)
        .list("aboutBooks", &info.about_books)
        .maybe("work", logoxide::works::describe(&info).as_deref(), SHORT)
        .list("licenseConcerns", &concerns);
    if let Some(upstream) = &info.upstream {
        untrusted = untrusted
            .maybe("upstreamUrl", upstream.url.as_deref(), LONG)
            .maybe("upstreamImport", upstream.import.as_deref(), SHORT)
            .maybe("upstreamVersion", upstream.version.as_deref(), SHORT)
            .maybe("upstreamNote", upstream.note.as_deref(), LONG);
    }
    Ok(json!({
        "name": locked.dependency_name(),
        "id": info.id,
        "version": info.version.to_string(),
        "kind": info.kind,
        "language": info.language,
        "script": info.text_script(),
        "direction": info.text_direction(),
        "free": is_free(&info),
        "rights": info.text_rights(),
        "enabled": project.in_use().iter().any(|p| p.name() == locked.name()),
        "installed": library.is_installed(locked, project.dir()),
        "commit": locked.commit,
        "checksum": locked.checksum,
        "format": format,
        "books": books,
        "verses": verses,
        "lockSource": locked_source(locked).value(),
        "untrusted": untrusted.value(),
    }))
}

// ---------------------------------------------------------------------------------------------
// Passages

/// One of the project's Bibles by the name read or compare gave, opened through the cache
fn bible(context: &Context, project: &Project, name: &str) -> Result<CachedPackage, String> {
    let library = context.library();
    let bibles = project.bibles(library).map_err(error)?;
    let (_, package) = (bibles.into_iter())
        .find(|(locked, _)| locked.dependency_name() == name)
        .ok_or_else(|| format!("no Bible `{}`", clean(name, SHORT)))?;
    (library.book_cache().open(package, context.app.reader())).map_err(error)
}

/// `read_passage`: a passage from one of the project's Bibles, its text in pages
pub fn read_passage(context: &Context, params: ReadParams) -> Out {
    let project = context.project()?;
    let reference = match &params.bible {
        Some(bible) => format!("{} {bible}", params.reference),
        None => params.reference.clone(),
    };
    let read = (context
        .app
        .read(&project, &reference, &style(params.labels)))
    .map_err(error)?;
    let cached = bible(context, &project, &read.bible)?;
    let info = cached.package().info();
    let (verses, half) = verses_in(&counts(&cached), std::slice::from_ref(&read.passage));
    if let Some(why) = withheld(info, verses, half, MAX_VERSES) {
        return Err(why);
    }
    let what = format!("read|{reference}|{:?}", params.labels);
    let text = read.text.as_ref().map(|t| clean(&t.text, usize::MAX));
    let pieces = text
        .as_deref()
        .map(|t| chunks(t, PAGE_CHARS))
        .unwrap_or_default();
    let at = offset(&what, params.cursor.as_deref())?;
    if at > 0 && at >= pieces.len() {
        return Err(String::from("that cursor is past the end"));
    }
    let next = (at + 1 < pieces.len()).then(|| cursor(&what, at + 1));
    Ok(json!({
        "bible": read.bible,
        "reference": read.reference,
        "osis": read.osis,
        "versification": clean(&read.versification, SHORT),
        "verses": verses,
        "license": license(info),
        "untrusted": {
            "source": clean(&text_source(info), LONG),
            "abbreviation": clean(&read.abbreviation, SHORT),
            "text": pieces.get(at),
        },
        "part": at + 1,
        "parts": pieces.len(),
        "nextCursor": next,
        "note": match text { None => Some("the Bible has no text there"), Some(_) => None },
    }))
}

/// `compare_passage`: a passage in several of the project's Bibles, each in its own numbering
pub fn compare_passage(context: &Context, params: CompareParams) -> Out {
    let project = context.project()?;
    let compared = (context.app.compare(
        &project,
        &params.reference,
        &params.bibles,
        &style(params.labels),
    ))
    .map_err(error)?;
    let mut left = MAX_VERSES;
    let mut items = vec![];
    let mut licenses = Map::new();
    for c in &compared {
        let cached = bible(context, &project, &c.bible)?;
        let info = cached.package().info();
        licenses.insert(c.bible.clone(), license(info));
        let (verses, half) = verses_in(&counts(&cached), &c.passages);
        let refused = withheld(info, verses, half, left);
        if refused.is_none() && !is_free(info) {
            left = left.saturating_sub(verses);
        }
        let text = c.text.as_ref().map(|t| clean(&t.text, usize::MAX));
        let pieces = match (&refused, &text) {
            (None, Some(text)) => chunks(text, PAGE_CHARS),
            _ => vec![String::new()],
        };
        let parts = pieces.len();
        for (i, piece) in pieces.into_iter().enumerate() {
            items.push(json!({
                "bible": c.bible,
                "reference": c.reference,
                "versification": clean(&c.versification, SHORT),
                "mapped": c.mapped,
                "missing": c.missing,
                "verses": verses,
                "part": i + 1,
                "parts": parts,
                "withheld": refused,
                "untrusted": {
                    "source": clean(&text_source(info), LONG),
                    "abbreviation": clean(&c.abbreviation, SHORT),
                    "text": match (&refused, &text) {
                        (None, Some(_)) => Some(piece),
                        _ => None,
                    },
                },
            }));
        }
    }
    let what = format!(
        "compare|{}|{:?}|{:?}",
        params.reference, params.bibles, params.labels
    );
    let page = page(&what, items, params.cursor.as_deref(), usize::MAX)?;
    let mut object = Map::new();
    object.insert("licenses".into(), Value::Object(licenses));
    Ok(page.into_object("passages", object))
}

/// `map_reference`: where a reference is in another numbering
pub fn map_reference(context: &Context, params: MapParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let reader = context.app.reader();
    let from = (project.numbering(library, reader, &params.from)).map_err(error)?;
    let to = (project.numbering(library, reader, &params.to)).map_err(error)?;
    let reference =
        logoxide_text::versification::parse_reference(&params.reference).map_err(error)?;
    let maps = project.versifications(library).map_err(error)?;
    let counts = from.counts.get(&reference.book);
    let mapped =
        (maps.map(&reference, &from.versification, &to.versification, counts)).map_err(error)?;
    Ok(json!({
        "from": clean(&from.versification, SHORT),
        "to": clean(&to.versification, SHORT),
        "reference": mapped.text(),
        "missing": mapped.missing_text(),
        "note": mapped.verses.is_empty().then_some("no counterpart in that numbering"),
    }))
}

// ---------------------------------------------------------------------------------------------
// Summaries and versions

/// The installed package's summary: the published one, else built from its text
fn installed_summary(
    context: &Context,
    package: &Package,
) -> Result<(Summary, Vec<VerseHash>, bool), String> {
    match Summary::read(package.storage().as_ref()) {
        Ok((summary, verses)) => Ok((summary, verses, true)),
        Err(e @ LogoxideError::Newer(_)) => Err(error(e)),
        Err(_) => {
            let (summary, verses) =
                logoxide::summary::summarize(package, context.app.reader()).map_err(error)?;
            Ok((summary, verses, false))
        }
    }
}

/// A released version's summary, from the package's git repository
fn released_summary(
    context: &Context,
    locked: &LockedPackage,
    version: &str,
) -> Result<(Summary, Vec<VerseHash>), String> {
    let url = locked.git.as_deref().ok_or_else(|| {
        format!(
            "{} isn't from a git repository: only the installed version can be read",
            locked.id
        )
    })?;
    let version: semver::Version = (version.trim_start_matches('v').parse())
        .map_err(|_| format!("`{}` isn't a version (1.0.0)", clean(version, SHORT)))?;
    let prefix = locked
        .folder
        .as_deref()
        .map(|f| format!("{f}/"))
        .unwrap_or_default();
    let tag = logoxide::release::tag_for(&prefix, &version);
    logoxide::release::summary_in(
        context.app.fetcher(),
        url,
        &tag,
        &prefix,
        context.app.reader(),
    )
    .map_err(error)
}

/// `show_summary`: what a version holds
pub fn show_summary(context: &Context, params: SummaryParams) -> Out {
    let project = context.project()?;
    let (locked, package) = find(&project, context.library(), &params.package)?;
    let (summary, _, published) = match &params.version {
        Some(version) => {
            let (summary, verses) = released_summary(context, locked, version)?;
            (summary, verses, true)
        }
        None => installed_summary(context, &package)?,
    };
    let books: Vec<Value> = (summary.books.iter())
        .map(|(code, counts)| {
            json!({
                "book": clean(code, SHORT),
                "chapters": counts.len(),
                "verses": counts.iter().map(|&c| c as u64).sum::<u64>(),
            })
        })
        .collect();
    let titles: Vec<String> = (summary.titles.iter().take(200))
        .map(|(path, title)| format!("{path}: {title}"))
        .collect();
    Ok(json!({
        "id": summary.id,
        "version": summary.version.to_string(),
        "published": published,
        "verseHashRule": summary.verse_hash,
        "verses": summary.verses,
        "books": books,
        "sizes": summary.sizes,
        "contents": summary.contents,
        "media": summary.media.as_ref().map(|m| json!({ "files": m.files, "bytes": m.bytes })),
        "references": summary.references,
        "mappings": summary.mappings,
        "entries": summary.entries,
        "textChecksum": summary.text_checksum,
        "untrusted": Untrusted::new(&format!("the summary of {} {}", summary.id, summary.version))
            .list("titles", &titles)
            .value(),
    }))
}

/// `diff_versions`: what changed between two released versions, or one and the installed one
pub fn diff_versions(context: &Context, params: DiffParams) -> Out {
    let project = context.project()?;
    let (locked, package) = find(&project, context.library(), &params.package)?;
    let old = released_summary(context, locked, &params.from)?;
    let new = match &params.to {
        Some(to) => released_summary(context, locked, to)?,
        None => {
            let (summary, verses, _) = installed_summary(context, &package)?;
            (summary, verses)
        }
    };
    let d = logoxide::summary::diff_summaries((&old.0, &old.1), (&new.0, &new.1));
    let mut items = vec![];
    let mut add = |change: &str, verse: String, to: Option<String>| {
        items.push(json!({ "change": change, "verse": verse, "to": to }));
    };
    for v in &d.changed {
        add("reworded", v.reference(), None);
    }
    for (from, to) in &d.moved {
        add("moved", from.reference(), Some(to.reference()));
    }
    for v in &d.removed {
        add("removed", v.reference(), None);
    }
    for v in &d.added {
        add("added", v.reference(), None);
    }
    for (from, to) in &d.joined {
        add("joined", from.reference(), Some(to.reference()));
    }
    for v in &d.split {
        add("split", v.reference(), None);
    }
    for v in &d.bridged {
        add("bridged", v.reference(), None);
    }
    for (path, _, _) in &d.retitled {
        add("retitled", clean(path, SHORT), None);
    }
    let what = format!("diff|{}|{}|{:?}", params.package, params.from, params.to);
    let page = page(&what, items, params.cursor.as_deref(), usize::MAX)?;
    let mut object = Map::new();
    object.insert("from".into(), json!(old.0.version.to_string()));
    object.insert("to".into(), json!(new.0.version.to_string()));
    object.insert(
        "description".into(),
        json!(clean(&logoxide::release::describe(&d), LONG)),
    );
    object.insert(
        "change".into(),
        json!(format!("{:?}", d.change()).to_lowercase()),
    );
    object.insert(
        "entries".into(),
        json!({
            "removed": d.entries_removed.len(),
            "added": d.entries_added.len(),
            "reworded": d.entries_changed.len(),
        }),
    );
    object.insert("filesChanged".into(), json!(d.files_changed));
    Ok(page.into_object("changes", object))
}

/// `cited_references`: the passages a book package's EPUBs cite, and how often
pub fn cited_references(context: &Context, params: CitedParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let (_, package) = find(&project, library, &params.package)?;
    let found = library.book_cache().references(&package).map_err(error)?;
    let items: Vec<Value> = (logoxide::references::cited(&found).iter())
        .map(|c| json!({ "passage": clean(&c.passage, SHORT), "count": c.count }))
        .collect();
    let what = format!("cited|{}", params.package);
    let page = page(&what, items, params.cursor.as_deref(), usize::MAX)?;
    let mut object = Map::new();
    object.insert("package".into(), json!(package.info().id));
    object.insert("references".into(), json!(found.len()));
    Ok(page.into_object("cited", object))
}

// ---------------------------------------------------------------------------------------------
// Commentaries, dictionaries, and cross-references

/// A note's verses as a reference: `John 3:16`, `John 3:16-18`, `John 3:36-4:2`, `John 3:20a`
fn note_reference(book: &str, note: &logoxide_text::Note) -> String {
    let part = match note.part {
        0 => String::new(),
        n => char::from(b'a' + (n - 1).min(25)).to_string(),
    };
    let first = format!("{book} {}:{}{part}", note.chapter, note.verse);
    if note.last_chapter != note.chapter {
        format!("{first}-{}:{}", note.last_chapter, note.last)
    } else if note.last != note.verse {
        format!("{first}-{}", note.last)
    } else {
        first
    }
}

/// `read_notes`: what the project's commentaries say on a passage
pub fn read_notes(context: &Context, params: NotesParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let passage = passage(&params.reference)?;
    let name = (bible_data().books().get_name(passage.book).cloned()).unwrap_or_default();
    let commentaries = of_kind(
        &project,
        library,
        PackageKind::Commentary,
        params.commentary.as_deref(),
    )?;
    let mut left = MAX_VERSES;
    let mut items = vec![];
    let mut licenses = Map::new();
    for (locked, package) in commentaries {
        let cached = (library.book_cache().open(package, context.app.reader())).map_err(error)?;
        let info = cached.package().info();
        let (verses, half) = verses_in(&counts(&cached), std::slice::from_ref(&passage));
        let refused = withheld(info, verses, half, left);
        if refused.is_none() && !is_free(info) {
            left = left.saturating_sub(verses);
        }
        licenses.insert(locked.dependency_name().to_string(), license(info));
        if let Some(why) = refused {
            items.push(json!({ "commentary": locked.dependency_name(), "withheld": why }));
            continue;
        }
        if !cached.package().books().contains(&passage.book) {
            continue;
        }
        let source = cached.package().source();
        let book = source.book(passage.book).map_err(error)?;
        for note in logoxide_text::source::book_notes(&book, &passage, params.stretch) {
            items.push(json!({
                "commentary": locked.dependency_name(),
                "reference": note_reference(&name, &note),
                "chapter": note.chapter,
                "verse": note.verse,
                "last": note.last,
                "lastChapter": note.last_chapter,
                "part": note.part,
                "untrusted": Untrusted::new(&text_source(info))
                    .text("text", &tree_text(&note.nodes), TEXT)
                    .value(),
            }));
        }
    }
    let what = format!("notes|{}|{:?}", params.reference, params.commentary);
    let page = page(&what, items, params.cursor.as_deref(), usize::MAX)?;
    let mut object = Map::new();
    object.insert("licenses".into(), Value::Object(licenses));
    Ok(page.into_object("notes", object))
}

/// `lookup_entry`: a dictionary's entry by its key
pub fn lookup_entry(context: &Context, params: EntryParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let dictionaries = of_kind(
        &project,
        library,
        PackageKind::Lexicon,
        params.dictionary.as_deref(),
    )?;
    let mut items = vec![];
    let mut licenses = Map::new();
    for (locked, package) in dictionaries {
        let info = package.info();
        licenses.insert(locked.dependency_name().to_string(), license(info));
        let Some(entry) = package.source().entry(&params.key).map_err(error)? else {
            continue;
        };
        items.push(json!({
            "dictionary": locked.dependency_name(),
            "key": clean(&entry.key, SHORT),
            "untrusted": Untrusted::new(&text_source(info))
                .maybe("headword", entry.headword.as_deref(), SHORT)
                .text("text", &tree_text(&entry.nodes), TEXT)
                .value(),
        }));
    }
    let what = format!("entry|{}|{:?}", params.key, params.dictionary);
    let page = page(&what, items, params.cursor.as_deref(), usize::MAX)?;
    let mut object = Map::new();
    object.insert("licenses".into(), Value::Object(licenses));
    if page.total == 0 {
        object.insert(
            "note".into(),
            json!(format!("no entry `{}`", clean(&params.key, SHORT))),
        );
    }
    Ok(page.into_object("entries", object))
}

/// `cross_references`: the cross-reference packages' lines covering a passage
pub fn cross_references(context: &Context, params: CrossParams) -> Out {
    let project = context.project()?;
    let library = context.library();
    let passage = passage(&params.reference)?;
    let osis = (passage.to_osis(bible_data().books())).ok_or_else(|| {
        format!(
            "`{}` can't be written as an OSIS reference",
            clean(&params.reference, SHORT)
        )
    })?;
    let packages = of_kind(
        &project,
        library,
        PackageKind::CrossReferences,
        params.package.as_deref(),
    )?;
    let mut items = vec![];
    let mut licenses = Map::new();
    for (locked, package) in packages {
        licenses.insert(
            locked.dependency_name().to_string(),
            license(package.info()),
        );
        for x in package.source().cross_references(&osis).map_err(error)? {
            items.push(json!({
                "package": locked.dependency_name(),
                "from": clean(&x.from, SHORT),
                "to": clean(&x.to, SHORT),
                "weight": x.weight,
            }));
        }
    }
    let what = format!("xref|{}|{:?}", params.reference, params.package);
    let page = page(&what, items, params.cursor.as_deref(), usize::MAX)?;
    let mut object = Map::new();
    object.insert("osis".into(), json!(osis));
    object.insert("licenses".into(), Value::Object(licenses));
    Ok(page.into_object("crossReferences", object))
}

// ---------------------------------------------------------------------------------------------
// Writes (only with --allow-writes: the owner's D14)

fn writes(context: &Context) -> Result<(), String> {
    match context.allow_writes {
        true => Ok(()),
        false => Err(String::from(
            "logoxide-mcp is read-only: start it with --allow-writes to install or import",
        )),
    }
}

/// `install`: installs what the project's manifest asks for, as its lock says
pub fn install(context: &Context, params: InstallParams) -> Out {
    writes(context)?;
    let mut project = context.project()?;
    let frozen = Frozen {
        locked: params.locked,
        offline: false,
    };
    let mut warnings = vec![];
    let lock = (context
        .app
        .install(&mut project, Update::Nothing, frozen, &mut |w| {
            warnings.push(clean(&w, LONG))
        }))
    .map_err(error)?;
    let packages: Vec<Value> = (lock.packages.iter())
        .map(|p| json!({ "name": p.dependency_name(), "id": p.id, "version": p.version.to_string() }))
        .collect();
    Ok(json!({ "installed": packages, "warnings": warnings }))
}

/// `import`: makes a package from a publisher's download one of the project's catalogs lists,
/// and adds it as an `import` dependency, installed in the library. Only the project's catalogs
/// (D14): no URLs or paths
pub fn import(context: &Context, params: ImportParams) -> Out {
    writes(context)?;
    logoxide_app::import::check_source(&params.source).map_err(error)?;
    let mut project = context.project()?;
    let loaded = context.loaded(&project)?;
    let entry = (loaded.catalogs.iter())
        .filter(|(_, registry, _)| !registry)
        .find_map(|(_, _, c)| c.find_import(&params.source))
        .cloned()
        .ok_or_else(|| {
            format!(
                "{}: in none of this project's catalogs (search_catalogs)",
                clean(&params.source, SHORT)
            )
        })?;
    let importing = context.app.importing(&project);
    let imported = logoxide_app::import::run(&mut project, context.library(), &entry, &importing)
        .map_err(error)?;
    let skipped: Vec<String> = (imported.skipped.iter().take(50))
        .map(|s| s.to_string())
        .collect();
    Ok(json!({
        "id": imported.id,
        "version": imported.version,
        "dir": clean(&imported.dir.display().to_string(), LONG),
        "untrusted": Untrusted::new(&format!("the import {}", entry.id))
            .text("name", &imported.name, SHORT)
            .list("skipped", &skipped)
            .value(),
    }))
}
