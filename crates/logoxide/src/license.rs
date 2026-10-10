/*!
Licenses and what they allow. Licenses are shown, never enforced: this says what a package's
license and rights are, and what a user should know before redistributing it ([`assess`]).

A package's `license` is an SPDX expression (`CC-BY-SA-4.0`, `MIT OR Apache-2.0`,
`LicenseRef-PublicDomain`), read leniently (the `spdx` crate's lax mode: `GPL-2.0+`, `/` for
`OR`, deprecated ids), with `LicenseRef-<name>` for anything SPDX has no id for. Its
`[package.rights]` say what the license allows in four words, each optional:

```toml
[package.rights]
redistributable = true        # may be shared as it is
derivatives = "share-alike"   # "allowed", "share-alike" (under the same terms), or "none"
commercial = false            # may be sold or used commercially
attribution = true            # must name its source
```

For a license logoxide knows ([`derived_rights`]: Creative Commons, the public domain, common
free licenses), they're derived; what's said wins over what's derived, and what neither says is
unknown.
*/

use serde::{Deserialize, Serialize};

use crate::{
    error::{LogoxideError, Result},
    manifest::Package,
};

/// Checks a license: an SPDX expression, read leniently, with `LicenseRef-…` for others
pub fn check_license(expression: &str) -> Result<()> {
    match spdx::Expression::parse_mode(expression, spdx::ParseMode::LAX) {
        Ok(_) => Ok(()),
        Err(e) => Err(LogoxideError::Manifest(format!(
            "license `{expression}` isn't an SPDX expression ({}): `CC-BY-SA-4.0`, \
             `MIT OR Apache-2.0`, or `LicenseRef-<name>` for one SPDX has no id for",
            e.reason
        ))),
    }
}

/// Whether derivatives (translations, revisions, adaptations) may be made
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Derivatives {
    Allowed,
    /// Allowed under the same terms (Creative Commons' ShareAlike, the GPL)
    ShareAlike,
    None,
}

/// What a license allows, each unknown when not said nor derived (`[package.rights]`)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Rights {
    /// May be shared as it is (by anyone, not only the one it was licensed to)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redistributable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivatives: Option<Derivatives>,
    /// May be sold or used commercially
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commercial: Option<bool>,
    /// Sharing it must name its source
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<bool>,
}

impl Rights {
    /// These, with what they don't say taken from `other`
    pub fn or(self, other: Rights) -> Rights {
        Rights {
            redistributable: self.redistributable.or(other.redistributable),
            derivatives: self.derivatives.or(other.derivatives),
            commercial: self.commercial.or(other.commercial),
            attribution: self.attribution.or(other.attribution),
        }
    }

    /// Whether nothing is known
    pub fn is_empty(&self) -> bool {
        *self == Rights::default()
    }

    /// What's known, in words (`redistributable, derivatives under the same terms, no commercial
    /// use, attribution required`); empty when nothing is
    pub fn describe(&self) -> String {
        let mut said = vec![];
        if let Some(redistributable) = self.redistributable {
            said.push(match redistributable {
                true => "redistributable",
                false => "not redistributable",
            });
        }
        if let Some(derivatives) = self.derivatives {
            said.push(match derivatives {
                Derivatives::Allowed => "derivatives allowed",
                Derivatives::ShareAlike => "derivatives under the same terms",
                Derivatives::None => "no derivatives",
            });
        }
        if let Some(commercial) = self.commercial {
            said.push(match commercial {
                true => "commercial use allowed",
                false => "no commercial use",
            });
        }
        if let Some(attribution) = self.attribution {
            said.push(match attribution {
                true => "attribution required",
                false => "no attribution required",
            });
        }
        said.join(", ")
    }

    fn known(derivatives: Derivatives, commercial: bool, attribution: bool) -> Rights {
        Rights {
            redistributable: Some(true),
            derivatives: Some(derivatives),
            commercial: Some(commercial),
            attribution: Some(attribution),
        }
    }
}

/// Licenses that put a text in the public domain, or as near as the law allows
const PUBLIC_DOMAIN: &[&str] = &[
    "LicenseRef-PublicDomain",
    "CC0-1.0",
    "CC-PDDC",
    "PDDL-1.0",
    "Unlicense",
    "0BSD",
    "MIT-0",
    "WTFPL",
];

/// Free licenses that ask only for their notice (derivatives allowed under any terms)
const PERMISSIVE: &[&str] = &[
    "MIT",
    "ISC",
    "Zlib",
    "BSL-1.0",
    "AFL-3.0",
    "Artistic-2.0",
    "ODC-By-1.0",
];

/// Families of permissive licenses, by prefix
const PERMISSIVE_PREFIXES: &[&str] = &["Apache-", "BSD-"];

/// Families of copyleft licenses (derivatives under the same terms), by prefix
const COPYLEFT_PREFIXES: &[&str] = &[
    "GPL-", "LGPL-", "AGPL-", "GFDL-", "MPL-", "EUPL-", "EPL-", "OFL-", "ODbL-",
];

/**
What a license allows, when it's one license logoxide knows: Creative Commons' (`CC-BY-NC-ND-4.0`
is redistributable, with attribution, not commercially, without derivatives), the public domain
(`LicenseRef-PublicDomain`, `CC0-1.0`), and common free ones (MIT, Apache, the GPL). An
expression of several, or a `LicenseRef-` other than the public domain's, derives nothing
*/
pub fn derived_rights(license: Option<&str>) -> Rights {
    let Some(id) = license.map(str::trim) else {
        return Rights::default();
    };
    let id = id.trim_end_matches('+');
    if id.contains(char::is_whitespace) {
        return Rights::default();
    }
    let is = |list: &[&str]| list.iter().any(|l| l.eq_ignore_ascii_case(id));
    let starts = |list: &[&str]| list.iter().any(|p| id.starts_with(p));
    if is(PUBLIC_DOMAIN) {
        return Rights::known(Derivatives::Allowed, true, false);
    }
    if let Some(terms) = id.strip_prefix("CC-BY") {
        let commercial = !terms.contains("-NC");
        let derivatives = match (terms.contains("-ND"), terms.contains("-SA")) {
            (true, _) => Derivatives::None,
            (false, true) => Derivatives::ShareAlike,
            (false, false) => Derivatives::Allowed,
        };
        return Rights::known(derivatives, commercial, true);
    }
    if is(PERMISSIVE) || starts(PERMISSIVE_PREFIXES) {
        return Rights::known(Derivatives::Allowed, true, true);
    }
    if starts(COPYLEFT_PREFIXES) {
        return Rights::known(Derivatives::ShareAlike, true, true);
    }
    Rights::default()
}

impl Package {
    /// What its license allows: `[package.rights]`, with what that doesn't say derived from
    /// `license` ([`derived_rights`])
    pub fn text_rights(&self) -> Rights {
        (self.rights.unwrap_or_default()).or(derived_rights(self.license.as_deref()))
    }
}

/**
Whether a statement says something is in the public domain, and doesn't deny it ("not in the
public domain", "isn't public domain", "no public domain"): at least one mention with no
negation in the few words before it
*/
pub fn says_public_domain(text: &str) -> bool {
    let lower = text.to_lowercase().replace(['-', '_'], " ");
    let negations = ["not", "isn't", "isnt", "no", "never", "non", "nor"];
    let mut rest = lower.as_str();
    let mut before = String::new();
    while let Some(at) = rest.find("public domain") {
        before.push_str(&rest[..at]);
        let words: Vec<&str> = before
            .split(|c: char| !(c.is_alphanumeric() || c == '\''))
            .filter(|w| !w.is_empty())
            .collect();
        let near = &words[words.len().saturating_sub(4)..];
        if !near.iter().any(|w| negations.contains(w)) {
            return true;
        }
        before.push_str("public domain");
        rest = &rest[at + "public domain".len()..];
    }
    false
}

/**
The license a statement gives, as SPDX: a Creative Commons license by its URL
(`creativecommons.org/licenses/by-sa/4.0/` is `CC-BY-SA-4.0`, `…/publicdomain/zero/1.0/` is
`CC0-1.0`), else by its name (`Creative Commons Attribution-Noncommercial-No Derivatives license
4.0`), else the public domain when it says so ([`says_public_domain`]); none otherwise
*/
pub fn license_from_text(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if let Some((_, rest)) = lower.split_once("creativecommons.org/licenses/") {
        let mut parts = rest.splitn(2, '/');
        let terms = parts.next().unwrap_or_default();
        let version = version_at(parts.next().unwrap_or_default());
        if let Some(license) = creative_commons(terms, version.as_deref()) {
            return Some(license);
        }
    }
    if lower.contains("creativecommons.org/publicdomain/zero") {
        return Some(String::from("CC0-1.0"));
    }
    if lower.contains("creativecommons.org/publicdomain/mark") {
        return Some(String::from("LicenseRef-PublicDomain"));
    }
    if let Some(license) = creative_commons_by_name(&lower) {
        return Some(license);
    }
    says_public_domain(text).then(|| String::from("LicenseRef-PublicDomain"))
}

/// A version at the start of `text` (`4.0`), passing over invisible characters (a URL's
/// `4\u{202a}.0`)
fn version_at(text: &str) -> Option<String> {
    let version: String = (text.chars())
        .filter(|c| !matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}'))
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let version = version.trim_end_matches('.');
    (!version.is_empty()).then(|| version.to_string())
}

/// A Creative Commons license from its terms in a URL (`by-nc-nd`) and version (4.0 if none)
fn creative_commons(terms: &str, version: Option<&str>) -> Option<String> {
    let known = ["by", "by-sa", "by-nd", "by-nc", "by-nc-sa", "by-nc-nd"];
    known
        .contains(&terms)
        .then(|| format!("CC-{}-{}", terms.to_uppercase(), version.unwrap_or("4.0")))
}

/// A Creative Commons license from its name in words (`creative commons attribution share-alike
/// license 4.0`), in lowercase text
fn creative_commons_by_name(lower: &str) -> Option<String> {
    let (_, rest) = lower.split_once("creative commons attribution")?;
    // The name runs to its version (`4.0`) or the end of the sentence
    let end = rest.find(['.', '\n', '<']).map_or(rest.len(), |at| {
        // A version's dot isn't the sentence's
        let after = rest[at + 1..].chars().next();
        match after.is_some_and(|c| c.is_ascii_digit()) {
            true => rest[at + 1..]
                .find(|c: char| !c.is_ascii_digit())
                .map_or(rest.len(), |n| at + 1 + n),
            false => at,
        }
    });
    let name = &rest[..end];
    let words = name.replace(['-', '_'], " ");
    let has = |word: &str| words.split_whitespace().any(|w| w == word);
    let mut terms = String::from("by");
    if has("noncommercial") || words.contains("non commercial") {
        terms.push_str("-nc");
    }
    if has("noderivatives") || has("noderivs") || words.contains("no derivative") {
        terms.push_str("-nd");
    } else if has("sharealike") || words.contains("share alike") {
        terms.push_str("-sa");
    }
    let version = (name.split_whitespace()).find_map(|w| version_at(w).filter(|v| v.contains('.')));
    creative_commons(&terms, version.as_deref())
}

/// What may keep a package from being shared, for [`assess`]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConcernKind {
    /// It gives no SPDX license
    NoLicense,
    /// Its license isn't one logoxide knows to be free or open (one allowing commercial use and
    /// derivatives), and its rights don't say it's redistributable
    NotFree,
    /// Its rights say it isn't redistributable: publishing it may break its license
    NotRedistributable,
    /// Its license allows no derivatives, and its import converted the publisher's markup
    Converted,
}

/// Something to know before redistributing a package
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concern {
    pub kind: ConcernKind,
    pub message: String,
}

impl Concern {
    /// Whether it should be confirmed before publishing, not only said (the owner's D8)
    pub fn is_serious(&self) -> bool {
        self.kind == ConcernKind::NotRedistributable
    }
}

/**
What to know before redistributing a package (`lx publish`), from its license and rights: that
they say it isn't redistributable (serious: confirm first); that it has no license, or none
logoxide knows to be free (unless `[package.rights]` says it's redistributable); and that its import
converted the publisher's markup when the license allows no derivatives (logoxide doesn't count
that as a derivative, the owner's D7, but says so). Licenses are shown, never enforced: these
are warnings
*/
pub fn assess(info: &Package) -> Vec<Concern> {
    let check = "licenses are shown, never enforced: check that you may redistribute it";
    let rights = info.text_rights();
    let mut concerns = vec![];
    let concern = |kind, message| Concern { kind, message };
    if rights.redistributable == Some(false) {
        concerns.push(concern(
            ConcernKind::NotRedistributable,
            format!(
                "{}'s rights say it isn't redistributable ({}): publishing it may break its \
                 license; {check}",
                info.id,
                info.license.as_deref().unwrap_or("no license given")
            ),
        ));
    } else if info.rights.and_then(|r| r.redistributable) != Some(true) {
        match info.license.as_deref().filter(|l| !l.trim().is_empty()) {
            None => {
                let file = (info.license_file.as_deref())
                    .map(|f| format!(" (only the file {f})"))
                    .unwrap_or_default();
                concerns.push(concern(
                    ConcernKind::NoLicense,
                    format!("{} gives no SPDX license{file}: {check}", info.id),
                ));
            }
            Some(license) if !is_free(license) => concerns.push(concern(
                ConcernKind::NotFree,
                format!(
                    "{}'s license, {license}, isn't a free or open license logoxide knows: {check}",
                    info.id
                ),
            )),
            Some(_) => {}
        }
    }
    let converted = (info.upstream.as_ref()).is_some_and(|u| u.converted);
    if converted && rights.derivatives == Some(Derivatives::None) {
        concerns.push(concern(
            ConcernKind::Converted,
            format!(
                "{}'s license allows no derivatives, and its import converted the publisher's \
                 markup (its words are unchanged: logoxide doesn't count that as a derivative, \
                 but the publisher might)",
                info.id
            ),
        ));
    }
    concerns
}

/**
Whether an SPDX expression lets anyone redistribute what it covers: an alternative each of whose
`AND`ed licenses is free (in the public domain, permissive, copyleft, or a Creative Commons
license allowing commercial use and derivatives)
*/
pub fn is_free(expression: &str) -> bool {
    expression.split(" OR ").any(|alternative| {
        alternative
            .replace(['(', ')'], " ")
            .split(" AND ")
            // `GPL-2.0 WITH Classpath-exception-2.0`: the license, not its exception
            .filter_map(|id| id.split(" WITH ").next().map(str::trim))
            .all(|id| {
                let rights = derived_rights(Some(id));
                rights.redistributable == Some(true)
                    && rights.commercial == Some(true)
                    && rights.derivatives != Some(Derivatives::None)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn licenses_are_spdx_expressions() {
        for good in [
            "CC0-1.0",
            "CC-BY-SA-4.0",
            "MIT OR Apache-2.0",
            "GPL-2.0+",
            "LicenseRef-PublicDomain",
            "LicenseRef-fetch.bible",
            "GPL-2.0-or-later WITH Classpath-exception-2.0",
        ] {
            assert!(check_license(good).is_ok(), "{good}");
        }
        for bad in ["Public Domain", "", "MIT OR", "Copyrighted; ask"] {
            assert!(check_license(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rights_are_derived_and_said() {
        let nc_nd = derived_rights(Some("CC-BY-NC-ND-4.0"));
        assert_eq!(
            nc_nd,
            Rights {
                redistributable: Some(true),
                derivatives: Some(Derivatives::None),
                commercial: Some(false),
                attribution: Some(true),
            }
        );
        assert_eq!(
            derived_rights(Some("CC-BY-SA-3.0")).derivatives,
            Some(Derivatives::ShareAlike)
        );
        assert_eq!(
            derived_rights(Some("LicenseRef-PublicDomain")).attribution,
            Some(false)
        );
        assert_eq!(
            derived_rights(Some("GPL-2.0-only")).derivatives,
            Some(Derivatives::ShareAlike)
        );
        assert!(derived_rights(Some("LicenseRef-SWORD")).is_empty());
        assert!(derived_rights(Some("MIT OR CC0-1.0")).is_empty());
        // What's said wins
        let said = Rights {
            commercial: Some(true),
            ..Rights::default()
        };
        assert_eq!(said.or(nc_nd).commercial, Some(true));
        assert_eq!(said.or(nc_nd).derivatives, Some(Derivatives::None));
    }

    #[test]
    fn public_domain_isnt_read_into_a_denial() {
        assert!(says_public_domain("public domain"));
        assert!(says_public_domain("This text is in the Public Domain."));
        assert!(says_public_domain("Public-Domain"));
        assert!(!says_public_domain("This is not public domain."));
        assert!(!says_public_domain("It is not in the public domain"));
        assert!(!says_public_domain("This translation isn't public domain"));
        assert!(!says_public_domain("Copyright 2020 Someone"));
        // One mention that isn't denied is enough
        assert!(says_public_domain(
            "The notes are not public domain; the text is public domain."
        ));
    }

    #[test]
    fn licenses_are_read_from_statements() {
        let link = "under the <a href='http://creativecommons.org/licenses/by-nc-nd/4.0/'>Creative Commons Attribution-Noncommercial-No Derivatives license 4.0.</a>";
        assert_eq!(license_from_text(link).as_deref(), Some("CC-BY-NC-ND-4.0"));
        // A stray direction mark in the URL's version
        let marked = "http://creativecommons.org/licenses/by-sa/4\u{202a}.0 or send a letter";
        assert_eq!(license_from_text(marked).as_deref(), Some("CC-BY-SA-4.0"));
        let zero = "https://creativecommons.org/publicdomain/zero/1.0/";
        assert_eq!(license_from_text(zero).as_deref(), Some("CC0-1.0"));
        let named = "made available under the terms of the Creative Commons Attribution Share-Alike license 4.0. You may";
        assert_eq!(license_from_text(named).as_deref(), Some("CC-BY-SA-4.0"));
        assert_eq!(
            license_from_text("public domain").as_deref(),
            Some("LicenseRef-PublicDomain")
        );
        assert_eq!(license_from_text("not public domain"), None);
        assert_eq!(license_from_text("Copyright © 2009 Someone"), None);
    }

    fn package(lines: &str) -> Package {
        crate::Manifest::parse(&format!(
            "[package]\nid = \"a\"\nname = \"A\"\nabbreviation = \"A\"\nversion = \"1.0.0\"\n\
             language = \"en\"\n{lines}\n[formats]\nusx = {{ path = \"usx/\" }}\n"
        ))
        .unwrap()
        .package
        .unwrap()
    }

    #[test]
    fn concerns_before_publishing() {
        let kinds = |lines: &str| {
            (assess(&package(lines)).into_iter())
                .map(|c| c.kind)
                .collect::<Vec<_>>()
        };
        for free in [
            "license = \"CC0-1.0\"",
            "license = \"CC-BY-SA-4.0\"",
            "license = \"MIT OR Apache-2.0\"",
            "license = \"GPL-2.0-or-later WITH Classpath-exception-2.0\"",
            "license = \"(MIT AND CC-BY-4.0)\"",
            "license = \"LicenseRef-PublicDomain\"",
            "license = \"CC-BY-NC-4.0 OR CC0-1.0\"",
            // Said to be redistributable, by a publisher's list
            "license = \"LicenseRef-eBible\"\n[package.rights]\nredistributable = true",
        ] {
            assert_eq!(kinds(free), [], "{free}");
        }
        assert_eq!(kinds(""), [ConcernKind::NoLicense]);
        for restricted in [
            "license = \"LicenseRef-Copyrighted\"",
            "license = \"CC-BY-NC-ND-4.0\"",
            "license = \"CC-BY-ND-4.0\"",
            "license = \"MIT AND LicenseRef-Proprietary\"",
        ] {
            assert_eq!(kinds(restricted), [ConcernKind::NotFree], "{restricted}");
        }
        let sword = "license = \"LicenseRef-SWORD\"\n\
                     upstream = { url = \"u\", converted = true }\n\
                     [package.rights]\nredistributable = false\nderivatives = \"none\"";
        let concerns = assess(&package(sword));
        assert_eq!(
            concerns.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [ConcernKind::NotRedistributable, ConcernKind::Converted]
        );
        assert!(concerns[0].is_serious() && !concerns[1].is_serious());
        assert!(concerns[0].message.contains("never enforced"));
        // Converted, but derivatives allowed: nothing to say
        let allowed = "license = \"CC-BY-4.0\"\nupstream = { url = \"u\", converted = true }";
        assert_eq!(kinds(allowed), []);
    }
}
