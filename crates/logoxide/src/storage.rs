/*!
Where packages and caches are read from and written to: a folder on disk ([`FsStorage`]), memory
([`MemoryStorage`]), or whatever a host provides (an Obsidian vault, IndexedDB), so the same code
runs natively and in WebAssembly.

Paths are relative, `/`-separated, and never leave the storage's root.
*/

use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::error::{LogoxideError, Result};

pub trait Storage: Send + Sync {
    /// A file's bytes
    fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// The names of the files and folders in a folder (`""` is the root), sorted; folders end
    /// with `/`
    fn list(&self, dir: &str) -> Result<Vec<String>>;

    /// Writes a file, making its folders
    fn write(&self, _path: &str, _bytes: &[u8]) -> Result<()> {
        Err(LogoxideError::ReadOnly)
    }

    /// Removes a file (one that isn't there is no error)
    fn remove(&self, _path: &str) -> Result<()> {
        Err(LogoxideError::ReadOnly)
    }

    fn exists(&self, path: &str) -> bool {
        self.read(path).is_ok()
    }

    /// A file as text (a byte order mark is dropped)
    fn read_string(&self, path: &str) -> Result<String> {
        let bytes = self.read(path)?;
        let text = String::from_utf8(bytes).map_err(|e| LogoxideError::io_message(path, e))?;
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
        return Err(LogoxideError::BadPath(path.to_string()));
    }
    Ok(path.trim_end_matches('/'))
}

/// A folder on disk
#[derive(Clone, Debug)]
pub struct FsStorage {
    root: PathBuf,
    /// Links under the root are never followed ([`FsStorage::no_symlinks`])
    no_symlinks: bool,
}

impl FsStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            no_symlinks: false,
        }
    }

    /**
    The same folder, but never through a link under its root (the root itself may be one): a
    path with a link on the way, or that is one, can't be read, written, or removed, and folders
    are listed without their links. For what is published or written over from elsewhere (`lx
    publish`, an import's update), where a link could bring in, or write to, a file anywhere
    */
    pub fn no_symlinks(mut self) -> Self {
        self.no_symlinks = true;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// With [`FsStorage::no_symlinks`], refuses a path that is a link or has one on the way
    /// (checked with `symlink_metadata`, part by part; a part not there ends the check)
    fn unlinked(&self, path: &str) -> Result<()> {
        if !self.no_symlinks {
            return Ok(());
        }
        let mut at = self.root.clone();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            at.push(part);
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(LogoxideError::io_message(
                        path,
                        "a link, which isn't followed here",
                    ));
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Ok(())
    }

    /**
    A path's place under the root. Besides [`checked`]'s rules, each part is a plain name: no
    `:` (a Windows drive, `C:x`, or an alternate stream), no NUL, and nothing the platform reads
    as a root, a prefix, or a step up
    */
    fn path(&self, path: &str) -> Result<PathBuf> {
        let path = checked(path)?;
        let plain = !path.contains([':', '\0'])
            && (Path::new(path).components()).all(|c| matches!(c, Component::Normal(_)));
        if !plain {
            return Err(LogoxideError::BadPath(path.to_string()));
        }
        Ok(self.root.join(path))
    }
}

/// A name for a file being written, beside it and unlike any other's: `.JHN.usx.<pid>.<n>.tmp`
fn staging_path(full: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = full
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    full.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

/**
Writes a file so that a reader sees the old file or the whole new one, never half, and a crash
or a power cut loses neither: the bytes go to a file beside it with a name of its own
([`staging_path`]), are synced to disk, renamed over the old file, and the folder is synced so the
rename itself lasts. Folders on the way are made. For the manifest, the lock, catalogs, and
whatever else is written over in place; [`FsStorage::write`] goes through it
*/
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&parent)?;
    let partial = staging_path(path);
    let written = (|| {
        let mut file = std::fs::File::create(&partial)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&partial, path)?;
        sync_dir(&parent)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written
}

/// Syncs a folder's entries to disk, so a rename in it lasts (where folders can be opened: on
/// Windows they can't, and the rename is durable as the file system makes it)
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

impl Storage for FsStorage {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.path(path)?;
        self.unlinked(path)?;
        std::fs::read(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => LogoxideError::NotFound(path.to_string()),
            _ => LogoxideError::io(path, e),
        })
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let full = self.path(dir)?;
        self.unlinked(dir)?;
        let entries = std::fs::read_dir(&full).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => LogoxideError::NotFound(dir.to_string()),
            _ => LogoxideError::io(dir, e),
        })?;
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let kind = entry.file_type().ok()?;
                if self.no_symlinks && kind.is_symlink() {
                    return None;
                }
                let dir = kind.is_dir();
                Some(if dir { format!("{name}/") } else { name })
            })
            .collect();
        names.sort();
        Ok(names)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let full = self.path(path)?;
        self.unlinked(path)?;
        // Written beside it, synced, then moved into place, so a reader never sees half a file
        // (a name of its own, so no other file, like `JHN.partial`, is written over)
        write_atomic(&full, bytes).map_err(|e| LogoxideError::io(path, e))
    }

    /// Removes a file, then the folders it leaves empty (never the root)
    fn remove(&self, path: &str) -> Result<()> {
        let full = self.path(path)?;
        self.unlinked(path)?;
        match std::fs::remove_file(&full) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(LogoxideError::io(path, e)),
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
        self.path(path).is_ok_and(|p| p.exists()) && self.unlinked(path).is_ok()
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
        let files = self
            .files
            .read()
            .map_err(|e| LogoxideError::io_message(path, e))?;
        files
            .get(path)
            .cloned()
            .ok_or_else(|| LogoxideError::NotFound(path.to_string()))
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let dir = checked(dir)?;
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let files = self
            .files
            .read()
            .map_err(|e| LogoxideError::io_message(dir, e))?;
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
            return Err(LogoxideError::NotFound(dir.to_string()));
        }
        Ok(names)
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<()> {
        let path = checked(path)?;
        let mut files = self
            .files
            .write()
            .map_err(|e| LogoxideError::io_message(path, e))?;
        files.insert(path.to_string(), bytes.to_vec());
        Ok(())
    }

    fn remove(&self, path: &str) -> Result<()> {
        let path = checked(path)?;
        let mut files = self
            .files
            .write()
            .map_err(|e| LogoxideError::io_message(path, e))?;
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
            ("logoxide.toml", "x"),
            ("usx/JHN.usx", "y"),
            ("usx/a/b.usx", "z"),
        ]);
        assert_eq!(storage.list("").unwrap(), ["logoxide.toml", "usx/"]);
        assert_eq!(storage.list("usx").unwrap(), ["JHN.usx", "a/"]);
        assert_eq!(
            storage.walk("").unwrap(),
            ["logoxide.toml", "usx/JHN.usx", "usx/a/b.usx"]
        );
        assert!(matches!(
            storage.read("../x"),
            Err(LogoxideError::BadPath(_))
        ));
        assert!(matches!(
            storage.read("nope"),
            Err(LogoxideError::NotFound(_))
        ));
        storage.write("cache/x.bin", b"1").unwrap();
        assert_eq!(storage.read("cache/x.bin").unwrap(), b"1");
    }

    #[test]
    fn fs_reads_writes_and_lists() {
        let dir = std::env::temp_dir().join(format!("logoxide-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let storage = FsStorage::new(&dir);
        storage.write("a/b.txt", b"hello").unwrap();
        assert_eq!(storage.read_string("a/b.txt").unwrap(), "hello");
        assert_eq!(storage.list("").unwrap(), ["a/"]);
        assert_eq!(storage.walk("").unwrap(), ["a/b.txt"]);
        assert!(storage.exists("a/b.txt") && !storage.exists("a/c.txt"));
        assert!(matches!(
            storage.read("/etc/passwd"),
            Err(LogoxideError::BadPath(_))
        ));
        for bad in ["C:x", "a/C:/b", "a/b:stream", "a/\0"] {
            assert!(
                matches!(storage.write(bad, b"x"), Err(LogoxideError::BadPath(_))),
                "{bad}"
            );
        }
        // Writing never touches a file named like a staging one
        storage.write("a/JHN.partial", b"mine").unwrap();
        storage.write("a/JHN.usx", b"book").unwrap();
        assert_eq!(storage.read_string("a/JHN.partial").unwrap(), "mine");
        assert_eq!(
            storage.walk("a").unwrap(),
            ["a/JHN.partial", "a/JHN.usx", "a/b.txt"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_writes_replace_and_leave_nothing_beside() {
        let dir = std::env::temp_dir().join(format!("logoxide-atomic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("deep/logoxide.toml");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let names: Vec<String> = (std::fs::read_dir(dir.join("deep")).unwrap())
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["logoxide.toml"]);
        // A path that can't be written leaves no staging file either
        std::fs::write(dir.join("file"), "x").unwrap();
        assert!(write_atomic(&dir.join("file/under"), b"x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn links_can_be_refused() {
        let dir = std::env::temp_dir().join(format!("logoxide-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (root, outside) = (dir.join("package"), dir.join("outside"));
        std::fs::create_dir_all(root.join("usx")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), "secret").unwrap();
        std::fs::write(root.join("usx/JHN.usx"), "john").unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), root.join("usx/ROM.usx")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("away")).unwrap();
        // Followed as before
        let open = FsStorage::new(&root);
        assert_eq!(open.read_string("usx/ROM.usx").unwrap(), "secret");
        assert_eq!(
            open.walk("").unwrap(),
            ["away", "usx/JHN.usx", "usx/ROM.usx"]
        );
        // Never followed: not listed, not read, not written or removed through
        let strict = FsStorage::new(&root).no_symlinks();
        assert_eq!(strict.walk("").unwrap(), ["usx/JHN.usx"]);
        assert!(strict.read("usx/ROM.usx").is_err());
        assert!(strict.read("away/secret").is_err());
        assert!(!strict.exists("away/secret") && strict.exists("usx/JHN.usx"));
        assert!(strict.write("away/new", b"x").is_err());
        assert!(strict.remove("away/secret").is_err());
        assert!(!outside.join("new").exists() && outside.join("secret").exists());
        strict.write("usx/new/MAT.usx", b"matthew").unwrap();
        assert_eq!(strict.read_string("usx/new/MAT.usx").unwrap(), "matthew");
        // The root may be a link itself
        let linked = dir.join("linked");
        std::os::unix::fs::symlink(&root, &linked).unwrap();
        assert_eq!(
            FsStorage::new(&linked)
                .no_symlinks()
                .read_string("usx/JHN.usx")
                .unwrap(),
            "john"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
