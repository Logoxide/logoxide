/*!
Where packages and caches are read from and written to: a folder on disk ([`FsStorage`]), memory
([`MemoryStorage`]), or whatever a host provides (an Obsidian vault, IndexedDB), so the same code
runs natively and in WebAssembly.

Paths are relative, `/`-separated, and never leave the storage's root.
*/

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::RwLock,
};

use crate::error::{BiblixError, Result};

pub trait Storage: Send + Sync {
    /// A file's bytes
    fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// The names of the files and folders in a folder (`""` is the root), sorted; folders end
    /// with `/`
    fn list(&self, dir: &str) -> Result<Vec<String>>;

    /// Writes a file, making its folders
    fn write(&self, _path: &str, _bytes: &[u8]) -> Result<()> {
        Err(BiblixError::ReadOnly)
    }

    /// Removes a file (one that isn't there is no error)
    fn remove(&self, _path: &str) -> Result<()> {
        Err(BiblixError::ReadOnly)
    }

    fn exists(&self, path: &str) -> bool {
        self.read(path).is_ok()
    }

    /// A file as text (a byte order mark is dropped)
    fn read_string(&self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        let text = String::from_utf8(bytes).map_err(|e| BiblixError::io(path, e))?;
        Ok(match text.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_string(),
            None => text,
        })
    }

    /// Every file under a folder, as paths from the root, sorted
    fn walk(&self, dir: &str) -> Result<Vec<String>> {
        let mut files = vec![];
        for name in self.list(dir)? {
            let path = join(dir, &name);
            match name.strip_suffix('/') {
                Some(_) => files.extend(self.walk(path.trim_end_matches('/'))?),
                None => files.push(path),
            }
        }
        Ok(files)
    }
}

/// `dir/name`, or `name` at the root
pub fn join(dir: &str, name: &str) -> String {
    match dir.trim_end_matches('/') {
        "" => name.to_string(),
        dir => format!("{dir}/{name}"),
    }
}

/// A path checked to stay inside the root: relative, without `..`
fn checked(path: &str) -> Result<&str> {
    let bad = path.starts_with('/')
        || path.contains('\\')
        || path.split('/').any(|part| part == ".." || part == ".");
    if bad {
        return Err(BiblixError::BadPath(path.to_string()));
    }
    Ok(path.trim_end_matches('/'))
}

/// A folder on disk
#[derive(Clone, Debug)]
pub struct FsStorage {
    root: PathBuf,
}

impl FsStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, path: &str) -> Result<PathBuf> {
        Ok(self.root.join(checked(path)?))
    }
}

impl Storage for FsStorage {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.path(path)?;
        std::fs::read(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BiblixError::NotFound(path.to_string()),
            _ => BiblixError::io(path, e),
        })
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let full = self.path(dir)?;
        let entries = std::fs::read_dir(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => BiblixError::NotFound(dir.to_string()),
            _ => BiblixError::io(dir, e),
        })?;
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let dir = entry.file_type().ok()?.is_dir();
                Some(if dir { format!("{name}/") } else { name })
            })
            .collect();
        names.sort();
        Ok(names)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let full = self.path(path)?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BiblixError::io(path, e))?;
        }
        // Written beside it, then moved into place, so a reader never sees half a file
        let partial = full.with_extension("partial");
        std::fs::write(&partial, bytes).map_err(|e| BiblixError::io(path, e))?;
        std::fs::rename(&partial, &full).map_err(|e| BiblixError::io(path, e))
    }

    /// Removes a file, then the folders it leaves empty (never the root)
    fn remove(&self, path: &str) -> Result<()> {
        let full = self.path(path)?;
        match std::fs::remove_file(&full) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(BiblixError::io(path, e)),
        }
        let mut dir = full.parent();
        while let Some(folder) = dir.filter(|d| *d != self.root && d.starts_with(&self.root)) {
            // Only an empty folder is removed
            if std::fs::remove_dir(folder).is_err() {
                break;
            }
            dir = folder.parent();
        }
        Ok(())
    }

    fn exists(&self, path: &str) -> bool {
        self.path(path).is_ok_and(|p| p.exists())
    }
}

/// Files in memory, for tests and for hosts that hand over a package's bytes
#[derive(Debug, Default)]
pub struct MemoryStorage {
    files: RwLock<BTreeMap<String, Vec<u8>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }

    /// With these files (path, contents)
    pub fn with<P: Into<String>, B: Into<Vec<u8>>>(
        files: impl IntoIterator<Item = (P, B)>,
    ) -> Self {
        let files = files
            .into_iter()
            .map(|(p, b)| (p.into(), b.into()))
            .collect();
        Self {
            files: RwLock::new(files),
        }
    }
}

impl Storage for MemoryStorage {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let path = checked(path)?;
        let files = self.files.read().map_err(|e| BiblixError::io(path, e))?;
        files
            .get(path)
            .cloned()
            .ok_or_else(|| BiblixError::NotFound(path.to_string()))
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let dir = checked(dir)?;
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let files = self.files.read().map_err(|e| BiblixError::io(dir, e))?;
        let mut names: Vec<String> = files
            .keys()
            .filter_map(|path| {
                let rest = path.strip_prefix(&prefix)?;
                Some(match rest.split_once('/') {
                    Some((folder, _)) => format!("{folder}/"),
                    None => rest.to_string(),
                })
            })
            .collect();
        names.dedup();
        if names.is_empty() && !dir.is_empty() {
            return Err(BiblixError::NotFound(dir.to_string()));
        }
        Ok(names)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let path = checked(path)?;
        let mut files = self.files.write().map_err(|e| BiblixError::io(path, e))?;
        files.insert(path.to_string(), bytes.to_vec());
        Ok(())
    }

    fn remove(&self, path: &str) -> Result<()> {
        let path = checked(path)?;
        let mut files = self.files.write().map_err(|e| BiblixError::io(path, e))?;
        files.remove(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_lists_and_walks() {
        let storage = MemoryStorage::with([
            ("biblix.toml", "x"),
            ("usx/JHN.usx", "y"),
            ("usx/a/b.usx", "z"),
        ]);
        assert_eq!(storage.list("").unwrap(), ["biblix.toml", "usx/"]);
        assert_eq!(storage.list("usx").unwrap(), ["JHN.usx", "a/"]);
        assert_eq!(
            storage.walk("").unwrap(),
            ["biblix.toml", "usx/JHN.usx", "usx/a/b.usx"]
        );
        assert!(matches!(storage.read("../x"), Err(BiblixError::BadPath(_))));
        assert!(matches!(
            storage.read("nope"),
            Err(BiblixError::NotFound(_))
        ));
        storage.write("cache/x.bin", b"1").unwrap();
        assert_eq!(storage.read("cache/x.bin").unwrap(), b"1");
    }

    #[test]
    fn fs_reads_writes_and_lists() {
        let dir = std::env::temp_dir().join(format!("biblix-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let storage = FsStorage::new(&dir);
        storage.write("a/b.txt", b"hello").unwrap();
        assert_eq!(storage.read_string("a/b.txt").unwrap(), "hello");
        assert_eq!(storage.list("").unwrap(), ["a/"]);
        assert_eq!(storage.walk("").unwrap(), ["a/b.txt"]);
        assert!(storage.exists("a/b.txt") && !storage.exists("a/c.txt"));
        assert!(matches!(
            storage.read("/etc/passwd"),
            Err(BiblixError::BadPath(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
