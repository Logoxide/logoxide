/*!
The files a text is read from: a read-only view of a folder, memory, or a host's files, so
nothing here touches `std::fs`. Paths are plain and relative (`usx/JHN.usx`), `/`-separated.
*/

use std::collections::BTreeMap;

use crate::error::{Result, TextError};

pub trait Files: Send + Sync {
    /// A file's bytes
    fn read(&self, path: &str) -> Result<Vec<u8>>;

    /// The names (not paths) of the files and folders directly in `dir` (`""` for the root)
    fn list(&self, dir: &str) -> Result<Vec<String>>;

    /// Every file under `dir`, at any depth, by path
    fn walk(&self, dir: &str) -> Result<Vec<String>>;

    fn exists(&self, path: &str) -> bool {
        self.read(path).is_ok()
    }

    /// A file's text: UTF-8, else the bytes read leniently
    fn read_string(&self, path: &str) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.read(path)?).into_owned())
    }
}

/// `dir/name` (`name` alone at the root)
pub fn join(dir: &str, name: &str) -> String {
    match dir.trim_end_matches('/') {
        "" => name.to_string(),
        dir => format!("{dir}/{name}"),
    }
}

/// Files in memory: the host's bytes, or a test's
#[derive(Clone, Debug, Default)]
pub struct MemoryFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemoryFiles {
    pub fn with<P: Into<String>, B: Into<Vec<u8>>>(
        files: impl IntoIterator<Item = (P, B)>,
    ) -> Self {
        let files = (files.into_iter())
            .map(|(p, b)| (p.into().trim_start_matches("./").to_string(), b.into()))
            .collect();
        Self { files }
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

impl Files for MemoryFiles {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        (self.files.get(path).cloned()).ok_or_else(|| TextError::NotFound(path.to_string()))
    }

    fn list(&self, dir: &str) -> Result<Vec<String>> {
        let prefix = match dir.trim_end_matches('/') {
            "" => String::new(),
            dir => format!("{dir}/"),
        };
        let mut names: Vec<String> = (self.files.keys())
            .filter_map(|p| p.strip_prefix(&prefix))
            .map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn walk(&self, dir: &str) -> Result<Vec<String>> {
        let prefix = match dir.trim_end_matches('/') {
            "" => String::new(),
            dir => format!("{dir}/"),
        };
        Ok((self.files.keys())
            .filter(|p| p.starts_with(&prefix))
            .cloned()
            .collect())
    }

    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}
