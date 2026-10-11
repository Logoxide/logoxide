/*!
Works and editions. A work is a text whatever its editions, formats, and publishers (the King
James Version); a package says which it is with `work = "kjv"`, and which edition with `edition`
(`1769-oxford`) and `edition-year`. logoxide's table of works (`data/works.toml`, the owner's D9)
gives each work's name, aliases, its Wikidata item when known, and the import names known to be
it, so an import is given its work by its name ([`of_import`]), never by guessing from a title.

A catalog may name works of its own (`[works.<id>]` in a `catalog.toml`, or a folder registry's
`works.toml`), in the same form without `imports` ([`parse_declared`]): so a registry's packages
can share a work logoxide's table doesn't have yet. logoxide's table wins: a catalog's work with
one of its ids is passed over. Naming a work owns nothing: any package may say it's any work, and
`lx works` says which works are named where and which claimed ones nothing names
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

/**
A work a catalog names (`[works.<id>]`), and the catalog's name: a registry's works sit beside
logoxide's ([`works`]), never in place of one
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredWork {
    pub work: Work,
    pub catalog: String,
}

/**
A catalog's `[works]` table read: each work checked ([`check`]) and none of logoxide's ids, the
rest passed over with a warning naming `location`. A work's `imports` are passed over too (only
logoxide's table gives imports their work, checked against the publishers' lists)
*/
pub fn parse_declared(
    catalog: &str,
    location: Option<&str>,
    table: BTreeMap<String, Work>,
) -> (Vec<DeclaredWork>, Vec<String>) {
    let at = location.map(|l| format!("{l}: ")).unwrap_or_default();
    let (mut declared, mut warnings) = (vec![], vec![]);
    for (id, work) in table {
        let work = Work { id, ..work };
        if let Err(why) = check(&work) {
            warnings.push(format!("{at}passed over {why}"));
            continue;
        }
        if get(&work.id).is_some() {
            warnings.push(format!(
                "{at}passed over work `{}`: logoxide's table names it",
                work.id
            ));
            continue;
        }
        if !work.imports.is_empty() {
            warnings.push(format!(
                "{at}work `{}`: `imports` passed over (only logoxide's table gives imports a work)",
                work.id
            ));
        }
        declared.push(DeclaredWork {
            work: Work {
                imports: vec![],
                ..work
            },
            catalog: catalog.to_string(),
        });
    }
    (declared, warnings)
}

/// Checks a work: its id is one (as a package's), it has a name, and its Wikidata item, if any,
/// is `Q` and digits
pub fn check(work: &Work) -> Result<(), String> {
    if !crate::manifest::is_valid_id(&work.id) {
        return Err(format!(
            "work `{}`: use lowercase letters, digits, `-`, and `_`",
            work.id
        ));
    }
    if work.name.trim().is_empty() {
        return Err(format!("work `{}`: no name", work.id));
    }
    if let Some(qid) = &work.wikidata {
        let digits = qid.strip_prefix('Q').unwrap_or_default();
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "work `{}`: `wikidata` `{qid}` isn't an item (`Q` and digits)",
                work.id
            ));
        }
    }
    Ok(())
}

/// A folder registry's works, in its root: logoxide's table's form, without `imports`
pub const WORKS_FILE: &str = "works.toml";

/// A `[works]` table's text (`works.toml`, or a catalog's `[works]` alone) read, unchecked
pub fn read_table(text: &str) -> Result<BTreeMap<String, Work>, String> {
    toml::from_str(text).map_err(|e| e.to_string())
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
    of_import(source.import_name()?).map(|w| w.id.clone())
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
            check(work).unwrap();
            for import in &work.imports {
                let (prefix, id) = import.split_once(':').unwrap();
                // The publishers logoxide-import reads (its `PREFIXES`)
                assert!(
                    ["ebible", "fetch", "ccel", "sword"].contains(&prefix) && !id.is_empty(),
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

    #[test]
    fn a_catalogs_works_are_checked() {
        let table: BTreeMap<String, Work> = toml::from_str(
            r#"
            [gill]
            name = "John Gill's Exposition"
            aliases = ["Gill"]
            wikidata = "Q123"

            [kjv]
            name = "Not the KJV"

            [Bad]
            name = "Bad id"

            [unnamed]
            name = " "

            [odd-qid]
            name = "Odd"
            wikidata = "123"

            [claims]
            name = "Claims imports"
            imports = ["ebible:eng-kjv2006"]
            "#,
        )
        .unwrap();
        let (declared, warnings) = parse_declared("mine", Some("catalog.toml"), table);
        let ids: Vec<&str> = declared.iter().map(|d| d.work.id.as_str()).collect();
        assert_eq!(ids, ["claims", "gill"]);
        assert!(
            declared
                .iter()
                .all(|d| d.catalog == "mine" && d.work.imports.is_empty())
        );
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("`kjv`: logoxide's table names it"))
        );
        assert!(warnings.iter().all(|w| w.starts_with("catalog.toml: ")));
        // logoxide's own table is untouched by what a catalog says
        assert_eq!(get("kjv").unwrap().name, "King James Version");
        assert_eq!(of_import("ebible:eng-kjv2006").unwrap().id, "kjv");
    }
}
