/*!
The book data topos's own config gives (`~/.config/topos/config.toml`: `data`, `merge-data`, and
`remove-data`, as the `topos` CLI and logoxide-lsp read them), so custom genres and books mean the
same everywhere; and filters over books like topos's (testaments, genres, books), for choosing
packages by the books they're about.

```toml
# ~/.config/topos/config.toml
merge-data = "~/.config/topos/genres.json"   # or a list
```
*/

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use topos_bible::{
    data::{
        bible_data::{BibleData, BibleDataInput},
        patch::DataPatch,
    },
    filter::{
        bible_filter::BibleFilter,
        filters::{book::BookFilter, genre::GenreFilter, testament::TestamentFilter},
    },
};

use crate::error::{LogoxideError, Result};

/// `$XDG_CONFIG_HOME/topos/config.toml`, else `~/.config/topos/config.toml`
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("topos").join("config.toml"))
}

/// `~/` as the home folder
pub fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| LogoxideError::io(&path.display().to_string(), e))
}

/// The config file's table: `path`'s, else the default one's; empty when there is none
pub fn read(path: Option<&Path>) -> Result<toml::Table> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => match default_path().filter(|p| p.exists()) {
            Some(path) => path,
            None => return Ok(toml::Table::new()),
        },
    };
    (read_text(&path)?.parse())
        .map_err(|e: toml::de::Error| LogoxideError::io_message(&path.display().to_string(), e))
}

/// A key's paths: one (`"a.json"`) or a list; `merge-data` and `merge_data` are the same key
fn paths(config: &toml::Table, key: &str) -> Vec<PathBuf> {
    let value = config
        .get(key)
        .or_else(|| config.get(&key.replace('-', "_")));
    let list: Vec<&str> = match value {
        Some(toml::Value::String(one)) => vec![one],
        Some(toml::Value::Array(many)) => many.iter().filter_map(toml::Value::as_str).collect(),
        _ => vec![],
    };
    list.into_iter().map(expand_home).collect()
}

/// The book data a config gives: `data` (else the defaults), then each `merge-data`, then each
/// `remove-data`, as topos reads them
pub fn bible_data(config: &toml::Table) -> Result<BibleData> {
    fn json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
        serde_json::from_str(&read_text(path)?)
            .map_err(|e| LogoxideError::io_message(&path.display().to_string(), e))
    }
    let bad = |path: &Path, e: &dyn std::fmt::Display| {
        LogoxideError::io_message(&path.display().to_string(), e)
    };
    let mut input = match paths(config, "data").first() {
        Some(path) => json::<BibleDataInput>(path)?,
        None => BibleDataInput::defaults(),
    };
    for path in paths(config, "merge-data") {
        (input.merge(json::<DataPatch>(&path)?)).map_err(|e| bad(&path, &e))?;
    }
    for path in paths(config, "remove-data") {
        (input.remove(json::<DataPatch>(&path)?)).map_err(|e| bad(&path, &e))?;
    }
    BibleData::new(input).map_err(|e| LogoxideError::Catalog(format!("topos's book data: {e}")))
}

/**
Filters over books, as topos's: including a testament limits to it, included genres and books add
up, and exclusions always win (`--nt -g Gospels` is the four Gospels; `-g Pentateuch -b
Revelation` is six books)
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BookFilters {
    /// `old` or `new`
    pub testaments: Vec<String>,
    pub exclude_testaments: Vec<String>,
    pub genres: Vec<String>,
    pub exclude_genres: Vec<String>,
    /// Books by name (`John`, `1 Cor`) or USFM code (`JHN`)
    pub books: Vec<String>,
    pub exclude_books: Vec<String>,
}

impl BookFilters {
    pub fn is_empty(&self) -> bool {
        self == &BookFilters::default()
    }

    /**
    The USFM codes of the books the filters keep, in `data` (its genres and names), or none when
    there are no filters
    */
    pub fn codes(&self, data: &BibleData) -> Result<Option<BTreeSet<String>>> {
        if self.is_empty() {
            return Ok(None);
        }
        let bad = |e: &dyn std::fmt::Display| LogoxideError::Catalog(e.to_string());
        let testament = |name: &String| -> Result<TestamentFilter> {
            name.parse()
                .map_err(|e: topos_bible::error::ToposError| bad(&e))
        };
        // A USFM code is a book as much as its name is
        let book = |name: &String| {
            let books = data.books();
            let name = (books.search_usfm(&name.to_ascii_uppercase()))
                .and_then(|id| books.get_name(id).cloned())
                .unwrap_or_else(|| name.clone());
            BookFilter::new(name)
        };
        let mut filter = BibleFilter::new(data.clone());
        for name in &self.testaments {
            filter.include(testament(name)?).map_err(|e| bad(&e))?;
        }
        for name in &self.genres {
            filter
                .include(GenreFilter::new(name))
                .map_err(|e| bad(&e))?;
        }
        for name in &self.books {
            filter.include(book(name)).map_err(|e| bad(&e))?;
        }
        for name in &self.exclude_testaments {
            filter.exclude(testament(name)?).map_err(|e| bad(&e))?;
        }
        for name in &self.exclude_genres {
            filter
                .exclude(GenreFilter::new(name))
                .map_err(|e| bad(&e))?;
        }
        for name in &self.exclude_books {
            filter.exclude(book(name)).map_err(|e| bad(&e))?;
        }
        Ok(Some(
            (filter.ids().iter())
                .filter_map(|id| data.books().get_usfm(*id).cloned())
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(filters: BookFilters) -> Vec<String> {
        let data = bible_data(&toml::Table::new()).unwrap();
        filters.codes(&data).unwrap().unwrap().into_iter().collect()
    }

    #[test]
    fn filters_books_as_topos_does() {
        let data = bible_data(&toml::Table::new()).unwrap();
        assert_eq!(BookFilters::default().codes(&data).unwrap(), None);
        let gospels = codes(BookFilters {
            testaments: vec!["new".into()],
            genres: vec!["Gospels".into()],
            ..Default::default()
        });
        assert_eq!(gospels, ["JHN", "LUK", "MAT", "MRK"]);
        // Genres and books add up; a code is a book; exclusions win
        let some = codes(BookFilters {
            genres: vec!["Pentateuch".into()],
            books: vec!["REV".into(), "Romans".into()],
            exclude_books: vec!["Genesis".into()],
            ..Default::default()
        });
        assert_eq!(some, ["DEU", "EXO", "LEV", "NUM", "REV", "ROM"]);
        assert_eq!(
            codes(BookFilters {
                exclude_testaments: vec!["old".into()],
                ..Default::default()
            })
            .len(),
            27
        );
        let wrong = BookFilters {
            testaments: vec!["middle".into()],
            ..Default::default()
        };
        assert!(wrong.codes(&data).is_err());
    }

    #[test]
    fn reads_the_book_data_a_config_names() {
        let dir =
            std::env::temp_dir().join(format!("logoxide-topos-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let patch = dir.join("genres.json");
        std::fs::write(&patch, r#"{ "genres": [{ "title": "Prison Epistles", "books": ["Ephesians", "Philippians", "Colossians", "Philemon"] }] }"#)
            .unwrap();
        let config: toml::Table = format!("merge-data = [{:?}]", patch.display().to_string())
            .parse()
            .unwrap();
        let data = bible_data(&config).unwrap();
        let prison = BookFilters {
            genres: vec!["Prison Epistles".into()],
            ..Default::default()
        };
        assert_eq!(prison.codes(&data).unwrap().unwrap().len(), 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
