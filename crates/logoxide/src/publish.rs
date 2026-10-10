/*!
Publishing releases as archives, for registries: a package's declared files as a `.tar.gz` that
the same files always make byte for byte (sorted, with no times or owners), its SHA-256, and its
entry in a registry's `catalog.toml`. A project names such a package with `registry` (the
catalog's URL) and a version, and installing downloads only that release's archive, so a registry
of thousands of books needs no repository per book, and any static host can serve one.

```toml
# catalog.toml
name = "dorean"

[[package]]
id = "the-dorean-principle"
name = "The Dorean Principle"
abbreviation = "TDP"
language = "en"
version = "1.0.2"
kind = "book"

[[package.release]]
version = "1.0.2"
url = "the-dorean-principle/the-dorean-principle-1.0.2.tar.gz"
sha256 = "sha256:9f86…"
checksum = "sha256:41c0…"
size = 1834201
mirrors = ["https://archive.org/download/dorean/the-dorean-principle-1.0.2.tar.gz"]  # optional
```

Anyone can mirror a release: its `mirrors` are tried in order when `url` fails, and the hash
makes every copy as good as the first.
*/

use std::io::Write;

use sha2::{Digest, Sha256};

use crate::{
    catalog::{Catalog, CatalogRelease},
    error::{LogoxideError, Result},
    manifest::Package,
    storage::Storage,
};

/// `sha256:` and the hex of the bytes' SHA-256, as lock files and catalogs write it
pub fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// A release's archive's file name: `the-dorean-principle-1.0.2.tar.gz`
pub fn archive_name(info: &Package) -> String {
    format!("{}-{}.tar.gz", info.id, info.version)
}

/**
A package's declared files ([`crate::package::declared_files`]) as a `.tar.gz` under one folder
(`<id>-<version>/`), the same bytes for the same files: in path order, each file's mode 644, with
no time or owner, and no time in the gzip header
*/
pub fn pack(storage: &dyn Storage, info: &Package) -> Result<Vec<u8>> {
    let top = format!("{}-{}", info.id, info.version);
    let fail = |e: std::io::Error| LogoxideError::Io {
        path: top.clone(),
        message: format!("packing: {e}"),
    };
    let gzip = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    let mut tar = tar::Builder::new(gzip);
    tar.mode(tar::HeaderMode::Deterministic);
    for path in crate::package::declared_files(storage)? {
        let bytes = storage.read(&path)?;
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        tar.append_data(&mut header, format!("{top}/{path}"), bytes.as_slice())
            .map_err(fail)?;
    }
    let mut gzip = tar.into_inner().map_err(fail)?;
    gzip.flush().map_err(fail)?;
    gzip.finish().map_err(fail)
}

/**
A registry's `catalog.toml` with a release added (`text` is the catalog as it is, if it exists;
`name` names a new one): the package's entry (made, or brought up to date when this is its newest
version, keeping its releases) and the release. `None` when the catalog has that release already,
the same archive; a release of that version with another archive is an error, since a published
version never changes. `path` names the package in the registry too (`books/romans`)
*/
pub fn add_release(
    text: Option<&str>,
    name: &str,
    info: &Package,
    path: Option<&str>,
    release: &CatalogRelease,
) -> Result<Option<String>> {
    let mut doc: toml_edit::DocumentMut = match text {
        Some(text) => text
            .parse()
            .map_err(|e| LogoxideError::Catalog(format!("catalog.toml: {e}")))?,
        None => {
            let mut doc = toml_edit::DocumentMut::new();
            let format = i64::from(crate::catalog::CATALOG_FORMAT);
            doc.insert("format", toml_edit::value(format));
            doc.insert("name", toml_edit::value(name));
            doc
        }
    };
    let packages = doc
        .entry("package")
        .or_insert(toml_edit::Item::ArrayOfTables(
            toml_edit::ArrayOfTables::new(),
        ))
        .as_array_of_tables_mut()
        .ok_or_else(|| LogoxideError::Catalog(String::from("`package` isn't [[package]]")))?;
    let index =
        (packages.iter()).position(|t| t.get("id").and_then(|i| i.as_str()) == Some(&info.id));
    let index = match index {
        Some(index) => index,
        None => {
            packages.push(toml_edit::Table::new());
            packages.len() - 1
        }
    };
    let Some(table) = packages.get_mut(index) else {
        return Err(LogoxideError::Catalog(format!("{}: no entry", info.id)));
    };
    let mut releases = match table.remove("release") {
        Some(item) => item.into_array_of_tables().map_err(|_| {
            LogoxideError::Catalog(format!("{}: `release` isn't [[package.release]]", info.id))
        })?,
        None => toml_edit::ArrayOfTables::new(),
    };
    let version = info.version.to_string();
    let mut newest = true;
    for existing in releases.iter() {
        let other = existing
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if other == version {
            return match existing.get("sha256").and_then(|s| s.as_str()) == Some(&release.sha256) {
                true => Ok(None),
                false => Err(LogoxideError::Catalog(format!(
                    "{} {version} is published already, as another archive: a published version \
                     never changes, so release a new one",
                    info.id
                ))),
            };
        }
        if other
            .parse::<semver::Version>()
            .is_ok_and(|v| v > info.version)
        {
            newest = false;
        }
    }
    // What the entry says comes from the newest release
    if newest || table.is_empty() {
        let mut fresh = Catalog::entry_table(info, "");
        fresh.remove("git");
        if let Some(path) = path.or_else(|| table.get("path").and_then(|p| p.as_str())) {
            fresh.insert("path", toml_edit::value(path));
        }
        *table = fresh;
    }
    let mut added = toml_edit::Table::new();
    added.insert("version", toml_edit::value(&version));
    added.insert("url", toml_edit::value(&release.url));
    if !release.mirrors.is_empty() {
        let mirrors: toml_edit::Array = release.mirrors.iter().collect();
        added.insert("mirrors", toml_edit::value(mirrors));
    }
    added.insert("sha256", toml_edit::value(&release.sha256));
    if let Some(checksum) = &release.checksum {
        added.insert("checksum", toml_edit::value(checksum));
    }
    if let Some(size) = release.size {
        added.insert("size", toml_edit::value(size as i64));
    }
    releases.push(added);
    table.insert("release", toml_edit::Item::ArrayOfTables(releases));
    Ok(Some(doc.to_string()))
}

/**
A registry's `catalog.toml` with a release yanked (`yanked = true`: new installs pass over it,
lock files that have it still install it), or put back. An error when the catalog has no such
release
*/
pub fn yank(text: &str, id: &str, version: &str, yanked: bool) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| LogoxideError::Catalog(format!("catalog.toml: {e}")))?;
    let release = (doc
        .get_mut("package")
        .and_then(|p| p.as_array_of_tables_mut()))
    .and_then(|packages| {
        (packages.iter_mut()).find(|t| t.get("id").and_then(|i| i.as_str()) == Some(id))
    })
    .and_then(|package| package.get_mut("release"))
    .and_then(|r| r.as_array_of_tables_mut())
    .and_then(|releases| {
        (releases.iter_mut()).find(|r| r.get("version").and_then(|v| v.as_str()) == Some(version))
    })
    .ok_or_else(|| LogoxideError::Catalog(format!("no release {id} {version} here")))?;
    match yanked {
        true => {
            release.insert("yanked", toml_edit::value(true));
        }
        false => {
            release.remove("yanked");
        }
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    fn package(version: &str) -> (MemoryStorage, Package) {
        let manifest = format!(
            "[package]\nid = \"romans\"\nname = \"Romans\"\nabbreviation = \"ROM\"\n\
             version = \"{version}\"\nlanguage = \"en\"\nkind = \"book\"\n\
             [formats]\nepub = {{ path = \"epub/\" }}\n"
        );
        let storage = MemoryStorage::new();
        storage.write("logoxide.toml", manifest.as_bytes()).unwrap();
        storage
            .write("epub/romans.epub", b"not really an epub")
            .unwrap();
        storage
            .write("notes.md", b"a reader's, not the package's")
            .unwrap();
        let info = crate::manifest::Manifest::parse(&manifest)
            .unwrap()
            .package
            .unwrap();
        (storage, info)
    }

    #[test]
    fn archives_are_the_same_bytes_and_hold_only_the_package() {
        let (storage, info) = package("1.0.0");
        let a = pack(&storage, &info).unwrap();
        let b = pack(&storage, &info).unwrap();
        assert_eq!(sha256(&a), sha256(&b));
        let unpacked = MemoryStorage::new();
        crate::archive::unpack(&a, &unpacked).unwrap();
        let mut files = unpacked.walk("").unwrap();
        files.sort();
        assert_eq!(files, ["epub/romans.epub", "logoxide.toml"]);
        assert_eq!(
            crate::package::checksum(&unpacked).unwrap(),
            crate::package::checksum(&storage).unwrap()
        );
    }

    #[test]
    fn releases_are_added_once_and_never_changed() {
        let release = |version: &str, sha: &str| CatalogRelease {
            version: version.into(),
            url: format!("romans/romans-{version}.tar.gz"),
            mirrors: vec![],
            sha256: sha.into(),
            checksum: Some("sha256:00".into()),
            size: Some(10),
            yanked: false,
        };
        let (_, one) = package("1.0.0");
        let (_, two) = package("1.1.0");
        let text = add_release(
            None,
            "mine",
            &one,
            Some("books/romans"),
            &release("1.0.0", "sha256:aa"),
        )
        .unwrap()
        .unwrap();
        let text = add_release(
            Some(&text),
            "mine",
            &two,
            None,
            &release("1.1.0", "sha256:bb"),
        )
        .unwrap()
        .unwrap();
        // The same archive again: nothing to do; another one for a published version: refused
        assert!(
            add_release(
                Some(&text),
                "mine",
                &two,
                None,
                &release("1.1.0", "sha256:bb")
            )
            .unwrap()
            .is_none()
        );
        assert!(
            add_release(
                Some(&text),
                "mine",
                &two,
                None,
                &release("1.1.0", "sha256:cc")
            )
            .is_err()
        );

        let catalog = Catalog::parse("https://example.org/r/catalog.toml", &text).unwrap();
        assert_eq!(catalog.name, "mine");
        let entry = catalog.find("books/romans").unwrap().unwrap();
        assert_eq!(entry.version.as_deref(), Some("1.1.0"));
        assert_eq!(
            entry.source,
            crate::catalog::CatalogSource::Registry {
                catalog: "https://example.org/r/catalog.toml".into()
            }
        );
        let newest = entry.release(None).unwrap();
        assert_eq!(
            newest.url,
            "https://example.org/r/romans/romans-1.1.0.tar.gz"
        );
        let req: semver::VersionReq = "~1.0".parse().unwrap();
        assert_eq!(entry.release(Some(&req)).unwrap().version, "1.0.0");

        // Yanked: new installs take the newest release left, and it can be put back
        let yanked = yank(&text, "romans", "1.1.0", true).unwrap();
        let catalog = Catalog::parse("catalog.toml", &yanked).unwrap();
        assert_eq!(catalog.entries[0].release(None).unwrap().version, "1.0.0");
        let back = yank(&yanked, "romans", "1.1.0", false).unwrap();
        assert_eq!(back, text);
        assert!(yank(&text, "romans", "9.0.0", true).is_err());
    }
}
