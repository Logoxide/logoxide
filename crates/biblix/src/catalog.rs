/*!
Catalogs: lists of packages to search, so a Bible can be found by its name or language instead of
its URL. A catalog holds what each package is and where it comes from, never its text, so anyone
can host one (a file in a git repository is enough). Two kinds are read:

- **biblix catalogs** (`catalog.toml`): packages published as git repositories, each as its
  `biblix.toml` describes it (`bx catalog entry <url>` writes an entry)
- **eBible.org's list** (`translations.csv`, [`EBIBLE_CATALOG`]): about 1,500 translations it
  publishes as USFM, imported into packages when installed ([`crate::import`])
- **fetch.bible's manifest** (`manifest.json`, [`FETCH_BIBLE_CATALOG`]): about 1,400 translations
  it publishes as USX, a file per book, imported the same way

```toml
# catalog.toml
name = "Bibles in English"

[[package]]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
language = "en"
license = "LicenseRef-PublicDomain"
git = "https://github.com/someone/bsb"
```

A vault names its catalogs in `[settings] catalogs` (URLs or paths; a `.csv` is eBible.org's
form, a `.json` fetch.bible's); without any, eBible.org's and fetch.bible's lists are searched.
*/

use serde::{Deserialize, Serialize};

use crate::{
    error::{BiblixError, Result},
    manifest::{Direction, PackageKind},
};

/// eBible.org's list of the translations it publishes
pub const EBIBLE_CATALOG: &str = "https://ebible.org/Scriptures/translations.csv";

/// fetch.bible's list of the translations it publishes
pub const FETCH_BIBLE_CATALOG: &str = "https://v1.fetch.bible/manifest.json";

/// The catalogs searched when a vault names none
pub const DEFAULT_CATALOGS: [&str; 2] = [EBIBLE_CATALOG, FETCH_BIBLE_CATALOG];

/// Where a listed package comes from
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogSource {
    /// A biblix package in a git repository
    Git { url: String },
    /// A translation eBible.org publishes as USFM: `id`'s zip, imported into a package
    Ebible { id: String },
    /// A translation fetch.bible publishes as USX: each of `books` (lowercase USFM codes),
    /// imported into a package
    FetchBible { id: String, books: Vec<String> },
}

impl CatalogSource {
    /// Every kind's import prefix: what `ebible:<id>` and `fetch:<id>` start with
    pub const PREFIXES: [&'static str; 2] = ["ebible", "fetch"];

    /// The prefix that names this kind's imports (`ebible`, `fetch`); a git package has none,
    /// since it's added by its URL
    pub fn prefix(&self) -> Option<&'static str> {
        match self {
            CatalogSource::Git { .. } => None,
            CatalogSource::Ebible { .. } => Some("ebible"),
            CatalogSource::FetchBible { .. } => Some("fetch"),
        }
    }

    /// The publisher's own id, for the kinds that are imported
    pub fn import_id(&self) -> Option<&str> {
        match self {
            CatalogSource::Git { .. } => None,
            CatalogSource::Ebible { id } | CatalogSource::FetchBible { id, .. } => Some(id),
        }
    }

    /// What `bx import` takes for it: `ebible:eng-asv`, `fetch:eng_asv`
    pub fn import_name(&self) -> Option<String> {
        Some(format!("{}:{}", self.prefix()?, self.import_id()?))
    }
}

/// One package a catalog lists
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The package's id (`bsb`, `eng-asv`)
    pub id: String,
    pub name: String,
    /// What references name it by (`BSB`)
    pub abbreviation: String,
    pub aliases: Vec<String>,
    /// BCP 47 (`en`), or ISO 639-3 (`eng`) as eBible.org gives it
    pub language: String,
    /// The language's name, when the catalog gives it (`English`)
    pub language_name: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    /// A copyright statement, when the catalog gives one
    pub copyright: Option<String>,
    /// The newest version, when the catalog says (eBible.org's: its update date, `2026.10.8`)
    pub version: Option<String>,
    pub kind: PackageKind,
    pub direction: Direction,
    pub source: CatalogSource,
}

/// The `[[package]]` tables of a `catalog.toml`
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct TomlEntry {
    id: String,
    name: String,
    abbreviation: String,
    #[serde(default)]
    aliases: Vec<String>,
    language: String,
    #[serde(default)]
    language_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    kind: PackageKind,
    #[serde(default)]
    direction: Direction,
    #[serde(default)]
    version: Option<String>,
    git: String,
}

#[derive(Clone, Debug, Deserialize)]
struct TomlCatalog {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "package")]
    packages: Vec<TomlEntry>,
}

/// A catalog's packages
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    pub name: String,
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// A catalog from a file's text: eBible.org's CSV when `name` ends in `.csv`, fetch.bible's
    /// manifest when it ends in `.json`, else a `catalog.toml`
    pub fn parse(name: &str, text: &str) -> Result<Catalog> {
        let path = name
            .split(['?', '#'])
            .next()
            .unwrap_or(name)
            .to_ascii_lowercase();
        if path.ends_with(".csv") {
            Catalog::parse_ebible(text)
        } else if path.ends_with(".json") {
            Catalog::parse_fetch_bible(text)
        } else {
            Catalog::parse_toml(text)
        }
    }

    /// fetch.bible's `manifest.json`: its translations, with their books and licenses
    pub fn parse_fetch_bible(text: &str) -> Result<Catalog> {
        use serde_json::Value;
        let bad = |message: &str| BiblixError::Catalog(format!("manifest.json: {message}"));
        let json: Value = serde_json::from_str(text).map_err(|e| bad(&e.to_string()))?;
        let bibles = json
            .get("bibles")
            .and_then(Value::as_object)
            .ok_or_else(|| bad("no bibles"))?;
        let ordered: Vec<String> = (json.get("books_ordered").and_then(Value::as_array))
            .map(|books| {
                books
                    .iter()
                    .filter_map(|b| Some(b.as_str()?.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        // The first 39 are the Old Testament's
        let (ot, nt) = ordered.split_at(ordered.len().min(39));
        let languages = json.get("languages");
        let text_of = |v: Option<&Value>| {
            v.and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
        };
        let books_of = |v: Option<&Value>, all: &[String]| match v {
            Some(Value::Bool(true)) => all.to_vec(),
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(|b| Some(b.as_str()?.to_ascii_lowercase()))
                .collect(),
            _ => vec![],
        };
        let mut entries = vec![];
        for (id, bible) in bibles {
            let mut books = books_of(bible.get("books_ot"), ot);
            books.extend(books_of(bible.get("books_nt"), nt));
            if books.is_empty() {
                continue;
            }
            let names = bible.get("name");
            let name_of = |key: &str| text_of(names.and_then(|n| n.get(key)));
            let code = id.split('_').next().unwrap_or(id).to_string();
            let language = languages.and_then(|l| l.get(&code));
            let copyright = bible.get("copyright");
            let license = (copyright.and_then(|c| c.get("licenses")))
                .and_then(Value::as_array)
                .and_then(|l| l.first())
                .map(|l| fetch_bible_license(l.get("license"), text_of(l.get("url"))));
            let abbreviation = name_of("english_abbrev")
                .or_else(|| name_of("local_abbrev"))
                .unwrap_or_else(|| id.to_uppercase());
            let english = name_of("english");
            let local = name_of("local");
            entries.push(CatalogEntry {
                id: package_id(id),
                name: english
                    .clone()
                    .or_else(|| local.clone())
                    .unwrap_or_else(|| id.clone()),
                aliases: local
                    .filter(|l| Some(l) != english.as_ref())
                    .into_iter()
                    .collect(),
                abbreviation,
                language_name: text_of(language.and_then(|l| l.get("english"))),
                language: code,
                description: None,
                license,
                copyright: text_of(copyright.and_then(|c| c.get("attribution"))),
                version: (bible.get("year").and_then(Value::as_u64)).map(|y| format!("{y}.0.0")),
                kind: PackageKind::Bible,
                direction: match bible.get("direction").and_then(Value::as_str) {
                    Some("rtl") => Direction::Rtl,
                    _ => Direction::Ltr,
                },
                source: CatalogSource::FetchBible {
                    id: id.clone(),
                    books,
                },
            });
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Catalog {
            name: String::from("fetch.bible"),
            entries,
        })
    }

    pub fn parse_toml(text: &str) -> Result<Catalog> {
        let catalog: TomlCatalog =
            toml::from_str(text).map_err(|e| BiblixError::Catalog(e.to_string()))?;
        let entries = (catalog.packages.into_iter())
            .map(|e| CatalogEntry {
                id: e.id,
                name: e.name,
                abbreviation: e.abbreviation,
                aliases: e.aliases,
                language: e.language,
                language_name: e.language_name,
                description: e.description,
                license: e.license,
                copyright: None,
                version: e.version,
                kind: e.kind,
                direction: e.direction,
                // `github:owner/repo` is listed by its URL
                source: CatalogSource::Git {
                    url: crate::git::expand_url(&e.git),
                },
            })
            .collect();
        Ok(Catalog {
            name: catalog.name.unwrap_or_default(),
            entries,
        })
    }

    /// eBible.org's `translations.csv`: the translations it lets anyone download
    pub fn parse_ebible(text: &str) -> Result<Catalog> {
        let rows = csv(text.trim_start_matches('\u{feff}'));
        let mut rows = rows.into_iter();
        let header = rows
            .next()
            .ok_or_else(|| BiblixError::Catalog(String::from("an empty translations.csv")))?;
        let column = |name: &str| {
            header.iter().position(|h| h == name).ok_or_else(|| {
                BiblixError::Catalog(format!("translations.csv has no {name} column"))
            })
        };
        let [
            language,
            id,
            language_name,
            title,
            description,
            copyright,
            short_title,
        ] = [
            "languageCode",
            "translationId",
            "languageNameInEnglish",
            "title",
            "description",
            "Copyright",
            "shortTitle",
        ]
        .map(column);
        let [
            language,
            id,
            language_name,
            title,
            description,
            copyright,
            short_title,
        ] = [
            language?,
            id?,
            language_name?,
            title?,
            description?,
            copyright?,
            short_title?,
        ];
        let [redistributable, downloadable, direction, updated] = [
            "Redistributable",
            "downloadable",
            "textDirection",
            "UpdateDate",
        ]
        .map(column);
        let [redistributable, downloadable, direction, updated] =
            [redistributable?, downloadable?, direction?, updated?];
        let get = |row: &[String], i: usize| row.get(i).map(|s| s.trim().to_string());
        let yes = |row: &[String], i: usize| get(row, i).is_some_and(|v| v == "True");
        let mut entries = vec![];
        for row in rows {
            if !yes(&row, downloadable) || !yes(&row, redistributable) {
                continue;
            }
            let (Some(code), Some(ebible_id)) = (get(&row, language), get(&row, id)) else {
                continue;
            };
            let copyright = get(&row, copyright).filter(|c| !c.is_empty());
            let public = copyright
                .as_deref()
                .is_some_and(|c| c.to_ascii_lowercase().contains("public domain"));
            let name = get(&row, title)
                .filter(|t| !t.is_empty())
                .or_else(|| get(&row, short_title))
                .unwrap_or_else(|| ebible_id.clone());
            entries.push(CatalogEntry {
                id: package_id(&ebible_id),
                abbreviation: abbreviation(&ebible_id, &code),
                name,
                aliases: vec![],
                language_name: get(&row, language_name).filter(|n| !n.is_empty()),
                language: code,
                description: get(&row, description).filter(|d| !d.is_empty()),
                license: Some(String::from(if public {
                    "LicenseRef-PublicDomain"
                } else {
                    "LicenseRef-eBible"
                })),
                copyright,
                version: get(&row, updated).and_then(|d| date_version(&d)),
                kind: PackageKind::Bible,
                direction: match get(&row, direction).as_deref() {
                    Some("rtl") => Direction::Rtl,
                    _ => Direction::Ltr,
                },
                source: CatalogSource::Ebible { id: ebible_id },
            });
        }
        Ok(Catalog {
            name: String::from("eBible.org"),
            entries,
        })
    }

    /// The entries `query` matches, best first: every word of it must be in the entry's id,
    /// name, abbreviation, aliases, language, or description (ignoring case); an exact
    /// abbreviation or id comes first, then names and languages, then descriptions
    pub fn search(&self, query: &str) -> Vec<&CatalogEntry> {
        let words: Vec<String> = query.split_whitespace().map(fold).collect();
        let mut found: Vec<(u32, &CatalogEntry)> = (self.entries.iter())
            .filter_map(|entry| Some((score(entry, &words)?, entry)))
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        found.into_iter().map(|(_, e)| e).collect()
    }

    /// The entry an import name (`ebible:eng-asv`, as [`CatalogSource::import_name`] writes it)
    /// names, if this catalog lists it
    pub fn find_import(&self, name: &str) -> Option<&CatalogEntry> {
        (self.entries.iter()).find(|e| e.source.import_name().as_deref() == Some(name))
    }

    /// This catalog with another's entries after its own (an id listed twice keeps the first)
    pub fn merge(mut self, other: Catalog) -> Catalog {
        for entry in other.entries {
            if !self.entries.iter().any(|e| e.id == entry.id) {
                self.entries.push(entry);
            }
        }
        self
    }

    /// A `[[package]]` table for a `catalog.toml`, from a package's manifest and its repository
    pub fn toml_entry(info: &crate::manifest::Package, git: &str) -> String {
        let mut table = toml_edit::Table::new();
        table.insert("id", toml_edit::value(&info.id));
        table.insert("name", toml_edit::value(&info.name));
        table.insert("abbreviation", toml_edit::value(&info.abbreviation));
        if !info.aliases.is_empty() {
            let aliases: toml_edit::Array = info.aliases.iter().map(String::as_str).collect();
            table.insert("aliases", toml_edit::value(aliases));
        }
        table.insert("language", toml_edit::value(&info.language));
        if let Some(license) = &info.license {
            table.insert("license", toml_edit::value(license));
        }
        if info.kind != PackageKind::Bible {
            let kind = toml::Value::try_from(info.kind)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            table.insert("kind", toml_edit::value(kind));
        }
        if info.direction == Direction::Rtl {
            table.insert("direction", toml_edit::value("rtl"));
        }
        table.insert("git", toml_edit::value(git));
        let mut doc = toml_edit::DocumentMut::new();
        let mut packages = toml_edit::ArrayOfTables::new();
        packages.push(table);
        doc.insert("package", toml_edit::Item::ArrayOfTables(packages));
        doc.to_string()
    }
}

/// A fetch.bible license as SPDX: `cc-by-sa` with its URL's version is `CC-BY-SA-4.0`, `public`
/// is public domain, and its own terms (an object of restrictions) are `LicenseRef-fetch.bible`
fn fetch_bible_license(license: Option<&serde_json::Value>, url: Option<String>) -> String {
    match license.and_then(serde_json::Value::as_str) {
        Some("public") => String::from("LicenseRef-PublicDomain"),
        Some(cc) if cc.starts_with("cc-") => {
            let version = (url.as_deref())
                .and_then(|u| u.trim_end_matches('/').rsplit('/').next())
                .filter(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.'))
                .unwrap_or("4.0")
                .to_string();
            format!("{}-{version}", cc.to_uppercase())
        }
        _ => String::from("LicenseRef-fetch.bible"),
    }
}

/// A version from a date (`2026-10-08` is `2026.10.8`), so newer downloads have newer versions
fn date_version(date: &str) -> Option<String> {
    let mut parts = date.split('-').map(|p| p.parse::<u32>().ok());
    let (Some(Some(y)), Some(Some(m)), Some(Some(d))) = (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    Some(format!("{y}.{m}.{d}"))
}

/// Lowercase, without accents' combining marks, for matching
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

fn score(entry: &CatalogEntry, words: &[String]) -> Option<u32> {
    if words.is_empty() {
        return Some(0);
    }
    let exact = [fold(&entry.abbreviation), fold(&entry.id)];
    let names: Vec<String> = [&entry.name]
        .into_iter()
        .chain(&entry.aliases)
        .chain(&entry.language_name)
        .chain([&entry.language])
        .map(|s| fold(s))
        .collect();
    let description = entry.description.as_deref().map(fold).unwrap_or_default();
    let mut total = 0;
    for word in words {
        total += if exact.iter().any(|e| e == word) {
            100
        } else if exact.iter().any(|e| e.contains(word.as_str())) {
            40
        } else if names
            .iter()
            .any(|n| n.split_whitespace().any(|w| w == word))
        {
            30
        } else if names.iter().any(|n| n.contains(word.as_str())) {
            20
        } else if description.contains(word.as_str()) {
            5
        } else {
            return None;
        };
    }
    Some(total)
}

/// A package id from eBible.org's (`eng-asv`, `engkjv2006`): lowercase letters, digits, `-`, `_`
fn package_id(id: &str) -> String {
    id.to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            _ => '-',
        })
        .collect()
}

/// What references could name an eBible.org translation by: its id without the language code
/// (`eng-asv` is `ASV`, `engwebp` is `WEBP`)
fn abbreviation(id: &str, language: &str) -> String {
    let rest = id
        .strip_prefix(language)
        .map(|r| r.trim_start_matches(['-', '_']))
        .filter(|r| !r.is_empty())
        .unwrap_or(id);
    rest.to_uppercase()
}

/// Rows of a CSV file: fields split by commas, quoted fields with `""` for a quote (and
/// commas or line breaks inside)
fn csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const EBIBLE: &str = "\u{feff}\"languageCode\",\"translationId\",\"languageName\",\"languageNameInEnglish\",\"dialect\",\"homeDomain\",\"title\",\"description\",\"Redistributable\",\"Copyright\",\"UpdateDate\",\"publicationURL\",\"OTbooks\",\"OTchapters\",\"OTverses\",\"NTbooks\",\"NTchapters\",\"NTverses\",\"DCbooks\",\"DCchapters\",\"DCverses\",\"FCBHID\",\"Certified\",\"inScript\",\"swordName\",\"rodCode\",\"textDirection\",\"downloadable\",\"font\",\"shortTitle\",\"PODISBN\",\"script\",\"sourceDate\"
\"eng\",\"eng-asv\",\"English\",\"English\",\"Archaic American\",\"ebible.org\",\"American Standard Version (1901)\",\"The American Standard Version of the Holy Bible, first published in 1901.\",\"True\",\"public domain\",\"2026-10-08\",\"http://ebible.org/eng-asv/\",\"39\",\"929\",\"23145\",\"27\",\"260\",\"7957\",\"0\",\"0\",\"0\",\"ENGASV\",\"True\",\"\",\"engASV1901eb\",\"00025\",\"ltr\",\"True\",\"Gentium\",\"American Standard Version (1901)\",\"\",\"Latin\",\"2026-10-08\"
\"eng\",\"engamp\",\"English\",\"English\",\"American\",\"eBible.org\",\"The Amplified Bible (2015)\",\"The Amplified Bible, 2015\",\"False\",\"Copyright © 2015 The Lockman Foundation\",\"2024-07-27\",\"\",\"39\",\"929\",\"23145\",\"27\",\"260\",\"7958\",\"0\",\"0\",\"0\",\"\",\"False\",\"\",\"\",\"\",\"ltr\",\"False\",\"\",\"Amplified\",\"\",\"Latin\",\"2024-07-27\"
\"heb\",\"hebwlc\",\"Hebrew\",\"Hebrew, Ancient\",\"\",\"ebible.org\",\"Westminster Leningrad Codex\",\"The \"\"WLC\"\", with vowels, and a comma, here\",\"True\",\"Copyright © 2020 J. Alan Groves Center\",\"2020-01-01\",\"\",\"39\",\"929\",\"23213\",\"0\",\"0\",\"0\",\"0\",\"0\",\"0\",\"\",\"True\",\"\",\"\",\"\",\"rtl\",\"True\",\"\",\"WLC\",\"\",\"Hebrew\",\"2020-01-01\"
";

    #[test]
    fn reads_ebibles_list() {
        let catalog = Catalog::parse(EBIBLE_CATALOG, EBIBLE).unwrap();
        // The Amplified can't be downloaded
        assert_eq!(catalog.entries.len(), 2);
        let asv = &catalog.entries[0];
        assert_eq!(
            (
                asv.id.as_str(),
                asv.abbreviation.as_str(),
                asv.language.as_str()
            ),
            ("eng-asv", "ASV", "eng")
        );
        assert_eq!(asv.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(
            asv.source,
            CatalogSource::Ebible {
                id: "eng-asv".into()
            }
        );
        assert_eq!(asv.version.as_deref(), Some("2026.10.8"));
        let wlc = &catalog.entries[1];
        assert_eq!(wlc.abbreviation, "WLC");
        assert_eq!(wlc.direction, Direction::Rtl);
        assert_eq!(wlc.license.as_deref(), Some("LicenseRef-eBible"));
        assert_eq!(
            wlc.description.as_deref(),
            Some("The \"WLC\", with vowels, and a comma, here")
        );
    }

    #[test]
    fn finds_imports_by_prefix() {
        let catalog = Catalog::parse(EBIBLE_CATALOG, EBIBLE).unwrap();
        let asv = catalog.find_import("ebible:eng-asv").unwrap();
        assert_eq!(asv.source.import_name().as_deref(), Some("ebible:eng-asv"));
        // Another kind's prefix, or none, finds nothing
        assert!(catalog.find_import("fetch:eng-asv").is_none());
        assert!(catalog.find_import("eng-asv").is_none());
        let git = CatalogSource::Git {
            url: "https://example.org/bsb".into(),
        };
        assert_eq!(git.import_name(), None);
    }

    #[test]
    fn lists_short_names_by_url() {
        let catalog = Catalog::parse_toml(
            "[[package]]\nid = \"kjv\"\nname = \"KJV\"\nabbreviation = \"KJV\"\nlanguage = \"en\"\ngit = \"github:someone/kjv\"\n",
        )
        .unwrap();
        assert_eq!(
            catalog.entries[0].source,
            CatalogSource::Git {
                url: "https://github.com/someone/kjv".into()
            }
        );
    }

    #[test]
    fn searches_and_merges() {
        let ebible = Catalog::parse("translations.csv", EBIBLE).unwrap();
        let mine = Catalog::parse_toml(
            r#"
name = "Mine"
[[package]]
id = "bsb"
name = "Berean Standard Bible"
abbreviation = "BSB"
language = "en"
language-name = "English"
git = "https://example.org/bsb"
"#,
        )
        .unwrap();
        let all = mine.merge(ebible);
        let ids = |q: &str| {
            all.search(q)
                .iter()
                .map(|e| e.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("asv"), ["eng-asv"]);
        assert_eq!(ids("english"), ["eng-asv", "bsb"]);
        assert_eq!(ids("hebrew codex"), ["hebwlc"]);
        assert_eq!(ids("berean"), ["bsb"]);
        assert!(ids("nothing like this").is_empty());
        assert_eq!(ids("").len(), 3);
        assert!(Catalog::parse_toml("[[package]]\nid = 1").is_err());
    }

    #[test]
    fn reads_fetch_bibles_manifest() {
        let json = r#"{"bibles": {
            "eng_bsb": {"name": {"local": "Berean Standard Bible", "local_abbrev": "BSB", "english": "Berean Standard Bible", "english_abbrev": "BSB"},
                "year": 2024, "direction": "ltr", "copyright": {"licenses": [{"license": "public", "url": ""}], "attribution": "BSB Publishing, LLC"},
                "books_ot": true, "books_nt": true},
            "spa_x": {"name": {"local": "Biblia", "local_abbrev": "", "english": "A Bible", "english_abbrev": "AB"},
                "year": 2020, "direction": "ltr", "copyright": {"licenses": [{"license": "cc-by-sa", "url": "https://creativecommons.org/licenses/by-sa/3.0/"}], "attribution": "Someone"},
                "books_ot": [], "books_nt": ["jhn", "rom"]},
            "heb_y": {"name": {"local": "", "local_abbrev": "", "english": "Restricted", "english_abbrev": "R"},
                "year": 2020, "direction": "rtl", "copyright": {"licenses": [{"license": {"forbid_commercial": true}, "url": ""}], "attribution": ""},
                "books_ot": ["gen"], "books_nt": false},
            "none_z": {"name": {"english": "Nothing"}, "books_ot": [], "books_nt": false}
        },
        "languages": {"eng": {"english": "English"}, "spa": {"english": "Spanish"}},
        "books_ordered": ["gen","exo","lev","num","deu","jos","jdg","rut","1sa","2sa","1ki","2ki","1ch","2ch","ezr","neh","est","job","psa","pro","ecc","sng","isa","jer","lam","ezk","dan","hos","jol","amo","oba","jon","mic","nam","hab","zep","hag","zec","mal","mat","mrk","luk","jhn","act","rom","1co","2co","gal","eph","php","col","1th","2th","1ti","2ti","tit","phm","heb","jas","1pe","2pe","1jn","2jn","3jn","jud","rev"]}"#;
        let catalog = Catalog::parse(FETCH_BIBLE_CATALOG, json).unwrap();
        assert_eq!(
            catalog.entries.len(),
            3,
            "a translation without books is left out"
        );
        let bsb = &catalog.entries[0];
        assert_eq!(
            (
                bsb.id.as_str(),
                bsb.abbreviation.as_str(),
                bsb.language.as_str()
            ),
            ("eng_bsb", "BSB", "eng")
        );
        assert_eq!(bsb.license.as_deref(), Some("LicenseRef-PublicDomain"));
        let CatalogSource::FetchBible { books, .. } = &bsb.source else {
            panic!()
        };
        assert_eq!((books.len(), books[42].as_str()), (66, "jhn"));
        let x = catalog.entries.iter().find(|e| e.id == "spa_x").unwrap();
        assert_eq!(x.license.as_deref(), Some("CC-BY-SA-3.0"));
        assert_eq!(x.aliases, ["Biblia"]);
        assert_eq!(x.language_name.as_deref(), Some("Spanish"));
        let y = catalog.entries.iter().find(|e| e.id == "heb_y").unwrap();
        assert_eq!(
            (y.license.as_deref(), y.direction),
            (Some("LicenseRef-fetch.bible"), Direction::Rtl)
        );
    }

    #[test]
    fn writes_an_entry() {
        let manifest = crate::Manifest::parse(
            "[package]\nid = \"bsb\"\nname = \"Berean Standard Bible\"\nabbreviation = \"BSB\"\nversion = \"1.0.0\"\nlanguage = \"en\"\nlicense = \"CC0-1.0\"\n[formats]\nusx = { path = \"usx/\" }\n",
        )
        .unwrap();
        let text = Catalog::toml_entry(
            manifest.package.as_ref().unwrap(),
            "https://example.org/bsb",
        );
        let catalog = Catalog::parse_toml(&text).unwrap();
        assert_eq!(catalog.entries[0].id, "bsb");
        assert_eq!(
            catalog.entries[0].source,
            CatalogSource::Git {
                url: "https://example.org/bsb".into()
            }
        );
    }
}
