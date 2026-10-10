/*!
Fetching packages from git repositories. [`Fetcher`] is what resolving and installing need (a
repository's tags, and its files at a commit); [`GitCli`] does it with the `git` command, which
reads any URL git does (`https://`, `ssh://`, `file://`, a local path).

WebAssembly and phones have no `git`: there, logoxide-ffi's fetcher reads what the host downloads
over HTTP (a repository's refs, and archives of its commits; see [`crate::archive`]).
*/

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{
    error::{LogoxideError, Result},
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

/**
Whether a repository or file is on this machine: a path or a `file://` URL. A URL with a scheme
(`https://`, `ssh://`) and an scp-like `user@host:path` are elsewhere; a short name
(`github:owner/repo`) is expanded first ([`expand_url`])
*/
pub fn is_local(url: &str) -> bool {
    let url = expand_url(url.trim());
    if url.to_ascii_lowercase().starts_with("file:") {
        return true;
    }
    if url.contains("://") {
        return false;
    }
    // `host:path` (scp-like: no `/` before the colon) is elsewhere, but `C:/…` is a Windows drive
    match url.split_once(':') {
        Some((host, _)) => host.contains('/') || host.contains('\\') || host.len() == 1,
        None => true,
    }
}

/// Whether a URL is fetched without encryption (`http://`, `git://`, `ftp://`), so what it
/// gives could have been changed on the way: worth a warning
pub fn is_insecure(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "git://", "ftp://"]
        .iter()
        .any(|s| lower.starts_with(s))
}

/// A local file named by a path or a `file://` URL (anything else isn't local: an error)
pub fn read_local(url: &str) -> Result<Vec<u8>> {
    if url.contains("://") && !url.starts_with("file://") {
        return Err(LogoxideError::network(url, "no way to download here"));
    }
    let path = url.strip_prefix("file://").unwrap_or(url);
    std::fs::read(path).map_err(|e| LogoxideError::io(url, e))
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

    /// A file's bytes: a registry's catalog, or a release's archive. A path or `file://` URL is
    /// read from disk ([`read_local`]); fetchers that can reach the web download the rest
    fn download(&self, url: &str) -> Result<Vec<u8>> {
        read_local(url)
    }

    /**
    The first of `urls` (one file's places: its URL, then its mirrors) that downloads and passes
    `check`, and which it was. Each failure is tried past; when all fail, the error names each
    */
    fn download_any(
        &self,
        urls: &[String],
        check: &dyn Fn(&str, &[u8]) -> Result<()>,
    ) -> Result<(String, Vec<u8>)> {
        let mut failures = vec![];
        for url in urls {
            match self.download(url).and_then(|bytes| {
                check(url, &bytes)?;
                Ok(bytes)
            }) {
                Ok(bytes) => return Ok((url.clone(), bytes)),
                Err(e) if urls.len() == 1 => return Err(e),
                Err(e) => failures.push(e.to_string()),
            }
        }
        Err(LogoxideError::network(
            urls.first().map(String::as_str).unwrap_or_default(),
            match failures.is_empty() {
                true => String::from("nowhere to download it from"),
                false => failures.join("; "),
            },
        ))
    }

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

/// The longest a git command may take (a fetch, or anything but listing tags) before it's stopped
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// The longest listing a repository's tags (`git ls-remote`) may take
pub const LIST_TIMEOUT: Duration = Duration::from_secs(60);

/// Fetching with the `git` command
#[derive(Clone, Debug)]
pub struct GitCli {
    /// Where repositories are cloned while fetching (a temporary folder by default)
    pub scratch: Option<PathBuf>,
    /// The longest a fetch (or any git command but `ls-remote`) may take ([`FETCH_TIMEOUT`])
    pub fetch_timeout: Duration,
    /// The longest listing a repository's tags may take ([`LIST_TIMEOUT`])
    pub list_timeout: Duration,
}

impl Default for GitCli {
    fn default() -> Self {
        GitCli {
            scratch: None,
            fetch_timeout: FETCH_TIMEOUT,
            list_timeout: LIST_TIMEOUT,
        }
    }
}

/**
Runs git. Its `ext::` transport (which runs a command a URL names) is off, and transports git
leaves to the user's choice (`fd::`, a helper's) are refused, since URLs come from manifests
anyone writes. Local repositories stay allowed (git counts them among those): the resolver keeps
packages from elsewhere from naming them ([`is_local`]). It's stopped after `timeout`.

A repository's `core.fsmonitor` names a command git would run, so it's off everywhere (lx also
runs git in repositories it didn't make: a package's own, a registry's folder). Fetching runs
nothing the repository or its host asks for ([`Mode::Fetching`])
*/
fn git(
    args: &[&str],
    dir: Option<&Path>,
    url: &str,
    timeout: Duration,
    mode: Mode,
) -> Result<String> {
    let mut command = Command::new("git");
    command
        .args(["-c", "protocol.ext.allow=never"])
        .args(["-c", "protocol.file.allow=always"])
        .args(["-c", "core.fsmonitor=false"]);
    if let Mode::Fetching { hooks } = mode {
        // Git LFS would download whatever the repository's `.lfsconfig` names: its pointer files
        // are kept as they are, so a package hashes the same whether LFS is installed or not
        command
            .args(["-c", "filter.lfs.process=", "-c", "filter.lfs.smudge="])
            .args(["-c", "filter.lfs.required=false"])
            .env("GIT_LFS_SKIP_SMUDGE", "1");
        // Hooks (the user's own `core.hooksPath` included) don't run in lx's clones
        if let Some(hooks) = hooks {
            let mut setting = std::ffi::OsString::from("core.hooksPath=");
            setting.push(hooks);
            command.arg("-c").arg(setting);
        }
        // SSH never asks (for a passphrase, or to trust a host): it fails instead
        if !user_ssh_command() {
            command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
    }
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PROTOCOL_FROM_USER", "0");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    run(command, url, timeout)
}

/// How git is run
#[derive(Clone, Copy)]
enum Mode<'a> {
    /// In a repository of the user's (a package being released, a registry's folder)
    Local,
    /// Fetching from elsewhere, into a repository lx made: `hooks` is an empty folder for
    /// `core.hooksPath` when the command could run one
    Fetching { hooks: Option<&'a Path> },
}

/// Whether the user chose how git runs SSH (`GIT_SSH_COMMAND`, `GIT_SSH`, or `core.sshCommand`),
/// which lx then leaves alone
fn user_ssh_command() -> bool {
    static CHOSEN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CHOSEN.get_or_init(|| {
        if ["GIT_SSH_COMMAND", "GIT_SSH"]
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()))
        {
            return true;
        }
        let mut command = Command::new("git");
        command
            .args(["config", "--get", "core.sshCommand"])
            .current_dir(std::env::temp_dir());
        run(command, "core.sshCommand", LIST_TIMEOUT).is_ok_and(|v| !v.trim().is_empty())
    })
}

/**
Runs a command and gives what it printed, killing it if it takes longer than `timeout` (a host
that never answers, or sends a byte a minute, would otherwise hold lx forever). What it prints is
read as it runs, so a full pipe never stops it
*/
fn run(mut command: Command, url: &str, timeout: Duration) -> Result<String> {
    let fail = |message: String| LogoxideError::git(url, message);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| fail(format!("couldn't run git: {e}")))?;
    fn read_all(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = vec![];
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    }
    let stdout = read_all(child.stdout.take());
    let stderr = read_all(child.stderr.take());
    let start = Instant::now();
    let mut pause = Duration::from_millis(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let left = timeout.saturating_sub(start.elapsed());
                if left.is_zero() {
                    let _ = child.kill();
                    let _ = child.wait();
                    // The readers are left to finish alone: a helper git started (for HTTPS or
                    // SSH) may hold the pipes a moment longer
                    return Err(fail(format!(
                        "git took longer than {} and was stopped (`git-timeout` and \
                         `git-list-timeout` in logoxide's config change how long it may take)",
                        describe(timeout)
                    )));
                }
                std::thread::sleep(pause.min(left));
                pause = (pause * 2).min(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(fail(format!("couldn't wait for git: {e}")));
            }
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if !status.success() {
        return Err(fail(String::from_utf8_lossy(&stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// A time limit in words: `10 minutes`, `90 s`, `100 ms`
fn describe(timeout: Duration) -> String {
    match timeout.as_secs() {
        0 => format!("{} ms", timeout.as_millis()),
        60 => String::from("1 minute"),
        s if s % 60 == 0 => format!("{} minutes", s / 60),
        s => format!("{s} s"),
    }
}

impl GitCli {
    /// Runs git, stopped after [`GitCli::fetch_timeout`]
    fn git(&self, args: &[&str], dir: Option<&Path>, url: &str) -> Result<String> {
        git(args, dir, url, self.fetch_timeout, Mode::Local)
    }

    fn scratch_dir(&self, what: &str) -> PathBuf {
        let scratch = self.scratch.clone().unwrap_or_else(std::env::temp_dir);
        scratch.join(format!(
            "logoxide-{what}-{}-{}",
            std::process::id(),
            unique()
        ))
    }
}

/**
Refuses what git could read as an option (`--upload-pack=…` runs a command) or that isn't what it
says: a URL or reference starting with `-`, and a revision that isn't hex. Manifests are checked
when read ([`crate::manifest::Manifest::validate`]); this is for whatever else reaches git. Each is
also put after `--`, where git reads no options
*/
fn check_args(url: &str, reference: &GitRef) -> Result<()> {
    let refuse = |what: &str| {
        Err(LogoxideError::git(
            url,
            format!("{what} isn't one git should be given"),
        ))
    };
    if url.starts_with('-') || url.trim().is_empty() || url.contains('\0') {
        return refuse("this URL");
    }
    let named = match reference {
        GitRef::Tag(name) | GitRef::Branch(name) => name,
        GitRef::Rev(rev) => {
            if rev.is_empty() || !rev.bytes().all(|b| b.is_ascii_hexdigit()) {
                return refuse(&format!("revision `{rev}`"));
            }
            rev
        }
        GitRef::Head => return Ok(()),
    };
    if named.starts_with('-') || named.is_empty() || named.contains(['\0', ' ', ':']) {
        return refuse(&format!("`{named}`"));
    }
    Ok(())
}

/// A path's file in `dir`, if it's a plain file there: each folder on the way is a folder and
/// the file a file, none of them a link (whose target may be anywhere)
fn plain_file(dir: &Path, path: &str) -> Option<PathBuf> {
    let mut at = dir.to_path_buf();
    for part in path.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        at.push(part);
        let meta = std::fs::symlink_metadata(&at).ok()?;
        if meta.file_type().is_symlink() {
            return None;
        }
    }
    std::fs::symlink_metadata(&at)
        .ok()
        .filter(|m| m.file_type().is_file())
        .map(|_| at)
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
        self.in_clone("files", url, reference, |repo, git| {
            git(&["remote", "add", "--", "origin", url])?;
            git(&["config", "remote.origin.promisor", "true"])?;
            git(&["config", "remote.origin.partialclonefilter", "blob:none"])?;
            let wanted = wanted(reference);
            let filter = "--filter=blob:none";
            git(&[
                "fetch", "-q", "--depth", "1", filter, "--", "origin", &wanted,
            ])?;
            let mut sparse = vec!["sparse-checkout", "set", "--no-cone"];
            let patterns: Vec<String> = paths.iter().map(|p| format!("/{p}")).collect();
            sparse.extend(patterns.iter().map(String::as_str));
            git(&sparse)?;
            git(&["checkout", "-q", "FETCH_HEAD"])?;
            let commit = git(&["rev-parse", "HEAD"])?.trim().to_string();
            for path in paths {
                // Only a file in the clone: never through a link, which may lead anywhere
                if let Some(from) = plain_file(repo, path) {
                    let bytes = std::fs::read(&from).map_err(|e| LogoxideError::io(path, e))?;
                    dest.write(path, &bytes)?;
                }
            }
            Ok(commit)
        })
    }

    fn tags(&self, url: &str) -> Result<Vec<(String, String)>> {
        check_args(url, &GitRef::Head)?;
        let out = git(
            &["ls-remote", "--tags", "--", url],
            None,
            url,
            self.list_timeout,
            Mode::Fetching { hooks: None },
        )?;
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
        self.in_clone("fetch", url, reference, |repo, git| {
            git(&["fetch", "-q", "--depth", "1", "--", url, &wanted(reference)])?;
            git(&["checkout", "-q", "FETCH_HEAD"])?;
            let commit = git(&["rev-parse", "HEAD"])?.trim().to_string();
            copy_into(repo, "", dest)?;
            Ok(commit)
        })
    }
}

/// Runs git in one of lx's clones
type InClone<'a> = &'a dyn Fn(&[&str]) -> Result<String>;

impl GitCli {
    /**
    Runs `work` in a new, empty repository in the scratch folder (removed after), given the
    repository's folder and a way to run git there as [`Mode::Fetching`], with an empty folder
    beside it for hooks
    */
    fn in_clone(
        &self,
        what: &str,
        url: &str,
        reference: &GitRef,
        work: impl FnOnce(&Path, InClone) -> Result<String>,
    ) -> Result<String> {
        let scratch = self.scratch_dir(what);
        let (repo, hooks) = (scratch.join("repo"), scratch.join("hooks"));
        let result = (|| {
            for dir in [&repo, &hooks] {
                std::fs::create_dir_all(dir)
                    .map_err(|e| LogoxideError::io(&dir.display().to_string(), e))?;
            }
            check_args(url, reference)?;
            let mode = Mode::Fetching {
                hooks: Some(&hooks),
            };
            let git = |args: &[&str]| git(args, Some(&repo), url, self.fetch_timeout, mode);
            git(&["init", "-q"])?;
            work(&repo, &git)
        })();
        let _ = std::fs::remove_dir_all(&scratch);
        result
    }
}

impl GitCli {
    /// A local repository's tags that are versions (`v1.2.0`), newest last
    pub fn local_versions(&self, repo: &Path) -> Result<Vec<(semver::Version, String)>> {
        let (_, prefix) = self.package_in(repo)?;
        let out = self.git(&["tag", "--list"], Some(repo), &repo.display().to_string())?;
        let mut versions: Vec<(semver::Version, String)> = out
            .lines()
            .filter_map(|tag| {
                // A package in a folder of the repository has its own tags (`romans/v1.0.0`)
                let own = tag.strip_prefix(prefix.as_str())?;
                if own.contains('/') {
                    return None;
                }
                Some((crate::resolve::tag_version(own)?, tag.to_string()))
            })
            .collect();
        versions.sort();
        Ok(versions)
    }

    /**
    The repository a folder is in, and the folder's path in it with a slash after
    (`the-dorean-principle/`; `""` at its root). A package's release tags start with that path,
    so one repository can hold many packages, each released on its own
    */
    pub fn package_in(&self, dir: &Path) -> Result<(PathBuf, String)> {
        let at = dir.display().to_string();
        let root = self.git(&["rev-parse", "--show-toplevel"], Some(dir), &at)?;
        let prefix = self.git(&["rev-parse", "--show-prefix"], Some(dir), &at)?;
        Ok((PathBuf::from(root.trim()), prefix.trim().to_string()))
    }

    /// Commits everything in a folder of a local repository (only that folder: other packages in
    /// the repository are left as they are) and tags it
    pub fn commit_and_tag(&self, repo: &Path, message: &str, tag: &str) -> Result<String> {
        let at = repo.display().to_string();
        self.git(&["add", "-A", "--", "."], Some(repo), &at)?;
        // Nothing new (the summary written is the one committed): the commit there is tagged
        let staged = self.git(&["status", "--porcelain", "--", "."], Some(repo), &at)?;
        if !staged.trim().is_empty() {
            self.git(&["commit", "-q", "-m", message, "--", "."], Some(repo), &at)?;
        }
        self.git(&["tag", tag], Some(repo), &at)?;
        Ok(self
            .git(&["rev-parse", "HEAD"], Some(repo), &at)?
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
    let io = |path: &Path, e: std::io::Error| LogoxideError::io(&path.display().to_string(), e);
    for entry in std::fs::read_dir(from).map_err(|e| io(from, e))? {
        let entry = entry.map_err(|e| io(from, e))?;
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            continue;
        };
        if name == ".git" {
            continue;
        }
        let (source, path) = (entry.path(), crate::storage::join(prefix, &name));
        // The entry itself (not what a link leads to): links are left out, so nothing outside
        // the checkout is read
        let kind = entry.file_type().map_err(|e| io(&source, e))?;
        if kind.is_dir() {
            copy_into(&source, &path, to)?;
        } else if kind.is_file() {
            to.write(&path, &std::fs::read(&source).map_err(|e| io(&source, e))?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_and_insecure_urls() {
        for local in [
            "/srv/kjv",
            "../kjv",
            "kjv",
            "file:///srv/kjv",
            "FILE:///srv/kjv",
            "C:/kjv",
        ] {
            assert!(is_local(local), "{local}");
        }
        for remote in [
            "https://github.com/a/kjv",
            "ssh://git@host/kjv",
            "git@github.com:a/kjv",
            "github:a/kjv",
        ] {
            assert!(!is_local(remote), "{remote}");
        }
        assert!(is_insecure("http://ftp.xiphos.org/pub/xiphos/"));
        assert!(is_insecure("git://example.org/kjv"));
        assert!(!is_insecure("https://example.org/kjv"));
    }

    #[test]
    fn nothing_reaches_git_as_an_option() {
        let marker = std::env::temp_dir().join(format!("logoxide-pwned-{}", std::process::id()));
        let url = format!("--upload-pack=touch {}", marker.display());
        let cli = GitCli::default();
        assert!(cli.tags(&url).is_err());
        assert!(
            cli.fetch(&url, &GitRef::Head, &MemoryStorage::new())
                .is_err()
        );
        let files = MemoryStorage::new();
        assert!(
            cli.fetch_files(&url, &GitRef::Head, &["x"], &files)
                .is_err()
        );
        let bad_rev = GitRef::Rev(String::from("--output=/tmp/x"));
        assert!(check_args("https://e.org/x", &bad_rev).is_err());
        assert!(check_args("https://e.org/x", &GitRef::Tag("-x".into())).is_err());
        assert!(check_args("https://e.org/x", &GitRef::Tag("romans/v1.0.0".into())).is_ok());
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn links_lead_nowhere() {
        let root = std::env::temp_dir().join(format!("logoxide-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (repo, outside) = (root.join("repo"), root.join("outside"));
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), "secret").unwrap();
        std::fs::write(repo.join("ok.txt"), "ok").unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), repo.join("evil")).unwrap();
        std::os::unix::fs::symlink(&outside, repo.join("dir")).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        run(&["init", "-q"]);
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "links"]);
        let url = repo.display().to_string();
        let cli = GitCli::default();
        let all = MemoryStorage::new();
        cli.fetch(&url, &GitRef::Head, &all).unwrap();
        assert_eq!(all.walk("").unwrap(), ["ok.txt"]);
        let some = MemoryStorage::new();
        (cli.fetch_files(
            &url,
            &GitRef::Head,
            &["evil", "dir/secret", "ok.txt"],
            &some,
        ))
        .unwrap();
        assert_eq!(some.walk("").unwrap(), ["ok.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn git_has_a_deadline() {
        let mut slow = Command::new("sleep");
        slow.arg("30");
        let start = Instant::now();
        let error = run(slow, "https://e.org/slow", Duration::from_millis(200))
            .unwrap_err()
            .to_string();
        assert!(start.elapsed() < Duration::from_secs(10), "{error}");
        assert!(error.contains("longer than 200 ms"), "{error}");
        // What finishes in time is given back, however much it prints
        let mut loud = Command::new("sh");
        loud.args(["-c", "head -c 1000000 /dev/zero; echo done >&2"]);
        let out = run(loud, "x", Duration::from_secs(30)).unwrap();
        assert_eq!(out.len(), 1_000_000);
        assert_eq!(describe(FETCH_TIMEOUT), "10 minutes");
        assert_eq!(describe(LIST_TIMEOUT), "1 minute");
        assert_eq!(describe(Duration::from_secs(90)), "90 s");
    }

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
