/*!
Updating an import: the same import made again from the publisher's newer download, compared
with the one a project has ([`compare_import`]), replaced ([`apply_import`]), and skipped when
the publisher's own version says it's current ([`import_is_current`])
*/

use std::sync::Arc;

use logoxide::{
    LogoxideError, Result,
    catalog::CatalogEntry,
    manifest::{MANIFEST, Manifest, Package as Info},
    package::{Features, Package},
    reader::TextReader,
    storage::{MemoryStorage, Storage},
    summary::Diff,
};

/// What updating an imported package changes ([`compare_import`])
#[derive(Clone, Debug)]
pub struct ImportUpdate {
    pub from: semver::Version,
    pub to: semver::Version,
    /// Its verses' changes (reworded, added, removed, renumbered), and its titles'
    pub diff: Diff,
    /// The publisher's versions before and after (`[package.upstream] version`), as given
    pub upstream_from: Option<String>,
    pub upstream_to: Option<String>,
}

/**
How a package made again from its publisher's download (`fresh`, as [`crate::build`] writes it)
differs from the one a project has (`current`): none when their declared files are the same. The
fresh package's version is the current one raised by what changed
([`logoxide::summary::next_version`]: rewording is a minor change, removing a verse a major one, a
new copyright notice or publisher's version alone a patch), written in its `logoxide.toml`, so one
version never names two texts (links record the version they were made in) and a version says
what an update can break, whatever the publisher numbers it. Both texts are read by `reader`
*/
pub fn compare_import(
    current: Arc<dyn Storage>,
    fresh: &Arc<MemoryStorage>,
    reader: &dyn TextReader,
) -> Result<Option<ImportUpdate>> {
    use logoxide::summary::{diff_summaries, next_version, summarize};
    let fresh_storage: Arc<dyn Storage> = fresh.clone();
    let info_of = |storage: &dyn Storage| -> Result<Info> {
        let manifest = Manifest::parse(&storage.read_string(MANIFEST)?)?;
        (manifest.package).ok_or_else(|| LogoxideError::Import(String::from("not a package")))
    };
    let had = info_of(current.as_ref())?;
    let made = info_of(fresh.as_ref())?;
    // Compared at the current version, so the same download is the same package
    set_version(fresh.as_ref(), &had.version)?;
    if logoxide::package::checksum(current.as_ref())?
        == logoxide::package::checksum(fresh.as_ref())?
    {
        return Ok(None);
    }
    let old = Package::open(current, Features::default())?;
    let new = Package::open(fresh_storage, Features::default())?;
    let (old_summary, old_verses) = summarize(&old, reader)?;
    let (new_summary, new_verses) = summarize(&new, reader)?;
    let diff = diff_summaries((&old_summary, &old_verses), (&new_summary, &new_verses));
    let to = next_version(&had.version, diff.change());
    set_version(fresh.as_ref(), &to)?;
    Ok(Some(ImportUpdate {
        from: had.version,
        to,
        diff,
        upstream_from: had.upstream.and_then(|u| u.version),
        upstream_to: made.upstream.and_then(|u| u.version),
    }))
}

/// Sets a made package's version in its `logoxide.toml`
pub fn set_version(storage: &dyn Storage, version: &semver::Version) -> Result<()> {
    let text = storage.read_string(MANIFEST)?;
    let mut doc: toml_edit::DocumentMut =
        (text.parse()).map_err(|e: toml_edit::TomlError| LogoxideError::Import(e.to_string()))?;
    doc["package"]["version"] = toml_edit::value(version.to_string());
    storage.write(MANIFEST, doc.to_string().as_bytes())
}

/**
Replaces an imported package's files (`current`) with a fresh import's: its declared files the
fresh one doesn't have are removed, the fresh one's written. Files beside them that aren't the
package's (a reader's annotations) stay. On disk, give it an
[`logoxide::FsStorage::no_symlinks`] folder, so a link in the project can't have a file written
or removed somewhere else
*/
pub fn apply_import(current: &dyn Storage, fresh: &dyn Storage) -> Result<()> {
    for path in import_removals(current, fresh)? {
        current.remove(&path)?;
    }
    for path in fresh.walk("")? {
        current.write(&path, &fresh.read(&path)?)?;
    }
    Ok(())
}

/// The imported package's files a fresh import replaces by nothing: its declared files the fresh
/// one lacks (what [`apply_import`] removes, for hosts that remove files themselves)
pub fn import_removals(current: &dyn Storage, fresh: &dyn Storage) -> Result<Vec<String>> {
    let new = fresh.walk("")?;
    Ok((logoxide::package::declared_files(current)?.into_iter())
        .filter(|path| !new.contains(path))
        .collect())
}

/**
Whether an import made from the publisher's version `upstream` (its `[package.upstream]
version`) is as new as `entry` lists, so it needn't be downloaded again to know (only where the
publisher's versions follow its updates:
[`logoxide::catalog::CatalogSource::versions_track_updates`]). The package's own version is
never compared: it's logoxide's, raised by updates
*/
pub fn import_is_current(entry: &CatalogEntry, upstream: Option<&str>) -> bool {
    let (Some(listed), Some(installed)) = (entry.version.as_deref(), upstream) else {
        return false;
    };
    entry.source.versions_track_updates()
        && compare_publisher_versions(listed, installed).is_some_and(|o| o.is_le())
}

/**
Two of a publisher's versions in order, when both are numbers between dots, dashes, or
underscores (`2026-10-08` before `2026-11-01`, `3.1` before `3.10`; missing parts are 0); others
only when they're the same
*/
pub fn compare_publisher_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    let parts = |v: &str| -> Option<Vec<u64>> {
        (v.trim().split(['.', '-', '_']))
            .map(|p| p.parse().ok())
            .collect()
    };
    match (parts(a), parts(b)) {
        (Some(mut a), Some(mut b)) => {
            let len = a.len().max(b.len());
            a.resize(len, 0);
            b.resize(len, 0);
            Some(a.cmp(&b))
        }
        _ => (a.trim() == b.trim()).then_some(std::cmp::Ordering::Equal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::zip;
    use logoxide::reader::DefaultReader;

    #[test]
    fn updates_an_import() {
        let csv = "\"languageCode\",\"translationId\",\"languageNameInEnglish\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"textDirection\",\"downloadable\",\"shortTitle\"\n\"eng\",\"eng-asv\",\"English\",\"American Standard Version (1901)\",\"\",\"True\",\"public domain\",\"2026-10-08\",\"ltr\",\"True\",\"ASV\"\n";
        let catalog = crate::ebible::catalog(csv).unwrap();
        let entry = &catalog.entries[0];
        let book = |text: &str| format!("\\id JHN\n\\c 3\n\\p\n\\v 16 {text}\n");
        let made = |files: &[(&str, &str)]| {
            let storage = Arc::new(MemoryStorage::new());
            crate::ebible::import(entry, &zip(files), storage.as_ref()).unwrap();
            storage
        };
        let current = made(&[
            ("73-JHNeng-asv.usfm", &book("For God so loved the world.")),
            (
                "74-ACTeng-asv.usfm",
                "\\id ACT\n\\c 1\n\\p\n\\v 1 The former treatise.\n",
            ),
        ]);
        // An imported package names its catalog entry by its upstream
        let package = Package::open(current.clone(), Features::default()).unwrap();
        let upstream = package.info().upstream.clone().unwrap();
        assert_eq!(catalog.find_upstream(&upstream), Some(entry));
        assert_eq!(crate::upstream(entry), Some(upstream));
        assert!(entry.source.versions_track_updates());
        // By its URL alone too (a package made by hand from the download)
        let by_url = logoxide::manifest::Upstream {
            import: None,
            ..package.info().upstream.clone().unwrap()
        };
        assert_eq!(catalog.find_upstream(&by_url), Some(entry));

        // The same download: nothing to update
        let same = made(&[
            ("73-JHNeng-asv.usfm", &book("For God so loved the world.")),
            (
                "74-ACTeng-asv.usfm",
                "\\id ACT\n\\c 1\n\\p\n\\v 1 The former treatise.\n",
            ),
        ]);
        assert!(
            compare_import(current.clone(), &same, &DefaultReader)
                .unwrap()
                .is_none()
        );

        // Reworded at the same publisher's version (as fetch.bible's change): the version is
        // raised by the change, a book removed being a major one
        let fresh = made(&[(
            "73-JHNeng-asv.usfm",
            &book("For God so much loved the world."),
        )]);
        let update = compare_import(current.clone(), &fresh, &DefaultReader)
            .unwrap()
            .unwrap();
        assert_eq!(
            (update.from.to_string(), update.to.to_string()),
            ("1.0.0".into(), "2.0.0".into())
        );
        assert_eq!(
            (
                update.upstream_from.as_deref(),
                update.upstream_to.as_deref()
            ),
            (Some("2026-10-08"), Some("2026-10-08"))
        );
        assert_eq!(
            (update.diff.changed.len(), update.diff.removed.len()),
            (1, 1)
        );
        // A reader's note beside the package stays; a book the publisher dropped goes
        current.write("JHN notes.md", b"mine").unwrap();
        apply_import(current.as_ref(), fresh.as_ref()).unwrap();
        assert_eq!(
            current.walk("").unwrap(),
            ["JHN notes.md", "logoxide.toml", "usfm/73-JHNeng-asv.usfm"]
        );
        let package = Package::open(current.clone(), Features::default()).unwrap();
        assert_eq!(package.info().version.to_string(), "2.0.0");
        // Made again from the same download: current at its raised version, not raised again
        let again = made(&[(
            "73-JHNeng-asv.usfm",
            &book("For God so much loved the world."),
        )]);
        assert!(
            compare_import(current.clone(), &again, &DefaultReader)
                .unwrap()
                .is_none()
        );
        // The raised version doesn't hide the publisher's next release: the publisher's
        // versions are compared, never the package's
        let upstream = package.info().upstream.clone().unwrap();
        assert!(import_is_current(entry, upstream.version.as_deref()));
        let next = crate::ebible::catalog(&csv.replace("2026-10-08", "2026-11-01")).unwrap();
        let next = &next.entries[0];
        assert_eq!(next.version.as_deref(), Some("2026-11-01"));
        assert!(!import_is_current(next, upstream.version.as_deref()));
        assert!(!import_is_current(next, None));
        // Its download, with the same text: a patch, which records the publisher's version
        let storage = Arc::new(MemoryStorage::new());
        let same_text = zip(&[(
            "73-JHNeng-asv.usfm",
            &book("For God so much loved the world."),
        )]);
        crate::ebible::import(next, &same_text, storage.as_ref()).unwrap();
        let update = compare_import(current.clone(), &storage, &DefaultReader)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                update.to.to_string(),
                update.upstream_from.as_deref(),
                update.upstream_to.as_deref()
            ),
            ("2.0.1".into(), Some("2026-10-08"), Some("2026-11-01"))
        );
        apply_import(current.as_ref(), storage.as_ref()).unwrap();
        let package = Package::open(current.clone(), Features::default()).unwrap();
        let upstream = package.info().upstream.clone().unwrap();
        assert!(import_is_current(next, upstream.version.as_deref()));
    }

    #[test]
    fn publishers_versions_are_compared_as_numbers() {
        use std::cmp::Ordering::*;
        assert_eq!(
            compare_publisher_versions("2026-10-08", "2026-11-01"),
            Some(Less)
        );
        assert_eq!(compare_publisher_versions("3.10", "3.9"), Some(Greater));
        assert_eq!(compare_publisher_versions("3.1", "3.1.0"), Some(Equal));
        assert_eq!(compare_publisher_versions("1.0b", "1.0b"), Some(Equal));
        assert_eq!(compare_publisher_versions("1.0b", "1.0c"), None);
    }
}
