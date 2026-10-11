/*!
Resolving a manifest's dependencies into a lock file, and installing what it names: each git
dependency's version (the newest tag matching its requirement, or the tag, branch, or revision it
names), each registry dependency's release (the newest matching archive its catalog lists), its
files in the library, and its own dependencies, with the features asked for.

What the previous lock file says is kept unless it's being updated or the manifest asks for
something else, so installing on another device gets the same commits, checked by checksum.
*/

use std::{
    collections::{BTreeSet, VecDeque},
    sync::Arc,
};

use crate::{
    error::{LogoxideError, Result},
    git::{Fetcher, GitRef},
    library::open_locked,
    lock::{LockedDownload, LockedPackage, Lockfile},
    manifest::{Dependency, DetailedDependency, Manifest},
    package::{Features, Package},
    storage::{MemoryStorage, Storage},
};

/**
Where installed packages are, for the resolver: the library's folder and the project's
([`crate::Library::packages`]), or the host's files (logoxide-ffi)
*/
pub trait Packages {
    /// A locked version's files: in the library, or a path package's folder. `None` when it
    /// isn't installed
    fn installed(&self, locked: &LockedPackage) -> Result<Option<Arc<dyn Storage>>>;

    /// A path package's folder, relative to the project's
    fn path_package(&self, relative: &str) -> Result<Arc<dyn Storage>>;

    /// Puts a fetched version's files in the library, where `locked` goes
    fn install(&self, locked: &LockedPackage, files: &dyn Storage) -> Result<()>;

    /// Puts an installed version's files back as they were fetched (`files`), where they were
    /// changed: its own files only, so a reader's annotations beside them stay
    fn repair(&self, locked: &LockedPackage, files: &dyn Storage) -> Result<()>;
}

/**
What the resolver asks a host to import ([`Fetcher::import`]): the package logoxide-import makes
from a publisher's downloads. The package manager knows no publisher; the host (logoxide-app,
the bindings) finds the import in its catalogs and builds it
*/
pub struct ImportRequest<'a> {
    /// The dependency's name, for messages
    pub name: &'a str,
    /// The import's name (`ebible:eng-kjv2006`)
    pub import: &'a str,
    pub kind: ImportKind<'a>,
}

/// Which import is asked for
pub enum ImportKind<'a> {
    /**
    The locked one made again: from the same downloads, each checked by its SHA-256, and given
    the locked version (an import's version is logoxide's label, given when it's locked). The
    resolver then checks its files against the locked checksum
    */
    Pinned(&'a LockedPackage),
    /**
    The publisher's text as it is now. With `current` (the version it replaces, and its files),
    its version is raised by what changed, or is `current`'s own when nothing did; without, it's
    a new import's
    */
    Fresh {
        current: Option<(&'a LockedPackage, Arc<dyn Storage>)>,
    },
}

/// An import's package as a host made it
pub struct ImportBuild {
    /// The package's files, `logoxide.toml` among them
    pub files: MemoryStorage,
    /// Each file it was made from, and where it was downloaded
    pub downloads: Vec<LockedDownload>,
    /// The publisher's version of the text, as its catalog listed it
    pub upstream_version: Option<String>,
}

/// Which locked packages may move to newer versions
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Update {
    /// None: keep what the lock file says where the manifest still allows it
    #[default]
    Nothing,
    All,
    Only(BTreeSet<String>),
}

impl Update {
    fn allows(&self, id: &str) -> bool {
        match self {
            Update::Nothing => false,
            Update::All => true,
            Update::Only(ids) => ids.contains(id),
        }
    }
}

/**
How far installing may go beyond the lock file and the library, as Cargo's `--locked` and
`--offline`: for CI, and for machines without a network
*/
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frozen {
    /// The lock file must come out as it was: a dependency it lacks, or anything that would
    /// change it, is an error
    pub locked: bool,
    /// Nothing is fetched: what isn't installed (or locked) already is an error
    pub offline: bool,
}

pub struct Resolver<'a> {
    pub packages: &'a dyn Packages,
    pub fetcher: &'a dyn Fetcher,
    /// The lock file before this resolve, if any
    pub previous: Option<&'a Lockfile>,
    pub update: Update,
    /// Installed versions whose files changed are fetched again and put back, instead of being
    /// an error
    pub repair: bool,
    /// Whether the lock file may change, and whether anything may be fetched
    pub frozen: Frozen,
}

/// A dependency to resolve, and where it was named
struct Want {
    name: String,
    dependency: DetailedDependency,
    /// The folder of the package that named it, relative to the manifest's (`""` for the manifest)
    within: String,
    /// Named by a git package (which can't have path dependencies)
    from_git: bool,
    /// Named by a package fetched from another machine (a remote repository or registry), which
    /// may name only what's on other machines: nothing on this one
    from_remote: bool,
    /// The index (in the lock file) of the package that named it, if not the manifest
    parent: Option<usize>,
    /// That package as locked (its id, rename, and name), to find what the previous lock file
    /// gave it
    parent_locked: Option<(String, Option<String>, String)>,
}

/// A tag's version: `v1.2.0` or `1.2.0`
pub fn tag_version(tag: &str) -> Option<semver::Version> {
    tag.strip_prefix('v').unwrap_or(tag).parse().ok()
}

/**
The version of a tag of the package in `folder` of its repository (`romans/v1.2.0`, as
`lx release` tags a package in a folder), or of a package at the repository's root (`v1.2.0`)
when there's no folder. Another folder's tags are no version of it
*/
pub fn folder_tag_version(tag: &str, folder: Option<&str>) -> Option<semver::Version> {
    let own = match folder
        .map(|f| f.trim_matches('/'))
        .filter(|f| !f.is_empty())
    {
        Some(folder) => tag.strip_prefix(folder)?.strip_prefix('/')?,
        None => tag,
    };
    if own.contains('/') {
        return None;
    }
    tag_version(own)
}

/**
Tags that look like releases of the package in `folder` (or at the top) but aren't semver
(`v1.0`, `2.1`), so no requirement ever matches them: worth saying, since a publisher may think
they're releases
*/
pub fn not_semver_tags<'t>(
    tags: impl IntoIterator<Item = &'t str>,
    folder: Option<&str>,
) -> Vec<&'t str> {
    let folder = folder
        .map(|f| f.trim_matches('/'))
        .filter(|f| !f.is_empty());
    (tags.into_iter())
        .filter(|tag| {
            let own = match folder {
                Some(folder) => tag.strip_prefix(folder).and_then(|t| t.strip_prefix('/')),
                None => Some(*tag),
            };
            own.filter(|own| !own.contains('/'))
                .map(|own| own.strip_prefix('v').unwrap_or(own))
                .is_some_and(|own| own.starts_with(|c: char| c.is_ascii_digit()))
                && folder_tag_version(tag, folder).is_none()
        })
        .collect()
}

/// What [`not_semver_tags`] finds, said after a message (` (v1.0 names no version: …)`), or
/// nothing
pub fn not_semver_note<'t>(
    tags: impl IntoIterator<Item = &'t str>,
    folder: Option<&str>,
) -> String {
    let odd = not_semver_tags(tags, folder);
    match odd.len() {
        0 => String::new(),
        n => format!(
            " ({} {} no version: releases are tagged with three numbers, v1.0.0)",
            odd.join(", "),
            if n == 1 { "names" } else { "name" }
        ),
    }
}

/// Whether a locked package came from another machine (not a path, a local repository, or a
/// file): what it names may come only from other machines too
fn is_remote(locked: &LockedPackage) -> bool {
    let remote = |url: &Option<String>| url.as_deref().is_some_and(|u| !crate::git::is_local(u));
    remote(&locked.git) || remote(&locked.registry) || remote(&locked.archive)
}

fn relative_join(within: &str, path: &str) -> String {
    // An absolute path is where it says, wherever it was named
    if path.starts_with('/') {
        return path.to_string();
    }
    let mut parts: Vec<&str> = within.split('/').filter(|p| !p.is_empty()).collect();
    for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." && parts.last().is_some_and(|p| *p != "..") {
            parts.pop();
        } else {
            parts.push(part);
        }
    }
    parts.join("/")
}

impl Resolver<'_> {
    /// The lock file for `manifest`, with every package it names installed
    pub fn resolve(&self, manifest: &Manifest) -> Result<Lockfile> {
        self.check_update(manifest)?;
        if self.frozen.locked && self.previous.is_none() {
            return Err(LogoxideError::Resolve(String::from(
                "there's no logoxide.lock to keep (install without --locked to write one)",
            )));
        }
        let lock = self.resolve_all(manifest)?;
        if self.frozen.locked && self.previous != Some(&lock) {
            return Err(LogoxideError::Resolve(String::from(
                "logoxide.lock would change (the manifest asks for something it doesn't have): \
                 install without --locked to update it",
            )));
        }
        Ok(lock)
    }

    /// Nothing may be fetched offline: `what` (of the dependency `name`) needs fetching
    fn may_fetch(&self, name: &str, what: &str) -> Result<()> {
        if self.frozen.offline {
            return Err(LogoxideError::Resolve(format!(
                "`{name}`: {what} isn't installed, and offline nothing is fetched"
            )));
        }
        Ok(())
    }

    /// Nothing new may be chosen when the lock file must stay as it is, nor offline (choosing
    /// reads tags or a catalog)
    fn may_choose(&self, name: &str) -> Result<()> {
        let why = match (self.frozen.locked, self.frozen.offline) {
            (true, _) => "logoxide.lock must stay as it is (--locked)",
            (_, true) => "offline nothing is fetched to choose a version from",
            _ => return Ok(()),
        };
        Err(LogoxideError::Resolve(format!(
            "`{name}` isn't locked as the manifest asks for it, and {why}"
        )))
    }

    fn resolve_all(&self, manifest: &Manifest) -> Result<Lockfile> {
        let mut lock = Lockfile::default();
        let mut todo: VecDeque<Want> = VecDeque::new();
        for (name, dependency) in &manifest.dependencies {
            let dependency = match dependency {
                Dependency::Detailed(d) => d.clone(),
                Dependency::Version(req) => {
                    return Err(LogoxideError::Resolve(format!(
                        "{name} = \"{req}\": there's no default registry; give `git`, `registry`, `path`, or `import`"
                    )));
                }
            };
            todo.push_back(Want {
                name: name.clone(),
                dependency,
                within: String::new(),
                from_git: false,
                from_remote: false,
                parent: None,
                parent_locked: None,
            });
        }
        while let Some(want) = todo.pop_front() {
            let mut locked = self.resolve_one(&want)?;
            // The manifest's own name for it, when that isn't its id
            if want.parent.is_none() && want.name != locked.id {
                locked.rename = Some(want.name.clone());
            }
            let name = locked.name();
            // Two names for one commit (`kjv` and `kjv-two`) are two entries sharing a folder:
            // each is turned on and off, and named in references, apart
            let (index, changed) = match lock.packages.iter().position(|p| p.name() == name) {
                Some(index) => {
                    // Asked for again, maybe with more features: one install with all of them
                    let existing = &mut lock.packages[index];
                    let before = (existing.features.clone(), existing.default_features);
                    let mut features: BTreeSet<String> =
                        existing.features.iter().cloned().collect();
                    features.extend(locked.features.iter().cloned());
                    existing.features = features.into_iter().collect();
                    existing.default_features |= locked.default_features;
                    (
                        index,
                        before != (existing.features.clone(), existing.default_features),
                    )
                }
                None => {
                    lock.packages.push(locked);
                    (lock.packages.len() - 1, true)
                }
            };
            let named_by = match want.parent {
                Some(parent) => &mut lock.packages[parent].dependencies,
                None => &mut lock.dependencies,
            };
            if !named_by.contains(&name) {
                named_by.push(name);
            }
            if !changed {
                continue;
            }
            // Its own dependencies, with the features it now has
            let locked = lock.packages[index].clone();
            let package = self.open(&locked)?;
            let enabled = package.enabled().clone();
            let within = match &locked.path {
                Some(path) => path.clone(),
                None => String::new(),
            };
            for (dep_name, dependency) in &package.manifest().dependencies {
                let Dependency::Detailed(dependency) = dependency else {
                    return Err(LogoxideError::Resolve(format!(
                        "{}: dependency `{dep_name}` has no `git`, `registry`, `path`, or `import`",
                        locked.name()
                    )));
                };
                if dependency.optional && !enabled.dependencies.contains(dep_name) {
                    continue;
                }
                let mut dependency = dependency.clone();
                if let Some(features) = enabled.dependency_features.get(dep_name) {
                    dependency.features.extend(features.iter().cloned());
                }
                todo.push_back(Want {
                    name: dep_name.clone(),
                    dependency,
                    within: within.clone(),
                    from_git: locked.git.is_some() || locked.registry.is_some(),
                    from_remote: want.from_remote || is_remote(&locked),
                    parent: Some(index),
                    parent_locked: Some((locked.id.clone(), locked.rename.clone(), locked.name())),
                });
            }
        }
        lock.packages
            .sort_by(|a, b| (&a.id, &a.version, &a.rename).cmp(&(&b.id, &b.version, &b.rename)));
        // Indexes changed with the sort; dependencies are named, so they still hold
        Ok(lock)
    }

    /**
    Each name to update is one the manifest has, or one a package brings in (as the previous
    lock file names it): a mistyped name would otherwise update nothing, and say nothing
    */
    fn check_update(&self, manifest: &Manifest) -> Result<()> {
        let Update::Only(names) = &self.update else {
            return Ok(());
        };
        let locked = |name: &String| {
            (self.previous.into_iter().flat_map(|l| &l.packages))
                .any(|p| p.id == *name || p.dependency_name() == name)
        };
        let unknown: Vec<&str> = (names.iter())
            .filter(|name| !manifest.dependencies.contains_key(*name) && !locked(name))
            .map(String::as_str)
            .collect();
        if unknown.is_empty() {
            return Ok(());
        }
        let known: Vec<&str> = manifest.dependencies.keys().map(String::as_str).collect();
        Err(LogoxideError::Resolve(format!(
            "nothing here is named {} (the manifest has {})",
            unknown.join(", "),
            match known.is_empty() {
                true => String::from("no dependencies"),
                false => known.join(", "),
            }
        )))
    }

    /// A locked version that's installed
    fn open(&self, locked: &LockedPackage) -> Result<Package> {
        let storage = self
            .packages
            .installed(locked)?
            .ok_or_else(|| LogoxideError::NotFound(format!("{} (not installed)", locked.name())))?;
        open_locked(storage, locked)
    }

    fn resolve_one(&self, want: &Want) -> Result<LockedPackage> {
        let d = &want.dependency;
        // Its name and id name folders: checked before they're used (a manifest built in code
        // isn't parsed)
        crate::manifest::check_id("dependency", &want.name)?;
        if let Some(package) = &d.package {
            crate::manifest::check_id(&format!("dependency `{}`: package", want.name), package)?;
        }
        if want.from_remote {
            // A package from elsewhere can't reach into this machine: a folder, a local
            // repository, or a file
            let local = match &d.path {
                Some(path) => Some(path),
                None => [&d.git, &d.registry]
                    .into_iter()
                    .flatten()
                    .find(|url| crate::git::is_local(&crate::git::expand_url(url))),
            };
            if let Some(local) = local {
                return Err(LogoxideError::Resolve(format!(
                    "`{}`: a package from another machine names `{local}`, on this one",
                    want.name
                )));
            }
            // git's own protocol has no encryption: anyone on the way could change the text
            if let Some(url) =
                (d.git.as_deref()).filter(|u| u.trim().to_ascii_lowercase().starts_with("git://"))
            {
                return Err(LogoxideError::Resolve(format!(
                    "`{}`: a package from another machine names `{url}`, over git's unencrypted protocol",
                    want.name
                )));
            }
        }
        let mut features: Vec<String> = d.features.clone();
        features.sort();
        features.dedup();
        if let Some(import) = &d.import {
            let mut locked = self.import_release(want, import)?;
            locked.features = features;
            locked.default_features = d.default_features;
            return Ok(locked);
        }
        if let Some(path) = &d.path {
            if want.from_git {
                return Err(LogoxideError::Resolve(format!(
                    "`{}`: a git or registry package can't have path dependencies",
                    want.name
                )));
            }
            let relative = relative_join(&want.within, path);
            let storage = self.packages.path_package(&relative)?;
            let names: Vec<&str> = features.iter().map(String::as_str).collect();
            let package = Package::open(
                storage,
                Features {
                    features: &names,
                    default: d.default_features,
                },
            )
            .map_err(|e| LogoxideError::Resolve(format!("{} ({relative}): {e}", want.name)))?;
            let info = package.info();
            self.check_id(&want.name, d, &info.id)?;
            return Ok(LockedPackage {
                path: Some(relative),
                features,
                default_features: d.default_features,
                ..LockedPackage::new(&info.id, info.version.clone())
            });
        }
        if let Some(registry) = &d.registry {
            if d.git.is_some() {
                return Err(LogoxideError::Resolve(format!(
                    "`{}`: give `git` or `registry`, not both",
                    want.name
                )));
            }
            let mut locked = self.registry_release(want, registry)?;
            locked.features = features;
            locked.default_features = d.default_features;
            return Ok(locked);
        }
        let url = d.git.as_deref().ok_or_else(|| {
            LogoxideError::Resolve(format!(
                "`{}`: give `git`, `registry`, `path`, or `import`",
                want.name
            ))
        })?;
        let url = &crate::git::expand_url(url);
        // What the previous lock file has, if the manifest still allows it and it isn't updating
        let kept = self.kept(want, |p| {
            // Still what the manifest asks for: the same tag, branch, or revision, or (for a
            // version requirement, whose tag was chosen) a version it allows
            let same = match (&d.tag, &d.branch, &d.rev) {
                (Some(tag), _, _) => p.tag.as_ref() == Some(tag),
                (_, Some(branch), _) => p.branch.as_ref() == Some(branch),
                (_, _, Some(rev)) => p.rev.as_ref() == Some(rev),
                _ => {
                    p.branch.is_none()
                        && p.rev.is_none()
                        && d.version.as_ref().is_none_or(|req| req.matches(&p.version))
                }
            };
            p.id == d.package_id(&want.name)
                && p.git.as_deref() == Some(url.as_str())
                && p.folder.as_deref().map(|f| f.trim_matches('/'))
                    == d.folder.as_deref().map(|f| f.trim_matches('/'))
                && p.commit.is_some()
                && same
        });
        let (reference, tag) = match kept {
            Some(kept) => (
                GitRef::Rev(kept.commit.clone().unwrap_or_default()),
                kept.tag.clone(),
            ),
            None => {
                self.may_choose(&want.name)?;
                self.reference(&want.name, url, d)?
            }
        };
        let mut locked = self.install(&want.name, d, url, &reference, kept)?;
        locked.tag = tag;
        locked.branch = d.branch.clone();
        locked.rev = d.rev.clone();
        locked.features = features;
        locked.default_features = d.default_features;
        Ok(locked)
    }

    /**
    What the previous lock file has for a dependency, if it's still what's asked for (`fits`)
    and isn't being updated: first what the same asker locked (the manifest's top-level
    `dependencies`, or the package that named it), then any entry. A lock can hold one package
    at two versions (a collection's `kjv` 1, the manifest's 2); each keeps its own
    */
    fn kept(&self, want: &Want, fits: impl Fn(&LockedPackage) -> bool) -> Option<&LockedPackage> {
        match self.update.allows(&want.name) {
            true => None,
            false => self.locked_before(want, fits),
        }
    }

    /// What the previous lock file had for a dependency that `fits`, updating or not: [`Self::kept`]
    fn locked_before(
        &self,
        want: &Want,
        fits: impl Fn(&LockedPackage) -> bool,
    ) -> Option<&LockedPackage> {
        let previous = self.previous?;
        let asker: Option<&Vec<String>> = match &want.parent_locked {
            None => Some(&previous.dependencies),
            Some((id, rename, name)) => (previous.named(name))
                .or_else(|| (previous.packages.iter()).find(|p| p.id == *id && p.rename == *rename))
                .map(|p| &p.dependencies),
        };
        let theirs = (asker.into_iter().flatten())
            .filter_map(|name| previous.named(name))
            .find(|p| fits(p));
        theirs.or_else(|| previous.packages.iter().find(|p| fits(p)))
    }

    /// The commit a dependency asks for, and the tag it's from (for version requirements)
    fn reference(
        &self,
        name: &str,
        url: &str,
        d: &DetailedDependency,
    ) -> Result<(GitRef, Option<String>)> {
        if let Some(tag) = &d.tag {
            return Ok((GitRef::Tag(tag.clone()), Some(tag.clone())));
        }
        if let Some(branch) = &d.branch {
            return Ok((GitRef::Branch(branch.clone()), None));
        }
        if let Some(rev) = &d.rev {
            return Ok((GitRef::Rev(rev.clone()), None));
        }
        let tags = self.fetcher.tags(url)?;
        let newest = tags
            .iter()
            .filter_map(|(tag, _)| Some((folder_tag_version(tag, d.folder.as_deref())?, tag)))
            .filter(|(version, _)| match &d.version {
                Some(req) => req.matches(version),
                // Without a requirement, the newest release (not a pre-release)
                None => version.pre.is_empty(),
            })
            .max_by(|a, b| a.0.cmp(&b.0));
        match (newest, &d.version) {
            (Some((_, tag)), _) => Ok((GitRef::Tag(tag.clone()), Some(tag.clone()))),
            (None, Some(req)) => {
                let mut message = match &d.folder {
                    Some(folder) => {
                        format!("`{name}`: no tag of {url} for {folder}/ matches {req}")
                    }
                    None => format!("`{name}`: no tag of {url} matches {req}"),
                };
                message.push_str(&not_semver_note(
                    tags.iter().map(|(t, _)| t.as_str()),
                    d.folder.as_deref(),
                ));
                Err(LogoxideError::Resolve(message))
            }
            (None, None) => Ok((GitRef::Head, None)),
        }
    }

    /// Fetches a version into the library (unless it's there), checking it's the package asked for
    fn install(
        &self,
        name: &str,
        d: &DetailedDependency,
        url: &str,
        reference: &GitRef,
        kept: Option<&LockedPackage>,
    ) -> Result<LockedPackage> {
        // Already installed from the lock file: check it's still the same text
        if let Some(kept) = kept
            && let Some(installed) = self.packages.installed(kept)?
        {
            let checksum = crate::package::checksum(installed.as_ref())?;
            let changed = kept.checksum.as_ref().is_some_and(|c| *c != checksum);
            if changed && !self.repair {
                return Err(LogoxideError::Changed {
                    id: kept.label(),
                    dir: crate::library::library_dir(kept),
                    expected: kept.checksum.clone().unwrap_or_default(),
                    actual: checksum,
                });
            }
            if !changed {
                return Ok(kept.clone());
            }
            // Changed, and repairing: fetched again below, and its files put back
        }
        let what = kept.map_or_else(|| url.to_string(), LockedPackage::label);
        self.may_fetch(name, &what)?;
        let fetched = MemoryStorage::new();
        let commit = (self.fetcher.fetch(url, reference, &fetched)).map_err(|e| match kept {
            // The lock's commit, fetched by itself: a tag moved or a branch rewritten since takes
            // it off the host
            Some(kept) if e.is_fetch() => e.with_fetch_note(&format!(
                "(the locked commit {} of {} may no longer be on the host: if its tag was moved \
                 or its branch rewritten, `lx update {name}` locks what's there now)",
                kept.commit.as_deref().unwrap_or("?"),
                kept.label()
            )),
            _ => e,
        })?;
        // A package in a folder of its repository is that folder's files
        let fetched = match d.folder.as_deref().map(|f| f.trim_matches('/')) {
            Some(folder) if !folder.is_empty() => {
                crate::release::within(&fetched, &format!("{folder}/"))?
            }
            _ => fetched,
        };
        // The package's own files: what its manifest declares
        let storage = crate::package::only_declared(&fetched)?;
        // Opened only to read what it is; no format needs to be enabled for that
        let manifest =
            crate::manifest::Manifest::parse(&storage.read_string(crate::manifest::MANIFEST)?)?;
        let info = manifest.package.clone().ok_or_else(|| {
            LogoxideError::Resolve(format!(
                "`{name}`: {url} has a logoxide.toml without [package]"
            ))
        })?;
        self.check_id(name, d, &info.id)?;
        if let GitRef::Tag(tag) = reference
            && let Some(version) = folder_tag_version(tag, d.folder.as_deref())
            && version != info.version
        {
            return Err(LogoxideError::Resolve(format!(
                "`{name}`: tag {tag} has version {} in its logoxide.toml",
                info.version
            )));
        }
        let checksum = crate::package::checksum(&storage)?;
        if let Some(kept) = kept
            && kept.checksum.as_ref().is_some_and(|c| *c != checksum)
        {
            return Err(LogoxideError::Checksum {
                id: kept.name(),
                expected: kept.checksum.clone().unwrap_or_default(),
                actual: checksum,
            });
        }
        let locked = LockedPackage {
            git: Some(url.to_string()),
            tag: match reference {
                GitRef::Tag(tag) => Some(tag.clone()),
                _ => None,
            },
            branch: match reference {
                GitRef::Branch(branch) => Some(branch.clone()),
                _ => None,
            },
            rev: None,
            commit: Some(commit),
            folder: d.folder.as_ref().map(|f| f.trim_matches('/').to_string()),
            checksum: Some(checksum),
            ..LockedPackage::new(&info.id, info.version.clone())
        };
        // Fetched by commit from the lock file: still the release (or branch) it was locked from
        let mut locked = locked;
        if let Some(kept) = kept {
            locked.tag = kept.tag.clone();
            locked.branch = kept.branch.clone();
            locked.rev = kept.rev.clone();
        }
        match self.packages.installed(&locked)? {
            // The same version from another request: it must be the same text
            Some(installed) => {
                let installed = crate::package::checksum(installed.as_ref())?;
                if Some(&installed) != locked.checksum.as_ref() {
                    if !self.repair {
                        return Err(LogoxideError::Checksum {
                            id: locked.name(),
                            expected: installed,
                            actual: locked.checksum.clone().unwrap_or_default(),
                        });
                    }
                    self.packages.repair(&locked, &storage)?;
                }
            }
            None => self.packages.install(&locked, &storage)?,
        }
        Ok(locked)
    }

    /**
    A registry dependency: the release the lock file has (while the manifest allows it and it
    isn't updating), else the newest its catalog lists that the requirement allows. Its archive
    is downloaded only when that version isn't installed, and must hash to what the catalog (or
    the lock) says, as its files must
    */
    fn registry_release(&self, want: &Want, registry: &str) -> Result<LockedPackage> {
        let d = &want.dependency;
        let id = d.package_id(&want.name);
        crate::manifest::check_id("package", id)?;
        let kept = self.kept(want, |p| {
            p.id == id
                && p.registry.as_deref() == Some(registry)
                && p.archive.is_some()
                && p.sha256.is_some()
                && p.checksum.is_some()
                && d.version.as_ref().is_none_or(|req| req.matches(&p.version))
        });
        let mut locked = match kept {
            Some(kept) => kept.clone(),
            None => {
                self.may_choose(&want.name)?;
                let text = self.fetcher.download(registry)?;
                let catalog =
                    crate::catalog::Catalog::parse(registry, &String::from_utf8_lossy(&text))?;
                let entry = catalog.find(id)?.ok_or_else(|| {
                    LogoxideError::Resolve(format!("`{}`: {registry} has no `{id}`", want.name))
                })?;
                // A remote catalog's archives are remote too (as reading it checks)
                let local = (entry.releases.iter())
                    .flat_map(|r| std::iter::once(&r.url).chain(&r.mirrors))
                    .find(|url| crate::git::is_local(url));
                if !crate::git::is_local(registry)
                    && let Some(local) = local
                {
                    return Err(LogoxideError::Resolve(format!(
                        "`{}`: {registry} lists `{local}`, on this machine",
                        want.name
                    )));
                }
                let release = entry.release(d.version.as_ref()).ok_or_else(|| {
                    let wanted = (d.version.as_ref()).map_or(String::from("a release"), |r| {
                        format!("a release matching {r}")
                    });
                    LogoxideError::Resolve(format!(
                        "`{}`: {registry} has no {wanted} of `{id}`",
                        want.name
                    ))
                })?;
                // Without the files' checksum, an installed copy could never be checked
                let checksum = release.checksum.clone().ok_or_else(|| {
                    LogoxideError::Resolve(format!(
                        "`{}`: {registry} lists {id} {} without a `checksum` (republish it with \
                         a newer lx)",
                        want.name, release.version
                    ))
                })?;
                let version = release.version.parse().map_err(|e| {
                    LogoxideError::Resolve(format!("{id} {}: {e}", release.version))
                })?;
                LockedPackage {
                    registry: Some(registry.to_string()),
                    archive: Some(release.url.clone()),
                    mirrors: release.mirrors.clone(),
                    sha256: Some(release.sha256.clone()),
                    checksum: Some(checksum),
                    ..LockedPackage::new(id, version)
                }
            }
        };
        locked.rename = None;
        locked.dependencies = vec![];
        // Installed already: still the same text
        let installed = self.packages.installed(&locked)?;
        if let Some(installed) = &installed {
            let checksum = crate::package::checksum(installed.as_ref())?;
            let changed = locked.checksum.as_ref().is_some_and(|c| *c != checksum);
            if changed && !self.repair {
                return Err(LogoxideError::Changed {
                    id: locked.label(),
                    dir: crate::library::library_dir(&locked),
                    expected: locked.checksum.clone().unwrap_or_default(),
                    actual: checksum,
                });
            }
            if !changed {
                locked.checksum = Some(checksum);
                return Ok(locked);
            }
        }
        self.may_fetch(&want.name, &locked.label())?;
        // From the archive's URL, else each mirror: the first whose bytes hash as listed
        let urls: Vec<String> = (locked.archive.iter().chain(&locked.mirrors))
            .cloned()
            .collect();
        let (url, bytes) = self.fetcher.download_any(&urls, &|url, bytes| {
            let sha256 = crate::publish::sha256(bytes);
            match Some(&sha256) == locked.sha256.as_ref() {
                true => Ok(()),
                false => Err(LogoxideError::Checksum {
                    id: format!("{} ({url})", locked.label()),
                    expected: locked.sha256.clone().unwrap_or_default(),
                    actual: sha256,
                }),
            }
        })?;
        let unpacked = MemoryStorage::new();
        crate::net::archive::unpack(&bytes, &unpacked)?;
        let storage = crate::package::only_declared(&unpacked)?;
        let manifest =
            crate::manifest::Manifest::parse(&storage.read_string(crate::manifest::MANIFEST)?)?;
        let info = manifest.package.ok_or_else(|| {
            LogoxideError::Resolve(format!("`{}`: {url} has no [package]", want.name))
        })?;
        self.check_id(&want.name, d, &info.id)?;
        if info.version != locked.version {
            return Err(LogoxideError::Resolve(format!(
                "`{}`: {url} is version {}, listed as {}",
                want.name, info.version, locked.version
            )));
        }
        let checksum = crate::package::checksum(&storage)?;
        if locked.checksum.as_ref().is_some_and(|c| *c != checksum) {
            return Err(LogoxideError::Checksum {
                id: locked.label(),
                expected: locked.checksum.clone().unwrap_or_default(),
                actual: checksum,
            });
        }
        locked.checksum = Some(checksum);
        match installed {
            Some(_) => self.packages.repair(&locked, &storage)?,
            None => self.packages.install(&locked, &storage)?,
        }
        Ok(locked)
    }

    /**
    An import: the version the lock file has (while the manifest allows it and it isn't
    updating), from the library, else made again from its locked downloads and checked against
    its checksum; else the publisher's text now, made by the host ([`Fetcher::import`]), against
    the version it replaces when updating. Kept in the library by its checksum (`+i`), so every
    project locking the same files shares one folder
    */
    fn import_release(&self, want: &Want, import: &str) -> Result<LockedPackage> {
        let d = &want.dependency;
        let name = want.name.as_str();
        let id = d.package_id(name);
        crate::manifest::check_id("package", id)?;
        let fits = |p: &LockedPackage| {
            p.id == id && p.import.as_deref() == Some(import) && p.checksum.is_some()
        };
        let kept = self.kept(want, |p| {
            fits(p) && d.version.as_ref().is_none_or(|req| req.matches(&p.version))
        });
        if let Some(kept) = kept {
            let mut locked = kept.clone();
            locked.rename = None;
            locked.dependencies = vec![];
            let installed = self.packages.installed(&locked)?;
            if let Some(installed) = &installed {
                let checksum = crate::package::checksum(installed.as_ref())?;
                let changed = locked.checksum.as_ref().is_some_and(|c| *c != checksum);
                if changed && !self.repair {
                    return Err(LogoxideError::Changed {
                        id: locked.label(),
                        dir: crate::library::library_dir(&locked),
                        expected: locked.checksum.clone().unwrap_or_default(),
                        actual: checksum,
                    });
                }
                if !changed {
                    return Ok(locked);
                }
            }
            self.may_fetch(name, &locked.label())?;
            let build = self.fetcher.import(&ImportRequest {
                name,
                import,
                kind: ImportKind::Pinned(kept),
            })?;
            let storage = self.built(name, d, &build)?;
            let info = Manifest::parse(&storage.read_string(crate::manifest::MANIFEST)?)?;
            let version = info.package.map(|p| p.version);
            let checksum = crate::package::checksum(&storage)?;
            if version.as_ref() != Some(&locked.version)
                || Some(&checksum) != locked.checksum.as_ref()
            {
                return Err(LogoxideError::Resolve(format!(
                    "`{name}`: {import} made again isn't the locked {} (its files' checksum is \
                     {checksum}, locked {}): the import has changed since; `lx update {name}` \
                     locks what it makes now",
                    locked.label(),
                    locked.checksum.as_deref().unwrap_or("none"),
                )));
            }
            match installed {
                Some(_) => self.packages.repair(&locked, &storage)?,
                None => self.packages.install(&locked, &storage)?,
            }
            return Ok(locked);
        }
        self.may_choose(name)?;
        // Updating: the version it replaces, when it's installed, so the new one is versioned
        // against it
        let before = self.locked_before(want, fits).cloned();
        let current = match &before {
            Some(before) => (self.packages.installed(before)?).map(|files| (before, files)),
            None => None,
        };
        let build = self.fetcher.import(&ImportRequest {
            name,
            import,
            kind: ImportKind::Fresh { current },
        })?;
        let storage = self.built(name, d, &build)?;
        let info = Manifest::parse(&storage.read_string(crate::manifest::MANIFEST)?)?
            .package
            .ok_or_else(|| {
                LogoxideError::Resolve(format!("`{name}`: {import} made no [package]"))
            })?;
        if let Some(req) = &d.version
            && !req.matches(&info.version)
        {
            return Err(LogoxideError::Resolve(format!(
                "`{name}`: {import} is version {} now, which `version = \"{req}\"` doesn't allow",
                info.version
            )));
        }
        let locked = LockedPackage {
            import: Some(import.to_string()),
            upstream_version: build.upstream_version.clone(),
            downloads: build.downloads.clone(),
            checksum: Some(crate::package::checksum(&storage)?),
            ..LockedPackage::new(&info.id, info.version)
        };
        // Its folder is named by its checksum: one already there is the same files, unless
        // they were changed in place
        match self.packages.installed(&locked)? {
            Some(installed) => {
                let checksum = crate::package::checksum(installed.as_ref())?;
                if Some(&checksum) != locked.checksum.as_ref() {
                    if !self.repair {
                        return Err(LogoxideError::Changed {
                            id: locked.label(),
                            dir: crate::library::library_dir(&locked),
                            expected: locked.checksum.clone().unwrap_or_default(),
                            actual: checksum,
                        });
                    }
                    self.packages.repair(&locked, &storage)?;
                }
            }
            None => self.packages.install(&locked, &storage)?,
        }
        Ok(locked)
    }

    /// An import's files as the host made them: its declared files, the package the dependency
    /// names
    fn built(
        &self,
        name: &str,
        d: &DetailedDependency,
        build: &ImportBuild,
    ) -> Result<MemoryStorage> {
        let storage = crate::package::only_declared(&build.files)?;
        let manifest = Manifest::parse(&storage.read_string(crate::manifest::MANIFEST)?)?;
        let info = manifest.package.ok_or_else(|| {
            LogoxideError::Resolve(format!("`{name}`: the import made no [package]"))
        })?;
        self.check_id(name, d, &info.id)?;
        Ok(storage)
    }

    /// The package fetched is the one the dependency names (its name, or `package`)
    fn check_id(&self, name: &str, d: &DetailedDependency, id: &str) -> Result<()> {
        match &d.package {
            None if name != id => Err(LogoxideError::Resolve(format!(
                "dependency `{name}` is the package `{id}`: name it `{id}`, or add `package = \"{id}\"`"
            ))),
            Some(package) if package != id => Err(LogoxideError::Resolve(format!(
                "dependency `{name}` asks for the package `{package}`, but it's `{id}`"
            ))),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_paths() {
        assert_eq!(tag_version("v1.2.0"), Some("1.2.0".parse().unwrap()));
        assert_eq!(tag_version("1.2.0-beta.1").unwrap().pre.as_str(), "beta.1");
        assert_eq!(tag_version("release"), None);
        assert_eq!(relative_join("", "Bibles/Mine"), "Bibles/Mine");
        assert_eq!(relative_join("Bibles/Mine", "../Lexicon"), "Bibles/Lexicon");
        assert_eq!(relative_join("Bibles/Mine", "/srv/Lexicon"), "/srv/Lexicon");
    }

    #[test]
    fn tags_that_look_like_releases() {
        let tags = [
            "v1.0",
            "2.1",
            "v1.0.0",
            "release",
            "v2",
            "books/v1.0",
            "books/v1.0.0",
        ];
        assert_eq!(not_semver_tags(tags, None), ["v1.0", "2.1", "v2"]);
        assert_eq!(not_semver_tags(tags, Some("books")), ["books/v1.0"]);
    }
}
