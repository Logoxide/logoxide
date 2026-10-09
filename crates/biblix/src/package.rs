/*!
A package: a `biblix.toml` with `[package]`, and its text in one or more formats. Opening one
picks the best format its enabled features turn on (USX, then USFM, OSIS, and ThML).
*/

use std::sync::Arc;

use sha2::{Digest, Sha256};
use topos_bible::data::books::BookId;

use crate::{
    error::{BiblixError, Result},
    manifest::{self, Enabled, Format, MANIFEST, Manifest},
    source::{OsisSource, Source, ThmlSource, UsfmSource, UsxSource},
    storage::Storage,
};

pub struct Package {
    manifest: Manifest,
    storage: Arc<dyn Storage>,
    enabled: Enabled,
    source: Box<dyn Source>,
}

/// Which features to turn on, like Cargo's `--features` and `--no-default-features`
#[derive(Clone, Debug)]
pub struct Features<'a> {
    pub features: &'a [&'a str],
    pub default: bool,
}

impl Default for Features<'_> {
    fn default() -> Self {
        Self {
            features: &[],
            default: true,
        }
    }
}

impl Package {
    /// The package at the root of `storage`
    pub fn open(storage: Arc<dyn Storage>, features: Features) -> Result<Package> {
        let manifest = Manifest::parse(&storage.read_string(MANIFEST)?)?;
        if manifest.package.is_none() {
            return Err(BiblixError::Manifest(String::from(
                "no [package]: this is a vault's manifest",
            )));
        }
        let enabled = manifest.enable(features.features, features.default)?;
        let mut formats: Vec<(Format, &str)> = manifest
            .formats(&enabled)
            .into_iter()
            .filter_map(|(name, entry)| Some((Format::from_name(name)?, entry.path.as_str())))
            .filter(|(format, _)| format.readable())
            .collect();
        // The best one biblix reads (the enum's order)
        formats.sort();
        let Some(&(format, path)) = formats.first() else {
            let names: Vec<&str> = manifest.formats(&enabled).iter().map(|(n, _)| *n).collect();
            return Err(BiblixError::NoReadableFormat(names.join(", ")));
        };
        let dir = path.trim_end_matches('/');
        let source: Box<dyn Source> = match format {
            Format::Usx => Box::new(UsxSource::open(storage.clone(), dir)?),
            Format::Usfm => Box::new(UsfmSource::open(storage.clone(), dir)?),
            Format::Osis => Box::new(OsisSource::open(storage.clone(), dir)?),
            Format::Thml => Box::new(ThmlSource::open(storage.clone(), dir)?),
            _ => unreachable!("only readable formats are chosen"),
        };
        Ok(Package {
            manifest,
            storage,
            enabled,
            source,
        })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// `[package]`, which every opened package has
    pub fn info(&self) -> &manifest::Package {
        self.manifest.package.as_ref().expect("checked when opened")
    }

    pub fn enabled(&self) -> &Enabled {
        &self.enabled
    }

    /// The format being read
    pub fn source(&self) -> &dyn Source {
        self.source.as_ref()
    }

    /// The format being read, keeping nothing else (for apps that hold a source for a session)
    pub fn into_source(self) -> Box<dyn Source> {
        self.source
    }

    pub fn storage(&self) -> &Arc<dyn Storage> {
        &self.storage
    }

    pub fn books(&self) -> Vec<BookId> {
        self.source.books()
    }

    /// The checksum of every file in the package (see [`checksum`])
    pub fn checksum(&self) -> Result<String> {
        checksum(self.storage.as_ref())
    }
}

/**
A hash of every file in a package (`sha256:` and hex), except git's: it names this exact text, for
caches, lock files, and checking downloads
- Each file's path and length go in before its bytes, so moving bytes between files changes it
*/
pub fn checksum(storage: &dyn Storage) -> Result<String> {
    checksum_without(storage, |_| false)
}

/// [`checksum`], leaving out the files `skip` names
pub fn checksum_without(storage: &dyn Storage, skip: impl Fn(&str) -> bool) -> Result<String> {
    let mut paths = vec![];
    for name in storage.list("")? {
        match name.strip_suffix('/') {
            Some(".git") => {}
            Some(dir) => paths.extend(storage.walk(dir)?),
            None => paths.push(name),
        }
    }
    let mut hasher = Sha256::new();
    for path in paths.into_iter().filter(|p| !skip(p)) {
        let bytes = storage.read(&path)?;
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    let hex: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(format!("sha256:{hex}"))
}
