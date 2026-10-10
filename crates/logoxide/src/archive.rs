/*!
Git repositories over plain HTTP, for hosts without `git` (WebAssembly, phones): a repository's
refs from git's smart HTTP (which GitHub, GitLab, Gitea, Forgejo, and Bitbucket all serve), and
archives of its commits. The host downloads; this reads what it downloaded.

An archive holds what a checkout holds, so its checksum is the same, unless the repository leaves
files out of archives (`export-ignore` in `.gitattributes`).
*/

use std::io::Read;

use crate::{
    error::{LogoxideError, Result},
    git::GitRef,
    storage::Storage,
};

/// A repository's URL without a trailing `/` or `.git`
fn base(url: &str) -> &str {
    let url = url.trim_end_matches('/');
    url.strip_suffix(".git").unwrap_or(url)
}

/// Where a repository lists its refs: `<url>/info/refs?service=git-upload-pack` (a GET, without a
/// `Git-Protocol` header, so the answer is the list itself)
pub fn refs_url(url: &str) -> String {
    format!("{}.git/info/refs?service=git-upload-pack", base(url))
}

/// A repository's refs, read from its [`refs_url`]'s answer
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refs {
    /// The commit `HEAD` names (the default branch)
    pub head: Option<String>,
    /// Each tag and its commit (an annotated tag's commit, not the tag object)
    pub tags: Vec<(String, String)>,
    /// Each branch and its commit
    pub branches: Vec<(String, String)>,
}

impl Refs {
    /// Reads git's pkt-line advertisement: a 4-digit hex length, then `<commit> <ref>`, the first
    /// with capabilities after a NUL
    pub fn parse(bytes: &[u8]) -> Result<Refs> {
        let bad = |message: &str| LogoxideError::Fetch {
            url: String::from("info/refs"),
            message: message.to_string(),
        };
        let mut refs = Refs::default();
        let mut rest = bytes;
        let mut lines = 0;
        while rest.len() >= 4 {
            let len = std::str::from_utf8(&rest[..4])
                .ok()
                .and_then(|hex| usize::from_str_radix(hex, 16).ok())
                .ok_or_else(|| bad("not git's smart HTTP (is the URL a git repository?)"))?;
            // 0000 is a flush; 0001 and 0002 are delimiters in newer protocols
            if len < 4 {
                rest = &rest[4..];
                continue;
            }
            let line = rest.get(4..len).ok_or_else(|| bad("an answer cut short"))?;
            rest = &rest[len..];
            lines += 1;
            let line = String::from_utf8_lossy(line);
            let line = line.split('\0').next().unwrap_or_default().trim_end();
            if line.starts_with('#') {
                continue;
            }
            let Some((commit, name)) = line.split_once(' ') else {
                continue;
            };
            let commit = commit.to_string();
            if name == "HEAD" {
                refs.head = Some(commit);
            } else if let Some(tag) = name.strip_prefix("refs/tags/") {
                match tag.strip_suffix("^{}") {
                    // An annotated tag: this line names its commit
                    Some(tag) => {
                        if let Some(entry) = refs.tags.iter_mut().find(|(t, _)| t == tag) {
                            entry.1 = commit;
                        }
                    }
                    None => refs.tags.push((tag.to_string(), commit)),
                }
            } else if let Some(branch) = name.strip_prefix("refs/heads/") {
                refs.branches.push((branch.to_string(), commit));
            }
        }
        if lines == 0 {
            return Err(bad("no refs (is the URL a git repository?)"));
        }
        Ok(refs)
    }

    /// The commit a reference names: a tag's, a branch's, `HEAD`'s, or a full revision itself
    pub fn commit(&self, reference: &GitRef) -> Option<String> {
        let find = |list: &[(String, String)], name: &str| {
            list.iter().find(|(n, _)| n == name).map(|(_, c)| c.clone())
        };
        match reference {
            GitRef::Tag(tag) => find(&self.tags, tag),
            GitRef::Branch(branch) => find(&self.branches, branch),
            GitRef::Head => self.head.clone(),
            GitRef::Rev(rev) => {
                let all = self.tags.iter().chain(&self.branches).map(|(_, c)| c);
                let known = all.chain(&self.head).find(|c| c.starts_with(rev.as_str()));
                match known {
                    Some(commit) => Some(commit.clone()),
                    // A full commit id needn't be a ref's
                    None => (rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()))
                        .then(|| rev.clone()),
                }
            }
        }
    }
}

/// Where hosts serve a commit as a `.tar.gz`, most likely first: GitHub, Gitea, Forgejo, and
/// Codeberg use `/archive/`, GitLab `/-/archive/`, Bitbucket `/get/`
pub fn archive_urls(url: &str, commit: &str) -> Vec<String> {
    let base = base(url);
    let name = base.rsplit('/').next().unwrap_or("archive");
    let github = format!("{base}/archive/{commit}.tar.gz");
    let gitlab = format!("{base}/-/archive/{commit}/{name}-{commit}.tar.gz");
    let bitbucket = format!("{base}/get/{commit}.tar.gz");
    let host = base
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host.contains("gitlab") {
        vec![gitlab, github]
    } else if host.contains("bitbucket") {
        vec![bitbucket]
    } else {
        vec![github, gitlab]
    }
}

/**
Unpacks a `.tar.gz` of a repository into `dest`. Hosts put everything in one folder named for
the repository and commit (`kjv-3f9c2a1b/`), which is left out; a package has its `logoxide.toml`
at the top. Only files are kept (not links)
*/
pub fn unpack(bytes: &[u8], dest: &dyn Storage) -> Result<()> {
    let bad = |e: std::io::Error| LogoxideError::Fetch {
        url: String::from("archive"),
        message: format!("not a .tar.gz: {e}"),
    };
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(bad)? {
        let mut entry = entry.map_err(bad)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(bad)?;
        let Some(path) = path
            .to_str()
            .map(|p| p.trim_start_matches("./").to_string())
        else {
            continue;
        };
        let mut data = vec![];
        entry.read_to_end(&mut data).map_err(bad)?;
        files.push((path, data));
    }
    // The one folder everything is in, unless the files are at the top
    let top = files
        .first()
        .and_then(|(p, _)| p.split_once('/'))
        .map(|(top, _)| format!("{top}/"));
    let strip = top.filter(|top| files.iter().all(|(p, _)| p.starts_with(top.as_str())));
    for (path, data) in files {
        let path = match &strip {
            Some(top) => &path[top.len()..],
            None => &path[..],
        };
        if path.split('/').any(|part| part == ".git") {
            continue;
        }
        dest.write(path, &data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    fn pkt(line: &str) -> String {
        format!("{:04x}{line}", line.len() + 4)
    }

    #[test]
    fn refs_from_smart_http() {
        let head = "a".repeat(40);
        let tag_object = "b".repeat(40);
        let tagged = "c".repeat(40);
        let light = "d".repeat(40);
        let answer = [
            pkt("# service=git-upload-pack\n"),
            String::from("0000"),
            pkt(&format!(
                "{head} HEAD\0multi_ack symref=HEAD:refs/heads/main\n"
            )),
            pkt(&format!("{head} refs/heads/main\n")),
            pkt(&format!("{tag_object} refs/tags/v1.0.0\n")),
            pkt(&format!("{tagged} refs/tags/v1.0.0^{{}}\n")),
            pkt(&format!("{light} refs/tags/v1.1.0\n")),
            String::from("0000"),
        ]
        .concat();
        let refs = Refs::parse(answer.as_bytes()).unwrap();
        assert_eq!(refs.head.as_deref(), Some(head.as_str()));
        assert_eq!(
            refs.tags,
            [
                ("v1.0.0".to_string(), tagged.clone()),
                ("v1.1.0".to_string(), light.clone())
            ]
        );
        assert_eq!(refs.commit(&GitRef::Tag("v1.0.0".into())), Some(tagged));
        assert_eq!(
            refs.commit(&GitRef::Branch("main".into())),
            Some(head.clone())
        );
        assert_eq!(refs.commit(&GitRef::Rev("dddd".into())), Some(light));
        assert_eq!(refs.commit(&GitRef::Rev("eeee".into())), None);
        assert_eq!(
            refs.commit(&GitRef::Rev("e".repeat(40))),
            Some("e".repeat(40))
        );
        assert!(Refs::parse(b"<html>not found</html>").is_err());
    }

    #[test]
    fn archive_urls_by_host() {
        let commit = "abc";
        assert_eq!(
            archive_urls("https://github.com/someone/kjv.git", commit)[0],
            "https://github.com/someone/kjv/archive/abc.tar.gz"
        );
        assert_eq!(
            archive_urls("https://gitlab.com/group/sub/kjv/", commit)[0],
            "https://gitlab.com/group/sub/kjv/-/archive/abc/kjv-abc.tar.gz"
        );
        assert_eq!(
            archive_urls("https://bitbucket.org/someone/kjv", commit),
            ["https://bitbucket.org/someone/kjv/get/abc.tar.gz"]
        );
        assert_eq!(
            refs_url("https://github.com/someone/kjv"),
            "https://github.com/someone/kjv.git/info/refs?service=git-upload-pack"
        );
    }

    fn tar_gz(files: &[(&str, &str)]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(gz);
        for (path, text) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path, text.as_bytes())
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn unpacks_without_the_top_folder() {
        let bytes = tar_gz(&[
            ("kjv-abc/logoxide.toml", "[package]"),
            ("kjv-abc/usfm/JHN.usfm", "\\id JHN"),
        ]);
        let dest = MemoryStorage::new();
        unpack(&bytes, &dest).unwrap();
        assert_eq!(dest.walk("").unwrap(), ["logoxide.toml", "usfm/JHN.usfm"]);
        // Already at the top
        let bytes = tar_gz(&[
            ("logoxide.toml", "[package]"),
            ("usfm/JHN.usfm", "\\id JHN"),
        ]);
        let dest = MemoryStorage::new();
        unpack(&bytes, &dest).unwrap();
        assert_eq!(dest.read_string("usfm/JHN.usfm").unwrap(), "\\id JHN");
        assert!(unpack(b"not gzip", &MemoryStorage::new()).is_err());
    }
}
