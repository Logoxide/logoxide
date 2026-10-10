/*!
Fetchers for installing: [`GitFetcher`] through the `git` command, and [`NetFetcher`] over HTTP
alone (a repository's refs from git's smart HTTP, and archives of its commits: the core's
[`logoxide::net::archive`]) for hosts without `git`. Both download registries' catalogs and
archives through an [`Http`], and say through their [`Notes`] each repository fetched without
encryption. [`fetcher`] picks one by whether `git` runs here
*/

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

use logoxide::{
    LogoxideError, Result, Storage,
    git::{Fetcher, GitCli, GitRef, is_insecure, is_local, read_local},
    net::archive::{self, Refs},
};

use crate::{Note, Notes, http::Http};

/// The `git` command where it runs, else [`NetFetcher`]
pub fn fetcher(git: GitCli, http: Arc<dyn Http>, notes: Notes) -> Box<dyn Fetcher> {
    match logoxide::git::git_available() {
        true => Box::new(GitFetcher::new(git, http, notes)),
        false => Box::new(NetFetcher::new(http, notes)),
    }
}

/// Whether a URL is downloaded over HTTP (else it's a path or `file://`, read from disk)
fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Says, once a URL, a repository fetched without encryption (`git://`, `http://`), which a
/// package from elsewhere can't name (`git://`) but the project itself may
struct Warned {
    notes: Notes,
    warned: Mutex<BTreeSet<String>>,
}

impl Warned {
    fn new(notes: Notes) -> Warned {
        Warned {
            notes,
            warned: Mutex::default(),
        }
    }

    fn check(&self, url: &str) {
        if is_insecure(url)
            && (self.warned.lock()).is_ok_and(|mut warned| warned.insert(url.to_string()))
        {
            (self.notes)(&Note::Unencrypted {
                url: url.to_string(),
            });
        }
    }
}

/// Git through the `git` command, and downloads (registries' catalogs and archives) through an
/// [`Http`]
pub struct GitFetcher {
    git: GitCli,
    http: Arc<dyn Http>,
    warned: Warned,
}

impl GitFetcher {
    pub fn new(git: GitCli, http: Arc<dyn Http>, notes: Notes) -> GitFetcher {
        GitFetcher {
            git,
            http,
            warned: Warned::new(notes),
        }
    }
}

impl std::ops::Deref for GitFetcher {
    type Target = GitCli;
    fn deref(&self) -> &GitCli {
        &self.git
    }
}

impl Fetcher for GitFetcher {
    fn tags(&self, url: &str) -> Result<Vec<(String, String)>> {
        self.warned.check(url);
        self.git.tags(url)
    }

    fn fetch(&self, url: &str, reference: &GitRef, dest: &dyn Storage) -> Result<String> {
        self.warned.check(url);
        self.git.fetch(url, reference, dest)
    }

    fn fetch_files(
        &self,
        url: &str,
        reference: &GitRef,
        paths: &[&str],
        dest: &dyn Storage,
    ) -> Result<String> {
        self.warned.check(url);
        self.git.fetch_files(url, reference, paths, dest)
    }

    fn download(&self, url: &str) -> Result<Vec<u8>> {
        match is_web(url) {
            true => self.http.get(url),
            false => read_local(url),
        }
    }
}

/**
Git repositories over HTTP alone, for hosts without `git`: tags from the repository's refs, files
from an archive of the commit (an archive holds what a checkout does, so it hashes the same).
Repositories on this machine need `git`
*/
pub struct NetFetcher {
    http: Arc<dyn Http>,
    warned: Warned,
    /// Each repository's refs, read once
    refs: Mutex<HashMap<String, Refs>>,
}

impl NetFetcher {
    pub fn new(http: Arc<dyn Http>, notes: Notes) -> NetFetcher {
        NetFetcher {
            http,
            warned: Warned::new(notes),
            refs: Mutex::default(),
        }
    }

    fn refs(&self, url: &str) -> Result<Refs> {
        if let Some(refs) = self.refs.lock().ok().and_then(|r| r.get(url).cloned()) {
            return Ok(refs);
        }
        if is_local(url) {
            return Err(LogoxideError::git(
                url,
                "a repository on this machine needs git, which isn't installed",
            ));
        }
        self.warned.check(url);
        let refs = Refs::parse(&self.http.get(&archive::refs_url(url))?).map_err(|e| match e {
            LogoxideError::Git { message, .. } => LogoxideError::git(url, message),
            e => e,
        })?;
        if let Ok(mut known) = self.refs.lock() {
            known.insert(url.to_string(), refs.clone());
        }
        Ok(refs)
    }
}

impl Fetcher for NetFetcher {
    fn tags(&self, url: &str) -> Result<Vec<(String, String)>> {
        Ok(self.refs(url)?.tags)
    }

    fn download(&self, url: &str) -> Result<Vec<u8>> {
        match is_web(url) {
            true => self.http.get(url),
            false => read_local(url),
        }
    }

    fn fetch(&self, url: &str, reference: &GitRef, dest: &dyn Storage) -> Result<String> {
        // A whole commit (a lock file's) needs no refs
        let commit = match reference {
            GitRef::Rev(rev)
                if (rev.len() == 40 || rev.len() == 64)
                    && rev.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                rev.to_ascii_lowercase()
            }
            _ => self
                .refs(url)?
                .find_commit(reference)
                .map_err(|e| match e {
                    LogoxideError::Git { message, .. } => LogoxideError::git(url, message),
                    e => e,
                })?,
        };
        if is_local(url) {
            return Err(LogoxideError::git(
                url,
                "a repository on this machine needs git, which isn't installed",
            ));
        }
        self.warned.check(url);
        // The host's form first, then the others'
        let mut failures = vec![];
        for at in archive::archive_urls(url, &commit) {
            match self.http.get(&at) {
                Ok(bytes) => {
                    archive::unpack(&bytes, dest)?;
                    return Ok(commit);
                }
                Err(e) => failures.push(e.to_string()),
            }
        }
        Err(LogoxideError::network(url, failures.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers each URL from a table
    struct Table(HashMap<String, Vec<u8>>);

    impl Http for Table {
        fn get(&self, url: &str) -> Result<Vec<u8>> {
            (self.0.get(url).cloned()).ok_or_else(|| LogoxideError::network(url, "404"))
        }
    }

    /// A pkt-line
    fn line(text: &str) -> String {
        format!("{:04x}{text}", text.len() + 4)
    }

    /// A `.tar.gz` of one folder holding `files`
    fn tarball(files: &[(&str, &str)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            vec![],
            flate2::Compression::fast(),
        ));
        for (path, text) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            (builder.append_data(&mut header, format!("repo-abc/{path}"), text.as_bytes()))
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn repositories_are_read_over_http() {
        let commit = "a".repeat(40);
        let url = "https://git.example.org/alice/kjv";
        let refs = format!(
            "{}0000{}{}0000",
            line("# service=git-upload-pack\n"),
            line(&format!("{commit} HEAD\0side-band\n")),
            line(&format!("{commit} refs/tags/v1.0.0\n")),
        );
        let mut table = HashMap::new();
        table.insert(archive::refs_url(url), refs.into_bytes());
        table.insert(
            archive::archive_urls(url, &commit)[0].clone(),
            tarball(&[("logoxide.toml", "[package]\n")]),
        );
        let quiet: Notes = Arc::new(|_: &Note| {});
        let fetcher = NetFetcher::new(Arc::new(Table(table)), quiet);
        assert_eq!(
            fetcher.tags(url).unwrap(),
            vec![(String::from("v1.0.0"), commit.clone())]
        );
        let dest = logoxide::MemoryStorage::new();
        let got = (fetcher.fetch(url, &GitRef::Tag(String::from("v1.0.0")), &dest)).unwrap();
        assert_eq!(got, commit);
        assert_eq!(dest.read_string("logoxide.toml").unwrap(), "[package]\n");
        // A local repository needs git
        assert!(fetcher.tags("/some/folder").is_err());
    }
}
