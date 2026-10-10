/*!
Folder registries: a folder of package repositories (at any depth) read as a catalog, each package
from its newest release, read from git without checking anything out. `lx catalog build` writes
one as a `catalog.toml` ([`build_catalog`]), and a registry that names a folder is read the same
way ([`folder_catalog`]). It runs `git`, so it's native only
*/

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    catalog::Catalog,
    error::{LogoxideError, Result},
    manifest::{MANIFEST, Manifest},
};

/// A local repository's newest release
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub commit: String,
    /// Its `logoxide.toml` at that tag
    pub manifest: String,
}

/// Runs git in a repository; what it printed
pub fn git_in(repo: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| LogoxideError::io(&repo.display().to_string(), format!("git: {e}")))?;
    if !output.status.success() {
        return Err(LogoxideError::io(
            &repo.display().to_string(),
            format!(
                "git {}: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/**
Every package repository in a folder, at any depth (a git repository with a `logoxide.toml`), with
its path there (`books/romans`), in path order. A repository's own folders and hidden folders
aren't looked in
*/
pub fn package_repositories(folder: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut found = vec![];
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if dir.join(".git").exists() && dir.join(MANIFEST).is_file() {
            let path = dir.strip_prefix(folder).unwrap_or(&dir);
            let path: Vec<String> = (path.components())
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            found.push((path.join("/"), dir));
            continue;
        }
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| LogoxideError::io(&dir.display().to_string(), e))?;
        for entry in entries.flatten() {
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            if !hidden && entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(entry.path());
            }
        }
    }
    found.sort();
    Ok(found)
}

/**
A local repository's newest release (its highest `v1.2.3` tag, not a prerelease) and its
`logoxide.toml` there, read from git without checking anything out; none before its first release
*/
pub fn newest_release(repo: &Path) -> Result<Option<Release>> {
    let refs = git_in(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:strip=2) %(objectname) %(*objectname)",
            "refs/tags",
        ],
    )?;
    let newest = (refs.lines())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let tag = parts.next()?;
            let object = parts.next()?;
            // An annotated tag names its commit after the tag object
            let commit = parts.next().unwrap_or(object);
            let version = crate::resolve::tag_version(tag).filter(|v| v.pre.is_empty())?;
            Some((version, tag.to_string(), commit.to_string()))
        })
        .max_by(|a, b| a.0.cmp(&b.0));
    let Some((_, tag, commit)) = newest else {
        return Ok(None);
    };
    let manifest = git_in(repo, &["show", &format!("{commit}:{MANIFEST}")])?;
    Ok(Some(Release {
        tag,
        commit,
        manifest,
    }))
}

/**
A folder's package repositories as a catalog named `name`, each from its newest release (its
`file://` URL); what couldn't be read is said in the warnings, and a repository not released yet
is left out
*/
pub fn folder_catalog(name: &str, folder: &Path) -> Result<(Catalog, Vec<String>)> {
    let (mut packages, mut warnings) = (vec![], vec![]);
    for (path, dir) in package_repositories(folder)? {
        match newest_release(&dir) {
            Ok(Some(release)) => match Manifest::parse(&release.manifest) {
                Ok(parsed) => packages.push((path, parsed)),
                Err(e) => warnings.push(format!("{path} {}: {e}", release.tag)),
            },
            Ok(None) => {}
            Err(e) => warnings.push(format!("{path}: {e}")),
        }
    }
    let url = format!("file://{}", folder.display());
    Ok((Catalog::from_packages(name, &url, packages), warnings))
}

/// What [`build_catalog`] made
#[derive(Clone, Debug)]
pub struct BuiltCatalog {
    /// The `catalog.toml`
    pub text: String,
    /// The packages in it
    pub count: usize,
    /// Those read from their release
    pub read: usize,
    /// Those kept from the previous catalog, their release unchanged
    pub kept: usize,
    /// The repositories left out, each with why (`books/romans (no release yet: lx release)`)
    pub skipped: Vec<String>,
}

/**
A `catalog.toml` of a folder's package repositories, each from its newest release, with its path
and commit. `url` is each package's git URL, `{path}` being its path in the folder and `{name}` its
folder's name (its `file://` URL by default); `name` is the catalog's (the folder's by default).
An entry of `previous` (the catalog written last time) whose path, commit, and URL are unchanged
is kept as it was
*/
pub fn build_catalog(
    folder: &Path,
    previous: Option<&str>,
    url: Option<&str>,
    name: Option<&str>,
) -> Result<BuiltCatalog> {
    let name = name.map(String::from).unwrap_or_else(|| {
        (folder.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    // The entries written last time, by path
    let mut before: HashMap<String, toml_edit::Table> = HashMap::new();
    if let Some(text) = previous {
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| LogoxideError::Catalog(format!("the previous catalog: {e}")))?;
        if let Some(packages) = doc.get("package").and_then(|p| p.as_array_of_tables()) {
            for table in packages.iter() {
                if let Some(path) = table.get("path").and_then(|p| p.as_str()) {
                    before.insert(path.to_string(), table.clone());
                }
            }
        }
    }
    let (mut tables, mut read, mut kept, mut skipped) = (vec![], 0, 0, vec![]);
    for (path, repo) in package_repositories(folder)? {
        let folder_name = path.rsplit('/').next().unwrap_or(&path);
        let git = match url {
            Some(template) => template
                .replace("{path}", &path)
                .replace("{name}", folder_name),
            None => format!("file://{}", repo.display()),
        };
        let release = match newest_release(&repo) {
            Ok(Some(release)) => release,
            Ok(None) => {
                skipped.push(format!("{path} (no release yet: lx release)"));
                continue;
            }
            Err(e) => {
                skipped.push(format!("{path} ({e})"));
                continue;
            }
        };
        let unchanged = before.remove(&path).filter(|t| {
            t.get("commit").and_then(|c| c.as_str()) == Some(release.commit.as_str())
                && t.get("git").and_then(|g| g.as_str()) == Some(git.as_str())
        });
        if let Some(table) = unchanged {
            tables.push(table);
            kept += 1;
            continue;
        }
        let info = match Manifest::parse(&release.manifest) {
            Ok(manifest) => match manifest.package {
                Some(info) => info,
                None => {
                    skipped.push(format!("{path} (a vault, not a package)"));
                    continue;
                }
            },
            Err(e) => {
                skipped.push(format!("{path} {} ({e})", release.tag));
                continue;
            }
        };
        let mut table = Catalog::entry_table(&info, &git);
        table.insert("path", toml_edit::value(&path));
        table.insert("commit", toml_edit::value(&release.commit));
        tables.push(table);
        read += 1;
    }
    let count = tables.len();
    let text = format!(
        "# Written by lx catalog build: the newest release of each package in {}\n{}",
        folder.display(),
        Catalog::toml_document(Some(&name), tables)
    );
    Ok(BuiltCatalog {
        text,
        count,
        read,
        kept,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "tag.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn package(repo: &Path, id: &str, version: &str) {
        std::fs::create_dir_all(repo).unwrap();
        std::fs::write(
            repo.join(MANIFEST),
            format!(
                "[package]\nid = \"{id}\"\nname = \"{id}\"\nabbreviation = \"{id}\"\nversion = \"{version}\"\nlanguage = \"en\"\n\n[formats]\nusx = {{ path = \"usx/\" }}\n"
            ),
        )
        .unwrap();
    }

    fn commit_and_tag(repo: &Path, tag: Option<&str>) {
        git(repo, &["add", "-A"]);
        git(repo, &["commit", "-q", "--allow-empty", "-m", "release"]);
        if let Some(tag) = tag {
            git(repo, &["tag", "-a", tag, "-m", tag]);
        }
    }

    #[test]
    fn folders_of_repositories_are_catalogs() {
        let root = std::env::temp_dir().join(format!("logoxide-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Two released packages (one a folder down), one not released, and a hidden folder
        let romans = root.join("books/romans");
        package(&romans, "romans", "1.0.0");
        git(&romans, &["init", "-q", "-b", "main"]);
        commit_and_tag(&romans, Some("v1.0.0"));
        package(&romans, "romans", "1.1.0");
        commit_and_tag(&romans, Some("v1.1.0"));
        // A prerelease isn't the newest release
        commit_and_tag(&romans, None);
        git(&romans, &["tag", "v2.0.0-beta.1"]);
        let gill = root.join("gill");
        package(&gill, "gill", "0.1.0");
        git(&gill, &["init", "-q", "-b", "main"]);
        commit_and_tag(&gill, Some("v0.1.0"));
        let draft = root.join("draft");
        package(&draft, "draft", "0.1.0");
        git(&draft, &["init", "-q", "-b", "main"]);
        commit_and_tag(&draft, None);
        package(&root.join(".hidden"), "hidden", "1.0.0");

        let found: Vec<String> = (package_repositories(&root).unwrap().into_iter())
            .map(|(path, _)| path)
            .collect();
        assert_eq!(found, ["books/romans", "draft", "gill"]);

        let newest = newest_release(&romans).unwrap().unwrap();
        assert_eq!(newest.tag, "v1.1.0");
        assert!(newest.manifest.contains("version = \"1.1.0\""));
        assert_eq!(
            newest.commit,
            git_in(&romans, &["rev-parse", "v1.1.0^{commit}"])
                .unwrap()
                .trim()
        );
        assert!(newest_release(&draft).unwrap().is_none());

        let (catalog, warnings) = folder_catalog("mine", &root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let ids: Vec<&str> = catalog.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["romans", "gill"]);

        let built = build_catalog(&root, None, Some("https://example.org/{name}"), None).unwrap();
        assert_eq!((built.count, built.read, built.kept), (2, 2, 0));
        assert_eq!(built.skipped, ["draft (no release yet: lx release)"]);
        assert!(built.text.contains("git = \"https://example.org/romans\""));
        assert!(built.text.contains("path = \"books/romans\""));
        let parsed = Catalog::parse("catalog.toml", &built.text).unwrap();
        assert_eq!(parsed.entries.len(), 2);

        // Built again: unchanged entries are kept, a new release is read
        package(&gill, "gill", "0.2.0");
        commit_and_tag(&gill, Some("v0.2.0"));
        let again = build_catalog(
            &root,
            Some(&built.text),
            Some("https://example.org/{name}"),
            Some("mine"),
        )
        .unwrap();
        assert_eq!((again.count, again.read, again.kept), (2, 1, 1));
        assert!(again.text.contains("version = \"0.2.0\""));
        assert!(again.text.contains("name = \"mine\""));
        let _ = std::fs::remove_dir_all(&root);
    }
}
