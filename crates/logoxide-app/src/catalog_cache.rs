/*!
Catalogs downloaded over HTTP, kept so `lx search` needn't download them every time:
`<cache>/catalogs/<hash of the URL>.catalog`, each a header line (`logoxide-catalog 1`, when it
was downloaded in seconds since 1970, and its URL) and then the bytes as they came, read again
each time they're used. A catalog is used from here while it's younger than the config's
`catalog-max-age` ([`Freshness::Fresh`]), whatever its age when nothing may be downloaded
([`Freshness::Offline`]), and never when it's being refreshed ([`Freshness::Refresh`]); a copy
too old to use is still used, and said to be, when the download fails. Only what parsed as a
catalog is kept. Local files and registries' folders are never kept: they're read each time
*/

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use logoxide::{LogoxideError, Result};

/// The first line's start, and the layout's version
const HEADER: &str = "logoxide-catalog 1";

/// When a catalog kept in the cache is used instead of downloading it
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Freshness {
    /// Kept and younger than the cache's longest age: used; else downloaded (and kept)
    #[default]
    Fresh,
    /// Downloaded again, whatever's kept (a kept one is used only when the download fails)
    Refresh,
    /// Only what's kept, of any age: nothing is downloaded
    Offline,
}

/// Where downloaded catalogs are kept, and how long they're used
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogCache {
    dir: PathBuf,
    max_age: Duration,
}

/// A catalog's bytes as they were downloaded, and when (seconds since 1970)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cached {
    pub bytes: Vec<u8>,
    pub fetched: u64,
}

impl CatalogCache {
    /// Catalogs kept in `dir`, each used until it's `max_age` old; none when that's zero
    pub fn new(dir: impl Into<PathBuf>, max_age: Duration) -> Option<CatalogCache> {
        (!max_age.is_zero()).then(|| CatalogCache {
            dir: dir.into(),
            max_age,
        })
    }

    /// `<cache>/catalogs` under a library's cache folder
    pub fn in_cache(cache_dir: &Path, max_age: Duration) -> Option<CatalogCache> {
        CatalogCache::new(cache_dir.join("catalogs"), max_age)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn max_age(&self) -> Duration {
        self.max_age
    }

    /// The file `url`'s catalog is kept in
    pub fn path(&self, url: &str) -> PathBuf {
        let hash = logoxide::publish::sha256(url.as_bytes());
        let hex = hash.trim_start_matches("sha256:");
        self.dir
            .join(format!("{}.catalog", &hex[..32.min(hex.len())]))
    }

    /// What's kept of `url`'s catalog, if anything is (and its header names that URL)
    pub fn read(&self, url: &str) -> Option<Cached> {
        let file = std::fs::read(self.path(url)).ok()?;
        let end = file.iter().position(|&b| b == b'\n')?;
        let header = std::str::from_utf8(&file[..end]).ok()?;
        let rest = header.strip_prefix(HEADER)?.strip_prefix(' ')?;
        let (fetched, kept_url) = rest.split_once(' ')?;
        (kept_url == url).then_some(())?;
        Some(Cached {
            bytes: file[end + 1..].to_vec(),
            fetched: fetched.parse().ok()?,
        })
    }

    /// Keeps `url`'s catalog as downloaded now
    pub fn store(&self, url: &str, bytes: &[u8]) -> Result<()> {
        self.store_at(url, bytes, now())
    }

    /// Keeps `url`'s catalog as downloaded at `fetched` (seconds since 1970)
    pub fn store_at(&self, url: &str, bytes: &[u8], fetched: u64) -> Result<()> {
        if url.contains(['\n', '\r']) {
            return Err(LogoxideError::Catalog(format!(
                "{url:?}: a URL with a line break isn't kept"
            )));
        }
        let mut file = format!("{HEADER} {fetched} {url}\n").into_bytes();
        file.extend_from_slice(bytes);
        let path = self.path(url);
        logoxide::storage::write_atomic(&path, &file)
            .map_err(|e| LogoxideError::io(&path.display().to_string(), e))
    }

    /// How long ago a kept catalog was downloaded (none, if the clock says it's to come)
    pub fn age(&self, cached: &Cached) -> Duration {
        Duration::from_secs(now().saturating_sub(cached.fetched))
    }

    /// Whether a kept catalog is young enough to use
    pub fn is_fresh(&self, cached: &Cached) -> bool {
        self.age(cached) < self.max_age
    }
}

/// Seconds since 1970
fn now() -> u64 {
    (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)).map_or(0, |d| d.as_secs())
}

/// An age in words, in its largest whole unit: `3 days`, `1 hour`, `40 seconds`
pub fn age_words(age: Duration) -> String {
    let seconds = age.as_secs();
    let (count, unit) = match seconds {
        s if s >= 86400 => (s / 86400, "day"),
        s if s >= 3600 => (s / 3600, "hour"),
        s if s >= 60 => (s / 60, "minute"),
        s => (s, "second"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_catalog_by_its_url() {
        let dir = crate::catalogs::tests::scratch("keeps");
        let cache = CatalogCache::new(&dir, Duration::from_secs(60)).unwrap();
        let url = "https://example.org/catalog.toml";
        assert_eq!(cache.read(url), None);
        cache.store(url, b"a\nb").unwrap();
        let kept = cache.read(url).unwrap();
        assert_eq!(kept.bytes, b"a\nb");
        assert!(cache.is_fresh(&kept));
        cache.store_at(url, b"old", now() - 61).unwrap();
        assert!(!cache.is_fresh(&cache.read(url).unwrap()));
        // Another URL's file isn't taken for it
        std::fs::copy(cache.path(url), cache.path("https://example.org/other")).unwrap();
        assert_eq!(cache.read("https://example.org/other"), None);
        assert!(cache.store("https://example.org/\nx", b"").is_err());
        assert_eq!(CatalogCache::new(&dir, Duration::ZERO), None);
        assert_eq!(age_words(Duration::from_secs(3 * 86400 + 5)), "3 days");
        assert_eq!(age_words(Duration::from_secs(3600)), "1 hour");
        assert_eq!(age_words(Duration::from_secs(5)), "5 seconds");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
