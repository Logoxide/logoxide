/*!
A project: a folder with a `biblix.toml` that asks for packages (a vault, or the library's own
for apps without one) and its `biblix.lock`. Adding and removing dependencies edits the manifest
in place, keeping its comments and layout.
*/

use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, value};

use crate::{
    error::{BiblixError, Result},
    git::Fetcher,
    library::Library,
    lock::{LOCKFILE, Lockfile},
    manifest::{Dependency, DetailedDependency, MANIFEST, Manifest, PackageKind},
    resolve::{Resolver, Update},
};
use std::collections::BTreeMap;

use topos_bible::data::{books::BookId, chapter_verses::ChapterVerses, translations::Translations};

use crate::versification::{Mapped, VersificationMap, Versifications};

/// What a Bible (or a versification named alone) numbers verses by: [`Project::numbering`]
#[derive(Clone, Debug)]
pub struct Numbering {
    /// The versification's name (`English`)
    pub versification: String,
    /// A Bible's own verse counts, by book (none for a versification named alone)
    pub counts: BTreeMap<BookId, ChapterVerses>,
}

/// A new project's `biblix.toml`
const NEW_MANIFEST: &str = "# The Bibles this uses: `bx add --git URL`\n[dependencies]\n";

#[derive(Clone)]
pub struct Project {
    dir: PathBuf,
    doc: DocumentMut,
    manifest: Manifest,
    lock: Option<Lockfile>,
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(BiblixError::io(&path.display().to_string(), e)),
    }
}

impl Project {
    /// The project in `dir` (an empty one if it has no `biblix.toml` yet)
    pub fn open(dir: impl Into<PathBuf>) -> Result<Project> {
        let dir = dir.into();
        let text = read_optional(&dir.join(MANIFEST))?.unwrap_or_default();
        let doc: DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| BiblixError::Manifest(e.to_string()))?;
        let manifest = Manifest::parse(&text)?;
        let lock = read_optional(&dir.join(LOCKFILE))?
            .map(|t| Lockfile::parse(&t))
            .transpose()?;
        Ok(Project {
            dir,
            doc,
            manifest,
            lock,
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
            .map_err(|e: toml_edit::TomlError| BiblixError::Manifest(e.to_string()))?;
        Ok(Project {
            dir: dir.into(),
            doc,
            manifest: Manifest::parse(text)?,
            lock: lock.map(Lockfile::parse).transpose()?,
        })
    }

    /// `biblix.toml`'s text, with any dependencies added or removed
    pub fn manifest_text(&self) -> String {
        self.doc.to_string()
    }

    /// Starts a project in `dir`: an empty `biblix.toml` (an error if it has one)
    pub fn init(dir: impl Into<PathBuf>) -> Result<Project> {
        let dir = dir.into();
        let path = dir.join(MANIFEST);
        if path.exists() {
            return Err(BiblixError::Manifest(format!(
                "{} already exists",
                path.display()
            )));
        }
        let project = Project {
            dir,
            doc: NEW_MANIFEST
                .parse()
                .map_err(|e: toml_edit::TomlError| BiblixError::Manifest(e.to_string()))?,
            manifest: Manifest::default(),
            lock: None,
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
            .ok_or_else(|| BiblixError::Manifest(String::from("[dependencies] isn't a table")))?;
        dependencies.insert(name, value(table));
        self.reparse()
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
            .ok_or_else(|| BiblixError::Manifest(String::from("[settings] isn't a table")))
    }

    /**
    Adds a catalog to search (a URL or a path relative to the project) to `[settings] catalogs`;
    whether it wasn't searched already. A default one (eBible.org's or fetch.bible's) is only
    written when the defaults are turned off
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
            .ok_or_else(|| BiblixError::Manifest(String::from("catalogs isn't a list")))?;
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
    Stops searching a catalog; whether it was searched. Removing eBible.org's or fetch.bible's
    list turns the defaults off (`default-catalogs = false`) and names the other one instead
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
            .ok_or_else(|| BiblixError::Manifest(String::from("registries isn't a table")))?;
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

    /// Writes `biblix.toml`
    pub fn save_manifest(&self) -> Result<()> {
        let path = self.dir.join(MANIFEST);
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| BiblixError::io(&self.dir.display().to_string(), e))?;
        std::fs::write(&path, self.doc.to_string())
            .map_err(|e| BiblixError::io(&path.display().to_string(), e))
    }

    /**
    Resolves the manifest (keeping the lock file's versions unless `update` says otherwise),
    installs every package into the library, and writes `biblix.lock`
    */
    pub fn install(
        &mut self,
        library: &Library,
        fetcher: &dyn Fetcher,
        update: Update,
    ) -> Result<&Lockfile> {
        self.install_with(library, fetcher, update, false)
    }

    /// [`Project::install`], putting back the files of installed versions that changed (a
    /// reader edited them) instead of stopping there; files the packages don't declare stay
    pub fn repair(&mut self, library: &Library, fetcher: &dyn Fetcher) -> Result<&Lockfile> {
        self.install_with(library, fetcher, Update::Nothing, true)
    }

    fn install_with(
        &mut self,
        library: &Library,
        fetcher: &dyn Fetcher,
        update: Update,
        repair: bool,
    ) -> Result<&Lockfile> {
        let packages = library.packages(&self.dir);
        let resolver = Resolver {
            packages: &packages,
            fetcher,
            previous: self.lock.as_ref(),
            update,
            repair,
        };
        let lock = resolver.resolve(&self.manifest)?;
        let path = self.dir.join(LOCKFILE);
        std::fs::write(&path, lock.to_toml()?)
            .map_err(|e| BiblixError::io(&path.display().to_string(), e))?;
        Ok(self.lock.insert(lock))
    }

    /// The Bibles the manifest asks for, and those in its collections, each opened
    fn bibles(&self, library: &Library) -> Result<Vec<(&crate::LockedPackage, crate::Package)>> {
        let mut bibles = vec![];
        let Some(lock) = &self.lock else {
            return Ok(bibles);
        };
        // The manifest's packages, and what each collection among them brings in
        let mut todo: Vec<&crate::LockedPackage> = lock.roots().collect();
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
            if package.info().kind == PackageKind::Bible {
                bibles.push((locked, package));
            }
        }
        Ok(bibles)
    }

    /**
    The translations the manifest asks for (its Bibles, and those in its collections; not other
    dependencies), for topos: each one's abbreviation, aliases, and verse counts, from the
    library's cache
    */
    pub fn translations(&self, library: &Library) -> Result<Translations> {
        let mut translations = Translations::default();
        let cache = library.book_cache();
        for (locked, package) in self.bibles(library)? {
            let (abbreviation, aliases) = self.names(locked.dependency_name(), package.info());
            translations = cache
                .open(package)?
                .add_as(translations, &abbreviation, &aliases);
        }
        Ok(translations)
    }

    /// The versification maps in the lock (any package of it, so a Bible's own dependency on a
    /// map counts too)
    pub fn versifications(&self, library: &Library) -> Result<Versifications> {
        let mut maps = Versifications::default();
        for locked in self.lock.iter().flat_map(|lock| &lock.packages) {
            let package = library.open(locked, &self.dir)?;
            if package.info().kind == PackageKind::VersificationMap {
                maps.add(VersificationMap::from_package(&package)?);
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
    pub fn numbering(&self, library: &Library, name: &str) -> Result<Numbering> {
        for (locked, package) in self.bibles(library)? {
            let (abbreviation, aliases) = self.names(locked.dependency_name(), package.info());
            let names = [locked.dependency_name(), abbreviation.as_str()];
            let is = (names.into_iter().chain(aliases.iter().map(String::as_str)))
                .any(|n| n.eq_ignore_ascii_case(name));
            if !is {
                continue;
            }
            let versification = package.info().versification.clone();
            if versification == "own" {
                return Err(BiblixError::Versification(format!(
                    "{name} names no versification (`versification = \"own\"`): give its \
                     [package] one (`versification = \"English\"`), or name a versification"
                )));
            }
            let cached = library.book_cache().open(package)?;
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
    pub fn map_reference(
        &self,
        library: &Library,
        reference: &str,
        from: &str,
        to: &str,
    ) -> Result<Mapped> {
        let passage = crate::versification::parse_reference(reference)?;
        let (from, to) = (self.numbering(library, from)?, self.numbering(library, to)?);
        let counts = from.counts.get(&passage.book);
        (self.versifications(library)?).map(
            &passage,
            &from.versification,
            &to.versification,
            counts,
        )
    }

    /// What references name a dependency by here: its `abbreviation` and `aliases` in the
    /// manifest, else the package's own
    pub fn names(
        &self,
        dependency: &str,
        info: &crate::manifest::Package,
    ) -> (String, Vec<String>) {
        let detailed = match self.manifest.dependencies.get(dependency) {
            Some(Dependency::Detailed(d)) => Some(d),
            _ => None,
        };
        let abbreviation = (detailed.and_then(|d| d.abbreviation.clone()))
            .unwrap_or_else(|| info.abbreviation.clone());
        let aliases =
            (detailed.and_then(|d| d.aliases.clone())).unwrap_or_else(|| info.aliases.clone());
        (abbreviation, aliases)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{DEFAULT_CATALOGS, EBIBLE_CATALOG, FETCH_BIBLE_CATALOG};

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
                FETCH_BIBLE_CATALOG
            ]
        );

        // Removing a default keeps the other one, by name
        assert!(project.remove_catalog(EBIBLE_CATALOG).unwrap());
        assert_eq!(
            project.manifest().catalogs(),
            ["https://example.org/catalog.toml", FETCH_BIBLE_CATALOG]
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
}
