/*!
logoxide's own config, `~/.config/logoxide/config.toml` (`$XDG_CONFIG_HOME/logoxide`, or the
file `LOGOXIDE_CONFIG` names): this machine's defaults for `lx` and logoxide-lsp. Every key is
optional; without the file nothing changes. topos's settings (book data, formats) stay in topos's
own `config.toml`.

```toml
library = "~/Bibles"                 # LOGOXIDE_HOME and --library win
cache = "~/.cache/logoxide"          # LOGOXIDE_CACHE and --cache-dir win
catalogs = ["https://example.org/catalog.toml"]   # searched in every project, after its own

git-timeout = 600                    # seconds a git fetch may take (10 minutes)
git-list-timeout = 60                # seconds listing a repository's tags may take
catalog-max-age = "7d"               # how long a downloaded catalog is used before it's
                                     # downloaded again: seconds, or "90s", "30m", "12h", "7d",
                                     # "2w"; 0 keeps none

[registries]                         # every project's, besides its own (its own name wins)
dorean-local = "~/Documents/logoxide-dorean-repos"
```

Paths may start with `~/`; a relative one is beside the config file.
*/

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    error::{LogoxideError, Result},
    library::{Library, env_dir},
    manifest::Manifest,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// The library's folder
    #[serde(default)]
    pub library: Option<PathBuf>,
    /// The cache's folder
    #[serde(default)]
    pub cache: Option<PathBuf>,
    /// Catalogs every project searches, after its own (URLs or paths)
    #[serde(default)]
    pub catalogs: Vec<String>,
    /// Registries every project can name, besides its own
    #[serde(default)]
    pub registries: BTreeMap<String, String>,
    /// Seconds a git fetch may take before it's stopped ([`crate::git::FETCH_TIMEOUT`])
    #[serde(default)]
    pub git_timeout: Option<u64>,
    /// Seconds listing a repository's tags may take ([`crate::git::LIST_TIMEOUT`])
    #[serde(default)]
    pub git_list_timeout: Option<u64>,
    /// Seconds a catalog downloaded over HTTP is used from the cache before it's downloaded
    /// again ([`CATALOG_MAX_AGE`] when not given; 0 keeps none). Written as seconds or as a
    /// duration (`"12h"`, [`parse_duration`])
    #[serde(default, deserialize_with = "max_age")]
    pub catalog_max_age: Option<u64>,
}

/// How long a downloaded catalog is used before it's downloaded again, unless the config says
pub const CATALOG_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/**
Seconds from a duration's text: a whole number of seconds, else one followed by `s`, `m`, `h`,
`d`, or `w` (`"90s"`, `"30m"`, `"12h"`, `"7d"`, `"2w"`)
*/
pub fn parse_duration(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: u64 = number.parse().ok()?;
    let unit = match unit.trim() {
        "" | "s" => 1,
        "m" => 60,
        "h" => 60 * 60,
        "d" => 24 * 60 * 60,
        "w" => 7 * 24 * 60 * 60,
        _ => return None,
    };
    number.checked_mul(unit)
}

/// `catalog-max-age`: seconds, or a duration's text
fn max_age<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<u64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        Seconds(u64),
        Text(String),
    }
    match Written::deserialize(deserializer) {
        Ok(Written::Seconds(seconds)) => Ok(Some(seconds)),
        Ok(Written::Text(text)) => parse_duration(&text).map(Some).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "`{text}` isn't a duration (seconds, or like \"90s\", \"30m\", \"12h\", \"7d\", \"2w\")"
            ))
        }),
        Err(_) => Err(serde::de::Error::custom(
            "catalog-max-age is seconds, or a duration like \"7d\"",
        )),
    }
}

/// `~/` as the home folder; a relative path beside `base`; a URL as it is
fn resolve(text: &str, base: &Path) -> String {
    if text.contains("://") {
        return text.to_string();
    }
    let path = crate::topos_config::expand_home(text);
    let whole = match path.is_absolute() {
        true => path,
        false => base.join(path),
    };
    // `cfg/../repos` is `repos`, without touching the disk
    let mut clean = PathBuf::new();
    for part in whole.components() {
        match part {
            std::path::Component::ParentDir => {
                clean.pop();
            }
            std::path::Component::CurDir => {}
            other => clean.push(other),
        }
    }
    clean.display().to_string()
}

impl Config {
    /// `LOGOXIDE_CONFIG`, else `$XDG_CONFIG_HOME/logoxide/config.toml`, else
    /// `~/.config/logoxide/config.toml`
    pub fn default_path() -> Option<PathBuf> {
        env_dir("LOGOXIDE_CONFIG").or_else(|| {
            let base =
                env_dir("XDG_CONFIG_HOME").or_else(|| Some(env_dir("HOME")?.join(".config")))?;
            Some(base.join("logoxide").join("config.toml"))
        })
    }

    /// The config at its default path; the defaults when there's no file
    pub fn load() -> Result<Config> {
        match Config::default_path() {
            Some(path) if path.exists() => Config::read(&path),
            Some(path) if std::env::var_os("LOGOXIDE_CONFIG").is_some() => Err(
                LogoxideError::NotFound(format!("{} (LOGOXIDE_CONFIG)", path.display())),
            ),
            _ => Ok(Config::default()),
        }
    }

    /// A config file, with its paths made whole
    pub fn read(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| LogoxideError::io(&path.display().to_string(), e))?;
        let base = path.parent().unwrap_or(Path::new("."));
        Config::parse(&text, base).map_err(|e| match e {
            LogoxideError::Manifest(message) => {
                LogoxideError::io_message(&path.display().to_string(), message)
            }
            other => other,
        })
    }

    /// A config's text, its relative paths beside `base`
    pub fn parse(text: &str, base: &Path) -> Result<Config> {
        let mut config: Config =
            toml::from_str(text).map_err(|e| LogoxideError::Manifest(e.to_string()))?;
        let whole = |p: &PathBuf| PathBuf::from(resolve(&p.to_string_lossy(), base));
        config.library = config.library.as_ref().map(whole);
        config.cache = config.cache.as_ref().map(whole);
        for catalog in &mut config.catalogs {
            *catalog = resolve(catalog, base);
        }
        for location in config.registries.values_mut() {
            *location = resolve(location, base);
        }
        for name in config.registries.keys() {
            crate::catalog::check_registry_name(name)?;
        }
        Ok(config)
    }

    /**
    The library: `root` and `cache` when given (`--library`, `--cache-dir`), else
    `LOGOXIDE_HOME` and `LOGOXIDE_CACHE`, else this config's, else the XDG folders
    */
    pub fn library(&self, root: Option<PathBuf>, cache: Option<PathBuf>) -> Library {
        let root = (root.or_else(|| env_dir("LOGOXIDE_HOME")))
            .or_else(|| self.library.clone())
            .unwrap_or_else(Library::default_root);
        let cache = (cache.or_else(|| env_dir("LOGOXIDE_CACHE")))
            .or_else(|| self.cache.clone())
            .unwrap_or_else(Library::default_cache);
        Library::new(root, cache)
    }

    /// The `git` command with this config's time limits, cloning in `scratch`
    pub fn git(&self, scratch: Option<PathBuf>) -> crate::git::GitCli {
        let seconds = std::time::Duration::from_secs;
        crate::git::GitCli {
            scratch,
            fetch_timeout: self.git_timeout.map_or(crate::git::FETCH_TIMEOUT, seconds),
            list_timeout: (self.git_list_timeout).map_or(crate::git::LIST_TIMEOUT, seconds),
        }
    }

    /// How long a downloaded catalog is used before it's downloaded again; zero keeps none
    pub fn catalog_max_age(&self) -> std::time::Duration {
        (self.catalog_max_age).map_or(CATALOG_MAX_AGE, std::time::Duration::from_secs)
    }

    /// A project's registries and this config's (the project's own name wins)
    pub fn registries(&self, manifest: &Manifest) -> BTreeMap<String, String> {
        let mut all = self.registries.clone();
        all.extend(manifest.registries());
        all
    }

    /// The catalogs a project searches: its own, this config's, then the default ones (unless it
    /// turns them off)
    pub fn catalogs(&self, manifest: &Manifest) -> Vec<String> {
        manifest.catalogs_with(&self.catalogs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_config() {
        let config = Config::parse(
            r#"library = "~/Bibles"
cache = "cache"
catalogs = ["https://example.org/catalog.toml", "mine.toml"]
[registries]
dorean-local = "../repos"
"#,
            Path::new("/etc/logoxide"),
        )
        .unwrap();
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            config.library,
            Some(PathBuf::from(format!("{home}/Bibles")))
        );
        assert_eq!(config.cache, Some(PathBuf::from("/etc/logoxide/cache")));
        assert_eq!(
            config.catalogs,
            [
                "https://example.org/catalog.toml",
                "/etc/logoxide/mine.toml"
            ]
        );
        assert_eq!(config.registries["dorean-local"], "/etc/repos");
        assert_eq!(config.git(None).fetch_timeout, crate::git::FETCH_TIMEOUT);
        let timed = Config::parse("git-timeout = 5\ngit-list-timeout = 2", Path::new("/")).unwrap();
        let git = timed.git(None);
        assert_eq!(git.fetch_timeout, std::time::Duration::from_secs(5));
        assert_eq!(git.list_timeout, std::time::Duration::from_secs(2));
        // Flags win over the config
        let library = config.library(Some(PathBuf::from("/flag")), None);
        assert_eq!(library.root(), Path::new("/flag"));
        assert!(Config::parse("libary = \"x\"", Path::new("/")).is_err());
        assert!(Config::parse("[registries]\ngithub = \"x\"", Path::new("/")).is_err());
    }

    #[test]
    fn reads_a_catalogs_max_age() {
        let age = |text: &str| Config::parse(text, Path::new("/")).map(|c| c.catalog_max_age());
        let secs = std::time::Duration::from_secs;
        assert_eq!(age("").unwrap(), CATALOG_MAX_AGE);
        assert_eq!(CATALOG_MAX_AGE, secs(7 * 86400));
        assert_eq!(age("catalog-max-age = 90").unwrap(), secs(90));
        assert_eq!(age("catalog-max-age = 0").unwrap(), secs(0));
        assert_eq!(age("catalog-max-age = \"0\"").unwrap(), secs(0));
        assert_eq!(age("catalog-max-age = \"90s\"").unwrap(), secs(90));
        assert_eq!(age("catalog-max-age = \"30m\"").unwrap(), secs(1800));
        assert_eq!(age("catalog-max-age = \"12h\"").unwrap(), secs(43200));
        assert_eq!(age("catalog-max-age = \"7d\"").unwrap(), secs(604800));
        assert_eq!(age("catalog-max-age = \"2w\"").unwrap(), secs(1209600));
        for bad in [
            "\"7 days\"",
            "\"d\"",
            "\"-1d\"",
            "-5",
            "1.5",
            "true",
            "\"\"",
        ] {
            let error = age(&format!("catalog-max-age = {bad}")).unwrap_err();
            assert!(
                error.to_string().contains("catalog-max-age")
                    || error.to_string().contains("duration"),
                "{bad}: {error}"
            );
        }
        assert_eq!(parse_duration("99999999999999999999d"), None);
    }

    #[test]
    fn adds_to_a_projects_catalogs_and_registries() {
        let config = Config::parse(
            "catalogs = [\"https://example.org/mine.toml\"]\n[registries]\nshared = \"/r\"\nlocal = \"/machine\"",
            Path::new("/"),
        )
        .unwrap();
        let manifest = Manifest::parse(
            "[settings]\ncatalogs = [\"project.toml\"]\n[settings.registries]\nlocal = \"/project\"",
        )
        .unwrap();
        let catalogs = config.catalogs(&manifest);
        assert_eq!(
            &catalogs[..2],
            ["project.toml", "https://example.org/mine.toml"]
        );
        assert_eq!(catalogs.len(), 2 + crate::catalog::DEFAULT_CATALOGS.len());
        let registries = config.registries(&manifest);
        assert_eq!(registries["local"], "/project");
        assert_eq!(registries["shared"], "/r");
    }
}
