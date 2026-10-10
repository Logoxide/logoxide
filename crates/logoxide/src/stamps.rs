/*!
Checksums remembered by each file's size and modification time, so a package isn't hashed whole
every time it's opened: hashing the KJV's files takes a noticeable part of a second, and nothing
about them changes between one `lx` run and the next.

A stamp file (`<cache>/stamps/<id>/<version>-<hash of where the package is>.postcard`) holds
the checksum last computed and, for every declared file, the [`Stamp`] it had then. The
remembered checksum is used only when the declared files are the same paths, every stamp
matches, and each file was last changed at least [`RACY`] (2 s) before the stamp file was
written: a file written in the same second as the stamps, after they were taken, could keep its
size and time (git's rule for racy files). Otherwise the files are hashed again and the stamps
written anew. `lx verify`, `lx install --repair`, and `lx release` never go through this: they
hash. A storage that can't give stamps (memory, a host's files in WebAssembly) is always hashed.
*/

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    package::checksum_of,
    storage::{Stamp, Storage},
};

/// How much older than the stamp file a file's modification time must be for its stamp to be
/// trusted (git's rule for racy files)
pub const RACY: Duration = Duration::from_secs(2);

/// What's remembered for a package at one place
#[derive(Debug, Serialize, Deserialize)]
struct Stamps {
    /// When they were taken, as seconds and nanoseconds since the epoch
    written: (u64, u32),
    checksum: String,
    /// Each declared file, in the order hashed, with its stamp
    files: Vec<(String, Stamp)>,
}

/// The stamp file for a package at a place: `<id>/<version>-<hash of the location>.postcard`
pub fn stamps_path(id: &str, version: &str, location: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(location.as_bytes());
    let hex: String = hash.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{id}/{version}-{hex}.postcard")
}

fn now() -> (u64, u32) {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (since.as_secs(), since.subsec_nanos())
}

/// The stamps every declared file has now, if the storage gives them
fn current(storage: &dyn Storage, declared: &[String]) -> Option<Vec<(String, Stamp)>> {
    declared
        .iter()
        .map(|path| storage.stamp(path).map(|s| (path.clone(), s)))
        .collect()
}

/// The checksum remembered for these files, when every stamp still matches and none is racy
fn remembered(store: &dyn Storage, path: &str, files: &[(String, Stamp)]) -> Option<String> {
    let bytes = store.read(path).ok()?;
    let stamps: Stamps = postcard::from_bytes(&bytes).ok()?;
    if stamps.files != files {
        return None;
    }
    let written = UNIX_EPOCH + Duration::new(stamps.written.0, stamps.written.1);
    let settled = files.iter().all(|(_, stamp)| {
        let modified = UNIX_EPOCH + Duration::new(stamp.modified.0, stamp.modified.1);
        written
            .duration_since(modified)
            .is_ok_and(|age| age >= RACY)
    });
    settled.then_some(stamps.checksum)
}

/**
The checksum of a package's declared files, remembered in `store` under `path` by their stamps
when it can be, else hashed now and remembered for next time. `location` is where the package
is (its folder), which keys the stamps; a storage without one is hashed every time
*/
pub fn checksum(
    store: Option<&dyn Storage>,
    storage: &dyn Storage,
    declared: &[String],
    path: &str,
) -> Result<String> {
    let files = store.and_then(|_| current(storage, declared));
    if let (Some(store), Some(files)) = (store, &files)
        && let Some(sum) = remembered(store, path, files)
    {
        return Ok(sum);
    }
    let sum = checksum_of(storage, declared)?;
    if let (Some(store), Some(files)) = (store, files) {
        let stamps = Stamps {
            written: now(),
            checksum: sum.clone(),
            files,
        };
        // A store that can't be written is just no memory for next time
        if let Ok(bytes) = postcard::to_stdvec(&stamps) {
            let _ = store.write(path, &bytes);
        }
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FsStorage, MemoryStorage};

    /// A fixed time well in the past, so a file set to it has the same stamp every time
    const OLD: Duration = Duration::from_secs(1_700_000_000);

    fn set_modified(path: &std::path::Path, ago: Duration) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + ago).unwrap();
    }

    #[test]
    fn checksums_are_remembered_while_stamps_hold_and_arent_racy() {
        let dir = std::env::temp_dir().join(format!("logoxide-stamps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let package = FsStorage::new(dir.join("package"));
        package.write("logoxide.toml", b"[package]\n").unwrap();
        package.write("usx/JHN.usx", b"john").unwrap();
        let declared = vec![String::from("logoxide.toml"), String::from("usx/JHN.usx")];
        for path in &declared {
            set_modified(&dir.join("package").join(path), OLD);
        }
        let store = MemoryStorage::new();
        let key = stamps_path("kjv", "1.0.0", "/somewhere");
        let first = checksum(Some(&store), &package, &declared, &key).unwrap();
        assert!(store.exists(&key));
        // Remembered: the files changed under the stamps aren't seen (the known limit), so
        // the remembered checksum is given back even though a file's bytes differ
        std::fs::write(dir.join("package/usx/JHN.usx"), b"JOHN").unwrap();
        set_modified(&dir.join("package/usx/JHN.usx"), OLD);
        assert_eq!(
            checksum(Some(&store), &package, &declared, &key).unwrap(),
            first
        );
        // A changed size, or a changed time, is hashed again
        package.write("usx/JHN.usx", b"john and more").unwrap();
        set_modified(&dir.join("package/usx/JHN.usx"), OLD);
        let second = checksum(Some(&store), &package, &declared, &key).unwrap();
        assert_ne!(second, first);
        assert_eq!(second, checksum_of(&package, &declared).unwrap());
        // A file changed within the racy window of the stamps is never trusted
        std::fs::write(dir.join("package/usx/JHN.usx"), b"john and more").unwrap();
        let third = checksum(Some(&store), &package, &declared, &key).unwrap();
        assert_eq!(third, second);
        std::fs::write(dir.join("package/usx/JHN.usx"), b"JOHN AND MORE").unwrap();
        assert_ne!(
            checksum(Some(&store), &package, &declared, &key).unwrap(),
            third
        );
        // Without a store, or with a storage that gives no stamps, it's hashed every time
        let memory = MemoryStorage::with([("logoxide.toml", "x")]);
        let one = vec![String::from("logoxide.toml")];
        assert_eq!(
            checksum(Some(&store), &memory, &one, &key).unwrap(),
            checksum_of(&memory, &one).unwrap()
        );
        assert_eq!(
            checksum(None, &package, &declared, &key).unwrap(),
            checksum_of(&package, &declared).unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
