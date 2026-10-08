/*!
Fetching packages from git repositories. [`Fetcher`] is what resolving and installing need (a
repository's tags, and its files at a commit); [`GitCli`] does it with the `git` command, which
reads any URL git does (`https://`, `ssh://`, `file://`, a local path).

WebAssembly and phones have no `git`: a fetcher there downloads a tag's archive (GitHub, GitLab,
and Gitea serve one per tag) through the host's HTTP. That fetcher comes with biblix-ffi.
*/

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::{BiblixError, Result};

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

    /// The repository's files at `reference` into `dest` (a new folder, without `.git`), and the
    /// commit they're from
    fn fetch(&self, url: &str, reference: &GitRef, dest: &Path) -> Result<String>;
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

impl Fetcher for GitCli {
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

    fn fetch(&self, url: &str, reference: &GitRef, dest: &Path) -> Result<String> {
        let scratch = self.scratch.clone().unwrap_or_else(std::env::temp_dir);
        let work = scratch.join(format!("biblix-fetch-{}-{}", std::process::id(), unique()));
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
            copy_tree(&work, dest)?;
            Ok(commit)
        })();
        let _ = std::fs::remove_dir_all(&work);
        result
    }
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
}

/// Copies a folder's files, leaving out `.git`
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    let io = |path: &Path, e: std::io::Error| BiblixError::io(&path.display().to_string(), e);
    std::fs::create_dir_all(to).map_err(|e| io(to, e))?;
    for entry in std::fs::read_dir(from).map_err(|e| io(from, e))? {
        let entry = entry.map_err(|e| io(from, e))?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let (source, target) = (entry.path(), to.join(&name));
        if entry.file_type().map_err(|e| io(&source, e))?.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            std::fs::copy(&source, &target).map_err(|e| io(&source, e))?;
        }
    }
    Ok(())
}
