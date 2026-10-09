/*!
Audio and video: listed in a package's `biblix.toml` by URL, size, and SHA-256, never stored in
it, so a package stays a small git repository and the media stays wherever its owner serves it.
A host downloads a file when it's first played, checks it ([`verify`]), and keeps it by its hash
([`store_path`]), so versions that list the same file share it.

```toml
[media]
timings = "timings/"             # each verse's start and end in each file (format to be chosen)

[[media.files]]
path = "JHN.mp3"                 # what timings name it by
url = "https://example.org/audio/JHN.mp3"
size = 31457280
sha256 = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
book = "JHN"                     # the book it reads, if one
```

The transcript is the package's text (`[formats]`, read like any other), which topos searches; a
recording names the edition it reads as a dependency. Releases are judged by the text, so a new
hash for the same words is a patch.
*/

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{BiblixError, Result};

/// A package's `[media]`
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    /// Where each file's verse timings are (a folder in the package)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timings: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<MediaFile>,
}

/// One audio or video file, by where it's served and what it hashes to
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaFile {
    /// Its name in the package (what timings name it by): `JHN.mp3`
    pub path: String,
    pub url: String,
    /// In bytes
    pub size: u64,
    /// Lowercase hex
    pub sha256: String,
    /// The book it reads (a USFM code), if one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<String>,
}

impl MediaFile {
    /// An entry for a file's bytes (`bx media add`)
    pub fn of(path: &str, url: &str, bytes: &[u8]) -> MediaFile {
        MediaFile {
            path: path.to_string(),
            url: url.to_string(),
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
            book: None,
        }
    }
}

/// A file's SHA-256, as lowercase hex
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Whether downloaded bytes are the file listed: its size, then its hash
pub fn verify(file: &MediaFile, bytes: &[u8]) -> Result<()> {
    let fail = |message: String| BiblixError::Fetch {
        url: file.url.clone(),
        message,
    };
    if bytes.len() as u64 != file.size {
        return Err(fail(format!(
            "{} bytes, not the {} listed",
            bytes.len(),
            file.size
        )));
    }
    let actual = sha256_hex(bytes);
    if !actual.eq_ignore_ascii_case(&file.sha256) {
        return Err(fail(format!(
            "its SHA-256 is {actual}, not the {} listed",
            file.sha256
        )));
    }
    Ok(())
}

/// Where a host keeps a media file, by its hash: `media/9f/9f86…` (relative to its cache), so
/// every version that lists it shares one copy
pub fn store_path(file: &MediaFile) -> String {
    let hash = file.sha256.to_ascii_lowercase();
    format!("media/{}/{hash}", hash.get(..2).unwrap_or("00"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_checks_media() {
        let manifest = crate::Manifest::parse(
            r#"
[package]
id = "bsb-audio"
name = "BSB audio"
abbreviation = "BSBA"
version = "1.0.0"
language = "en"

[formats]
usx = { path = "usx/" }

[media]
timings = "timings/"

[[media.files]]
path = "JHN.mp3"
url = "https://example.org/JHN.mp3"
size = 5
sha256 = "2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824"
book = "JHN"
"#,
        )
        .unwrap();
        let media = manifest.media.unwrap();
        let file = &media.files[0];
        assert!(verify(file, b"hello").is_ok());
        assert!(verify(file, b"hellO").is_err());
        assert!(verify(file, b"hello!").is_err());
        assert_eq!(
            store_path(file),
            "media/2c/2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(
            MediaFile::of("a.mp3", "u", b"hello").sha256,
            file.sha256.to_lowercase()
        );
    }
}
