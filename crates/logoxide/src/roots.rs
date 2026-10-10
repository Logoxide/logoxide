/*!
The projects that use a library (`<library>/roots/<hash>.toml`), so `lx clean` keeps what each
of them locks: a vault elsewhere on this machine, or on another sharing the library through a
sync service, would otherwise lose its versions to a clean run from here.

Each root is written whenever its project installs ([`crate::Library::record_root`]), and
names the project (its path and the host it's on), when it was written, and the packages its
lock holds in the library:

```toml
# Written by lx: a project that uses this library, so `lx clean` keeps its packages
path = "/home/me/notes"
host = "laptop"
updated = "2026-10-10T14:03:11Z"
packages = ["kjv 1.0.3+gdfdf94a0", "bsb 1.0.3+g9f86d081"]
```

A root whose project is gone (on this host, its `logoxide.toml` no longer there) is reported
by `lx clean` and removed by `lx clean --prune-roots`; one from another host is kept, since this
host can't see whether the project is still there.
*/

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    error::{LogoxideError, Result},
    lock::Lockfile,
    manifest::MANIFEST,
};

/// Where roots go in the library
pub const ROOTS_DIR: &str = "roots";

/// A project that uses the library, as its root file says
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Root {
    /// The project's folder, as this host names it
    pub path: String,
    /// The host it's on
    pub host: String,
    /// When it was written (RFC 3339, UTC)
    pub updated: String,
    /// The packages its lock holds in the library, each `<id> <key>`
    pub packages: Vec<String>,
    /// The root's own file, when read from one
    #[serde(skip)]
    pub file: Option<PathBuf>,
}

impl Root {
    /// A project's root as it is now: the packages its lock keeps in the library (not path
    /// packages, which are its own files)
    pub fn of(project: &Path, lock: &Lockfile) -> Root {
        let path = project
            .canonicalize()
            .or_else(|_| std::path::absolute(project))
            .unwrap_or_else(|_| project.to_path_buf());
        Root {
            path: path.display().to_string(),
            host: host_name(),
            updated: rfc3339(std::time::SystemTime::now()),
            packages: (lock.packages.iter())
                .filter(|p| p.path.is_none())
                .map(|p| format!("{} {}", p.id, p.key()))
                .collect(),
            file: None,
        }
    }

    /// The root file's name: a hash of the host and the path, so a path on two hosts is two
    pub fn file_name(&self) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(format!("{}\0{}", self.host, self.path).as_bytes());
        let hex: String = hash.iter().take(16).map(|b| format!("{b:02x}")).collect();
        format!("{hex}.toml")
    }

    /// Whether the project is on this host and no longer there (its `logoxide.toml` is gone)
    pub fn is_gone(&self) -> bool {
        self.host == host_name() && !Path::new(&self.path).join(MANIFEST).is_file()
    }

    /// The packages as `(id, key)`
    pub fn package_keys(&self) -> Vec<(String, String)> {
        (self.packages.iter())
            .filter_map(|name| name.split_once(' '))
            .map(|(id, key)| (id.to_string(), key.to_string()))
            .collect()
    }

    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string(self).map_err(|e| LogoxideError::Lock(e.to_string()))?;
        Ok(format!(
            "# Written by lx: a project that uses this library, so `lx clean` keeps its packages\n{body}"
        ))
    }

    pub fn parse(text: &str) -> Result<Root> {
        toml::from_str(text).map_err(|e| LogoxideError::Lock(format!("root: {e}")))
    }
}

/// Writes a project's root in `roots` (whole or not at all)
pub fn write_root(roots: &Path, root: &Root) -> Result<()> {
    let path = roots.join(root.file_name());
    crate::storage::write_atomic(&path, root.to_toml()?.as_bytes())
        .map_err(|e| LogoxideError::io(&path.display().to_string(), e))
}

/// Every root in `roots`, by file name; one that can't be read is an error naming it, since
/// cleaning without knowing what it keeps could remove a project's versions
pub fn read_roots(roots: &Path) -> Result<Vec<Root>> {
    let mut found = vec![];
    let entries = match std::fs::read_dir(roots) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(LogoxideError::io(&roots.display().to_string(), e)),
    };
    let mut files: Vec<PathBuf> = (entries.flatten().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    for file in files {
        let shown = file.display().to_string();
        let text = std::fs::read_to_string(&file).map_err(|e| LogoxideError::io(&shown, e))?;
        let mut root =
            Root::parse(&text).map_err(|e| LogoxideError::Lock(format!("{shown}: {e}")))?;
        root.file = Some(file);
        found.push(root);
    }
    Ok(found)
}

/// This machine's name, as the operating system gives it (`unknown` when it won't)
pub fn host_name() -> String {
    #[cfg(unix)]
    {
        let mut buffer = [0u8; 256];
        // SAFETY: gethostname writes at most the given length into the buffer, which lives for
        // the call; a name longer than it is cut, which is fine for telling hosts apart
        let code = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if code == 0 {
            let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
            let name = String::from_utf8_lossy(&buffer[..end]).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }
    for var in ["HOSTNAME", "COMPUTERNAME"] {
        if let Some(name) = std::env::var_os(var).filter(|v| !v.is_empty()) {
            return name.to_string_lossy().into_owned();
        }
    }
    String::from("unknown")
}

/// A time as `2026-10-10T14:03:11Z`
pub fn rfc3339(time: std::time::SystemTime) -> String {
    let seconds = (time.duration_since(std::time::UNIX_EPOCH))
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (hour, minute, second) = (rest / 3600, rest % 3600 / 60, rest % 60);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_rfc3339() {
        let at = |s: u64| rfc3339(std::time::UNIX_EPOCH + std::time::Duration::from_secs(s));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_791_727_391), "2026-10-11T14:03:11Z");
    }

    #[test]
    fn roots_are_written_read_and_judged_gone() {
        let dir = std::env::temp_dir().join(format!("logoxide-roots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("vault");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(MANIFEST), "[dependencies]\n").unwrap();
        let lock = Lockfile::parse(
            "version = 1\ndependencies = [\"kjv 1.0.0+gabcdef12\", \"own 1.0.0\"]\n\n[[package]]\nid = \"kjv\"\nversion = \"1.0.0\"\ngit = \"https://example.org/kjv\"\ncommit = \"abcdef1234567890\"\n\n[[package]]\nid = \"own\"\nversion = \"1.0.0\"\npath = \"own\"\n",
        )
        .unwrap();
        let root = Root::of(&project, &lock);
        assert_eq!(root.packages, ["kjv 1.0.0+gabcdef12"]);
        assert!(!root.is_gone());
        let roots = dir.join("roots");
        write_root(&roots, &root).unwrap();
        let read = read_roots(&roots).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].path, root.path);
        assert_eq!(
            read[0].package_keys(),
            [(String::from("kjv"), String::from("1.0.0+gabcdef12"))]
        );
        assert!(read[0].file.is_some());
        // The same project writes the same file again
        write_root(&roots, &Root::of(&project, &lock)).unwrap();
        assert_eq!(read_roots(&roots).unwrap().len(), 1);
        // Gone once its manifest is
        std::fs::remove_file(project.join(MANIFEST)).unwrap();
        assert!(read_roots(&roots).unwrap()[0].is_gone());
        // A root from another host is never gone from here
        let mut elsewhere = root.clone();
        elsewhere.host = String::from("another-host");
        assert!(!elsewhere.is_gone());
        // A root that can't be read stops cleaning
        std::fs::write(roots.join("bad.toml"), "path = 1\n").unwrap();
        assert!(read_roots(&roots).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
