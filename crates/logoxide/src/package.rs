/*!
A package: a `logoxide.toml` with `[package]`, and its text in one or more formats. Opening one
picks the best format its enabled features turn on (USX, then USFM, OSIS, and ThML).
*/

use std::sync::Arc;

use sha2::{Digest, Sha256};
use topos_bible::data::books::BookId;

use crate::{
    error::{LogoxideError, Result},
    manifest::{self, Enabled, Format, MANIFEST, Manifest},
    source::{NoText, OsisSource, Source, ThmlSource, UsfmSource, UsxSource},
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
            return Err(LogoxideError::Manifest(String::from(
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
        // The best one logoxide reads (the enum's order)
        formats.sort();
        let Some(&(format, path)) = formats.first() else {
            let enabled_formats = manifest.formats(&enabled);
            // A Bible is read as verses; a book or a collection can have none, and so can a
            // recording that reads another package's text ([media] text)
            let kind = manifest
                .package
                .as_ref()
                .map(|p| p.kind)
                .unwrap_or_default();
            let recording = (manifest.formats.is_empty())
                && (manifest.media.as_ref()).is_some_and(crate::media::Media::names_its_text);
            if kind == manifest::PackageKind::Bible && !recording {
                let names: Vec<&str> = enabled_formats.iter().map(|(n, _)| *n).collect();
                return Err(LogoxideError::NoReadableFormat(names.join(", ")));
            }
            let format = (enabled_formats.iter()).find_map(|(name, _)| Format::from_name(name));
            return Ok(Package {
                manifest,
                storage,
                enabled,
                source: Box::new(NoText::new(format)),
            });
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

    /// The checksum of the package's files (see [`checksum`])
    pub fn checksum(&self) -> Result<String> {
        checksum(self.storage.as_ref())
    }
}

/// The files a package is made of: what its manifest declares, so other files in its folder (a
/// reader's annotations beside a book, `.DS_Store`) are never part of it. In path order:
/// - `logoxide.toml`, its license file and README, and the summary's files
/// - each format's files, by the format's extensions (`usx/*.usx`; a format logoxide doesn't read,
///   like SWORD, is every file under its path)
/// - the media's timings (`*.vtt` under `[media] timings`, and each file's own `timings`)
/// - what `include` matches (`README.md`, `images/**`)
pub fn declared_files(storage: &dyn Storage) -> Result<Vec<String>> {
    let manifest = Manifest::parse(&storage.read_string(MANIFEST)?)?;
    let mut all = vec![];
    for name in storage.list("")? {
        match name.strip_suffix('/') {
            Some(".git") => {}
            Some(dir) => all.extend(storage.walk(dir)?),
            None => all.push(name),
        }
    }
    let mut files: Vec<String> = all
        .into_iter()
        .filter(|path| declares(&manifest, path))
        .collect();
    files.sort();
    Ok(files)
}

/// Whether a manifest declares a path as one of its package's files
pub fn declares(manifest: &Manifest, path: &str) -> bool {
    let under = |dir: &str| {
        let dir = dir.trim_matches('/');
        dir.is_empty() || path == dir || path.starts_with(&format!("{dir}/"))
    };
    let extension = path
        .rsplit_once('/')
        .map_or(path, |(_, name)| name)
        .rsplit_once('.')
        .map(|(_, e)| e);
    let has = |extensions: &[&str]| {
        extension.is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
    };
    let info = manifest.package.as_ref();
    path == MANIFEST
        || crate::summary::is_summary_file(path)
        || info.and_then(|i| i.license_file.as_deref()) == Some(path)
        || info.and_then(|i| i.readme.as_deref()) == Some(path)
        || manifest.formats.iter().any(|(name, entry)| {
            under(&entry.path)
                && (Format::from_name(name).and_then(Format::extensions))
                    // A format whose path is the file itself
                    .is_none_or(|extensions| has(extensions) || path == entry.path)
        })
        || (manifest.media.as_ref())
            .and_then(|m| m.timings.as_deref())
            .is_some_and(|dir| under(dir) && has(&["vtt"]))
        || (manifest.media.iter())
            .flat_map(|m| &m.files)
            .any(|f| f.timings.as_deref() == Some(path))
        || info.is_some_and(|i| i.include.iter().any(|pattern| glob(pattern, path)))
}

/// Whether `path` matches a pattern: `*` is any characters but `/`, `?` one of them, `**` any
/// folders (or none), and a pattern ending in `/` is everything under that folder
pub fn glob(pattern: &str, path: &str) -> bool {
    if let Some(dir) = pattern.strip_suffix('/') {
        return path.starts_with(&format!("{dir}/"));
    }
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    glob_parts(&pattern, &path)
}

/// The path's parts against the pattern's: `**` is any number of parts, any other part one
fn glob_parts(pattern: &[&str], path: &[&str]) -> bool {
    wildcard(
        pattern,
        path,
        |part| *part == "**",
        |part, name| glob_name(part.as_bytes(), name.as_bytes()),
    )
}

/// A name against a part of the pattern: `*` is any characters, `?` one
fn glob_name(pattern: &[u8], name: &[u8]) -> bool {
    wildcard(pattern, name, |b| *b == b'*', |p, c| *p == b'?' || p == c)
}

/**
Whether `text` matches `pattern`, where `star` items match any run of items (or none) and each
other item matches one item as `one` says. Each star's run grows only when what follows fails,
from the latest star alone (earlier stars' runs never need to change), so it takes at most
pattern × text steps, however many stars there are
*/
fn wildcard<P, T>(
    pattern: &[P],
    text: &[T],
    star: impl Fn(&P) -> bool,
    one: impl Fn(&P, &T) -> bool,
) -> bool {
    let (mut p, mut t) = (0, 0);
    // The latest star, and where its run ends
    let mut last: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some(item) if star(item) => {
                last = Some((p, t));
                p += 1;
            }
            Some(item) if one(item, &text[t]) => {
                p += 1;
                t += 1;
            }
            _ => match last {
                // The latest star takes one more item, and what follows it is tried again
                Some((star_at, run_end)) => {
                    last = Some((star_at, run_end + 1));
                    p = star_at + 1;
                    t = run_end + 1;
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(star)
}

/// A hash of a package's files ([`declared_files`]; `sha256:` and hex): it names this exact text,
/// for caches, lock files, and checking downloads. Files a reader added beside them don't change it
/// - Each file's path and length go in before its bytes, so moving bytes between files changes it
pub fn checksum(storage: &dyn Storage) -> Result<String> {
    checksum_of(storage, &declared_files(storage)?)
}

/// The files in a package's folder that aren't the package's (annotations, notes): kept by
/// repairs, and they stop `lx clean` from removing the folder
pub fn other_files(storage: &dyn Storage) -> Result<Vec<String>> {
    let declared = declared_files(storage)?;
    let mut others = vec![];
    for name in storage.list("")? {
        let paths = match name.strip_suffix('/') {
            Some(".git") => continue,
            Some(dir) => storage.walk(dir)?,
            None => vec![name],
        };
        others.extend(paths.into_iter().filter(|p| !declared.contains(p)));
    }
    others.sort();
    Ok(others)
}

/// A hash of these files (in this order), as [`checksum`] makes it
pub fn checksum_of(storage: &dyn Storage, paths: &[String]) -> Result<String> {
    let mut hasher = Sha256::new();
    for path in paths {
        let bytes = storage.read(path)?;
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

/// A copy of a package's declared files alone ([`declared_files`])
pub fn only_declared(storage: &dyn Storage) -> Result<crate::MemoryStorage> {
    let copy = crate::MemoryStorage::new();
    for path in declared_files(storage)? {
        copy.write(&path, &storage.read(&path)?)?;
    }
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStorage;

    #[test]
    fn globs() {
        assert!(glob("README.md", "README.md"));
        assert!(!glob("README.md", "docs/README.md"));
        assert!(glob("*.md", "NOTES.md"));
        assert!(!glob("*.md", "docs/NOTES.md"));
        assert!(glob("images/**", "images/a/b.png"));
        assert!(glob("**/*.png", "a.png"));
        assert!(glob("**/*.png", "x/y/a.png"));
        assert!(glob("ch?.txt", "ch1.txt"));
        assert!(glob("extras/", "extras/a/b"));
        assert!(!glob("extras/", "extras"));
        assert!(glob("**", "a/b/c"));
        assert!(glob("a/**/c/*.txt", "a/c/x.txt"));
        assert!(glob("a/**/c/*.txt", "a/b/b/c/x.txt"));
        assert!(!glob("a/**/c/*.txt", "a/b/c/d/x.txt"));
        assert!(glob("*a*b", "xaxxab"));
        assert!(!glob("*a*b", "xaxxa"));
        assert!(!glob("ch?.txt", "ch.txt"));
        assert!(glob("**/x/**/y", "x/y"));
        assert!(!glob("**/x/**/y", "x/z"));
    }

    #[test]
    fn pathological_globs_finish_fast() {
        // Each of these took exponential time when matched by trying every split
        let start = std::time::Instant::now();
        let name = "a".repeat(200);
        assert!(!glob(&format!("{}b", "*a".repeat(30)), &name));
        let path = vec!["a"; 60].join("/");
        let pattern = format!("{}b", "**/a/".repeat(25));
        assert!(!glob(&pattern, &path));
        assert!(glob(&vec!["**"; 25].join("/"), &path));
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn a_package_is_what_it_declares() {
        let files = MemoryStorage::new();
        let manifest = "[package]\nid = \"t\"\nname = \"T\"\nabbreviation = \"T\"\nversion = \"1.0.0\"\nlanguage = \"en\"\nlicense-file = \"LICENSE.txt\"\ninclude = [\"images/**\"]\n\n[formats]\nusx = { path = \"usx/\" }\nsword = { path = \"sword/\" }\n\n[media]\ntimings = \"timings/\"\n";
        for (path, text) in [
            ("logoxide.toml", manifest),
            ("LICENSE.txt", "free"),
            ("README.md", "not declared"),
            ("usx/JHN.usx", "<usx/>"),
            ("usx/JHN - Annotations.md", "a reader's note"),
            ("sword/mods.d/t.conf", "[T]"),
            ("timings/JHN.vtt", "WEBVTT"),
            ("timings/notes.md", "not a timing"),
            ("images/a/map.png", "png"),
            (".git/HEAD", "ref"),
        ] {
            files.write(path, text.as_bytes()).unwrap();
        }
        assert_eq!(
            declared_files(&files).unwrap(),
            [
                "LICENSE.txt",
                "images/a/map.png",
                "logoxide.toml",
                "sword/mods.d/t.conf",
                "timings/JHN.vtt",
                "usx/JHN.usx"
            ]
        );
        assert_eq!(
            other_files(&files).unwrap(),
            ["README.md", "timings/notes.md", "usx/JHN - Annotations.md"]
        );
        // A note beside a book doesn't change the checksum; a changed book does
        let before = checksum(&files).unwrap();
        files.write("usx/JHN - Annotations.md", b"more").unwrap();
        assert_eq!(checksum(&files).unwrap(), before);
        files.write("usx/JHN.usx", b"<usx>changed</usx>").unwrap();
        assert_ne!(checksum(&files).unwrap(), before);
    }
}
