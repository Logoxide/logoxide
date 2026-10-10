/*!
Registries of archives: `lx registry init` sets one up, and `lx publish` adds a package's release
to it (its archive, and its entry in the registry's `catalog.toml`). A registry is hosted as:

- **A folder**: `catalog.toml` and each archive beside it (`romans/romans-1.0.0.tar.gz`). Any
  static host serves it (a web server, GitHub Pages, Netlify, an S3 bucket), and on disk it's a
  registry too
- **GitHub Releases** (`github:owner/repo`): each archive is a release's file, and the catalog is
  the file of a release named `index`, so the repository itself stays empty however many books it
  publishes. `gh` (logged in) does the work
*/

use std::path::{Path, PathBuf};

use logoxide::{
    LogoxideError, MemoryStorage, Storage,
    catalog::CatalogRelease,
    git::{Fetcher, GitRef},
    publish,
};

/// Where a registry's releases go
pub enum Host {
    Folder(PathBuf),
    /// `owner/repo`
    GitHub(String),
}

impl Host {
    pub fn parse(text: &str) -> Host {
        match text.strip_prefix("github:") {
            Some(repo) => Host::GitHub(repo.trim_matches('/').to_string()),
            None => Host::Folder(PathBuf::from(text)),
        }
    }

    /// Where its catalog is read from, for `lx catalog add`
    fn catalog_location(&self) -> String {
        match self {
            Host::Folder(dir) => dir.join(CATALOG).display().to_string(),
            Host::GitHub(repo) => {
                format!("https://github.com/{repo}/releases/download/{INDEX}/{CATALOG}")
            }
        }
    }

    /// A name for its catalog: the folder's or the repository's
    fn default_name(&self) -> String {
        let name = match self {
            Host::Folder(dir) => (dir.canonicalize().ok())
                .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())),
            Host::GitHub(repo) => repo.rsplit('/').next().map(String::from),
        };
        name.unwrap_or_else(|| String::from("registry"))
    }
}

const CATALOG: &str = "catalog.toml";
/// The GitHub release whose file is the catalog
const INDEX: &str = "index";

fn io(path: &Path, e: std::io::Error) -> LogoxideError {
    LogoxideError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

/// Runs `gh`; what it printed
fn gh(args: &[&str]) -> logoxide::Result<String> {
    let output = std::process::Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| LogoxideError::Fetch {
            url: String::from("gh"),
            message: format!("couldn't run gh (https://cli.github.com, then gh auth login): {e}"),
        })?;
    if !output.status.success() {
        return Err(LogoxideError::Fetch {
            url: format!("gh {}", args.join(" ")),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn scratch(what: &str) -> logoxide::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("lx-{what}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| io(&dir, e))?;
    Ok(dir)
}

const README: &str = "# A logoxide registry

`catalog.toml` lists each package and its releases; each release is an archive beside it
(`<id>/<id>-<version>.tar.gz`), with its SHA-256. `lx publish --to <this folder>` adds one.

Serve this folder from any static host (a web server, GitHub Pages, Netlify, an S3 bucket), then:

```sh
lx catalog add https://example.org/<where>/catalog.toml --name <name>
lx add <name>:<id>
```

On this computer it's a registry already: `lx catalog add <this folder>/catalog.toml --name <name>`.
";

/// `lx registry init`: an empty registry, ready to publish to; how to use it
pub fn init(host: &Host, name: Option<&str>, private: bool) -> logoxide::Result<()> {
    let name = name.map_or_else(|| host.default_name(), String::from);
    let catalog =
        format!("# A logoxide registry: lx publish --to … adds releases\nname = \"{name}\"\n");
    match host {
        Host::Folder(dir) => {
            std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
            let path = dir.join(CATALOG);
            if path.exists() {
                outln!("{} is a registry already", dir.display());
            } else {
                std::fs::write(&path, &catalog).map_err(|e| io(&path, e))?;
                let readme = dir.join("README.md");
                if !readme.exists() {
                    std::fs::write(&readme, README).map_err(|e| io(&readme, e))?;
                }
                outln!("Made the registry {name} in {}", dir.display());
            }
            outln!(
                "Publish to it:  lx publish <package folder> --to {}\n\
                 Use it here:    lx catalog add {} --name {name}\n\
                 Host it:        serve the folder from any static host, then\n                \
                 lx catalog add https://<where>/{CATALOG} --name {name}",
                dir.display(),
                host.catalog_location(),
            );
        }
        Host::GitHub(repo) => {
            if gh(&["repo", "view", repo, "--json", "name"]).is_err() {
                let visibility = if private { "--private" } else { "--public" };
                gh(&[
                    "repo",
                    "create",
                    repo,
                    visibility,
                    "--add-readme",
                    "--description",
                    "A logoxide registry: each package release is a GitHub release",
                ])?;
                outln!("Made the repository github.com/{repo}");
            }
            if gh(&["release", "view", INDEX, "-R", repo]).is_ok() {
                outln!("github.com/{repo} is a registry already");
            } else {
                let dir = scratch("registry")?;
                let path = dir.join(CATALOG);
                std::fs::write(&path, &catalog).map_err(|e| io(&path, e))?;
                gh(&[
                    "release",
                    "create",
                    INDEX,
                    &path.display().to_string(),
                    "-R",
                    repo,
                    "--title",
                    "Registry index",
                    "--notes",
                    "This release's catalog.toml lists every package and release here (lx publish updates it).",
                    "--latest=false",
                ])?;
                let _ = std::fs::remove_dir_all(&dir);
                outln!("Made the registry {name} in github.com/{repo}'s releases");
            }
            outln!(
                "Publish to it:  lx publish <package folder> --to github:{repo}\n\
                 Use it:         lx catalog add {} --name {name}",
                host.catalog_location()
            );
        }
    }
    Ok(())
}

/// A package's newest release, read from git: its archive and what's needed to list it
struct Packed {
    info: logoxide::manifest::Package,
    bytes: Vec<u8>,
    checksum: String,
    /// The package's folder in its repository (`books/romans`), if not at its root
    path: Option<String>,
}

/**
The package in `dir` at its release tag for the version its `logoxide.toml` gives (`v1.0.0`, or
`<folder>/v1.0.0` in a folder of its repository): its declared files, packed
*/
fn pack(dir: &Path, git: &logoxide::git::GitCli) -> logoxide::Result<Packed> {
    let (root, prefix) = git.package_in(dir)?;
    let manifest = logoxide::Manifest::parse(
        &std::fs::read_to_string(dir.join(logoxide::manifest::MANIFEST)).map_err(|e| io(dir, e))?,
    )?;
    let version = (manifest.package.as_ref())
        .map(|p| p.version.clone())
        .ok_or_else(|| LogoxideError::Manifest(format!("{}: no [package]", dir.display())))?;
    let tag = logoxide::release::tag_for(&prefix, &version);
    if !git.local_versions(dir)?.iter().any(|(_, t)| *t == tag) {
        return Err(LogoxideError::Resolve(format!(
            "{} has no release {tag}: lx release first (a published version is a release)",
            dir.display()
        )));
    }
    let all = MemoryStorage::new();
    git.fetch(&root.display().to_string(), &GitRef::Tag(tag), &all)?;
    let files = logoxide::package::only_declared(&logoxide::release::within(&all, &prefix)?)?;
    let info = logoxide::Manifest::parse(&files.read_string(logoxide::manifest::MANIFEST)?)?
        .package
        .ok_or_else(|| LogoxideError::Manifest(String::from("no [package] at its release")))?;
    Ok(Packed {
        bytes: publish::pack(&files, &info)?,
        checksum: logoxide::package::checksum(&files)?,
        info,
        path: Some(prefix.trim_end_matches('/').to_string()).filter(|p| !p.is_empty()),
    })
}

/// SPDX licenses that allow anyone to redistribute what they cover (with their terms)
const FREE_LICENSES: &[&str] = &[
    "0BSD",
    "AFL-3.0",
    "Artistic-2.0",
    "BSL-1.0",
    "CC-PDDC",
    "CC0-1.0",
    "ISC",
    "MIT",
    "MIT-0",
    "ODbL-1.0",
    "ODC-By-1.0",
    "OFL-1.1",
    "PDDL-1.0",
    "Unlicense",
    "WTFPL",
    "Zlib",
];

/// Prefixes of SPDX families that allow redistribution (`CC-BY-4.0`, `GPL-3.0-or-later`)
const FREE_PREFIXES: &[&str] = &[
    "AGPL-", "Apache-", "BSD-", "CC-BY-", "EUPL-", "GFDL-", "GPL-", "LGPL-", "MPL-",
];

/// Whether one SPDX id allows redistribution: a free license, or the public domain said in words
fn free_license(id: &str) -> bool {
    let words = id.to_lowercase().replace(['-', '_'], " ");
    // Creative Commons' noncommercial and no-derivatives licenses aren't free to share on
    let restricted = id.starts_with("CC-BY-") && (id.contains("-NC") || id.contains("-ND"));
    words.contains("public domain")
        || words.contains("publicdomain")
        || FREE_LICENSES.iter().any(|l| l.eq_ignore_ascii_case(id))
        || (!restricted && FREE_PREFIXES.iter().any(|p| id.starts_with(p)))
}

/**
Why a package's license may not allow publishing it: none given, or an SPDX expression no
alternative of which is a free license (each of its `AND`ed ids free) or the public domain
(`LicenseRef-…`, a noncommercial license, an unknown id). Licenses are shown, never enforced, so
this only warns
*/
fn license_warning(info: &logoxide::manifest::Package) -> Option<String> {
    let check = "licenses are shown, never enforced: check that you may redistribute it";
    let Some(license) = info.license.as_deref().filter(|l| !l.trim().is_empty()) else {
        let file = (info.license_file.as_deref())
            .map(|f| format!(" (only the file {f})"))
            .unwrap_or_default();
        return Some(format!("{} gives no SPDX license{file}: {check}", info.id));
    };
    let free = license.split(" OR ").any(|alternative| {
        alternative
            .replace(['(', ')'], " ")
            .split(" AND ")
            // `GPL-2.0 WITH Classpath-exception-2.0`: the license, not its exception
            .filter_map(|id| id.split(" WITH ").next().map(str::trim))
            .all(|id| free_license(id.trim_end_matches('+')))
    });
    match free {
        true => None,
        false => Some(format!(
            "{}'s license, {license}, isn't a free or open license logoxide knows: {check}",
            info.id
        )),
    }
}

/// `lx publish`: a package's release (its archive) added to a registry
pub fn publish(dir: &Path, host: &Host, git: &logoxide::git::GitCli) -> logoxide::Result<()> {
    let packed = pack(dir, git)?;
    let info = &packed.info;
    if let Some(warning) = license_warning(info) {
        errln!("warning: {warning}");
    }
    let file = publish::archive_name(info);
    let sha256 = publish::sha256(&packed.bytes);
    let release = |url: String| CatalogRelease {
        version: info.version.to_string(),
        url,
        mirrors: vec![],
        sha256: sha256.clone(),
        checksum: Some(packed.checksum.clone()),
        size: Some(packed.bytes.len() as u64),
        yanked: false,
    };
    let label = format!("{} {}", info.id, info.version);
    let size = packed.bytes.len() as f64 / 1_000_000.0;
    match host {
        Host::Folder(registry) => {
            let catalog = registry.join(CATALOG);
            let text = std::fs::read_to_string(&catalog).map_err(|_| {
                LogoxideError::Catalog(format!(
                    "{} isn't a registry (lx registry init {})",
                    registry.display(),
                    registry.display()
                ))
            })?;
            let relative = format!("{}/{file}", info.id);
            let release = release(relative.clone());
            let Some(updated) = publish::add_release(
                Some(&text),
                &host.default_name(),
                info,
                packed.path.as_deref(),
                &release,
            )?
            else {
                outln!("{label} is published there already");
                return Ok(());
            };
            let archive = registry.join(&relative);
            std::fs::create_dir_all(registry.join(&info.id)).map_err(|e| io(registry, e))?;
            std::fs::write(&archive, &packed.bytes).map_err(|e| io(&archive, e))?;
            std::fs::write(&catalog, updated).map_err(|e| io(&catalog, e))?;
            outln!("Published {label} ({size:.1} MB) to {}", registry.display());
        }
        Host::GitHub(repo) => {
            let work = scratch("publish")?;
            gh(&[
                "release",
                "download",
                INDEX,
                "-R",
                repo,
                "-p",
                CATALOG,
                "-D",
                &work.display().to_string(),
                "--clobber",
            ])
            .map_err(|e| {
                LogoxideError::Catalog(format!(
                    "github.com/{repo} isn't a registry yet (lx registry init github:{repo}): {e}"
                ))
            })?;
            let catalog = work.join(CATALOG);
            let text = std::fs::read_to_string(&catalog).map_err(|e| io(&catalog, e))?;
            let tag = format!("{}-v{}", info.id, info.version);
            let url = format!("https://github.com/{repo}/releases/download/{tag}/{file}");
            let Some(updated) = publish::add_release(
                Some(&text),
                &host.default_name(),
                info,
                packed.path.as_deref(),
                &release(url),
            )?
            else {
                outln!("{label} is published there already");
                return Ok(());
            };
            let archive = work.join(&file);
            std::fs::write(&archive, &packed.bytes).map_err(|e| io(&archive, e))?;
            let archive = archive.display().to_string();
            // A release left by a publish that stopped halfway gets its file again
            if gh(&["release", "view", &tag, "-R", repo]).is_ok() {
                gh(&["release", "upload", &tag, &archive, "-R", repo, "--clobber"])?;
            } else {
                gh(&[
                    "release",
                    "create",
                    &tag,
                    &archive,
                    "-R",
                    repo,
                    "--title",
                    &format!("{} {}", info.name, info.version),
                    "--notes",
                    &format!("{label}: {sha256}"),
                    "--latest=false",
                ])?;
            }
            std::fs::write(&catalog, updated).map_err(|e| io(&catalog, e))?;
            gh(&[
                "release",
                "upload",
                INDEX,
                &catalog.display().to_string(),
                "-R",
                repo,
                "--clobber",
            ])?;
            let _ = std::fs::remove_dir_all(&work);
            outln!("Published {label} ({size:.1} MB) to github.com/{repo}");
        }
    }
    outln!(
        "Readers add it with:  lx catalog add {} --name <name>, then lx add <name>:{}",
        host.catalog_location(),
        packed.path.as_deref().unwrap_or(&info.id)
    );
    Ok(())
}

/**
`lx publish --all`: every package in a folder (its own, and its folders' at any depth, not inside
`.git`) whose version has a release tag, published (each already published is said so); one
without a release is passed over
*/
pub fn publish_all(dir: &Path, host: &Host, git: &logoxide::git::GitCli) -> logoxide::Result<()> {
    let mut found = vec![];
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        if folder.join(logoxide::manifest::MANIFEST).is_file() {
            found.push(folder.clone());
        }
        let entries = std::fs::read_dir(&folder).map_err(|e| io(&folder, e))?;
        for entry in entries.flatten() {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if !hidden && entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
    }
    found.sort();
    for package in found {
        match publish(&package, host, git) {
            Err(LogoxideError::Resolve(message)) if message.contains("lx release first") => {
                outln!("{}: not released yet, passed over", package.display())
            }
            // A vault's manifest, or a folder that isn't a package
            Err(LogoxideError::Manifest(message)) if message.contains("no [package]") => {}
            result => result?,
        }
    }
    Ok(())
}

/// `lx yank`: a release withdrawn from a registry (new installs pass over it), or put back
pub fn yank(host: &Host, id: &str, version: &str, undo: bool) -> logoxide::Result<()> {
    let what = if undo { "Put back" } else { "Yanked" };
    match host {
        Host::Folder(registry) => {
            let catalog = registry.join(CATALOG);
            let text = std::fs::read_to_string(&catalog).map_err(|e| io(&catalog, e))?;
            let updated = publish::yank(&text, id, version, !undo)?;
            std::fs::write(&catalog, updated).map_err(|e| io(&catalog, e))?;
            outln!("{what} {id} {version} in {}", registry.display());
        }
        Host::GitHub(repo) => {
            let work = scratch("yank")?;
            gh(&[
                "release",
                "download",
                INDEX,
                "-R",
                repo,
                "-p",
                CATALOG,
                "-D",
                &work.display().to_string(),
                "--clobber",
            ])?;
            let catalog = work.join(CATALOG);
            let text = std::fs::read_to_string(&catalog).map_err(|e| io(&catalog, e))?;
            std::fs::write(&catalog, publish::yank(&text, id, version, !undo)?)
                .map_err(|e| io(&catalog, e))?;
            gh(&[
                "release",
                "upload",
                INDEX,
                &catalog.display().to_string(),
                "-R",
                repo,
                "--clobber",
            ])?;
            let _ = std::fs::remove_dir_all(&work);
            outln!("{what} {id} {version} in github.com/{repo}");
        }
    }
    outln!("Lock files that have it still install it; new installs and updates pass over it");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(license: Option<&str>) -> logoxide::manifest::Package {
        let license = license
            .map(|l| format!("license = \"{l}\"\n"))
            .unwrap_or_default();
        let text = format!(
            "[package]\nid = \"a\"\nname = \"A\"\nabbreviation = \"A\"\nversion = \"1.0.0\"\nlanguage = \"en\"\n{license}\n[formats]\nusx = {{ path = \"usx/\" }}\n"
        );
        logoxide::Manifest::parse(&text).unwrap().package.unwrap()
    }

    #[test]
    fn licenses_that_may_not_allow_publishing_are_warned_of() {
        for free in [
            "CC0-1.0",
            "CC-BY-SA-4.0",
            "MIT OR Apache-2.0",
            "GPL-2.0-or-later WITH Classpath-exception-2.0",
            "(MIT AND CC-BY-4.0)",
            "Public Domain",
            "LicenseRef-PublicDomain",
            "CC-BY-NC-4.0 OR CC0-1.0",
        ] {
            assert_eq!(license_warning(&package(Some(free))), None, "{free}");
        }
        for restricted in [
            None,
            Some("LicenseRef-Copyrighted"),
            Some("CC-BY-NC-ND-4.0"),
            Some("CC-BY-ND-4.0"),
            Some("MIT AND LicenseRef-Proprietary"),
            Some("unknown"),
        ] {
            let warning = license_warning(&package(restricted));
            assert!(
                warning.is_some_and(|w| w.contains("never enforced")),
                "{restricted:?}"
            );
        }
    }
}
