/*!
Catalogs: lists of packages to search, so a Bible can be found by its name or language instead of
its URL. A catalog holds what each package is and where it comes from, never its text, so anyone
can host one (a file in a git repository is enough). Two kinds are read:

- **biblix catalogs** (`catalog.toml`): packages published as git repositories, each as its
  `biblix.toml` describes it (`bx catalog entry <url>` writes an entry)
- **eBible.org's list** (`translations.csv`, [`EBIBLE_CATALOG`]): about 1,500 translations it
  publishes as USFM, imported into packages when installed ([`crate::import`])

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
form); without any, eBible.org's list is the one searched.
*/

use serde::{Deserialize, Serialize};

use crate::{
    error::{BiblixError, Result},
    manifest::{Direction, PackageKind},
};

/// eBible.org's list of the translations it publishes
pub const EBIBLE_CATALOG: &str = "https://ebible.org/Scriptures/translations.csv";

/// Where a listed package comes from
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogSource {
    /// A biblix package in a git repository
    Git { url: String },
    /// A translation eBible.org publishes as USFM: `id`'s zip, imported into a package
    Ebible { id: String },
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
    /// A catalog from a file's text: eBible.org's CSV when `name` ends in `.csv`, else a
    /// `catalog.toml`
    pub fn parse(name: &str, text: &str) -> Result<Catalog> {
        let path = name.split(['?', '#']).next().unwrap_or(name);
        if path.to_ascii_lowercase().ends_with(".csv") {
            Catalog::parse_ebible(text)
        } else {
            Catalog::parse_toml(text)
        }
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
                source: CatalogSource::Git { url: e.git },
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
