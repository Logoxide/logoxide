/*!
A project: a folder with a `logoxide.toml` that asks for packages (a vault, or the library's own
for apps without one) and its `logoxide.lock`. Adding and removing dependencies edits the manifest
in place, keeping its comments and layout.
*/

use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, value};

use crate::{
    error::{LogoxideError, Result},
    git::Fetcher,
    library::Library,
    lock::{LOCKFILE, Lockfile},
    manifest::{Dependency, DetailedDependency, MANIFEST, Manifest, PackageKind},
    reader::TextReader,
    resolve::{Frozen, Resolver, Update},
};
use std::collections::BTreeMap;

use topos_bible::data::{books::BookId, chapter_verses::ChapterVerses, translations::Translations};

#[cfg(feature = "read")]
use logoxide_text::versification::{Mapped, Versifications};

/// What a Bible (or a versification named alone) numbers verses by: [`Project::numbering`]
#[derive(Clone, Debug)]
pub struct Numbering {
    /// The versification's name (`English`)
    pub versification: String,
    /// A Bible's own verse counts, by book (none for a versification named alone)
    pub counts: BTreeMap<BookId, ChapterVerses>,
}

/// Two or more of a project's Bibles answering to one name: [`Project::name_clashes`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameClash {
    /// The name, as the first of them spells it
    pub name: String,
    /// Each package answering to it (`kjv 1.0.0`, `kjv-bob (kjv) 1.0.0`), in the project's order
    pub packages: Vec<String>,
}

/**
The names more than one Bible answers to, ignoring case, from each Bible's label, abbreviation,
and aliases (as [`Project::names`] gives them): references naming one of them can't say which
*/
pub fn name_clashes(
    bibles: impl IntoIterator<Item = (String, String, Vec<String>)>,
) -> Vec<NameClash> {
    let mut clashes: Vec<NameClash> = vec![];
    for (label, abbreviation, aliases) in bibles {
        let mut own: Vec<String> = vec![];
        for name in std::iter::once(abbreviation).chain(aliases) {
            if name.is_empty() || own.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
                continue;
            }
            own.push(name.clone());
            match clashes
                .iter_mut()
                .find(|c| c.name.eq_ignore_ascii_case(&name))
            {
                Some(clash) => clash.packages.push(label.clone()),
                None => clashes.push(NameClash {
                    name,
                    packages: vec![label.clone()],
                }),
            }
        }
    }
    clashes.retain(|c| c.packages.len() > 1);
    clashes
}

/// Two or more of a project's Bibles that are one work: [`Project::same_works`]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SameWork {
    /// The work's id (`kjv`)
    pub work: String,
    /// Each package that is it (`kjv 1.0.0`, `ebible-eng-kjv2006 (path …) 1.0.0`), in the
    /// project's order
    pub packages: Vec<String>,
}

/**
The works more than one Bible is, from each Bible's label, package id, and `work`. A Bible that
doesn't say its work takes the work of another with its package id (a fork named with `package =
"kjv"` is the work its original says)
*/
pub fn same_works(
    bibles: impl IntoIterator<Item = (String, String, Option<String>)>,
) -> Vec<SameWork> {
    let bibles: Vec<(String, String, Option<String>)> = bibles.into_iter().collect();
    let said: BTreeMap<&str, &str> = (bibles.iter())
        .filter_map(|(_, id, work)| Some((id.as_str(), work.as_deref()?)))
        .collect();
    let mut works: Vec<SameWork> = vec![];
    for (label, id, work) in &bibles {
        let Some(work) = work.as_deref().or_else(|| said.get(id.as_str()).copied()) else {
            continue;
        };
        match works.iter_mut().find(|w| w.work == work) {
            Some(same) => same.packages.push(label.clone()),
            None => works.push(SameWork {
                work: work.to_string(),
                packages: vec![label.clone()],
            }),
        }
    }
    works.retain(|w| w.packages.len() > 1);
    works
}

/// A new project's `logoxide.toml`
const NEW_MANIFEST: &str = "# The Bibles this uses: `lx add --git URL`\n[dependencies]\n";

#[derive(Clone)]
pub struct Project {
    dir: PathBuf,
    doc: DocumentMut,
    manifest: Manifest,
    lock: Option<Lockfile>,
    /// `logoxide.toml`'s text as it was read (or last written), so a change made since by
    /// someone else is seen before it's written over
    manifest_on_disk: Option<String>,
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(LogoxideError::io(&path.display().to_string(), e)),
    }
}

impl Project {
    /// The project in `dir` (an empty one if it has no `logoxide.toml` yet)
    pub fn open(dir: impl Into<PathBuf>) -> Result<Project> {
        let dir = dir.into();
        let text = read_optional(&dir.join(MANIFEST))?;
        let doc: DocumentMut = (text.as_deref().unwrap_or_default())
            .parse()
            .map_err(|e: toml_edit::TomlError| LogoxideError::Manifest(e.to_string()))?;
        let manifest = Manifest::parse(text.as_deref().unwrap_or_default())?;
        let lock = read_optional(&dir.join(LOCKFILE))?
            .map(|t| Lockfile::parse(&t))
            .transpose()?;
        Ok(Project {
            dir,
            doc,
            manifest,
            lock,
            manifest_on_disk: text,
        })
    }

    /// A project from its files' text, for hosts that read them (an empty manifest if it has
    /// none yet); its path packages are relative to `dir`, and nothing is written
    pub fn from_text(
        dir: impl Into<PathBuf>,
        manifest: Option<&str>,
        lock: Option<&str>,
    ) -> Result<Project> {
        let text = manifest.unwrap_or(NEW_MANIFEST);
        let doc: DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| LogoxideError::Manifest(e.to_string()))?;
        Ok(Project {
            dir: dir.into(),
            doc,
            manifest: Manifest::parse(text)?,
            lock: lock.map(Lockfile::parse).transpose()?,
            manifest_on_disk: manifest.map(String::from),
        })
    }

    /// `logoxide.toml`'s text, with any dependencies added or removed
    pub fn manifest_text(&self) -> String {
        self.doc.to_string()
    }

    /// Starts a project in `dir`: an empty `logoxide.toml` (an error if it has one)
    pub fn init(dir: impl Into<PathBuf>) -> Result<Project> {
        let dir = dir.into();
        let path = dir.join(MANIFEST);
        if path.exists() {
            return Err(LogoxideError::Manifest(format!(
                "{} already exists",
                path.display()
            )));
        }
        let mut project = Project {
            dir,
            doc: NEW_MANIFEST
                .parse()
                .map_err(|e: toml_edit::TomlError| LogoxideError::Manifest(e.to_string()))?,
            manifest: Manifest::default(),
            lock: None,
            manifest_on_disk: None,
        };
        project.save_manifest()?;
        Ok(project)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn lock(&self) -> Option<&Lockfile> {
        self.lock.as_ref()
    }

    /// Adds (or replaces) a dependency in `[dependencies]`
    pub fn add(&mut self, name: &str, dependency: &DetailedDependency) -> Result<()> {
        let mut table = InlineTable::new();
        let mut put = |key: &str, v: Option<&str>| {
            if let Some(v) = v {
                table.insert(key, v.into());
            }
        };
        put("git", dependency.git.as_deref());
        put("registry", dependency.registry.as_deref());
        put("path", dependency.path.as_deref());
        // `1.2.0` like Cargo writes it (the same requirement as `^1.2.0`)
        let version = (dependency.version.as_ref())
            .map(|v| v.to_string().trim_start_matches('^').to_string());
        put("version", version.as_deref());
        put("tag", dependency.tag.as_deref());
        put("branch", dependency.branch.as_deref());
        put("rev", dependency.rev.as_deref());
        put("folder", dependency.folder.as_deref());
        if dependency.optional {
            table.insert("optional", true.into());
        }
        if !dependency.features.is_empty() {
            let features: Array = dependency.features.iter().map(String::as_str).collect();
            table.insert("features", features.into());
        }
        if !dependency.default_features {
            table.insert("default-features", false.into());
        }
        if let Some(package) = &dependency.package {
            table.insert("package", package.as_str().into());
        }
        if let Some(abbreviation) = &dependency.abbreviation {
            table.insert("abbreviation", abbreviation.as_str().into());
        }
        if let Some(aliases) = &dependency.aliases {
            let aliases: Array = aliases.iter().map(String::as_str).collect();
            table.insert("aliases", aliases.into());
        }
        let dependencies = self
            .doc
            .entry("dependencies")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| LogoxideError::Manifest(String::from("[dependencies] isn't a table")))?;
        dependencies.insert(name, value(table));
        self.reparse()
    }

    /// Whether a dependency is on (any name the manifest doesn't have is)
    pub fn is_enabled(&self, name: &str) -> bool {
        self.manifest
            .dependencies
            .get(name)
            .is_none_or(Dependency::enabled)
    }

    /**
    Turns a dependency on or off (`enabled = false`, kept in the manifest and the lock, and
    installed, but not read); whether the manifest has it. Turning it on takes the key out
    */
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<bool> {
        let Some(item) = (self.doc.get_mut("dependencies"))
            .and_then(Item::as_table_like_mut)
            .and_then(|t| t.get_mut(name))
        else {
            return Ok(false);
        };
        let table = item
            .as_table_like_mut()
            .ok_or_else(|| LogoxideError::Manifest(format!("dependency `{name}` isn't a table")))?;
        if enabled {
            table.remove("enabled");
        } else {
            table.insert("enabled", value(false));
        }
        // `{ path = "bsb", enabled = false }`, not `{ path = "bsb" , enabled = false }`
        if let Some(inline) = item.as_inline_table_mut() {
            inline.fmt();
        }
        self.reparse()?;
        Ok(true)
    }

    /// What the manifest calls the dependencies that are on (`enabled`) or off, by name
    pub fn dependencies_enabled(&self, enabled: bool) -> Vec<String> {
        (self.manifest.dependencies.iter())
            .filter(|(_, d)| d.enabled() == enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /**
    The lock's packages in use: those the manifest asks for and hasn't turned off, and what
    each brings in (a package another one in use brings in is in use, even if it's off itself)
    */
    pub fn in_use(&self) -> Vec<&crate::LockedPackage> {
        let Some(lock) = &self.lock else {
            return vec![];
        };
        let mut todo: Vec<&crate::LockedPackage> = (lock.roots())
            .filter(|p| self.is_enabled(p.dependency_name()))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        let mut used = vec![];
        while let Some(locked) = todo.pop() {
            if !seen.insert(locked.name()) {
                continue;
            }
            todo.extend((locked.dependencies.iter()).filter_map(|name| lock.named(name)));
            used.push(locked);
        }
        used
    }

    /// Removes a dependency; whether it was there
    pub fn remove(&mut self, name: &str) -> Result<bool> {
        let removed = self
            .doc
            .get_mut("dependencies")
            .and_then(Item::as_table_mut)
            .and_then(|t| t.remove(name))
            .is_some();
        self.reparse()?;
        Ok(removed)
    }

    /// The `[settings]` table, made if it's missing
    fn settings_table(&mut self) -> Result<&mut Table> {
        self.doc
            .entry("settings")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| LogoxideError::Manifest(String::from("[settings] isn't a table")))
    }

    /**
    Adds a catalog to search (a URL or a path relative to the project) to `[settings] catalogs`;
    whether it wasn't searched already. A default one ([`crate::catalog::DEFAULT_CATALOGS`]) is
    only written when the defaults are turned off
    */
    pub fn add_catalog(&mut self, location: &str) -> Result<bool> {
        if self.manifest.catalogs().iter().any(|c| c == location) {
            return Ok(false);
        }
        let settings = self.settings_table()?;
        let catalogs = settings
            .entry("catalogs")
            .or_insert(value(Array::new()))
            .as_array_mut()
            .ok_or_else(|| LogoxideError::Manifest(String::from("catalogs isn't a list")))?;
        catalogs.push(location);
        // Every default named again: back to `default-catalogs = true`, unwritten
        let defaults = crate::catalog::DEFAULT_CATALOGS;
        let named: Vec<String> = (catalogs.iter())
            .filter_map(|c| c.as_str().map(String::from))
            .collect();
        if defaults.iter().all(|d| named.iter().any(|c| c == d)) {
            catalogs.retain(|c| c.as_str().is_none_or(|c| !defaults.contains(&c)));
            if catalogs.is_empty() {
                settings.remove("catalogs");
            }
            settings.remove("default-catalogs");
        }
        self.drop_empty_settings();
        self.reparse()?;
        Ok(true)
    }

    /**
    Stops searching a catalog; whether it was searched. Removing a default one turns the defaults
    off (`default-catalogs = false`) and names the others instead
    */
    pub fn remove_catalog(&mut self, location: &str) -> Result<bool> {
        if !self.manifest.catalogs().iter().any(|c| c == location) {
            return Ok(false);
        }
        let named = (self.manifest.settings.as_ref())
            .map(|s| s.catalogs.clone())
            .unwrap_or_default();
        let defaults = crate::catalog::DEFAULT_CATALOGS;
        let mut kept: Vec<String> = named.into_iter().filter(|c| c != location).collect();
        let settings = self.settings_table()?;
        if defaults.contains(&location) {
            for other in defaults.into_iter().filter(|d| *d != location) {
                if !kept.iter().any(|c| c == other) {
                    kept.push(other.to_string());
                }
            }
            settings.insert("default-catalogs", value(false));
        }
        if kept.is_empty() {
            settings.remove("catalogs");
        } else {
            settings.insert("catalogs", value(kept.iter().collect::<Array>()));
        }
        self.drop_empty_settings();
        self.reparse()?;
        Ok(true)
    }

    /// Drops `[settings]` once nothing is left in it
    /**
    Names a registry (`[settings.registries]`): a folder of package repositories or a catalog,
    so `name:<id or path>` names its packages. Replaces one of that name; whether it changed
    */
    pub fn add_registry(&mut self, name: &str, location: &str) -> Result<bool> {
        crate::catalog::check_registry_name(name)?;
        if self.manifest.registries().get(name).map(String::as_str) == Some(location) {
            return Ok(false);
        }
        let settings = self.settings_table()?;
        let registries = settings
            .entry("registries")
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .ok_or_else(|| LogoxideError::Manifest(String::from("registries isn't a table")))?;
        registries.insert(name, value(location));
        // `[settings.registries]` alone needs no `[settings]` header
        settings.set_implicit(true);
        self.reparse()?;
        Ok(true)
    }

    /// Stops naming a registry; whether there was one of that name
    pub fn remove_registry(&mut self, name: &str) -> Result<bool> {
        if !self.manifest.registries().contains_key(name) {
            return Ok(false);
        }
        let settings = self.settings_table()?;
        if let Some(registries) = settings
            .get_mut("registries")
            .and_then(Item::as_table_like_mut)
        {
            registries.remove(name);
            if registries.is_empty() {
                settings.remove("registries");
            }
        }
        self.drop_empty_settings();
        self.reparse()?;
        Ok(true)
    }

    fn drop_empty_settings(&mut self) {
        if (self.doc.get("settings").and_then(Item::as_table)).is_some_and(Table::is_empty) {
            self.doc.remove("settings");
        }
    }

    fn reparse(&mut self) -> Result<()> {
        self.manifest = Manifest::parse(&self.doc.to_string())?;
        Ok(())
    }

    /**
    Writes `logoxide.toml` (whole, or not at all: [`crate::storage::write_atomic`]). If the file
    changed since this project read it (another lx, an editor), nothing is written and the error
    is [`LogoxideError::Conflict`]: open the project again and redo the change
    */
    pub fn save_manifest(&mut self) -> Result<()> {
        let _lock = self.take_lock()?;
        let path = self.dir.join(MANIFEST);
        if read_optional(&path)? != self.manifest_on_disk {
            return Err(LogoxideError::Conflict {
                path: path.display().to_string(),
            });
        }
        let text = self.doc.to_string();
        crate::storage::write_atomic(&path, text.as_bytes())
            .map_err(|e| LogoxideError::io(&path.display().to_string(), e))?;
        self.manifest_on_disk = Some(text);
        Ok(())
    }

    /// Writes `logoxide.lock`, whole or not at all
    fn save_lock(&self, lock: &Lockfile) -> Result<()> {
        let path = self.dir.join(LOCKFILE);
        crate::storage::write_atomic(&path, lock.to_toml()?.as_bytes())
            .map_err(|e| LogoxideError::io(&path.display().to_string(), e))
    }

    /**
    Resolves the manifest (keeping the lock file's versions unless `update` says otherwise),
    installs every package into the library, and writes `logoxide.lock`
    */
    pub fn install(
        &mut self,
        library: &Library,
        fetcher: &dyn Fetcher,
        update: Update,
    ) -> Result<&Lockfile> {
        self.install_with(library, fetcher, update, false, Frozen::default())
    }

    /**
    [`Project::install`] keeping what's locked (Cargo's `--locked`: the lock file must come out
    as it was) and/or fetching nothing (`--offline`: everything must be installed already)
    */
    pub fn install_frozen(
        &mut self,
        library: &Library,
        fetcher: &dyn Fetcher,
        frozen: Frozen,
    ) -> Result<&Lockfile> {
        self.install_with(library, fetcher, Update::Nothing, false, frozen)
    }

    /// [`Project::install`], putting back the files of installed versions that changed (a
    /// reader edited them) instead of stopping there; files the packages don't declare stay
    pub fn repair(&mut self, library: &Library, fetcher: &dyn Fetcher) -> Result<&Lockfile> {
        self.install_with(library, fetcher, Update::Nothing, true, Frozen::default())
    }

    /// The project's lock ([`crate::locking`]), held while its files are written
    pub fn take_lock(&self) -> Result<crate::locking::Guard> {
        crate::locking::lock(&self.dir, "the project at")
    }

    fn install_with(
        &mut self,
        library: &Library,
        fetcher: &dyn Fetcher,
        update: Update,
        repair: bool,
        frozen: Frozen,
    ) -> Result<&Lockfile> {
        // The library's lock and the project's, from resolving through writing the lock file
        let _locks = crate::locking::lock_all(&[
            (library.root(), "the library at"),
            (&self.dir, "the project at"),
        ])?;
        let packages = library.packages(&self.dir);
        let resolver = Resolver {
            packages: &packages,
            fetcher,
            previous: self.lock.as_ref(),
            update,
            repair,
            frozen,
        };
        let lock = resolver.resolve(&self.manifest)?;
        self.save_lock(&lock)?;
        // This project uses the library: what it locks is kept by `lx clean` run from elsewhere
        library.record_root(&self.dir, &lock)?;
        Ok(self.lock.insert(lock))
    }

    /// The Bibles the manifest asks for, and those in its collections, each opened
    pub fn bibles(
        &self,
        library: &Library,
    ) -> Result<Vec<(&crate::LockedPackage, crate::Package)>> {
        let mut bibles = vec![];
        let Some(lock) = &self.lock else {
            return Ok(bibles);
        };
        // The manifest's packages that are on, and what each collection among them brings in
        let mut todo: Vec<&crate::LockedPackage> = (lock.roots())
            .filter(|p| self.is_enabled(p.dependency_name()))
            .collect();
        todo.reverse();
        let mut seen = std::collections::BTreeSet::new();
        while let Some(locked) = todo.pop() {
            if !seen.insert(locked.name()) {
                continue;
            }
            let package = library.open(locked, &self.dir)?;
            if package.info().kind == PackageKind::Collection {
                let inside = (locked.dependencies.iter()).filter_map(|name| lock.named(name));
                todo.extend(inside.rev());
                continue;
            }
            // A recording without a text of its own (`[media] text`) has no verses to give, nor
            // has a dictionary
            let verses = (package.text_format()).is_some_and(|(format, _)| format.is_verse_text());
            if package.info().kind == PackageKind::Bible && verses {
                bibles.push((locked, package));
            }
        }
        Ok(bibles)
    }

    /**
    The packages of a kind in use (the lock's packages the manifest asks for and hasn't turned
    off, and what each brings in: [`Project::in_use`]), each opened, in the lock's order
    */
    pub fn packages_of(
        &self,
        library: &Library,
        kind: PackageKind,
    ) -> Result<Vec<(&crate::LockedPackage, crate::Package)>> {
        let mut found = vec![];
        for locked in self.in_use() {
            let package = library.open(locked, &self.dir)?;
            if package.info().kind == kind {
                found.push((locked, package));
            }
        }
        Ok(found)
    }

    /// The commentaries in use ([`Project::packages_of`])
    pub fn commentaries(
        &self,
        library: &Library,
    ) -> Result<Vec<(&crate::LockedPackage, crate::Package)>> {
        self.packages_of(library, PackageKind::Commentary)
    }

    /// The dictionaries in use (`kind = "lexicon"`; [`Project::packages_of`])
    pub fn dictionaries(
        &self,
        library: &Library,
    ) -> Result<Vec<(&crate::LockedPackage, crate::Package)>> {
        self.packages_of(library, PackageKind::Lexicon)
    }

    /// The cross-reference packages in use ([`Project::packages_of`])
    pub fn cross_references(
        &self,
        library: &Library,
    ) -> Result<Vec<(&crate::LockedPackage, crate::Package)>> {
        self.packages_of(library, PackageKind::CrossReferences)
    }

    /**
    The translations the manifest asks for (its Bibles, and those in its collections; not other
    dependencies), for topos: each one's abbreviation, aliases, and verse counts, from the
    library's cache (read by `reader` when the cache lacks them)
    */
    pub fn translations(&self, library: &Library, reader: &dyn TextReader) -> Result<Translations> {
        let mut translations = Translations::default();
        let cache = library.book_cache();
        for (locked, package) in self.bibles(library)? {
            let (abbreviation, aliases) = self.names(locked, package.info());
            translations =
                cache
                    .open(package, reader)?
                    .add_as(translations, &abbreviation, &aliases);
        }
        Ok(translations)
    }

    /**
    The names two or more of the project's Bibles (those [`Project::translations`] gives topos)
    answer to: a package the manifest asks for at one version and a collection at another, or
    two packages sharing an abbreviation. References naming one can't say which
    */
    pub fn name_clashes(&self, library: &Library) -> Result<Vec<NameClash>> {
        let bibles = self.bibles(library)?;
        Ok(name_clashes(bibles.iter().map(|(locked, package)| {
            let (abbreviation, aliases) = self.names(locked, package.info());
            (locked.label(), abbreviation, aliases)
        })))
    }

    /**
    The works two or more of the project's Bibles (those [`Project::translations`] gives topos)
    are: two editions of the KJV, two forks of one package, or a package the manifest asks for
    at one version and a collection at another. Not wrong, but worth saying
    */
    pub fn same_works(&self, library: &Library) -> Result<Vec<SameWork>> {
        let bibles = self.bibles(library)?;
        Ok(same_works(bibles.iter().map(|(locked, package)| {
            let info = package.info();
            (locked.label(), info.id.clone(), info.work.clone())
        })))
    }

    /**
    The text a recording with no text of its own reads (`[media] text`, a dependency of this
    project's package), as installed: none when it names none. One that isn't locked or
    installed is an error saying to install it
    */
    pub fn media_text(&self, library: &Library) -> Result<Option<crate::Package>> {
        let Some(name) = (self.manifest.media.as_ref()).and_then(|m| m.text.as_deref()) else {
            return Ok(None);
        };
        let locked = (self.lock.as_ref())
            .and_then(|lock| lock.get(name))
            .ok_or_else(|| {
                LogoxideError::NotFound(format!(
                    "{name}, the text [media] names, isn't locked (run `lx install` in {})",
                    self.dir.display()
                ))
            })?;
        library.open(locked, &self.dir).map(Some)
    }

    /// The versification maps in use (any package of the lock in use, so a Bible's own
    /// dependency on a map counts too; see [`Project::in_use`])
    #[cfg(feature = "read")]
    pub fn versifications(&self, library: &Library) -> Result<Versifications> {
        let mut maps = Versifications::default();
        for locked in self.in_use() {
            let package = library.open(locked, &self.dir)?;
            if package.info().kind == PackageKind::VersificationMap {
                maps.add(crate::reader::versification_map(&package)?);
            }
        }
        Ok(maps)
    }

    /**
    What a name numbers verses by, for mapping: one of the project's Bibles (what the manifest
    calls it, its abbreviation, or an alias, ignoring case) is its `versification`, with its own
    verse counts; any other name is a versification's (`English`). A Bible numbered its own
    way (`versification = "own"`) is an error: it says nothing a map can use
    */
    #[cfg(feature = "read")]
    pub fn numbering(
        &self,
        library: &Library,
        reader: &dyn TextReader,
        name: &str,
    ) -> Result<Numbering> {
        for (locked, package) in self.bibles(library)? {
            let (abbreviation, aliases) = self.names(locked, package.info());
            let names = [locked.dependency_name(), abbreviation.as_str()];
            let is = (names.into_iter().chain(aliases.iter().map(String::as_str)))
                .any(|n| n.eq_ignore_ascii_case(name));
            if !is {
                continue;
            }
            let versification = package.info().versification.clone();
            if versification == "own" {
                return Err(LogoxideError::Versification(format!(
                    "{name} names no versification (`versification = \"own\"`): give its \
                     [package] one (`versification = \"English\"`), or name a versification"
                )));
            }
            let cached = library.book_cache().open(package, reader)?;
            let counts = (cached.manifest().books.iter())
                .map(|(book, counts)| (*book, ChapterVerses::new(counts.clone())))
                .collect();
            return Ok(Numbering {
                versification,
                counts,
            });
        }
        Ok(Numbering {
            versification: name.to_string(),
            counts: BTreeMap::new(),
        })
    }

    /// Where a reference numbered by one Bible or versification is in another's numbering
    /// (see [`Project::numbering`]), through the project's maps
    #[cfg(feature = "read")]
    pub fn map_reference(
        &self,
        library: &Library,
        reader: &dyn TextReader,
        reference: &str,
        from: &str,
        to: &str,
    ) -> Result<Mapped> {
        let passage = logoxide_text::versification::parse_reference(reference)?;
        let (from, to) = (
            self.numbering(library, reader, from)?,
            self.numbering(library, reader, to)?,
        );
        let counts = from.counts.get(&passage.book);
        Ok((self.versifications(library)?).map(
            &passage,
            &from.versification,
            &to.versification,
            counts,
        )?)
    }

    /**
    What references name a locked package by here: the `abbreviation` and `aliases` the manifest
    gives the dependency, else the package's own. Only the manifest's own copy takes them: the
    same package at another version, brought in by a collection, keeps its own names
    */
    pub fn names(
        &self,
        locked: &crate::LockedPackage,
        info: &crate::manifest::Package,
    ) -> (String, Vec<String>) {
        let root = (self.lock.iter())
            .flat_map(|lock| lock.roots())
            .any(|r| r.name() == locked.name());
        let detailed = match self.manifest.dependencies.get(locked.dependency_name()) {
            Some(Dependency::Detailed(d)) if root => Some(d),
            _ => None,
        };
        let abbreviation = (detailed.and_then(|d| d.abbreviation.clone()))
            .unwrap_or_else(|| info.abbreviation.clone());
        let aliases =
            (detailed.and_then(|d| d.aliases.clone())).unwrap_or_else(|| info.aliases.clone());
        (abbreviation, aliases)
    }

    /**
    Lists a media file in `[media]` (a package's `logoxide.toml`: `lx media add`), as a
    `[[media.files]]` table; one already listed under its path is replaced. The manifest must still
    read afterwards
    */
    pub fn add_media(&mut self, entry: &crate::media::MediaFile) -> Result<()> {
        let media = self
            .doc
            .entry("media")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| LogoxideError::Manifest(String::from("[media] isn't a table")))?;
        let files = media
            .entry("files")
            .or_insert(Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or_else(|| LogoxideError::Manifest(String::from("media.files isn't a list")))?;
        // Listed again: replaced
        let mut kept = toml_edit::ArrayOfTables::new();
        for table in files.iter() {
            if table.get("path").and_then(|p| p.as_str()) != Some(&entry.path) {
                kept.push(table.clone());
            }
        }
        let mut table = Table::new();
        table.insert("path", value(&entry.path));
        table.insert("url", value(&entry.url));
        if !entry.mirrors.is_empty() {
            let mirrors: Array = entry.mirrors.iter().collect();
            table.insert("mirrors", value(mirrors));
        }
        table.insert("size", value(entry.size as i64));
        table.insert("sha256", value(&entry.sha256));
        if let Some(book) = &entry.book {
            table.insert("book", value(book));
        }
        kept.push(table);
        *files = kept;
        self.reparse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        CCEL_CATALOG, DEFAULT_CATALOGS, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG, SWORD_CATALOG,
    };

    #[test]
    fn a_manifest_changed_since_it_was_read_isnt_written_over() {
        let dir = std::env::temp_dir().join(format!("logoxide-conflict-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut project = Project::init(&dir).unwrap();
        let mut other = Project::open(&dir).unwrap();
        project
            .add_catalog("https://example.org/catalog.toml")
            .unwrap();
        project.save_manifest().unwrap();
        other.add_catalog("https://example.org/other.toml").unwrap();
        assert!(matches!(
            other.save_manifest(),
            Err(LogoxideError::Conflict { .. })
        ));
        assert!(
            std::fs::read_to_string(dir.join(MANIFEST))
                .unwrap()
                .contains("catalog.toml")
        );
        // Saved again by the one that wrote it: no conflict with itself
        project
            .add_catalog("https://example.org/third.toml")
            .unwrap();
        project.save_manifest().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dependencies_are_turned_off_and_on() {
        let manifest = r#"# my vault
[dependencies]
kjv = { git = "https://example.org/kjv", version = "1" } # the old one
bsb = { path = "bsb" }

[dependencies.library]
path = "library"
"#;
        let lock = r#"version = 1
dependencies = ["kjv 1.0.0", "bsb 1.0.0", "library 1.0.0"]

[[package]]
id = "kjv"
version = "1.0.0"
path = "kjv"

[[package]]
id = "bsb"
version = "1.0.0"
path = "bsb"

[[package]]
id = "library"
version = "1.0.0"
path = "library"
dependencies = ["gill 1.0.0"]

[[package]]
id = "gill"
version = "1.0.0"
path = "gill"
"#;
        let mut project = Project::from_text("", Some(manifest), Some(lock)).unwrap();
        let in_use = |p: &Project| -> Vec<String> {
            let mut ids: Vec<String> = p.in_use().iter().map(|l| l.id.clone()).collect();
            ids.sort();
            ids
        };
        assert_eq!(in_use(&project), ["bsb", "gill", "kjv", "library"]);
        // Off: kept in the manifest, with its comment, whatever form it was written in
        for name in ["kjv", "bsb", "library"] {
            assert!(project.set_enabled(name, false).unwrap());
            assert!(!project.is_enabled(name));
        }
        assert!(!project.set_enabled("nope", false).unwrap());
        let text = project.manifest_text();
        assert!(text.contains("# my vault"), "{text}");
        assert!(
            text.contains(r#"version = "1", enabled = false } # the old one"#),
            "{text}"
        );
        assert!(
            text.contains(r#"bsb = { path = "bsb", enabled = false }"#),
            "{text}"
        );
        assert!(
            text.contains("path = \"library\"\nenabled = false"),
            "{text}"
        );
        // What a package that's off brings in is off too
        assert!(in_use(&project).is_empty());
        assert!(project.dependencies_enabled(true).is_empty());
        assert_eq!(
            project.dependencies_enabled(false),
            ["bsb", "kjv", "library"]
        );
        // And on again: the key goes
        project.set_enabled("library", true).unwrap();
        project.set_enabled("kjv", true).unwrap();
        assert_eq!(in_use(&project), ["gill", "kjv", "library"]);
        assert_eq!(project.dependencies_enabled(true), ["kjv", "library"]);
        assert_eq!(project.dependencies_enabled(false), ["bsb"]);
        let text = project.manifest_text();
        assert!(text.contains(r#"version = "1" } # the old one"#), "{text}");
        assert!(!text.contains("path = \"library\"\nenabled"), "{text}");
        assert_eq!(project.manifest().dependencies.len(), 3);
    }

    #[test]
    fn registries_are_named_and_removed() {
        let mut project = Project::from_text("", Some("# mine\n[dependencies]\n"), None).unwrap();
        assert!(project.add_registry("dorean-local", "~/repos").unwrap());
        assert!(!project.add_registry("dorean-local", "~/repos").unwrap());
        assert!(project.add_registry("github", "~/repos").is_err());
        let text = project.manifest_text();
        assert!(
            text.contains("[settings.registries]\ndorean-local = \"~/repos\""),
            "{text}"
        );
        assert_eq!(project.manifest().registries()["dorean-local"], "~/repos");
        assert!(project.remove_registry("dorean-local").unwrap());
        assert!(!project.remove_registry("dorean-local").unwrap());
        assert_eq!(project.manifest_text(), "# mine\n[dependencies]\n");
    }

    #[test]
    fn catalogs_are_added_and_removed_keeping_comments() {
        let text = "# My vault\n[dependencies]\n\n[settings]\n# Mine\ndefault = \"bsb\"\n";
        let mut project = Project::from_text("", Some(text), None).unwrap();
        assert!(
            project
                .add_catalog("https://example.org/catalog.toml")
                .unwrap()
        );
        assert!(!project.add_catalog(EBIBLE_CATALOG).unwrap());
        assert!(
            project
                .manifest_text()
                .contains("# Mine\ndefault = \"bsb\"")
        );
        assert_eq!(
            project.manifest().catalogs(),
            [
                "https://example.org/catalog.toml",
                EBIBLE_CATALOG,
                FETCH_BIBLE_CATALOG,
                CCEL_CATALOG,
                SWORD_CATALOG
            ]
        );

        // Removing a default keeps the others, by name
        assert!(project.remove_catalog(EBIBLE_CATALOG).unwrap());
        assert_eq!(
            project.manifest().catalogs(),
            [
                "https://example.org/catalog.toml",
                FETCH_BIBLE_CATALOG,
                CCEL_CATALOG,
                SWORD_CATALOG
            ]
        );
        assert!(project.manifest_text().contains("default-catalogs = false"));
        // Added back: the defaults again, and nothing about them written
        assert!(project.add_catalog(EBIBLE_CATALOG).unwrap());
        assert!(!project.manifest_text().contains("default-catalogs"));
        assert!(!project.manifest_text().contains(FETCH_BIBLE_CATALOG));
        let mine = "https://example.org/catalog.toml";
        assert!(project.remove_catalog(mine).unwrap());
        assert!(!project.remove_catalog(mine).unwrap());
        assert_eq!(project.manifest().catalogs(), DEFAULT_CATALOGS);
        assert!(!project.manifest_text().contains("catalogs"));
        assert!(
            project
                .manifest_text()
                .contains("[settings]\n# Mine\ndefault")
        );

        // A manifest without [settings]
        let mut new = Project::from_text("", None, None).unwrap();
        new.add_catalog("catalog.toml").unwrap();
        assert!(
            new.manifest_text()
                .contains("[settings]\ncatalogs = [\"catalog.toml\"]")
        );
    }

    #[test]
    fn media_is_listed_and_replaced() {
        let manifest = "# a recording\n[package]\nid = \"a\"\nname = \"A\"\nabbreviation = \"A\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n\n[formats]\nusx = { path = \"usx/\" }\n";
        let mut project = Project::from_text("", Some(manifest), None).unwrap();
        let mut entry =
            crate::media::MediaFile::of("JHN.mp3", "https://example.org/JHN.mp3", b"abc");
        entry.mirrors = vec![String::from("https://mirror.example.org/JHN.mp3")];
        entry.book = Some(String::from("JHN"));
        project.add_media(&entry).unwrap();
        let other = crate::media::MediaFile::of("MRK.mp3", "https://example.org/MRK.mp3", b"de");
        project.add_media(&other).unwrap();
        let files = |p: &Project| p.manifest().media.clone().unwrap_or_default().files;
        assert_eq!(files(&project), [entry.clone(), other.clone()]);
        assert!(project.manifest_text().starts_with("# a recording\n"));
        // Listed again: replaced, not added
        let changed =
            crate::media::MediaFile::of("JHN.mp3", "https://example.org/v2/JHN.mp3", b"abcd");
        project.add_media(&changed).unwrap();
        assert_eq!(files(&project), [other, changed]);
        assert_eq!(
            project.manifest_text().matches("[[media.files]]").count(),
            2
        );
    }

    #[test]
    fn bibles_of_one_work_are_found() {
        let bible = |label: &str, id: &str, work: Option<&str>| {
            (label.to_string(), id.to_string(), work.map(String::from))
        };
        let found = same_works([
            bible("kjv 1.0.0", "kjv", Some("kjv")),
            bible("bsb 1.0.0", "bsb", Some("bsb")),
            bible(
                "ebible-eng-kjv2006 1.0.0",
                "ebible-eng-kjv2006",
                Some("kjv"),
            ),
            // A fork that doesn't say: its original's work
            bible("kjv-bob (kjv) 1.0.0", "kjv", None),
            bible("mine 0.1.0", "mine", None),
        ]);
        assert_eq!(
            found,
            [SameWork {
                work: String::from("kjv"),
                packages: vec![
                    String::from("kjv 1.0.0"),
                    String::from("ebible-eng-kjv2006 1.0.0"),
                    String::from("kjv-bob (kjv) 1.0.0")
                ],
            }]
        );
    }
}
