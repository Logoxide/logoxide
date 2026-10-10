/*!
Publishers' statements read as licenses: a Creative Commons link or name, or the public domain
([`license_from_text`], from eBible.org's `copr.htm`), and whether a statement says something is
in the public domain without denying it ([`says_public_domain`], for CCEL's `DC.Rights` and
SWORD's `DistributionLicense`). What a license allows ([`logoxide::license::Rights`]) is the
package manager's: this only reads the publishers' words
*/

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
