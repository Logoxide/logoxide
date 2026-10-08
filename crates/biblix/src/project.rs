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
    manifest::{DetailedDependency, MANIFEST, Manifest, PackageKind},
    resolve::{Resolver, Update},
};
use topos_bible::data::translations::Translations;

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
            doc: "# The Bibles this uses: `bx add --git URL`\n[dependencies]\n"
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
        let resolver = Resolver {
            library,
            fetcher,
            previous: self.lock.as_ref(),
            update,
        };
        let lock = resolver.resolve(&self.manifest, &self.dir)?;
        let path = self.dir.join(LOCKFILE);
        std::fs::write(&path, lock.to_toml()?)
            .map_err(|e| BiblixError::io(&path.display().to_string(), e))?;
        Ok(self.lock.insert(lock))
    }

    /**
    The translations the manifest asks for (its Bibles, not their dependencies), for topos: each
    one's abbreviation, aliases, and verse counts, from the library's cache
    */
    pub fn translations(&self, library: &Library) -> Result<Translations> {
        let mut translations = Translations::default();
        let Some(lock) = &self.lock else {
            return Ok(translations);
        };
        let cache = library.book_cache();
        for locked in lock.roots() {
            let package = library.open(locked, &self.dir)?;
            if package.info().kind != PackageKind::Bible {
                continue;
            }
            translations = cache.open(&package)?.add_to(translations);
        }
        Ok(translations)
    }
}
