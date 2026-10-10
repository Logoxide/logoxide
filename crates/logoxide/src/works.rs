/*!
Works and editions. A work is a text whatever its editions, formats, and publishers (the King
James Version); a package says which it is with `work = "kjv"`, and which edition with `edition`
(`1769-oxford`) and `edition-year`. logoxide's table of works (`data/works.toml`, the owner's D9)
gives each work's name, aliases, its Wikidata item when known, and the import names known to be
it, so an import is given its work by its name ([`of_import`]), never by guessing from a title.
*/

use std::{collections::BTreeMap, sync::OnceLock};

use serde::Deserialize;

/// One work logoxide knows
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Work {
    /// What a package's `work` names it by (`kjv`); the table's key
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Its Wikidata item (`Q…`), when known
    #[serde(default)]
    pub wikidata: Option<String>,
    /// The import names known to be it (`ebible:eng-kjv2006`, `sword:crosswire/KJV`)
    #[serde(default)]
    pub imports: Vec<String>,
}

/// The table, as logoxide is built with it
const WORKS: &str = include_str!("../data/works.toml");

/// Every work logoxide knows, in the order of their ids
pub fn works() -> &'static [Work] {
    static PARSED: OnceLock<Vec<Work>> = OnceLock::new();
    PARSED.get_or_init(|| {
        // The table is logoxide's own, checked by its tests: one that doesn't read is no works
        let table: BTreeMap<String, Work> = toml::from_str(WORKS).unwrap_or_default();
        (table.into_iter())
            .map(|(id, work)| Work { id, ..work })
            .collect()
    })
}

/// The work with this id (`kjv`)
pub fn get(id: &str) -> Option<&'static Work> {
    works().iter().find(|w| w.id == id)
}

/// The work an import name (`ebible:eng-kjv2006`, as `lx import` takes it) is known to be
pub fn of_import(name: &str) -> Option<&'static Work> {
    works().iter().find(|w| w.imports.iter().any(|i| i == name))
}

/// The work an entry's import is known to be, by its id: what an import writes as its `work`
pub fn of_source(source: &crate::catalog::CatalogSource) -> Option<String> {
    of_import(&source.import_name()?).map(|w| w.id.clone())
}

/// A package's work and edition in words (``King James Version (`kjv`), edition 1769-oxford
/// (1769)``), if it says its work
pub fn describe(info: &crate::manifest::Package) -> Option<String> {
    let work = info.work.as_deref()?;
    let name = get(work).map_or(work, |w| w.name.as_str());
    let edition = match (&info.edition, info.edition_year) {
        // An edition named by its year (`1769`) needn't say it twice
        (Some(edition), Some(year)) if edition.contains(&year.to_string()) => {
            format!(", edition {edition}")
        }
        (Some(edition), Some(year)) => format!(", edition {edition} ({year})"),
        (Some(edition), None) => format!(", edition {edition}"),
        (None, Some(year)) => format!(", edition of {year}"),
        (None, None) => String::new(),
    };
    Some(format!("{name} (`{work}`){edition}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_reads_and_holds_together() {
        let table: BTreeMap<String, Work> = toml::from_str(WORKS).unwrap();
        assert_eq!(works().len(), table.len());
        assert!(works().len() >= 7);
        let mut imports = std::collections::BTreeSet::new();
        for work in works() {
            assert!(crate::manifest::is_valid_id(&work.id), "{}", work.id);
            assert!(!work.name.is_empty(), "{}", work.id);
            if let Some(qid) = &work.wikidata {
                let digits = qid.strip_prefix('Q').unwrap_or_default();
                assert!(
                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
                    "{qid}"
                );
            }
            for import in &work.imports {
                let (prefix, id) = import.split_once(':').unwrap();
                assert!(
                    crate::catalog::CatalogSource::PREFIXES.contains(&prefix) && !id.is_empty(),
                    "{import}"
                );
                assert!(imports.insert(import), "{import} is two works");
            }
        }
        assert_eq!(get("kjv").unwrap().name, "King James Version");
        assert_eq!(of_import("sword:crosswire/KJV").unwrap().id, "kjv");
        assert_eq!(of_import("ebible:eng-asv").unwrap().id, "asv");
        assert!(of_import("ebible:nothing").is_none());
    }
}
