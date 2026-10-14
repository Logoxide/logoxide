//! A package's media, checked against what it lists

use std::ops::ControlFlow;

use logoxide::{LogoxideError, Result, media::MediaFile};
use sha2::{Digest, Sha256};

use crate::http::Http;

/**
Downloads a media file from `url` (its URL or a mirror) and checks it's the one listed (its size,
then its SHA-256), hashing as it reads rather than holding it; errors are named by `url`
*/
pub fn verify(http: &dyn Http, url: &str, file: &MediaFile) -> Result<()> {
    let fail = |message: String| LogoxideError::Mismatch {
        url: url.to_string(),
        message,
    };
    let (mut hasher, mut size) = (Sha256::new(), 0u64);
    http.get_to(url, &mut |bytes| {
        size += bytes.len() as u64;
        // Already longer than listed: no need to read the rest
        if size > file.size {
            return ControlFlow::Break(());
        }
        hasher.update(bytes);
        ControlFlow::Continue(())
    })?;
    if size > file.size {
        return Err(fail(format!("more than the {} bytes listed", file.size)));
    }
    if size != file.size {
        return Err(fail(format!("{size} bytes, not the {} listed", file.size)));
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !logoxide::media::sha256_matches(&file.sha256, &actual) {
        return Err(fail(format!(
            "its SHA-256 is {actual}, not the {} listed",
            file.sha256
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gives the same bytes for every URL
    struct Fixed(Vec<u8>);

    impl Http for Fixed {
        fn get(&self, _: &str) -> Result<Vec<u8>> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn media_is_checked_by_size_and_hash() {
        let listed = MediaFile::of("a.mp3", "https://example.org/a.mp3", b"abc");
        verify(&Fixed(b"abc".to_vec()), &listed.url, &listed).unwrap();
        let longer = verify(&Fixed(b"abcd".to_vec()), &listed.url, &listed).unwrap_err();
        assert!(
            longer.to_string().contains("more than the 3 bytes"),
            "{longer}"
        );
        let other = verify(&Fixed(b"abd".to_vec()), &listed.url, &listed).unwrap_err();
        assert!(other.to_string().contains("SHA-256"), "{other}");
    }
}
