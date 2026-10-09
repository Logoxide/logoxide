/*!
Fetching packages from git repositories. [`Fetcher`] is what resolving and installing need (a
repository's tags, and its files at a commit); [`GitCli`] does it with the `git` command, which
reads any URL git does (`https://`, `ssh://`, `file://`, a local path).

WebAssembly and phones have no `git`: there, biblix-ffi's fetcher reads what the host downloads
over HTTP (a repository's refs, and archives of its commits; see [`crate::archive`]).
*/

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    error::{BiblixError, Result},
    storage::{MemoryStorage, Storage},
};

/// The git hosts a short name can name: `github:alice/kjv` is `https://github.com/alice/kjv`
pub const HOSTS: [(&str, &str); 3] = [
    ("github", "https://github.com"),
    ("gitlab", "https://gitlab.com"),
    ("codeberg", "https://codeberg.org"),
];

/**
A repository's URL from a short name (`github:alice/kjv`, `gitlab:group/sub/kjv`,
`codeberg:alice/kjv`); anything else is returned as it is. The path is the host's own, so GitLab's
nested groups work. Manifests and lock files keep the full URL
*/
pub fn expand_url(url: &str) -> String {
    let short = (url.split_once(':')).and_then(|(prefix, path)| {
        let (_, base) = HOSTS.iter().find(|(name, _)| *name == prefix)?;
        let path = path.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        (path.contains('/') && !path.starts_with('/')).then(|| format!("{base}/{path}"))
    });
    short.unwrap_or_else(|| url.to_string())
}

/// Which commit of a repository: a tag, a branch, a revision, or its default branch
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitRef {
    Tag(String),
    Branch(String),
    Rev(String),
    Head,
}

pub trait Fetcher {
    /// Each tag and the commit it names
    fn tags(&self, url: &str) -> Result<Vec<(String, String)>>;

    /// The repository's files at `reference` into `dest` (without `.git`), and the commit
    /// they're from
    fn fetch(&self, url: &str, reference: &GitRef, dest: &dyn Storage) -> Result<String>;

    /// Only these files (by path) at `reference`, into `dest`, without downloading the rest when
    /// the host allows (a summary without its text). Missing files are left out
    fn fetch_files(
        &self,
        url: &str,
        reference: &GitRef,
        paths: &[&str],
        dest: &dyn Storage,
    ) -> Result<String> {
        let all = MemoryStorage::new();
        let commit = self.fetch(url, reference, &all)?;
        for path in paths {
            if let Ok(bytes) = all.read(path) {
                dest.write(path, &bytes)?;
            }
        }
        Ok(commit)
    }
}

/// Fetching with the `git` command
#[derive(Clone, Debug, Default)]
pub struct GitCli {
    /// Where repositories are cloned while fetching (a temporary folder by default)
    pub scratch: Option<PathBuf>,
}

fn git(args: &[&str], dir: Option<&Path>, url: &str) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args).env("GIT_TERMINAL_PROMPT", "0");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().map_err(|e| BiblixError::Fetch {
        url: url.to_string(),
        message: format!("couldn't run git: {e}"),
    })?;
    if !output.status.success() {
        return Err(BiblixError::Fetch {
            url: url.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl GitCli {
    fn scratch_dir(&self, what: &str) -> PathBuf {
        let scratch = self.scratch.clone().unwrap_or_else(std::env::temp_dir);
        scratch.join(format!("biblix-{what}-{}-{}", std::process::id(), unique()))
    }
}

fn wanted(reference: &GitRef) -> String {
    match reference {
        GitRef::Tag(tag) => format!("refs/tags/{tag}"),
        GitRef::Branch(branch) => format!("refs/heads/{branch}"),
        GitRef::Rev(rev) => rev.clone(),
        GitRef::Head => String::from("HEAD"),
    }
}

impl Fetcher for GitCli {
    /// A partial clone (no file contents) with only these files checked out, so only they are
    /// downloaded from hosts that allow it (GitHub, GitLab, Gitea; a local repository sends all)
    fn fetch_files(
        &self,
        url: &str,
        reference: &GitRef,
        paths: &[&str],
        dest: &dyn Storage,
    ) -> Result<String> {
        let work = self.scratch_dir("files");
        let io = |e: std::io::Error| BiblixError::io(&work.display().to_string(), e);
        std::fs::create_dir_all(&work).map_err(io)?;
        let result = (|| {
            let w = Some(work.as_path());
            git(&["init", "-q"], w, url)?;
            git(&["remote", "add", "origin", url], w, url)?;
            git(&["config", "remote.origin.promisor", "true"], w, url)?;
            git(
                &["config", "remote.origin.partialclonefilter", "blob:none"],
                w,
                url,
            )?;
            git(
                &[
                    "fetch",
                    "-q",
                    "--depth",
                    "1",
                    "--filter=blob:none",
                    "origin",
                    &wanted(reference),
                ],
                w,
                url,
            )?;
            let mut sparse = vec!["sparse-checkout", "set", "--no-cone"];
            let patterns: Vec<String> = paths.iter().map(|p| format!("/{p}")).collect();
            sparse.extend(patterns.iter().map(String::as_str));
            git(&sparse, w, url)?;
            git(&["checkout", "-q", "FETCH_HEAD"], w, url)?;
            let commit = git(&["rev-parse", "HEAD"], w, url)?.trim().to_string();
            for path in paths {
                let from = work.join(path);
                if from.is_file() {
                    let bytes = std::fs::read(&from).map_err(|e| BiblixError::io(path, e))?;
                    dest.write(path, &bytes)?;
                }
            }
            Ok(commit)
        })();
        let _ = std::fs::remove_dir_all(&work);
        result
    }

    fn tags(&self, url: &str) -> Result<Vec<(String, String)>> {
        let out = git(&["ls-remote", "--tags", url], None, url)?;
        let mut tags: Vec<(String, String)> = vec![];
        for line in out.lines() {
            let Some((commit, name)) = line.split_once('\t') else {
                continue;
            };
            let Some(tag) = name.strip_prefix("refs/tags/") else {
                continue;
            };
            // An annotated tag's `^{}` line names its commit, after the tag object's
            match tag.strip_suffix("^{}") {
                Some(tag) => {
                    if let Some(entry) = tags.iter_mut().find(|(t, _)| t == tag) {
                        entry.1 = commit.to_string();
                    }
                }
                None => tags.push((tag.to_string(), commit.to_string())),
            }
        }
        Ok(tags)
    }

    fn fetch(&self, url: &str, reference: &GitRef, dest: &dyn Storage) -> Result<String> {
        let work = self.scratch_dir("fetch");
        let io = |e: std::io::Error| BiblixError::io(&work.display().to_string(), e);
        std::fs::create_dir_all(&work).map_err(io)?;
        let result = (|| {
            git(&["init", "-q"], Some(&work), url)?;
            let wanted = match reference {
                GitRef::Tag(tag) => format!("refs/tags/{tag}"),
                GitRef::Branch(branch) => format!("refs/heads/{branch}"),
                GitRef::Rev(rev) => rev.clone(),
                GitRef::Head => String::from("HEAD"),
            };
            git(
                &["fetch", "-q", "--depth", "1", url, &wanted],
                Some(&work),
                url,
            )?;
            git(&["checkout", "-q", "FETCH_HEAD"], Some(&work), url)?;
            let commit = git(&["rev-parse", "HEAD"], Some(&work), url)?
                .trim()
                .to_string();
            copy_into(&work, "", dest)?;
            Ok(commit)
        })();
        let _ = std::fs::remove_dir_all(&work);
        result
    }
}

impl GitCli {
    /// A local repository's tags that are versions (`v1.2.0`), newest last
    pub fn local_versions(&self, repo: &Path) -> Result<Vec<(semver::Version, String)>> {
        let out = git(&["tag", "--list"], Some(repo), &repo.display().to_string())?;
        let mut versions: Vec<(semver::Version, String)> = out
            .lines()
            .filter_map(|tag| Some((crate::resolve::tag_version(tag)?, tag.to_string())))
            .collect();
        versions.sort();
        Ok(versions)
    }

    /// Commits everything in a local repository and tags it
    pub fn commit_and_tag(&self, repo: &Path, message: &str, tag: &str) -> Result<String> {
        let at = repo.display().to_string();
        git(&["add", "-A"], Some(repo), &at)?;
        git(&["commit", "-q", "-m", message], Some(repo), &at)?;
        git(&["tag", tag], Some(repo), &at)?;
        Ok(git(&["rev-parse", "HEAD"], Some(repo), &at)?
            .trim()
            .to_string())
    }
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Copies a folder's files into `to` under `prefix`, leaving out `.git`
fn copy_into(from: &Path, prefix: &str, to: &dyn Storage) -> Result<()> {
    let io = |path: &Path, e: std::io::Error| BiblixError::io(&path.display().to_string(), e);
    for entry in std::fs::read_dir(from).map_err(|e| io(from, e))? {
        let entry = entry.map_err(|e| io(from, e))?;
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            continue;
        };
        if name == ".git" {
            continue;
        }
        let (source, path) = (entry.path(), crate::storage::join(prefix, &name));
        if entry.file_type().map_err(|e| io(&source, e))?.is_dir() {
            copy_into(&source, &path, to)?;
        } else {
            to.write(&path, &std::fs::read(&source).map_err(|e| io(&source, e))?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_short_names() {
        assert_eq!(
            expand_url("github:alice/kjv"),
            "https://github.com/alice/kjv"
        );
        assert_eq!(
            expand_url("gitlab:group/sub/kjv.git"),
            "https://gitlab.com/group/sub/kjv"
        );
        assert_eq!(
            expand_url("codeberg:bob/kjv"),
            "https://codeberg.org/bob/kjv"
        );
        // Not short names: other URLs, an unknown host, or no owner
        for url in [
            "https://github.com/alice/kjv",
            "file:///tmp/kjv",
            "ssh://git@github.com/alice/kjv",
            "sourcehut:alice/kjv",
            "github:kjv",
        ] {
            assert_eq!(expand_url(url), url);
        }
    }
}
