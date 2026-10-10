//! Archives a text comes in: a `.zip` (an EPUB is one), read within limits so a small file
//! can't fill memory.

use std::io::Read;

use crate::error::{Result, TextError};

/// The most an archive may unpack to (1 GiB)
pub const MAX_UNPACKED_BYTES: u64 = 1 << 30;
/// The most one file in an archive may be (1 GiB)
pub const MAX_ENTRY_BYTES: u64 = 1 << 30;
/// The most entries an archive may have
pub const MAX_ENTRIES: usize = 200_000;

/// Each file in a `.zip`, by path (folders left out)
pub fn unzip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    let bad = |e: &dyn std::fmt::Display| TextError::Archive(format!("not a .zip: {e}"));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| bad(&e))?;
    if archive.len() > MAX_ENTRIES {
        return Err(TextError::Archive(format!(
            "more than {MAX_ENTRIES} files in the .zip"
        )));
    }
    let mut files = vec![];
    let mut total = 0;
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| bad(&e))?;
        if !file.is_file() {
            continue;
        }
        let Some(path) = file
            .enclosed_name()
            .and_then(|p| p.to_str().map(String::from))
        else {
            continue;
        };
        let size = file.size();
        let data = read_limited(file, size, &mut total, &path)?;
        files.push((path.replace('\\', "/"), data));
    }
    Ok(files)
}

/// Reads one entry's bytes (its size said to be `size`) within the limits, counting them in
/// `total`; an entry that goes past what it said is refused
fn read_limited(reader: impl Read, size: u64, total: &mut u64, path: &str) -> Result<Vec<u8>> {
    if size > MAX_ENTRY_BYTES {
        return Err(TextError::Archive(format!(
            "{path}: {size} bytes is more than one file may be"
        )));
    }
    if *total + size > MAX_UNPACKED_BYTES {
        return Err(TextError::Archive(format!(
            "{path}: more than {MAX_UNPACKED_BYTES} bytes in all"
        )));
    }
    let mut data = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    (reader.take(size + 1))
        .read_to_end(&mut data)
        .map_err(|e| TextError::Archive(format!("{path}: {e}")))?;
    if data.len() as u64 > size {
        return Err(TextError::Archive(format!(
            "{path}: longer than the {size} bytes it says"
        )));
    }
    *total += data.len() as u64;
    Ok(data)
}
