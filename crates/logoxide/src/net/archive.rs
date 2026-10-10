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
        let bad = |message: &str| LogoxideError::git("info/refs", message);
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

    /// The commit a reference names: a tag's, a branch's, `HEAD`'s, or a full revision itself.
    /// `None` when nothing does, or a revision is too short or names more than one commit
    /// ([`Refs::find_commit`] says which)
    pub fn commit(&self, reference: &GitRef) -> Option<String> {
        self.find_commit(reference).ok()
    }

    /**
    [`Refs::commit`], with what went wrong. A revision is a commit's hex: in full (which needn't be
    a ref's), or at least [`MIN_REV_PREFIX`] digits of one of the refs' commits, and only one
    */
    pub fn find_commit(&self, reference: &GitRef) -> Result<String> {
        let missing = |what: String| LogoxideError::git("info/refs", what);
        let find = |list: &[(String, String)], name: &str| {
            list.iter().find(|(n, _)| n == name).map(|(_, c)| c.clone())
        };
        match reference {
            GitRef::Tag(tag) => {
                find(&self.tags, tag).ok_or_else(|| missing(format!("no tag {tag}")))
            }
            GitRef::Branch(branch) => {
                (find(&self.branches, branch)).ok_or_else(|| missing(format!("no branch {branch}")))
            }
            GitRef::Head => (self.head.clone()).ok_or_else(|| missing(String::from("no HEAD"))),
            GitRef::Rev(rev) => {
                let rev = rev.to_ascii_lowercase();
                let hex = rev.bytes().all(|b| b.is_ascii_hexdigit());
                if !hex || rev.len() < MIN_REV_PREFIX || rev.len() > 64 {
                    return Err(missing(format!(
                        "revision `{rev}`: give at least {MIN_REV_PREFIX} hex digits of a commit"
                    )));
                }
                let all = self.tags.iter().chain(&self.branches).map(|(_, c)| c);
                let mut known: Vec<&String> = (all.chain(&self.head))
                    .filter(|c| c.to_ascii_lowercase().starts_with(&rev))
                    .collect();
                known.sort();
                known.dedup();
                match known.as_slice() {
                    [commit] => Ok((*commit).clone()),
                    // A full commit id needn't be a ref's
                    [] if rev.len() == 40 || rev.len() == 64 => Ok(rev),
                    [] => Err(missing(format!(
                        "revision `{rev}`: no ref names it (give the whole commit)"
                    ))),
                    many => Err(missing(format!(
                        "revision `{rev}` is the start of {} commits: give more of it",
                        many.len()
                    ))),
                }
            }
        }
    }
}

/// The fewest hex digits of a commit a revision may give over HTTP (as `git` shows them)
pub const MIN_REV_PREFIX: usize = 7;

/// The most an archive may unpack to in all (1 GiB), so a small download can't fill memory or a
/// disk
pub const MAX_UNPACKED_BYTES: u64 = 1 << 30;

/// The most one file in an archive may be (1 GiB)
pub const MAX_ENTRY_BYTES: u64 = 1 << 30;

/// The most entries (files, folders, links) an archive may have
pub const MAX_ENTRIES: usize = 200_000;

/**
Reads one archive entry's bytes (its size said to be `size`) within the limits, counting them in
`total`: [`MAX_ENTRY_BYTES`] a file and [`MAX_UNPACKED_BYTES`] in all. What's read is capped, so an
entry that says it's small and isn't is caught too
*/
pub(crate) fn read_limited(
    reader: impl Read,
    size: u64,
    total: &mut u64,
    name: &str,
) -> std::result::Result<Vec<u8>, String> {
    let room = MAX_ENTRY_BYTES.min(MAX_UNPACKED_BYTES.saturating_sub(*total));
    if size > room {
        return Err(format!("{name}: too large to unpack ({size} bytes)"));
    }
    let mut data = Vec::with_capacity(usize::try_from(size.min(1 << 20)).unwrap_or(0));
    reader
        .take(room + 1)
        .read_to_end(&mut data)
        .map_err(|e| format!("{name}: {e}"))?;
    if data.len() as u64 > room {
        return Err(format!("{name}: too large to unpack"));
    }
    *total += data.len() as u64;
    Ok(data)
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
at the top. Only files are kept (not links).

The archive is read twice: once for its names and sizes, to find that folder and check the limits
([`MAX_ENTRIES`], [`MAX_ENTRY_BYTES`], [`MAX_UNPACKED_BYTES`]) before anything is written, then
again to write each file to `dest` as it's read, so only one file is held at a time
*/
pub fn unpack(bytes: &[u8], dest: &dyn Storage) -> Result<()> {
    let bad = |e: std::io::Error| LogoxideError::archive("archive", format!("not a .tar.gz: {e}"));
    let too_large = |message: String| LogoxideError::archive("archive", message);
    // The files' names, and the room they say they need
    let mut paths: Vec<String> = vec![];
    let (mut entries, mut declared) = (0, 0u64);
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(bad)? {
        let entry = entry.map_err(bad)?;
        entries += 1;
        if entries > MAX_ENTRIES {
            return Err(too_large(format!("more than {MAX_ENTRIES} entries")));
        }
        let Some(path) = file_path(&entry).map_err(bad)? else {
            continue;
        };
        let size = entry.header().size().map_err(bad)?;
        declared = declared.saturating_add(size);
        if size > MAX_ENTRY_BYTES || declared > MAX_UNPACKED_BYTES {
            return Err(too_large(format!(
                "{path}: too large to unpack ({size} bytes)"
            )));
        }
        paths.push(path);
    }
    // The one folder everything is in, unless the files are at the top
    let top = paths
        .first()
        .and_then(|p| p.split_once('/'))
        .map(|(top, _)| format!("{top}/"));
    let strip = top.filter(|top| paths.iter().all(|p| p.starts_with(top.as_str())));
    let mut total = 0;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries().map_err(bad)? {
        let entry = entry.map_err(bad)?;
        let Some(path) = file_path(&entry).map_err(bad)? else {
            continue;
        };
        let inside = match &strip {
            Some(top) => path.get(top.len()..).unwrap_or_default(),
            None => &path[..],
        };
        if inside.is_empty() || inside.split('/').any(|part| part == ".git") {
            continue;
        }
        let inside = inside.to_string();
        let size = entry.header().size().map_err(bad)?;
        // Capped again as it's read, should an entry hold more than it said
        let data = read_limited(entry, size, &mut total, &path).map_err(too_large)?;
        dest.write(&inside, &data)?;
    }
    Ok(())
}

/// An entry's path, if it's a file and its path is text (`./` dropped)
fn file_path<R: Read>(entry: &tar::Entry<R>) -> std::io::Result<Option<String>> {
    if !entry.header().entry_type().is_file() {
        return Ok(None);
    }
    let path = entry.path()?;
    Ok(path
        .to_str()
        .map(|p| p.trim_start_matches("./").to_string()))
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
        assert_eq!(refs.commit(&GitRef::Rev("ddddddd".into())), Some(light));
        assert_eq!(refs.commit(&GitRef::Rev("eeeeeee".into())), None);
        // Too short to say which (an empty one matched the first commit once)
        for short in ["", "d", "dddd", "dddddd"] {
            assert_eq!(refs.commit(&GitRef::Rev(short.into())), None, "{short}");
        }
        assert!(refs.commit(&GitRef::Rev("--upload-pack".into())).is_none());
        // Two commits starting the same: ambiguous, said so
        let mut two = refs.clone();
        let near = format!("{}e", "d".repeat(39));
        two.branches.push(("other".into(), near.clone()));
        let error = two.find_commit(&GitRef::Rev("ddddddd".into())).unwrap_err();
        assert!(error.to_string().contains("2 commits"), "{error}");
        assert_eq!(two.find_commit(&GitRef::Rev(near.clone())).unwrap(), near);
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

    #[test]
    fn unpacking_has_limits() {
        // A file larger than it says, or than what's left, is refused
        let mut total = 0;
        assert_eq!(
            read_limited(&b"hello"[..], 5, &mut total, "a").unwrap(),
            b"hello"
        );
        assert_eq!(total, 5);
        let mut total = MAX_UNPACKED_BYTES - 10;
        assert!(read_limited(&[0u8; 20][..], 5, &mut total, "a").is_err());
        assert!(read_limited(&[0u8; 5][..], 20, &mut total, "a").is_err());
        assert!(read_limited(&b""[..], MAX_ENTRY_BYTES + 1, &mut 0, "a").is_err());
    }

    #[test]
    fn nothing_is_written_from_an_archive_too_large() {
        // A small file, then one that says it's 2 GiB (its bytes cut short): refused before
        // the first is written
        let header = |path: &str, size: u64| {
            let mut header = tar::Header::new_gnu();
            header.set_path(path).unwrap();
            header.set_size(size);
            header.set_mode(0o644);
            header.set_cksum();
            header.as_bytes().to_vec()
        };
        let mut tar = header("kjv-abc/logoxide.toml", 9);
        let mut data = b"[package]".to_vec();
        data.resize(512, 0);
        tar.extend(data);
        tar.extend(header("kjv-abc/big.usx", 2 << 30));
        tar.extend([0; 512]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        let bytes = gz.finish().unwrap();
        let dest = MemoryStorage::new();
        let error = unpack(&bytes, &dest).unwrap_err();
        assert!(error.to_string().contains("too large"), "{error}");
        assert!(dest.walk("").unwrap().is_empty());
    }

    #[test]
    fn the_top_folder_is_judged_by_every_file() {
        // The first file is in a folder, but not every one is: nothing is left out
        let bytes = tar_gz(&[
            ("usfm/JHN.usfm", "\\id JHN"),
            ("logoxide.toml", "[package]"),
        ]);
        let dest = MemoryStorage::new();
        unpack(&bytes, &dest).unwrap();
        assert_eq!(dest.walk("").unwrap(), ["logoxide.toml", "usfm/JHN.usfm"]);
    }
}
