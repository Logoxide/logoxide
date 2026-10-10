/*!
Exclusive advisory locks on the library and on a project, so two `lx` processes (an install and
a clean, two installs from two vaults sharing one library) don't write over each other.

A lock is taken for the time a command writes: installing (resolving through writing the lock
file), repairing, cleaning, and writing the manifest; readers take none, since a version is
renamed into place whole and the manifest and lock are written whole. Each lock is a file in
[`lock_dir`] (`$XDG_RUNTIME_DIR/logoxide/`, else `locks/` in the default cache folder) named by
a hash of the folder's canonical path, never in the library itself, which a sync service may
copy. The lock is the operating system's (`std::fs::File::lock`), so a process that dies
releases it; the file names the holder for the message when another has to give up.

Taking a lock waits up to [`LOCK_TIMEOUT`], then fails with [`LogoxideError::Busy`]. Several
locks are taken in one fixed order ([`lock_all`]), so two processes can't each wait for the
other's. In WebAssembly there is no file system to lock, and every lock is a no-op.
*/

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::error::{LogoxideError, Result};

/// How long taking a lock waits for whoever holds it
pub const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// How often a held lock is tried again
const POLL: Duration = Duration::from_millis(50);

/// A lock held while it lives; dropping it releases the lock
#[derive(Debug)]
#[must_use = "the lock is released when the guard is dropped"]
pub struct Guard {
    #[cfg(not(target_arch = "wasm32"))]
    _file: Option<std::fs::File>,
}

/// Where lock files go: `$XDG_RUNTIME_DIR/logoxide`, else `locks/` under the default cache
/// folder (`LOGOXIDE_CACHE`, `$XDG_CACHE_HOME/logoxide`); never the library, which may be synced
pub fn lock_dir() -> PathBuf {
    crate::library::env_dir("XDG_RUNTIME_DIR")
        .map(|d| d.join("logoxide"))
        .unwrap_or_else(|| crate::Library::default_cache().join("locks"))
}

/// The lock file for a folder in `dir`: the folder's canonical path (or absolute, if it isn't
/// there yet) hashed, so every process names the same file for one folder however it was spelled
pub fn lock_path_in(dir: &Path, folder: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let canonical = folder
        .canonicalize()
        .or_else(|_| std::path::absolute(folder))
        .unwrap_or_else(|_| folder.to_path_buf());
    let hash = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let hex: String = hash.iter().take(16).map(|b| format!("{b:02x}")).collect();
    dir.join(format!("{hex}.lock"))
}

/// The lock file for a folder, in [`lock_dir`]
pub fn lock_path(folder: &Path) -> PathBuf {
    lock_path_in(&lock_dir(), folder)
}

/// Takes the exclusive lock on `folder` (`what` names it in errors: "the library at"),
/// waiting up to [`LOCK_TIMEOUT`]
pub fn lock(folder: &Path, what: &str) -> Result<Guard> {
    lock_with(folder, what, LOCK_TIMEOUT)
}

/// [`lock`] with its own wait
pub fn lock_with(folder: &Path, what: &str, timeout: Duration) -> Result<Guard> {
    lock_file(
        &lock_path(folder),
        &format!("{what} {}", folder.display()),
        timeout,
    )
}

/// Takes the locks on several folders, in one fixed order (by lock file), each once: the
/// library and a project that is the library's own folder are one lock
pub fn lock_all(folders: &[(&Path, &str)]) -> Result<Vec<Guard>> {
    let mut wanted: Vec<(PathBuf, &Path, &str)> = folders
        .iter()
        .map(|(folder, what)| (lock_path(folder), *folder, *what))
        .collect();
    wanted.sort_by(|a, b| a.0.cmp(&b.0));
    wanted.dedup_by(|a, b| a.0 == b.0);
    wanted
        .iter()
        .map(|(path, folder, what)| {
            lock_file(path, &format!("{what} {}", folder.display()), LOCK_TIMEOUT)
        })
        .collect()
}

/// Takes the lock that is the file at `path` (`shown` is what it guards, for the error),
/// waiting up to `timeout`
#[cfg(not(target_arch = "wasm32"))]
pub fn lock_file(path: &Path, shown: &str, timeout: Duration) -> Result<Guard> {
    use std::io::{Read, Seek, Write};

    let name = path.display().to_string();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| LogoxideError::io(&name, e))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| LogoxideError::io(&name, e))?;
    let started = std::time::Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(e)) => return Err(LogoxideError::io(&name, e)),
        }
        if started.elapsed() >= timeout {
            let mut holder = String::new();
            let _ = file.read_to_string(&mut holder);
            let holder = match holder.trim() {
                "" => String::from("another process"),
                who => format!("another process ({})", crate::text::printable(who)),
            };
            return Err(LogoxideError::Busy {
                what: shown.to_string(),
                message: format!(
                    "in use by {holder} for more than {} s: try again when it's done",
                    timeout.as_secs()
                ),
            });
        }
        std::thread::sleep(POLL);
    }
    // Who holds it, for the message another process gives up with
    let _ = file
        .set_len(0)
        .and_then(|()| file.rewind())
        .and_then(|()| file.write_all(holder_name().as_bytes()));
    Ok(Guard { _file: Some(file) })
}

#[cfg(target_arch = "wasm32")]
pub fn lock_file(_path: &Path, _shown: &str, _timeout: Duration) -> Result<Guard> {
    Ok(Guard {})
}

/// This process, as the lock file names it: `lx install, pid 1234`
#[cfg(not(target_arch = "wasm32"))]
fn holder_name() -> String {
    let words: Vec<String> = std::env::args()
        .take(2)
        .enumerate()
        .map(|(i, arg)| match i {
            0 => Path::new(&arg)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or(arg),
            _ => arg,
        })
        .filter(|a| !a.starts_with('-'))
        .collect();
    format!("{}, pid {}", words.join(" "), std::process::id())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn a_held_lock_makes_the_next_taker_busy() {
        let dir = std::env::temp_dir().join(format!("logoxide-locking-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let held = lock(&dir, "the library at").unwrap();
        let error = lock_with(&dir, "the library at", Duration::from_millis(120)).unwrap_err();
        assert!(matches!(error, LogoxideError::Busy { .. }), "{error}");
        let text = error.to_string();
        assert!(
            text.contains("in use by another process (") && text.contains("pid"),
            "{text}"
        );
        drop(held);
        let _again = lock_with(&dir, "the library at", Duration::from_millis(120)).unwrap();
        // One folder spelled two ways is one lock, and a folder that isn't there yet has one
        assert_eq!(lock_path(&dir), lock_path(&dir.join(".")));
        assert_ne!(lock_path(&dir), lock_path(&dir.join("new")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn locks_on_one_folder_are_taken_once() {
        let dir = std::env::temp_dir().join(format!("logoxide-lock-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let guards = lock_all(&[(&dir, "the library at"), (&dir, "the project at")]).unwrap();
        assert_eq!(guards.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
