/*!
Door43 (unfoldingWord's Door43 Content Service, git.door43.org): its catalog of Bibles
([`catalog`], the catalog API's `search?subject=Bible&stage=prod`) and a Bible's release
([`import`]): the repository's `archive/<tag>.tar.gz`, a resource container whose
`manifest.yaml` describes it (`dublin_core`: title, language, rights, version) and whose `.usfm`
files are its books.

Written without the service answering (it timed out from the owner's machine in October 2026):
the `manifest.yaml` shape is the resource container specification's, but the catalog API's JSON
is as remembered (`tests/fixtures/door43/README.md`), so [`catalog`] reads it loosely: every field
but `owner` and `name` may be missing.
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};
use serde::Deserialize;

use crate::{Made, external, finish, import_id, package_id};

/// Door43's catalog of Bibles in production, as the catalog API lists them
pub const CATALOG: &str = "https://git.door43.org/api/v1/catalog/search?subject=Bible&stage=prod";

/// Where Door43 serves a repository's files at a tag, as a `.tar.gz`
pub fn archive_url(repository: &str, tag: &str) -> String {
    format!("https://git.door43.org/{repository}/archive/{tag}.tar.gz")
}

/// Whether `location` is Door43's catalog API (a search of it)
pub fn is_catalog(location: &str) -> bool {
    location
        .to_ascii_lowercase()
        .contains("door43.org/api/v1/catalog")
}

/// The catalog API's answer: its entries in `data`
#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    data: Vec<Listed>,
}

/// One entry of the catalog API, the fields read
#[derive(Deserialize)]
struct Listed {
    owner: String,
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    language_title: Option<String>,
    #[serde(default)]
    language_direction: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    branch_or_tag_name: Option<String>,
    #[serde(default)]
    release: Option<Release>,
    #[serde(default)]
    tarbar_url: Option<String>,
    #[serde(default)]
    abbreviation: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    tag_name: Option<String>,
}

/// Whether a name is one of a Door43 owner's or repository's (`Door43-Catalog`, `hi_irv`)
fn is_repository_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['.', '-'])
        && (name.bytes()).all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}

/**
Door43's catalog ([`CATALOG`]): each Bible (`subject` "Bible") a `door43:<owner>/<repo>` import
of its release's archive, its id `door43-<owner>-<repo>`, its version the release's tag (which a
newer release raises, so `tracks_versions`), named, languaged, and directed as the catalog says.
An entry without a release, or whose owner or name isn't a repository's, is passed over
*/
pub fn catalog(text: &str) -> Result<Catalog> {
    let search: Search = serde_json::from_str(text)
        .map_err(|e| LogoxideError::Catalog(format!("Door43's catalog: {e}")))?;
    let mut entries: Vec<CatalogEntry> = vec![];
    for listed in search.data {
        if listed.subject.as_deref() != Some("Bible") {
            continue;
        }
        if !is_repository_name(&listed.owner) || !is_repository_name(&listed.name) {
            continue;
        }
        let tag = (listed.release.and_then(|r| r.tag_name))
            .or(listed.branch_or_tag_name)
            .filter(|t| !t.is_empty() && !t.starts_with('-'));
        let Some(tag) = tag else { continue };
        let repository = format!("{}/{}", listed.owner, listed.name);
        if (entries.iter()).any(|e| e.source.import_id() == Some(repository.as_str())) {
            continue;
        }
        let mut urls = vec![archive_url(&repository, &tag)];
        urls.extend(listed.tarbar_url.filter(|u| !urls.contains(u)));
        let downloads = vec![Download {
            name: String::from("archive"),
            urls,
        }];
        let (language, language_name) = match listed.language.as_deref() {
            Some(code) => {
                let (tag, raw) = language::normalize_language(code);
                (tag, raw.or(listed.language_title))
            }
            None => (String::from("und"), listed.language_title),
        };
        let direction = match listed.language_direction.as_deref() {
            Some("rtl") => Direction::Rtl,
            _ => Direction::Ltr,
        };
        // `hi_irv` is IRV, as the catalog says
        let abbreviation = (listed.abbreviation.filter(|a| !a.is_empty()))
            .unwrap_or_else(|| {
                let name = &listed.name;
                name.split_once('_')
                    .map_or(name.as_str(), |(_, a)| a)
                    .to_string()
            })
            .to_uppercase();
        entries.push(CatalogEntry {
            id: format!("door43-{}", package_id(&repository.replace('/', "-"))),
            name: listed.title.unwrap_or_else(|| listed.name.clone()),
            abbreviation,
            aliases: vec![],
            language,
            language_name,
            script: None,
            description: None,
            license: None,
            rights: Default::default(),
            copyright: None,
            version: Some(tag),
            kind: PackageKind::Bible,
            direction,
            source: external("door43", &repository, true, downloads),
            path: None,
            releases: vec![],
            keywords: vec![],
            categories: vec![],
            about_books: vec![],
            work: None,
            edition: None,
            edition_year: None,
            keys: None,
        });
    }
    if entries.is_empty() {
        return Err(LogoxideError::Catalog(String::from(
            "Door43's catalog lists no Bibles (has its API changed?)",
        )));
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    logoxide::catalog::known_works(&mut entries);
    Ok(Catalog {
        name: String::from("Door43"),
        entries,
        warnings: vec![],
    })
}

/// A resource container's `manifest.yaml`, the fields read
#[derive(Debug, Default, Deserialize)]
pub struct ResourceManifest {
    #[serde(default)]
    pub dublin_core: DublinCore,
}

/// `manifest.yaml`'s `dublin_core`
#[derive(Debug, Default, Deserialize)]
pub struct DublinCore {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub language: Option<RcLanguage>,
    /// A license's name (`CC BY-SA 4.0`)
    #[serde(default)]
    pub rights: Option<String>,
    /// The resource's version (`3`), which may be written as a number
    #[serde(default)]
    pub version: Option<serde_yaml::Value>,
    #[serde(default)]
    pub publisher: Option<String>,
}

/// `dublin_core.language`
#[derive(Debug, Default, Deserialize)]
pub struct RcLanguage {
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
}

impl DublinCore {
    /// Its version as text (`3`, `1.2`), if it gives one
    pub fn version(&self) -> Option<String> {
        let version = match self.version.as_ref()? {
            serde_yaml::Value::String(s) => s.trim().to_string(),
            serde_yaml::Value::Number(n) => n.to_string(),
            _ => return None,
        };
        (!version.is_empty()).then_some(version)
    }
}

/// The license `dublin_core.rights` names, as SPDX: Door43's short names (`CC BY-SA 4.0` is
/// `CC-BY-SA-4.0`, `CC0 1.0`), else as a statement ([`crate::license::license_from_text`])
fn rights_license(rights: &str) -> Option<String> {
    let spdx = rights.split_whitespace().collect::<Vec<_>>().join("-");
    match logoxide::license::check_license(&spdx) {
        Ok(()) if spdx.to_ascii_uppercase().starts_with("CC") => Some(spdx),
        _ => crate::license::license_from_text(rights),
    }
}

/**
A Bible Door43 publishes, from its release's archive (the bytes at [`archive_url`]), as a
package in `dest`: `logoxide.toml`, each `.usfm` file in `usfm/`, and `LICENSE.md` (the license
file) when the archive has one. `manifest.yaml`'s `dublin_core` names it (`title`), gives its
language and direction, its license (`rights`), and the upstream version (`version`, else the
release's tag). An archive without a `manifest.yaml` or USFM is an error
*/
pub fn import(entry: &CatalogEntry, archive: &[u8], dest: &dyn Storage) -> Result<()> {
    let repository = import_id(entry, "door43", "Door43")?;
    let unpacked = MemoryStorage::new();
    logoxide::net::archive::unpack(archive, &unpacked)?;
    let manifest = unpacked.read_string("manifest.yaml").map_err(|_| {
        LogoxideError::Import(format!(
            "{repository}: no manifest.yaml (is it a resource container?)"
        ))
    })?;
    let manifest: ResourceManifest = serde_yaml::from_str(&manifest)
        .map_err(|e| LogoxideError::Import(format!("{repository}: manifest.yaml: {e}")))?;
    let core = manifest.dublin_core;
    let staged = MemoryStorage::new();
    let mut license_file = None;
    for path in unpacked.walk("")? {
        let name = path.rsplit('/').next().unwrap_or(&path);
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".usfm") {
            staged.write(&format!("usfm/{name}"), &unpacked.read(&path)?)?;
        } else if path == "LICENSE.md" {
            staged.write("LICENSE.md", &unpacked.read(&path)?)?;
            license_file = Some(String::from("LICENSE.md"));
        }
    }
    let mut entry = entry.clone();
    if let Some(title) = core
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        && title != entry.name
    {
        let listed = std::mem::replace(&mut entry.name, title.to_string());
        entry.aliases.push(listed);
    }
    if let Some(description) = core.description.as_deref().map(str::trim) {
        entry.description = (!description.is_empty()).then(|| description.to_string());
    }
    if let Some(rc) = core.language.as_ref() {
        if let Some(code) = rc.identifier.as_deref() {
            let (tag, raw) = language::normalize_language(code);
            entry.language = tag;
            entry.language_name = raw.or_else(|| rc.title.clone()).or(entry.language_name);
        }
        match rc.direction.as_deref() {
            Some("rtl") => entry.direction = Direction::Rtl,
            Some("ltr") => entry.direction = Direction::Ltr,
            _ => {}
        }
    }
    entry.license = (core.rights.as_deref())
        .and_then(rights_license)
        .or(entry.license);
    entry.version = core.version().or(entry.version);
    let made = Made {
        format: "usfm",
        license_file,
        publisher: "Door43",
        homepage: format!("https://git.door43.org/{repository}"),
        versification: None,
        keys: None,
    };
    finish(&entry, made, staged, dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use logoxide::package::{Features, Package};

    /// The catalog API's answer, as remembered (see the fixture's README)
    const SEARCH: &str = include_str!("../tests/fixtures/door43/catalog-search.json");

    const MANIFEST: &str = r#"---
dublin_core:
    conformsto: 'rc0.2'
    contributor:
        - 'A Contributor'
    creator: 'Someone Or Organization'
    description: 'One or two sentence description of the resource.'
    format: 'text/usfm'
    identifier: 'irv'
    issued: '2019-12-17'
    language:
        identifier: 'hi'
        title: 'हिन्दी'
        direction: 'ltr'
    modified: '2019-12-22T12:01:30-05:00'
    publisher: 'Bridge Connectivity Solutions'
    relation:
        - 'hi/tn'
    rights: 'CC BY-SA 4.0'
    subject: 'Bible'
    title: 'Indian Revised Version'
    type: 'bundle'
    version: 2
checking:
    checking_level: '3'
projects:
    -
        categories:
            - 'bible-nt'
        identifier: 'jhn'
        path: './44-JHN.usfm'
        sort: 43
        title: 'यूहन्ना'
        versification: 'ufw'
"#;

    /// A `.tar.gz` of these files in one top folder, as Door43 serves a tag's archive
    fn archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, text) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, format!("hi_irv/{path}"), text.as_bytes())
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn the_catalog() {
        assert!(is_catalog(CATALOG));
        let catalog = crate::catalog::parse_text(CATALOG, SEARCH).unwrap();
        assert_eq!(catalog.name, "Door43");
        // Translation notes aren't a Bible
        let ids: Vec<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "door43-door43-catalog-ar_avd",
                "door43-door43-catalog-hi_irv",
                "door43-door43-catalog-ur-deva_irv"
            ]
        );
        let irv = catalog.find_import("door43:Door43-Catalog/hi_irv").unwrap();
        assert_eq!(
            (irv.abbreviation.as_str(), irv.language.as_str()),
            ("IRV", "hi")
        );
        assert_eq!(irv.version.as_deref(), Some("v2"));
        assert!(irv.source.versions_track_updates());
        assert_eq!(
            irv.source.downloads()[0].urls,
            ["https://git.door43.org/Door43-Catalog/hi_irv/archive/v2.tar.gz"]
        );
        let avd = catalog.find_import("door43:Door43-Catalog/ar_avd").unwrap();
        assert_eq!(avd.direction, Direction::Rtl);
        assert_eq!(crate::catalog::publisher_name(&avd.source), Some("Door43"));
        assert!(crate::catalog::parse_text(CATALOG, "{\"ok\":true,\"data\":[]}").is_err());
    }

    #[test]
    fn imports_a_release() {
        let catalog = catalog(SEARCH).unwrap();
        let entry = catalog.find_import("door43:Door43-Catalog/hi_irv").unwrap();
        let usfm = "\\id JHN\n\\h यूहन्ना\n\\c 1\n\\p\n\\v 1 आदि में वचन था।\n\\v 2 यही आदि में परमेश्‍वर के साथ था।\n";
        let bytes = archive(&[
            ("manifest.yaml", MANIFEST),
            ("LICENSE.md", "# License\n\nCC BY-SA 4.0\n"),
            ("44-JHN.usfm", usfm),
            ("media.yaml", "projects: []\n"),
        ]);
        let dest = MemoryStorage::new();
        crate::tests::build_twice(entry, &[("archive".into(), bytes)], &dest);
        assert_eq!(
            dest.walk("").unwrap(),
            ["LICENSE.md", "logoxide.toml", "usfm/44-JHN.usfm"]
        );
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let info = package.info();
        assert_eq!(info.name, "Indian Revised Version");
        assert_eq!(info.aliases, ["Indian Revised Version (IRV) Hindi - 2019"]);
        assert_eq!(info.license.as_deref(), Some("CC-BY-SA-4.0"));
        assert_eq!(info.license_file.as_deref(), Some("LICENSE.md"));
        assert_eq!(info.version.to_string(), "1.0.0");
        let upstream = info.upstream.as_ref().unwrap();
        assert_eq!(
            (upstream.import.as_deref(), upstream.version.as_deref()),
            (Some("door43:Door43-Catalog/hi_irv"), Some("2"))
        );
        assert_eq!(
            upstream.url.as_deref(),
            Some("https://git.door43.org/Door43-Catalog/hi_irv/archive/v2.tar.gz")
        );
        assert_eq!(package.books().len(), 1);
        // Without a manifest, it isn't a resource container
        let bytes = archive(&[("44-JHN.usfm", usfm)]);
        let error = import(entry, &bytes, &MemoryStorage::new()).unwrap_err();
        assert!(error.to_string().contains("manifest.yaml"), "{error}");
        // Without a version, the release's tag
        let bare = MANIFEST.replace("    version: 2\n", "");
        let dest = MemoryStorage::new();
        import(
            entry,
            &archive(&[("manifest.yaml", &bare), ("44-JHN.usfm", usfm)]),
            &dest,
        )
        .unwrap();
        let package = Package::open(std::sync::Arc::new(dest), Features::default()).unwrap();
        let upstream = package.info().upstream.clone().unwrap();
        assert_eq!(upstream.version.as_deref(), Some("v2"));
    }

    #[test]
    fn rights_as_licenses() {
        assert_eq!(
            rights_license("CC BY-SA 4.0").as_deref(),
            Some("CC-BY-SA-4.0")
        );
        assert_eq!(rights_license("CC BY 4.0").as_deref(), Some("CC-BY-4.0"));
        assert_eq!(
            rights_license("Public Domain").as_deref(),
            Some("LicenseRef-PublicDomain")
        );
        assert_eq!(rights_license("All rights reserved"), None);
    }
}
